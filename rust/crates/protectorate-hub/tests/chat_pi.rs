#![cfg(target_os = "linux")]
use crate::support::executable;
use protectorate_connect_client::Client;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::mpsc,
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
    call_as(addr, method, "owner-secret", body).await
}
async fn call_as(
    addr: std::net::SocketAddr,
    method: &str,
    token: &str,
    body: Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request=format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
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
async fn read_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            return String::from_utf8(line).unwrap();
        }
    }
}
async fn read_event(stream: &mut TcpStream) -> Value {
    let line = read_line(stream).await;
    let size = usize::from_str_radix(line.trim(), 16).unwrap();
    let mut frame = vec![0; size];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(read_line(stream).await, "\r\n");
    assert_eq!(frame[0], 0);
    serde_json::from_slice(&frame[5..]).unwrap()
}
async fn subscribe(addr: std::net::SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let frame = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!("POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n", frame.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&frame).await.unwrap();
    assert!(read_line(&mut stream).await.contains("200"));
    while read_line(&mut stream).await != "\r\n" {}
    assert_eq!(read_event(&mut stream).await["kind"], "heartbeat");
    stream
}
fn make_fake(path: &Path) {
    let script = r#"#!/usr/bin/python3
import json,os,sys,time
def emit(obj):
 print(json.dumps(obj),flush=True)
compacts=0
for line in sys.stdin:
 m=json.loads(line)
 k=m.get('type')
 if k=='get_state':
  with open('cache-env-proof','w') as f:
   f.write('|'.join(os.environ.get(k,'') for k in ('PUB_CACHE','GOCACHE','GOMODCACHE','CARGO_HOME')))
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'sessionId':'pi-test-session','model':{'id':'test-model','provider':'test-local','baseUrl':'http://127.0.0.1:1234/v1','contextWindow':8192},'thinkingLevel':'medium'}})
 elif k=='get_available_models':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'models':[{'id':'test-model','provider':'test-local','name':'Test Model','reasoning':True},{'id':'test-alt','provider':'test-local','name':'Alternate','reasoning':True},{'id':'test-tail','provider':'test-local','name':'Tailnet','reasoning':True},{'id':'test-named','provider':'test-local','name':'Named','reasoning':True}]}})
 elif k=='get_available_thinking_levels':
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'levels':['off','low','medium','high']}})
 elif k=='set_model':
  base={'test-alt':'https://api.example.com/v1','test-tail':'http://gpu.example.ts.net:11434/v1','test-named':'https://LLM.example.com./v1'}.get(m['modelId'],'http://127.0.0.1:1234/v1')
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'id':m['modelId'],'provider':m['provider'],'baseUrl':base}})
 elif k=='set_thinking_level':
  emit({'type':'response','id':m['id'],'command':k,'success':True})
 elif k=='compact':
  compacts+=1
  if compacts==1:
   emit({'type':'compaction_end','result':{'tokensBefore':150,'estimatedTokensAfter':30}})
   emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'tokensBefore':150,'estimatedTokensAfter':30}})
  else:
   emit({'type':'compaction_end','result':None})
   emit({'type':'response','id':m['id'],'command':k,'success':False,'error':'Nothing to compact (session too small)'})
 elif k in ('prompt','steer'):
  emit({'type':'response','id':m['id'],'command':k,'success':True,'data':{'disposition':'started'}})
  if m['message']=='slow': time.sleep(0.3)
  emit({'type':'message_start','message':{'role':'assistant'}})
  emit({'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'answer: '+m['message']}})
  emit({'type':'message_end','message':{'role':'assistant','model':'test-model','provider':'test-local','usage':{'totalTokens':13,'input':10,'output':3}}})
  emit({'type':'agent_settled','aborted':False})
 elif k=='abort':
  emit({'type':'agent_settled','aborted':True})
"#;
    executable::write_executable(path, script);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_requested_selection_survives_hub_restart() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let workspace = root.path().join("work");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [
        &state,
        &owner,
        &workspace,
        &tools,
        &runtime,
        &state.join("auth/pi"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    let pi = tools.join("pi");
    make_fake(&pi);
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        codex_executable: None,
        codex_tool_roots: Vec::new(),
        grok_executable: None,
        grok_tool_roots: Vec::new(),
        claude_tool_roots: Vec::new(),
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
        .with_chat_runtime(ChatRuntime::new(launcher.clone()));
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({
            "name":"pi-recover", "harness":"HARNESS_PI", "workspace":workspace,
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let (status, selected) = call(
        addr,
        "ChatService/SetSelection",
        json!({
            "sleeve":"pi-recover", "selection":{"model":"test-local/test-alt","effort":"low"}
        }),
    )
    .await;
    assert_eq!(status, 200, "{selected}");
    let (status, sent_before_restart) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-recover","text":"remember this send","requestId":"restart-key-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{sent_before_restart}");
    server.abort();
    let _ = server.await;

    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(launcher));
    assert!(hub.recover_chat_sleeves().is_empty());
    let (addr, server) = serve(hub).await;
    let (status, recovered) = call(
        addr,
        "SleeveService/GetSleeve",
        json!({"name":"pi-recover"}),
    )
    .await;
    assert_eq!(status, 200, "{recovered}");
    let (status, replayed_after_restart) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-recover","text":"remember this send","requestId":"restart-key-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{replayed_after_restart}");
    assert_eq!(replayed_after_restart["seq"], sent_before_restart["seq"]);
    assert_eq!(
        recovered["sleeve"]["chat"]["settings"]["requested"]["model"],
        "test-local/test-alt"
    );
    assert_eq!(
        recovered["sleeve"]["chat"]["settings"]["requested"]["effort"],
        "low"
    );
    assert_eq!(
        recovered["sleeve"]["chat"]["settings"]["effective"]["model"],
        "test-local/test-alt"
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_chat_recovery_is_visible_in_audit_and_sleeve_state() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let workspace = root.path().join("work");
    fs::create_dir_all(&workspace).unwrap();
    let hub = Hub::open(&state, "owner-secret", "test").unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "unavailable",
                Harness::Pi,
                Mode::Chat,
                &workspace,
                "2026-10-23T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    drop(hub);

    let hub = Hub::open(&state, "owner-secret", "test").unwrap();
    let failures = hub.recover_chat_sleeves();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "unavailable");
    let (addr, server) = serve(hub).await;
    let (status, row) = call(
        addr,
        "SleeveService/GetSleeve",
        json!({"name":"unavailable"}),
    )
    .await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["sleeve"]["state"], "SLEEVE_STATE_EXITED");
    let (status, audit) = call(
        addr,
        "AuditService/ListAuditEntries",
        json!({"procedure":"chat_recovery", "subject":"unavailable", "outcome":"error"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    assert_eq!(audit["entries"].as_array().unwrap().len(), 1);
    assert!(audit["entries"][0]["detail"]
        .as_str()
        .unwrap()
        .contains("not configured"));
    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_chat_sleeve_sandbox_send_replay_and_kill() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let workspace = root.path().join("work");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [
        &state,
        &owner,
        &workspace,
        &tools,
        &runtime,
        &state.join("auth/pi"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    let pi = tools.join("pi");
    make_fake(&pi);
    let launcher = ChatLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime,
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        pi_executable: Some(pi),
        claude_executable: None,
        codex_executable: None,
        codex_tool_roots: Vec::new(),
        grok_executable: None,
        grok_tool_roots: Vec::new(),
        claude_tool_roots: Vec::new(),
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
        .with_local_model_hosts(vec!["llm.example.com".into()])
        .with_chat_runtime(ChatRuntime::new(launcher));
    let (addr, server) = serve(hub).await;
    let mut event_stream = subscribe(addr).await;
    let (status, rejected) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({
            "name":"pi-extra", "harness":"HARNESS_PI", "workspace":workspace,
            "args":["--model", "test"]
        }),
    )
    .await;
    assert_eq!(status, 400, "{rejected}");
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({
            "name":"pi-one","harness":"HARNESS_PI","workspace":workspace,
            "brief":"first prompt", "supervised":true
        }),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["supervised"], true);
    assert_eq!(
        spawned["sleeve"]["chat"]["harnessSessionId"],
        "pi-test-session"
    );
    assert_eq!(spawned["sleeve"]["chat"]["localModel"], true);
    assert!(!spawned.to_string().contains("127.0.0.1:1234"));
    let mut states = Vec::new();
    while states.last().is_none_or(|state| state != "CHAT_STATE_IDLE")
        || !states.iter().any(|state| state == "CHAT_STATE_WORKING")
    {
        let event = tokio::time::timeout(Duration::from_secs(3), read_event(&mut event_stream))
            .await
            .unwrap();
        if event["kind"] == "chat.state" && event["subject"] == "pi-one" {
            assert_eq!(event["payload"]["local_model"], true);
            assert_eq!(event["payload"]["provider"], "test-local");
            assert!(event["payload"]["context_window"].is_number());
            assert!(event["payload"].get("baseUrl").is_none());
            states.push(event["payload"]["state"].as_str().unwrap().to_owned());
        }
    }
    assert!(
        states.len() <= 4,
        "unexpected repeated state events: {states:?}"
    );
    for _ in 0..3 {
        let (code, _) = call(addr, "SleeveService/ListSleeves", json!({})).await;
        assert_eq!(code, 200);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(200), read_event(&mut event_stream))
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(workspace.join("cache-env-proof")).unwrap(),
        format!(
            "{}|{}|{}|{}",
            root.path().join("owner/.cache/pub").display(),
            root.path().join("owner/.cache/go-build").display(),
            root.path().join("owner/.cache/go-mod").display(),
            root.path().join("owner/.cargo").display(),
        )
    );
    let (empty_status, empty) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":" \t\n","requestId":"empty-id"}),
    )
    .await;
    assert_eq!(empty_status, 400, "{empty}");
    assert_eq!(empty["code"], "invalid_argument");
    let (invalid_id_status, invalid_id) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"do not send","requestId":"short"}),
    )
    .await;
    assert_eq!(invalid_id_status, 400, "{invalid_id}");
    assert_eq!(invalid_id["code"], "invalid_argument");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"second prompt","requestId":"chat-id-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let (same_status, same) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"second prompt","requestId":"chat-id-0001"}),
    )
    .await;
    assert_eq!(same_status, 200, "{same}");
    assert_eq!(sent["seq"], same["seq"]);
    let path = state.join("sleeves/pi-one/chat.jsonl");
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
        if events
            .iter()
            .any(|e| e.payload["text"]["delta"] == "answer: second prompt")
            && events
                .iter()
                .filter(|e| e.payload["settled"].is_object())
                .count()
                >= 2
        {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.payload["user"]["text"] == "second prompt")
                    .count(),
                1
            );
            break;
        }
        assert!(Instant::now() < deadline, "Pi output missing: {events:?}");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let (status, catalog) = call(addr, "ChatService/ListModels", json!({"sleeve":"pi-one"})).await;
    assert_eq!(status, 200, "{catalog}");
    assert_eq!(catalog["supported"], true);
    assert_eq!(catalog["models"][1]["model"], "test-local/test-alt");
    let (status, unsupported) = call(addr, "ChatService/ListModels", json!({"harness":"pi"})).await;
    assert_eq!(status, 200, "{unsupported}");
    assert!(!unsupported["supported"].as_bool().unwrap_or(false));
    let (status, invalid) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"pi-one","selection":{"model":"wrong/model","effort":"low"}}),
    )
    .await;
    assert_eq!(status, 400, "{invalid}");
    let (status, selected) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"pi-one","selection":{"model":"test-local/test-alt","effort":"low"}}),
    )
    .await;
    assert_eq!(status, 200, "{selected}");
    assert_eq!(
        selected["chat"]["settings"]["effective"]["model"],
        "test-local/test-alt"
    );
    assert!(!selected["chat"]["localModel"].as_bool().unwrap_or(false));
    loop {
        let event = tokio::time::timeout(Duration::from_secs(3), read_event(&mut event_stream))
            .await
            .unwrap();
        if event["kind"] == "chat.state" && event["payload"]["model"] == "test-alt" {
            assert_eq!(event["payload"]["local_model"], false);
            break;
        }
    }
    // Only configured names and private addresses are local, never a
    // tailnet name.
    for (model, local) in [("test-tail", false), ("test-named", true)] {
        let (status, selected) = call(
            addr,
            "ChatService/SetSelection",
            json!({"sleeve":"pi-one","selection":{"model":format!("test-local/{model}"),"effort":"low"}}),
        )
        .await;
        assert_eq!(status, 200, "{selected}");
        assert_eq!(
            selected["chat"]["localModel"].as_bool().unwrap_or(false),
            local,
            "{model}: {selected}"
        );
    }
    let (status, compacted) = call(addr, "ChatService/Compact", json!({"sleeve":"pi-one"})).await;
    assert_eq!(status, 200, "{compacted}");
    assert_eq!(compacted["compaction"]["tokensBefore"], "150");
    assert!(!compacted["compaction"]["native"].as_bool().unwrap_or(false));
    let (status, small) = call(addr, "ChatService/Compact", json!({"sleeve":"pi-one"})).await;
    assert_eq!(status, 400, "{small}");
    assert_eq!(small["code"], "failed_precondition");
    let (status, restored) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"pi-one","selection":{}}),
    )
    .await;
    assert_eq!(status, 200, "{restored}");
    assert_eq!(
        restored["chat"]["settings"]["effective"]["model"],
        "test-local/test-model"
    );
    assert_eq!(restored["chat"]["localModel"], true);
    let (status, responded) = call(
        addr,
        "ChatService/Respond",
        json!({"sleeve":"pi-one","approvalId":"a1","allow":true}),
    )
    .await;
    assert_eq!(status, 501, "{responded}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_IDLE","timeout":"1s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, early) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_REPORT","timeout":"0.05s"}),
    )
    .await;
    assert_eq!(status, 504, "{early}");
    let token_path = state.join("sleeves/pi-one/home/.protectorate-token");
    let sleeve_token = fs::read_to_string(&token_path).unwrap();
    let sleeve_token = sleeve_token.trim();
    let watch_token = sleeve_token.to_owned();
    let denied_watch = tokio::task::spawn_blocking(move || {
        Client::new(
            &format!("http://{addr}"),
            &watch_token,
            Duration::from_secs(5),
        )
        .unwrap()
        .stream(
            "/protectorate.v1.ChatService/Watch",
            &json!({"sleeve":"pi-other","afterSeq":"0"}),
            |_| Ok(()),
        )
        .unwrap_err()
    })
    .await
    .unwrap();
    assert_eq!(denied_watch.code, protectorate_wire::Code::PermissionDenied);
    let (status, denied) = call(
        addr,
        "ChatService/Report",
        json!({"sleeve":"pi-one","status":"REPORT_STATUS_COMPLETE","summary":"done"}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    let (status, reported) = call_as(
        addr,
        "ChatService/Report",
        sleeve_token,
        json!({"status":"REPORT_STATUS_COMPLETE","summary":"done"}),
    )
    .await;
    assert_eq!(status, 200, "{reported}");
    assert_eq!(reported["result"]["summary"], "done");
    assert_eq!(reported["result"]["briefSeq"], sent["seq"]);
    let (status, finished) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_REPORT","timeout":"1s"}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["result"]["summary"], "done");
    let (status, third) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"third prompt","requestId":"chat-id-0002"}),
    )
    .await;
    assert_eq!(status, 200, "{third}");
    let (status, stale) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_REPORT","timeout":"0.05s"}),
    )
    .await;
    assert_eq!(status, 504, "{stale}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_IDLE","timeout":"1s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, reported) = call_as(
        addr,
        "ChatService/Report",
        sleeve_token,
        json!({"status":"REPORT_STATUS_COMPLETE","summary":"third done"}),
    )
    .await;
    assert_eq!(status, 200, "{reported}");
    assert_eq!(reported["result"]["briefSeq"], third["seq"]);
    let (status, old_retry) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"second prompt","requestId":"chat-id-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{old_retry}");
    assert_eq!(old_retry["seq"], sent["seq"]);
    let (status, current) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_REPORT","timeout":"1s"}),
    )
    .await;
    assert_eq!(status, 200, "{current}");
    assert_eq!(current["result"]["summary"], "third done");
    let (status, slow) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"pi-one","text":"slow","requestId":"chat-id-0003"}),
    )
    .await;
    assert_eq!(status, 200, "{slow}");
    let (status, busy) = call(addr, "ChatService/Compact", json!({"sleeve":"pi-one"})).await;
    assert_eq!(status, 400, "{busy}");
    assert_eq!(busy["code"], "failed_precondition");
    let (status, busy) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"pi-one","selection":{"model":"test-local/test-alt","effort":"low"}}),
    )
    .await;
    assert_eq!(status, 400, "{busy}");
    assert_eq!(busy["code"], "failed_precondition");
    let (status, waiting) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_IDLE","timeout":"0.05s"}),
    )
    .await;
    assert_eq!(status, 504, "{waiting}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"pi-one","until":"WAIT_UNTIL_IDLE","timeout":"1s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (replayed_tx, replayed_rx) = mpsc::channel();
    let watch = tokio::task::spawn_blocking(move || {
        let client = Client::new(
            &format!("http://{addr}"),
            "owner-secret",
            Duration::from_secs(5),
        )
        .unwrap();
        let mut found = false;
        let result = client.stream(
            "/protectorate.v1.ChatService/Watch",
            &json!({"sleeve":"pi-one","afterSeq":"0"}),
            |event| {
                if event["text"]["delta"] == "answer: second prompt" && !found {
                    found = true;
                    let _ = replayed_tx.send(());
                }
                Ok(())
            },
        );
        assert!(result.is_ok(), "Watch failed: {result:?}");
        assert!(found, "Watch did not replay Pi output: {result:?}");
    });
    tokio::task::spawn_blocking(move || replayed_rx.recv_timeout(Duration::from_secs(5)))
        .await
        .unwrap()
        .expect("Watch did not start replaying Pi output before Kill");
    let (status, _) = call(addr, "ChatService/Interrupt", json!({"sleeve":"pi-one"})).await;
    assert_eq!(status, 200);
    let (status, _) = call(addr, "SleeveService/KillSleeve", json!({"name":"pi-one"})).await;
    assert_eq!(status, 200);
    assert!(!token_path.exists());
    let (status, revoked) = call_as(
        addr,
        "ChatService/Report",
        sleeve_token,
        json!({"status":"REPORT_STATUS_COMPLETE","summary":"late"}),
    )
    .await;
    assert_eq!(status, 401, "{revoked}");
    watch.await.unwrap();
    let replay = tokio::task::spawn_blocking(move || {
        let client = Client::new(
            &format!("http://{addr}"),
            "owner-secret",
            Duration::from_secs(5),
        )
        .unwrap();
        let mut texts = Vec::new();
        client
            .stream(
                "/protectorate.v1.ChatService/Watch",
                &json!({"sleeve":"pi-one","afterSeq":"0"}),
                |event| {
                    if let Some(text) = event["text"]["delta"].as_str() {
                        texts.push(text.to_owned());
                    }
                    Ok(())
                },
            )
            .unwrap();
        texts
    })
    .await
    .unwrap();
    assert!(replay.iter().any(|text| text == "answer: second prompt"));
    server.abort();
    let _ = server.await;
}
