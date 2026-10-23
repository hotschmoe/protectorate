#![cfg(unix)]

use protectorate_cli::dispatch_endpoint;
use protectorate_hub::Hub;
use std::fs;
use tokio::net::TcpListener;

#[tokio::test]
async fn workspace_discovery_and_lifecycle_errors_reach_real_hub() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let checkout_root = root.path().join("checkouts");
    let checkout = checkout_root.join("app");
    let managed = state.join("workspaces/worker");
    let reference = root.path().join("reference");
    for directory in [&state, &checkout, &managed, &reference] {
        fs::create_dir_all(directory).unwrap();
    }
    let hub = Hub::open(&state, "owner-secret", "test")
        .unwrap()
        .with_workspace_roots(vec![checkout_root.clone()])
        .unwrap()
        .with_reference_dirs(vec![reference.clone()])
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let url_for_calls = url.clone();
    let root_path = checkout_root.to_string_lossy().into_owned();
    let (list, browse_roots, browse_children, spawn_error, resleeve_error) =
        tokio::task::spawn_blocking(move || {
            let list = dispatch_endpoint(
                &url_for_calls,
                "owner-secret",
                ["--json", "sleeve", "workspaces"],
            )
            .unwrap();
            let roots = dispatch_endpoint(
                &url_for_calls,
                "owner-secret",
                ["--json", "sleeve", "browse"],
            )
            .unwrap();
            let children = dispatch_endpoint(
                &url_for_calls,
                "owner-secret",
                ["sleeve", "browse", &root_path],
            )
            .unwrap();
            let spawn = dispatch_endpoint(
                &url_for_calls,
                "owner-secret",
                [
                    "sleeve",
                    "spawn",
                    "worker",
                    "--workspace",
                    &root_path,
                    "--harness",
                    "shell",
                    "--mode",
                    "terminal",
                ],
            )
            .unwrap_err();
            let resleeve = dispatch_endpoint(
                &url_for_calls,
                "owner-secret",
                ["sleeve", "resleeve", "missing", "--to", "pi"],
            )
            .unwrap_err();
            (list, roots, children, spawn, resleeve)
        })
        .await
        .unwrap();
    let listed: serde_json::Value = serde_json::from_str(&list).unwrap();
    assert!(listed["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| { row["path"] == managed.to_string_lossy().as_ref() }));
    assert!(listed["workspaces"].as_array().unwrap().iter().any(|row| {
        row["path"] == reference.to_string_lossy().as_ref() && row["readOnly"] == true
    }));
    let roots: serde_json::Value = serde_json::from_str(&browse_roots).unwrap();
    assert_eq!(
        roots["directories"][0]["path"],
        checkout_root.to_string_lossy().as_ref()
    );
    assert!(browse_children.contains("app"), "{browse_children}");
    assert!(browse_children.contains("true"), "{browse_children}");
    assert_eq!(spawn_error.code, protectorate_cli::Code::Unavailable);
    assert_eq!(resleeve_error.code, protectorate_cli::Code::NotFound);
    server.abort();
}

#[tokio::test]
async fn prune_dry_run_and_yes_reach_the_real_hub() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path(), "owner-secret", "test").unwrap();
    let cache = root.path().join("sleeves/dormant/overlays");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("old-data"), b"discardable").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, hub.router().unwrap()).await.unwrap();
    });
    let first_url = url.clone();
    let dry_run = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(&first_url, "owner-secret", ["sleeve", "prune"])
    })
    .await
    .unwrap()
    .unwrap();
    assert!(dry_run.contains("would remove"), "{dry_run}");
    assert!(dry_run.contains("Dry run"), "{dry_run}");
    assert!(cache.join("old-data").exists());
    let second_url = url.clone();
    let removed = tokio::task::spawn_blocking(move || {
        dispatch_endpoint(&second_url, "owner-secret", ["sleeve", "prune", "--yes"])
    })
    .await
    .unwrap()
    .unwrap();
    assert!(removed.contains("removed"), "{removed}");
    assert!(!cache.exists());
    server.abort();
}
