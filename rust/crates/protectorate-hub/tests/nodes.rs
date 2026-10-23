use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
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
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}
async fn call(addr: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

struct MockMetrics(Arc<AtomicBool>);
impl MetricsCollector for MockMetrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        if self.0.load(Ordering::SeqCst) {
            return Err(io::Error::other("mock probe failed"));
        }
        Ok(NodeMetrics {
            hostname: "peer".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            ..NodeMetrics::default()
        })
    }
}

#[tokio::test]
async fn live_agent_poll_controls_status_and_lease_admission() {
    let dir = tempfile::tempdir().unwrap();
    let failure = Arc::new(AtomicBool::new(false));
    let agent = Agent::new(Arc::new(MockMetrics(Arc::clone(&failure))));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let agent_addr = listener.local_addr().unwrap();
    let agent_task = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let inventory = Arc::clone(&hub.inventory);
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
    let monitor = Arc::new(
        Monitor::new(
            Arc::clone(&hub.inventory),
            hub.hostname.clone(),
            dir.path().to_path_buf(),
        )
        .with_timing(Timing {
            stale_after: Duration::from_secs(1),
            fetch_timeout: Duration::from_secs(1),
        }),
    );
    let hub = hub.with_node_monitor(Arc::clone(&monitor)).unwrap();
    let (hub_addr, hub_task) = serve(hub).await;
    let acquire = "/protectorate.v1.LeaseService/AcquireLease";
    let get = "/protectorate.v1.NodeService/GetNode";
    let list = "/protectorate.v1.NodeService/ListNodes";
    let (status, waiting) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{waiting}");
    assert_eq!(waiting["error"], "awaiting first poll");
    let (status, denied) = call(
        hub_addr,
        acquire,
        "owner-secret",
        json!({"resource":"peer", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (400, &json!("failed_precondition"))
    );
    let poll = Arc::clone(&monitor);
    tokio::task::spawn_blocking(move || poll.poll_node("peer"))
        .await
        .unwrap();
    let (status, healthy) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{healthy}");
    assert_eq!(healthy["freshness"], "NODE_FRESHNESS_FRESH");
    assert_eq!(healthy["metrics"]["hostname"], "peer");
    assert_eq!(healthy["metrics"]["os"], "linux");
    assert!(healthy["fetchedAt"].as_str().is_some());
    let (status, listing) = call(hub_addr, list, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{listing}");
    assert_eq!(listing["nodes"][1]["freshness"], "NODE_FRESHNESS_FRESH");
    let (status, lease) = call(
        hub_addr,
        acquire,
        "owner-secret",
        json!({"resource":"peer", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(status, 200, "{lease}");
    let (status, _) = call(
        hub_addr,
        "/protectorate.v1.LeaseService/ReleaseLease",
        "owner-secret",
        json!({"id":lease["id"]}),
    )
    .await;
    assert_eq!(status, 200);
    failure.store(true, Ordering::SeqCst);
    let poll = Arc::clone(&monitor);
    tokio::task::spawn_blocking(move || poll.poll_node("peer"))
        .await
        .unwrap();
    let (status, stale) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{stale}");
    assert_eq!(stale["freshness"], "NODE_FRESHNESS_STALE");
    assert!(stale["error"]
        .as_str()
        .unwrap()
        .contains("mock probe failed"));
    let (status, denied) = call(
        hub_addr,
        acquire,
        "owner-secret",
        json!({"resource":"peer", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (400, &json!("failed_precondition"))
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (status, down) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{down}");
    assert_eq!(down["freshness"], "NODE_FRESHNESS_UNREACHABLE");
    assert_eq!(down["alerts"][0]["kind"], "ALERT_KIND_UNREACHABLE");
    failure.store(false, Ordering::SeqCst);
    inventory
        .add(
            Node {
                name: "invited".into(),
                address: agent_addr.to_string(),
                invited: true,
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let (status, pending) = call(hub_addr, get, "owner-secret", json!({"name":"invited"})).await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["freshness"], "NODE_FRESHNESS_INVITED");
    let poll = Arc::clone(&monitor);
    tokio::task::spawn_blocking(move || poll.poll_node("invited"))
        .await
        .unwrap();
    let (status, joined) = call(hub_addr, get, "owner-secret", json!({"name":"invited"})).await;
    assert_eq!(status, 200, "{joined}");
    assert_eq!(joined["freshness"], "NODE_FRESHNESS_FRESH");
    assert!(joined["node"]["invited"].is_null());
    assert_eq!(joined["node"]["os"], "linux");
    inventory
        .add(
            Node {
                name: "peer".into(),
                address: "127.0.0.1:1".into(),
                ..Node::default()
            },
            true,
        )
        .unwrap();
    let (status, moved) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["error"], "awaiting first poll");
    let (status, denied) = call(
        hub_addr,
        acquire,
        "owner-secret",
        json!({"resource":"peer", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (400, &json!("failed_precondition"))
    );
    hub_task.abort();
    agent_task.abort();
}
