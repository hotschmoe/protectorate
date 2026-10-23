#![cfg(windows)]

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
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

fn call(addr: SocketAddr, method: &str, input: Value) -> io::Result<(u16, Value)> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let body = input.to_string();
    write!(
        stream,
        "POST {method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let text = String::from_utf8(bytes).map_err(io::Error::other)?;
    let (headers, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| io::Error::other("response has no body separator"))?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::other("response has no status"))?
        .parse()
        .map_err(io::Error::other)?;
    Ok((status, serde_json::from_str(body)?))
}

fn wait_metrics(addr: SocketAddr, phase: Option<&str>, guard: &mut Guard) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    while Instant::now() < deadline {
        assert!(
            guard.0.try_wait().unwrap().is_none(),
            "guardian exited: {last}"
        );
        match call(addr, METRICS, json!({})) {
            Ok((200, value)) if phase.is_none_or(|phase| value["update"]["phase"] == phase) => {
                return value;
            }
            Ok((status, value)) => last = format!("status {status}: {value}"),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("node did not report phase {phase:?}: {last}");
}

fn apply(addr: SocketAddr, stage: &Path, name: &str, version: &str) -> Value {
    let bytes = fs::read(stage).unwrap();
    let (status, value) = call(
        addr,
        APPLY,
        json!({"path":name,"size":bytes.len().to_string(),
            "sha256":format!("{:x}", Sha256::digest(&bytes)), "expectedVersion":version}),
    )
    .unwrap();
    assert_eq!(status, 200, "{value}");
    value
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
    fs::write(home.join("agent-token"), "node-secret").unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let child = Command::new(&guardian)
        .args(["node", "serve", "--home"])
        .arg(&home)
        .arg("--agent-token-file")
        .arg(home.join("agent-token"))
        .arg("--listen")
        .arg(addr.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut guard = Guard(child);
    let original = fs::read(&binary).unwrap();
    let version = wait_metrics(addr, None, &mut guard)["agentVersion"]
        .as_str()
        .unwrap()
        .to_owned();

    let good = files.join("good.exe");
    fs::copy(source, &good).unwrap();
    let reply = apply(addr, &good, "good.exe", &version);
    assert_eq!(reply["version"], version);
    let confirmed = wait_metrics(addr, Some("confirmed"), &mut guard);
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
    apply(addr, &failing, "failing.exe", &version);
    let rolled_back = wait_metrics(addr, Some("rolledback"), &mut guard);
    assert_eq!(rolled_back["agentVersion"], version);
    assert_eq!(fs::read(&binary).unwrap(), original);
    assert!(guard.0.try_wait().unwrap().is_none());
}
