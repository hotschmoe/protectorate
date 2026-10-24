#![cfg(target_os = "linux")]
use crate::support::private_tmux;
use private_tmux::PrivateTmux;
use protectorate_hub::{ChatLauncher, ChatRuntime, Hub, TerminalLauncher};
use protectorate_session::Manager;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// Real panes captured from the harness TUIs; the first line names the harness.
const CLAUDE_BLOCKED: &str =
    include_str!("../../../fuzz/corpus/terminal_pane/claude-permission-prompt");
const CODEX_WORKING: &str = include_str!("../../../fuzz/corpus/terminal_pane/codex-working");
const PI_IDLE: &str = include_str!("../../../fuzz/corpus/terminal_pane/pi-idle");

async fn call(addr: std::net::SocketAddr, path: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

async fn line(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n") {
            return String::from_utf8(bytes).unwrap();
        }
    }
}

async fn event(stream: &mut TcpStream) -> Value {
    let count = usize::from_str_radix(line(stream).await.trim(), 16).unwrap();
    let mut frame = vec![0; count];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(line(stream).await, "\r\n");
    serde_json::from_slice(&frame[5..]).unwrap()
}

/// Reads events until every sleeve has announced its expected activity.
async fn activity_events(stream: &mut TcpStream, mut want: BTreeMap<&str, &str>) -> Vec<Value> {
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(60), async {
        while !want.is_empty() {
            let received = event(stream).await;
            if received["kind"] != "sleeve.activity" {
                continue;
            }
            let subject = received["subject"].as_str().unwrap().to_owned();
            if want.get(subject.as_str()) == received["payload"]["activity"].as_str().as_ref() {
                want.remove(subject.as_str());
                seen.push(received);
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing activity events: {want:?}"));
    seen
}

fn fake_tui(path: &std::path::Path, pane: &str) {
    let body = pane
        .lines()
        .skip(1)
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim_end();
    fs::write(
        path,
        format!("#!/usr/bin/sh\ncat <<'PANE'\n{body}\nPANE\nexec sleep 600\n"),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_sleeves_report_blocked_working_and_idle_from_their_screens() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner_home = root.path().join("owner");
    let workspace = root.path().join("workspace");
    let runtime = root.path().join("runtime");
    let tools = root.path().join("tools");
    for dir in [&state, &owner_home, &workspace, &runtime, &tools] {
        fs::create_dir_all(dir).unwrap();
    }
    let cases = [
        (
            "claude",
            "HARNESS_CLAUDE",
            CLAUDE_BLOCKED,
            "blocked",
            "claude.permission",
        ),
        (
            "codex",
            "HARNESS_CODEX",
            CODEX_WORKING,
            "working",
            "codex.interrupt",
        ),
        ("pi", "HARNESS_PI", PI_IDLE, "idle", "pi.footer"),
    ];
    for (provider, _, pane, _, _) in cases {
        fs::create_dir_all(state.join("auth").join(provider)).unwrap();
        fake_tui(&tools.join(provider), pane);
    }
    let socket = format!("p-rust-activity-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let helper = std::path::PathBuf::from(env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper"));
    let terminal = TerminalLauncher {
        state_root: state.clone(),
        user_home: owner_home.clone(),
        runtime_dir: runtime.clone(),
        runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
        helper: helper.clone(),
        hub_url: format!("http://{addr}"),
        cli_executable: None,
        tool_roots: Vec::new(),
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        tasks_max: 64,
    };
    let chat = ChatLauncher {
        state_root: state.clone(),
        user_home: owner_home,
        runtime_dir: runtime,
        helper,
        pi_executable: Some(tools.join("pi")),
        claude_executable: Some(tools.join("claude")),
        claude_tool_roots: vec![tools.clone()],
        codex_executable: Some(tools.join("codex")),
        codex_tool_roots: vec![tools.clone()],
        grok_executable: None,
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
        .with_sessions(Arc::clone(&manager))
        .with_terminal_launcher(terminal)
        .with_chat_runtime(ChatRuntime::new(chat));
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let frame = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!(
        "POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n",
        frame.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&frame).await.unwrap();
    assert!(line(&mut stream).await.contains("200"));
    while line(&mut stream).await != "\r\n" {}

    for (name, harness, _, _, _) in cases {
        let (status, spawned) = call(
            addr,
            "/protectorate.v1.SleeveService/SpawnSleeve",
            json!({"name":name,"harness":harness,"mode":"SLEEVE_MODE_TERMINAL","workspace":workspace}),
        )
        .await;
        assert_eq!(status, 200, "{name}: {spawned}");
    }
    let announced = activity_events(&mut stream, cases.iter().map(|c| (c.0, c.3)).collect()).await;
    for (name, _, _, activity, rule) in cases {
        let event = announced.iter().find(|e| e["subject"] == name).unwrap();
        assert_eq!(event["payload"]["rule"], rule, "{event}");
        let upper = format!("TERMINAL_ACTIVITY_{}", activity.to_uppercase());
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            let (status, got) = call(
                addr,
                "/protectorate.v1.SleeveService/GetTerminalStatus",
                json!({"name":name}),
            )
            .await;
            assert_eq!(status, 200, "{got}");
            if got["status"]["activity"] == upper {
                break got;
            }
            assert!(
                Instant::now() < deadline,
                "{name} never read as {activity}: {got}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        assert_eq!(status["status"]["rule"], rule, "{status}");
        let matched = status["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["matched"] == true)
            .unwrap();
        assert_eq!(
            matched["id"], rule,
            "the first matching rule decides: {status}"
        );
        assert!(!status["lines"].as_array().unwrap().is_empty(), "{status}");
        let (_, shown) = call(
            addr,
            "/protectorate.v1.SleeveService/GetSleeve",
            json!({"name":name}),
        )
        .await;
        assert_eq!(shown["sleeve"]["terminal"]["activity"], upper, "{shown}");
    }
    let (status, wrong) = call(
        addr,
        "/protectorate.v1.SleeveService/GetTerminalStatus",
        json!({"name":"nobody"}),
    )
    .await;
    assert_eq!(status, 404, "{wrong}");
    for (name, ..) in cases {
        let (status, killed) = call(
            addr,
            "/protectorate.v1.SleeveService/KillSleeve",
            json!({"name":name}),
        )
        .await;
        assert_eq!(status, 200, "{killed}");
    }
    server.abort();
}
