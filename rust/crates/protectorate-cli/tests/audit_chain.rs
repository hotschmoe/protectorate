#![cfg(unix)]

use crate::common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use std::fs;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn audit_verify_passes_then_detects_an_edited_and_a_truncated_log() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let audit = home.path().join("audit");
    let result = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| cli(&url, "owner-secret", args);
        for title in ["one", "two", "three"] {
            call(&["backlog", "add", title]).unwrap();
        }
        let report: Value =
            serde_json::from_str(&call(&["--json", "audit", "verify"]).unwrap()).unwrap();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["entries"], "3", "{report}");
        let status: Value = serde_json::from_str(&call(&["--json", "status"]).unwrap()).unwrap();
        assert_eq!(status["auditHead"], report["head"], "{status}");
        let page: Value = serde_json::from_str(&call(&["--json", "audit", "ls"]).unwrap()).unwrap();
        assert_eq!(page["head"], report["head"]);
        let entries = page["entries"].as_array().unwrap();
        assert_eq!(entries[0]["prevHash"], entries[1]["hash"]);
        assert_eq!(entries[0]["seq"], "3");

        let file = fs::read_dir(&audit)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .unwrap();
        let original = fs::read_to_string(&file).unwrap();
        fs::write(&file, original.replacen("CreateNote", "DeleteNote", 1)).unwrap();
        let broken = call(&["audit", "verify"]).unwrap_err();
        assert!(broken.contains("audit chain broken"), "{broken}");
        assert!(broken.contains(":1: hash does not match"), "{broken}");

        let mut lines: Vec<&str> = original.lines().collect();
        lines.pop();
        fs::write(&file, lines.join("\n") + "\n").unwrap();
        let truncated = call(&["audit", "verify"]).unwrap_err();
        assert!(
            truncated.contains("the log ends at entry 2 but the recorded head is entry 3"),
            "{truncated}"
        );

        fs::write(&file, &original).unwrap();
        call(&["backlog", "add", "four"]).unwrap();
        let report: Value =
            serde_json::from_str(&call(&["--json", "audit", "verify"]).unwrap()).unwrap();
        assert_eq!(report["entries"], "4", "{report}");
    })
    .await;
    server.abort();
    result.unwrap();
}
