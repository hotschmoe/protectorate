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

async fn call(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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
import json,sys,time
def emit(obj): print(json.dumps(obj),flush=True)
for line in sys.stdin:
 m=json.loads(line); k=m.get('type')
 if k=='get_state':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'sessionId':'mail-test-session','model':{'id':'test','provider':'local','contextWindow':8192},'thinkingLevel':'medium'}})
 elif k=='get_available_models':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'models':[{'id':'test','provider':'local','name':'Test','reasoning':True}]}})
 elif k=='get_available_thinking_levels':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'levels':['off','low','medium','high']}})
 elif k=='set_model':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'id':m['modelId'],'provider':m['provider']}})
 elif k=='set_thinking_level':
  emit({'type':'response','id':m['id'],'command':k,'success':True})
 elif k in ('prompt','steer'):
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'disposition':'started'}})
  if m['message']=='slow': time.sleep(3.5)
  emit({'type':'message_start','message':{'role':'assistant'}})
  emit({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'done'}})
  emit({'type':'message_end','message':{'role':'assistant','model':'test','provider':'local','usage':{'totalTokens':13,'input':10,'output':3}}})
  emit({'type':'agent_settled','aborted':False})
 elif k=='abort': emit({'type':'agent_settled','aborted':True})
"#).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mail_waits_for_idle_then_injects_once_with_reply_instructions() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let owner = temp.path().join("owner");
    let workspace = temp.path().join("work");
    let tools = temp.path().join("tools");
    let runtime = temp.path().join("runtime");
    for path in [
        &state,
        &owner,
        &workspace,
        &tools,
        &runtime,
        &state.join("auth/pi"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    let pi = tools.join("pi");
    fake_pi(&pi);
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        codex_executable: None,
        grok_executable: None,
        codex_tool_roots: Vec::new(),
        claude_tool_roots: Vec::new(),
        grok_tool_roots: Vec::new(),
        tool_roots: vec![tools],
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        environment: BTreeMap::new(),
        tasks_max: 64,
    };
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (status, body) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({"name":"worker","harness":"HARNESS_PI","workspace":workspace}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"worker","text":"slow"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call(addr, "MessageService/Send", json!({"to":"worker","body":"Please inspect the build","kind":"MESSAGE_KIND_REQUEST","requestId":"mail-0001"})).await;
    assert_eq!(status, 200, "{body}");
    let id = body["message"]["id"].as_str().unwrap().to_owned();
    let (status, peek) = call(
        addr,
        "MessageService/Inbox",
        json!({"as":"worker","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{peek}");
    assert!(peek["messages"][0]["deliveries"][0]["deliveredAt"].is_null());
    tokio::time::sleep(Duration::from_millis(2300)).await;
    let (status, peek) = call(
        addr,
        "MessageService/Inbox",
        json!({"as":"worker","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{peek}");
    assert!(
        peek["messages"][0]["deliveries"][0]["deliveredAt"].is_null(),
        "mail injected during a working turn: {peek}"
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let (status, peek) = call(
            addr,
            "MessageService/Inbox",
            json!({"as":"worker","peek":true}),
        )
        .await;
        assert_eq!(status, 200, "{peek}");
        if peek["messages"][0]["deliveries"][0]["deliveredAt"].is_string() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "mail was never delivered: {peek}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let events = protectorate_chatlog::read_all(state.join("sleeves/worker/chat.jsonl")).unwrap();
    let delivered: Vec<_> = events
        .iter()
        .filter(|e| {
            e.payload["user"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(&format!("request {id}")))
        })
        .collect();
    assert_eq!(delivered.len(), 1, "duplicate chat injection: {events:?}");
    assert_eq!(delivered[0].payload["user"]["from"], "owner");
    let text = delivered[0].payload["user"]["text"].as_str().unwrap();
    assert!(text.contains(&format!("Reply: protectorate msg reply {id}")));
    assert!(text.contains(&format!("Ack: protectorate msg ack {id}")));
    let (status, killed) = call(addr, "SleeveService/KillSleeve", json!({"name":"worker"})).await;
    assert_eq!(status, 200, "{killed}");
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_reaper_waits_for_work_and_pending_mail() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let owner = temp.path().join("owner");
    let workspace = temp.path().join("work");
    let tools = temp.path().join("tools");
    let runtime = temp.path().join("runtime");
    for path in [
        &state,
        &owner,
        &workspace,
        &tools,
        &runtime,
        &state.join("auth/pi"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    let pi = tools.join("pi");
    fake_pi(&pi);
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        codex_executable: None,
        grok_executable: None,
        codex_tool_roots: Vec::new(),
        claude_tool_roots: Vec::new(),
        grok_tool_roots: Vec::new(),
        tool_roots: vec![tools],
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        environment: BTreeMap::new(),
        tasks_max: 64,
    };
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (status, rejected) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({
            "name":"terminal", "harness":"HARNESS_SHELL", "workspace":workspace,
            "exitAfterIdle":"0.200s"
        }),
    )
    .await;
    assert_eq!(status, 400, "{rejected}");
    for (name, brief) in [("busy", "slow"), ("mailbox", "")] {
        let (status, spawned) = call(
            addr,
            "SleeveService/SpawnSleeve",
            json!({
                "name":name, "harness":"HARNESS_PI", "workspace":workspace,
                "brief":brief, "exitAfterIdle":"0.200s"
            }),
        )
        .await;
        assert_eq!(status, 200, "{spawned}");
        assert_eq!(spawned["sleeve"]["exitAfterIdle"], "0.200s");
    }
    let persisted =
        protectorate_sleeve_registry::Registry::open(state.join("sleeves.json")).unwrap();
    assert_eq!(
        persisted.get("busy").unwrap().exit_after_idle.as_deref(),
        Some("0.200s")
    );
    let (status, sent) = call(
        addr,
        "MessageService/Send",
        json!({
            "to":"mailbox", "body":"pending work", "kind":"MESSAGE_KIND_REQUEST"
        }),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    tokio::time::sleep(Duration::from_millis(1200)).await;
    for name in ["busy", "mailbox"] {
        let (status, body) = call(addr, "SleeveService/GetSleeve", json!({"name":name})).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["sleeve"]["state"], "SLEEVE_STATE_RUNNING", "{body}");
    }
    let (status, inbox) = call(
        addr,
        "MessageService/Inbox",
        json!({"as":"mailbox","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{inbox}");
    assert!(
        inbox["messages"][0]["deliveries"][0]["deliveredAt"].is_null(),
        "{inbox}"
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    for name in ["busy", "mailbox"] {
        loop {
            let (status, body) = call(addr, "SleeveService/GetSleeve", json!({"name":name})).await;
            assert_eq!(status, 200, "{body}");
            if body["sleeve"]["state"] == "SLEEVE_STATE_EXITED" {
                assert_eq!(body["sleeve"]["killed"], true, "{body}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "idle sleeve {name} remained running: {body}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    task.abort();
}
