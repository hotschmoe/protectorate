use protectorate_hub::Hub;
use protectorate_inventory::Node;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const GET: &str = "/protectorate.v1.NodeService/GetDeskPolicy";
const SET: &str = "/protectorate.v1.NodeService/SetDeskPolicy";

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    (address, task)
}

async fn call(address: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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
async fn desk_policy_is_validated_owner_only_and_persists_without_claiming_runtime_state() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let local = hub.hostname.clone();
    for (name, os) in [("linux-peer", "linux"), ("windows-peer", "windows")] {
        hub.inventory
            .add(
                Node {
                    name: name.into(),
                    address: "127.0.0.1:9".into(),
                    os: os.into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
    }
    let reader = hub
        .identity
        .create(
            "reader",
            &["read"],
            Duration::from_secs(60),
            chrono::Utc::now().timestamp(),
            &[],
        )
        .unwrap();
    let reader_secret = reader.secret.expose().to_owned();
    let (address, task) = serve(hub).await;

    let (status, initial) = call(address, GET, "owner-secret", json!({"name":"linux-peer"})).await;
    assert_eq!(status, 200, "{initial}");
    assert_eq!(initial["state"]["reason"], "DESK_REASON_UNSUPPORTED");
    assert!(initial.get("policy").is_none());
    let (status, windows) =
        call(address, GET, "owner-secret", json!({"name":"windows-peer"})).await;
    assert_eq!(status, 200, "{windows}");
    assert_eq!(windows["state"]["reason"], "DESK_REASON_UNKNOWN");

    let policy = json!({"window":"19:00-07:00", "idle":"900s", "timezone":"Asia/Shanghai", "lockAfter":true});
    let (status, denied) = call(
        address,
        SET,
        &reader_secret,
        json!({"name":"linux-peer", "policy":policy}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    let (status, saved) = call(
        address,
        SET,
        "owner-secret",
        json!({"name":"linux-peer", "policy":policy}),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["policy"], policy);
    assert_eq!(saved["state"]["reason"], "DESK_REASON_UNSUPPORTED");

    for invalid in [
        json!({"window":"bad", "idle":"900s", "timezone":"Asia/Shanghai"}),
        json!({"window":"19:00-07:00", "idle":"0s", "timezone":"Asia/Shanghai"}),
        json!({"window":"19:00-07:00", "idle":"86401s", "timezone":"Asia/Shanghai"}),
        json!({"window":"19:00-07:00", "idle":"900s", "timezone":"Not/AZone"}),
    ] {
        let (status, response) = call(
            address,
            SET,
            "owner-secret",
            json!({"name":"linux-peer", "policy":invalid}),
        )
        .await;
        assert_eq!(status, 400, "{response}");
    }
    let (status, unknown) = call(address, GET, "owner-secret", json!({"name":"missing"})).await;
    assert_eq!(status, 404, "{unknown}");
    if !local.is_empty() {
        let (status, own) = call(
            address,
            SET,
            "owner-secret",
            json!({"name":local, "policy":policy}),
        )
        .await;
        assert_eq!(status, 400, "{own}");
        assert_eq!(own["code"], "failed_precondition");
    }
    task.abort();

    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let (address, task) = serve(hub).await;
    let (status, persisted) =
        call(address, GET, "owner-secret", json!({"name":"linux-peer"})).await;
    assert_eq!(status, 200, "{persisted}");
    assert_eq!(persisted["policy"], policy);
    let (status, cleared) = call(address, SET, "owner-secret", json!({"name":"linux-peer"})).await;
    assert_eq!(status, 200, "{cleared}");
    assert!(cleared.get("policy").is_none());
    task.abort();
}
