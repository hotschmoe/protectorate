#![cfg(target_os = "linux")]
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

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

fn fake_pi(path: &Path) {
    fs::write(path, r#"#!/usr/bin/python3
import json,sys,os
p=os.environ.get('PROTECTORATE_TOKEN_FILE','')
with open('token-readable','w') as marker:
 marker.write('yes' if p and open(p).read().strip() else 'no')
for line in sys.stdin:
 m=json.loads(line)
 k=m.get('type')
 if k=='get_state':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'sessionId':'envoy-test-session','model':{'id':'test','provider':'local','contextWindow':8192},'thinkingLevel':'medium'}}),flush=True)
 elif k=='get_available_models':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'models':[{'id':'test','provider':'local','name':'Test','reasoning':True}]}}),flush=True)
 elif k=='get_available_thinking_levels':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'levels':['off','low','medium','high']}}),flush=True)
 elif k=='set_model':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'id':m['modelId'],'provider':m['provider']}}),flush=True)
 elif k=='set_thinking_level':
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True}),flush=True)
 elif k in ('prompt','steer'):
  print(json.dumps({'type':'response','id':m['id'],'command':k,'success':True,'data':{'disposition':'started'}}),flush=True)
  if m['message']=='exit-now': sys.exit(0)
  print(json.dumps({'type':'agent_settled','aborted':False}),flush=True)
 elif k=='abort':
  print(json.dumps({'type':'agent_settled','aborted':True}),flush=True)
"#).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_mail_wakes_an_unexpectedly_exited_envoy() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [&state, &owner, &tools, &runtime, &state.join("auth/pi")] {
        fs::create_dir_all(dir).unwrap();
    }
    let pi = tools.join("pi");
    fake_pi(&pi);
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: Some(pi),
            claude_executable: None,
            claude_tool_roots: vec![],
            codex_executable: None,
            codex_tool_roots: vec![],
            grok_executable: None,
            grok_tool_roots: vec![],
            tool_roots: vec![tools],
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::from([(
                "PROTECTORATE_HUB".into(),
                "http://127.0.0.1:43210".into(),
            )]),
            tasks_max: 64,
        }));
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnEnvoy",
        "owner-secret",
        json!({"harness":"HARNESS_PI"}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        "owner-secret",
        json!({"sleeve":"envoy","text":"exit-now"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (status, shown) = call(
            addr,
            "SleeveService/GetSleeve",
            "owner-secret",
            json!({"name":"envoy"}),
        )
        .await;
        assert_eq!(status, 200, "{shown}");
        if shown["sleeve"]["state"] == "SLEEVE_STATE_EXITED" {
            break;
        }
        assert!(Instant::now() < deadline, "Envoy did not exit: {shown}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (status, mailed) = call(
        addr,
        "MessageService/Send",
        "owner-secret",
        json!({"to":"orchestrator","body":"wake the exited Envoy"}),
    )
    .await;
    assert_eq!(status, 200, "{mailed}");
    let id = mailed["message"]["id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let (status, inbox) = call(
            addr,
            "MessageService/Inbox",
            "owner-secret",
            json!({"as":"envoy","peek":true}),
        )
        .await;
        assert_eq!(status, 200, "{inbox}");
        let message = inbox["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap();
        if message["deliveries"][0]["deliveredAt"].is_string() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Envoy mail remained pending: {inbox}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let events = protectorate_chatlog::read_all(state.join("sleeves/envoy/chat.jsonl")).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.payload["user"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("wake the exited Envoy")))
            .count(),
        1
    );
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_spawns_and_respawns_envoy_with_scoped_rotated_token() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [&state, &owner, &tools, &runtime, &state.join("auth/pi")] {
        fs::create_dir_all(dir).unwrap();
    }
    let pi = tools.join("pi");
    fake_pi(&pi);
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: Some(pi),
            claude_executable: None,
            claude_tool_roots: vec![],
            codex_executable: None,
            codex_tool_roots: vec![],
            grok_executable: None,
            grok_tool_roots: vec![],
            tool_roots: vec![tools],
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::from([(
                "PROTECTORATE_HUB".into(),
                "http://127.0.0.1:43210".into(),
            )]),
            tasks_max: 64,
        }));
    let (addr, server) = serve(hub).await;
    let (status, denied) = call(
        addr,
        "SleeveService/SpawnSleeve",
        "owner-secret",
        json!({"name":"envoy","harness":"HARNESS_PI","workspace":state}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    let (status, first) = call(
        addr,
        "SleeveService/SpawnEnvoy",
        "owner-secret",
        json!({"harness":"HARNESS_PI"}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["sleeve"]["name"], "envoy");
    assert_eq!(first["sleeve"]["harness"], "HARNESS_PI");
    assert_eq!(first["token"]["name"], "orchestrator-envoy");
    let workspace = state.join("workspaces/envoy");
    assert_eq!(first["workspace"], json!(workspace));
    assert_eq!(
        fs::read_to_string(workspace.join("token-readable")).unwrap(),
        "yes"
    );
    let token_path = workspace.join(".protectorate-token");
    let old_secret = fs::read_to_string(&token_path).unwrap();
    let old_secret = old_secret.trim();
    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert!(agents.contains("# Envoy"));
    assert!(agents.contains("## How to decide"));
    assert!(agents.contains("protectorate chat wait <name> --until report"));
    assert!(agents.contains(protectorate_mailbox::CONCISE_GUIDANCE));
    assert!(agents.contains("## CLI reference"));
    assert!(agents.contains("sleeve spawn <name> --workspace <path> --brief <file>"));
    assert!(agents.contains("report --status complete|partial|blocked --summary-file"));
    assert!(agents.contains("| `hub_get_status` | read-only | yes |"));
    assert!(agents.contains("| `sleeve_kill_sleeve` | destructive | no |"));
    assert_eq!(
        fs::read_link(workspace.join("CLAUDE.md")).unwrap(),
        Path::new("AGENTS.md")
    );
    let (status, listed) = call(addr, "SleeveService/ListSleeves", old_secret, json!({})).await;
    assert_eq!(status, 200, "{listed}");
    let (status, forbidden) = call(
        addr,
        "SleeveService/SpawnEnvoy",
        old_secret,
        json!({"harness":"HARNESS_PI"}),
    )
    .await;
    assert_eq!(status, 403, "{forbidden}");
    let (status, forbidden) = call(
        addr,
        "SleeveService/PruneSleeves",
        old_secret,
        json!({"yes":true,"homes":true}),
    )
    .await;
    assert_eq!(status, 403, "{forbidden}");
    let (status, second) = call(
        addr,
        "SleeveService/SpawnEnvoy",
        "owner-secret",
        json!({"harness":"HARNESS_PI"}),
    )
    .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["replaced"], true);
    assert_ne!(second["token"]["id"], first["token"]["id"]);
    assert_eq!(
        fs::read_to_string(workspace.join("AGENTS.md")).unwrap(),
        agents
    );
    let new_secret = fs::read_to_string(&token_path).unwrap();
    let (status, old) = call(addr, "SleeveService/ListSleeves", old_secret, json!({})).await;
    assert_eq!(status, 401, "{old}");
    let (status, current) = call(
        addr,
        "SleeveService/ListSleeves",
        new_secret.trim(),
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{current}");
    let (status, reported) = call(
        addr,
        "ChatService/Report",
        new_secret.trim(),
        json!({"status":"REPORT_STATUS_COMPLETE","summary":"envoy completed its brief"}),
    )
    .await;
    assert_eq!(status, 200, "{reported}");
    assert_eq!(reported["result"]["status"], "REPORT_STATUS_COMPLETE");
    let (status, forbidden) = call(
        addr,
        "ChatService/Report",
        new_secret.trim(),
        json!({"sleeve":"another","status":"REPORT_STATUS_COMPLETE"}),
    )
    .await;
    assert_eq!(status, 403, "{forbidden}");
    let (status, forbidden) = call(
        addr,
        "ChatService/Report",
        "owner-secret",
        json!({"sleeve":"envoy","status":"REPORT_STATUS_COMPLETE"}),
    )
    .await;
    assert_eq!(status, 403, "{forbidden}");
    let (status, audit) = call(
        addr,
        "AuditService/ListAuditEntries",
        "owner-secret",
        json!({"procedure":"ChatService/Report"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{audit}");
    assert_eq!(entries[0]["subject"].as_str().unwrap_or_default(), "");
    assert_eq!(entries[1]["subject"].as_str().unwrap_or_default(), "");
    assert_eq!(entries[2]["subject"], "envoy");
    assert_eq!(entries[2]["outcome"], "ok");
    let (status, audit) = call(
        addr,
        "AuditService/ListAuditEntries",
        "owner-secret",
        json!({"procedure":"SleeveService/SpawnEnvoy"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{audit}");
    assert_eq!(entries[0]["subject"], "envoy");
    assert_eq!(entries[0]["outcome"], "ok");
    assert_eq!(entries[0]["detail"], "harness=pi replaced=true");
    assert_eq!(entries[1]["outcome"], "denied");
    assert_eq!(entries[1]["actor"], "token:orchestrator-envoy");
    assert_eq!(entries[2]["subject"], "envoy");
    assert_eq!(entries[2]["outcome"], "ok");
    assert_eq!(entries[2]["detail"], "harness=pi replaced=false");
    assert!(!audit.to_string().contains(old_secret));
    assert!(!audit.to_string().contains(new_secret.trim()));
    server.abort();
}
