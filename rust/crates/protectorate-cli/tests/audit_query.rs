#![cfg(unix)]

use crate::common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn audit_query_filters_by_actor_and_procedure_over_real_hub_http() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let result = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();
        call(&["backlog", "add", "first"]);
        call(&["backlog", "add", "second"]);
        let raw = call(&[
            "--json",
            "audit",
            "ls",
            "--lines",
            "1",
            "--actor",
            "owner",
            "--procedure",
            "BacklogService/CreateNote",
        ]);
        let page: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(page["entries"].as_array().unwrap().len(), 1);
        assert_eq!(page["entries"][0]["actor"], "owner");
        assert!(page["entries"][0]["procedure"]
            .as_str()
            .unwrap()
            .ends_with("BacklogService/CreateNote"));
        let tail = call(&["audit", "tail", "-n", "20"]);
        assert!(tail.contains("BacklogService/CreateNote"), "{tail}");
    })
    .await;
    server.abort();
    result.unwrap();
}
