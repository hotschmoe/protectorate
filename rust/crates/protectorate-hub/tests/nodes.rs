use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_node_monitor::{Monitor, Timing};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use std::{
    fs, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[test]
fn hub_startup_regenerates_hosts_and_reports_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let hosts = dir.path().join("hosts.md");
    fs::write(&hosts, b"stale").unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let text = fs::read_to_string(&hosts).unwrap();
    assert!(text.contains("# Hosts\n"));
    assert!(text.contains(&format!("{} (this machine, hub)", hub.hostname)));
    drop(hub);

    fs::remove_file(&hosts).unwrap();
    fs::create_dir(&hosts).unwrap();
    assert!(Hub::open(dir.path(), "owner-secret", "test").is_err());
    assert!(hosts.is_dir());
}

#[tokio::test]
async fn local_node_metrics_include_only_running_visible_sleeves() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let active = Record::new(
        "worker",
        Harness::Pi,
        Mode::Chat,
        &workspace,
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    hub.sleeves.record_spawned(active).unwrap();
    let mut helper = Record::new(
        "sum-worker",
        Harness::Pi,
        Mode::Chat,
        &workspace,
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    helper.helper = true;
    hub.sleeves.record_spawned(helper).unwrap();
    hub.node_monitor.poll_node(&hub.hostname);
    let name = hub.hostname.clone();
    let (addr, server) = serve(hub).await;
    let (status, body) = call(
        addr,
        "/protectorate.v1.NodeService/GetNode",
        "owner-secret",
        json!({"name":name}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["metrics"]["sleeves"].as_array().unwrap().len(), 1);
    assert_eq!(body["metrics"]["sleeves"][0]["name"], "worker");
    assert_eq!(body["metrics"]["sleeves"][0]["harness"], "HARNESS_PI");
    server.abort();
}
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_watch_moves_from_invited_to_ready_after_authenticated_agent_poll() {
    let dir = tempfile::tempdir().unwrap();
    let agent = Agent::new(Arc::new(MockMetrics(Arc::new(AtomicBool::new(false)))));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let agent_addr = listener.local_addr().unwrap();
    let agent_task = tokio::spawn(async move {
        axum::serve(listener, agent.router().unwrap())
            .await
            .unwrap()
    });
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let inventory = Arc::clone(&hub.inventory);
    inventory
        .add(
            Node {
                name: "peer".into(),
                address: agent_addr.to_string(),
                invited: true,
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let monitor = Arc::new(Monitor::new(
        Arc::clone(&inventory),
        hub.hostname.clone(),
        dir.path().to_path_buf(),
    ));
    let (hub_addr, hub_task) = serve(hub.with_node_monitor(Arc::clone(&monitor)).unwrap()).await;
    let url = format!("http://{hub_addr}");
    let watch = "/protectorate.v1.NodeService/WatchNodeSetup";
    let first = tokio::task::spawn_blocking({
        let url = url.clone();
        move || {
            let mut events = Vec::new();
            let result = Client::new(&url, "owner-secret", Duration::from_secs(3))
                .unwrap()
                .stream(watch, &json!({"name":"peer"}), |event| {
                    events.push(event);
                    Err(RpcError::new(Code::Canceled, "read first stage"))
                });
            (result, events)
        }
    })
    .await
    .unwrap();
    assert_eq!(first.0.unwrap_err().code, Code::Canceled);
    assert_eq!(first.1.len(), 1);
    assert_eq!(first.1[0]["stage"], "NODE_SETUP_STAGE_INVITED");
    inventory.clear_invited("peer").unwrap();
    let monitor_poll = Arc::clone(&monitor);
    tokio::task::spawn_blocking(move || monitor_poll.poll_node("peer"))
        .await
        .unwrap();
    let ready = tokio::task::spawn_blocking(move || {
        let mut events = Vec::new();
        Client::new(&url, "owner-secret", Duration::from_secs(3))
            .unwrap()
            .stream(watch, &json!({"name":"peer"}), |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();
        events
    })
    .await
    .unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0]["stage"], "NODE_SETUP_STAGE_READY");
    assert_eq!(ready[0]["status"]["freshness"], "NODE_FRESHNESS_FRESH");
    hub_task.abort();
    agent_task.abort();
}

#[tokio::test]
async fn inventory_http_round_trip_auth_update_and_tier() {
    let dir = tempfile::tempdir().unwrap();
    let (addr, server) = serve(Hub::open(dir.path(), "owner-secret", "test").unwrap()).await;
    let list = "/protectorate.v1.NodeService/ListNodes";
    let add = "/protectorate.v1.NodeService/AddNode";
    let get = "/protectorate.v1.NodeService/GetNode";
    let tier = "/protectorate.v1.NodeService/SetNodeTier";
    let (status, empty) = call(addr, list, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{empty}");
    assert_eq!(empty["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(empty["nodes"][0]["node"]["local"], true);
    let (status, missing_lease) = call(
        addr,
        "/protectorate.v1.LeaseService/AcquireLease",
        "owner-secret",
        json!({"resource":"stranger", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(
        (status, &missing_lease["code"]),
        (400, &json!("failed_precondition"))
    );
    let (status, denied) = call(
        addr,
        add,
        "bad",
        json!({"node":{"name":"compute","address":"compute"}}),
    )
    .await;
    assert_eq!((status, &denied["code"]), (401, &json!("unauthenticated")));
    let input = json!({"node":{"name":" Compute ","address":"compute:7475", "tags":["GPU", "gpu", "linux"], "notes":" first ", "sshAlias":"compute-ssh"}});
    let (status, created) = call(addr, add, "owner-secret", input.clone()).await;
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["name"], "compute");
    assert_eq!(created["tags"], json!(["gpu", "linux"]));
    assert_eq!(created["tier"], "shared");
    let (status, duplicate) = call(addr, add, "owner-secret", input).await;
    assert_eq!(
        (status, &duplicate["code"]),
        (409, &json!("already_exists"))
    );
    let (status, updated) = call(
        addr,
        add,
        "owner-secret",
        json!({"update":true,"node":{"name":"compute","address":"", "tags":["new"]}}),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["address"], "compute:7475");
    assert_eq!(updated["notes"], "first");
    assert_eq!(updated["tags"], json!(["new"]));
    let (status, changed) = call(
        addr,
        tier,
        "owner-secret",
        json!({"name":"COMPUTE", "tier":"CARE"}),
    )
    .await;
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["tier"], "care");
    let (status, care_lease) = call(
        addr,
        "/protectorate.v1.LeaseService/AcquireLease",
        "owner-secret",
        json!({"resource":"compute", "holder":"test", "ttl":"10s"}),
    )
    .await;
    assert_eq!(
        (status, &care_lease["code"]),
        (400, &json!("failed_precondition"))
    );
    let (status, node) = call(addr, get, "owner-secret", json!({"name":"COMPUTE"})).await;
    assert_eq!(status, 200, "{node}");
    assert_eq!(node["node"]["tier"], "care");
    assert!(
        node["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()),
        "{node}"
    );
    let (status, invalid) = call(
        addr,
        add,
        "owner-secret",
        json!({"node":{"name":"bad","address":"https://evil/path"}}),
    )
    .await;
    assert_eq!(
        (status, &invalid["code"]),
        (400, &json!("invalid_argument"))
    );
    let (status, invited) = call(
        addr,
        add,
        "owner-secret",
        json!({"node":{"name":"pending","address":"pending", "invited":true}}),
    )
    .await;
    assert_eq!(status, 200, "{invited}");
    let (status, denied_tier) = call(
        addr,
        tier,
        "owner-secret",
        json!({"name":"pending","tier":"care"}),
    )
    .await;
    assert_eq!(
        (status, &denied_tier["code"]),
        (400, &json!("failed_precondition"))
    );
    server.abort();
    let (addr, server) = serve(Hub::open(dir.path(), "owner-secret", "test").unwrap()).await;
    let (status, list) = call(addr, list, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(list["nodes"][1]["node"]["tier"], "care");
    assert_eq!(list["nodes"][2]["freshness"], "NODE_FRESHNESS_INVITED");
    server.abort();
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

#[cfg(target_os = "linux")]
#[tokio::test]
async fn local_node_needs_actual_collector_poll_for_lease() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let name = hub.hostname.clone();
    let monitor = Arc::clone(&hub.node_monitor);
    let (addr, server) = serve(hub).await;
    let request = json!({"resource": name, "holder":"test", "ttl":"1s"});
    let (status, denied) = call(
        addr,
        "/protectorate.v1.LeaseService/AcquireLease",
        "owner-secret",
        request.clone(),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (400, &json!("failed_precondition"))
    );
    tokio::task::spawn_blocking(move || monitor.poll_node(&name))
        .await
        .unwrap();
    let (status, lease) = call(
        addr,
        "/protectorate.v1.LeaseService/AcquireLease",
        "owner-secret",
        request,
    )
    .await;
    assert_eq!(status, 200, "{lease}");
    server.abort();
}
