#![cfg(unix)]
use protectorate_node::{Agent, MetricsCollector, UpdateRestart};
use protectorate_node_metrics::NodeMetrics;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "node".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            agent_version: "v1".into(),
            ..NodeMetrics::default()
        })
    }
}

fn write_binary(path: &Path, version: &str) -> Vec<u8> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let script = format!("#!/bin/sh\nprintf '%s\\n' '{{\"version\":\"{version}\",\"os\":\"{}\",\"arch\":\"{arch}\"}}'\n", std::env::consts::OS);
    fs::write(path, script.as_bytes()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    script.into_bytes()
}

async fn call(addr: std::net::SocketAddr, path: &str, body: Value) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = body.to_string();
    let req = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer node-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

#[tokio::test]
async fn apply_update_checks_digest_platform_and_records_trial_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    let exe = dir.path().join("protectorate-rs");
    write_binary(&exe, "v1");
    let staged = root.join("next");
    let bytes = write_binary(&staged, "v2");
    let restart_count = Arc::new(AtomicUsize::new(0));
    let counts = Arc::clone(&restart_count);
    let restart: UpdateRestart = Arc::new(move || {
        counts.fetch_add(1, Ordering::SeqCst);
    });
    let agent = Agent::new(Arc::new(Metrics))
        .with_files(&root, false)
        .unwrap()
        .with_update(&exe, "v1", restart)
        .unwrap();
    assert!(!agent.start_update_trial().unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    let path = "/protectorate.v1.NodeAgentService/ApplyUpdate";
    let mut request = json!({"path":"next","size":bytes.len().to_string(),
        "sha256":format!("{:x}",Sha256::digest(&bytes))});
    let (code, err) = call(
        addr,
        path,
        json!({"path":"../protectorate-rs","size":"0","sha256":"0".repeat(64)}),
    )
    .await;
    assert_eq!(code, 403, "{err}");
    request["sha256"] = json!("0".repeat(64));
    let (code, err) = call(addr, path, request.clone()).await;
    assert_eq!(code, 400, "{err}");
    assert_eq!(
        fs::read(&exe).unwrap(),
        write_binary(&dir.path().join("original"), "v1")
    );
    request["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    request["expectedVersion"] = json!("v3");
    let (code, err) = call(addr, path, request.clone()).await;
    assert_eq!(code, 400, "{err}");
    assert_eq!(
        fs::read(&exe).unwrap(),
        fs::read(dir.path().join("original")).unwrap()
    );
    request["expectedVersion"] = json!("v2");
    let (code, result) = call(addr, path, request).await;
    assert_eq!(code, 200, "{result}");
    assert_eq!(result["version"], "v2");
    assert_eq!(result["previousVersion"], "v1");
    assert_eq!(result["restart"], "re-exec");
    assert_eq!(fs::read(&exe).unwrap(), bytes);
    assert!(dir.path().join("protectorate-rs.old").exists());
    let marker: Value =
        serde_json::from_slice(&fs::read(dir.path().join("protectorate-rs.update")).unwrap())
            .unwrap();
    assert_eq!(marker["phase"], "trial");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(restart_count.load(Ordering::SeqCst), 1);
    server.abort();

    let trial = Agent::new(Arc::new(Metrics))
        .with_files(&root, false)
        .unwrap()
        .with_update(&exe, "v2", Arc::new(|| {}))
        .unwrap();
    assert!(!trial.start_update_trial().unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trial_addr = listener.local_addr().unwrap();
    let trial_server = tokio::spawn(async move {
        axum::serve(listener, trial.router().unwrap())
            .await
            .unwrap()
    });
    let (code, metrics) = call(
        trial_addr,
        "/protectorate.v1.NodeAgentService/GetMetrics",
        json!({}),
    )
    .await;
    assert_eq!(code, 200, "{metrics}");
    let marker: Value =
        serde_json::from_slice(&fs::read(dir.path().join("protectorate-rs.update")).unwrap())
            .unwrap();
    assert_eq!(
        marker["phase"], "trial",
        "a mismatched metrics version cannot confirm the trial"
    );
    trial_server.abort();
    let retried = Agent::new(Arc::new(Metrics))
        .with_files(&root, false)
        .unwrap()
        .with_update(&exe, "v2", Arc::new(|| {}))
        .unwrap();
    assert!(retried.start_update_trial().unwrap());
    let rolled_back: Value =
        serde_json::from_slice(&fs::read(dir.path().join("protectorate-rs.update")).unwrap())
            .unwrap();
    assert_eq!(rolled_back["phase"], "rolledback");
    assert_eq!(
        fs::read(&exe).unwrap(),
        fs::read(dir.path().join("original")).unwrap()
    );
}
