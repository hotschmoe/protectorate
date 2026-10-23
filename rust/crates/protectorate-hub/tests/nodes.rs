use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_node_monitor::{Monitor, Timing};
use protectorate_release::{Info, Source};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_tailnet_admin::Admin;
use protectorate_wire::{Code, RpcError};
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
    let agent = Agent::new(
        Arc::new(MockMetrics(Arc::new(AtomicBool::new(false)))),
        "node-secret",
    );
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
                invite_key_id: 42,
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
    monitor.set_token("node-secret".into()).unwrap();
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
        node["error"] == "awaiting first poll"
            || node["error"] == "node agent token is not configured",
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
    let (status, invited) = call(addr, add, "owner-secret", json!({"node":{"name":"pending","address":"pending", "invited":true, "inviteKeyId":"18446744073709551615"}})).await;
    assert_eq!(status, 200, "{invited}");
    assert_eq!(invited["inviteKeyId"], "18446744073709551615");
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
