use protectorate_hub::{Hub, TailnetStatusSource};
use protectorate_tailnet_map::{Netmap, Node};
use protectorate_tailnet_packets::PeerTraffic;
use protectorate_tailnet_runtime::{MapSnapshot, StatusSnapshot};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    (address, task)
}

async fn get(address: std::net::SocketAddr) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let path = "/protectorate.v1.TailnetService/GetStatus";
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}");
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
async fn status_uses_only_the_connected_map_and_rejects_disabled_tailnet() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let slot = hub.tailnet_status_slot();
    let (address, task) = serve(hub).await;

    let (code, disabled) = get(address).await;
    assert_eq!(code, 400, "{disabled}");
    assert_eq!(disabled["code"], "failed_precondition");

    *slot.write().unwrap() = Some(Arc::new(|| None));
    let (code, disconnected) = get(address).await;
    assert_eq!(code, 200, "{disconnected}");
    assert_eq!(disconnected["backendState"], "Disconnected");
    assert!(disconnected.get("self").is_none());
    assert!(disconnected.get("peers").is_none());

    let self_node: Node = serde_json::from_value(json!({
        "ID":1, "Key":"nodekey:self", "Name":"hub.example.test.",
        "Addresses":["100.64.0.1/32", "fd7a:115c:a1e0::1/128"]
    }))
    .unwrap();
    let peer: Node = serde_json::from_value(json!({
        "ID":2, "Key":"nodekey:peer", "Name":"peer.example.test.",
        "Addresses":["100.64.0.2/32", "10.0.0.0/8"],
        "Tags":["tag:server"], "Online":false,
        "HomeDERP":42, "Endpoints":["192.0.2.2:1234"],
        "LastSeen":"2026-10-22T03:04:05Z"
    }))
    .unwrap();
    let mut map = Netmap::default();
    map.self_node = Some(self_node);
    map.peers.insert(peer.id, peer);
    let snapshot = MapSnapshot {
        generation: 7,
        map: Arc::new(map),
    };
    let mut traffic = BTreeMap::new();
    let mut record = PeerTraffic::default();
    record.key = "nodekey:peer".into();
    record.last_handshake = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(42));
    record.direct_addr = Some("192.0.2.2:1234".parse().unwrap());
    record.rx_bytes = 123;
    record.tx_bytes = 456;
    traffic.insert(2, record);
    let source: TailnetStatusSource = Arc::new(move || {
        Some(StatusSnapshot {
            map: snapshot.clone(),
            traffic: traffic.clone(),
        })
    });
    *slot.write().unwrap() = Some(source);

    let (code, running) = get(address).await;
    assert_eq!(code, 200, "{running}");
    assert_eq!(running["backendState"], "Running");
    assert_eq!(running["self"]["name"], "hub.example.test");
    assert_eq!(
        running["self"]["ips"],
        json!(["100.64.0.1", "fd7a:115c:a1e0::1"])
    );
    assert_eq!(running["peers"][0]["name"], "peer.example.test");
    assert_eq!(running["peers"][0]["ips"], json!(["100.64.0.2"]));
    assert_eq!(running["peers"][0]["tags"], json!(["tag:server"]));
    assert!(running["peers"][0].get("online").is_none());
    assert_eq!(running["peers"][0]["directAddr"], "192.0.2.2:1234");
    assert_eq!(running["peers"][0]["lastSeen"], "2026-10-22T03:04:05Z");
    assert_eq!(running["peers"][0]["lastHandshake"], "1970-01-01T00:00:42Z");
    assert_eq!(running["peers"][0]["rxBytes"], "123");
    assert_eq!(running["peers"][0]["txBytes"], "456");
    assert!(running["peers"][0].get("relay").is_none());
    task.abort();
}
