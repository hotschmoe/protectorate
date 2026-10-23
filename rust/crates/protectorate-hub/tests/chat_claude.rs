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
async fn call(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request=format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
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
fn fake(path: &Path) {
    let script = r#"#!/usr/bin/python3
import json,sys,os
args=sys.argv
assert args[args.index('--permission-mode')+1] == 'manual'
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
assert os.path.exists(os.path.join(os.environ['CLAUDE_CONFIG_DIR'],'.credentials.json'))
assert json.load(open(os.path.join(os.environ['CLAUDE_CONFIG_DIR'],'.credentials.json')))['test'] == 'credential'
def emit(x): print(json.dumps(x),flush=True)
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='user':
  txt=m['message']['content']
  emit({'type':'system','subtype':'init','session_id':sid})
  if txt=='/compact':
   emit({'type':'system','subtype':'compact_boundary','compact_metadata':{'pre_tokens':120,'post_tokens':30,'trigger':'manual'}})
   emit({'type':'result','session_id':sid,'subtype':'success','is_error':False})
   continue
  if txt=='chunks':
   for part in ('po','ng'):
    emit({'type':'assistant','message':{'id':'msg-chunks','model':'claude-test','content':[{'type':'text','text':part},{'type':'tool_use','id':'tool-chunks','name':'Bash','input':{'command':'true'}}]}})
   emit({'type':'result','session_id':sid,'subtype':'success','is_error':False})
   continue
  if txt in ('approve','deny'):
   emit({'type':'control_request','request_id':'perm-1','request':{'subtype':'can_use_tool','tool_name':'Bash','input':{'command':'printf ok'}}})
   response=json.loads(next(sys.stdin))
   assert response['type']=='control_response' and response['response']['request_id']=='perm-1'
   assert response['response']['response']['behavior']==('allow' if txt=='approve' else 'deny')
   if txt=='deny': assert response['response']['response']['message']=='nope'
  emit({'type':'assistant','message':{'id':'msg-'+txt,'model':'claude-test','content':[{'type':'text','text':'answer: '+txt},{'type':'tool_use','id':'tool-1','name':'Bash','input':{'command':'printf ok'}}], 'usage':{'input_tokens':2,'output_tokens':3,'cache_creation_input_tokens':9} if txt=='cold' else {'input_tokens':2,'output_tokens':3,'cache_read_input_tokens':4}}})
  emit({'type':'user','message':{'content':[{'type':'tool_result','tool_use_id':'tool-1','content':'ok','is_error':False}]}})
  emit({'type':'result','session_id':sid,'subtype':'success','is_error':False,'modelUsage':{'claude-test':{'contextWindow':200000}}})
 elif m.get('type')=='control_request' and m['request']['subtype']=='interrupt':
  emit({'type':'result','session_id':sid,'subtype':'interrupted','is_error':False})
"#;
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_stream_json_sandbox_approval_compaction_and_replay() {
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
    fs::write(
        state.join("auth/claude/.credentials.json"),
        r#"{"test":"credential"}"#,
    )
    .unwrap();
    let executable = tools.join("claude");
    fake(&executable);
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: None,
            claude_executable: Some(executable),
            codex_executable: None,
            codex_tool_roots: Vec::new(),
            grok_executable: None,
            grok_tool_roots: Vec::new(),
            tool_roots: Vec::new(),
            claude_tool_roots: vec![tools],
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    let (addr, server) = serve(hub).await;
    let (status,spawned)=call(addr,"SleeveService/SpawnSleeve",json!({"name":"claude-one","harness":"HARNESS_CLAUDE","workspace":work,"brief":"hello","supervised":true})).await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(
        spawned["sleeve"]["chat"]["harnessSessionId"]
            .as_str()
            .unwrap()
            .len(),
        36
    );
    assert_eq!(spawned["sleeve"]["supervised"], true);
    let (status, _) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200);
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"claude-one","text":"approve","requestId":"request-1"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let path = state.join("sleeves/claude-one/chat.jsonl");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
        if events
            .iter()
            .any(|e| e.payload["approval"]["id"] == "perm-1")
        {
            break;
        }
        assert!(Instant::now() < deadline, "missing approval: {events:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, response) = call(
        addr,
        "ChatService/Respond",
        json!({"sleeve":"claude-one","approvalId":"perm-1","allow":true}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, denied_turn) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"claude-one","text":"deny"}),
    )
    .await;
    assert_eq!(status, 200, "{denied_turn}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
        if events
            .iter()
            .filter(|e| e.payload["approval"]["id"] == "perm-1")
            .count()
            >= 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "missing second approval: {events:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, denied) = call(
        addr,
        "ChatService/Respond",
        json!({"sleeve":"claude-one","approvalId":"perm-1","allow":false,"note":"nope"}),
    )
    .await;
    assert_eq!(status, 200, "{denied}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, same) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"claude-one","text":"approve","requestId":"request-1"}),
    )
    .await;
    assert_eq!(status, 200, "{same}");
    assert_eq!(sent["seq"], same["seq"]);
    let (status, compacted) =
        call(addr, "ChatService/Compact", json!({"sleeve":"claude-one"})).await;
    assert_eq!(status, 200, "{compacted}");
    assert_eq!(compacted["compaction"]["tokensBefore"], "120");
    let events = protectorate_chatlog::read_all(&path).unwrap();
    assert!(events
        .iter()
        .any(|e| e.payload["approvalResolved"]["allow"] == true));
    assert!(events
        .iter()
        .any(|e| e.payload["toolResult"]["output"] == "ok"));
    assert_eq!(
        events
            .iter()
            .filter(|e| e.payload["user"]["requestId"] == "request-1")
            .count(),
        1
    );
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"claude-one"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    let (status, resumed) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({
            "name":"claude-one","harness":"HARNESS_CLAUDE","workspace":work,"brief":"cold"
        }),
    )
    .await;
    assert_eq!(status, 200, "{resumed}");
    assert_eq!(
        resumed["sleeve"]["chat"]["harnessSessionId"],
        spawned["sleeve"]["chat"]["harnessSessionId"]
    );
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let deadline = Instant::now() + Duration::from_secs(10);
    let cold = loop {
        let (_, got) = call(
            addr,
            "SleeveService/GetSleeve",
            json!({"name":"claude-one"}),
        )
        .await;
        if got["sleeve"]["chat"]["cache"].get("firstCall").is_some() {
            break got;
        }
        assert!(Instant::now() < deadline, "no first call: {got}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let cache = &cold["sleeve"]["chat"]["cache"];
    assert_eq!(cache["resumed"], true, "{cold}");
    assert_eq!(cache["coldResume"], true, "{cold}");
    assert_eq!(cache["firstCall"]["cacheWriteTokens"], "9", "{cold}");
    assert!(cache["firstCall"].get("readRatio").is_none(), "{cold}");
    assert_eq!(cache["total"]["calls"], "4", "{cold}");
    assert_eq!(cache["total"]["cacheReadTokens"], "12", "{cold}");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"claude-one","text":"chunks"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let events =
        protectorate_chatlog::read_all(state.join("sleeves/claude-one/chat.jsonl")).unwrap();
    assert_eq!(
        events
            .iter()
            .filter_map(|e| e.payload["text"]["delta"].as_str())
            .filter(|part| *part == "po" || *part == "ng")
            .collect::<String>(),
        "pong"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.payload["toolCall"]["id"] == "tool-chunks")
            .count(),
        1
    );
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"claude-one","text":"approve"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"claude-one","until":"WAIT_UNTIL_IDLE","timeout":"2s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let events = protectorate_chatlog::read_all(&path).unwrap();
    let turn = events
        .iter()
        .rev()
        .find_map(|event| event.payload["settled"].get("usage"))
        .expect("settled turn carries usage");
    assert_eq!(turn["cacheReadTokens"], "4", "{turn}");
    assert!((turn["readRatio"].as_f64().unwrap() - 4.0 / 6.0).abs() < 1e-9);
    let (_, warm) = call(
        addr,
        "SleeveService/GetSleeve",
        json!({"name":"claude-one"}),
    )
    .await;
    assert_eq!(
        warm["sleeve"]["chat"]["cache"]["coldResume"], true,
        "{warm}"
    );
    assert!(events.iter().any(|event| {
        event.payload["approvalResolved"]["by"] == "autonomous"
            && event.payload["approvalResolved"]["allow"] == true
    }));
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"claude-one"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}
