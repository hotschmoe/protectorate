use axum::{
    body::{to_bytes, Body},
    http::Request,
    middleware::{from_fn, Next},
    response::Response,
};
use chrono::{DateTime, Utc};
use protectorate_desk_node::{Controller, Desktop, Result as DeskResult};
use protectorate_desk_policy::{Chord, Ready, ScreenState};
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use serde_json::{json, Value};
use std::{
    io,
    sync::{Arc, Mutex},
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
            os: "windows".into(),
            arch: "amd64".into(),
            ..NodeMetrics::default()
        })
    }
}

struct TestDesktop {
    last_input: Mutex<DateTime<Utc>>,
}
impl Desktop for TestDesktop {
    fn ready(&self) -> Ready {
        Ready::Interactive
    }
    fn screen(&self) -> ScreenState {
        ScreenState::Unlocked
    }
    fn last_physical_input(&self) -> Option<DateTime<Utc>> {
        Some(*self.last_input.lock().unwrap())
    }
    fn screenshot(&self, _: i32) -> DeskResult<Value> {
        unreachable!()
    }
    fn windows(&self) -> DeskResult<Value> {
        unreachable!()
    }
    fn foreground(&self) -> DeskResult<Value> {
        unreachable!()
    }
    fn move_pointer(&self, _: i32, _: i32) -> DeskResult<()> {
        unreachable!()
    }
    fn click(&self, _: i32, _: i32, _: &str, _: bool) -> DeskResult<()> {
        unreachable!()
    }
    fn type_text(&self, _: &str) -> DeskResult<()> {
        unreachable!()
    }
    fn key(&self, _: Chord) -> DeskResult<()> {
        unreachable!()
    }
    fn run(&self, _: &str, _: &[String]) -> DeskResult<u32> {
        unreachable!()
    }
    fn lock(&self) -> DeskResult<()> {
        unreachable!()
    }
}

async fn call(address: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

async fn serve(router: axum::Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (address, task)
}

async fn wait_for_end(seen: &Arc<Mutex<Vec<Value>>>, expected: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let count = seen
                .lock()
                .unwrap()
                .iter()
                .filter(|request| request["leaseEnded"] == true)
                .count();
            if count >= expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let values = seen.lock().unwrap();
    let latest = values
        .iter()
        .rfind(|request| request["leaseEnded"] == true)
        .unwrap();
    assert_eq!(latest["policy"]["lockAfter"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_expiry_and_policy_refusal_notify_real_node() {
    let home = tempfile::tempdir().unwrap();
    let desktop = Arc::new(TestDesktop {
        last_input: Mutex::new(Utc::now() - chrono::Duration::minutes(30)),
    });
    std::fs::create_dir(home.path().join("node")).unwrap();
    let controller =
        Arc::new(Controller::with_desktop(home.path().join("node"), desktop.clone()).unwrap());
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = Arc::clone(&seen);
    let node = Agent::new(Arc::new(Metrics))
        .with_desktop(controller)
        .router()
        .unwrap()
        .layer(from_fn(move |request: Request<Body>, next: Next| {
            let capture = Arc::clone(&capture);
            async move {
                let request =
                    if request.uri().path() == "/protectorate.v1.NodeAgentService/GetDeskState" {
                        let (parts, body) = request.into_parts();
                        let bytes = to_bytes(body, 1024 * 1024).await.unwrap();
                        capture
                            .lock()
                            .unwrap()
                            .push(serde_json::from_slice(&bytes).unwrap());
                        Request::from_parts(parts, Body::from(bytes))
                    } else {
                        request
                    };
                let response: Response = next.run(request).await;
                response
            }
        }));
    let (node_addr, node_task) = serve(node).await;
    let hub = Hub::open(home.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                os: "windows".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.node_monitor.poll_node("peer");
    let leases = Arc::clone(&hub.leases);
    let (hub_addr, hub_task) = serve(hub.router().unwrap()).await;
    let now = Utc::now();
    let window = format!(
        "{}-{}",
        (now - chrono::Duration::hours(1)).format("%H:%M"),
        (now + chrono::Duration::hours(1)).format("%H:%M")
    );
    let (status, policy) = call(
        hub_addr,
        "NodeService/SetDeskPolicy",
        json!({
            "name":"peer", "policy":{"window":window,"idle":"60s","timezone":"UTC","lockAfter":true}
        }),
    )
    .await;
    assert_eq!(status, 200, "{policy}");
    assert_eq!(policy["state"]["available"], true, "{policy}");

    let (status, release) = call(
        hub_addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:peer", "holder":"test", "ttl":"30s"
        }),
    )
    .await;
    assert_eq!(status, 200, "{release}");
    let (status, ended) = call(
        hub_addr,
        "LeaseService/ReleaseLease",
        json!({"id":release["id"]}),
    )
    .await;
    assert_eq!(status, 200, "{ended}");
    wait_for_end(&seen, 1).await;

    let (status, expiry) = call(
        hub_addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:peer", "holder":"test", "ttl":"1s"
        }),
    )
    .await;
    assert_eq!(status, 200, "{expiry}");
    leases.sweep(Utc::now().timestamp_millis() + 1500).unwrap();
    wait_for_end(&seen, 2).await;

    let (status, refusal) = call(
        hub_addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:peer", "holder":"test", "ttl":"30s"
        }),
    )
    .await;
    assert_eq!(status, 200, "{refusal}");
    *desktop.last_input.lock().unwrap() = Utc::now();
    let (status, blocked) = call(
        hub_addr,
        "DeskService/Type",
        json!({
            "target":{"name":"peer", "leaseId":refusal["id"]}, "text":"blocked"
        }),
    )
    .await;
    assert_eq!(status, 400, "{blocked}");
    wait_for_end(&seen, 3).await;
    assert!(leases
        .blocked(
            &protectorate_lease::parse_resource("desk:peer").unwrap(),
            Utc::now().timestamp_millis()
        )
        .unwrap()
        .is_none());
    hub_task.abort();
    node_task.abort();
}
