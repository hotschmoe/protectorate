#![cfg(unix)]
#![cfg(target_os = "linux")]
use protectorate_cli::{dispatch_endpoint, Code};
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
fn exited_listing(
    call: &dyn Fn(&[&str]) -> Result<String, protectorate_cli::RpcError>,
    name: &str,
) -> Value {
    for _ in 0..100 {
        let listing: Value =
            serde_json::from_str(&call(&["--json", "session", "ls"]).unwrap()).unwrap();
        if session_row(&listing, name).is_some_and(|s| s["state"] == "SESSION_STATE_EXITED") {
            return listing;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{name} never exited");
}

#[tokio::test]
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
        let call = |args: &[&str]| dispatch_endpoint(&url, "owner-secret", args);
        let created = call(&[
            "session",
            "new",
            "runner",
            "--cwd",
            cwd.as_str(),
            "--",
            "/bin/sh",
        ]);
        let duplicate = call(&["session", "new", "runner"]);
        let reserved = call(&["session", "new", "sleeve-taken"]);
        let listed = call(&["session", "ls"]);
        let json_list = call(&["--json", "session", "ls"]);
        std::thread::sleep(std::time::Duration::from_millis(400));
        let capture = call(&["session", "capture", "runner", "--lines", "20"]);
        let respawn_running = call(&["session", "respawn", "runner"]);

        let printed = call(&[
            "session",
            "new",
            "printer",
            "--cwd",
            cwd.as_str(),
            "--env",
            "GREETING=hello",
            "--",
            "env",
        ]);
        let printed_listing = exited_listing(&call, "printer");
        let capture_printed = call(&["session", "capture", "printer", "--lines", "200"]);

        let crashed = call(&["session", "new", "crash", "--", "/bin/false"]);
        let crashed_listing = exited_listing(&call, "crash");
        let respawned = call(&["session", "respawn", "crash"]);

        let killed = call(&["session", "kill", "runner"]);
        let capture_gone = call(&["session", "capture", "runner"]);
        let capture_missing = call(&["session", "capture", "never-existed"]);

        let issued: Value = serde_json::from_str(
            &call(&["--json", "token", "create", "viewer", "--scope", "read"]).unwrap(),
        )
        .unwrap();
        let secret = issued["secret"].as_str().unwrap().to_owned();
        let scoped_list = dispatch_endpoint(&url, &secret, ["session", "ls"]);
        let scoped_capture = dispatch_endpoint(&url, &secret, ["session", "capture", "printer"]);
        let scoped_new = dispatch_endpoint(&url, &secret, ["session", "new", "ghost"]);
        let scoped_respawn = dispatch_endpoint(&url, &secret, ["session", "respawn", "crash"]);
        let scoped_kill = dispatch_endpoint(&url, &secret, ["session", "kill", "crash"]);

        (
            created,
            duplicate,
            reserved,
            listed,
            json_list,
            capture,
            respawn_running,
            printed,
            printed_listing,
            capture_printed,
            crashed,
            crashed_listing,
            respawned,
            killed,
            capture_gone,
            capture_missing,
            scoped_list,
            scoped_capture,
            scoped_new,
            scoped_respawn,
            scoped_kill,
        )
    })
    .await
    .unwrap();
    for name in ["runner", "printer", "crash", "ghost"] {
        let _ = manager.kill(name);
    }
    let _ = Command::new("tmux")
        .args(["-L", &socket, "kill-server"])
        .output();
    server.abort();

    let (
        created,
        duplicate,
        reserved,
        listed,
        json_list,
        capture,
        respawn_running,
        printed,
        printed_listing,
        capture_printed,
        crashed,
        crashed_listing,
        respawned,
        killed,
        capture_gone,
        capture_missing,
        scoped_list,
        scoped_capture,
        scoped_new,
        scoped_respawn,
        scoped_kill,
    ) = outcome;

    let created = created.unwrap();
    assert_eq!(created, "created\trunner\n");
    assert_eq!(duplicate.unwrap_err().code, Code::AlreadyExists);
    assert_eq!(reserved.unwrap_err().code, Code::FailedPrecondition);
    let listed = listed.unwrap();
    assert!(listed.contains("runner"), "{listed}");
    assert!(listed.contains("running"), "{listed}");

    let listing: Value = serde_json::from_str(&json_list.unwrap()).unwrap();
    let runner = session_row(&listing, "runner").expect("runner is listed");
    assert_eq!(runner["cwd"], root.path().to_string_lossy().as_ref());
    assert_eq!(runner["command"], Value::Array(vec!["/bin/sh".into()]));
    assert_eq!(runner["state"], "SESSION_STATE_RUNNING");
    assert!(capture.is_ok(), "{capture:?}");
    assert_eq!(respawn_running.unwrap_err().code, Code::FailedPrecondition);

    let printed = printed.unwrap();
    assert_eq!(printed, "created\tprinter\n");
    assert_eq!(
        integer(
            session_row(&printed_listing, "printer").unwrap(),
            "exitCode"
        ),
        0
    );
    let pane = capture_printed.unwrap();
    assert!(pane.contains("GREETING=hello"), "{pane}");
    assert!(
        pane.contains(root.path().to_string_lossy().as_ref()),
        "{pane}"
    );

    let crashed = crashed.unwrap();
    assert_eq!(crashed, "created\tcrash\n");
    assert_eq!(
        integer(session_row(&crashed_listing, "crash").unwrap(), "exitCode"),
        1
    );
    let respawned = respawned.unwrap();
    assert_eq!(respawned, "respawned\tcrash\n");

    let killed = killed.unwrap();
    assert_eq!(killed, "killed\trunner\n");
    assert_eq!(capture_gone.unwrap_err().code, Code::NotFound);
    assert_eq!(capture_missing.unwrap_err().code, Code::NotFound);

    // The hub keeps session new, respawn and kill owner-only; a read token
    // still gets the two NO_SIDE_EFFECTS calls.
    assert!(scoped_list.is_ok(), "{scoped_list:?}");
    assert!(scoped_capture.is_ok(), "{scoped_capture:?}");
    for denied in [scoped_new, scoped_respawn, scoped_kill] {
        assert_eq!(denied.unwrap_err().code, Code::PermissionDenied);
    }
}

#[test]
fn session_names_are_rejected_before_any_request() {
    for args in [
        ["session", "attach", "not a name"].as_slice(),
        &["session", "new", "a/b", "--", "sh"],
        &["session", "capture", "x".repeat(65).as_str()],
    ] {
        let error = dispatch_endpoint("http://127.0.0.1:1", "owner-secret", args).unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{args:?}");
        assert!(error.message.contains("1-64"), "{error}");
    }
}
