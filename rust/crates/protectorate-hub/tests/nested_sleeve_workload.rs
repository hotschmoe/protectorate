#![cfg(target_os = "linux")]
#[path = "support/private_tmux.rs"]
mod private_tmux;

use private_tmux::PrivateTmux;
use protectorate_hub::{Hub, TerminalLauncher};
use protectorate_session::Manager;
use serde_json::{json, Value};
use std::{
    fs,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /protectorate.v1.SleeveService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_hub_sleeve_builds_sibling_with_private_overlay_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let owner = root.path().join("owner");
    let runtime = root.path().join("runtime");
    let app = state.join("workspaces/app");
    let lib = state.join("workspaces/lib");
    let sdk = root.path().join("flutter");
    let sdk_cache = sdk.join("bin/cache");
    let lower = state.join("cache/flutter-test");
    for path in [&state, &owner, &runtime, &app, &lib, &sdk_cache, &lower] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(
        app.join("main.c"),
        "#include \"../lib/module.h\"\nint main(void) { return module_answer() != 42; }\n",
    )
    .unwrap();
    fs::write(
        lib.join("module.h"),
        "static int module_answer(void) { return 42; }\n",
    )
    .unwrap();
    fs::write(root.path().join("hidden-secret"), "ungranted").unwrap();
    fs::write(sdk_cache.join("shared"), "host-original").unwrap();
    fs::write(lower.join("shared"), "published-original").unwrap();
    fs::write(
        lower.join(".protectorate-flutter-cache.json"),
        json!({"sdk":sdk,"sha256":"test","bytes":18,"entries":1}).to_string(),
    )
    .unwrap();

    let socket = format!("p-rust-nested-{}", std::process::id());
    let _tmux = PrivateTmux::new(&socket);
    let manager = Arc::new(Manager::new(&socket));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let launcher = TerminalLauncher {
        state_root: state.clone(),
        user_home: owner,
        runtime_dir: runtime.clone(),
        runner: env!("CARGO_BIN_EXE_protectorate-hub-sleeve-runner").into(),
        helper: env!("CARGO_BIN_EXE_protectorate-hub-sandbox-helper").into(),
        hub_url: format!("http://{addr}"),
        cli_executable: None,
        tool_roots: vec![sdk.clone()],
        flutter_sdk_root: Some(sdk.clone()),
        flutter_cache_lower: Some(lower.clone()),
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: None,
        tasks_max: 64,
    };
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager))
        .with_terminal_launcher(launcher);
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let (status, spawned) = call(
        addr,
        "SpawnSleeve",
        json!({
            "name":"nested-app", "harness":"HARNESS_SHELL", "workspace":app,
            "readOnly":[{"source":lib,"target":lib}],
            "nested":true, "limits":{"cpuPercent":100,"memoryBytes":"536870912"},
            "args":["sh"]
        }),
    )
    .await;
    assert_eq!(status, 200, "required nested launch failed: {spawned}");
    assert_eq!(spawned["sleeve"]["nested"], true);
    assert_eq!(
        spawned["sleeve"]["readOnly"][0]["source"],
        lib.to_string_lossy().as_ref()
    );
    let pane_pid = Command::new("tmux")
        .args([
            "-L",
            &socket,
            "display-message",
            "-p",
            "-t",
            "=sleeve-nested-app:",
            "#{pane_pid}",
        ])
        .output()
        .unwrap();
    assert!(pane_pid.status.success());
    let pane_pid = String::from_utf8(pane_pid.stdout).unwrap();
    let pane_pid = pane_pid.trim();
    assert!(
        fs::read_dir(&runtime).unwrap().next().is_none(),
        "host mount staging must be removed after namespace setup"
    );
    let units = Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--all",
            "--plain",
            "--no-legend",
            "--full",
            &format!("protectorate-{pane_pid}-*.scope"),
        ])
        .output()
        .unwrap();
    assert!(units.status.success());
    let units = String::from_utf8(units.stdout).unwrap();
    let unit = units
        .split_whitespace()
        .next()
        .expect("nested sleeve has a bounded systemd scope")
        .to_owned();
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &unit])
        .status()
        .unwrap();
    assert!(active.success(), "nested sleeve scope {unit} is not active");

    let command = format!(
        "set -eu; test ! -e '{secret}'; test \"$(cat '{sdk}/bin/cache/shared')\" = published-original; printf private-copy > '{sdk}/bin/cache/shared'; test \"$(cat ../lib/module.h)\" = 'static int module_answer(void) {{ return 42; }}'; if touch ../lib/forbidden 2>/dev/null; then exit 88; fi; root=/run/protectorate/cgroup; test -d \"$root\"; grep -q '^0::/run$' /proc/self/cgroup; mkdir \"$root/child\"; printf '33554432\\n' > \"$root/child/memory.max\"; test \"$(cat \"$root/child/memory.max\")\" = 33554432; rmdir \"$root/child\"; cc main.c -o program; ./program; printf nested-build-ok > proof",
        secret = root.path().join("hidden-secret").display(),
        sdk = sdk.display(),
    );
    let sent = Command::new("tmux")
        .args([
            "-L",
            &socket,
            "send-keys",
            "-t",
            "=sleeve-nested-app:",
            &command,
            "Enter",
        ])
        .status()
        .unwrap();
    assert!(sent.success(), "could not send work to nested sleeve PTY");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !app.join("proof").is_file() {
        let captured = manager.capture("sleeve-nested-app", 60).unwrap();
        assert!(
            Instant::now() < deadline,
            "nested build did not finish: {captured:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        fs::read_to_string(app.join("proof")).unwrap(),
        "nested-build-ok"
    );
    assert_eq!(
        fs::read_to_string(lower.join("shared")).unwrap(),
        "published-original"
    );
    assert_eq!(
        fs::read_to_string(sdk_cache.join("shared")).unwrap(),
        "host-original"
    );
    assert_eq!(
        fs::read_to_string(state.join("sleeves/nested-app/overlays/flutter/upper/shared")).unwrap(),
        "private-copy"
    );
    assert!(!lib.join("forbidden").exists());

    let (status, killed) = call(addr, "KillSleeve", json!({"name":"nested-app"})).await;
    assert_eq!(status, 200, "{killed}");
    let (status, row) = call(addr, "GetSleeve", json!({"name":"nested-app"})).await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["sleeve"]["state"], "SLEEVE_STATE_EXITED");
    assert!(!state
        .join("sleeves/nested-app/home/.protectorate-token")
        .exists());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let active = Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", &unit])
            .status()
            .unwrap();
        let staging_empty = fs::read_dir(&runtime).unwrap().next().is_none();
        if !active.success() && staging_empty {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "nested sleeve cleanup timed out: scope active={}, host staging empty={}",
            active.success(),
            staging_empty
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    server.abort();
    let _ = server.await;
}
