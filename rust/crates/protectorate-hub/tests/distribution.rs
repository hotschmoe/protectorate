use protectorate_hub::Hub;
use protectorate_release::{Info, Source};
use protectorate_tailnet_admin::Admin;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn home() -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "protectorate-http-routes-{}-{stamp}",
        std::process::id()
    ))
}

async fn get(
    addr: std::net::SocketAddr,
    path: &str,
    host: &str,
    token: &str,
) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8_lossy(&bytes[..split]).to_lowercase();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, headers, bytes[split + 4..].to_vec())
}

#[tokio::test]
async fn public_installers_and_owner_diagnostics_are_explicit_routes() {
    let root = home();
    let source = Arc::new(
        Source::new(
            "owner/repo",
            "stable",
            "",
            root.join("releases").to_str().unwrap(),
        )
        .unwrap(),
    );
    let info = Info {
        version: "v1-test".into(),
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
    let fake_server = b"#!/bin/sh\nprintf '%s\\n' \"$*\"\n";
    let fake_helper = b"#!/bin/sh\nexit 0\n";
    let fake_android = b"termux node fixture";
    let sums = format!(
        "{:x}  {server_asset}\n{:x}  {helper_asset}\n{:x}  {android_asset}\n",
        Sha256::digest(fake_server),
        Sha256::digest(fake_helper),
        Sha256::digest(fake_android),
    );
    let release_dir = source.store(&info, sums.as_bytes()).unwrap();
    std::fs::write(release_dir.join(&server_asset), fake_server).unwrap();
    std::fs::write(release_dir.join(&helper_asset), fake_helper).unwrap();
    std::fs::write(release_dir.join(android_asset), fake_android).unwrap();
    source.activate(&info.version).unwrap();
    let admin = Arc::new(
        Admin::new(
            "https://control.example.test",
            "test-api-key",
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let hub = Hub::open(root.join("home"), "owner-secret", "v1-test")
        .unwrap()
        .with_release_source(source)
        .with_tailnet_admin(admin);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (status, headers, body) = get(
        addr,
        "/install.sh",
        &format!("127.0.0.1:{}", addr.port()),
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert!(headers.contains("cache-control: no-store"));
    assert!(headers.contains("content-type: text/plain; charset=utf-8"));
    let script = String::from_utf8(body).unwrap();
    assert!(script.contains("protectorate-rs-linux-$arch"));
    assert!(script.contains("https://control.example.test"));
    assert!(script.contains("sha256sum --check --status"));
    let (status, headers, bytes) = get(
        addr,
        "/dl/protectorate-rs-android-arm64",
        "hub.example.test",
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert!(headers.contains("application/octet-stream"));
    assert_eq!(bytes, fake_android);
    let (status, _, checksum) = get(
        addr,
        "/dl/protectorate-rs-android-arm64.sha256",
        "hub.example.test",
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        String::from_utf8(checksum).unwrap(),
        format!("{:x}  {android_asset}\n", Sha256::digest(fake_android))
    );
    let (status, headers, body) = get(addr, "/install-termux.sh", "hub.example.test", "").await;
    assert_eq!(status, 200);
    assert!(headers.contains("cache-control: no-store"));
    let termux = String::from_utf8(body).unwrap();
    assert!(termux.contains("protectorate-rs-android-arm64"));
    assert!(termux.contains("https://control.example.test"));
    assert!(termux.contains("sha256sum --check --status"));
    #[cfg(target_os = "linux")]
    {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new("sh")
            .args(["-s", "--", "--print-target"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), server_asset);

        let mut child = Command::new("sh")
            .args(["-s", "--", "--print-target"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(termux.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            android_asset
        );

        let mut child = tokio::process::Command::new("sh")
            .args(["-s", "--", "--name", "worker", "--dry-run"])
            .env("PROTECTORATE_ENROLL_KEY", "test-enrollment-key")
            .env("XDG_CACHE_HOME", root.join("cache"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .await
            .unwrap();
        let output = child.wait_with_output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "node install --control https://control.example.test --name worker --dry-run"
        );
    }

    let (status, _, body) = get(addr, "/install.ps1", "hub.example.test", "").await;
    assert_eq!(status, 200);
    let script = String::from_utf8(body).unwrap();
    assert!(script.contains("$Version = 'v1-test'"));
    assert!(script.contains("$ReleaseTag = 'dev'"));
    assert!(script.contains("$ReleaseRepo = 'owner/repo'"));
    assert!(script.contains("$ReleaseRepo -ne 'owner/repo'"));
    assert!(script.contains("$Control = 'https://control.example.test'"));
    assert!(script.contains("$ManifestSha256 = '"));
    assert!(script.contains("$Name = $env:PROTECTORATE_NODE_NAME"));
    assert!(script.contains("$KeyFile = $env:PROTECTORATE_ENROLL_KEY_FILE"));
    let (status, _, _) = get(addr, "/install.sh", "user@bad.example", "").await;
    assert_eq!(status, 400);

    for path in [
        "/debug/metrics",
        "/debug/pprof/heap",
        "/debug/pprof/goroutine",
        "/debug/pprof/profile?seconds=1",
        "/debug/no-such-report",
    ] {
        for token in ["", "invalid"] {
            let (status, headers, _) = get(addr, path, "hub.example.test", token).await;
            assert_eq!(status, 403, "{path}");
            assert!(headers.contains("cache-control: no-store"));
        }
    }
    let (status, headers, body) =
        get(addr, "/debug/metrics", "hub.example.test", "owner-secret").await;
    assert_eq!(status, 200);
    assert!(headers.contains("content-type: application/json"));
    assert!(headers.contains("cache-control: no-store"));
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(body["/process/threads:count"].as_u64().unwrap() > 0);
    let (status, _, body) = get(
        addr,
        "/debug/pprof/heap",
        "hub.example.test",
        "owner-secret",
    )
    .await;
    assert_eq!(status, 200);
    assert!(String::from_utf8_lossy(&body).contains("VmRSS:"));
    let (status, _, _) = get(
        addr,
        "/debug/no-such-report",
        "hub.example.test",
        "owner-secret",
    )
    .await;
    assert_eq!(status, 404);
    let (status, _, _) = get(
        addr,
        "/debug/pprof/profile?seconds=0",
        "hub.example.test",
        "owner-secret",
    )
    .await;
    assert_eq!(status, 400);
    let (status, _, _) = get(
        addr,
        "/debug/nodes/absent/metrics",
        "hub.example.test",
        "owner-secret",
    )
    .await;
    assert_eq!(status, 404);
    std::fs::write(release_dir.join(android_asset), b"tampered Termux node").unwrap();
    let (status, _, _) = get(addr, "/install-termux.sh", "hub.example.test", "").await;
    assert_eq!(status, 503);
    let (status, _, _) = get(
        addr,
        "/dl/protectorate-rs-android-arm64",
        "hub.example.test",
        "",
    )
    .await;
    assert_eq!(status, 503);
    server.abort();
}

#[tokio::test]
async fn installer_without_tailnet_is_unavailable() {
    let hub = Hub::open(home(), "owner-secret", "v-test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    for path in ["/install.sh", "/install-termux.sh", "/install.ps1"] {
        let (status, _, _) = get(addr, path, "hub.example.test", "").await;
        assert_eq!(status, 503);
    }
    server.abort();
}
