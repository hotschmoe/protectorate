use protectorate_hub::Hub;
use protectorate_release::{Info, Source};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn get(addr: std::net::SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let split = response.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let header = String::from_utf8_lossy(&response[..split]);
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(header.contains("cache-control: no-store") || status != 200);
    (status, response[split + 4..].to_vec())
}

#[tokio::test]
async fn serves_only_verified_current_rust_components() {
    let temp = tempfile::tempdir().unwrap();
    let source = Arc::new(
        Source::new(
            "owner/repo",
            "stable",
            "",
            temp.path().join("releases").to_str().unwrap(),
        )
        .unwrap(),
    );
    let name = "protectorate-rs-linux-amd64";
    let bytes = b"rust executable";
    let digest = format!("{:x}", Sha256::digest(bytes));
    let sums = format!("{digest}  {name}\n");
    let info = Info {
        version: "v-test".into(),
        ..Default::default()
    };
    let dir = source.store(&info, sums.as_bytes()).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
    source.activate(&info.version).unwrap();
    let hub = Hub::open(temp.path().join("home"), "owner-secret", "v-test")
        .unwrap()
        .with_release_source(Arc::clone(&source));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (status, body) = get(addr, &format!("/dl/{name}")).await;
    assert_eq!(status, 200);
    assert_eq!(body, bytes);
    let (status, body) = get(addr, &format!("/dl/{name}.sha256")).await;
    assert_eq!(status, 200);
    assert_eq!(body, sums.as_bytes());
    let (status, _) = get(addr, "/dl/protectorate-linux-amd64").await;
    assert_eq!(status, 404);
    let (status, _) = get(addr, "/dl/protectorate-rs-windows-amd64.exe").await;
    assert_eq!(status, 404);
    let (status, _) = get(addr, "/dl/../../etc/passwd").await;
    assert_ne!(status, 200);
    std::fs::write(dir.join(name), b"corrupt").unwrap();
    let (status, _) = get(addr, &format!("/dl/{name}")).await;
    assert_eq!(status, 503);
    let (status, _) = get(addr, &format!("/dl/{name}.sha256")).await;
    assert_eq!(status, 503);
    server.abort();
}
