//! Each driver runs against a real child process that replays the fuzz input
//! as the harness's stdout after a fixed, valid handshake.
use protectorate_chatlog::{read_all, Log};
use protectorate_sandbox::{
    runtime::LaunchOptions, Limits, Policy, Request, ValidatedPlan, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const AUTONOMOUS: &[u8] = b"#autonomous\n";

struct Peer {
    _dir: tempfile::TempDir,
    log_path: PathBuf,
    log: Arc<Log>,
    plan: ValidatedPlan,
    options: LaunchOptions,
}

/// `steps` pairs a count of driver lines to read first with the bytes the
/// child then writes. After the last step the child closes its stdout and
/// keeps draining stdin until it is killed.
fn peer(steps: &[(usize, &[u8])]) -> Peer {
    let dir = tempfile::tempdir().expect("scratch directory");
    let root = dir.path().to_path_buf();
    for name in ["state", "home", "work", "runtime"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    let mut script = String::new();
    let mut args = Vec::new();
    for (i, (reads, bytes)) in steps.iter().enumerate() {
        let file = root.join(format!("step{i}"));
        fs::write(&file, bytes).unwrap();
        for _ in 0..*reads {
            script.push_str("read -r _ || exit 0; ");
        }
        script.push_str(&format!("cat \"${}\"; ", i + 1));
        args.push(file.to_string_lossy().into_owned());
    }
    script.push_str("exec >&-; exec cat >/dev/null");
    let mut command = vec!["/bin/sh".into(), "-c".into(), script, "peer".into()];
    command.extend(args);
    let policy = Policy {
        state_root: root.join("state"),
        user_home: root.join("home"),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let plan = policy
        .plan(Request {
            workload: Workload::DirectHarnessDriver,
            command,
            workspace: None,
            managed_workspace_name: None,
            private_home: None,
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: Limits {
                cpu_percent: 0,
                memory_bytes: 0,
                tasks_max: 0,
            },
            egress: None,
        })
        .expect("direct harness plan");
    let options = LaunchOptions::new(
        "/nonexistent-helper",
        root.join("runtime"),
        root.join("home"),
        root.join("work"),
    );
    let log_path = root.join("chat.jsonl");
    let log = Arc::new(Log::open(&log_path).unwrap());
    Peer {
        _dir: dir,
        log_path,
        log,
        plan,
        options,
    }
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "{what} did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}

/// The reader and waiter threads each hold a session clone. Waits until the
/// reader has consumed all of stdout, stops the child, and checks the chat
/// log the driver left behind.
fn settle<S>(peer: &Peer, session: Arc<S>, status: impl Fn(&S), stop: impl Fn(&S)) {
    wait_for("driver stdout reader", || Arc::strong_count(&session) <= 2);
    status(&session);
    stop(&session);
    wait_for("driver exit waiter", || Arc::strong_count(&session) == 1);
    status(&session);
    check_log(&peer.log_path);
}

fn check_log(path: &Path) {
    let events = read_all(path).expect("the chat log the driver wrote parses");
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, i as u64 + 1, "chat log sequence gap");
    }
    let last = events.last().expect("an exited session logged its exit");
    assert_eq!(last.payload["state"]["state"], "CHAT_STATE_EXITED");
    Log::open(path).expect("reopen the chat log");
}

fn split_autonomous(data: &[u8]) -> (bool, &[u8]) {
    match data.strip_prefix(AUTONOMOUS) {
        Some(rest) => (true, rest),
        None => (false, data),
    }
}

/// Claude Code `--output-format stream-json` lines after one user message.
pub fn claude_stream(data: &[u8]) {
    let (autonomous, data) = split_autonomous(data);
    let peer = peer(&[(1, data)]);
    let Ok(session) = protectorate_claude_driver::Session::start(
        &peer.plan,
        peer.options.clone(),
        Arc::clone(&peer.log),
        "fuzz-session".into(),
        autonomous,
    ) else {
        return;
    };
    let _ = session.send("hello", "fuzz", "r1");
    settle(&peer, session, |s| drop(s.status()), |s| s.stop());
}

const CODEX_HANDSHAKE: &[u8] = b"{\"id\":1,\"result\":{}}\n\
{\"id\":2,\"result\":{\"thread\":{\"id\":\"fuzz-thread\"},\"model\":\"gpt\",\"reasoningEffort\":\"low\"}}\n\
{\"id\":3,\"result\":{\"turn\":{\"id\":\"fuzz-turn\"}}}\n";

/// Codex app-server JSON-RPC notifications and requests during one turn.
pub fn codex_rpc(data: &[u8]) {
    let (autonomous, data) = split_autonomous(data);
    let peer = peer(&[(0, CODEX_HANDSHAKE), (4, data)]);
    let Ok(session) = protectorate_codex_driver::Session::start(
        &peer.plan,
        peer.options.clone(),
        Arc::clone(&peer.log),
        None,
        autonomous,
    ) else {
        return;
    };
    let _ = session.send("hello", "fuzz", "r1");
    settle(&peer, session, |s| drop(s.status()), |s| s.stop());
}

const GROK_HANDSHAKE: &[u8] =
    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}\n\
{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"fuzz-session\"}}\n\
{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}\n";

/// Grok ACP (`grok agent stdio`) updates, permission requests and the reply
/// to prompt id 4.
pub fn grok_acp(data: &[u8]) {
    let (autonomous, data) = split_autonomous(data);
    let peer = peer(&[(0, GROK_HANDSHAKE), (4, data)]);
    let Ok(session) = protectorate_grok_driver::Session::start(
        &peer.plan,
        peer.options.clone(),
        Arc::clone(&peer.log),
        None,
        autonomous,
    ) else {
        return;
    };
    let _ = session.send("hello", "fuzz", "r1");
    settle(&peer, session, |s| drop(s.status()), |s| s.stop());
}

const PI_HANDSHAKE: &[u8] = b"{\"type\":\"response\",\"id\":\"c1\",\"command\":\"get_state\",\"success\":true,\
\"data\":{\"sessionId\":\"fuzz-session\",\"thinkingLevel\":\"low\",\
\"model\":{\"id\":\"m\",\"provider\":\"p\",\"baseUrl\":\"http://127.0.0.1:1\",\"contextWindow\":1000}}}\n";

/// Pi `--mode rpc` events after one prompt.
pub fn pi_rpc(data: &[u8]) {
    let peer = peer(&[(1, PI_HANDSHAKE), (1, data)]);
    let Ok(session) = protectorate_pi_driver::Session::start(
        &peer.plan,
        peer.options.clone(),
        Arc::clone(&peer.log),
        None,
    ) else {
        return;
    };
    let _ = session.send("hello", "fuzz", "r1");
    settle(&peer, session, |s| drop(s.status()), |s| s.stop());
}
