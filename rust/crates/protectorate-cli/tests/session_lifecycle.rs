#![cfg(target_os = "linux")]
mod common;

use common::{cli, PrivateTmux};
use protectorate_hub::Hub;
use protectorate_session::Manager;
use serde_json::Value;
use std::{process::Command, sync::Arc, time::SystemTime, time::UNIX_EPOCH};
use tokio::net::TcpListener;

fn session_row<'a>(listing: &'a Value, name: &str) -> Option<&'a Value> {
    listing
        .get("sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|session| session["name"] == name)
}

/// Proto3 JSON leaves a zero out of the reply.
fn integer(row: &Value, key: &str) -> i64 {
    row.get(key).and_then(Value::as_i64).unwrap_or_default()
}

/// Waits for the hub to report `name` as exited, and returns the listing that
/// first said so.
fn exited_listing(call: &dyn Fn(&[&str]) -> Result<String, String>, name: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        let listing: Value =
            serde_json::from_str(&call(&["--json", "session", "ls"]).unwrap()).unwrap();
        if session_row(&listing, name).is_some_and(|s| s["state"] == "SESSION_STATE_EXITED") {
            return listing;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{name} never exited");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_commands_drive_a_real_hub_and_a_private_tmux_server() {
    if Command::new("tmux").arg("-V").output().is_err() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let socket = format!("protectorate-cli-sessions-{}-{nonce}", std::process::id());
    let _tmux = PrivateTmux(socket.clone());
    let manager = Arc::new(Manager::new(&socket));
    let hub = Hub::open(root.path(), "owner-secret", "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });

    let cwd = root.path().to_string_lossy().into_owned();
    let outcome = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| cli(&url, "owner-secret", args);
        assert_eq!(
            call(&["session", "new", "runner", "--cwd", &cwd, "--", "/bin/sh"]).unwrap(),
            "created\trunner\n"
        );
        assert!(call(&["session", "new", "runner"]).is_err());
        assert!(call(&["session", "new", "sleeve-taken"]).is_err());
        let listed = call(&["session", "ls"]).unwrap();
        assert!(listed.contains("runner"), "{listed}");
        assert!(listed.contains("running"), "{listed}");
        let listing: Value =
            serde_json::from_str(&call(&["--json", "session", "ls"]).unwrap()).unwrap();
        let runner = session_row(&listing, "runner").expect("runner is listed");
        assert_eq!(runner["cwd"], cwd.as_str());
        assert_eq!(runner["command"], Value::Array(vec!["/bin/sh".into()]));
        assert_eq!(runner["state"], "SESSION_STATE_RUNNING");
        std::thread::sleep(std::time::Duration::from_millis(400));
        call(&["session", "capture", "runner", "--lines", "20"]).unwrap();
        assert!(call(&["session", "respawn", "runner"]).is_err());

        // The printer stays alive until its output is on the pane: tmux
        // drops pty output still in flight when a pane's process exits.
        let release = format!("{cwd}/release-printer");
        let printed = call(&[
            "session",
            "new",
            "printer",
            "--cwd",
            &cwd,
            "--env",
            "GREETING=hello",
            "--",
            "/bin/sh",
            "-c",
            "env; while [ ! -e release-printer ]; do sleep 0.05; done",
        ]);
        assert_eq!(printed.unwrap(), "created\tprinter\n");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            let pane = call(&["session", "capture", "printer", "--lines", "200"]).unwrap();
            if pane.contains("GREETING=hello") && pane.contains(&format!("PWD={cwd}")) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{pane}");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        std::fs::write(&release, "").unwrap();
        let printed_listing = exited_listing(&call, "printer");
        assert_eq!(
            integer(
                session_row(&printed_listing, "printer").unwrap(),
                "exitCode"
            ),
            0
        );

        assert_eq!(
            call(&["session", "new", "crash", "--", "/bin/false"]).unwrap(),
            "created\tcrash\n"
        );
        let crashed_listing = exited_listing(&call, "crash");
        assert_eq!(
            integer(session_row(&crashed_listing, "crash").unwrap(), "exitCode"),
            1
        );
        assert_eq!(
            call(&["session", "respawn", "crash"]).unwrap(),
            "respawned\tcrash\n"
        );

        assert_eq!(
            call(&["session", "kill", "runner"]).unwrap(),
            "killed\trunner\n"
        );
        assert!(call(&["session", "capture", "runner"]).is_err());
        assert!(call(&["session", "capture", "never-existed"]).is_err());
        let bad_name = call(&["session", "new", "a/b", "--", "sh"]).unwrap_err();
        assert!(bad_name.contains("1-64"), "{bad_name}");

        // The hub keeps session new, respawn and kill owner-only; a read token
        // still gets the two NO_SIDE_EFFECTS calls.
        let issued: Value = serde_json::from_str(
            &call(&["--json", "token", "create", "viewer", "--scope", "read"]).unwrap(),
        )
        .unwrap();
        let secret = issued["secret"].as_str().unwrap();
        let scoped = |args: &[&str]| cli(&url, secret, args);
        scoped(&["session", "ls"]).unwrap();
        scoped(&["session", "capture", "printer"]).unwrap();
        for args in [
            ["session", "new", "ghost"],
            ["session", "respawn", "crash"],
            ["session", "kill", "crash"],
        ] {
            assert!(scoped(&args).is_err(), "{args:?}");
        }
    })
    .await;
    server.abort();
    outcome.unwrap();
}
