use protectorate_auth::Method as ScopeMethod;
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_wire::Catalog;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn call(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.SleeveService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

fn workspace<'a>(body: &'a Value, path: &Path) -> &'a Value {
    body["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == path.to_string_lossy().as_ref())
        .unwrap()
}

#[tokio::test]
async fn list_workspaces_discovers_managed_external_and_read_only_references() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let managed = state.join("workspaces/alpha");
    let symlinked_git = state.join("workspaces/beta");
    let external = temp.path().join("projects/outside");
    let references = temp.path().join("references");
    let hidden = state.join("workspaces/.hidden");
    for path in [
        &managed,
        &symlinked_git.join(".git"),
        &external,
        &references,
        &hidden,
        &managed.join("references"),
        &managed.join(".git"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(managed.join(".git/HEAD"), "ref: refs/heads/feature\n").unwrap();
    fs::write(
        managed.join(".git/config"),
        "[remote \"origin\"]\n url = https://user:secret@example.com/team/alpha.git\n",
    )
    .unwrap();
    let private_config = temp.path().join("private-config");
    fs::write(
        &private_config,
        "[remote \"origin\"]\n url = https://secret.example/private\n",
    )
    .unwrap();
    symlink(&private_config, symlinked_git.join(".git/config")).unwrap();
    let hub = Hub::open(&state, "owner", "test")
        .unwrap()
        .with_reference_dirs(vec![references.clone()])
        .unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Pi,
                Mode::Chat,
                &external,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (status, body) = call(addr, "ListWorkspaces", "owner", json!({})).await;
    assert_eq!(status, 200, "{body}");
    let managed_row = workspace(&body, &managed);
    assert_eq!(managed_row["requiredName"], "alpha");
    assert_eq!(managed_row["gitBranch"], "feature");
    assert_eq!(
        managed_row["gitRemote"],
        "https://example.com/team/alpha.git"
    );
    assert!(workspace(&body, &symlinked_git)["gitRemote"].is_null());
    let external_row = workspace(&body, &external);
    assert_eq!(external_row["sleeve"], "worker");
    assert_eq!(external_row["busy"], true);
    assert_eq!(workspace(&body, &references)["readOnly"], true);
    assert_eq!(
        workspace(&body, &managed.join("references"))["readOnly"],
        true
    );
    assert!(!body["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == hidden.to_string_lossy().as_ref()));
    task.abort();
}

#[tokio::test]
async fn browse_is_confined_hides_dot_entries_and_denies_sleeve_identity() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let root = temp.path().join("projects");
    let project = root.join("alpha");
    let hidden = root.join(".hidden");
    let outside = temp.path().join("outside");
    for path in [&state, &project, &hidden, &outside] {
        fs::create_dir_all(path).unwrap();
    }
    symlink(&outside, root.join("escape")).unwrap();
    fs::write(project.join(".git"), "gitdir: elsewhere").unwrap();
    fs::write(root.join("ordinary-file"), "not a directory").unwrap();
    let hub = Hub::open(&state, "owner", "test")
        .unwrap()
        .with_workspace_roots(vec![root.clone()])
        .unwrap();
    let catalog = Catalog::embedded().unwrap();
    let methods: Vec<_> = catalog
        .methods()
        .map(|m| ScopeMethod {
            tool: &m.tool,
            service: &m.service,
            read_only: m.read_only,
            owner_only: m.owner_only,
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let reader = hub
        .identity
        .create(
            "reader",
            &["read"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap();
    let sleeve = hub
        .identity
        .create(
            "sleeve:worker",
            &["read"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (status, top) = call(addr, "BrowseWorkspaces", reader.secret.expose(), json!({})).await;
    assert_eq!(status, 200, "{top}");
    assert_eq!(
        top["directories"][0]["path"],
        root.to_string_lossy().as_ref()
    );
    let (status, body) = call(addr, "BrowseWorkspaces", "owner", json!({"path":root})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["directories"].as_array().unwrap().len(), 1);
    assert_eq!(body["directories"][0]["name"], "alpha");
    assert_eq!(body["directories"][0]["isGit"], true);
    assert_eq!(body["parent"].as_str().unwrap_or(""), "");
    let (status, body) = call(
        addr,
        "BrowseWorkspaces",
        "owner",
        json!({"path":root.join("escape")}),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let (status, body) = call(
        addr,
        "BrowseWorkspaces",
        "owner",
        json!({"path":"relative"}),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let (status, body) = call(addr, "BrowseWorkspaces", sleeve.secret.expose(), json!({})).await;
    assert_eq!(status, 403, "{body}");
    task.abort();
}
