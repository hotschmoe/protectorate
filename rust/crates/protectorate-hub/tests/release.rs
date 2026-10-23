use flate2::{write::GzEncoder, Compression};
use protectorate_hub::Hub;
use protectorate_release::{Source, BINARIES, RUST_BUNDLE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener as StdListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct ReleaseServer {
    base: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ReleaseServer {
    fn start(version: &str, corrupt: bool) -> Self {
        let listener = StdListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut assets = BTreeMap::new();
        for name in BINARIES {
            assets.insert(name.to_owned(), format!("binary:{name}").into_bytes());
        }
        for component in RUST_BUNDLE {
            let arch = match std::env::consts::ARCH {
                "aarch64" => "arm64",
                "x86_64" => "amd64",
                other => other,
            };
            assets.insert(
                format!("{component}-linux-{arch}"),
                format!("rust:{component}").into_bytes(),
            );
        }
        let web = format!("protectorate-web-{version}.tar.gz");
        assets.insert(web, web_archive());
        let mut sums = String::new();
        for (name, bytes) in &assets {
            sums.push_str(&format!("{:x}  {name}\n", Sha256::digest(bytes)));
        }
        assets.insert("SHA256SUMS".into(), sums.into_bytes());
        if corrupt {
            assets.insert(BINARIES[0].into(), b"corrupt binary".to_vec());
            let arch = match std::env::consts::ARCH {
                "aarch64" => "arm64",
                "x86_64" => "amd64",
                other => other,
            };
            assets.insert(
                format!("protectorate-rs-linux-{arch}"),
                b"corrupt Rust binary".to_vec(),
            );
        }
        let release = json!({
            "tag_name":version,
            "html_url":format!("{base}/notes"),
            "assets":assets.keys().map(|name| json!({
                "name":name,"browser_download_url":format!("{base}/assets/{name}")
            })).collect::<Vec<_>>()
        })
        .to_string()
        .into_bytes();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(3));
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let mut buf = [0u8; 8192];
                let len = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..len]);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let (status, body) = if path.starts_with("/repos/owner/repo/releases/") {
                    (200, release.as_slice())
                } else if let Some(name) = path.strip_prefix("/assets/") {
                    match assets.get(name) {
                        Some(body) => (200, body.as_slice()),
                        None => (404, b"missing".as_slice()),
                    }
                } else {
                    (404, b"missing".as_slice())
                };
                let head = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        Self {
            base,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for ReleaseServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

fn web_archive() -> Vec<u8> {
    let body = b"<html>release</html>";
    let mut tar = vec![0u8; 512];
    tar[..10].copy_from_slice(b"index.html");
    tar[100..108].copy_from_slice(b"0000644\0");
    tar[124..136].copy_from_slice(format!("{:011o}\0", body.len()).as_bytes());
    tar[148..156].copy_from_slice(b"        ");
    tar[156] = b'0';
    tar[257..263].copy_from_slice(b"ustar\0");
    tar[263..265].copy_from_slice(b"00");
    let sum: u32 = tar.iter().map(|byte| u32::from(*byte)).sum();
    tar[148..156].copy_from_slice(format!("{sum:07o}\0").as_bytes());
    tar.extend_from_slice(body);
    tar.resize(1024, 0);
    tar.resize(2048, 0);
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&tar).unwrap();
    gzip.finish().unwrap()
}

async fn start(
    source: Source,
    version: &str,
    home: &std::path::Path,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let hub = Hub::open(home, "owner-secret", version)
        .unwrap()
        .with_release_source(Arc::new(source));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}

async fn start_guarded(
    source: Source,
    version: &str,
    home: &std::path::Path,
    restarted: Arc<AtomicBool>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let hub = Hub::open(home, "owner-secret", version)
        .unwrap()
        .with_release_source(Arc::new(source))
        .with_release_restart(Arc::new(move || {
            restarted.store(true, Ordering::SeqCst);
        }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}

#[tokio::test]
async fn update_hub_cross_version_stages_rust_bundle_and_requests_guardian_restart() {
    let home = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let api = ReleaseServer::start("v2.0.0", false);
    let source = Source::new(
        "owner/repo",
        "stable",
        &api.base,
        cache.path().to_str().unwrap(),
    )
    .unwrap();
    let installed = protectorate_release::Info {
        tag: "v1.0.0".into(),
        version: "v1.0.0".into(),
        ..Default::default()
    };
    source.store(&installed, b"").unwrap();
    source.activate("v1.0.0").unwrap();
    let restarted = Arc::new(AtomicBool::new(false));
    let (addr, task) = start_guarded(source, "v1.0.0", home.path(), Arc::clone(&restarted)).await;
    let (status, info) = call(
        addr,
        "/protectorate.v1.ReleaseService/CheckRelease",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{info}");
    assert_eq!(info["canUpdate"], true);
    let (status, events, raw) = stream_update(addr).await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(events.last().unwrap()["phase"], "UPDATE_PHASE_RESTARTING");
    assert!(restarted.load(Ordering::SeqCst));
    let source = Source::new(
        "owner/repo",
        "stable",
        &api.base,
        cache.path().to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(source.current().version, "v1.0.0");
    source
        .verify_rust_bundle(
            "v2.0.0",
            "linux",
            match std::env::consts::ARCH {
                "aarch64" => "arm64",
                _ => "amd64",
            },
        )
        .unwrap();
    assert!(cache.path().join(".hub-trial.json").is_file());
    let (retry_status, _, retry_raw) = stream_update(addr).await;
    assert!(
        retry_status == 400 || retry_raw.contains("failed_precondition"),
        "{retry_status} {retry_raw}"
    );
    assert!(retry_raw.contains("awaiting guardian confirmation"));
    let (status, audit) = call(
        addr,
        "/protectorate.v1.AuditService/ListAuditEntries",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let updates = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["procedure"] == "/protectorate.v1.ReleaseService/UpdateHub")
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2, "{audit}");
    assert!(updates.iter().any(|entry| entry["outcome"] == "error"));
    let success = updates
        .iter()
        .find(|entry| entry["outcome"] == "ok")
        .unwrap();
    assert_eq!(success["subject"], "hub");
    assert_eq!(success["detail"], "channel=stable version=v2.0.0");
    task.abort();
}

async fn call(addr: SocketAddr, path: &str, body: Value) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = body.to_string();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer owner-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let divider = response
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap();
    let head = std::str::from_utf8(&response[..divider]).unwrap();
    let body = &response[divider + 4..];
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_slice(body).unwrap(),
    )
}

async fn stream_update(addr: SocketAddr) -> (u16, Vec<Value>, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let frame = [0, 0, 0, 0, 2, b'{', b'}'];
    let head = format!("POST /protectorate.v1.ReleaseService/UpdateHub HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer owner-secret\r\nContent-Type: application/connect+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", frame.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(&frame).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let divider = response
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap();
    let head = std::str::from_utf8(&response[..divider]).unwrap();
    let body = &response[divider + 4..];
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let mut events = Vec::new();
    let mut remainder = body;
    while let Some(end) = remainder.iter().position(|byte| *byte == b'\n') {
        let chunk_len =
            usize::from_str_radix(std::str::from_utf8(&remainder[..end]).unwrap().trim(), 16)
                .unwrap();
        remainder = &remainder[end + 1..];
        if chunk_len == 0 {
            break;
        }
        let chunk = &remainder[..chunk_len];
        if chunk.len() >= 5 && chunk[0] == 0 {
            events.push(serde_json::from_slice::<Value>(&chunk[5..]).unwrap());
        }
        remainder = &remainder[chunk_len + 2..];
    }
    (status, events, String::from_utf8_lossy(body).into_owned())
}

#[tokio::test]
async fn update_hub_stages_only_the_running_version_and_checks_assets() {
    let home = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let api = ReleaseServer::start("v1.0.0", false);
    let source = Source::new(
        "owner/repo",
        "stable",
        &api.base,
        cache.path().to_str().unwrap(),
    )
    .unwrap();
    let (addr, task) = start(source, "v1.0.0", home.path()).await;
    let (status, info) = call(
        addr,
        "/protectorate.v1.ReleaseService/CheckRelease",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{info}");
    assert_eq!(info["canUpdate"], true);
    let (status, events, raw) = stream_update(addr).await;
    assert_eq!(status, 200, "{raw}");
    assert!(
        events
            .iter()
            .any(|event| event["phase"] == "UPDATE_PHASE_VERIFYING"),
        "{events:?}"
    );
    assert_eq!(events.last().unwrap()["phase"], "UPDATE_PHASE_CONFIRMED");
    assert_eq!(
        std::fs::read_to_string(cache.path().join("current/web/index.html")).unwrap(),
        "<html>release</html>"
    );
    let (_, events, _) = stream_update(addr).await;
    assert_eq!(events[0]["phase"], "UPDATE_PHASE_SKIPPED");
    let (status, audit) = call(
        addr,
        "/protectorate.v1.AuditService/ListAuditEntries",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{audit}");
    let updates = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["procedure"] == "/protectorate.v1.ReleaseService/UpdateHub")
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2, "{audit}");
    for update in updates {
        assert_eq!(update["subject"], "hub");
        assert_eq!(update["detail"], "channel=stable version=v1.0.0");
        assert_eq!(update["outcome"], "ok");
    }
    task.abort();
}

#[tokio::test]
async fn update_hub_rejects_new_binary_and_corrupt_assets_without_activation() {
    let home = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let newer = ReleaseServer::start("v2.0.0", false);
    let source = Source::new(
        "owner/repo",
        "stable",
        &newer.base,
        cache.path().to_str().unwrap(),
    )
    .unwrap();
    let (addr, task) = start(source, "v1.0.0", home.path()).await;
    let (status, _, raw) = stream_update(addr).await;
    assert!(
        status == 400 || raw.contains("failed_precondition"),
        "{status} {raw}"
    );
    assert!(!cache.path().join("current").exists());
    let (audit_status, audit) = call(
        addr,
        "/protectorate.v1.AuditService/ListAuditEntries",
        json!({}),
    )
    .await;
    assert_eq!(audit_status, 200, "{audit}");
    let updates = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["procedure"] == "/protectorate.v1.ReleaseService/UpdateHub")
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 1, "{audit}");
    assert_eq!(updates[0]["outcome"], "error");
    assert_eq!(updates[0]["subject"], "hub");
    task.abort();

    let corrupt = ReleaseServer::start("v1.0.0", true);
    let source = Source::new(
        "owner/repo",
        "stable",
        &corrupt.base,
        cache.path().to_str().unwrap(),
    )
    .unwrap();
    let (addr, task) = start(source, "v1.0.0", home.path()).await;
    let (_, events, raw) = stream_update(addr).await;
    assert!(
        !events
            .iter()
            .any(|event| event["phase"] == "UPDATE_PHASE_CONFIRMED"),
        "{raw}"
    );
    assert!(!cache.path().join("current").exists());
    task.abort();
}
