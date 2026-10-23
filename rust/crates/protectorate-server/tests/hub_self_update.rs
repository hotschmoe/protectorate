#![cfg(target_os = "linux")]
//! The real hub and its update guardian against a local release server:
//! verified fetch and activation, tamper refusal, rollback of a candidate that
//! never becomes healthy, and confirmation with pruning.
mod support;

use protectorate_connect_client::Client;
use protectorate_release::RUST_BUNDLE;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use support::{binary, free_port, OWNER};

type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

struct ReleaseServer {
    base: String,
    files: Files,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ReleaseServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files: Files = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (served, stopped) = (Arc::clone(&files), Arc::clone(&stop));
        let handle = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                let body = served.lock().unwrap().get(&path).cloned();
                let (status, body) = match body {
                    Some(body) => (200, body),
                    None => (404, b"missing".to_vec()),
                };
                let head = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        Self {
            base,
            files,
            stop,
            handle: Some(handle),
        }
    }

    /// Serves `assets` under `tag` with a SHA256SUMS that lists `listed`.
    fn publish(
        &self,
        tag: &str,
        listed: &BTreeMap<String, Vec<u8>>,
        mut assets: BTreeMap<String, Vec<u8>>,
    ) {
        let sums: String = listed
            .iter()
            .map(|(name, bytes)| format!("{:x}  {name}\n", Sha256::digest(bytes)))
            .collect();
        assets.insert("SHA256SUMS".into(), sums.into_bytes());
        let release = json!({
            "tag_name": tag,
            "html_url": format!("{}/notes/{tag}", self.base),
            "assets": assets.keys().map(|name| json!({
                "name": name,
                "browser_download_url": format!("{}/assets/{tag}/{name}", self.base),
            })).collect::<Vec<_>>(),
        });
        let mut files = self.files.lock().unwrap();
        files.insert(
            format!("/repos/owner/repo/releases/tags/{tag}"),
            release.to_string().into_bytes(),
        );
        for (name, bytes) in assets {
            files.insert(format!("/assets/{tag}/{name}"), bytes);
        }
    }
}

impl Drop for ReleaseServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The guardian in its own process group, so cleanup reaches every hub child
/// and candidate it started.
struct Guardian(Child);

impl Drop for Guardian {
    fn drop(&mut self) {
        let group = -(self.0.id() as i32);
        unsafe { libc::kill(group, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        unsafe { libc::kill(group, libc::SIGKILL) };
        let _ = self.0.wait();
    }
}

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn web_archive(root: &Path, tag: &str, link: bool) -> Vec<u8> {
    let dir = root.join(format!("web-{tag}"));
    fs::create_dir_all(&dir).unwrap();
    if link {
        std::os::unix::fs::symlink("/etc/passwd", dir.join("index.html")).unwrap();
    } else {
        fs::write(dir.join("index.html"), format!("<html>{tag}</html>")).unwrap();
    }
    let archive = root.join(format!("web-{tag}.tar.gz"));
    let status = Command::new("tar")
        .args(["--format=ustar", "-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&dir)
        .arg("index.html")
        .status()
        .unwrap();
    assert!(status.success());
    fs::read(archive).unwrap()
}

/// A complete Rust bundle whose server is `server`, plus its web archive.
fn bundle(root: &Path, tag: &str, server: String, link: bool) -> BTreeMap<String, Vec<u8>> {
    let mut assets = BTreeMap::new();
    for component in RUST_BUNDLE {
        let name = protectorate_release::rust_bundle_name(component, "linux", arch()).unwrap();
        let body = if component == "protectorate-rs" {
            server.clone()
        } else {
            format!("#!/bin/sh\n# {component} {tag}\n")
        };
        assets.insert(name, body.into_bytes());
    }
    assets.insert(
        format!("protectorate-web-{tag}.tar.gz"),
        web_archive(root, tag, link),
    );
    assets
}

/// A candidate hub that serves GetStatus with its own version on the plain
/// listener the guardian passes it.
fn healthy_candidate(version: &str) -> String {
    format!(
        r#"#!/usr/bin/python3
import http.server, json, sys
host, port = sys.argv[sys.argv.index('--listen') + 1].rsplit(':', 1)
body = json.dumps({{'version': '{version}'}}).encode()
class Status(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length', 0)))
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.ThreadingHTTPServer((host, int(port)), Status).serve_forever()
"#
    )
}

fn wait_until(what: &str, log: &Path, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{what} timed out: {}",
            fs::read_to_string(log).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn pointer(releases: &Path, name: &str) -> String {
    fs::read_link(releases.join(name))
        .map(|target| target.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn update(client: &Client, tag: &str) -> Result<Vec<Value>, String> {
    let mut events = Vec::new();
    client
        .stream(
            "/protectorate.v1.ReleaseService/UpdateHub",
            &json!({"version": tag}),
            |event| {
                events.push(event);
                Ok(())
            },
        )
        .map(|_| events)
        .map_err(|error| error.message)
}

#[test]
fn guardian_installs_verified_releases_refuses_tampering_and_rolls_back() {
    let root = tempfile::tempdir().unwrap();
    let (home, user_home, releases) = (
        root.path().join("hub"),
        root.path().join("user"),
        root.path().join("releases"),
    );
    fs::create_dir_all(&user_home).unwrap();
    let token = root.path().join("owner-token");
    fs::write(&token, OWNER).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let running: Value = serde_json::from_slice(
        &Command::new(binary())
            .args(["version", "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let running = running["version"].as_str().unwrap().to_owned();
    let api = ReleaseServer::start();
    let plain = free_port();
    let log = root.path().join("guardian.log");
    let _guardian = Guardian(
        Command::new(binary())
            .args(["serve", "--home"])
            .arg(&home)
            .arg("--owner-token-file")
            .arg(&token)
            .args(["--listen", &plain, "--release-api-url", &api.base])
            .args(["--release-repo", "owner/repo", "--release-dir"])
            .arg(&releases)
            .args(["--tmux-socket", &format!("p-update-{}", std::process::id())])
            .env("HOME", &user_home)
            .env_remove("PROTECTORATE_HOME")
            .env_remove("PROTECTORATE_HUB_GUARD_CHILD")
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let client = Client::new(&format!("http://{plain}"), OWNER, Duration::from_secs(60)).unwrap();
    let serves = |version: &str| {
        client
            .call("/protectorate.v1.HubService/GetStatus", &json!({}))
            .is_ok_and(|status| status["version"] == version)
    };
    wait_until("hub start", &log, || serves(&running));

    let candidate = |tag: &str| format!("#!/bin/sh\n# hub {tag}\nexec sleep 600\n");
    let assets = bundle(root.path(), &running, candidate(&running), false);
    api.publish(&running, &assets, assets.clone());
    let events = update(&client, &running).unwrap();
    assert_eq!(
        events.last().unwrap()["phase"],
        "UPDATE_PHASE_CONFIRMED",
        "{events:?}"
    );
    assert_eq!(pointer(&releases, "current"), running);
    let page = ureq::get(&format!("http://{plain}/"))
        .call()
        .unwrap()
        .into_body()
        .read_to_string()
        .unwrap();
    assert_eq!(page, format!("<html>{running}</html>"));

    let server = format!("protectorate-rs-linux-{}", arch());
    for (tag, refusal) in [
        ("v9.0.1", "checksum mismatch"),
        ("v9.0.2", "missing or invalid checksum"),
        ("v9.0.3", "unsupported entry"),
    ] {
        let mut served = bundle(root.path(), tag, candidate(tag), tag == "v9.0.3");
        let mut listed = served.clone();
        match tag {
            "v9.0.1" => {
                served.insert(server.clone(), b"#!/bin/sh\necho swapped\n".to_vec());
            }
            "v9.0.2" => {
                listed.remove(&server);
            }
            _ => {}
        }
        api.publish(tag, &listed, served);
        let error = update(&client, tag).unwrap_err();
        assert!(error.contains(refusal), "{tag}: {error}");
        assert!(!releases.join(tag).exists(), "{tag} was published");
        assert!(!releases.join(".hub-trial.json").exists());
        assert_eq!(pointer(&releases, "current"), running);
        assert!(serves(&running));
    }

    let assets = bundle(root.path(), "v9.1.0", "#!/bin/sh\nexit 1\n".into(), false);
    api.publish("v9.1.0", &assets, assets.clone());
    let events = update(&client, "v9.1.0").unwrap();
    assert_eq!(events.last().unwrap()["phase"], "UPDATE_PHASE_RESTARTING");
    wait_until("rollback to the running release", &log, || {
        !releases.join(".hub-trial.json").exists() && serves(&running)
    });
    assert_eq!(pointer(&releases, "current"), running);
    assert!(fs::read_to_string(&log)
        .unwrap()
        .contains("hub candidate exited before health confirmation"));

    let assets = bundle(root.path(), "v9.2.0", healthy_candidate("v9.2.0"), false);
    api.publish("v9.2.0", &assets, assets.clone());
    let events = update(&client, "v9.2.0").unwrap();
    assert_eq!(events.last().unwrap()["phase"], "UPDATE_PHASE_RESTARTING");
    wait_until("confirmed candidate", &log, || {
        serves("v9.2.0") && !releases.join(".hub-trial.json").exists()
    });
    assert_eq!(pointer(&releases, "current"), "v9.2.0");
    assert_eq!(pointer(&releases, "previous"), running);
    let mut kept = fs::read_dir(&releases)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && name != "current" && name != "previous")
        .collect::<Vec<_>>();
    kept.sort();
    let mut expected = vec![running.clone(), "v9.2.0".to_owned()];
    expected.sort();
    assert_eq!(kept, expected, "the failed v9.1.0 trial was not pruned");
    assert_eq!(
        fs::read_to_string(releases.join("current/web/index.html")).unwrap(),
        "<html>v9.2.0</html>"
    );
}
