use crate::support::capability;
use protectorate_hub::Hub;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// A well-formed P-256 point; pairing stores it without verifying a proof.
const DEVICE_KEY: &str =
    "BAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";

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
async fn public_pairing_audits_start_without_recording_poll_codes() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_loopback_device_auth();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            hub.router()
                .unwrap()
                .into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let start = "/protectorate.v1.DeviceService/StartDeviceAuth";
    let poll = "/protectorate.v1.DeviceService/PollDeviceAuth";
    let (status, started) = call_path(
        addr,
        start,
        "",
        json!({"name":"audit-applicant", "platform":"linux", "publicKey":DEVICE_KEY}),
    )
    .await;
    assert_eq!(status, 200, "{started}");
    let device_code = started["deviceCode"].as_str().unwrap();
    let (status, pending) = call_path(addr, poll, "", json!({"deviceCode":device_code})).await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["status"], "pending");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{audit}");
    assert_eq!(entries[0]["procedure"], start);
    assert_eq!(entries[0]["actor"], "unauthenticated");
    assert_eq!(entries[0]["subject"].as_str().unwrap_or(""), "");
    assert_eq!(entries[0]["outcome"], "ok");
    assert_eq!(entries[0]["detail"], "requesting_ip=127.0.0.1");
    assert!(!audit.to_string().contains(device_code));
    let user_code = started["userCode"].as_str().unwrap();
    let (status, approved) = call_path(
        addr,
        "/protectorate.v1.DeviceService/ApproveDeviceAuth",
        "owner-secret",
        json!({"userCode":user_code}),
    )
    .await;
    assert_eq!(status, 200, "{approved}");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{audit}");
    let approval = entries
        .iter()
        .find(|entry| entry["procedure"] == "/protectorate.v1.DeviceService/ApproveDeviceAuth")
        .unwrap();
    assert_eq!(approval["actor"], "owner");
    assert_eq!(approval["subject"], "audit-applicant");
    assert_eq!(approval["outcome"], "ok");
    assert!(!audit.to_string().contains(device_code));
    assert!(!audit.to_string().contains(user_code));
    let (status, started) = call_path(
        addr,
        start,
        "",
        json!({"name":"denied-applicant", "platform":"linux", "publicKey":DEVICE_KEY}),
    )
    .await;
    assert_eq!(status, 200, "{started}");
    let denied_code = started["userCode"].as_str().unwrap();
    let (status, denied) = call_path(
        addr,
        "/protectorate.v1.DeviceService/DenyDeviceAuth",
        "owner-secret",
        json!({"userCode":denied_code}),
    )
    .await;
    assert_eq!(status, 200, "{denied}");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4, "{audit}");
    let denial = entries
        .iter()
        .find(|entry| entry["procedure"] == "/protectorate.v1.DeviceService/DenyDeviceAuth")
        .unwrap();
    assert_eq!(denial["subject"], "denied-applicant");
    assert_eq!(denial["outcome"], "ok");
    assert!(!audit.to_string().contains(denied_code));
    assert!(!audit
        .to_string()
        .contains(started["deviceCode"].as_str().unwrap()));
    let (status, invalid) =
        call_path(addr, start, "", json!({"name":"", "platform":"linux"})).await;
    assert_eq!(status, 400, "{invalid}");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5, "{audit}");
    let failed_start = entries
        .iter()
        .find(|entry| entry["procedure"] == start && entry["outcome"] == "error")
        .unwrap();
    assert_eq!(failed_start["detail"], "requesting_ip=127.0.0.1");
    assert_eq!(failed_start["subject"].as_str().unwrap_or(""), "");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let (status, issued) = call_path(addr, poll, "", json!({"deviceCode":device_code})).await;
    assert_eq!(status, 200, "{issued}");
    let token_id = issued["device"]["id"].as_str().unwrap();
    let rename = "/protectorate.v1.DeviceService/RenameDevice";
    let (status, renamed) = call_path(
        addr,
        rename,
        "owner-secret",
        json!({"id":token_id,"name":"audit-renamed"}),
    )
    .await;
    assert_eq!(status, 200, "{renamed}");
    let revoke = "/protectorate.v1.DeviceService/RevokeDevice";
    let (status, revoked) = call_path(addr, revoke, "owner-secret", json!({"id":token_id})).await;
    assert_eq!(status, 200, "{revoked}");
    let (status, audit) = call(addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 7, "{audit}");
    for procedure in [rename, revoke] {
        let entry = entries
            .iter()
            .find(|entry| entry["procedure"] == procedure)
            .unwrap();
        assert_eq!(entry["subject"], token_id);
        assert_eq!(entry["outcome"], "ok");
    }
    server.abort();
}

#[tokio::test]
async fn pairing_rate_limit_is_audited_as_an_error_with_source_ip() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_loopback_device_auth();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            hub.router()
                .unwrap()
                .into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let start = "/protectorate.v1.DeviceService/StartDeviceAuth";
    for index in 0..10 {
        let (status, response) = call_path(
            addr,
            start,
            "",
            json!({"name": format!("pair-{index}"), "platform": "linux", "publicKey": DEVICE_KEY}),
        )
        .await;
        assert_eq!(status, 200, "{response}");
    }
    let (status, blocked) = call_path(
        addr,
        start,
        "",
        json!({"name":"pair-blocked", "platform":"linux", "publicKey":DEVICE_KEY}),
    )
    .await;
    assert_eq!(status, 429, "{blocked}");
    let (status, audit) = call(
        addr,
        "owner-secret",
        json!({"procedure":"DeviceService/StartDeviceAuth"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 11, "{audit}");
    let failure = &entries[0];
    assert_eq!(failure["actor"], "unauthenticated");
    assert_eq!(failure["subject"].as_str().unwrap_or_default(), "");
    assert_eq!(failure["outcome"], "error");
    assert_eq!(failure["code"], "resource_exhausted");
    assert_eq!(failure["detail"], "requesting_ip=127.0.0.1");
    assert_eq!(entries[1]["outcome"], "ok");
    assert!(!audit.to_string().contains("pair-blocked"));
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
