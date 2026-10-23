#![cfg(windows)]

use protectorate_connect_client::{Client, NodeTls};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::{SocketAddr, TcpListener},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const METRICS: &str = "/protectorate.v1.NodeAgentService/GetMetrics";
const APPLY: &str = "/protectorate.v1.NodeAgentService/ApplyUpdate";

struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What `node join` leaves in a home, issued by a CA this test owns, plus
/// the client side of the hub's mutual TLS.
struct Joined {
    server_name: String,
    address: String,
    tls: NodeTls,
}

impl Joined {
    fn create(home: &Path, address: SocketAddr) -> Self {
        let ca = protectorate_tls::Ca::open_or_create(&home.join("hub-ca")).unwrap();
        let (key_pem, public_key, _) = protectorate_tls::node_key(b"join").unwrap();
        let (name, id) = ("guardian", "cert1");
        let (cert, _) = ca.issue_node(name, id, &public_key).unwrap();
        let tls_dir = home.join("tls");
        fs::create_dir_all(&tls_dir).unwrap();
        fs::write(tls_dir.join("ca.pem"), ca.pem()).unwrap();
        fs::write(tls_dir.join("node-key.pem"), &key_pem).unwrap();
        fs::write(tls_dir.join("node.pem"), &cert).unwrap();
        fs::write(
            home.join("node.json"),
            json!({"hub": "https://127.0.0.1:1", "name": name, "address": address}).to_string(),
        )
        .unwrap();
        let hub = ca.hub_identity(&[]).unwrap();
        Self {
            server_name: protectorate_tls::node_name(name, id),
            address: address.to_string(),
            tls: NodeTls::from_pem(
                ca.pem().as_bytes(),
                hub.cert_pem.as_bytes(),
                hub.key_pem.as_bytes(),
            )
            .unwrap(),
        }
    }

    fn call(&self, method: &str, input: Value) -> Result<Value, String> {
        Client::new_node(
            &self.server_name,
            &self.address,
            Some(&self.tls),
            Duration::from_secs(2),
        )
        .and_then(|client| client.call(method, &input))
        .map_err(|error| error.to_string())
    }
}

fn wait_metrics(node: &Joined, phase: Option<&str>, guard: &mut Guard) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    while Instant::now() < deadline {
        assert!(
            guard.0.try_wait().unwrap().is_none(),
            "guardian exited: {last}"
        );
        match node.call(METRICS, json!({})) {
            Ok(value) if phase.is_none_or(|phase| value["update"]["phase"] == phase) => {
                return value;
            }
            Ok(value) => last = value.to_string(),
            Err(error) => last = error,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("node did not report phase {phase:?}: {last}");
}

fn apply(node: &Joined, stage: &Path, name: &str, version: &str) -> Value {
    let bytes = fs::read(stage).unwrap();
    node.call(
        APPLY,
        json!({"path":name,"size":bytes.len().to_string(),
            "sha256":format!("{:x}", Sha256::digest(&bytes)), "expectedVersion":version}),
    )
    .unwrap()
}

#[test]
fn update_keeps_task_guardian_and_recovers_failing_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_BIN_EXE_protectorate-rs-windows"));
    let guardian = temp.path().join("protectorate-rs-guardian.exe");
    let binary = temp.path().join("protectorate-rs.exe");
    fs::copy(source, &guardian).unwrap();
    fs::copy(source, &binary).unwrap();
    let home = temp.path().join("state");
    let files = home.join("node-files");
    fs::create_dir_all(&files).unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let node = Joined::create(&home, addr);
    let child = Command::new(&guardian)
        .args(["node", "serve", "--home"])
        .arg(&home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut guard = Guard(child);
    let original = fs::read(&binary).unwrap();
    let version = wait_metrics(&node, None, &mut guard)["agentVersion"]
        .as_str()
        .unwrap()
        .to_owned();

    let good = files.join("good.exe");
    fs::copy(source, &good).unwrap();
    let reply = apply(&node, &good, "good.exe", &version);
    assert_eq!(reply["version"], version);
    let confirmed = wait_metrics(&node, Some("confirmed"), &mut guard);
    assert_eq!(confirmed["update"]["attempts"], 1);
    assert_eq!(fs::read(&binary).unwrap(), original);

    let failing = files.join("failing.exe");
    let code = temp.path().join("failing.rs");
    fs::write(
        &code,
        format!(
            "fn main() {{ if std::env::args().nth(1).as_deref() == Some(\"version\") {{ println!(\"{{}}\", r#\"{{\"version\":\"{version}\",\"os\":\"windows\",\"arch\":\"amd64\"}}\"#); }} else {{ std::process::exit(23); }} }}"
        ),
    )
    .unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2021"])
        .arg(&code)
        .arg("-o")
        .arg(&failing)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    apply(&node, &failing, "failing.exe", &version);
    let rolled_back = wait_metrics(&node, Some("rolledback"), &mut guard);
    assert_eq!(rolled_back["agentVersion"], version);
    assert_eq!(fs::read(&binary).unwrap(), original);
    assert!(guard.0.try_wait().unwrap().is_none());
}
