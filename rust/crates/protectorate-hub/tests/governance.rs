//! Run correlation, approval gates, harness pinning and token budgets
//! against a real hub over HTTP, with sandboxed fake harnesses.
#![cfg(target_os = "linux")]
#[path = "support/capability.rs"]
mod capability;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use protectorate_inventory::{Node, SHARED};
use protectorate_lease::{parse_resource, Chooser, Error};
use protectorate_node::{Agent, Jobs, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";

async fn serve(router: axum::Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
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
        serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.into())),
    )
}

async fn until<F, Fut>(what: &str, mut check: F) -> Value
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<Value>>,
{
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A stream-json harness: "egress" asks the sandbox proxy for a blocked
/// host, "burn N" reports N input tokens, "hold N" reports N and keeps the
/// turn open until interrupted, anything else reports 10.
fn fake_claude(path: &Path, variant: &str) {
    let script = format!(
        r#"#!/usr/bin/python3
# {variant}
import json,sys,os,socket
args=sys.argv
sid=args[args.index('--session-id')+1] if '--session-id' in args else args[args.index('--resume')+1]
def emit(x): print(json.dumps(x),flush=True)
n=0
for line in sys.stdin:
 m=json.loads(line)
 if m.get('type')=='control_request' and m['request']['subtype']=='interrupt':
  emit({{'type':'result','session_id':sid,'subtype':'interrupted','is_error':False}})
  continue
 if m.get('type')!='user': continue
 n+=1
 txt=m['message']['content']
 emit({{'type':'system','subtype':'init','session_id':sid}})
 answer='done'
 tokens=10
 if txt=='egress':
  s=socket.create_connection(('127.0.0.1',3128))
  s.sendall(b'CONNECT blocked.invalid:443 HTTP/1.1\r\nHost: blocked.invalid:443\r\n\r\n')
  answer=s.recv(64).decode(errors='replace').split('\r\n')[0]
 elif txt.split()[0] in ('burn','hold'):
  tokens=int(txt.split()[1])
 emit({{'type':'assistant','message':{{'id':'msg-%d'%n,'model':'claude-test','content':[{{'type':'text','text':answer}}],'usage':{{'input_tokens':tokens,'output_tokens':0}}}}}})
 if txt.startswith('hold '):
  for line in sys.stdin:
   m=json.loads(line)
   if m.get('type')=='control_request' and m['request']['subtype']=='interrupt': break
  emit({{'type':'result','session_id':sid,'subtype':'interrupted','is_error':False}})
  continue
 emit({{'type':'result','session_id':sid,'subtype':'success','is_error':False}})
"#
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Setup {
    _root: tempfile::TempDir,
    state: PathBuf,
    work: PathBuf,
    harness: PathBuf,
}

fn setup() -> (Setup, Hub) {
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
    let harness = tools.join("claude");
    fake_claude(&harness, "v1");
    let hub = Hub::open(&state, OWNER, "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: owner,
            runtime_dir: runtime,
            helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
            pi_executable: None,
            claude_executable: Some(harness.clone()),
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
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    (
        Setup {
            _root: root,
            state,
            work,
            harness,
        },
        hub,
    )
}

fn sleeve_token(state: &Path, name: &str) -> String {
    fs::read_to_string(
        state
            .join("sleeves")
            .join(name)
            .join("home/.protectorate-token"),
    )
    .unwrap()
    .trim()
    .to_owned()
}

async fn wait_idle(addr: std::net::SocketAddr, name: &str) {
    let (status, idle) = call(
        addr,
        "ChatService/Wait",
        OWNER,
        json!({"sleeve":name,"until":"WAIT_UNTIL_IDLE","timeout":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{idle}");
}

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

fn any_lease() -> Chooser {
    Arc::new(|request, blocked| {
        let resource = parse_resource(request)?;
        if let Some(held) = blocked(&resource) {
            return Err(Error::Busy(format!("held by {}", held.holder)));
        }
        Ok(resource)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_run_id_marks_the_sleeves_audit_jobs_mail_leases_checkpoints_and_egress() {
    let (setup, hub) = setup();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&setup.work)
        .status()
        .unwrap()
        .success());
    fs::write(setup.work.join("file.txt"), "one\n").unwrap();
    let node_root = setup.state.join("node");
    let node = Agent::new(Arc::new(Metrics))
        .with_jobs(Jobs::new(&node_root, 2).unwrap().with_plain_mode());
    let (node_addr, node_server) = serve(node.router().unwrap()).await;
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                tier: SHARED.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_lease_chooser(any_lease()).unwrap();
    let identity = std::sync::Arc::clone(&hub.identity);
    let (addr, server) = serve(hub.router().unwrap()).await;

    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        OWNER,
        json!({"name":"runner","harness":"HARNESS_CLAUDE","workspace":setup.work}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let run = spawned["sleeve"]["runId"].as_str().unwrap().to_owned();
    assert_eq!(run.len(), 16, "{spawned}");
    assert!(!sleeve_token(&setup.state, "runner").is_empty());
    // Capability scopes are fixed, so the owner mints the runner a wider one.
    let token = capability::mint_in(
        &identity,
        "sleeve:runner",
        &sleeve_scopes_with(&["job_submit_job"]),
    )
    .1;

    let (status, mail) = call(
        addr,
        "MessageService/Send",
        &token,
        json!({"to":"owner","body":"status: working"}),
    )
    .await;
    assert_eq!(status, 200, "{mail}");
    assert_eq!(mail["message"]["runId"], run.as_str());
    let (status, lease) = call(
        addr,
        "LeaseService/AcquireLease",
        &token,
        json!({"resource":"peer","ttl":"600s"}),
    )
    .await;
    assert_eq!(status, 200, "{lease}");
    assert_eq!(lease["runId"], run.as_str());
    assert_eq!(lease["holder"], "runner");
    let (status, job) = call(
        addr,
        "JobService/SubmitJob",
        &token,
        json!({"target":{"leaseId":lease["id"]},"command":["true"],"timeout":"30s"}),
    )
    .await;
    assert_eq!(status, 200, "{job}");
    assert_eq!(job["runId"], run.as_str());

    let (status, sent) = call(
        addr,
        "ChatService/Send",
        OWNER,
        json!({"sleeve":"runner","text":"egress"}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    wait_idle(addr, "runner").await;
    let (status, checkpoint) = call(
        addr,
        "SleeveService/CreateCheckpoint",
        OWNER,
        json!({"name":"runner"}),
    )
    .await;
    assert_eq!(status, 200, "{checkpoint}");
    assert_eq!(checkpoint["checkpoint"]["runId"], run.as_str());

    let page = until("egress and job audit entries", || async {
        let (status, page) = call(
            addr,
            "AuditService/ListAuditEntries",
            OWNER,
            json!({"runId":run,"limit":500}),
        )
        .await;
        assert_eq!(status, 200, "{page}");
        let procedures = page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["procedure"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        procedures.iter().any(|p| p == "egress").then_some(page)
    })
    .await;
    let entries = page["entries"].as_array().unwrap();
    for want in [
        "/protectorate.v1.SleeveService/SpawnSleeve",
        "/protectorate.v1.MessageService/Send",
        "/protectorate.v1.LeaseService/AcquireLease",
        "/protectorate.v1.JobService/SubmitJob",
        "/protectorate.v1.ChatService/Send",
        "/protectorate.v1.SleeveService/CreateCheckpoint",
        "egress",
    ] {
        assert!(
            entries.iter().any(|e| e["procedure"] == want),
            "no {want} entry for run {run}: {page}"
        );
    }
    let egress = entries.iter().find(|e| e["procedure"] == "egress").unwrap();
    assert_eq!(egress["subject"], "blocked.invalid:443");
    assert_eq!(egress["outcome"], "denied");
    assert!(entries.iter().all(|e| e["runId"] == run.as_str()));

    server.abort();
    node_server.abort();
}

async fn call_approved(
    addr: std::net::SocketAddr,
    method: &str,
    token: &str,
    body: Value,
    approval: &str,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nProtectorate-Approval: {approval}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

/// The scopes the hub gives a sleeve, plus `extra`.
fn sleeve_scopes_with(extra: &[&'static str]) -> Vec<&'static str> {
    let mut scopes = vec![
        "read",
        "backlog_*",
        "directory_*",
        "lease_*",
        "message_*",
        "chat_report",
    ];
    scopes.extend_from_slice(extra);
    scopes
}

fn envoy_token(hub: &Hub, scopes: &[&str]) -> String {
    capability::mint(hub, "orchestrator-envoy", scopes).1
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_changed_harness_needs_approval_before_an_agent_launches_it() {
    let (setup, hub) = setup();
    let envoy = envoy_token(
        &hub,
        &["read", "sleeve_spawn_sleeve", "sleeve_resleeve_sleeve"],
    );
    let (addr, server) = serve(hub.router().unwrap()).await;
    let spawn = json!({"name":"child","harness":"HARNESS_CLAUDE","workspace":setup.work});
    let (status, first) = call(addr, "SleeveService/SpawnSleeve", &envoy, spawn.clone()).await;
    assert_eq!(status, 200, "{first}");
    let first_hash = first["sleeve"]["harnessSha256"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(first_hash.len(), 64);
    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        OWNER,
        json!({"name":"child"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");

    fake_claude(&setup.harness, "v2");
    let (status, pending) = call(addr, "SleeveService/SpawnSleeve", &envoy, spawn.clone()).await;
    assert_eq!(status, 409, "{pending}");
    let id = pending["details"][0]["debug"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, listed) = call(addr, "ApprovalService/ListApprovals", OWNER, json!({})).await;
    let request = &listed["approvals"][0];
    assert_eq!(request["procedure"], "harness.pin");
    assert_eq!(request["subject"], "child");
    assert_eq!(request["actor"], "token:orchestrator-envoy");
    assert!(
        request["detail"]
            .as_str()
            .unwrap()
            .starts_with(&format!("chat/claude executable {}", &first_hash[..12])),
        "{listed}"
    );
    let (_, sleeve) = call(
        addr,
        "SleeveService/GetSleeve",
        OWNER,
        json!({"name":"child"}),
    )
    .await;
    assert_eq!(sleeve["sleeve"]["state"], "SLEEVE_STATE_EXITED");
    let (status, decided) = call(
        addr,
        "ApprovalService/DecideApproval",
        OWNER,
        json!({"id":id,"approve":true}),
    )
    .await;
    assert_eq!(status, 200, "{decided}");
    let (status, second) = call_approved(
        addr,
        "SleeveService/SpawnSleeve",
        &envoy,
        spawn.clone(),
        &id,
    )
    .await;
    assert_eq!(status, 200, "{second}");
    let second_hash = second["sleeve"]["harnessSha256"].as_str().unwrap();
    assert_ne!(second_hash, first_hash);

    fake_claude(&setup.harness, "v3");
    let resleeve = json!({"name":"child","to":"HARNESS_CLAUDE"});
    let (status, pending) = call(
        addr,
        "SleeveService/ResleeveSleeve",
        &envoy,
        resleeve.clone(),
    )
    .await;
    assert_eq!(status, 409, "{pending}");
    let id = pending["details"][0]["debug"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, sleeve) = call(
        addr,
        "SleeveService/GetSleeve",
        OWNER,
        json!({"name":"child"}),
    )
    .await;
    assert_eq!(sleeve["sleeve"]["state"], "SLEEVE_STATE_RUNNING");
    assert_eq!(sleeve["sleeve"]["harnessSha256"], second_hash);
    call(
        addr,
        "ApprovalService/DecideApproval",
        OWNER,
        json!({"id":id,"approve":false}),
    )
    .await;
    let (status, denied) =
        call_approved(addr, "SleeveService/ResleeveSleeve", &envoy, resleeve, &id).await;
    assert_eq!(status, 403, "{denied}");

    let (status, killed) = call(
        addr,
        "SleeveService/KillSleeve",
        OWNER,
        json!({"name":"child"}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    let (status, owner_spawn) = call(addr, "SleeveService/SpawnSleeve", OWNER, spawn).await;
    assert_eq!(status, 200, "{owner_spawn}");
    let (_, audit) = call(
        addr,
        "AuditService/ListAuditEntries",
        OWNER,
        json!({"subject":"child","limit":500}),
    )
    .await;
    let entries = audit["entries"].as_array().unwrap();
    let changed = entries
        .iter()
        .find(|e| e["procedure"] == "harness.pin_changed")
        .unwrap();
    assert_eq!(changed["actor"], "owner");
    assert!(changed["detail"]
        .as_str()
        .unwrap()
        .contains("chat/claude executable"));
    server.abort();
}

async fn send(addr: std::net::SocketAddr, name: &str, text: &str) -> (u16, Value) {
    call(
        addr,
        "ChatService/Send",
        OWNER,
        json!({"sleeve":name,"text":text}),
    )
    .await
}

async fn used(addr: std::net::SocketAddr, name: &str) -> (u64, u64) {
    let (_, sleeve) = call(addr, "SleeveService/GetSleeve", OWNER, json!({"name":name})).await;
    let field = |key: &str| {
        sleeve["sleeve"][key]
            .as_str()
            .map_or(0, |n| n.parse::<u64>().unwrap())
    };
    (field("tokensUsed"), field("tokenBudget"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn budgets_warn_at_90_percent_and_stop_the_turn_at_100_including_children() {
    let (setup, hub) = setup();
    let identity = std::sync::Arc::clone(&hub.identity);
    let (addr, server) = serve(hub.router().unwrap()).await;
    let (status, spawned) = call(
        addr,
        "SleeveService/SpawnSleeve",
        OWNER,
        json!({"name":"parent","harness":"HARNESS_CLAUDE","workspace":setup.work,"tokenBudget":"200"}),
    )
    .await;
    assert_eq!(status, 200, "{spawned}");
    let run = spawned["sleeve"]["runId"].as_str().unwrap().to_owned();
    assert!(!sleeve_token(&setup.state, "parent").is_empty());
    let parent = capability::mint_in(
        &identity,
        "sleeve:parent",
        &sleeve_scopes_with(&["sleeve_spawn_sleeve"]),
    )
    .1;
    let kid_work = setup.work.with_file_name("kid-work");
    fs::create_dir_all(&kid_work).unwrap();
    let (status, kid) = call(
        addr,
        "SleeveService/SpawnSleeve",
        &parent,
        json!({"name":"kid","harness":"HARNESS_CLAUDE","workspace":kid_work,"tokenBudget":"5000"}),
    )
    .await;
    assert_eq!(status, 200, "{kid}");
    assert_eq!(kid["sleeve"]["tokenBudget"], "200", "{kid}");

    assert_eq!(send(addr, "parent", "burn 120").await.0, 200);
    wait_idle(addr, "parent").await;
    assert_eq!(used(addr, "parent").await, (120, 200));
    assert_eq!(send(addr, "kid", "burn 65").await.0, 200);
    wait_idle(addr, "kid").await;
    assert_eq!(used(addr, "kid").await, (65, 200));
    assert_eq!(used(addr, "parent").await, (185, 200));
    let chat = setup.state.join("sleeves/parent/chat.jsonl");
    let notices = || {
        protectorate_chatlog::read_all(&chat)
            .unwrap()
            .into_iter()
            .filter_map(|e| e.payload["error"]["text"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    assert_eq!(notices().len(), 1, "{:?}", notices());
    assert!(notices()[0].starts_with("Token budget 90% used: 185 of 200"));

    assert_eq!(send(addr, "parent", "hold 20").await.0, 200);
    wait_idle(addr, "parent").await;
    assert_eq!(used(addr, "parent").await, (205, 200));
    assert!(
        notices()
            .iter()
            .any(|n| n.starts_with("Token budget spent: 205 of 200")),
        "{:?}",
        notices()
    );
    let (status, refused) = send(addr, "parent", "more").await;
    assert_eq!(status, 429, "{refused}");
    let (status, refused) = call(
        addr,
        "SleeveService/SpawnSleeve",
        &parent,
        json!({"name":"kid2","harness":"HARNESS_CLAUDE","workspace":kid_work}),
    )
    .await;
    assert_eq!(status, 429, "{refused}");
    assert_eq!(send(addr, "kid", "burn 5").await.0, 200);
    wait_idle(addr, "kid").await;
    let (_, audit) = call(
        addr,
        "AuditService/ListAuditEntries",
        OWNER,
        json!({"runId":run,"procedure":"budget.","limit":50}),
    )
    .await;
    let procedures = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["procedure"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        procedures,
        ["budget.exhausted", "budget.warning"],
        "{audit}"
    );
    server.abort();
}
