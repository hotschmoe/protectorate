use protectorate_auth::Method as ScopeMethod;
use protectorate_hub::Hub;
use protectorate_inventory::Node;
use protectorate_wire::Catalog;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    call_approved(addr, path, token, body, "").await
}

async fn call_approved(
    addr: std::net::SocketAddr,
    path: &str,
    token: &str,
    body: Value,
    approval: &str,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nProtectorate-Approval: {approval}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
async fn owner_mints_bounded_task_grant_and_grant_cannot_mint_another() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let admin = "/protectorate.v1.GrantService/Admin";
    let (status, issued) = call(
        addr,
        admin,
        "owner-secret",
        json!({"node":"Peer", "task":"repair peer", "ttl":"3600s"}),
    )
    .await;
    assert_eq!(status, 200, "{issued}");
    assert_eq!(issued["token"]["task"], "repair peer");
    assert_eq!(issued["token"]["mintedBy"], "owner");
    assert_eq!(
        issued["token"]["scopes"],
        json!(["job_*", "node_admin:peer"])
    );
    let secret = issued["secret"].as_str().unwrap();
    let (status, denied) = call(
        addr,
        admin,
        secret,
        json!({"node":"peer", "task":"escalate"}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    let (_status, _job) = call(
        addr,
        "/protectorate.v1.JobService/CancelJob",
        secret,
        json!({"id":"missing"}),
    )
    .await;
    let (status, jobs) = call(
        addr,
        "/protectorate.v1.JobService/ListJobs",
        secret,
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{jobs}");
    let (status, denied_read) = call(
        addr,
        "/protectorate.v1.NodeService/ListNodes",
        secret,
        json!({}),
    )
    .await;
    assert_eq!(status, 403, "{denied_read}");
    let (status, audit) = call(
        addr,
        "/protectorate.v1.AuditService/ListAuditEntries",
        "owner-secret",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |entry| entry["procedure"] == "/protectorate.v1.JobService/CancelJob"
                && entry["task"] == "repair peer"
        ));
    assert!(audit["entries"].as_array().unwrap().iter().any(|entry| {
        entry["procedure"] == "/protectorate.v1.JobService/ListJobs"
            && entry["task"] == "repair peer"
            && entry["outcome"] == "ok"
    }));
    assert!(audit["entries"].as_array().unwrap().iter().any(|entry| {
        entry["procedure"] == "/protectorate.v1.NodeService/ListNodes"
            && entry["task"] == "repair peer"
            && entry["outcome"] == "denied"
    }));
    let (status, listed) = call(
        addr,
        "/protectorate.v1.TokenService/ListTokens",
        "owner-secret",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["tokens"][0]["task"], "repair peer");
    assert!(!listed.to_string().contains(secret));
    let (status, invalid) = call(
        addr,
        admin,
        "owner-secret",
        json!({"node":"peer", "task":"repair peer", "ttl":"90000s"}),
    )
    .await;
    assert_eq!(status, 400, "{invalid}");
    server.abort();
}

#[tokio::test]
async fn orchestrator_can_grant_only_named_shared_nodes_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_orchestrator_admin_grants();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: "peer:7475".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.inventory
        .add(
            Node {
                name: "care".into(),
                address: "care:7475".into(),
                tier: "care".into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let catalog = Catalog::embedded().unwrap();
    let methods: Vec<_> = catalog
        .methods()
        .map(|method| ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let issued = hub
        .identity
        .create(
            "orchestrator-envoy",
            &["grant_admin"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap();
    let secret = issued.secret.expose().to_string();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let admin = "/protectorate.v1.GrantService/Admin";
    let (status, pending) = call(
        addr,
        admin,
        &secret,
        json!({"node":"peer", "task":"fix peer"}),
    )
    .await;
    assert_eq!(status, 409, "{pending}");
    let id = pending["details"][0]["debug"]["id"].as_str().unwrap();
    let (status, decided) = call(
        addr,
        "/protectorate.v1.ApprovalService/DecideApproval",
        "owner-secret",
        json!({"id":id, "approve":true}),
    )
    .await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(decided["approval"]["reason"], "admin grant");
    let (status, allowed) = call_approved(
        addr,
        admin,
        &secret,
        json!({"node":"peer", "task":"fix peer"}),
        id,
    )
    .await;
    assert_eq!(status, 200, "{allowed}");
    assert_eq!(allowed["token"]["mintedBy"], "orchestrator-envoy");
    for node in ["care", "peer*"] {
        let (status, denied) = call(
            addr,
            admin,
            &secret,
            json!({"node":node, "task":"fix node"}),
        )
        .await;
        assert!(status == 400 || status == 403, "{denied}");
    }
    let (_, listed) = call(
        addr,
        "/protectorate.v1.ApprovalService/ListApprovals",
        "owner-secret",
        json!({"all":true}),
    )
    .await;
    assert_eq!(listed["approvals"].as_array().unwrap().len(), 1, "{listed}");
    server.abort();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (status, denied) = call(
        addr,
        admin,
        &secret,
        json!({"node":"peer", "task":"fix peer"}),
    )
    .await;
    assert_eq!(status, 403, "{denied}");
    server.abort();
}
