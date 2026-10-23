use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::{CpuMetrics, FilesystemMetrics, MemoryMetrics, NodeMetrics};
use protectorate_node_monitor::{Monitor, Timing};
use serde_json::{json, Value};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::net::TcpListener;

struct Metrics(Arc<AtomicBool>);
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        if self.0.load(Ordering::SeqCst) {
            return Err(io::Error::other("probe failed"));
        }
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            cpu: Some(CpuMetrics {
                usage_percent: 37.5,
                ..Default::default()
            }),
            memory: Some(MemoryMetrics {
                used_percent: 92.0,
                ..Default::default()
            }),
            filesystems: vec![FilesystemMetrics {
                mount: "/data".into(),
                used_percent: 94.0,
                ..Default::default()
            }],
            ..Default::default()
        })
    }
}

async fn next_kind(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let event = rx.recv().await.expect("subscription ended");
            if event["kind"] == kind {
                return event;
            }
        }
    })
    .await
    .expect("expected event")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn node_poll_events_match_live_status_and_emit_once_per_down_transition() {
    let home = tempfile::tempdir().unwrap();
    let failing = Arc::new(AtomicBool::new(false));
    let agent = Agent::new(Arc::new(Metrics(Arc::clone(&failing))), "node-secret");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let agent_task = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: address.clone(),
                invited: true,
                invite_key_id: 42,
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let monitor = Arc::new(
        Monitor::new(
            Arc::clone(&hub.inventory),
            hub.hostname.clone(),
            home.path().to_path_buf(),
        )
        .with_timing(Timing {
            stale_after: Duration::from_secs(1),
            fetch_timeout: Duration::from_secs(2),
        }),
    );
    monitor.set_token("node-secret".into()).unwrap();
    let hub = hub.with_node_monitor(Arc::clone(&monitor)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_addr = listener.local_addr().unwrap();
    let hub_task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let url = format!("http://{hub_addr}");
    let subscriber = tokio::task::spawn_blocking({
        let url = url.clone();
        move || {
            Client::new(&url, "owner-secret", Duration::from_secs(15))
                .unwrap()
                .stream(
                    "/protectorate.v1.EventService/Subscribe",
                    &json!({}),
                    |event| {
                        tx.send(event).unwrap();
                        Ok(())
                    },
                )
        }
    });
    next_kind(&mut rx, "heartbeat").await;

    tokio::task::spawn_blocking({
        let monitor = Arc::clone(&monitor);
        move || monitor.poll_node("peer")
    })
    .await
    .unwrap();
    let joined = next_kind(&mut rx, "node.joined").await;
    assert_eq!(joined["subject"], "peer");
    assert_eq!(joined["payload"]["address"], address);
    let after_join = tokio::task::spawn_blocking({
        let url = url.clone();
        move || {
            Client::new(&url, "owner-secret", Duration::from_secs(2))
                .unwrap()
                .call(
                    "/protectorate.v1.NodeService/GetNode",
                    &json!({"name":"peer"}),
                )
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert_eq!(after_join["freshness"], "NODE_FRESHNESS_FRESH");
    assert_ne!(after_join["node"]["invited"], true);
    let metrics = next_kind(&mut rx, "node.metrics").await;
    assert_eq!(metrics["payload"]["freshness"], "fresh");
    assert_eq!(metrics["payload"]["cpu_percent"], 37.5);
    assert_eq!(metrics["payload"]["mem_percent"], 92.0);
    assert_eq!(metrics["payload"]["worst_disk_percent"], 94.0);
    assert_eq!(metrics["payload"]["worst_disk_mount"], "/data");
    assert_eq!(metrics["payload"]["gpu_count"].as_f64(), Some(0.0));
    assert_eq!(metrics["payload"]["containers"].as_f64(), Some(0.0));
    assert_eq!(
        metrics["payload"]["alerts"],
        json!(["disk_percent", "mem_pressure"])
    );
    let status = tokio::task::spawn_blocking({
        let url = url.clone();
        move || {
            Client::new(&url, "owner-secret", Duration::from_secs(2))
                .unwrap()
                .call(
                    "/protectorate.v1.NodeService/GetNode",
                    &json!({"name":"peer"}),
                )
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert_eq!(status["freshness"], "NODE_FRESHNESS_FRESH");
    let alert_kinds = status["alerts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|alert| {
            alert["kind"]
                .as_str()
                .unwrap()
                .trim_start_matches("ALERT_KIND_")
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    assert_eq!(metrics["payload"]["alerts"], json!(alert_kinds));

    failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    tokio::task::spawn_blocking({
        let monitor = Arc::clone(&monitor);
        move || monitor.poll_node("peer")
    })
    .await
    .unwrap();
    let down = next_kind(&mut rx, "node.unreachable").await;
    assert_eq!(down["subject"], "peer");
    assert_eq!(down["payload"]["address"], address);
    assert!(down["payload"]["error"]
        .as_str()
        .unwrap()
        .contains("probe failed"));
    tokio::task::spawn_blocking({
        let monitor = Arc::clone(&monitor);
        move || monitor.poll_node("peer")
    })
    .await
    .unwrap();
    assert!(tokio::time::timeout(Duration::from_millis(150), rx.recv())
        .await
        .is_err());

    failing.store(false, Ordering::SeqCst);
    tokio::task::spawn_blocking({
        let monitor = Arc::clone(&monitor);
        move || monitor.poll_node("peer")
    })
    .await
    .unwrap();
    next_kind(&mut rx, "node.metrics").await;
    failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    tokio::task::spawn_blocking({
        let monitor = Arc::clone(&monitor);
        move || monitor.poll_node("peer")
    })
    .await
    .unwrap();
    next_kind(&mut rx, "node.unreachable").await;
    hub_task.abort();
    agent_task.abort();
    subscriber.abort();
}
