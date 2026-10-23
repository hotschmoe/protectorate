use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::Agent;
use protectorate_node_metrics::{FilesystemMetrics, NodeMetrics};
use serde_json::{json, Value};
use std::{io, sync::Arc, time::Duration};
use tokio::net::TcpListener;

const WATCH: &str = "/protectorate.v1.NodeService/WatchNodeSetup";
const GET_NODE: &str = "/protectorate.v1.NodeService/GetNode";

struct Report;
impl protectorate_node::MetricsCollector for Report {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            agent_version: "test".into(),
            filesystems: vec![FilesystemMetrics {
                mount: "/".into(),
                used_percent: 10.0,
                primary: true,
                ..FilesystemMetrics::default()
            }],
            ..NodeMetrics::default()
        })
    }
}

/// The hub polls its inventory on its own interval, which a setup watch must
/// not have to wait for: Go's WatchNodeSetup kicks a poll of the node it is
/// watching, and the Rust watch has to do the same or the flow sits at JOINED.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_watch_polls_the_node_it_watches() {
    let dir = tempfile::tempdir().unwrap();
    let agent = Agent::new(Arc::new(Report), "node-secret");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let agent_addr = listener.local_addr().unwrap();
    let agent_task = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    // Added straight to the inventory, so nothing has polled this node yet:
    // AddNode's own kick is not what is under test here.
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: agent_addr.to_string(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let monitor = Arc::new(protectorate_node_monitor::Monitor::new(
        Arc::clone(&hub.inventory),
        hub.hostname.clone(),
        dir.path().to_path_buf(),
    ));
    monitor.set_token("node-secret".into()).unwrap();
    let hub_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_addr = hub_listener.local_addr().unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(
            hub_listener,
            hub.with_node_monitor(monitor).unwrap().router().unwrap(),
        )
        .await
        .unwrap()
    });
    let url = format!("http://{hub_addr}");
    let before = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            Client::new(&url, "owner-secret", Duration::from_secs(5))
                .unwrap()
                .call(GET_NODE, &json!({"name":"peer"}))
                .unwrap()
        })
        .await
        .unwrap()
    };
    assert_eq!(before["error"], "awaiting first poll", "{before}");
    assert!(before["metrics"].is_null(), "{before}");
    let events: Vec<Value> = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || {
            let mut events = Vec::new();
            Client::new(&url, "owner-secret", Duration::from_secs(30))
                .unwrap()
                .stream(WATCH, &json!({"name":"peer"}), |event| {
                    events.push(event);
                    Ok(())
                })
                .unwrap();
            events
        })
        .await
        .unwrap()
    };
    // No poll_all or poll_node call anywhere in this test: the watch itself
    // has to reach the agent.
    assert_eq!(
        events.first().unwrap()["stage"],
        "NODE_SETUP_STAGE_JOINED",
        "{events:?}"
    );
    assert_eq!(
        events.first().unwrap()["message"],
        "awaiting first poll",
        "{events:?}"
    );
    let last = events.last().unwrap();
    assert_eq!(last["stage"], "NODE_SETUP_STAGE_READY", "{events:?}");
    assert_eq!(last["message"], "linux/x86_64, agent test", "{events:?}");
    assert_eq!(
        last["status"]["freshness"], "NODE_FRESHNESS_FRESH",
        "{events:?}"
    );
    assert_eq!(
        last["status"]["metrics"]["agentVersion"], "test",
        "{events:?}"
    );
    hub_task.abort();
    agent_task.abort();
}
