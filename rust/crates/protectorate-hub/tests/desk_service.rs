use axum::{routing::post, Json, Router};
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_inventory::CARE;
use protectorate_lease::{self as leases, Chooser};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, String, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        head.into(),
        serde_json::from_str(body).unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn care_node_allows_explicit_desk_lease_but_not_compute_lease() {
    let node_router = Router::new()
        .route(
            "/protectorate.v1.NodeAgentService/GetMetrics",
            post(|| async { Json(json!({"os":"windows","arch":"amd64"})) }),
        )
        .route(
            "/protectorate.v1.NodeAgentService/DeskAction",
            post(|| async { Json(json!({"input":{}})) }),
        )
        .route(
            "/protectorate.v1.NodeAgentService/GetDeskState",
            post(|| async { Json(json!({"available":true,"detail":"ready"})) }),
        );
    let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_addr = node_listener.local_addr().unwrap();
    let node_task = tokio::spawn(async move {
        axum::serve(node_listener, node_router).await.unwrap();
    });
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "care-peer".into(),
                address: node_addr.to_string(),
                os: "windows".into(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.node_monitor.poll_node("care-peer");
    assert!(hub
        .node_monitor
        .is_fresh("care-peer", &node_addr.to_string()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let (status, _, compute) = call(
        addr,
        "LeaseService/AcquireLease",
        json!({"resource":"care-peer","holder":"test","ttl":"60s"}),
    )
    .await;
    assert_eq!(status, 400, "{compute}");
    let (status, _, desk) = call(
        addr,
        "LeaseService/AcquireLease",
        json!({"resource":"desk:care-peer","holder":"test","ttl":"60s"}),
    )
    .await;
    assert_eq!(status, 200, "{desk}");
    let (status, _, input) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"care-peer","leaseId":desk["id"]},"text":"hello"}),
    )
    .await;
    assert_eq!(status, 200, "{input}");
    let mut projected = None;
    for _ in 0..40 {
        let (code, _, node) = call(addr, "NodeService/GetNode", json!({"name":"care-peer"})).await;
        assert_eq!(code, 200, "{node}");
        if node["desk"]["available"] == true {
            projected = Some(node);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let projected = projected.expect("background desk refresh should fill node status");
    assert_eq!(projected["desk"]["detail"], "ready");
    assert_eq!(projected["deskHolder"], "test");
    assert_eq!(projected["deskInputs"][0]["count"], 1);
    hub_task.abort();
    node_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desk_actions_require_and_manage_leases_and_forward_to_node() {
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = Arc::clone(&calls);
    let node_router = Router::new()
        .route("/protectorate.v1.NodeAgentService/DeskAction", post(move |Json(body): Json<Value>| {
            let seen = Arc::clone(&seen);
            async move {
                seen.lock().unwrap().push(body.clone());
                let mut headers = axum::http::HeaderMap::new();
                if body.get("type").is_some() && body["type"]["text"] == "refuse" {
                    headers.insert("Protectorate-Desk-Unavailable", "DESK_REASON_USER_ACTIVE".parse().unwrap());
                    return (axum::http::StatusCode::BAD_REQUEST, headers, Json(json!({"code":"failed_precondition","message":"user active"})));
                }
                if body.get("type").is_some() && body["type"]["text"] == "fail" {
                    return (axum::http::StatusCode::BAD_REQUEST, headers, Json(json!({"code":"failed_precondition","message":"node rejected input"})));
                }
                let reply = if body.get("screenshot").is_some() { json!({"screenshot":{"png":"AQID","width":2,"height":1}}) }
                    else if body.get("run").is_some() { json!({"run":{"pid":123}}) }
                    else { json!({"input":{}}) };
                (axum::http::StatusCode::OK, headers, Json(reply))
            }
        }))
        .route("/protectorate.v1.NodeAgentService/GetDeskState", post(|| async {
            Json(json!({"reason":"DESK_REASON_USER_ACTIVE", "detail":"user active"}))
        }));
    let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_addr = node_listener.local_addr().unwrap();
    let node_task = tokio::spawn(async move {
        axum::serve(node_listener, node_router).await.unwrap();
    });

    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_addr.to_string(),
                os: "windows".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let chooser: Chooser = Arc::new(|request, blocked| {
        let resource = leases::parse_resource(request)?;
        if blocked(&resource).is_some() {
            return Err(leases::Error::Busy("occupied".into()));
        }
        Ok(resource)
    });
    let hub = hub.with_lease_chooser(chooser).unwrap();
    let leases = Arc::clone(&hub.leases);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });

    let (code, _, shot) = call(
        addr,
        "DeskService/Screenshot",
        json!({"target":{"name":"peer"}}),
    )
    .await;
    assert_eq!(code, 200, "{shot}");
    assert_eq!(shot["png"], "AQID");
    let (code, headers, typed) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer"}, "text":"hello"}),
    )
    .await;
    assert_eq!(code, 200, "{typed}");
    let lease_id = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("protectorate-desk-lease: ")
                .map(str::to_owned)
        })
        .unwrap();
    assert!(headers
        .to_ascii_lowercase()
        .contains("protectorate-desk-lease-expires:"));
    let (code, headers, run) = call(
        addr,
        "DeskService/Run",
        json!({"target":{"name":"peer"}, "program":"demo"}),
    )
    .await;
    assert_eq!(code, 200, "{run}");
    assert_eq!(run["pid"], 123);
    assert!(headers.contains(&lease_id));
    let (code, _, status) = call(addr, "NodeService/GetDeskPolicy", json!({"name":"peer"})).await;
    assert_eq!(code, 200, "{status}");
    assert_eq!(status["inputs"][0]["count"], 2);
    let (code, _, node_status) = call(addr, "NodeService/GetNode", json!({"name":"peer"})).await;
    assert_eq!(code, 200, "{node_status}");
    assert_eq!(node_status["desk"], status["state"]);
    assert_eq!(node_status["deskInputs"], status["inputs"]);
    assert_eq!(node_status["deskHolder"], status["holder"]);
    let (code, _, listed) = call(addr, "NodeService/ListNodes", json!({})).await;
    assert_eq!(code, 200, "{listed}");
    let listed_peer = listed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["node"]["name"] == "peer")
        .unwrap();
    assert_eq!(listed_peer["desk"], status["state"]);
    assert_eq!(listed_peer["deskInputs"], status["inputs"]);
    assert_eq!(listed_peer["deskHolder"], status["holder"]);

    let (code, _, denied) = call(
        addr,
        "DeskService/Click",
        json!({"target":{"name":"peer", "leaseId":"missing"}, "x":1, "y":1}),
    )
    .await;
    assert_eq!(code, 404, "{denied}");
    let (code, _, denied) = call(
        addr,
        "DeskService/Click",
        json!({"target":{"name":"peer", "leaseId":lease_id}, "x":1, "y":1}),
    )
    .await;
    assert_eq!(code, 200, "{denied}");
    let now = chrono::Utc::now().timestamp_millis();
    leases.release(&lease_id, now).unwrap();
    let foreign = leases
        .acquire(
            "other-user",
            std::time::Duration::from_secs(60),
            "desk:peer",
            now,
        )
        .unwrap();
    let (code, _, held) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer"}, "text":"held"}),
    )
    .await;
    assert_eq!(code, 400, "{held}");
    assert_eq!(held["code"], "failed_precondition");
    leases
        .release(&foreign.id, chrono::Utc::now().timestamp_millis())
        .unwrap();
    let (code, _, denied) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer"}, "text":"fail"}),
    )
    .await;
    assert_ne!(code, 200, "{denied}");
    assert!(leases
        .blocked(
            &leases::parse_resource("desk:peer").unwrap(),
            chrono::Utc::now().timestamp_millis()
        )
        .unwrap()
        .is_none());
    assert_eq!(calls.lock().unwrap().len(), 5);

    let manual = leases
        .acquire(
            "manual-holder",
            std::time::Duration::from_secs(60),
            "desk:peer",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    let (code, _, _) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer", "leaseId":manual.id}, "text":"fail"}),
    )
    .await;
    assert_eq!(code, 400);
    assert!(leases
        .blocked(
            &leases::parse_resource("desk:peer").unwrap(),
            chrono::Utc::now().timestamp_millis()
        )
        .unwrap()
        .is_none());
    let manual = leases
        .acquire(
            "manual-holder",
            std::time::Duration::from_secs(60),
            "desk:peer",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    let (code, _, refused) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer", "leaseId":manual.id}, "text":"refuse"}),
    )
    .await;
    assert_eq!(code, 400, "{refused}");
    assert!(leases
        .blocked(
            &leases::parse_resource("desk:peer").unwrap(),
            chrono::Utc::now().timestamp_millis()
        )
        .unwrap()
        .is_none());
    assert_eq!(calls.lock().unwrap().len(), 7);

    let policy = json!({"window":"00:00-23:59", "idle":"60s"});
    let (code, _, saved) = call(
        addr,
        "NodeService/SetDeskPolicy",
        json!({"name":"peer", "policy":policy}),
    )
    .await;
    assert_eq!(code, 200, "{saved}");
    assert_eq!(saved["state"]["reason"], "DESK_REASON_USER_ACTIVE");
    let (code, _, blocked) = call(
        addr,
        "DeskService/Type",
        json!({"target":{"name":"peer"}, "text":"blocked"}),
    )
    .await;
    assert_eq!(code, 400, "{blocked}");
    assert_eq!(blocked["code"], "failed_precondition");
    let (code, _, blocked) = call(
        addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:peer", "holder":"manual-holder", "ttl":"60s"
        }),
    )
    .await;
    assert_eq!(code, 400, "{blocked}");
    assert_eq!(blocked["code"], "failed_precondition");
    assert!(leases
        .blocked(
            &leases::parse_resource("desk:peer").unwrap(),
            chrono::Utc::now().timestamp_millis()
        )
        .unwrap()
        .is_none());
    assert_eq!(calls.lock().unwrap().len(), 7);
    hub_task.abort();
    node_task.abort();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(home.path().join("desk-use.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
    let reopened = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, reopened.router().unwrap())
            .await
            .unwrap();
    });
    let (code, _, persisted) =
        call(addr, "NodeService/GetDeskPolicy", json!({"name":"peer"})).await;
    assert_eq!(code, 200, "{persisted}");
    assert_eq!(persisted["inputs"][0]["count"], 3);
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watcher_revokes_unavailable_desk_and_notifies_sleeve_holder() {
    let available = Arc::new(AtomicBool::new(true));
    let reported = Arc::clone(&available);
    let node_router = Router::new()
        .route("/protectorate.v1.NodeAgentService/GetMetrics", post(|| async {
            Json(json!({"os":"windows","arch":"amd64"}))
        }))
        .route("/protectorate.v1.NodeAgentService/GetDeskState", post(move || {
            let reported = Arc::clone(&reported);
            async move {
                if reported.load(Ordering::SeqCst) {
                    Json(json!({"available":true,"detail":"ready"}))
                } else {
                    Json(json!({"reason":"DESK_REASON_USER_ACTIVE","detail":"physical input returned"}))
                }
            }
        }));
    let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_addr = node_listener.local_addr().unwrap();
    let node_task =
        tokio::spawn(async move { axum::serve(node_listener, node_router).await.unwrap() });

    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "win-desk".into(),
                address: node_addr.to_string(),
                os: "windows".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Shell,
                Mode::Terminal,
                home.path(),
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    hub.sleeves
        .record_exit(
            "worker",
            "2026-01-01T00:00:01Z".into(),
            None,
            0,
            false,
            false,
        )
        .unwrap();
    hub.node_monitor.poll_node("win-desk");
    let leases = Arc::clone(&hub.leases);
    let mail = Arc::clone(&hub.mail);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hub_task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (code, _, policy) = call(
        addr,
        "NodeService/SetDeskPolicy",
        json!({
            "name":"win-desk", "policy":{"window":"00:00-23:59","idle":"60s"}
        }),
    )
    .await;
    assert_eq!(code, 200, "{policy}");
    assert_eq!(policy["state"]["available"], true);
    let (code, _, lease) = call(
        addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:win-desk", "holder":"auto:token:sleeve:worker", "ttl":"60s"
        }),
    )
    .await;
    assert_eq!(code, 200, "{lease}");
    let (code, _, queued) = call(
        addr,
        "LeaseService/AcquireLease",
        json!({
            "resource":"desk:win-desk", "holder":"next-holder", "ttl":"60s", "wait":true
        }),
    )
    .await;
    assert_eq!(code, 200, "{queued}");
    assert_eq!(queued["state"], "LEASE_STATE_QUEUED");
    available.store(false, Ordering::SeqCst);

    let resource = leases::parse_resource("desk:win-desk").unwrap();
    let mut revoked = false;
    for _ in 0..160 {
        let now = chrono::Utc::now().timestamp_millis();
        if leases.blocked(&resource, now).unwrap().is_none() {
            revoked = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(revoked, "watcher did not revoke the active desk lease");
    assert!(
        matches!(
            leases
                .get(
                    queued["id"].as_str().unwrap(),
                    chrono::Utc::now().timestamp_millis()
                )
                .unwrap(),
            leases::Entry::Queued { .. }
        ),
        "queued desktop lease must not be granted while unavailable"
    );
    let mut notice = None;
    for _ in 0..40 {
        notice = mail
            .pending()
            .get("worker")
            .and_then(|rows| rows.first())
            .cloned();
        if notice.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let notice = notice.expect("revoked sleeve holder should receive a notice");
    assert_eq!(notice.kind, protectorate_mailbox::Kind::NOTICE);
    assert!(notice.body.contains("physical input returned"));
    assert!(notice.body.contains(lease["id"].as_str().unwrap()));
    hub_task.abort();
    node_task.abort();
}
