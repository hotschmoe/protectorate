use crate::support::capability;
use protectorate_hub::Hub;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call_path(
    addr: std::net::SocketAddr,
    path: &str,
    token: &str,
    body: Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
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
        serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.into())),
    )
}

async fn call(addr: std::net::SocketAddr, token: &str, body: Value) -> (u16, Value) {
    call_path(
        addr,
        "/protectorate.v1.AuditService/ListAuditEntries",
        token,
        body,
    )
    .await
}

#[tokio::test]
async fn device_changes_are_audited_by_id_without_their_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let add = "/protectorate.v1.DeviceService/AddDevice";
    let rename = "/protectorate.v1.DeviceService/RenameDevice";
    let revoke = "/protectorate.v1.DeviceService/RevokeDevice";
    let (status, added) = call_path(addr, add, "owner-secret", json!({"name":"laptop"})).await;
    assert_eq!(status, 200, "{added}");
    let id = added["device"]["id"].as_str().unwrap();
    let token = added["token"].as_str().unwrap();
    let (status, invalid) = call_path(addr, add, "owner-secret", json!({"name":""})).await;
    assert_eq!(status, 400, "{invalid}");
    let forged = format!("pd1.{}", "A".repeat(43));
    let (status, denied) = call_path(addr, add, &forged, json!({"name":"intruder"})).await;
    assert_eq!(status, 401, "{denied}");
    let (status, renamed) = call_path(
        addr,
        rename,
        token,
        json!({"id":id,"name":"laptop renamed"}),
    )
    .await;
    assert_eq!(status, 200, "{renamed}");
    let (status, revoked) = call_path(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(status, 200, "{revoked}");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let text = audit.to_string();
    assert!(
        !text.contains(&token[4..]),
        "a device token reached the audit log"
    );
    assert!(
        !text.contains(&forged[4..]),
        "a presented token reached the audit log"
    );
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5, "{audit}");
    let entry = |procedure: &str, outcome: &str| {
        entries
            .iter()
            .find(|entry| entry["procedure"] == procedure && entry["outcome"] == outcome)
            .unwrap_or_else(|| panic!("no {outcome} {procedure}: {audit}"))
    };
    let added = entry(add, "ok");
    assert_eq!(added["actor"], "owner");
    assert_eq!(added["subject"], id);
    assert_eq!(added["detail"], "name=laptop");
    assert_eq!(entry(add, "error")["code"], "invalid_argument");
    let denied = entry(add, "denied");
    assert_eq!(denied["actor"], "unauthenticated");
    assert_eq!(denied["code"], "unauthenticated");
    let renamed = entry(rename, "ok");
    assert_eq!(renamed["actor"], "token:laptop");
    assert_eq!(renamed["tokenId"], id);
    assert_eq!(renamed["subject"], id);
    assert_eq!(entry(revoke, "ok")["subject"], id);
    server.abort();
}

#[tokio::test]
async fn chat_selection_requires_explicit_write_scope_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let (_, sleeve_secret) = capability::mint(&hub, "sleeve:worker", &["read", "chat_report"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let create = "/protectorate.v1.TokenService/CreateToken";
    let selection = "/protectorate.v1.ChatService/SetSelection";
    let sleeve_secret = sleeve_secret.as_str();
    let (status, denied) = call_path(
        addr,
        selection,
        sleeve_secret,
        json!({"sleeve":"worker", "selection":{"model":"a"}}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    assert!(denied["message"]
        .as_str()
        .unwrap()
        .contains("chat_set_selection"));
    let (status, controller) = call_path(
        addr,
        create,
        "owner-secret",
        json!({"name":"model-controller", "scopes":["chat_set_selection"], "ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{controller}");
    let controller_secret = controller["secret"].as_str().unwrap();
    let (status, reached_handler) = call_path(
        addr,
        selection,
        controller_secret,
        json!({"sleeve":"missing-chat", "selection":{"model":"a"}}),
    )
    .await;
    assert_eq!(status, 404, "{reached_handler}");
    let (status, audit) = call(
        addr,
        "owner-secret",
        json!({"procedure":"ChatService/SetSelection"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{audit}");
    assert_eq!(entries[0]["actor"], "token:model-controller");
    assert_eq!(entries[0]["outcome"], "error");
    assert_eq!(entries[1]["actor"], "token:sleeve:worker");
    assert_eq!(entries[1]["outcome"], "denied");
    server.abort();
}
