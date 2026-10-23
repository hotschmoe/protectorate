#![cfg(unix)]

use protectorate_cli::{dispatch_endpoint, parse, Command};
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use std::{fs, io, sync::Arc};
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

#[tokio::test]
async fn file_and_directory_round_trip_through_hub() {
    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("node-files");
    let node = Agent::new(Arc::new(Metrics), "node-secret")
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
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;

    let source = root.path().join("source");
    let dest = root.path().join("dest");
    let bytes = vec![b'a'; protectorate_xfer::CHUNK_SIZE + 17];
    fs::write(&source, &bytes).unwrap();
    let url = hub_url.clone();
    let input = source.to_string_lossy().to_string();
    let output = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "put", "peer", &input, "uploaded", "--no-compress"],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(output.contains("put\tpeer:"));
    assert_eq!(fs::read(files.join("uploaded")).unwrap(), bytes);
    let url = hub_url.clone();
    let output = dest.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "get", "peer", "uploaded", &output],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(fs::read(&dest).unwrap(), bytes);

    fs::write(&dest, b"keep").unwrap();
    let url = hub_url.clone();
    let output = dest.to_string_lossy().to_string();
    let error = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "get", "peer", "uploaded", &output],
        )
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.message.contains("destination exists"));
    assert_eq!(fs::read(&dest).unwrap(), b"keep");
    let url = hub_url.clone();
    let output = dest.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "get", "peer", "uploaded", &output, "-f"],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(fs::read(&dest).unwrap(), bytes);

    let tree = root.path().join("tree");
    fs::create_dir_all(tree.join("nested")).unwrap();
    fs::write(tree.join("nested/file"), b"nested data").unwrap();
    let url = hub_url.clone();
    let input = tree.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "put", "peer", &input, "tree-remote"],
        )
    })
    .await
    .unwrap()
    .unwrap();
    let tree_copy = root.path().join("tree-copy");
    let url = hub_url.clone();
    let output = tree_copy.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url,
            "owner-secret",
            ["node", "get", "peer", "tree-remote", &output],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        fs::read(tree_copy.join("nested/file")).unwrap(),
        b"nested data"
    );
    hub_task.abort();
    node_task.abort();
}

#[test]
fn transfer_arguments() {
    let put = parse(["node", "put", "n", "local", "remote", "-f", "--no-compress"]).unwrap();
    assert!(matches!(
        put.command,
        Command::NodePut {
            force: true,
            no_compress: true,
            ..
        }
    ));
    let get = parse(["node", "get", "n", "remote", "local"]).unwrap();
    assert!(matches!(get.command, Command::NodeGet { force: false, .. }));
    assert!(matches!(
        parse(["node", "put", "-f", "n", "local", "remote"])
            .unwrap()
            .command,
        Command::NodePut { force: true, .. }
    ));
}
