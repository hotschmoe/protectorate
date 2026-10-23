use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_node_monitor::{Monitor, Timing};
use serde_json::{json, Value};
use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: SocketAddr, path: &str, body: Value) -> Value {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// An agent that answers each GetMetrics with the next scripted reply, the
/// last one repeating, so a test can deliver samples late or out of order.
async fn scripted_agent(replies: Vec<Value>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut next = 0;
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = stream.read(&mut chunk).await.unwrap();
                request.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&request);
                let Some((head, body)) = text.split_once("\r\n\r\n") else {
                    continue;
                };
                let length = head
                    .to_ascii_lowercase()
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: ")?.parse::<usize>().ok())
                    .unwrap_or(0);
                if body.len() >= length || count == 0 {
                    break;
                }
            }
            let body = replies[next.min(replies.len() - 1)].to_string();
            next += 1;
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    addr
}

struct Steady;
impl MetricsCollector for Steady {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics {
            hostname: "steady".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            ..NodeMetrics::default()
        })
    }
}

async fn serve_agent(agent: Agent) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_samples_are_dropped_and_a_restarted_agent_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let sample = |host: &str, instance: &str, seq: u64| json!({"hostname":host,"os":"linux","arch":"x86_64","instance":instance,"seq":seq.to_string()});
    let scripted = scripted_agent(vec![
        sample("first", "a", 5),
        sample("late", "a", 4),
        sample("repeat", "a", 5),
        sample("second", "a", 6),
        sample("restarted", "b", 1),
        sample("restart repeat", "b", 1),
        sample("counting on", "b", 2),
    ])
    .await;
    let steady = serve_agent(Agent::new(Arc::new(Steady))).await;
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    for (name, addr) in [("scripted", scripted), ("steady", steady)] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: addr.to_string(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let monitor = Arc::new(
        Monitor::new(
            Arc::clone(&hub.inventory),
            hub.hostname.clone(),
            dir.path().to_path_buf(),
        )
        .with_timing(Timing {
            stale_after: Duration::from_secs(30),
            fetch_timeout: Duration::from_secs(5),
        }),
    );
    let hub = hub.with_node_monitor(Arc::clone(&monitor)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let get = "/protectorate.v1.NodeService/GetNode";
    for (poll, shown) in [
        "first",
        "first",
        "first",
        "second",
        "restarted",
        "restarted",
        "counting on",
    ]
    .into_iter()
    .enumerate()
    {
        let monitor = Arc::clone(&monitor);
        tokio::task::spawn_blocking(move || monitor.poll_node("scripted"))
            .await
            .unwrap();
        let node = call(hub_addr, get, json!({"name":"scripted"})).await;
        assert_eq!(node["metrics"]["hostname"], shown, "poll {poll}: {node}");
    }
    for _ in 0..3 {
        let monitor = Arc::clone(&monitor);
        tokio::task::spawn_blocking(move || monitor.poll_node("steady"))
            .await
            .unwrap();
    }
    let node = call(hub_addr, get, json!({"name":"steady"})).await;
    assert_eq!(node["metrics"]["seq"], "3", "{node}");
    assert!(node["metrics"]["instance"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
}
