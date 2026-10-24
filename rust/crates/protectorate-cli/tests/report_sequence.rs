#![cfg(target_os = "linux")]

//! `report` from a sleeve: each report carries a growing sequence, and a late
//! or repeated one is dropped by the hub instead of replacing a newer result.
use crate::common;

use protectorate_hub::{ChatLauncher, ChatRuntime, Hub};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::Value;
use std::{collections::BTreeMap, fs, process::Command};
use tokio::net::TcpListener;

fn sleeve_token(hub: &Hub, name: &str) -> String {
    common::mint(hub, &format!("sleeve:{name}"), &["chat_report"])
}

#[tokio::test(flavor = "multi_thread")]
async fn late_and_repeated_reports_are_dropped() {
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join("state");
    let work = home.path().join("work");
    for dir in [&state, &work] {
        fs::create_dir_all(dir).unwrap();
    }
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_chat_runtime(ChatRuntime::new(ChatLauncher {
            state_root: state.clone(),
            user_home: home.path().join("owner"),
            runtime_dir: home.path().join("runtime"),
            helper: "/nonexistent".into(),
            pi_executable: None,
            claude_executable: None,
            codex_executable: None,
            codex_tool_roots: Vec::new(),
            grok_executable: None,
            grok_tool_roots: Vec::new(),
            tool_roots: Vec::new(),
            claude_tool_roots: Vec::new(),
            flutter_sdk_root: None,
            flutter_cache_lower: None,
            flutter_tools_lower: None,
            pub_cache_lower: None,
            go_mod_cache_lower: None,
            environment: BTreeMap::new(),
            tasks_max: 64,
        }));
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Pi,
                Mode::Chat,
                &work,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    hub.sleeves
        .record_exit(
            "worker",
            "2026-01-01T00:01:00Z".to_string(),
            None,
            0,
            false,
            false,
        )
        .unwrap();
    let token = sleeve_token(&hub, "worker");
    let token_file = home.path().join("token");
    fs::write(&token_file, &token).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let outputs = tokio::task::spawn_blocking(move || {
        let run = |args: &[&str]| {
            let output = Command::new(env!("CARGO_BIN_EXE_protectorate"))
                .env("PROTECTORATE_HUB", &url)
                .env("PROTECTORATE_TOKEN", &token)
                .env("PROTECTORATE_TOKEN_FILE", &token_file)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        let first = home.path().join("first.md");
        let second = home.path().join("second.md");
        fs::write(&first, "first summary").unwrap();
        fs::write(&second, "second summary").unwrap();
        let sent = |path: &std::path::Path, status: &str| {
            run(&[
                "--json",
                "report",
                "--status",
                status,
                "--summary-file",
                path.to_str().unwrap(),
            ])
        };
        let a = sent(&first, "partial");
        let b = sent(&second, "complete");
        let late = run(&[
            "--json",
            "api",
            "call",
            "ChatService/Report",
            r#"{"status":"REPORT_STATUS_PARTIAL","summary":"late","sourceSeq":"1"}"#,
        ]);
        let again = run(&[
            "report",
            "--status",
            "complete",
            "--summary-file",
            second.to_str().unwrap(),
        ]);
        let unsequenced = run(&[
            "--json",
            "api",
            "call",
            "ChatService/Report",
            r#"{"status":"REPORT_STATUS_BLOCKED","summary":"arrival order"}"#,
        ]);
        [a, b, late, again, unsequenced]
    })
    .await
    .unwrap();
    let json = |text: &str| serde_json::from_str::<Value>(text).unwrap();
    let [a, b, late, again, unsequenced] = outputs;
    let (a, b) = (json(&a), json(&b));
    let seq = |v: &Value| {
        v["result"]["sourceSeq"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert!(seq(&a) > 0 && seq(&b) > seq(&a), "{a} {b}");
    assert_eq!(b["result"]["summary"], "second summary");
    let late = json(&late);
    assert_eq!(late["dropped"], true, "{late}");
    assert_eq!(late["result"]["summary"], "second summary", "{late}");
    assert!(again.starts_with("reported complete"), "{again}");
    let unsequenced = json(&unsequenced);
    assert!(unsequenced.get("dropped").is_none(), "{unsequenced}");
    assert_eq!(unsequenced["result"]["summary"], "arrival order");
    server.abort();
}
