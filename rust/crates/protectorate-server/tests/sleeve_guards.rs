//! A real hub process runs a real terminal sleeve: its egress is confined
//! to the proxy and audited, and its workspace checkpoints round-trip.
#![cfg(target_os = "linux")]
use protectorate_connect_client::Client;
use serde_json::{json, Value};
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const SLEEVE: &str = "/protectorate.v1.SleeveService/";

struct Hub(Child, String);
impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = Command::new("tmux")
            .args(["-L", &self.1, "kill-server"])
            .stderr(Stdio::null())
            .status();
    }
}

fn git(workspace: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn start(root: &Path, extra: &[&str]) -> (Hub, Client, String) {
    let home = root.join("hub");
    fs::create_dir_all(&home).unwrap();
    let token_file = root.join("owner-token");
    let token = "sleeve-guards-owner-token";
    fs::write(&token_file, token).unwrap();
    fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
    let listen = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let tmux = format!("p-rust-guards-{}", std::process::id());
    let log = fs::File::create(root.join("hub.stderr")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_protectorate-rs"))
        .arg("serve")
        .arg("--home")
        .arg(&home)
        .arg("--owner-token-file")
        .arg(&token_file)
        .args(["--listen", &listen.to_string()])
        .args(["--release-api-url", "http://127.0.0.1:1"])
        .args(["--tmux-socket", &tmux])
        .args(extra)
        .env("HOME", root)
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .env_remove("PROTECTORATE_HUB")
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap();
    let mut hub = Hub(child, tmux);
    let url = format!("http://{listen}");
    let client = Client::new(&url, token, Duration::from_secs(60)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while client
        .call("/protectorate.v1.HubService/GetStatus", &json!({}))
        .is_err()
    {
        assert!(
            hub.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "hub did not become ready: {}",
            fs::read_to_string(root.join("hub.stderr")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(100));
    }
    (hub, client, listen.port().to_string())
}

fn spawn_shell(client: &Client, name: &str, workspace: &Path, script: &str) -> Value {
    client
        .call(
            &format!("{SLEEVE}SpawnSleeve"),
            &json!({
                "name": name,
                "harness": "HARNESS_SHELL",
                "mode": "SLEEVE_MODE_TERMINAL",
                "workspace": workspace,
                "args": ["/usr/bin/sh", "-c", script],
            }),
        )
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"))
}

fn wait_for(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "{} did not appear",
            path.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(100));
    fs::read_to_string(path).unwrap()
}

const EGRESS_PROBE: &str = r#"
import os, socket
proxy = int(os.environ["HTTPS_PROXY"].rsplit(":", 1)[1])
def tunnel(target):
    s = socket.create_connection(("127.0.0.1", proxy), timeout=20)
    s.sendall(f"CONNECT {target} HTTP/1.1\r\n\r\n".encode())
    return s.makefile("rb").readline().split(b" ")[1].decode()
results = [tunnel(t) for t in ("not-listed.test:443", "10.1.2.3:22", "127.0.0.1:" + os.environ["HUB_PORT"])]
try:
    socket.create_connection(("1.1.1.1", 443), timeout=3)
    results.append("direct")
except OSError:
    results.append("no-route")
hub = socket.create_connection(("127.0.0.1", int(os.environ["HUB_PORT"])), timeout=5)
hub.sendall(b"GET / HTTP/1.0\r\n\r\n")
results.append("hub-" + hub.makefile("rb").readline().decode().split("/")[0])
open("egress.txt.tmp", "w").write(" ".join(results))
os.rename("egress.txt.tmp", "egress.txt")
"#;

#[test]
fn sleeve_egress_is_proxied_and_audited() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("egress-ws");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(workspace.join("probe.py"), EGRESS_PROBE).unwrap();
    let (_hub, client, port) = start(root.path(), &["--egress-allow", "*.example.invalid"]);
    spawn_shell(
        &client,
        "egress",
        &workspace,
        &format!("HUB_PORT={port} /usr/bin/python3 probe.py; exec sleep 600"),
    );
    let results = wait_for(&workspace.join("egress.txt"));
    assert_eq!(results, "403 403 200 no-route hub-HTTP", "{results}");
    let audit = client
        .call(
            "/protectorate.v1.AuditService/ListAuditEntries",
            &json!({"procedure": "egress", "limit": 50}),
        )
        .unwrap();
    let entries = audit["entries"].as_array().cloned().unwrap_or_default();
    let outcome = |subject: &str| {
        entries
            .iter()
            .find(|e| e["subject"] == subject && e["actor"] == "sleeve:egress")
            .map(|e| e["outcome"].as_str().unwrap_or_default().to_owned())
    };
    assert_eq!(
        outcome("not-listed.test:443").as_deref(),
        Some("denied"),
        "{entries:?}"
    );
    assert_eq!(outcome("10.1.2.3:22").as_deref(), Some("denied"));
    assert_eq!(outcome(&format!("127.0.0.1:{port}")).as_deref(), Some("ok"));
}

#[test]
fn checkpoints_round_trip_on_a_real_workspace() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("ckpt-ws");
    fs::create_dir_all(&workspace).unwrap();
    git(&workspace, &["init", "-q"]);
    fs::write(workspace.join(".gitignore"), "build/\n").unwrap();
    fs::write(workspace.join("tracked.txt"), "one\n").unwrap();
    git(&workspace, &["add", "."]);
    git(&workspace, &["commit", "-qm", "base"]);
    fs::write(workspace.join("untracked.txt"), "draft\n").unwrap();
    fs::create_dir_all(workspace.join("build")).unwrap();
    fs::write(workspace.join("build/out.bin"), "ignored\n").unwrap();
    let (_hub, client, _) = start(root.path(), &[]);
    spawn_shell(&client, "ckpt", &workspace, "exec sleep 600");
    let call = |method: &str, body: Value| {
        client
            .call(&format!("{SLEEVE}{method}"), &body)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    };

    let first = call("CreateCheckpoint", json!({"name": "ckpt"}));
    assert_eq!(first["checkpoint"]["id"], "1", "{first}");
    let again = call("CreateCheckpoint", json!({"name": "ckpt"}));
    assert_eq!(again["unchanged"], true, "{again}");

    fs::write(workspace.join("tracked.txt"), "two\n").unwrap();
    fs::remove_file(workspace.join("untracked.txt")).unwrap();
    fs::write(workspace.join("added.txt"), "new\n").unwrap();
    let second = call("CreateCheckpoint", json!({"name": "ckpt"}));
    assert_eq!(second["checkpoint"]["id"], "2", "{second}");
    let list = call("ListCheckpoints", json!({"name": "ckpt"}));
    let ids = list["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["1", "2"], "{list}");

    let diff = call(
        "DiffCheckpoint",
        json!({"name": "ckpt", "fromId": "1", "toId": "2"}),
    );
    let text = diff["diff"].as_str().unwrap();
    for needle in ["-one", "+two", "deleted file mode", "untracked.txt", "+new"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    assert!(
        !text.contains("out.bin"),
        "ignored file entered a checkpoint"
    );
    fs::write(workspace.join("tracked.txt"), "three\n").unwrap();
    let live = call("DiffCheckpoint", json!({"name": "ckpt", "fromId": "2"}));
    assert!(live["diff"].as_str().unwrap().contains("+three"), "{live}");

    let restored = call("RestoreCheckpoint", json!({"name": "ckpt", "id": "1"}));
    assert_eq!(restored["before"]["id"], "3", "{restored}");
    assert_eq!(
        fs::read_to_string(workspace.join("tracked.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("untracked.txt")).unwrap(),
        "draft\n"
    );
    assert!(!workspace.join("added.txt").exists());
    assert_eq!(
        fs::read_to_string(workspace.join("build/out.bin")).unwrap(),
        "ignored\n"
    );
    assert!(
        git(&workspace, &["for-each-ref"])
            .lines()
            .all(|r| !r.contains("protectorate")),
        "checkpoint refs leaked into the workspace repository"
    );
    let undo = call("RestoreCheckpoint", json!({"name": "ckpt", "id": "3"}));
    assert_eq!(undo["before"]["id"], "4", "{undo}");
    assert_eq!(
        fs::read_to_string(workspace.join("tracked.txt")).unwrap(),
        "three\n"
    );
    assert!(workspace.join("added.txt").exists());

    let missing = client.call(
        &format!("{SLEEVE}RestoreCheckpoint"),
        &json!({"name": "ckpt", "id": "99"}),
    );
    assert!(missing.is_err());
}
