use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE, SHARED};
use protectorate_lease::{parse_resource, Chooser, Error};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const ACQUIRE: &str = "/protectorate.v1.LeaseService/AcquireLease";
const RENEW: &str = "/protectorate.v1.LeaseService/RenewLease";
const RELEASE: &str = "/protectorate.v1.LeaseService/ReleaseLease";
const LIST: &str = "/protectorate.v1.LeaseService/ListLeases";

// Queue/event tests exercise the lease table independently of node health.
fn test_chooser() -> Chooser {
    Arc::new(|request, blocked| {
        let resource = parse_resource(request)?;
        if let Some(held) = blocked(&resource) {
            return Err(Error::Busy(format!("held by {}", held.holder)));
        }
        Ok(resource)
    })
}

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

async fn read_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            return String::from_utf8(line).unwrap();
        }
    }
}

async fn read_event(stream: &mut TcpStream) -> Value {
    let size = usize::from_str_radix(read_line(stream).await.trim(), 16).unwrap();
    let mut frame = vec![0; size];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(read_line(stream).await, "\r\n");
    assert_eq!(frame[0], 0);
    assert_eq!(
        u32::from_be_bytes(frame[1..5].try_into().unwrap()) as usize,
        size - 5
    );
    serde_json::from_slice(&frame[5..]).unwrap()
}

#[tokio::test]
async fn lease_change_reaches_event_subscriber() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_lease_chooser(test_chooser())
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, added) = call(
        addr,
        "/protectorate.v1.NodeService/AddNode",
        "owner-secret",
        json!({"node":{"name":"peer","address":"peer:7475"}}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!("POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    assert!(read_line(&mut stream).await.contains("200"));
    while read_line(&mut stream).await != "\r\n" {}
    let heartbeat = tokio::time::timeout(Duration::from_secs(2), read_event(&mut stream))
        .await
        .unwrap();
    assert_eq!(heartbeat["kind"], "heartbeat");

    let (status, lease) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"subscriber", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{lease}");
    let event = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let event = read_event(&mut stream).await;
            if event["kind"] == "lease.acquired" && event["subject"] == lease["id"] {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(event["kind"], "lease.acquired");
    assert_eq!(event["subject"], lease["id"]);
    assert_eq!(event["payload"]["holder"], "subscriber");
    drop(stream);
    server.abort();
}

#[tokio::test]
async fn lease_queue_priority_claim_and_restart_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_lease_chooser(test_chooser())
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, added) = call(
        addr,
        "/protectorate.v1.NodeService/AddNode",
        "owner-secret",
        json!({"node":{"name":"peer","address":"peer:7475"}}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    let (status, held) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"first", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{held}");
    assert_eq!(held["resource"], "peer");
    assert_eq!(held["ttl"], "20s");
    let (status, busy) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"other", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &busy["code"]),
        (400, &json!("failed_precondition"))
    );
    for (holder, priority) in [("low-a", 0), ("low-b", 0), ("high", 5)] {
        let (status, ticket) = call(addr, ACQUIRE, "owner-secret", json!({"resource":"peer", "holder":holder, "ttl":"20s", "wait":true, "priority":priority})).await;
        assert_eq!(status, 200, "{ticket}");
        assert_eq!(ticket["state"], "LEASE_STATE_QUEUED");
    }
    let (status, list) = call(addr, LIST, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        list["leases"]
            .as_array()
            .unwrap()
            .iter()
            .skip(1)
            .map(|l| l["holder"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["high", "low-a", "low-b"]
    );
    server.abort();

    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_lease_chooser(test_chooser())
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, _) = call(addr, RELEASE, "owner-secret", json!({"id":held["id"]})).await;
    assert_eq!(status, 200);
    for expected in ["high", "low-a", "low-b"] {
        let (status, list) = call(addr, LIST, "owner-secret", json!({})).await;
        assert_eq!(status, 200, "{list}");
        let lease = &list["leases"][0];
        assert_eq!(lease["holder"], expected);
        assert!(lease.get("state").is_none());
        let (status, claimed) = call(addr, RENEW, "owner-secret", json!({"id":lease["id"]})).await;
        assert_eq!(status, 200, "{claimed}");
        assert_eq!(claimed["ttl"], "20s");
        let (status, _) = call(addr, RELEASE, "owner-secret", json!({"id":lease["id"]})).await;
        assert_eq!(status, 200);
    }
    let (status, empty) = call(addr, LIST, "owner-secret", json!({})).await;
    assert_eq!(status, 200);
    assert!(empty
        .get("leases")
        .is_none_or(|leases| leases.as_array().is_some_and(Vec::is_empty)));
    server.abort();
}

#[tokio::test]
async fn selector_admission_is_injected_and_default_fails_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let (addr, server) = serve(Hub::open(dir.path(), "owner-secret", "test").unwrap()).await;
    let (status, failure) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"selector":{"gpu":"intel"}, "holder":"client", "ttl":"1s"}),
    )
    .await;
    assert_eq!(
        (status, &failure["code"]),
        (400, &json!("failed_precondition"))
    );
    assert!(failure["message"]
        .as_str()
        .unwrap()
        .contains("node inventory"));
    server.abort();

    let chooser: Chooser = Arc::new(|request, blocked| {
        let resource = if request.starts_with('{') {
            assert_eq!(
                serde_json::from_str::<Value>(request).unwrap()["selector"]["gpu"],
                "intel"
            );
            parse_resource("gpu-box:gpu1")?
        } else {
            parse_resource(request)?
        };
        if blocked(&resource).is_some() {
            return Err(Error::Busy("held".into()));
        }
        Ok(resource)
    });
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_lease_chooser(chooser)
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, lease) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"selector":{"gpu":"intel"}, "holder":"client", "ttl":"0.2s"}),
    )
    .await;
    assert_eq!(status, 200, "{lease}");
    assert_eq!(lease["resource"], "gpu-box:gpu1");
    assert_eq!(lease["gpuIndex"], 1);
    assert_eq!(lease["ttl"], "0.200s");
    tokio::time::sleep(Duration::from_millis(250)).await;
    let (status, expired) = call(addr, RENEW, "owner-secret", json!({"id":lease["id"]})).await;
    assert_eq!((status, &expired["code"]), (404, &json!("not_found")));
    server.abort();
}

fn gpu_json(index: i32, used: u64, total: u64, integrated: bool) -> Value {
    json!({
        "index": index,
        "memoryUsedBytes": used.to_string(),
        "memoryTotalBytes": total.to_string(),
        "utilizationPercent": 0.0,
        "temperatureCelsius": 30.0,
        "integrated": integrated,
    })
}

fn metrics_with_gpus(gpus: Vec<Value>) -> Value {
    json!({
        "gpus": gpus,
        "engine": {"containers": [
            {"name": "ollama", "state": "running"},
            {"name": "stopped", "state": "exited"}
        ]}
    })
}

fn add_shared(hub: &Hub, name: &str, address: &str) {
    hub.inventory
        .add(
            Node {
                name: name.into(),
                address: address.into(),
                tier: SHARED.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
}

#[tokio::test]
async fn default_chooser_rejects_busy_explicit_and_discrete_gpus() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    add_shared(&hub, "gpu-box", "gpu-box.example:7475");
    hub.node_monitor.record_success(
        "gpu-box",
        "gpu-box.example:7475",
        metrics_with_gpus(vec![
            gpu_json(0, 100, 1000, false),
            gpu_json(1, 900, 1000, false),
        ]),
    );
    let (addr, server) = serve(hub).await;

    let (status, busy) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"gpu-box:gpu1", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &busy["code"]),
        (400, &json!("failed_precondition")),
        "{busy}"
    );
    let message = busy["message"].as_str().unwrap();
    assert!(
        message.contains(
            "node gpu-box gpu 1 is busy: 90% of its VRAM is in use (running containers: ollama)"
        ),
        "{message}"
    );
    assert!(
        message.contains("protectorate msg send orchestrator"),
        "{message}"
    );
    assert!(message.contains("resource unavailable:"), "{message}");

    let (status, node) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"gpu-box", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &node["code"]),
        (400, &json!("failed_precondition")),
        "{node}"
    );
    assert!(
        node["message"]
            .as_str()
            .unwrap()
            .contains("node gpu-box gpu 1 is busy: 90% of its VRAM is in use"),
        "{node}"
    );

    let (status, free) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"gpu-box:gpu0", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{free}");
    assert_eq!(free["resource"], "gpu-box:gpu0");
    server.abort();
}

#[tokio::test]
async fn busy_integrated_gpu_does_not_block_whole_node_or_desk() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    add_shared(&hub, "gpu-box", "gpu-box.example:7475");
    hub.node_monitor.record_success(
        "gpu-box",
        "gpu-box.example:7475",
        metrics_with_gpus(vec![
            gpu_json(0, 100, 1000, false),
            gpu_json(1, 900, 1000, true),
        ]),
    );
    let (addr, server) = serve(hub).await;

    let (status, node) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"gpu-box", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{node}");
    assert_eq!(node["resource"], "gpu-box");
    let (status, _) = call(addr, RELEASE, "owner-secret", json!({"id":node["id"]})).await;
    assert_eq!(status, 200);

    let (status, explicit) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"gpu-box:gpu1", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &explicit["code"]),
        (400, &json!("failed_precondition")),
        "{explicit}"
    );
    assert!(
        explicit["message"]
            .as_str()
            .unwrap()
            .contains("gpu 1 is busy"),
        "{explicit}"
    );

    let (status, desk) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"desk:gpu-box", "holder":"desk-user", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{desk}");
    assert_eq!(desk["resource"], "desk:gpu-box");
    server.abort();
}

#[tokio::test]
async fn default_chooser_keeps_freshness_tier_and_holders() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    add_shared(&hub, "peer", "peer.example:7475");
    hub.inventory
        .add(
            Node {
                name: "care-box".into(),
                address: "care.example:7475".into(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.node_monitor.record_success(
        "care-box",
        "care.example:7475",
        metrics_with_gpus(vec![gpu_json(0, 100, 1000, false)]),
    );
    let (addr, server) = serve(hub).await;

    let (status, stale) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &stale["code"]),
        (400, &json!("failed_precondition")),
        "{stale}"
    );
    assert!(
        stale["message"]
            .as_str()
            .unwrap()
            .contains("node peer has no fresh successful metrics poll"),
        "{stale}"
    );

    let (status, care) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"care-box", "holder":"client", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &care["code"]),
        (400, &json!("failed_precondition")),
        "{care}"
    );
    assert!(
        care["message"]
            .as_str()
            .unwrap()
            .contains("node care-box is not available for leases"),
        "{care}"
    );
    server.abort();

    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    hub.node_monitor.record_success(
        "peer",
        "peer.example:7475",
        metrics_with_gpus(vec![gpu_json(0, 100, 1000, false)]),
    );
    let (addr, server) = serve(hub).await;
    let (status, first) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"first", "ttl":"20s"}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let (status, held) = call(
        addr,
        ACQUIRE,
        "owner-secret",
        json!({"resource":"peer", "holder":"other", "ttl":"20s"}),
    )
    .await;
    assert_eq!(
        (status, &held["code"]),
        (400, &json!("failed_precondition")),
        "{held}"
    );
    assert!(
        held["message"]
            .as_str()
            .unwrap()
            .contains("peer held by first"),
        "{held}"
    );
    server.abort();
}
