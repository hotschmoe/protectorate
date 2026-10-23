#![cfg(unix)]

mod common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn admin_grant_and_audit_query_use_real_hub_http() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let path = home.path().join("grant.token");
    let result = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();
        let path_text = path.to_str().unwrap();
        let output = call(&[
            "grant",
            "admin",
            "--node",
            "kidsbox",
            "--task",
            "repair machine",
            "--out",
            path_text,
        ]);
        assert!(output.contains("grant\t"), "{output}");
        assert!(output.contains("task\trepair machine"), "{output}");
        let secret = std::fs::read_to_string(&path).unwrap();
        assert!(!secret.trim().is_empty());
        assert!(
            !output.contains(secret.trim()),
            "grant secret was printed despite --out"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(cli(&url, secret.trim(), ["job", "ls"]).is_ok());
        let raw = call(&[
            "--json",
            "audit",
            "ls",
            "--lines",
            "1",
            "--actor",
            "owner",
            "--procedure",
            "GrantService/Admin",
        ]);
        let page: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(page["entries"].as_array().unwrap().len(), 1);
        assert_eq!(page["entries"][0]["actor"], "owner");
        assert!(page["entries"][0]["procedure"]
            .as_str()
            .unwrap()
            .ends_with("GrantService/Admin"));
        let tail = call(&["audit", "tail", "-n", "20"]);
        assert!(tail.contains("GrantService/Admin"), "{tail}");
        cli(
            &url,
            "owner-secret",
            [
                "grant", "admin", "--node", "kidsbox", "--task", "another", "--out", path_text,
            ],
        )
        .unwrap_err();
    })
    .await;
    server.abort();
    result.unwrap();
}
