use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_node_monitor::{Monitor, Timing};
use protectorate_release::{Info, Source};
use protectorate_tailnet_admin::Admin;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
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

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invite_node_issues_key_reinvites_and_expires_previous_key() {
    use axum::{
        body::Bytes,
        routing::{get, post},
    };
    use std::sync::{atomic::AtomicUsize, Mutex};
    let home = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let issued = Arc::new(AtomicUsize::new(0));
    let issued_for_create = Arc::clone(&issued);
    let calls_for_create = Arc::clone(&calls);
    let calls_for_expire = Arc::clone(&calls);
    let admin_router = axum::Router::new()
        .route(
            "/api/v1/user",
            get(|| async {
                (
                    [("content-type", "application/json")],
                    r#"{"users":[{"id":"42","name":"protectorate"}]}"#,
                )
            }),
        )
        .route(
            "/api/v1/node",
            get(|| async { ([("content-type", "application/json")], r#"{"nodes":[]}"#) }),
        )
        .route(
            "/api/v1/preauthkey",
            post(move |body: Bytes| {
                let calls = Arc::clone(&calls_for_create);
                let issued = Arc::clone(&issued_for_create);
                async move {
                    let payload: Value = serde_json::from_slice(&body).unwrap();
                    calls.lock().unwrap().push(json!({"created":payload}));
                    let id = issued.fetch_add(1, Ordering::SeqCst) + 80;
                    (
                        [("content-type", "application/json")],
                        json!({"preAuthKey":{"id":id.to_string(),"key":format!("authkey{id}")}})
                            .to_string(),
                    )
                }
            }),
        )
        .route(
            "/api/v1/preauthkey/expire",
            post(move |body: Bytes| {
                let calls = Arc::clone(&calls_for_expire);
                async move {
                    let payload: Value = serde_json::from_slice(&body).unwrap();
                    calls.lock().unwrap().push(json!({"expired":payload}));
                    ([("content-type", "application/json")], "{}")
                }
            }),
        );
    let admin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin_url = format!("http://{}", admin_listener.local_addr().unwrap());
    let admin_server =
        tokio::spawn(async move { axum::serve(admin_listener, admin_router).await.unwrap() });
    let mut admin = Admin::new(&admin_url, "api-secret", Duration::from_secs(3)).unwrap();
    admin.ensure_policy_user().unwrap();
    let source = Arc::new(
        Source::new(
            "owner/repo",
            "stable",
            "",
            home.path().join("releases").to_str().unwrap(),
        )
        .unwrap(),
    );
    let info = Info {
        version: "v1-invite-test".into(),
        tag: "dev".into(),
        ..Default::default()
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let server_asset = format!("protectorate-rs-linux-{arch}");
    let helper_asset = format!("protectorate-node-sandbox-helper-linux-{arch}");
    let android_asset = "protectorate-rs-android-arm64";
    let real_binary = std::env::var_os("PROTECTORATE_TEST_RUST_NODE_BIN");
    let server_bytes = real_binary
        .as_ref()
        .map(fs::read)
        .transpose()
        .unwrap()
        .unwrap_or_else(|| b"#!/bin/sh\nprintf 'rust bundle: %s\\n' \"$*\"\n".to_vec());
    let helper_bytes = b"#!/bin/sh\nexit 0\n";
    let android_bytes = b"#!/bin/sh\nprintf 'termux bundle: %s\\n' \"$*\"\n";
    let sums = format!(
        "{:x}  {server_asset}\n{:x}  {helper_asset}\n{:x}  {android_asset}\n",
        Sha256::digest(&server_bytes),
        Sha256::digest(helper_bytes),
        Sha256::digest(android_bytes),
    );
    let release_dir = source.store(&info, sums.as_bytes()).unwrap();
    fs::write(release_dir.join(&server_asset), &server_bytes).unwrap();
    fs::write(release_dir.join(&helper_asset), helper_bytes).unwrap();
    fs::write(release_dir.join(android_asset), android_bytes).unwrap();
    source.activate(&info.version).unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test")
        .unwrap()
        .with_release_source(source)
        .with_tailnet_admin(Arc::new(admin))
        .with_tailnet_pools(vec!["build".into()]);
    let (addr, server) = serve(hub).await;
    let path = "/protectorate.v1.NodeService/InviteNode";
    for bad in [
        json!({"name":"Bad","installUrl":"https://example.test"}),
        json!({"name":"worker","tier":"admin","installUrl":"https://example.test"}),
        json!({"name":"worker","ttl":"0s","installUrl":"https://example.test"}),
        json!({"name":"worker","installUrl":"https://user@example.test"}),
    ] {
        let (status, body) = call(addr, path, "owner-secret", bad).await;
        assert_eq!(status, 400, "{body}");
    }
    assert!(calls.lock().unwrap().is_empty());
    let (status, first) = call(addr, path, "owner-secret", json!({"name":"worker","tier":"care","pools":["build"],"ttl":"90s","installUrl":"https://example.test"})).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["key"], "authkey80");
    assert_eq!(first["node"]["invited"], true);
    assert_eq!(first["node"]["inviteKeyId"], "80");
    assert_eq!(first["node"]["tags"], json!(["pool-build"]));
    assert_eq!(first["controlUrl"], admin_url);
    let commands = first["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 3);
    for (command, platform, shell, path) in [
        (
            &commands[0],
            "INSTALL_PLATFORM_WINDOWS",
            "PowerShell",
            "install.ps1",
        ),
        (&commands[1], "INSTALL_PLATFORM_LINUX", "sh", "install.sh"),
        (
            &commands[2],
            "INSTALL_PLATFORM_TERMUX",
            "Termux",
            "install-termux.sh",
        ),
    ] {
        assert_eq!(command["platform"], platform);
        assert_eq!(command["shell"], shell);
        let line = command["command"].as_str().unwrap();
        assert!(
            line.contains(&format!("https://example.test/{path}")),
            "{line}"
        );
        assert_eq!(line.matches("authkey80").count(), 1, "{line}");
        assert!(line.contains("worker"), "{line}");
    }
    let (status, second) = call(
        addr,
        path,
        "owner-secret",
        json!({"name":"worker","installUrl":"https://example.test"}),
    )
    .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["node"]["inviteKeyId"], "81");
    assert!(second["commands"][0]["command"]
        .as_str()
        .unwrap()
        .contains("authkey81"));
    assert!(!second["commands"][0]["command"]
        .as_str()
        .unwrap()
        .contains("authkey80"));
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(
        calls[0]["created"]["aclTags"],
        json!(["tag:node", "tag:tier-care", "tag:pool-build"])
    );
    assert_eq!(
        calls[1]["created"]["aclTags"],
        json!(["tag:node", "tag:tier-shared"])
    );
    assert_eq!(calls[2]["expired"]["id"], "80");
    let (status, third) = call(
        addr,
        path,
        "owner-secret",
        json!({"name":"worker","installUrl":format!("http://{addr}")}),
    )
    .await;
    assert_eq!(status, 200, "{third}");
    assert_eq!(third["key"], "authkey82");
    let line = third["commands"][1]["command"].as_str().unwrap();
    assert!(line.contains("/install.sh"), "{line}");
    let output = tokio::process::Command::new("sh")
        .args(["-c", &format!("{line} --dry-run")])
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    if real_binary.is_some() {
        assert!(
            stdout.contains("dry run: no files or services changed"),
            "{stdout}"
        );
    } else {
        assert_eq!(
            stdout.trim(),
            format!("rust bundle: node install --control {admin_url} --name worker --dry-run")
        );
    }
    let cache = home.path().join("cache/protectorate-rust");
    let staged_count = || {
        fs::read_dir(&cache)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("node-install.")
            })
            .count()
    };
    assert_eq!(staged_count(), 1);
    if real_binary.is_none() {
        let repeat = tokio::process::Command::new("sh")
            .args(["-c", &format!("{line} --dry-run")])
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .output()
            .await
            .unwrap();
        assert!(
            repeat.status.success(),
            "{}",
            String::from_utf8_lossy(&repeat.stderr)
        );
        assert_eq!(staged_count(), 1);
    }
    let fake_bin = home.path().join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    let uname = fake_bin.join("uname");
    fs::write(&uname, b"#!/bin/sh\nprintf 'aarch64\\n'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&uname, fs::Permissions::from_mode(0o755)).unwrap();
    let termux_line = third["commands"][2]["command"].as_str().unwrap();
    assert!(termux_line.contains("/install-termux.sh"), "{termux_line}");
    let termux_output = tokio::process::Command::new("sh")
        .args(["-c", &format!("{termux_line} --dry-run")])
        .env("PREFIX", home.path().join("com.termux/files/usr"))
        .env(
            "PATH",
            format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .output()
        .await
        .unwrap();
    assert!(
        termux_output.status.success(),
        "{}",
        String::from_utf8_lossy(&termux_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&termux_output.stdout).trim(),
        format!("termux bundle: node install --control {admin_url} --name worker --dry-run")
    );
    server.abort();
    admin_server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tier_change_retags_tailnet_first_and_keeps_inventory_on_retag_failure() {
    use axum::{
        body::Bytes,
        http::StatusCode,
        routing::{get, post},
    };
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: "peer".into(),
                tier: "shared".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let inventory = Arc::clone(&hub.inventory);
    let reject = Arc::new(AtomicBool::new(false));
    let blocked = Arc::clone(&reject);
    let admin_router = axum::Router::new()
        .route(
            "/api/v1/node",
            get(|| async {
                (
                    [("content-type", "application/json")],
                    r#"{"nodes":[{"id":"7","name":"peer","tags":["tag:node","tag:tier-shared"]}]}"#,
                )
            }),
        )
        .route(
            "/api/v1/node/7/tags",
            post(move |body: Bytes| {
                let inventory = Arc::clone(&inventory);
                let reject = Arc::clone(&blocked);
                async move {
                    let tags: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        inventory.get("peer").unwrap().tier,
                        if reject.load(Ordering::SeqCst) {
                            "care"
                        } else {
                            "shared"
                        }
                    );
                    assert!(tags["tags"].as_array().unwrap().iter().any(|tag| tag
                        == if reject.load(Ordering::SeqCst) {
                            "tag:tier-shared"
                        } else {
                            "tag:tier-care"
                        }));
                    if reject.load(Ordering::SeqCst) {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            [("content-type", "application/json")],
                            "{}",
                        )
                    } else {
                        (StatusCode::OK, [("content-type", "application/json")], "{}")
                    }
                }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin_url = format!("http://{}", listener.local_addr().unwrap());
    let admin_server =
        tokio::spawn(async move { axum::serve(listener, admin_router).await.unwrap() });
    let admin = Arc::new(Admin::new(&admin_url, "api-secret", Duration::from_secs(3)).unwrap());
    let (addr, server) = serve(hub.with_tailnet_admin(admin)).await;
    let procedure = "/protectorate.v1.NodeService/SetNodeTier";
    let (status, changed) = call(
        addr,
        procedure,
        "owner-secret",
        json!({"name":"peer","tier":"care"}),
    )
    .await;
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["tier"], "care");
    reject.store(true, Ordering::SeqCst);
    let (status, refused) = call(
        addr,
        procedure,
        "owner-secret",
        json!({"name":"peer","tier":"shared"}),
    )
    .await;
    assert_ne!(status, 200, "{refused}");
    let (_, still_care) = call(
        addr,
        "/protectorate.v1.NodeService/GetNode",
        "owner-secret",
        json!({"name":"peer"}),
    )
    .await;
    assert_eq!(still_care["node"]["tier"], "care");
    server.abort();
    admin_server.abort();
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
    let agent = Agent::new(Arc::new(MockMetrics(Arc::clone(&failure))), "node-secret");
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
    monitor.set_token("wrong-secret".into()).unwrap();
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
    let (status, denied) = call(hub_addr, get, "owner-secret", json!({"name":"peer"})).await;
    assert_eq!(status, 200, "{denied}");
    assert_eq!(denied["freshness"], "NODE_FRESHNESS_UNREACHABLE");
    assert!(
        denied["error"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("unauthenticated"),
        "{denied}"
    );
    monitor.set_token("node-secret".into()).unwrap();
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
                invite_key_id: 42,
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
    assert!(joined["node"]["inviteKeyId"].is_null());
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
