use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_connect_client::Client;
use protectorate_hub::Hub;
use protectorate_inventory::{Node, CARE};
use protectorate_node::{Agent, MetricsCollector};
use protectorate_node_metrics::NodeMetrics;
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener};

struct Metrics;
impl MetricsCollector for Metrics {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Ok(NodeMetrics::default())
    }
}

async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

async fn call(url: String, method: &'static str, value: Value) -> Result<Value, RpcError> {
    tokio::task::spawn_blocking(move || {
        Client::new(&url, "owner-secret", Duration::from_secs(5))?.call(method, &value)
    })
    .await
    .unwrap()
}

const PUT: &str = "/protectorate.v1.NodeService/PutFile";
const GET: &str = "/protectorate.v1.NodeService/GetFile";
const SHA_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[tokio::test]
async fn lost_node_chunk_reply_can_be_replayed_without_publishing_partial_file() {
    use axum::{http::StatusCode, middleware::from_fn, response::IntoResponse};

    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("node-files");
    let hits = Arc::new(AtomicUsize::new(0));
    let fault_hits = Arc::clone(&hits);
    let node = Agent::new(Arc::new(Metrics), "node-secret")
        .with_files(&files, false)
        .unwrap();
    let node_router = node.router().unwrap().layer(from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let hits = Arc::clone(&fault_hits);
            async move {
                let number = if request.uri().path().ends_with("/PutFile") {
                    hits.fetch_add(1, Ordering::SeqCst) + 1
                } else {
                    0
                };
                let response = next.run(request).await;
                if number == 1 || number == 3 {
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                } else {
                    response
                }
            }
        },
    ));
    let (node_url, node_task) = serve(node_router).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_url.trim_start_matches("http://").into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("node-secret".into()).unwrap();
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;

    let first = json!({
        "name":"peer","transferId":"lost-reply-001","path":"recovered",
        "kind":"FILE_KIND_FILE","mode":384,"offset":"0",
        "chunk":{"data":"YWJj","rawSize":3}
    });
    assert!(call(hub_url.clone(), PUT, first.clone()).await.is_err());
    assert!(!files.join("recovered").exists());
    assert_eq!(
        call(hub_url.clone(), PUT, first).await.unwrap()["received"],
        "3"
    );
    let second = json!({
        "name":"peer","transferId":"lost-reply-001","offset":"3",
        "chunk":{"data":"ZGVm","rawSize":3}
    });
    assert!(call(hub_url.clone(), PUT, second.clone()).await.is_err());
    assert!(!files.join("recovered").exists());
    let altered = json!({
        "name":"peer","transferId":"lost-reply-001","offset":"3",
        "chunk":{"data":"WFla","rawSize":3}
    });
    assert_eq!(
        call(hub_url.clone(), PUT, altered).await.unwrap_err().code,
        Code::InvalidArgument
    );
    assert_eq!(
        call(hub_url.clone(), PUT, second).await.unwrap()["received"],
        "6"
    );
    let last = json!({
        "name":"peer","transferId":"lost-reply-001","offset":"6","last":true,
        "trailer":{"size":"6","sha256":"bef57ec7f53a6d40beb640a780a639c83bc29ac8a9816f1fc6c5c6dcd93c4721"}
    });
    assert_eq!(
        call(hub_url.clone(), PUT, last).await.unwrap()["done"],
        true
    );
    assert_eq!(std::fs::read(files.join("recovered")).unwrap(), b"abcdef");
    assert_eq!(hits.load(Ordering::SeqCst), 6);

    let bytes = vec![b'a'; 3072];
    let encoded = STANDARD.encode(&bytes);
    let first_after_cut = json!({
        "name":"peer","transferId":"body-cut-001","path":"body-cut",
        "kind":"FILE_KIND_FILE","mode":384,"offset":"0",
        "chunk":{"data":encoded.clone(),"rawSize":bytes.len()}
    });
    let body = first_after_cut.to_string();
    let middle_of_chunk = body.find(&encoded).unwrap() + encoded.len() / 2;
    let address = node_url.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let headers = format!(
        "POST /protectorate.v1.NodeAgentService/PutFile HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer node-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream
        .write_all(&body.as_bytes()[..middle_of_chunk])
        .await
        .unwrap();
    stream.shutdown().await.unwrap();
    drop(stream);
    assert!(!files.join("body-cut").exists());
    assert_eq!(
        call(hub_url.clone(), PUT, first_after_cut).await.unwrap()["received"],
        bytes.len().to_string()
    );
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let finish_after_cut = json!({
        "name":"peer","transferId":"body-cut-001","offset":bytes.len().to_string(),
        "last":true,"trailer":{"size":bytes.len().to_string(),"sha256":digest}
    });
    assert_eq!(
        call(hub_url, PUT, finish_after_cut).await.unwrap()["done"],
        true
    );
    assert_eq!(std::fs::read(files.join("body-cut")).unwrap(), bytes);
    hub_task.abort();
    node_task.abort();
}

#[tokio::test]
async fn named_and_local_file_transfers_relay_without_hub_staging() {
    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("node-files");
    let node = Agent::new(Arc::new(Metrics), "node-secret")
        .with_files(&files, false)
        .unwrap();
    let (node_url, node_task) = serve(node.router().unwrap()).await;

    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "peer".into(),
                address: node_url.trim_start_matches("http://").into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let local = hub.hostname.clone();
    let hub = hub
        .with_node_token("node-secret".into())
        .unwrap()
        .with_local_node_endpoint(node_url);
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;

    for (index, name) in ["peer", local.as_str()].into_iter().enumerate() {
        let path = format!("through-hub-{index}");
        let id = format!("transfer-{index:04}");
        let first = call(
            hub_url.clone(),
            PUT,
            json!({
                "name":name,"transferId":id,"path":path,"kind":"FILE_KIND_FILE",
                "mode":384,"offset":"0","chunk":{"data":"YQ==","rawSize":1}
            }),
        )
        .await
        .unwrap();
        assert_eq!(first["received"], "1");
        assert!(!files.join(&path).exists());
        let middle = call(
            hub_url.clone(),
            PUT,
            json!({
                "name":name,"transferId":id,"offset":"1",
                "chunk":{"data":"Yg==","rawSize":1}
            }),
        )
        .await
        .unwrap();
        assert_eq!(middle["received"], "2");
        let done = call(
            hub_url.clone(),
            PUT,
            json!({
                "name":name,"transferId":id,"offset":"2","last":true,
                "chunk":{"data":"Yw==","rawSize":1},
                "trailer":{"size":"3","sha256":SHA_ABC}
            }),
        )
        .await
        .unwrap();
        assert_eq!(done["done"], true);
        assert_eq!(done["received"], "3");
        assert_eq!(std::fs::read(files.join(&path)).unwrap(), b"abc");
        let (url, name) = (hub_url.clone(), name.to_owned());
        let parts = tokio::task::spawn_blocking(move || {
            let client = Client::new(&url, "owner-secret", Duration::from_secs(5)).unwrap();
            let mut parts = Vec::new();
            client
                .stream(
                    GET,
                    &json!({"name":name,"path":path,"noCompress":true}),
                    |part| {
                        parts.push(part);
                        Ok(())
                    },
                )
                .unwrap();
            parts
        })
        .await
        .unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["header"]["kind"], "FILE_KIND_FILE");
        assert_eq!(parts[1]["chunk"]["data"], "YWJj");
        assert_eq!(parts[2]["trailer"]["sha256"], SHA_ABC);
    }
    let bad = call(
        hub_url.clone(),
        PUT,
        json!({"name":"peer","transferId":"bad-hash-001","path":"bad-hash",
            "kind":"FILE_KIND_FILE","last":true,
            "trailer":{"size":"0","sha256":SHA_ABC}}),
    )
    .await
    .unwrap_err();
    assert_eq!(bad.code, Code::DataLoss);
    assert!(!files.join("bad-hash").exists());
    let escape = call(
        hub_url.clone(),
        PUT,
        json!({"name":"peer","transferId":"escape-001","path":"../outside",
            "kind":"FILE_KIND_FILE","last":true,
            "trailer":{"size":"0","sha256":SHA_ABC}}),
    )
    .await
    .unwrap_err();
    assert_eq!(escape.code, Code::PermissionDenied);
    assert!(!root.path().join("outside").exists());
    assert!(!root.path().join("hub/through-hub-0").exists());
    let audit = call(
        hub_url,
        "/protectorate.v1.AuditService/ListAuditEntries",
        json!({}),
    )
    .await
    .unwrap();
    let successful_puts = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["procedure"] == PUT && entry["outcome"] == "ok")
        .count();
    assert_eq!(
        successful_puts, 4,
        "first and final chunks are audited; middle chunks are omitted"
    );
    for name in ["peer", local.as_str()] {
        assert_eq!(
            audit["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| {
                    entry["procedure"] == PUT
                        && entry["outcome"] == "ok"
                        && entry["subject"] == name
                })
                .count(),
            2,
            "first and final chunk must both be recorded for {name}"
        );
    }
    assert!(audit["entries"].as_array().unwrap().iter().any(|entry| {
        entry["procedure"] == PUT
            && entry["outcome"] == "error"
            && entry["subject"] == "peer"
            && entry["code"] == "permission_denied"
    }));
    hub_task.abort();
    node_task.abort();
}

#[tokio::test]
async fn node_selection_and_node_agent_auth_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("node-files");
    let node = Agent::new(Arc::new(Metrics), "actual-node-secret")
        .with_files(&files, false)
        .unwrap();
    let (node_url, node_task) = serve(node.router().unwrap()).await;
    let hub = Hub::open(root.path().join("hub"), "owner-secret", "test").unwrap();
    hub.inventory
        .add(
            Node {
                name: "invited".into(),
                address: node_url.trim_start_matches("http://").into(),
                invited: true,
                ..Node::default()
            },
            false,
        )
        .unwrap();
    hub.inventory
        .add(
            Node {
                name: "care".into(),
                address: node_url.trim_start_matches("http://").into(),
                tier: CARE.into(),
                ..Node::default()
            },
            false,
        )
        .unwrap();
    let hub = hub.with_node_token("wrong-node-secret".into()).unwrap();
    let (hub_url, hub_task) = serve(hub.router().unwrap()).await;
    let request = |name: &str| json!({"name":name,"transferId":"transfer-test","path":"dest","kind":"FILE_KIND_FILE","last":true,"trailer":{"size":"0","sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}});
    assert_eq!(
        call(hub_url.clone(), PUT, request("missing"))
            .await
            .unwrap_err()
            .code,
        Code::NotFound
    );
    assert_eq!(
        call(hub_url.clone(), PUT, request("invited"))
            .await
            .unwrap_err()
            .code,
        Code::FailedPrecondition
    );
    assert_eq!(
        call(hub_url.clone(), PUT, request("care"))
            .await
            .unwrap_err()
            .code,
        Code::Unauthenticated
    );
    assert!(!files.join("dest").exists());
    hub_task.abort();
    node_task.abort();
}
