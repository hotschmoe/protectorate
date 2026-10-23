#![cfg(unix)]

mod common;

use common::cli;
use protectorate_hub::Hub;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn edit_pin_and_remove_notes_over_real_hub_http() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), "owner-secret", "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let root = home.path().to_owned();
    let result = tokio::task::spawn_blocking(move || {
        let call = |args: &[&str]| cli(&url, "owner-secret", args).unwrap();
        let created: Value = serde_json::from_str(&call(&[
            "--json",
            "backlog",
            "add",
            "Original",
            "--body",
            "line one",
            "--project",
            "old",
        ]))
        .unwrap();
        let id = created["id"].as_str().unwrap();
        let edited: Value = serde_json::from_str(&call(&[
            "--json",
            "backlog",
            "edit",
            id,
            "--title",
            "New",
            "--project=",
        ]))
        .unwrap();
        assert_eq!(edited["title"], "New");
        assert_eq!(edited["project"].as_str().unwrap_or_default(), "");
        let appended: Value = serde_json::from_str(&call(&[
            "--json", "backlog", "edit", id, "--append", "line two",
        ]))
        .unwrap();
        assert_eq!(appended["body"], "line one\nline two\n");
        let body_file = root.join("body.md");
        std::fs::write(&body_file, "replacement\n").unwrap();
        let replaced: Value = serde_json::from_str(&call(&[
            "--json",
            "backlog",
            "edit",
            id,
            "--body-file",
            body_file.to_str().unwrap(),
        ]))
        .unwrap();
        assert_eq!(replaced["body"], "replacement\n");
        let pinned: Value = serde_json::from_str(&call(&["--json", "backlog", "pin", id])).unwrap();
        assert_eq!(pinned["pinned"], true);
        let unpinned: Value =
            serde_json::from_str(&call(&["--json", "backlog", "pin", id, "--off"])).unwrap();
        assert!(!unpinned["pinned"].as_bool().unwrap_or(false));

        let editor = root.join("editor.sh");
        std::fs::write(
            &editor,
            "#!/bin/sh\nprintf 'Editor title\\n\\nEditor body\\n' > \"$1\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
        let edit = std::process::Command::new(env!("CARGO_BIN_EXE_protectorate"))
            .env("PROTECTORATE_HUB", &url)
            .env("PROTECTORATE_TOKEN", "owner-secret")
            .env("EDITOR", &editor)
            .args(["backlog", "edit", id])
            .output()
            .unwrap();
        assert!(
            edit.status.success(),
            "{}",
            String::from_utf8_lossy(&edit.stderr)
        );
        let edited: Value =
            serde_json::from_str(&call(&["--json", "backlog", "show", id])).unwrap();
        assert_eq!(edited["title"], "Editor title");
        assert_eq!(edited["body"], "Editor body\n");
        assert_eq!(call(&["backlog", "rm", id]), format!("deleted\t{id}\n"));
        cli(&url, "owner-secret", ["backlog", "show", id]).unwrap_err();
    })
    .await;
    server.abort();
    result.unwrap();
}
