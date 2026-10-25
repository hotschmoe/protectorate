#![cfg(target_os = "linux")]
use crate::support::executable;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
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
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
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
import json,os,sys
assert sys.argv[1:]==['agent','stdio']
home=os.environ['GROK_HOME']
assert json.load(open(os.path.join(home,'auth.json')))['test']=='credential'
assert os.path.islink(os.path.join(home,'auth.json.lock'))
sid='grok-test'; gated=False
def emit(x): print(json.dumps(x),flush=True)
def recv():
 line=sys.stdin.readline()
 if not line: raise SystemExit(0)
 return json.loads(line)
def note(update): emit({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':update}})
while True:
 m=recv(); mid=m.get('id'); method=m.get('method'); p=m.get('params',{})
 if method=='initialize':
  assert p['protocolVersion']==1
  emit({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{'loadSession':True}}})
 elif method=='session/new':
  assert p['cwd'] and p['mcpServers']==[]
  emit({'jsonrpc':'2.0','id':mid,'result':{'sessionId':sid}})
 elif method=='session/load':
  assert p['sessionId']==sid and p['cwd'] and p['mcpServers']==[]
  emit({'jsonrpc':'2.0','id':mid,'result':{}})
 elif method=='session/prompt':
  text=p['prompt'][0]['text']; assert p['sessionId']==sid
  if not gated:
   assert text=='/always-approve off'; gated=True
   emit({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}}); continue
  if text in ('approve','deny'):
   emit({'jsonrpc':'2.0','id':100,'method':'session/request_permission','params':{'sessionId':sid,'toolCall':{'toolCallId':'tc1','kind':'edit','title':'Write file'},'options':[{'optionId':'allow-once','kind':'allow_once'},{'optionId':'deny-once','kind':'reject_once'}]}})
   answer=recv(); assert answer['id']==100
   option=answer['result']['outcome']['optionId']
   assert option==('allow-once' if text=='approve' else 'deny-once')
  note({'sessionUpdate':'tool_call','toolCallId':'tc1','title':'Write file','rawInput':{'path':'hello.txt'}})
  note({'sessionUpdate':'tool_call_update','toolCallId':'tc1','status':'completed','content':[{'type':'content','content':{'type':'text','text':'ok'}}]})
  note({'sessionUpdate':'agent_message_chunk','messageId':'m1','content':{'type':'text','text':'answer: '+text}})
  emit({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn','_meta':{'usage':{'inputTokens':7,'outputTokens':2,'cachedReadTokens':1}}}})
"#;
    executable::write_executable(path, script);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grok_acp_sandbox_gate_approvals_events_and_resume() {
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
        &state.join("auth/grok"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        state.join("auth/grok/auth.json"),
        r#"{"test":"credential"}"#,
    )
    .unwrap();
    let executable = tools.join("grok");
    fake(&executable);
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: None,
            claude_executable: None,
            claude_tool_roots: Vec::new(),
            codex_executable: None,
            codex_tool_roots: Vec::new(),
            grok_executable: Some(executable),
            grok_tool_roots: vec![tools],
            tool_roots: Vec::new(),
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    let (addr, server) = serve(hub).await;
    let (status,spawned)=call(addr,"SleeveService/SpawnSleeve",json!({"name":"grok-one","harness":"HARNESS_GROK","workspace":work,"brief":"hello","supervised":true})).await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["chat"]["harnessSessionId"], "grok-test");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"grok-one","until":"WAIT_UNTIL_IDLE","timeout":"3s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, models) = call(addr, "ChatService/ListModels", json!({"sleeve":"grok-one"})).await;
    assert_eq!(status, 200, "{models}");
    assert!(
        models["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("Grok ACP"),
        "{models}"
    );
    let (status, compact) = call(addr, "ChatService/Compact", json!({"sleeve":"grok-one"})).await;
    assert_eq!(status, 501, "{compact}");
    for (turn, allow) in [("approve", true), ("deny", false)] {
        let (status, sent) = call(
            addr,
            "ChatService/Send",
            json!({"sleeve":"grok-one","text":turn,"requestId":if allow {"grok-approve"} else {"grok-deny"}}),
        )
        .await;
        assert_eq!(status, 200, "{sent}");
        let path = state.join("sleeves/grok-one/chat.jsonl");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
            if events
                .iter()
                .filter(|e| e.payload["approval"]["id"] == "100")
                .count()
                >= if allow { 1 } else { 2 }
            {
                break;
            }
            assert!(Instant::now() < deadline, "missing approval: {events:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let (status, response) = call(
            addr,
            "ChatService/Respond",
            json!({"sleeve":"grok-one","approvalId":"100","allow":allow}),
        )
        .await;
        assert_eq!(status, 200, "{response}");
        let (status, idle) = call(
            addr,
            "ChatService/Wait",
            json!({"sleeve":"grok-one","until":"WAIT_UNTIL_IDLE","timeout":"3s"}),
        )
        .await;
        assert_eq!(status, 200, "{idle}");
    }
    let events = protectorate_chatlog::read_all(state.join("sleeves/grok-one/chat.jsonl")).unwrap();
    crate::support::chatlog::assert_decodable(&events);
    assert!(events
        .iter()
        .any(|e| e.payload["toolResult"]["output"] == "ok"));
    assert!(events
        .iter()
        .any(|e| e.payload["usage"]["cacheReadTokens"] == "1"));
    let (status, killed) = call(addr, "SleeveService/KillSleeve", json!({"name":"grok-one"})).await;
    assert_eq!(status, 200, "{killed}");
    let (status,resumed)=call(addr,"SleeveService/SpawnSleeve",json!({"name":"grok-one","harness":"HARNESS_GROK","workspace":work,"brief":"resumed","supervised":true})).await;
    assert_eq!(status, 200, "{resumed}");
    assert_eq!(resumed["sleeve"]["chat"]["harnessSessionId"], "grok-test");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"grok-one","until":"WAIT_UNTIL_IDLE","timeout":"3s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, killed) = call(addr, "SleeveService/KillSleeve", json!({"name":"grok-one"})).await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}

/// A Grok that answers a prompt only after flooding stdout with far more than
/// a pipe holds, before it has read the whole prompt line.
fn flooding_fake(path: &Path) {
    let script = r#"#!/usr/bin/python3
import json,os,sys,threading
sid='grok-flood'; gated=False
buf=b''; flooded=False
def emit(x): sys.stdout.write(json.dumps(x)+'\n'); sys.stdout.flush()
def recv():
 global buf,flooded
 while b'\n' not in buf:
  chunk=os.read(0,4096)
  if not chunk: raise SystemExit(0)
  buf+=chunk
  if not flooded and b'\n' not in buf and len(buf)>=4096:
   flooded=True; threading.Timer(20,os._exit,[3]).start()
   for _ in range(256): emit({'jsonrpc':'2.0','method':'noise','params':{'pad':'x'*1024}})
 line,_,buf=buf.partition(b'\n')
 return json.loads(line)
while True:
 m=recv(); mid=m.get('id'); method=m.get('method'); p=m.get('params',{})
 if method=='initialize':
  emit({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{'loadSession':True}}})
 elif method=='session/new':
  emit({'jsonrpc':'2.0','id':mid,'result':{'sessionId':sid}})
 elif method=='session/prompt':
  text=p['prompt'][0]['text']
  if gated:
   emit({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','messageId':'m1','content':{'type':'text','text':'answer: '+str(len(text))}}}})
  gated=True
  emit({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}})
"#;
    executable::write_executable(path, script);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grok_prompt_larger_than_a_pipe_while_grok_floods_stdout() {
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
        &state.join("auth/grok"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        state.join("auth/grok/auth.json"),
        r#"{"test":"credential"}"#,
    )
    .unwrap();
    let executable = tools.join("grok");
    flooding_fake(&executable);
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: None,
            claude_executable: None,
            claude_tool_roots: Vec::new(),
            codex_executable: None,
            codex_tool_roots: Vec::new(),
            grok_executable: Some(executable),
            grok_tool_roots: vec![tools],
            tool_roots: Vec::new(),
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    let (addr, server) = serve(hub).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({"name":"grok-flood","harness":"HARNESS_GROK","workspace":work,"supervised":true}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let prompt = "x".repeat(200_000);
    let (status, sent) = tokio::time::timeout(
        Duration::from_secs(15),
        call(
            addr,
            "ChatService/Send",
            json!({"sleeve":"grok-flood","text":prompt,"requestId":"grok-flood-0001"}),
        ),
    )
    .await
    .expect("Send deadlocked against a harness blocked on stdout");
    assert_eq!(status, 200, "{sent}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"grok-flood","until":"WAIT_UNTIL_IDLE","timeout":"10s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let events =
        protectorate_chatlog::read_all(state.join("sleeves/grok-flood/chat.jsonl")).unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.payload["text"]["delta"] == "answer: 200000"),
        "{events:?}"
    );
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"grok-flood"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}
