//! ListWorkspaces and BrowseWorkspaces over HTTP against a real directory
//! tree: what is offered, what is redacted, and where browsing stops.
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink, path::Path};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";

async fn call(addr: std::net::SocketAddr, method: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.SleeveService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {OWNER}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

fn git_checkout(dir: &Path, branch: &str, config: Option<&str>) {
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join(".git/HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    if let Some(config) = config {
        fs::write(dir.join(".git/config"), config).unwrap();
    }
}

fn sleeve(hub: &Hub, name: &str, workspace: &Path, running: bool) {
    hub.sleeves
        .record_spawned(
            Record::new(
                name,
                Harness::Pi,
                Mode::Chat,
                workspace,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    if !running {
        hub.sleeves
            .record_exit(name, "2026-01-01T00:00:01Z".into(), None, 0, false, false)
            .unwrap();
    }
}

#[tokio::test]
async fn workspaces_are_listed_redacted_and_browsed_only_inside_roots() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let root = base.join("box");
    let state = root.join("state");
    let managed = state.join("workspaces");
    let project = root.join("proj");
    let outside = base.join("outside");
    let references = base.join("references");
    for dir in [
        &managed.join("alpha/references"),
        &managed.join("beta"),
        &managed.join(".hidden"),
        &project.join("sub"),
        &root.join(".dot"),
        &state.join("sleeves/inside"),
        &outside,
        &references,
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    git_checkout(
        &managed.join("alpha"),
        "feature",
        Some("[remote \"origin\"]\n url = https://user:secret@example.com/team/alpha.git\n"),
    );
    git_checkout(&managed.join("beta"), "main", None);
    let private = base.join("private-config");
    fs::write(
        &private,
        "[remote \"origin\"]\n url = https://private.example/leak\n",
    )
    .unwrap();
    symlink(&private, managed.join("beta/.git/config")).unwrap();
    git_checkout(&project, "main", None);
    symlink(&project, root.join("link-in")).unwrap();
    symlink(&outside, root.join("escape")).unwrap();
    fs::write(root.join("file"), "not a directory").unwrap();

    let hub = Hub::open(&state, OWNER, "test")
        .unwrap()
        .with_workspace_roots(vec![root.clone()])
        .unwrap()
        .with_reference_dirs(vec![references.clone()])
        .unwrap();
    sleeve(&hub, "worker", &project, true);
    sleeve(&hub, "old", &outside, false);
    sleeve(&hub, "inside", &state.join("sleeves/inside"), true);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let (status, listed) = call(addr, "ListWorkspaces", json!({})).await;
    assert_eq!(status, 200, "{listed}");
    assert!(!listed.to_string().contains("secret"), "{listed}");
    assert!(!listed.to_string().contains("private.example"), "{listed}");
    let rows = listed["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let flag = |key: &str| row[key] == true;
            let text = |key: &str| row[key].as_str().unwrap_or("").to_owned();
            (
                row["path"].as_str().unwrap().to_owned(),
                text("requiredName"),
                text("gitBranch"),
                text("gitRemote"),
                text("sleeve"),
                flag("busy"),
                flag("readOnly"),
            )
        })
        .collect::<Vec<_>>();
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let row = |p: &Path, name: &str, branch: &str, remote: &str, sleeve: &str, busy, ro| {
        (
            path(p),
            name.to_owned(),
            branch.to_owned(),
            remote.to_owned(),
            sleeve.to_owned(),
            busy,
            ro,
        )
    };
    assert_eq!(
        rows,
        [
            row(
                &managed.join("alpha"),
                "alpha",
                "feature",
                "https://example.com/team/alpha.git",
                "",
                false,
                false
            ),
            row(&managed.join("beta"), "beta", "main", "", "", false, false),
            row(&outside, "old", "", "", "old", false, false),
            row(&project, "worker", "main", "", "worker", true, false),
            row(
                &managed.join("alpha/references"),
                "",
                "",
                "",
                "",
                false,
                true
            ),
            row(&references, "", "", "", "", false, true),
        ]
    );

    let (status, top) = call(addr, "BrowseWorkspaces", json!({})).await;
    assert_eq!(status, 200, "{top}");
    assert_eq!(top["directories"][0]["path"], path(&root));
    let (status, listing) = call(addr, "BrowseWorkspaces", json!({"path":root})).await;
    assert_eq!(status, 200, "{listing}");
    assert!(listing.get("parent").is_none_or(|p| p == ""), "{listing}");
    let entries = listing["directories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["name"].as_str().unwrap().to_owned(),
                d["path"].as_str().unwrap().to_owned(),
                d["acceptable"] == true,
                d["reason"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [
            ("link-in".into(), path(&project), true, String::new()),
            ("proj".into(), path(&project), true, String::new()),
            (
                "state".into(),
                path(&state),
                false,
                "inside Protectorate state".into()
            ),
        ]
    );
    let (status, nested) = call(
        addr,
        "BrowseWorkspaces",
        json!({"path":project.join("sub")}),
    )
    .await;
    assert_eq!(status, 200, "{nested}");
    assert_eq!(nested["parent"], path(&project));
    let (status, managed_list) = call(addr, "BrowseWorkspaces", json!({"path":managed})).await;
    assert_eq!(status, 200, "{managed_list}");
    assert_eq!(managed_list["directories"][0]["requiredName"], "alpha");
    assert_eq!(managed_list["directories"][0]["acceptable"], true);

    for refused in [
        project.join("../../outside"),
        root.join("escape"),
        root.join("file"),
        "/".into(),
        root.join("missing"),
    ] {
        let (status, body) = call(addr, "BrowseWorkspaces", json!({"path":refused})).await;
        assert_eq!(
            (status, &body["code"]),
            (400, &json!("invalid_argument")),
            "{}: {body}",
            refused.display()
        );
    }
    server.abort();
}
