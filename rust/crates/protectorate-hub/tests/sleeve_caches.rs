#![cfg(target_os = "linux")]
use crate::support::capability;
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink};
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

async fn call(addr: std::net::SocketAddr, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.SleeveService/KillSleeve HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

fn register_chat(hub: &Hub, workspace: &std::path::Path, name: &str) {
    hub.sleeves
        .record_spawned(
            Record::new(
                name,
                Harness::Pi,
                Mode::Chat,
                workspace,
                "2026-10-22T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
}

#[tokio::test]
async fn owner_drop_caches_stops_sleeve_and_keeps_home_workspace_and_auth() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("work.txt"), "work").unwrap();
    let home = root.path().join("sleeves/chat/home");
    for name in ["go-build", "go-mod", "pub", "flutter", "overlays"] {
        let cache = home.join(".cache").join(name);
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("artifact"), name).unwrap();
    }
    fs::create_dir_all(home.join(".pi/sessions")).unwrap();
    fs::write(home.join(".pi/sessions/history.jsonl"), "transcript").unwrap();
    fs::write(home.join(".protectorate-token"), "auth").unwrap();
    fs::write(home.join(".cache/other"), "keep").unwrap();
    let outside_cache = root.path().join("outside-cache");
    fs::create_dir(&outside_cache).unwrap();
    fs::write(outside_cache.join("keep.txt"), "outside").unwrap();
    symlink(&outside_cache, home.join(".cache/go-build/link")).unwrap();
    let overlays = root.path().join("sleeves/chat/overlays/build/upper");
    fs::create_dir_all(&overlays).unwrap();
    fs::write(overlays.join("artifact"), "discard").unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    register_chat(&hub, &workspace, "chat");
    let (_, limited) = capability::mint(&hub, "limited", &["sleeve_*"]);
    let (addr, server) = serve(hub).await;
    let (status, denied) = call(addr, &limited, json!({"name":"chat","dropCaches":true})).await;
    assert_eq!(status, 403, "{denied}");
    assert!(home.join(".cache/go-build/artifact").is_file());
    let (status, killed) = call(
        addr,
        "owner-secret",
        json!({"name":"chat","dropCaches":true}),
    )
    .await;
    assert_eq!(status, 200, "{killed}");
    for name in ["go-build", "go-mod", "pub", "flutter", "overlays"] {
        assert!(
            !home.join(".cache").join(name).exists(),
            "{name} cache remained"
        );
    }
    assert!(!root.path().join("sleeves/chat/overlays").exists());
    assert_eq!(
        fs::read_to_string(home.join(".pi/sessions/history.jsonl")).unwrap(),
        "transcript"
    );
    assert_eq!(
        fs::read_to_string(home.join(".protectorate-token")).unwrap(),
        "auth"
    );
    assert_eq!(
        fs::read_to_string(home.join(".cache/other")).unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("work.txt")).unwrap(),
        "work"
    );
    assert_eq!(
        fs::read_to_string(outside_cache.join("keep.txt")).unwrap(),
        "outside"
    );
    server.abort();
}

#[tokio::test]
async fn symlinked_cache_parent_is_rejected_without_following_it() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let home = root.path().join("sleeves/chat/home");
    let outside = root.path().join("outside");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret"), "keep").unwrap();
    symlink(&outside, home.join(".cache")).unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    register_chat(&hub, &workspace, "chat");
    let (addr, server) = serve(hub).await;
    let (status, rejected) = call(
        addr,
        "owner-secret",
        json!({"name":"chat","dropCaches":true}),
    )
    .await;
    assert_eq!(status, 400, "{rejected}");
    assert_eq!(rejected["code"], "failed_precondition");
    assert_eq!(fs::read_to_string(outside.join("secret")).unwrap(), "keep");
    assert!(home.join(".cache").is_symlink());
    server.abort();
}
