#![cfg(target_os = "linux")]

//! `sleeve status` against a real hub watching a real tmux pane that shows a
//! pane captured from the Claude Code TUI at a permission prompt.
mod common;

use common::{cli, PrivateTmux};
use protectorate_hub::Hub;
use protectorate_session::{CreateOptions, Manager};
use protectorate_sleeve_registry::{Harness, Mode, Record};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";
const PANE: &str = include_str!("../../../fuzz/corpus/terminal_pane/claude-permission-prompt");

#[tokio::test(flavor = "multi_thread")]
async fn sleeve_status_explains_a_blocked_terminal_sleeve() {
    let home = tempfile::tempdir().unwrap();
    let socket = PrivateTmux(format!("p-cli-status-{}", std::process::id()));
    let manager = Arc::new(Manager::new(&socket.0));
    let hub = Hub::open(home.path(), OWNER, "test")
        .unwrap()
        .with_sessions(Arc::clone(&manager));
    let work = home.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let pane = home.path().join("pane.txt");
    let body = PANE.lines().skip(1).map(str::trim_end).collect::<Vec<_>>();
    std::fs::write(&pane, body.join("\n").trim_end()).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "gate",
                Harness::Claude,
                Mode::Terminal,
                &work,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    manager
        .create(&CreateOptions {
            name: "sleeve-gate".into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                format!("cat '{}'; exec sleep 600", pane.display()),
            ],
            ..Default::default()
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let (status, listed) = tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let until = |args: &[&str], want: &str| loop {
            let out = cli(&url, OWNER, args.iter().copied()).expect("sleeve command");
            if out.contains(want) {
                break out;
            }
            assert!(std::time::Instant::now() < deadline, "no {want}: {out}");
            std::thread::sleep(Duration::from_millis(200));
        };
        (
            until(&["sleeve", "status", "gate", "--explain"], "blocked"),
            until(&["sleeve", "ls"], "running (blocked)"),
        )
    })
    .await
    .unwrap();
    assert!(status.contains("claude.permission"), "{status}");
    assert!(status.contains("match"), "{status}");
    assert!(status.contains("do you want to proceed?"), "{status}");
    assert!(listed.contains("gate"), "{listed}");
    server.abort();
}
