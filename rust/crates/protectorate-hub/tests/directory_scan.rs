use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const LIST: &str = "/protectorate.v1.DirectoryService/ListProjects";
const GET: &str = "/protectorate.v1.DirectoryService/GetProject";
const KILL: &str = "/protectorate.v1.SleeveService/KillSleeve";

async fn call(addr: std::net::SocketAddr, path: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

async fn wait_for(
    addr: std::net::SocketAddr,
    path: &str,
    body: Value,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (status, value) = call(addr, path, body.clone()).await;
        assert_eq!(status, 200, "{value}");
        if predicate(&value) {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "directory did not converge: {value}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn scans_running_sleeves_on_startup_and_after_kill() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"widget\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    assert!(Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&workspace)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widget.git"
        ])
        .status()
        .unwrap()
        .success());
    let hub = Hub::open(dir.path().join("state"), "owner-secret", "test").unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Pi,
                Mode::Chat,
                &workspace,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let listed = wait_for(addr, LIST, json!({"sleeve":"worker"}), |v| {
        v["projects"].as_array().is_some_and(|p| p.len() == 1)
    })
    .await;
    assert_eq!(listed["projects"][0]["id"], "github.com/acme/widget");
    assert_eq!(listed["projects"][0]["provides"][0], "cargo:widget");
    assert_eq!(listed["projects"][0]["dependencyCount"], 1);
    let (status, killed) = call(addr, KILL, json!({"name":"worker"})).await;
    assert_eq!(status, 200, "{killed}");
    wait_for(addr, LIST, json!({"sleeve":"worker"}), |v| {
        v["projects"].as_array().is_none_or(Vec::is_empty)
    })
    .await;
    let (status, kept) = call(addr, GET, json!({"project":"acme/widget"})).await;
    assert_eq!(status, 200, "{kept}");
    assert!(kept["project"]["owners"]
        .as_array()
        .is_none_or(Vec::is_empty));
    server.abort();
}
