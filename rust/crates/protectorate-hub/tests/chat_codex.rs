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
import json,os,subprocess,sys
assert sys.argv[1:] == ['app-server']
home=os.environ['CODEX_HOME']
assert json.load(open(os.path.join(home,'auth.json')))['test']=='credential'
assert 'danger-full-access' in open(os.path.join(home,'config.toml')).read()
tid='thread-test'
turn=0
selection='gpt-test'
def emit(x): print(json.dumps(x),flush=True)
def note(method,params): params['threadId']=tid; emit({'method':method,'params':params})
def complete(status='completed'):
 note('turn/completed',{'turn':{'id':'turn-'+str(turn),'status':status}})
for line in sys.stdin:
 m=json.loads(line)
 if 'method' not in m: continue
 method=m['method']; p=m.get('params',{}); mid=m.get('id')
 if method=='initialized': continue
 if method=='initialize':
  assert p['capabilities']['experimentalApi'] == True
  emit({'id':mid,'result':{}})
 elif method=='thread/start':
  assert p['cwd'] and 'approvalPolicy' not in p
  assert subprocess.run(['git','config','user.email'],capture_output=True,text=True).stdout=='owner@example.com\n'
  emit({'id':mid,'result':{'thread':{'id':tid},'model':'gpt-test','reasoningEffort':'medium'}})
 elif method=='thread/resume':
  assert p['threadId']==tid and p['excludeTurns']==True
  emit({'id':mid,'result':{'thread':{'id':tid},'model':'gpt-test','reasoningEffort':'medium'}})
 elif method=='model/list':
  if p.get('cursor')=='more':
   emit({'id':mid,'result':{'data':[{'model':'gpt-alt','displayName':'Alternate','isDefault':False,'defaultReasoningEffort':'low','supportedReasoningEfforts':[{'reasoningEffort':'low'}]}]}})
  else:
   emit({'id':mid,'result':{'data':[{'model':'gpt-test','displayName':'Test','isDefault':True,'defaultReasoningEffort':'medium','supportedReasoningEfforts':[{'reasoningEffort':'medium'},{'reasoningEffort':'high'}]}],'nextCursor':'more'}})
 elif method=='thread/settings/update':
  selection=p['model']; emit({'id':mid,'result':{}})
  note('thread/settings/updated',{'threadSettings':{'model':selection,'effort':p.get('effort','low')}})
 elif method=='turn/start':
  turn+=1; txt=p['input'][0]['text']
  emit({'id':mid,'result':{'turn':{'id':'turn-'+str(turn)}}})
  note('turn/started',{'turn':{'id':'turn-'+str(turn)}})
  if txt in ('approve','deny','permissions'):
   if txt=='permissions':
    method='item/permissions/requestApproval'; params={'permissions':{'network':{'enabled':True}},'reason':'network'}
   else:
    method='item/commandExecution/requestApproval'; params={'command':'printf ok'}
   emit({'id':0,'method':method,'params':params})
   reply=json.loads(next(sys.stdin)); assert reply['id']==0
   if txt=='permissions': assert reply['result']['permissions']==({'network':{'enabled':True}} if os.environ.get('EXPECT_ALLOW','1')=='1' else {})
   else: assert reply['result']['decision']==('decline' if txt=='deny' else 'accept')
  elif txt=='slow':
   reply=json.loads(next(sys.stdin)); assert reply['method']=='turn/steer'
   assert reply['params']['expectedTurnId']=='turn-'+str(turn)
   emit({'id':reply['id'],'result':{'turnId':'turn-'+str(turn)}})
  elif txt=='interrupt':
   reply=json.loads(next(sys.stdin)); assert reply['method']=='turn/interrupt'
   emit({'id':reply['id'],'result':{}}); complete('interrupted'); continue
  note('item/started',{'item':{'id':'exec-1','type':'commandExecution','command':'printf ok'}})
  note('item/completed',{'item':{'id':'exec-1','type':'commandExecution','aggregatedOutput':'ok','exitCode':0}})
  note('item/agentMessage/delta',{'itemId':'msg-1','delta':'answer: '+txt})
  note('thread/tokenUsage/updated',{'tokenUsage':{'last':{'inputTokens':10,'outputTokens':3,'cachedInputTokens':4,'totalTokens':13},'modelContextWindow':258400}})
  complete()
 elif method=='thread/compact/start':
  emit({'id':mid,'result':{}})
  note('item/started',{'item':{'id':'comp-1','type':'contextCompaction'}})
  note('thread/tokenUsage/updated',{'tokenUsage':{'last':{'inputTokens':0,'totalTokens':3}}})
  note('item/completed',{'item':{'id':'comp-1','type':'contextCompaction'}})
  note('turn/completed',{'turn':{'id':'comp-turn','status':'completed'}})
 elif method=='thread/unsubscribe': emit({'id':mid,'result':{}})
"#;
    executable::write_executable(path, script);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_app_server_sandbox_turns_approvals_selection_compaction_and_resume() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let work = root.path().join("work");
    let explicit_work = root.path().join("work-explicit");
    let tools = root.path().join("tools");
    let runtime = root.path().join("runtime");
    for dir in [
        &state,
        &owner,
        &work,
        &explicit_work,
        &tools,
        &runtime,
        &state.join("auth/codex"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        owner.join(".gitconfig"),
        "[user]\n\tname = Owner\n\temail = owner@example.com\n",
    )
    .unwrap();
    fs::write(
        state.join("auth/codex/auth.json"),
        r#"{"test":"credential"}"#,
    )
    .unwrap();
    let executable = tools.join("codex");
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
            codex_executable: Some(executable),
            codex_tool_roots: vec![tools],
            grok_executable: None,
            grok_tool_roots: Vec::new(),
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
    let (status, catalog) = call(addr, "ChatService/ListModels", json!({"harness":"codex"})).await;
    assert_eq!(status, 200, "{catalog}");
    assert_eq!(catalog["supported"], true);
    assert_eq!(catalog["models"].as_array().unwrap().len(), 2);
    assert_eq!(catalog["models"][1]["model"], "gpt-alt");
    let (status, sleeves) = call(addr, "SleeveService/ListSleeves", json!({})).await;
    assert_eq!(status, 200, "{sleeves}");
    assert!(
        sleeves["sleeves"].as_array().is_none_or(Vec::is_empty),
        "{sleeves}"
    );
    let (status, unsupported) =
        call(addr, "ChatService/ListModels", json!({"harness":"claude"})).await;
    assert_eq!(status, 200, "{unsupported}");
    assert!(!unsupported["supported"].as_bool().unwrap_or(false));
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({"name":"codex-one","harness":"HARNESS_CODEX","workspace":work,"brief":"hello"}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    assert_eq!(spawned["sleeve"]["chat"]["harnessSessionId"], "thread-test");
    assert_eq!(
        spawned["sleeve"]["chat"]["settings"]["requested"]["effort"],
        "high"
    );
    assert_eq!(
        spawned["sleeve"]["chat"]["settings"]["effective"]["effort"],
        "high"
    );
    let (status, explicit) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({"name":"codex-explicit","harness":"HARNESS_CODEX","workspace":explicit_work,
            "selection":{"effort":"medium"}}),
    )
    .await;
    assert_eq!(status, 200, "{explicit}");
    assert_eq!(
        explicit["sleeve"]["chat"]["settings"]["requested"]["effort"],
        "medium"
    );
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"codex-explicit"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, models) = call(
        addr,
        "ChatService/ListModels",
        json!({"sleeve":"codex-one"}),
    )
    .await;
    assert_eq!(status, 200, "{models}");
    assert_eq!(models["models"].as_array().unwrap().len(), 2);
    let (status, invalid) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"codex-one","selection":{"model":"gpt-alt","effort":"high"}}),
    )
    .await;
    assert_eq!(status, 400, "{invalid}");
    let (status, selected) = call(
        addr,
        "ChatService/SetSelection",
        json!({"sleeve":"codex-one","selection":{"model":"gpt-alt","effort":"low"}}),
    )
    .await;
    assert_eq!(status, 200, "{selected}");
    assert_eq!(selected["chat"]["model"], "gpt-alt");
    let logged =
        protectorate_chatlog::read_all(state.join("sleeves/codex-one/chat.jsonl")).unwrap();
    assert!(logged
        .iter()
        .any(|e| e.payload["settings"]["requested"]["model"] == "gpt-alt"));
    for (text, allow) in [("approve", true), ("deny", false)] {
        let (status, sent) = call(
            addr,
            "ChatService/Send",
            json!({"sleeve":"codex-one","text":text,"requestId":if allow {"codex-once"} else {"codex-twice"}}),
        )
        .await;
        assert_eq!(status, 200, "{sent}");
        let path = state.join("sleeves/codex-one/chat.jsonl");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
            if events
                .iter()
                .filter(|e| e.payload["approval"]["id"] == "0")
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
            json!({"sleeve":"codex-one","approvalId":"0","allow":allow}),
        )
        .await;
        assert_eq!(status, 200, "{response}");
        let (status, idle) = call(
            addr,
            "ChatService/Wait",
            json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
        )
        .await;
        assert_eq!(status, 200, "{idle}");
    }
    let (status, replay) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"codex-one","text":"approve","requestId":"codex-once"}),
    )
    .await;
    assert_eq!(status, 200, "{replay}");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"codex-one","text":"slow"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let (status, steered) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"codex-one","text":"follow up"}),
    )
    .await;
    assert_eq!(status, 200, "{steered}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"codex-one","text":"interrupt"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let (status, interrupted) =
        call(addr, "ChatService/Interrupt", json!({"sleeve":"codex-one"})).await;
    assert_eq!(status, 200, "{interrupted}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, sent) = call(
        addr,
        "ChatService/Send",
        json!({"sleeve":"codex-one","text":"permissions"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let path = state.join("sleeves/codex-one/chat.jsonl");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let events = protectorate_chatlog::read_all(&path).unwrap_or_default();
        if events
            .iter()
            .any(|e| e.payload["approval"]["kind"] == "permissions")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "missing permissions approval: {events:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, response) = call(
        addr,
        "ChatService/Respond",
        json!({"sleeve":"codex-one","approvalId":"0","allow":true}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, compacted) =
        call(addr, "ChatService/Compact", json!({"sleeve":"codex-one"})).await;
    assert_eq!(status, 200, "{compacted}");
    assert_eq!(compacted["compaction"]["tokensBefore"], "10");
    assert_eq!(compacted["compaction"]["tokensAfter"], "3");
    let path = state.join("sleeves/codex-one/chat.jsonl");
    let events = protectorate_chatlog::read_all(&path).unwrap();
    assert!(events
        .iter()
        .any(|e| e.payload["toolResult"]["output"] == "ok"));
    assert!(events
        .iter()
        .any(|e| e.payload["usage"]["cacheReadTokens"] == "4"));
    crate::support::chatlog::assert_decodable(&events);
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"codex-one"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    let (status, resumed) = call(
        addr,
        "SleeveService/SpawnSleeve",
        json!({"name":"codex-one","harness":"HARNESS_CODEX","workspace":work,"brief":"resume"}),
    )
    .await;
    assert_eq!(status, 200, "{resumed}");
    assert_eq!(resumed["sleeve"]["chat"]["harnessSessionId"], "thread-test");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-one","until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"codex-one"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}

/// A Codex that answers a turn only after flooding stdout with far more than
/// a pipe holds, before it has read the whole turn/start line.
fn flooding_fake(path: &Path) {
    let script = r#"#!/usr/bin/python3
import json,os,sys,threading
tid='thread-flood'
buf=b''; flooded=False
def emit(x): sys.stdout.write(json.dumps(x)+'\n'); sys.stdout.flush()
def note(method,params): params['threadId']=tid; emit({'method':method,'params':params})
def recv():
 global buf,flooded
 while b'\n' not in buf:
  chunk=os.read(0,4096)
  if not chunk: raise SystemExit(0)
  buf+=chunk
  if not flooded and b'\n' not in buf and len(buf)>=4096:
   flooded=True; threading.Timer(20,os._exit,[3]).start()
   for _ in range(256): emit({'method':'noise','params':{'pad':'x'*1024}})
 line,_,buf=buf.partition(b'\n')
 return json.loads(line)
while True:
 m=recv(); method=m.get('method'); p=m.get('params',{}); mid=m.get('id')
 if method=='thread/start':
  emit({'id':mid,'result':{'thread':{'id':tid},'model':'gpt-test','reasoningEffort':'medium'}})
 elif method=='turn/start':
  txt=p['input'][0]['text']
  emit({'id':mid,'result':{'turn':{'id':'turn-1'}}})
  note('turn/started',{'turn':{'id':'turn-1'}})
  note('item/agentMessage/delta',{'itemId':'msg-1','delta':'answer: '+str(len(txt))})
  note('turn/completed',{'turn':{'id':'turn-1','status':'completed'}})
 elif method=='model/list':
  emit({'id':mid,'result':{'data':[{'model':'gpt-test','displayName':'Test','isDefault':True,'defaultReasoningEffort':'medium','supportedReasoningEfforts':[{'reasoningEffort':'medium'}]}]}})
 elif method=='thread/settings/update':
  emit({'id':mid,'result':{}})
  note('thread/settings/updated',{'threadSettings':{'model':p['model'],'effort':p.get('effort','medium')}})
 elif method and mid is not None:
  emit({'id':mid,'result':{}})
"#;
    executable::write_executable(path, script);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_turn_larger_than_a_pipe_while_codex_floods_stdout() {
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
        &state.join("auth/codex"),
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        state.join("auth/codex/auth.json"),
        r#"{"test":"credential"}"#,
    )
    .unwrap();
    let executable = tools.join("codex");
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
            codex_executable: Some(executable),
            codex_tool_roots: vec![tools],
            grok_executable: None,
            grok_tool_roots: Vec::new(),
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
        json!({"name":"codex-flood","harness":"HARNESS_CODEX","workspace":work}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let prompt = "x".repeat(200_000);
    let (status, sent) = tokio::time::timeout(
        Duration::from_secs(15),
        call(
            addr,
            "ChatService/Send",
            json!({"sleeve":"codex-flood","text":prompt,"requestId":"codex-flood-0001"}),
        ),
    )
    .await
    .expect("Send deadlocked against a harness blocked on stdout");
    assert_eq!(status, 200, "{sent}");
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        json!({"sleeve":"codex-flood","until":"WAIT_UNTIL_IDLE","timeout":"10s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
    let events =
        protectorate_chatlog::read_all(state.join("sleeves/codex-flood/chat.jsonl")).unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.payload["text"]["delta"] == "answer: 200000"),
        "{events:?}"
    );
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        json!({"name":"codex-flood"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    server.abort();
}
