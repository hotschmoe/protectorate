use protectorate_auth::Method as ScopeMethod;
use protectorate_hub::Hub;
use protectorate_wire::Catalog;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::symlink,
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
