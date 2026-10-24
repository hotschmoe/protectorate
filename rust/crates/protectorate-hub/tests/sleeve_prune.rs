#![cfg(target_os = "linux")]
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, ReadOnlyMount, Record, State};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink, path::Path, process::Command};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}

async fn call(addr: std::net::SocketAddr, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.SleeveService/PruneSleeves HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

fn write(path: &Path, bytes: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn source(root: &Path, format: &str) -> std::path::PathBuf {
    let source = root.join(format!("source-{format}"));
    let status = Command::new("git")
        .args([
            "init",
            "-q",
            "-b",
            "main",
            &format!("--object-format={format}"),
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    git(&source, &["config", "user.name", "Test"]);
    git(&source, &["config", "user.email", "test@example.test"]);
    write(&source.join("tracked"), "base\n");
    git(&source, &["add", "tracked"]);
    git(&source, &["commit", "-qm", "base"]);
    source
}

fn managed_clone(hub: &Hub, source: &Path, name: &str, state: &Path) -> std::path::PathBuf {
    let path = state.join("workspaces").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("git")
        .args(["clone", "-q", "--no-hardlinks"])
        .arg(source)
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    git(&path, &["config", "user.name", "Test"]);
    git(&path, &["config", "user.email", "test@example.test"]);
    let mut row =
        Record::new(name, Harness::Pi, Mode::Chat, &path, "2026-10-22T00:00:00Z").unwrap();
    row.managed_clone = true;
    row.base_repo = source.to_string_lossy().into_owned();
    hub.sleeves.record_spawned(row).unwrap();
    hub.sleeves
        .record_exit(
            name,
            "2026-10-22T00:00:01Z".into(),
            Some(0),
            0,
            false,
            false,
        )
        .unwrap();
    path
}

fn workspace_row<'a>(response: &'a Value, path: &Path) -> &'a Value {
    response["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == json!(path))
        .unwrap()
}

#[tokio::test]
async fn managed_clean_clone_is_pruned_only_after_yes_with_independent_origin() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha1");
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let clone = managed_clone(&hub, &source, "clean", &state);
    let (addr, server) = serve(hub).await;
    let (status, dry) = call(addr, "owner", json!({"workspaces":true})).await;
    assert_eq!(status, 200, "{dry}");
    assert_eq!(workspace_row(&dry, &clone)["eligible"], true, "{dry}");
    assert!(clone.exists());
    let (status, yes) = call(addr, "owner", json!({"workspaces":true,"yes":true})).await;
    assert_eq!(status, 200, "{yes}");
    assert_eq!(workspace_row(&yes, &clone)["removed"], true, "{yes}");
    assert!(!clone.exists());
    assert_eq!(
        fs::read_to_string(source.join("tracked")).unwrap(),
        "base\n"
    );
    server.abort();
}

#[tokio::test]
async fn dirty_ignored_and_unpushed_clones_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha1");
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let staged = managed_clone(&hub, &source, "staged", &state);
    write(&staged.join("tracked"), "staged\n");
    git(&staged, &["add", "tracked"]);
    let modified = managed_clone(&hub, &source, "modified", &state);
    write(&modified.join("tracked"), "modified\n");
    let untracked = managed_clone(&hub, &source, "untracked", &state);
    write(&untracked.join("new"), "new\n");
    let ignored = managed_clone(&hub, &source, "ignored", &state);
    write(&ignored.join(".git/info/exclude"), "ignored-file\n");
    write(&ignored.join("ignored-file"), "ignored\n");
    let unpushed = managed_clone(&hub, &source, "unpushed", &state);
    write(&unpushed.join("tracked"), "commit\n");
    git(&unpushed, &["add", "tracked"]);
    git(&unpushed, &["commit", "-qm", "local"]);
    let dangling = managed_clone(&hub, &source, "dangling", &state);
    write(&dangling.join("tracked"), "reset commit\n");
    git(&dangling, &["add", "tracked"]);
    git(&dangling, &["commit", "-qm", "local to reset"]);
    git(&dangling, &["reset", "--hard", "HEAD~1"]);
    let (addr, server) = serve(hub).await;
    let (status, response) = call(addr, "owner", json!({"workspaces":true,"yes":true})).await;
    assert_eq!(status, 200, "{response}");
    for path in [
        &staged, &modified, &untracked, &ignored, &unpushed, &dangling,
    ] {
        assert_ne!(
            workspace_row(&response, path)["eligible"],
            true,
            "{response}"
        );
        assert!(path.exists(), "{}", path.display());
    }
    assert_eq!(
        workspace_row(&response, &ignored)["reason"],
        "untracked or ignored files"
    );
    assert_eq!(
        workspace_row(&response, &unpushed)["reason"],
        "local Git object is not independently recoverable from origin"
    );
    assert_eq!(
        workspace_row(&response, &dangling)["reason"],
        "local Git object is not independently recoverable from origin"
    );
    assert!(fs::read_dir(state.join("workspaces"))
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".prune-")));
    server.abort();
}

#[tokio::test]
async fn sha256_clone_and_cross_sleeve_workspace_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha256");
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let clone = managed_clone(&hub, &source, "sha-clone", &state);
    let mut borrowed = Record::new(
        "borrower",
        Harness::Pi,
        Mode::Chat,
        &clone,
        "2026-10-22T00:00:02Z",
    )
    .unwrap();
    borrowed.read_only.push(ReadOnlyMount {
        source: clone.to_string_lossy().into_owned(),
        target: "/reference".into(),
    });
    hub.sleeves.record_spawned(borrowed).unwrap();
    let (addr, server) = serve(hub).await;
    let (status, busy) = call(
        addr,
        "owner",
        json!({"workspaces":true,"running":true,"yes":true}),
    )
    .await;
    assert_eq!(status, 200, "{busy}");
    assert_ne!(workspace_row(&busy, &clone)["eligible"], true);
    assert!(clone.exists());
    server.abort();
    let hub = Hub::open(&state, "owner", "test").unwrap();
    if hub.sleeves.get("borrower").unwrap().state == State::Running {
        hub.sleeves
            .record_exit(
                "borrower",
                "2026-10-22T00:00:03Z".into(),
                Some(0),
                0,
                false,
                false,
            )
            .unwrap();
    }
    let clean_sha256 = managed_clone(&hub, &source, "sha-clean", &state);
    let (addr, server) = serve(hub).await;
    let (status, dry) = call(addr, "owner", json!({"workspaces":true})).await;
    assert_eq!(status, 200, "{dry}");
    assert_ne!(workspace_row(&dry, &clone)["eligible"], true, "{dry}");
    assert_eq!(
        workspace_row(&dry, &clean_sha256)["eligible"],
        true,
        "{dry}"
    );
    let (status, yes) = call(addr, "owner", json!({"workspaces":true,"yes":true})).await;
    assert_eq!(status, 200, "{yes}");
    assert_ne!(workspace_row(&yes, &clone)["removed"], true, "{yes}");
    assert_eq!(workspace_row(&yes, &clean_sha256)["removed"], true, "{yes}");
    assert!(clone.exists());
    assert!(!clean_sha256.exists());
    server.abort();
}

#[tokio::test]
async fn file_url_origin_is_verified_and_agent_git_config_is_not_executed() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha1");
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let clone = state.join("workspaces/url-clone");
    fs::create_dir_all(clone.parent().unwrap()).unwrap();
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(format!("file://{}", source.display()))
        .arg(&clone)
        .status()
        .unwrap();
    assert!(status.success());
    let proof = dir.path().join("git-hook-ran");
    write(
        &clone.join(".git/config"),
        &format!(
            "[core]\n\tfsmonitor = sh -c 'touch {}'\n[remote \"origin\"]\n\turl = file://{}\n",
            proof.display(),
            source.display()
        ),
    );
    let mut row = Record::new(
        "url-clone",
        Harness::Pi,
        Mode::Chat,
        &clone,
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    row.managed_clone = true;
    hub.sleeves.record_spawned(row).unwrap();
    hub.sleeves
        .record_exit(
            "url-clone",
            "2026-10-22T00:00:01Z".into(),
            Some(0),
            0,
            false,
            false,
        )
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, response) = call(addr, "owner", json!({"workspaces":true,"yes":true})).await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(
        workspace_row(&response, &clone)["removed"],
        true,
        "{response}"
    );
    assert!(!proof.exists());
    server.abort();
}

#[tokio::test]
async fn stopped_registered_sleeves_retain_shared_clone_paths() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha1");
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let shared_workspace = managed_clone(&hub, &source, "shared-workspace", &state);
    let shared_mount = managed_clone(&hub, &source, "shared-mount", &state);
    let borrowed = Record::new(
        "stopped-borrower",
        Harness::Pi,
        Mode::Chat,
        &shared_workspace,
        "2026-10-22T00:00:02Z",
    )
    .unwrap();
    hub.sleeves.record_spawned(borrowed).unwrap();
    hub.sleeves
        .record_exit(
            "stopped-borrower",
            "2026-10-22T00:00:03Z".into(),
            Some(0),
            0,
            false,
            false,
        )
        .unwrap();
    let other = state.join("other-workspace");
    fs::create_dir(&other).unwrap();
    let mut mounted = Record::new(
        "stopped-mounter",
        Harness::Pi,
        Mode::Chat,
        &other,
        "2026-10-22T00:00:04Z",
    )
    .unwrap();
    mounted.read_only.push(ReadOnlyMount {
        source: shared_mount.to_string_lossy().into_owned(),
        target: "/reference".into(),
    });
    hub.sleeves.record_spawned(mounted).unwrap();
    hub.sleeves
        .record_exit(
            "stopped-mounter",
            "2026-10-22T00:00:05Z".into(),
            Some(0),
            0,
            false,
            false,
        )
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, response) = call(addr, "owner", json!({"workspaces":true,"yes":true})).await;
    assert_eq!(status, 200, "{response}");
    let workspace_entry = workspace_row(&response, &shared_workspace);
    assert_ne!(workspace_entry["eligible"], true, "{response}");
    assert!(workspace_entry["reason"]
        .as_str()
        .unwrap()
        .contains("registered sleeve stopped-borrower"));
    let mount_row = workspace_row(&response, &shared_mount);
    assert_ne!(mount_row["eligible"], true, "{response}");
    assert!(mount_row["reason"]
        .as_str()
        .unwrap()
        .contains("registered sleeve stopped-mounter"));
    assert!(shared_workspace.exists());
    assert!(shared_mount.exists());
    server.abort();
}

#[tokio::test]
async fn dry_run_and_yes_remove_only_private_stopped_cache() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path();
    let stopped = state.join("sleeves/stopped/home");
    let active = state.join("sleeves/active/home");
    write(&stopped.join(".cache/go-build/artifact"), "cache");
    write(&stopped.join(".cache/other/artifact"), "keep");
    write(&stopped.join(".pi/sessions/history"), "history");
    write(&active.join(".cache/go-build/artifact"), "active");
    let outside = state.join("outside");
    write(&outside.join("valuable"), "safe");
    symlink(&outside, stopped.join(".cache/go-build/link")).unwrap();
    let hub = Hub::open(state, "owner", "test").unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "active",
                Harness::Pi,
                Mode::Chat,
                &outside,
                "2026-10-22T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, dry) = call(addr, "owner", json!({})).await;
    assert_eq!(status, 200, "{dry}");
    let rows = dry["entries"].as_array().unwrap();
    let cache = rows
        .iter()
        .find(|row| row["path"] == json!(stopped.join(".cache/go-build")))
        .unwrap();
    assert_eq!(cache["bytes"], "5");
    assert_eq!(cache["eligible"], true);
    assert_ne!(cache["removed"], true);
    assert!(rows.iter().all(|row| row["sleeve"] != "active"));
    assert!(stopped.join(".cache/go-build/artifact").exists());
    let (status, pruned) = call(addr, "owner", json!({"yes":true})).await;
    assert_eq!(status, 200, "{pruned}");
    assert!(pruned["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["removed"] == true));
    assert!(!stopped.join(".cache/go-build").exists());
    assert!(stopped.join(".cache/other/artifact").exists());
    assert!(stopped.join(".pi/sessions/history").exists());
    assert!(active.join(".cache/go-build/artifact").exists());
    assert_eq!(
        fs::read_to_string(outside.join("valuable")).unwrap(),
        "safe"
    );
    server.abort();
}

#[tokio::test]
async fn homes_require_selection_and_running_data_is_always_kept() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path();
    let stopped = state.join("sleeves/stopped/home");
    let active = state.join("sleeves/active/home");
    write(&stopped.join(".pi/sessions/history"), "history");
    write(&active.join(".pi/sessions/history"), "active");
    let hub = Hub::open(state, "owner", "test").unwrap();
    let active_workspace = state.join("active-workspace");
    fs::create_dir(&active_workspace).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "active",
                Harness::Pi,
                Mode::Chat,
                &active_workspace,
                "2026-10-22T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, dry) = call(addr, "owner", json!({"homes":true,"running":true})).await;
    assert_eq!(status, 200, "{dry}");
    let rows = dry["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().find(|r| r["sleeve"] == "stopped").unwrap()["eligible"],
        true
    );
    let active_row = rows.iter().find(|r| r["sleeve"] == "active").unwrap();
    assert_ne!(active_row["eligible"], true);
    assert!(active_row["reason"].as_str().unwrap().contains("running"));
    let (status, yes) = call(
        addr,
        "owner",
        json!({"homes":true,"running":true,"yes":true}),
    )
    .await;
    assert_eq!(status, 200, "{yes}");
    assert!(!stopped.exists());
    assert!(active.exists());
    server.abort();
}

#[tokio::test]
async fn managed_clone_and_symlinked_cache_are_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path();
    let clone = state.join("workspaces/clone");
    write(&clone.join("unpublished.txt"), "user work");
    let outside = state.join("outside");
    write(&outside.join("artifact"), "outside");
    let home = state.join("sleeves/stopped/home");
    fs::create_dir_all(&home).unwrap();
    symlink(&outside, home.join(".cache")).unwrap();
    let (addr, server) = serve(Hub::open(state, "owner", "test").unwrap()).await;
    let (status, rows) = call(
        addr,
        "owner",
        json!({"yes":true,"caches":true,"workspaces":true}),
    )
    .await;
    assert_eq!(status, 200, "{rows}");
    let clone_row = rows["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == json!(clone))
        .unwrap();
    assert_ne!(clone_row["eligible"], true);
    assert!(clone.join("unpublished.txt").exists());
    assert_eq!(
        fs::read_to_string(outside.join("artifact")).unwrap(),
        "outside"
    );
    server.abort();
}

#[tokio::test]
async fn mounted_private_home_is_kept_even_when_its_own_sleeve_exited() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path();
    let home = state.join("sleeves/stopped/home");
    write(&home.join(".cache/go-build/artifact"), "keep");
    let workspace = state.join("active-workspace");
    fs::create_dir(&workspace).unwrap();
    let hub = Hub::open(state, "owner", "test").unwrap();
    let mut active = Record::new(
        "active",
        Harness::Pi,
        Mode::Chat,
        &workspace,
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    active.read_only.push(ReadOnlyMount {
        source: home.to_string_lossy().into_owned(),
        target: "/reference".into(),
    });
    hub.sleeves.record_spawned(active).unwrap();
    let (addr, server) = serve(hub).await;
    let (status, response) = call(
        addr,
        "owner",
        json!({"yes":true,"homes":true,"running":true}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let row = response["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == json!(home))
        .unwrap();
    assert_ne!(row["eligible"], true);
    assert!(row["reason"].as_str().unwrap().contains("mounted"));
    assert!(home.join(".cache/go-build/artifact").exists());
    server.abort();
}

#[tokio::test]
async fn ignored_flag_excuses_only_committed_gitignore_rules() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    fs::create_dir(&state).unwrap();
    let source = source(dir.path(), "sha1");
    write(&source.join(".gitignore"), "target/\n*.o\n");
    git(&source, &["add", ".gitignore"]);
    git(&source, &["commit", "-qm", "ignore build output"]);
    let hub = Hub::open(&state, "owner", "test").unwrap();
    let build = managed_clone(&hub, &source, "build", &state);
    write(&build.join("target/debug/app"), "binary\n");
    write(&build.join("target/odd\nname.o"), "odd\n");
    write(&build.join("src/main.o"), "object\n");
    let untracked = managed_clone(&hub, &source, "untracked", &state);
    write(&untracked.join("target/debug/app"), "binary\n");
    write(&untracked.join("notes.txt"), "work\n");
    let odd = managed_clone(&hub, &source, "odd", &state);
    write(&odd.join("target/app"), "binary\n");
    write(&odd.join("keep\nme"), "work\n");
    let exclude = managed_clone(&hub, &source, "exclude", &state);
    write(&exclude.join(".git/info/exclude"), "secret\n");
    write(&exclude.join("secret"), "work\n");
    let hidden = managed_clone(&hub, &source, "hidden", &state);
    write(&hidden.join("sub/.gitignore"), "*\n");
    write(&hidden.join("sub/work"), "work\n");
    let modified = managed_clone(&hub, &source, "modified", &state);
    write(&modified.join(".gitignore"), "target/\n*.o\nwork\n");
    write(&modified.join("work"), "work\n");
    let nested = managed_clone(&hub, &source, "nested", &state);
    write(&nested.join("target/inner/file"), "work\n");
    git(&nested.join("target/inner"), &["init", "-q"]);
    let linked = managed_clone(&hub, &source, "linked", &state);
    symlink("/etc", linked.join("outside")).unwrap();
    let (addr, server) = serve(hub).await;

    let kept = [
        &untracked, &odd, &exclude, &hidden, &modified, &nested, &linked,
    ];
    for flags in [
        json!({"workspaces":true,"yes":true}),
        json!({"workspaces":true,"yes":true,"ignored":true}),
    ] {
        let with_flag = flags.get("ignored").is_some();
        let (status, response) = call(addr, "owner", flags).await;
        assert_eq!(status, 200, "{response}");
        for path in kept {
            let row = workspace_row(&response, path);
            assert_ne!(row["eligible"], true, "{response}");
            let reason = if *path == modified {
                "uncommitted changes or unreadable worktree"
            } else {
                "untracked or ignored files"
            };
            assert_eq!(row["reason"], reason, "{response}");
            assert!(path.exists(), "{}", path.display());
        }
        assert_eq!(
            workspace_row(&response, &build)["removed"] == true,
            with_flag,
            "{response}"
        );
        assert_eq!(build.exists(), !with_flag);
    }
    server.abort();
}
