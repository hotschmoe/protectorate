use protectorate_hub::Hub;
use protectorate_lease::{parse_resource, Chooser, Error};
use serde_json::{json, Value};
use std::sync::Arc;
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
