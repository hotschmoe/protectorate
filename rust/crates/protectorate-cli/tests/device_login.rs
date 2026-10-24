#![cfg(unix)]

//! `login` with no harness against a real hub: the pairing round trip, the
//! private credential file it leaves behind, and what ordinary commands do
//! with that credential when no token flag or token environment is set.
use crate::common;

use common::cli as owner_cli;
use protectorate_hub::Hub;
use protectorate_identity::Store;
use serde_json::Value;
use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";

async fn serve_hub() -> (
    String,
    std::sync::Arc<Store>,
    tempfile::TempDir,
    tokio::task::JoinHandle<()>,
) {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), OWNER, "test")
        .unwrap()
        .with_loopback_device_auth();
    let identity = hub.identity.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (url, identity, home, server)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// The real CLI binary with a disposable HOME and no hub credentials, so only
/// a saved device credential can authenticate it.
fn cli(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_protectorate"));
    command
        .env("HOME", home)
        .env_remove("PROTECTORATE_HOME")
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .env_remove("PROTECTORATE_HUB")
        .args(args);
    command
}

fn run_cli(home: &Path, args: &[&str]) -> (bool, String, String) {
    let output = cli(home, args).output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn spawn_login(home: &Path, args: &[&str]) -> Child {
    cli(home, args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Read both streams off the child so a waiting login cannot fill a pipe, and
/// never wait past the hub's own ten-minute pairing window.
fn finish(mut child: Child) -> (bool, String, String) {
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = mpsc::channel::<(&'static str, String)>();
    let out = spawn_reader(stdout, "out", sender.clone());
    let err = spawn_reader(stderr, "err", sender.clone());
    let deadline = Instant::now() + Duration::from_secs(300);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("login did not finish");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let status = child.wait().unwrap();
    drop(sender);
    out.join().unwrap();
    err.join().unwrap();
    let mut text = String::new();
    let mut errors = String::new();
    for (key, value) in receiver.iter() {
        if key == "out" {
            text = value;
        } else {
            errors = value;
        }
    }
    (status.success(), text, errors)
}

fn spawn_reader<S: Read + Send + 'static>(
    stream: S,
    key: &'static str,
    sender: mpsc::Sender<(&'static str, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut text = String::new();
        BufReader::new(stream).read_to_string(&mut text).unwrap();
        let _ = sender.send((key, text));
    })
}

/// The user code the hub is holding for `name`, once the CLI has started it.
fn wait_for_code(hub: &str, name: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        let raw: Value =
            serde_json::from_str(&owner_cli(hub, OWNER, ["--json", "devices", "pending"]).unwrap())
                .unwrap();
        let requests = raw.get("requests").and_then(Value::as_array);
        if let Some(request) = requests
            .iter()
            .flat_map(|requests| requests.iter())
            .find(|request| request["name"] == name)
        {
            return request["userCode"].as_str().unwrap().to_owned();
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("no pending device request for {name}");
}

fn credential_dir(home: &Path) -> PathBuf {
    home.join(".protectorate").join("credentials")
}

fn only_credential(home: &Path) -> PathBuf {
    let dir = credential_dir(home);
    let mut files = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1, "{}", dir.display());
    files.pop().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn login_approves_saves_and_ordinary_commands_reuse_the_credential() {
    let (hub, _identity, _hub_home, server) = serve_hub().await;
    let cli_home = tempfile::tempdir().unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let cli_home = cli_home.path();
        let child = spawn_login(
            cli_home,
            &[
                "--hub",
                &hub,
                "login",
                "--name",
                "desk",
                "--platform",
                "linux/amd64",
            ],
        );
        let code = wait_for_code(&hub, "desk");
        assert_eq!(
            owner_cli(&hub, OWNER, ["devices", "approve", &code]).unwrap(),
            format!("approved {code}\n")
        );
        let (ok, stdout, stderr) = finish(child);
        assert!(ok, "{stderr}");
        assert!(stdout.contains(&format!("Code: {code}")), "{stdout}");
        assert!(
            stdout.contains("approve only if this code matches"),
            "{stdout}"
        );
        assert!(
            stdout.contains(&format!("protectorate devices approve {code}")),
            "{stdout}"
        );
        assert!(stdout.contains("Waiting for approval until"), "{stdout}");
        assert!(
            stdout.contains(&format!("Connected to {hub}. Credential saved in")),
            "{stdout}"
        );

        let path = only_credential(cli_home);
        let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["hub"], hub);
        assert_eq!(saved["name"], "desk");
        assert_eq!(saved["platform"], "linux/amd64");
        assert!(saved["deviceId"].as_str().is_some_and(|id| !id.is_empty()));
        let secret = saved["key"].as_str().unwrap();
        assert!(!secret.is_empty());
        assert!(saved.get("secret").is_none(), "{saved}");
        assert!(!stdout.contains(secret), "{stdout}");
        assert!(!stderr.contains(secret), "{stderr}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(credential_dir(cli_home))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        // No token flag and no token environment: the saved key signs every
        // request, as a device, not as the owner.
        let (ok, stdout, stderr) = run_cli(cli_home, &["--hub", &hub, "devices", "ls"]);
        assert!(ok, "{stderr}");
        assert!(stdout.contains("\tdesk\tlinux/amd64\t"), "{stdout}");
        assert!(!stdout.contains(secret) && !stderr.contains(secret));
        let (ok, _, stderr) = run_cli(cli_home, &["--hub", &hub, "status"]);
        assert!(ok, "{stderr}");
        let (ok, stdout, stderr) = run_cli(cli_home, &["--hub", &hub, "backlog", "add", "note"]);
        assert!(ok, "{stdout}{stderr}");

        // An explicit token file, then the token environments, outrank it.
        let bogus = cli_home.join("bogus.token");
        fs::write(&bogus, "not-a-device-credential\n").unwrap();
        let (ok, _, stderr) = run_cli(
            cli_home,
            &[
                "--hub",
                &hub,
                "--token-file",
                bogus.to_str().unwrap(),
                "status",
            ],
        );
        assert!(!ok, "an explicit token file must win");
        assert!(stderr.contains("invalid credential"), "{stderr}");
        let output = cli(cli_home, &["--hub", &hub, "status"])
            .env("PROTECTORATE_TOKEN", "not-a-device-credential")
            .output()
            .unwrap();
        assert!(!output.status.success(), "PROTECTORATE_TOKEN must win");
        let output = cli(cli_home, &["--hub", &hub, "status"])
            .env("PROTECTORATE_TOKEN_FILE", &bogus)
            .output()
            .unwrap();
        assert!(!output.status.success(), "PROTECTORATE_TOKEN_FILE must win");

        // A home with no credential for this hub still has nothing to offer.
        let other = tempfile::tempdir().unwrap();
        let (ok, _, stderr) = run_cli(other.path(), &["--hub", &hub, "status"]);
        assert!(!ok, "{stderr}");
        assert!(stderr.contains("no credential"), "{stderr}");
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn login_json_output_never_carries_the_key() {
    let (hub, _identity, _hub_home, server) = serve_hub().await;
    let cli_home = tempfile::tempdir().unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let cli_home = cli_home.path();
        let child = spawn_login(
            cli_home,
            &["--json", "--hub", &hub, "login", "--name", "json"],
        );
        let code = wait_for_code(&hub, "json");
        owner_cli(&hub, OWNER, ["devices", "approve", &code]).unwrap();
        let (ok, stdout, stderr) = finish(child);
        assert!(ok, "{stderr}");
        let lines = stdout.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2, "{stdout}");
        let started: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(started["userCode"], code);
        assert!(started["expiresAt"].is_string());
        let done: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(done["status"], "approved");
        assert_eq!(done["device"]["name"], "json");
        let path = only_credential(cli_home);
        assert_eq!(
            done["credentialFile"].as_str().unwrap(),
            path.to_string_lossy()
        );
        let raw = fs::read_to_string(&path).unwrap();
        let secret = raw
            .split('\"')
            .skip_while(|part| *part != "key")
            .nth(2)
            .unwrap_or_default()
            .to_owned();
        assert!(!secret.is_empty(), "{raw}");
        assert!(!stdout.contains(&secret), "{stdout}");
        assert!(!stderr.contains(&secret), "{stderr}");
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn login_reports_a_denied_request_and_keeps_no_credential() {
    let (hub, _identity, _hub_home, server) = serve_hub().await;
    let cli_home = tempfile::tempdir().unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let cli_home = cli_home.path();
        let child = spawn_login(cli_home, &["--hub", &hub, "login", "--name", "watched"]);
        let code = wait_for_code(&hub, "watched");
        assert_eq!(
            owner_cli(&hub, OWNER, ["devices", "deny", &code]).unwrap(),
            format!("denied {code}\n")
        );
        let (ok, stdout, stderr) = finish(child);
        assert!(!ok, "{stdout}");
        assert!(stdout.contains(&format!("Code: {code}")), "{stdout}");
        assert!(
            stderr.contains("device authorization denied by a trusted device"),
            "{stderr}"
        );
        assert!(!credential_dir(cli_home).exists(), "{stdout}");
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn login_stops_when_the_hub_lets_the_request_expire() {
    let (hub, identity, _hub_home, server) = serve_hub().await;
    let cli_home = tempfile::tempdir().unwrap();
    let result = tokio::task::spawn_blocking(move || {
        let cli_home = cli_home.path();
        let child = spawn_login(cli_home, &["--hub", &hub, "login", "--name", "slow"]);
        wait_for_code(&hub, "slow");
        // The hub drops a request once its own window has passed. Sweeping with
        // a time past that window is how the request reaches the CLI as expired.
        identity.sweep(now_unix() + 700).unwrap();
        let (ok, _, stderr) = finish(child);
        assert!(!ok, "{stderr}");
        assert!(
            stderr.contains("device authorization expired; run login again to retry"),
            "{stderr}"
        );
        assert!(!credential_dir(cli_home).exists());
    })
    .await;
    server.abort();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_credential_readable_by_others_is_refused() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let (hub, _identity, _hub_home, server) = serve_hub().await;
        let cli_home = tempfile::tempdir().unwrap();
        let result = tokio::task::spawn_blocking(move || {
            let cli_home = cli_home.path();
            let child = spawn_login(cli_home, &["--hub", &hub, "login", "--name", "loose"]);
            let code = wait_for_code(&hub, "loose");
            owner_cli(&hub, OWNER, ["devices", "approve", &code]).unwrap();
            let (ok, _, stderr) = finish(child);
            assert!(ok, "{stderr}");
            let path = only_credential(cli_home);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            let (ok, _, stderr) = run_cli(cli_home, &["--hub", &hub, "status"]);
            assert!(!ok, "{stderr}");
            assert!(stderr.contains("readable by others"), "{stderr}");
        })
        .await;
        server.abort();
        result.unwrap();
    }
}

#[test]
fn login_without_a_home_or_a_token_names_both_fixes() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate"))
        .args(["--hub", "http://127.0.0.1:1", "status"])
        .env("HOME", home.path())
        .env_remove("PROTECTORATE_HOME")
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no credential"), "{stderr}");
    assert!(
        stderr.contains("run login --hub http://127.0.0.1:1"),
        "{stderr}"
    );
}

#[test]
fn a_credential_for_another_hub_or_without_a_key_is_refused() {
    use sha2::{Digest, Sha256};
    let hub = "http://127.0.0.1:1";
    let key: String = Sha256::digest(hub.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".protectorate").join("credentials");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{key}.json"));
    let write = |body: &str| {
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
    };
    write("{\"hub\":\"http://other-hub\",\"key\":\"abc\"}\n");
    let (ok, _, stderr) = run_cli(home.path(), &["--hub", hub, "status"]);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("belongs to another hub"), "{stderr}");
    write("{\"hub\":\"http://127.0.0.1:1\",\"key\":\"\"}\n");
    let (ok, _, stderr) = run_cli(home.path(), &["--hub", hub, "status"]);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("has no key"), "{stderr}");
}
