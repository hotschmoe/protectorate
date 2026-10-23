#![cfg(unix)]

use protectorate_cli::{dispatch_endpoint, parse, Command};
use protectorate_hub::Hub;
use protectorate_lease::LeaseTable;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::test]
async fn renew_and_release_real_hub_lease() {
    let root = tempfile::tempdir().unwrap();
    // Seed a persisted lease before the hub attaches its live-node chooser.
    let table = LeaseTable::open(root.path().join("leases.json")).unwrap();
    let lease = table
        .acquire("holder", Duration::from_secs(60), "compute", now_millis())
        .unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let id = lease.id.clone();
    let url2 = url.clone();
    let text = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(
            &url2,
            "owner-secret",
            ["lease", "renew", &id, "--ttl", "2m"],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(text.contains(&lease.id), "{text}");
    assert!(text.contains("resource"));
    let entries = dispatch_list(&url).await;
    assert_eq!(entries["leases"][0]["ttl"], "120s");
    let id = lease.id.clone();
    let url2 = url.clone();
    let text = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(&url2, "owner-secret", ["lease", "release", &id])
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(text, format!("released {}\n", lease.id));
    assert!(dispatch_list(&url).await["leases"]
        .as_array()
        .is_none_or(Vec::is_empty));
    task.abort();
}

async fn dispatch_list(url: &str) -> serde_json::Value {
    let url = url.to_owned();
    let output = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(&url, "owner-secret", ["--json", "lease", "ls"])
    })
    .await
    .unwrap()
    .unwrap();
    serde_json::from_str(&output).unwrap()
}

#[test]
fn lease_control_arguments() {
    assert!(
        matches!(parse(["lease", "renew", "lease-12345678", "--ttl", "1h30m"]).unwrap().command,
        Command::LeaseRenew { ttl: Some(ttl), .. } if ttl == Duration::from_secs(5400))
    );
    assert!(
        matches!(parse(["lease", "release", "lease-a", "lease-b"]).unwrap().command,
        Command::LeaseRelease { ids } if ids.len() == 2)
    );
    assert!(parse(["lease", "release"]).is_err());
}
