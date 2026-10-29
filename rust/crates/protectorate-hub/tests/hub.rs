use crate::support::capability;
#[cfg(unix)]
use crate::support::private_tmux;
use protectorate_hub::Hub;
use protectorate_session::Manager as SessionManager;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    call_as(addr, path, &format!("Bearer {token}"), body).await
}

async fn call_as(
    addr: std::net::SocketAddr,
    path: &str,
    authorization: &str,
    body: Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: {authorization}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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
async fn status_and_backlog_use_real_identity_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let (_, reader_secret) = capability::mint(&hub, "reader", &["read"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let status = "/protectorate.v1.HubService/GetStatus";
    let create = "/protectorate.v1.BacklogService/CreateNote";
    let list = "/protectorate.v1.BacklogService/ListNotes";
    let update = "/protectorate.v1.BacklogService/UpdateNote";
    let delete = "/protectorate.v1.BacklogService/DeleteNote";
    let (code, body) = call(addr, status, "owner-secret", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(body["version"], "v-rust");
    let runtime = "/protectorate.v1.HubService/GetSleeveRuntime";
    let (code, body) = call(addr, runtime, "owner-secret", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(body["runtime"].as_str().unwrap_or(""), "");
    let (code, body) = call(addr, runtime, "wrong", json!({})).await;
    assert_eq!(code, 401);
    assert_eq!(body["code"], "unauthenticated");
    let (code, body) = call(addr, status, "wrong", json!({})).await;
    assert_eq!(code, 401);
    assert_eq!(body["code"], "unauthenticated");
    let (code, body) = call(addr, create, &reader_secret, json!({"title":"denied"})).await;
    assert_eq!(code, 403);
    assert_eq!(body["code"], "permission_denied");
    let (code, body) = call(
        addr,
        create,
        "owner-secret",
        json!({"title":"first", "body":"text", "pinned":true}),
    )
    .await;
    assert_eq!(code, 200);
    let id = body["note"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["note"]["source"], "owner");
    let (code, body) = call(addr, list, &reader_secret, json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(body["notes"].as_array().unwrap().len(), 1);
    let (code, body) = call(
        addr,
        update,
        "owner-secret",
        json!({"id":id,"state":"done","resolution":"finished"}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(body["note"]["completion"]["doneBy"], "owner");
    let (code, body) = call(addr, delete, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 200);
    assert_eq!(body, json!({}));
    server.abort();
    let reopened = protectorate_backlog::Store::new(dir.path());
    assert!(reopened
        .list(protectorate_backlog::Filter {
            state: "all".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn session_rpc_uses_isolated_tmux_and_owner_grant() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = format!("protectorate-hub-test-{}", std::process::id());
    let _tmux = private_tmux::PrivateTmux::new(&socket);
    let sessions = Arc::new(SessionManager::new(&socket));
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_sessions(Arc::clone(&sessions));
    let (_, reader) = capability::mint(&hub, "reader", &["read"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let create = "/protectorate.v1.SessionService/CreateSession";
    let list = "/protectorate.v1.SessionService/ListSessions";
    let capture = "/protectorate.v1.SessionService/CapturePane";
    let kill = "/protectorate.v1.SessionService/KillSession";
    // The command outlives the capture: tmux drops pty output still in flight
    // when a pane's process exits.
    let input = json!({"name":"job", "command":["sh", "-c", "printf 'hello\\n'; exec sleep 600"]});
    let (code, body) = call(addr, create, &reader, input.clone()).await;
    assert_eq!((code, &body["code"]), (403, &json!("permission_denied")));
    let (code, body) = call(addr, create, "owner-secret", input).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["session"]["name"], "job");
    let (code, body) = call(addr, list, &reader, json!({})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["sessions"][0]["name"], "job");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let (code, body) = call(
            addr,
            capture,
            "owner-secret",
            json!({"name":"job","lines":1}),
        )
        .await;
        assert_eq!(code, 200, "{body}");
        if body["text"].as_str().unwrap().contains("hello") {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{body}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (code, body) = call(addr, kill, "owner-secret", json!({"name":"job"})).await;
    assert_eq!(code, 200, "{body}");
    server.abort();
}

#[tokio::test]
async fn owner_manages_scoped_tokens_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let create = "/protectorate.v1.TokenService/CreateToken";
    let list = "/protectorate.v1.TokenService/ListTokens";
    let renew = "/protectorate.v1.TokenService/RenewToken";
    let revoke = "/protectorate.v1.TokenService/RevokeToken";
    let status = "/protectorate.v1.HubService/GetStatus";
    let (code, created) = call(
        addr,
        create,
        "owner-secret",
        json!({"name":"reader","scopes":["read"],"ttl":"3600s"}),
    )
    .await;
    assert_eq!(code, 200, "{created}");
    let secret = created["secret"].as_str().unwrap();
    let id = created["token"]["id"].as_str().unwrap();
    assert!(secret.starts_with("pc1."));
    assert!(created["token"]["expiresAt"].is_string());
    let (code, _) = call(addr, list, secret, json!({})).await;
    assert_eq!(code, 403);
    let (code, body) = call(addr, status, secret, json!({})).await;
    assert_eq!(code, 200, "{body}");
    let (code, body) = call(
        addr,
        "/protectorate.v1.BacklogService/CreateNote",
        secret,
        json!({"title":"token note"}),
    )
    .await;
    assert_eq!(code, 403, "{body}");
    let (code, renewed) = call(addr, renew, secret, json!({})).await;
    assert_eq!(code, 200, "{renewed}");
    assert_eq!(renewed["token"]["id"], id);
    let fresh = renewed["secret"].as_str().unwrap();
    let (code, body) = call(addr, status, fresh, json!({})).await;
    assert_eq!(code, 200, "{body}");
    let (code, body) = call(addr, renew, "owner-secret", json!({})).await;
    assert_eq!(code, 403, "{body}");
    let (code, listed) = call(addr, list, "owner-secret", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(listed["tokens"][0]["id"], id);
    assert!(listed["tokens"][0].get("secret").is_none());
    let (code, body) = call(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 200, "{body}");
    assert!(body["token"]["revokedAt"].is_string());
    for token in [secret, fresh] {
        let (code, body) = call(addr, status, token, json!({})).await;
        assert_eq!(code, 401, "{body}");
    }
    server.abort();
}

#[tokio::test]
async fn devices_are_bearer_tokens_the_owner_and_devices_manage() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let (_, everything) = capability::mint(&hub, "script", &["*"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let add = "/protectorate.v1.DeviceService/AddDevice";
    let devices = "/protectorate.v1.DeviceService/ListDevices";
    let rename = "/protectorate.v1.DeviceService/RenameDevice";
    let revoke = "/protectorate.v1.DeviceService/RevokeDevice";
    for name in ["", "envoy", "orchestrator-envoy", "sleeve:worker", "a\nb"] {
        let (code, body) = call(addr, add, "owner-secret", json!({"name":name})).await;
        assert_eq!(code, 400, "{name:?}: {body}");
    }
    let (code, body) = call(addr, add, &everything, json!({"name":"script phone"})).await;
    assert_eq!(code, 403, "a capability added a device: {body}");
    let (code, added) = call(addr, add, "owner-secret", json!({"name":"phone"})).await;
    assert_eq!(code, 200, "{added}");
    let id = added["device"]["id"].as_str().unwrap();
    let token = added["token"].as_str().unwrap();
    assert_eq!(id.len(), 16);
    assert!(token.starts_with("pd1."), "{added}");
    assert_eq!(added["device"]["addedBy"], "owner");
    let (code, body) = call(addr, devices, token, json!({})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["devices"][0]["id"], id);
    assert!(body["devices"][0].get("token").is_none(), "{body}");
    let (code, body) = call(addr, devices, &everything, json!({})).await;
    assert_eq!(code, 403, "{body}");
    // Names need not be unique; ids are.
    let (code, twin) = call(addr, add, token, json!({"name":"phone"})).await;
    assert_eq!(code, 200, "{twin}");
    assert_ne!(twin["device"]["id"], id);
    assert_eq!(twin["device"]["addedBy"], "token:phone");
    let (code, body) = call(addr, rename, token, json!({"id":id,"name":"new phone"})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["device"]["name"], "new phone");
    let (code, body) = call(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 200, "{body}");
    let (code, body) = call(addr, devices, token, json!({})).await;
    assert_eq!(code, 401, "{body}");
    let (code, body) = call(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 404, "{body}");
    let twin_token = twin["token"].as_str().unwrap();
    let (code, body) = call(addr, devices, twin_token, json!({})).await;
    assert_eq!(code, 200, "{body}");
    server.abort();
}
