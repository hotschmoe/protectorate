#![cfg(unix)]

use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use std::{
    fs, io,
    path::Path,
    process::{Command, Output},
    sync::Arc,
};
use tokio::net::TcpListener;

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

async fn cli(url: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate"));
    command
        .env("PROTECTORATE_HUB", url)
        .env("PROTECTORATE_TOKEN", "owner-secret")
        .args(args);
    tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap()
}

fn ok(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn file_and_directory_round_trip_through_hub() {
    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("node-files");
    let node = Agent::new(Arc::new(Metrics))
        .with_files(&files, false)
        .unwrap();
    let (node_url, node_task) = serve(node.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
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
    let (url, hub_task) = serve(hub.router().unwrap()).await;

    let source = root.path().join("source");
    let dest = root.path().join("dest");
    let bytes = vec![b'a'; protectorate_xfer::CHUNK_SIZE + 17];
    fs::write(&source, &bytes).unwrap();
    let put = cli(
        &url,
        &[
            "node",
            "put",
            "peer",
            text(&source),
            "uploaded",
            "--no-compress",
        ],
    )
    .await;
    assert!(ok(&put).contains("put\tpeer:"));
    assert_eq!(fs::read(files.join("uploaded")).unwrap(), bytes);
    ok(&cli(&url, &["node", "get", "peer", "uploaded", text(&dest)]).await);
    assert_eq!(fs::read(&dest).unwrap(), bytes);

    fs::write(&dest, b"keep").unwrap();
    let refused = cli(&url, &["node", "get", "peer", "uploaded", text(&dest)]).await;
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("destination exists"));
    assert_eq!(fs::read(&dest).unwrap(), b"keep");
    ok(&cli(
        &url,
        &["node", "get", "peer", "uploaded", text(&dest), "-f"],
    )
    .await);
    assert_eq!(fs::read(&dest).unwrap(), bytes);

    let tree = root.path().join("tree");
    fs::create_dir_all(tree.join("nested")).unwrap();
    fs::write(tree.join("nested/file"), b"nested data").unwrap();
    ok(&cli(&url, &["node", "put", "peer", text(&tree), "tree-remote"]).await);
    let tree_copy = root.path().join("tree-copy");
    ok(&cli(
        &url,
        &["node", "get", "peer", "tree-remote", text(&tree_copy)],
    )
    .await);
    assert_eq!(
        fs::read(tree_copy.join("nested/file")).unwrap(),
        b"nested data"
    );
    hub_task.abort();
    node_task.abort();
}
