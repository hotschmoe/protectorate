#![cfg(unix)]
use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector, UpdateRestart};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use std::{fs, io, os::unix::fs::PermissionsExt, path::Path, sync::Arc, time::Duration};
use tokio::net::TcpListener;

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            agent_version: "v1".into(),
            ..NodeMetrics::default()
        })
    }
}
fn write_binary(path: &Path, version: &str) {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let script=format!("#!/bin/sh\nprintf '%s\\n' '{{\"version\":\"{version}\",\"os\":\"{}\",\"arch\":\"{arch}\"}}'\n", std::env::consts::OS);
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}
async fn update(url: String, name: &str, force: bool) -> Result<Vec<Value>, RpcError> {
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || {
        let client = Client::new(&url, "owner-secret", Duration::from_secs(10))?;
        let mut events = Vec::new();
        client.stream(
            "/protectorate.v1.NodeService/UpdateNode",
            &json!({"name":name,"force":force}),
            |event| {
                events.push(event);
                Ok(())
            },
        )?;
        Ok(events)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn update_node_skips_and_never_confirms_without_guardian_restart() {
    let temp = tempfile::tempdir().unwrap();
    let node_root = temp.path().join("node-files");
    let exe = temp.path().join("node-exe");
    write_binary(&exe, "v1");
    let restart: UpdateRestart = Arc::new(|| {});
    let agent = Agent::new(Arc::new(Metrics), "node-secret")
        .with_files(&node_root, false)
        .unwrap()
        .with_update(&exe, "v1", restart)
        .unwrap();
    let (node_url, node_task) = serve(agent.router().unwrap()).await;
    let bins = temp.path().join("bins");
    fs::create_dir(&bins).unwrap();
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let binary = bins.join(format!("protectorate-rs-{}-{arch}", std::env::consts::OS));
    write_binary(&binary, "v1");
    let hub = Hub::open(temp.path().join("hub"), "owner-secret", "v1").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_url.trim_start_matches("http://").into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub
        .with_node_token("node-secret".into())
        .unwrap()
        .with_node_binary_dir(&bins)
        .with_node_update_timing(Duration::from_secs(5), Duration::from_millis(20));
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;
    let skipped = update(hub_url.clone(), "PEER", false).await.unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["phase"], "UPDATE_PHASE_SKIPPED");
    assert!(!node_root.join("updates/protectorate-rs-next").exists());
    write_binary(&binary, "v2");
    let mismatch = update(hub_url.clone(), "peer", true).await.unwrap_err();
    assert_eq!(mismatch.code, Code::FailedPrecondition);
    assert!(!std::path::PathBuf::from(format!("{}.update", exe.display())).exists());
    write_binary(&binary, "v1");
    let unconfirmed = update(hub_url.clone(), "peer", true).await.unwrap_err();
    assert_eq!(unconfirmed.code, Code::DeadlineExceeded);
    assert!(unconfirmed.message.contains("did not confirm"));
    assert_eq!(fs::read(&exe).unwrap(), fs::read(&binary).unwrap());
    let marker: Value =
        serde_json::from_slice(&fs::read(format!("{}.update", exe.display())).unwrap()).unwrap();
    assert_eq!(marker["phase"], "trial");
    let unknown = update(hub_url.clone(), "missing", false).await.unwrap_err();
    assert_eq!(unknown.code, Code::NotFound);
    hub_task.abort();
    node_task.abort();
}
