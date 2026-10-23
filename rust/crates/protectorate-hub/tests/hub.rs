use protectorate_auth::Method as ScopeMethod;
use protectorate_directory::{Manifest, Scan, Workspace};
use protectorate_hub::Hub;
use protectorate_session::Manager as SessionManager;
use protectorate_wire::Catalog;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

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

async fn read_stream_event(stream: &mut TcpStream) -> Value {
    let line = read_line(stream).await;
    let size = usize::from_str_radix(line.trim().split(';').next().unwrap(), 16).unwrap();
    assert!(size >= 5);
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
async fn status_and_backlog_use_real_identity_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let catalog = Catalog::embedded().unwrap();
    let methods: Vec<_> = catalog
        .methods()
        .map(|m| ScopeMethod {
            tool: &m.tool,
            service: &m.service,
            read_only: m.read_only,
            owner_only: m.owner_only,
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let reader = hub
        .identity
        .create(
            "reader",
            &["read"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap();
    let reader_secret = reader.secret.expose().to_string();
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
    let sessions = Arc::new(SessionManager::new(&socket));
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_sessions(Arc::clone(&sessions));
    let catalog = Catalog::embedded().unwrap();
    let methods: Vec<_> = catalog
        .methods()
        .map(|m| ScopeMethod {
            tool: &m.tool,
            service: &m.service,
            read_only: m.read_only,
            owner_only: m.owner_only,
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let reader = hub
        .identity
        .create(
            "reader",
            &["read"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let create = "/protectorate.v1.SessionService/CreateSession";
    let list = "/protectorate.v1.SessionService/ListSessions";
    let capture = "/protectorate.v1.SessionService/CapturePane";
    let kill = "/protectorate.v1.SessionService/KillSession";
    let input = json!({"name":"job", "command":["sh", "-c", "printf 'hello\\n'; exit 7"]});
    let (code, body) = call(addr, create, reader.secret.expose(), input.clone()).await;
    assert_eq!((code, &body["code"]), (403, &json!("permission_denied")));
    let (code, body) = call(addr, create, "owner-secret", input).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["session"]["name"], "job");
    let (code, body) = call(addr, list, reader.secret.expose(), json!({})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["sessions"][0]["name"], "job");
    let (code, body) = call(
        addr,
        capture,
        "owner-secret",
        json!({"name":"job","lines":1}),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    assert!(body["text"].as_str().unwrap().contains("hello"));
    let (code, body) = call(addr, kill, "owner-secret", json!({"name":"job"})).await;
    assert_eq!(code, 200, "{body}");
    server.abort();
    let _ = std::process::Command::new("tmux")
        .args(["-L", &socket, "kill-server"])
        .output();
}

#[tokio::test]
async fn event_stream_starts_with_heartbeat_and_delivers_note_change() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!("POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    assert!(read_line(&mut stream).await.contains("200"));
    loop {
        if read_line(&mut stream).await == "\r\n" {
            break;
        }
    }
    let first = tokio::time::timeout(Duration::from_secs(2), read_stream_event(&mut stream))
        .await
        .unwrap();
    assert_eq!(first["kind"], "heartbeat");
    let (code, created) = call(
        addr,
        "/protectorate.v1.BacklogService/CreateNote",
        "owner-secret",
        json!({"title":"streamed"}),
    )
    .await;
    assert_eq!(code, 200);
    let event = tokio::time::timeout(Duration::from_secs(2), read_stream_event(&mut stream))
        .await
        .unwrap();
    assert_eq!(event["kind"], "note.created");
    assert_eq!(event["subject"], created["note"]["id"]);
    assert_eq!(event["payload"]["title"], "streamed");
    drop(stream);
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
    let update = "/protectorate.v1.TokenService/UpdateTokenScopes";
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
    assert!(secret.starts_with("ptk_"));
    assert!(created["token"]["expiresAt"].is_string());
    let (code, _) = call(addr, list, secret, json!({})).await;
    assert_eq!(code, 403);
    let (code, body) = call(addr, status, secret, json!({})).await;
    assert_eq!(code, 200, "{body}");
    let (code, body) = call(
        addr,
        update,
        "owner-secret",
        json!({"id":id,"add":["backlog_create_note"],"remove":["read"]}),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["token"]["scopes"], json!(["backlog_create_note"]));
    let (code, _) = call(addr, status, secret, json!({})).await;
    assert_eq!(code, 403);
    let (code, body) = call(
        addr,
        "/protectorate.v1.BacklogService/CreateNote",
        secret,
        json!({"title":"token note"}),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    let (code, listed) = call(addr, list, "owner-secret", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(listed["tokens"][0]["id"], id);
    assert!(listed["tokens"][0].get("secret").is_none());
    let (code, body) = call(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 200, "{body}");
    assert!(body["token"]["revokedAt"].is_string());
    let (code, _) = call(
        addr,
        "/protectorate.v1.BacklogService/CreateNote",
        secret,
        json!({"title":"denied"}),
    )
    .await;
    assert_eq!(code, 401);
    server.abort();
}

#[tokio::test]
async fn loopback_device_pairing_approves_once_and_revokes() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_loopback_device_auth();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let start = "/protectorate.v1.DeviceService/StartDeviceAuth";
    let poll = "/protectorate.v1.DeviceService/PollDeviceAuth";
    let pending = "/protectorate.v1.DeviceService/ListDeviceAuth";
    let approve = "/protectorate.v1.DeviceService/ApproveDeviceAuth";
    let devices = "/protectorate.v1.DeviceService/ListDevices";
    let rename = "/protectorate.v1.DeviceService/RenameDevice";
    let revoke = "/protectorate.v1.DeviceService/RevokeDevice";
    let (code, started) = call(
        addr,
        start,
        "",
        json!({"name":"phone", "platform":"android"}),
    )
    .await;
    assert_eq!(code, 200, "{started}");
    let device_code = started["deviceCode"].as_str().unwrap();
    let user_code = started["userCode"].as_str().unwrap();
    let (code, body) = call(addr, pending, "owner-secret", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(body["requests"][0]["userCode"], user_code);
    let (code, body) = call(addr, approve, "owner-secret", json!({"userCode":user_code})).await;
    assert_eq!(code, 200, "{body}");
    let (code, approved) = call(addr, poll, "", json!({"deviceCode":device_code})).await;
    assert_eq!(code, 200, "{approved}");
    assert_eq!(approved["status"], "approved");
    let secret = approved["secret"].as_str().unwrap();
    let id = approved["token"]["id"].as_str().unwrap();
    let (code, body) = call(addr, poll, "", json!({"deviceCode":device_code})).await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "consumed");
    assert!(body.get("secret").is_none());
    let (code, body) = call(addr, devices, secret, json!({})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["devices"][0]["id"], id);
    let (code, body) = call(
        addr,
        rename,
        "owner-secret",
        json!({"id":id,"name":"new phone"}),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["token"]["name"], "new phone");
    let (code, body) = call(addr, revoke, "owner-secret", json!({"id":id})).await;
    assert_eq!(code, 200, "{body}");
    let (code, _) = call(addr, devices, secret, json!({})).await;
    assert_eq!(code, 401);
    server.abort();
}

#[tokio::test]
async fn device_pairing_is_disabled_without_loopback_admission() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (code, body) = call(
        addr,
        "/protectorate.v1.DeviceService/StartDeviceAuth",
        "",
        json!({"name":"phone","platform":"android"}),
    )
    .await;
    assert_eq!(code, 403, "{body}");
    server.abort();
}

#[tokio::test]
async fn tls_pairing_fails_closed_without_transport_peer() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_tls_device_auth();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (code, body) = call(
        addr,
        "/protectorate.v1.DeviceService/StartDeviceAuth",
        "",
        json!({"name":"phone","platform":"android"}),
    )
    .await;
    assert_eq!(code, 403, "{body}");
    server.abort();
}

#[tokio::test]
async fn directory_rpc_resolves_project_edges_and_sleeve_filter() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let scan = |sleeve: &str, repo: &str, name: &str, deps: Vec<&str>| Scan {
        sleeve: sleeve.into(),
        workspace: Workspace {
            path: dir.path().join(repo),
            remote: format!("github.com/acme/{repo}"),
            branch: "main".into(),
            head: "abc".into(),
            manifests: vec![Manifest {
                kind: "cargo".into(),
                path: "Cargo.toml".into(),
                name: name.into(),
                deps: deps.into_iter().map(str::to_owned).collect(),
            }],
        },
    };
    hub.directory
        .update(&[
            scan("builder", "app", "cargo:app", vec!["cargo:lib"]),
            scan("library", "lib", "cargo:lib", vec![]),
        ])
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let list = "/protectorate.v1.DirectoryService/ListProjects";
    let get = "/protectorate.v1.DirectoryService/GetProject";
    let (code, body) = call(addr, list, "owner-secret", json!({"sleeve":"builder"})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["projects"].as_array().unwrap().len(), 1);
    assert_eq!(body["projects"][0]["id"], "github.com/acme/app");
    assert_eq!(
        body["projects"][0]["dependsOn"][0]["project"],
        "github.com/acme/lib"
    );
    assert_eq!(body["projects"][0]["dependsOn"][0]["display"], "acme/lib");
    let (code, body) = call(addr, get, "owner-secret", json!({"project":"lib"})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(
        body["project"]["dependents"][0]["project"],
        "github.com/acme/app"
    );
    let (code, body) = call(addr, get, "owner-secret", json!({"project":"missing"})).await;
    assert_eq!(code, 404, "{body}");
    server.abort();
}
