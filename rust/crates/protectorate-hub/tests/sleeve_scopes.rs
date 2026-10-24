//! A sleeve Envoy spawns holds only what both the sleeve scopes and Envoy's
//! bounded capability allow, on its first launch, after a resleeve and after
//! the hub restarts and recovers it.
#![cfg(target_os = "linux")]
use crate::support::executable;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";
const ENVOY_SCOPES: [&str; 3] = ["message_*", "read", "sleeve_spawn_sleeve"];
/// The sleeve scopes narrowed to Envoy's: the backlog, directory and lease
/// tools that only read survive through Envoy's "read".
const CHILD_SCOPES: [&str; 6] = [
    "backlog_get_note",
    "backlog_list_notes",
    "directory_*",
    "lease_list_leases",
    "message_*",
    "read",
];

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}

async fn call(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

fn fake_claude(path: &Path) {
    executable::write_executable(
        path,
        r#"#!/usr/bin/python3
import json,sys
args=sys.argv
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
def emit(x): print(json.dumps(x),flush=True)
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='control_request' and m['request']['subtype']=='interrupt':
  emit({'type':'result','session_id':sid,'subtype':'interrupted','is_error':False})
  continue
 if m.get('type')!='user': continue
 emit({'type':'system','subtype':'init','session_id':sid})
 emit({'type':'assistant','message':{'id':'msg','model':'claude-test','content':[{'type':'text','text':'done'}],'usage':{'input_tokens':1,'output_tokens':1}}})
 emit({'type':'result','session_id':sid,'subtype':'success','is_error':False})
"#,
    );
}

/// The scopes of the one live capability of `principal`.
async fn live_scopes(addr: std::net::SocketAddr, principal: &str) -> Vec<String> {
    let (status, listed) = call(addr, "TokenService/ListTokens", OWNER, json!({})).await;
    assert_eq!(status, 200, "{listed}");
    let live = listed["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|token| token["name"] == principal && token.get("revokedAt").is_none())
        .collect::<Vec<_>>();
    assert_eq!(live.len(), 1, "{principal}: {listed}");
    let mut scopes = live[0]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    scopes.sort();
    scopes
}

/// What the child's own token file can do: read, but not write the backlog
/// (an owner-spawned sleeve may) or spawn a sleeve of its own.
async fn assert_child_bounded(addr: std::net::SocketAddr, state: &Path, work: &Path, when: &str) {
    assert_eq!(
        live_scopes(addr, "sleeve:child").await,
        CHILD_SCOPES,
        "{when}"
    );
    let token = fs::read_to_string(state.join("sleeves/child/home/.protectorate-token")).unwrap();
    let token = token.trim();
    let (status, listed) = call(addr, "SleeveService/ListSleeves", token, json!({})).await;
    assert_eq!(status, 200, "{when}: {listed}");
    let (status, denied) = call(
        addr,
        "BacklogService/CreateNote",
        token,
        json!({"title":"outside the bound"}),
    )
    .await;
    assert_eq!(status, 403, "{when}: {denied}");
    let (status, denied) = call(
        addr,
        "SleeveService/SpawnSleeve",
        token,
        json!({"name":"grandchild","harness":"HARNESS_CLAUDE","workspace":work}),
    )
    .await;
    assert_eq!(status, 403, "{when}: {denied}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn envoy_spawned_sleeve_keeps_its_bound_across_resleeve_and_recovery() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let work = root.path().join("work");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [
        &state,
        &owner,
        &work,
        &tools,
        &runtime,
        &state.join("auth/claude"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(state.join("auth/claude/.credentials.json"), "{}").unwrap();
    let claude = tools.join("claude");
    fake_claude(&claude);
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: None,
        claude_executable: Some(claude),
        claude_tool_roots: vec![tools],
        codex_executable: None,
        codex_tool_roots: Vec::new(),
        grok_executable: None,
        grok_tool_roots: Vec::new(),
        tool_roots: Vec::new(),
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        environment: BTreeMap::from([("PROTECTORATE_HUB".into(), "http://127.0.0.1:43210".into())]),
        tasks_max: 64,
    };
    let hub = Hub::open(&state, OWNER, "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher.clone()));
    let (addr, server) = serve(hub).await;
    let (status, envoy) = call(
        addr,
        "SleeveService/SpawnEnvoy",
        OWNER,
        json!({"harness":"HARNESS_CLAUDE","scopes":ENVOY_SCOPES}),
    )
    .await;
    assert_eq!(status, 200, "{envoy}");
    let envoy_token = fs::read_to_string(state.join("workspaces/envoy/.protectorate-token"))
        .unwrap()
        .trim()
        .to_owned();
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        &envoy_token,
        json!({"name":"child","harness":"HARNESS_CLAUDE","workspace":work}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["parent"], "envoy");
    assert_child_bounded(addr, &state, &work, "first launch").await;

    let (status, moved) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        OWNER,
        json!({"name":"child","to":"HARNESS_CLAUDE"}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_child_bounded(addr, &state, &work, "after resleeve").await;
    server.abort();
    let _ = server.await;

    let hub = Hub::open(&state, OWNER, "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    assert!(hub.recover_chat_sleeves().is_empty());
    let (addr, server) = serve(hub).await;
    let (status, recovered) = call(
        addr,
        "SleeveService/GetSleeve",
        OWNER,
        json!({"name":"child"}),
    )
    .await;
    assert_eq!(status, 200, "{recovered}");
    assert_eq!(recovered["sleeve"]["state"], "SLEEVE_STATE_RUNNING");
    assert_eq!(recovered["sleeve"]["parent"], "envoy");
    assert_child_bounded(addr, &state, &work, "after hub restart").await;
    assert_eq!(
        live_scopes(addr, "orchestrator-envoy").await,
        ENVOY_SCOPES,
        "Envoy's own bound after recovery"
    );
    server.abort();
    let _ = server.await;
}
