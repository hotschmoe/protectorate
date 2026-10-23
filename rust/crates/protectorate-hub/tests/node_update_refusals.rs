#![cfg(target_os = "linux")]
//! A node never swaps in a binary that fails its digest or platform checks,
//! whether the hub pushes it or a caller asks for an apply directly.
use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector, UpdateRestart};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::Code;
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

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn version_script(os: &str, version: &str) -> String {
    format!(
        "#!/bin/sh\nprintf '%s\\n' '{{\"version\":\"{version}\",\"os\":\"{os}\",\"arch\":\"{}\"}}'\n",
        arch()
    )
}

async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_refuses_binaries_with_wrong_digest_or_platform() {
    let temp = tempfile::tempdir().unwrap();
    let node_root = temp.path().join("node-files");
    let exe = temp.path().join("node-exe");
    let installed = version_script(std::env::consts::OS, "v1");
    executable(&exe, &installed);
    let restarts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&restarts);
    let restart: UpdateRestart = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let agent = Agent::new(Arc::new(Metrics))
        .with_files(&node_root, false)
        .unwrap()
        .with_update(&exe, "v1", restart)
        .unwrap();
    let (node_url, node_task) = serve(agent.router().unwrap()).await;
    let plain = temp.path().join("plain");
    let bundle = temp.path().join("bundle");
    fs::create_dir(&plain).unwrap();
    fs::create_dir(&bundle).unwrap();
    let server_name = format!("protectorate-rs-{}-{}", std::env::consts::OS, arch());
    executable(&plain.join(&server_name), &version_script("plan9", "v2"));
    let mut sums = String::new();
    for component in protectorate_release::RUST_BUNDLE {
        let name = protectorate_release::rust_bundle_name(component, "linux", arch()).unwrap();
        let body = version_script("linux", "v2");
        sums.push_str(&format!("{:x}  {name}\n", Sha256::digest(body.as_bytes())));
        executable(&bundle.join(&name), &body);
    }
    fs::write(bundle.join("SHA256SUMS"), sums).unwrap();
    fs::write(bundle.join("VERSION"), "v2\n").unwrap();
    executable(&bundle.join(&server_name), "#!/bin/sh\necho tampered\n");

    for (dir, expected) in [
        (&plain, "binary platform does not match this node"),
        (&bundle, "checksum mismatch"),
    ] {
        let hub = Hub::open(temp.path().join("hub"), "owner-secret", "v2").unwrap();
        if hub.inventory.get("peer").is_err() {
            hub.inventory
                .add(
                    Node {
                        name: "peer".into(),
                        address: node_url.trim_start_matches("http://").into(),
                        ..Node::default()
                    },
                    false,
                )
                .unwrap();
        }
        let hub = hub
            .with_node_binary_dir(dir)
            .with_node_update_timing(Duration::from_secs(5), Duration::from_millis(20));
        let (hub_url, hub_task) = serve(hub.router().unwrap()).await;
        let error = blocking(move || {
            Client::new(&hub_url, "owner-secret", Duration::from_secs(30))
                .unwrap()
                .stream(
                    "/protectorate.v1.NodeService/UpdateNode",
                    &json!({"name":"peer","force":true}),
                    |_| Ok(()),
                )
                .unwrap_err()
        })
        .await;
        assert_eq!(error.code, Code::FailedPrecondition, "{error:?}");
        assert!(error.message.contains(expected), "{error:?}");
        hub_task.abort();
    }
    assert!(!node_root
        .join("updates/protectorate-rs-bundle-next")
        .exists());

    let staged = node_root.join("updates/protectorate-rs-next");
    let body = fs::read(&staged).expect("the plain binary reached the node before its apply");
    let size = body.len().to_string();
    let sha = format!("{:x}", Sha256::digest(&body));
    let hub = Hub::open(temp.path().join("hub"), "owner-secret", "v2").unwrap();
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;
    let cases = [
        (
            json!({"sha256":"0".repeat(64),"size":size}),
            Code::FailedPrecondition,
            "does not match",
        ),
        (
            json!({"sha256":sha,"size":"1"}),
            Code::FailedPrecondition,
            "does not match",
        ),
        (
            json!({"sha256":"not-hex","size":size}),
            Code::InvalidArgument,
            "64 hexadecimal",
        ),
        (
            json!({"sha256":sha,"size":size}),
            Code::FailedPrecondition,
            "platform does not match",
        ),
    ];
    for (mut apply, code, expected) in cases {
        apply["name"] = json!("peer");
        apply["path"] = json!("updates/protectorate-rs-next");
        let url = hub_url.clone();
        let error = blocking(move || {
            Client::new(&url, "owner-secret", Duration::from_secs(30))
                .unwrap()
                .call("/protectorate.v1.NodeService/ApplyUpdate", &apply)
                .unwrap_err()
        })
        .await;
        assert_eq!(error.code, code, "{error:?}");
        assert!(error.message.contains(expected), "{error:?}");
    }
    let metrics: Value = blocking(move || {
        Client::new(&node_url, "owner-secret", Duration::from_secs(10))
            .unwrap()
            .call("/protectorate.v1.NodeAgentService/GetMetrics", &json!({}))
            .unwrap()
    })
    .await;
    assert!(metrics.get("update").is_none(), "{metrics}");
    assert_eq!(fs::read_to_string(&exe).unwrap(), installed);
    assert!(!Path::new(&format!("{}.update", exe.display())).exists());
    assert_eq!(restarts.load(Ordering::SeqCst), 0);
    hub_task.abort();
    node_task.abort();
}
