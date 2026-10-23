//! Run correlation, approval gates, harness pinning and token budgets
//! against a real hub over HTTP, with sandboxed fake harnesses.
#![cfg(target_os = "linux")]
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
/// host, "burn N" reports N input tokens, anything else reports 10.
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
 elif txt.startswith('burn '):
  tokens=int(txt.split()[1])
 emit({{'type':'assistant','message':{{'id':'msg-%d'%n,'model':'claude-test','content':[{{'type':'text','text':answer}}],'usage':{{'input_tokens':tokens,'output_tokens':0}}}}}})
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
    let token = sleeve_token(&setup.state, "runner");
    let (_, tokens) = call(addr, "TokenService/ListTokens", OWNER, json!({})).await;
    let token_id = tokens["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "sleeve:runner" && t.get("revokedAt").is_none())
        .unwrap()["id"]
        .clone();
    let (status, scoped) = call(
        addr,
        "TokenService/UpdateTokenScopes",
        OWNER,
        json!({"id":token_id,"add":["job_submit_job"]}),
    )
    .await;
    assert_eq!(status, 200, "{scoped}");

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
