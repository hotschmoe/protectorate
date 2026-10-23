use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::TailnetIdentity;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io, sync::Arc, time::Duration};
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
    let content_type = if path == "/protectorate.v1.NodeService/GetFile" {
        "application/connect+json"
    } else {
        "application/json"
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
async fn audit_records_trusted_tailnet_identity_and_leaves_loopback_blank() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let router = hub.router().unwrap();
    let ordinary_router = router.clone();
    let ordinary_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ordinary_addr = ordinary_listener.local_addr().unwrap();
    let ordinary = tokio::spawn(async move {
        axum::serve(ordinary_listener, ordinary_router)
            .await
            .unwrap()
    });
    let tailnet_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tailnet_addr = tailnet_listener.local_addr().unwrap();
    let trusted = router.layer(axum::middleware::from_fn(
        |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
            request
                .extensions_mut()
                .insert(TailnetIdentity("client.example".into()));
            next.run(request).await
        },
    ));
    let tailnet =
        tokio::spawn(async move { axum::serve(tailnet_listener, trusted).await.unwrap() });
    let create = "/protectorate.v1.BacklogService/CreateNote";
    let (status, first) = call_path(
        tailnet_addr,
        create,
        "owner-secret",
        json!({"title":"tailnet-note"}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let (status, second) = call_path(
        ordinary_addr,
        create,
        "owner-secret",
        json!({"title":"loopback-note"}),
    )
    .await;
    assert_eq!(status, 200, "{second}");
    let (status, _) = call_path(
        tailnet_addr,
        create,
        "invalid-token",
        json!({"title":"denied-note"}),
    )
    .await;
    assert_eq!(status, 401);
    let (status, audit) = call(ordinary_addr, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{audit}");
    assert_eq!(entries[0]["actor"], "unauthenticated");
    assert_eq!(entries[0]["outcome"], "denied");
    assert_eq!(entries[0]["tailnet"], "client.example");
    assert_eq!(entries[1]["actor"], "owner");
    assert_eq!(entries[1]["tailnet"].as_str().unwrap_or_default(), "");
    assert_eq!(entries[2]["actor"], "owner");
    assert_eq!(entries[2]["tailnet"], "client.example");
    ordinary.abort();
    tailnet.abort();
}

#[tokio::test]
async fn grant_mint_and_scope_denial_each_have_one_audit_entry_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let grant = "/protectorate.v1.GrantService/Admin";
    let (status, issued) = call_path(
        addr,
        grant,
        "owner-secret",
        json!({"node":" Peer ","task":" repair peer ","ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{issued}");
    assert_eq!(
        issued["token"]["scopes"],
        json!(["job_*", "node_admin:peer"])
    );
    let (status, reader) = call_path(
        addr,
        "/protectorate.v1.TokenService/CreateToken",
        "owner-secret",
        json!({"name":"grant-audit-reader","scopes":["read"],"ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{reader}");
    let (status, denied) = call_path(
        addr,
        grant,
        reader["secret"].as_str().unwrap(),
        json!({"node":"peer","task":"attempt escalation"}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");

    let (status, audit) = call(
        addr,
        "owner-secret",
        json!({"procedure":"GrantService/Admin"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{audit}");
    assert_eq!(entries[0]["outcome"], "denied");
    assert_eq!(entries[0]["actor"], "token:grant-audit-reader");
    assert_eq!(entries[1]["outcome"], "ok");
    assert_eq!(entries[1]["subject"], "peer");
    assert_eq!(entries[1]["detail"], "task: repair peer");
    assert!(!audit
        .to_string()
        .contains(issued["secret"].as_str().unwrap()));
    assert!(!audit
        .to_string()
        .contains(reader["secret"].as_str().unwrap()));
    server.abort();
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
        json!({"name":"audit-applicant", "platform":"linux"}),
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
        json!({"name":"denied-applicant", "platform":"linux"}),
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
    let token_id = issued["token"]["id"].as_str().unwrap();
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
            json!({"name": format!("pair-{index}"), "platform": "linux"}),
        )
        .await;
        assert_eq!(status, 200, "{response}");
    }
    let (status, blocked) = call_path(
        addr,
        start,
        "",
        json!({"name":"pair-blocked", "platform":"linux"}),
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let create = "/protectorate.v1.TokenService/CreateToken";
    let selection = "/protectorate.v1.ChatService/SetSelection";
    let (status, sleeve) = call_path(
        addr,
        create,
        "owner-secret",
        json!({"name":"sleeve:worker", "scopes":["read", "chat_report"], "ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{sleeve}");
    let sleeve_secret = sleeve["secret"].as_str().unwrap();
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

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

#[tokio::test]
async fn node_file_download_is_audited_without_logging_its_path_or_contents() {
    let home = tempfile::tempdir().unwrap();
    let files = home.path().join("node-files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(
        files.join("private-report.txt"),
        b"SECRET-DOWNLOAD-CONTENTS",
    )
    .unwrap();
    let node = Agent::new(Arc::new(Metrics), "node-secret")
        .with_files(&files, false)
        .unwrap();
    let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_address = node_listener.local_addr().unwrap();
    let node_task = tokio::spawn(async move {
        axum::serve(node_listener, node.router().unwrap())
            .await
            .unwrap();
    });

    let hub = Hub::open(home.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_address.to_string(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let hub_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_address = hub_listener.local_addr().unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(hub_listener, hub.router().unwrap())
            .await
            .unwrap();
    });

    let chunks = tokio::task::spawn_blocking(move || {
        let client = Client::new(
            &format!("http://{hub_address}"),
            "owner-secret",
            Duration::from_secs(5),
        )
        .unwrap();
        let mut chunks = Vec::new();
        client
            .stream(
                "/protectorate.v1.NodeService/GetFile",
                &json!({"name":"peer", "path":"private-report.txt", "noCompress":true}),
                |part| {
                    chunks.push(part);
                    Ok(())
                },
            )
            .unwrap();
        chunks
    })
    .await
    .unwrap();
    assert!(chunks.len() >= 2);

    let (status, denied) = call_path(
        hub_address,
        "/protectorate.v1.NodeService/GetFile",
        "wrong-token",
        json!({"name":"peer", "path":"private-report.txt"}),
    )
    .await;
    assert_eq!(status, 200, "{denied}");
    assert!(denied.as_str().unwrap().contains("unauthenticated"));

    let (status, audit) = call(
        hub_address,
        "owner-secret",
        json!({"procedure":"NodeService/GetFile"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{audit}");
    assert_eq!(entries[0]["actor"], "unauthenticated");
    assert_eq!(entries[0]["outcome"], "denied");
    assert_eq!(entries[1]["actor"], "owner");
    assert_eq!(entries[1]["subject"], "peer");
    assert_eq!(entries[1]["outcome"], "ok");
    assert!(!audit.to_string().contains("private-report.txt"));
    assert!(!audit.to_string().contains("SECRET-DOWNLOAD-CONTENTS"));
    hub_task.abort();
    node_task.abort();
}

#[tokio::test]
async fn file_upload_audits_first_last_and_failed_chunks_without_payload() {
    let home = tempfile::tempdir().unwrap();
    let files = home.path().join("node-files");
    let node = Agent::new(Arc::new(Metrics), "node-secret")
        .with_files(&files, false)
        .unwrap();
    let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_address = node_listener.local_addr().unwrap();
    let node_task = tokio::spawn(async move {
        axum::serve(node_listener, node.router().unwrap())
            .await
            .unwrap();
    });

    let hub = Hub::open(home.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_address.to_string(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let hub_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_address = hub_listener.local_addr().unwrap();
    let hub_task = tokio::spawn(async move {
        axum::serve(hub_listener, hub.router().unwrap())
            .await
            .unwrap();
    });

    let procedure = "/protectorate.v1.NodeService/PutFile";
    let (status, failed) = call_path(
        hub_address,
        procedure,
        "owner-secret",
        json!({
            "name":"peer", "transferId":"audit-upload-001", "offset":"4",
            "chunk":{"data":"ZGVm", "rawSize":3}
        }),
    )
    .await;
    assert_ne!(status, 200, "{failed}");

    let first = json!({
        "name":"peer", "transferId":"audit-upload-001", "path":"private-upload",
        "kind":"FILE_KIND_FILE", "mode":384, "offset":"0",
        "chunk":{"data":"YWJj", "rawSize":3}
    });
    let (status, started) = call_path(hub_address, procedure, "owner-secret", first).await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["received"], "3");

    let (status, middle) = call_path(
        hub_address,
        procedure,
        "owner-secret",
        json!({
            "name":"peer", "transferId":"audit-upload-001", "offset":"3",
            "chunk":{"data":"ZGVm", "rawSize":3}
        }),
    )
    .await;
    assert_eq!(status, 200, "{middle}");
    assert_eq!(middle["received"], "6");

    let (status, finished) = call_path(
        hub_address,
        procedure,
        "owner-secret",
        json!({
            "name":"peer", "transferId":"audit-upload-001", "offset":"6",
            "chunk":{"data":"Z2hp", "rawSize":3}, "last":true,
            "trailer":{"size":"9", "sha256":format!("{:x}", Sha256::digest(b"abcdefghi"))}
        }),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["done"], true);
    assert_eq!(
        std::fs::read(files.join("private-upload")).unwrap(),
        b"abcdefghi"
    );

    let (status, audit) = call(
        hub_address,
        "owner-secret",
        json!({"procedure":"NodeService/PutFile"}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let entries = audit["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{audit}");
    assert_eq!(entries[0]["outcome"], "ok");
    assert_eq!(entries[1]["outcome"], "ok");
    assert_eq!(entries[2]["outcome"], "error");
    for entry in entries {
        assert_eq!(entry["subject"], "peer");
    }
    assert!(!audit.to_string().contains("private-upload"));
    assert!(!audit.to_string().contains("YWJj"));
    assert!(!audit.to_string().contains("ZGVm"));
    assert!(!audit.to_string().contains("Z2hp"));
    hub_task.abort();
    node_task.abort();
}
