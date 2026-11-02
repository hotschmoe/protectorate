//! Updates and doctor, all real binaries against a fake GitHub release served on loopback
//! (`release_api`), never the live services and never a real update command:
//!
//! - The hub under a test-played supervisor that does what the systemd unit does: run
//!   `releases/previous/protectorate hub probation`, then `releases/current/protectorate serve`,
//!   restart on a failing exit. `hub update` stages, switches and restarts on probation; the new
//!   hub's health check keeps it; `--rollback` swaps back; a release whose serve crashes is
//!   switched back by the prestart after three starts; one that fails to start (its listener is
//!   taken) switches itself back.
//! - A node joined through the hub's headscale, under the same kind of supervisor: `node update`
//!   has it fetch, check, switch and restart, and a hub request confirms it; with the hub killed
//!   right after the switch, the node switches itself back when its window passes.
//! - Doctor: a harness whose `--version` fails is refused for spawn and start (409), Envoy on it
//!   parks, and a `doctor --refresh` that finds it fixed starts Envoy again.
//! - Harness updates: fake claude, codex, grok and pi programs record their update commands; the
//!   24 h loop runs each once (and not again after a restart), and `harness update pi` waits for a
//!   pi sleeve mid-turn.
//!
//! Release "binaries" are bash wrappers: they print their tag for the version check, else exec
//! the real binary under the wrapper's own argv[0], as systemd would start it.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use common::{Hub, LONG, Run, bin, eventually, free_port, tail, temp_repo};
use pt_http::{Config, Request, Response, Server, ShutdownHandle};
use pt_json::{Value, obj};

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => panic!("no release assets for {a}"),
    }
}

fn dir(name: &str) -> PathBuf {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() % 1_000_000);
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("upd-{name}-{}-{t}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_exec(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A release binary: `<check>` prints `<program> <tag>`; anything else runs `real` with `extra`
/// appended, unless `crash` says serve exits at once.
fn wrapper(
    program: &str,
    check: &str,
    tag: &str,
    real: &Path,
    extra: &str,
    crash: bool,
) -> Vec<u8> {
    let crash = if crash {
        "for a in \"$@\"; do [ \"$a\" = serve ] && { echo \"$0: crashing\" >&2; exit 1; }; done\n"
    } else {
        ""
    };
    format!(
        "#!/bin/bash\nif [ \"$1\" = {check} ] && [ $# = 1 ]; then echo '{program} {tag}'; exit 0; fi\n{crash}exec -a \"$0\" '{}' \"$@\" {extra}\n",
        real.display()
    )
    .into_bytes()
}

struct Rel {
    tag: String,
    assets: BTreeMap<String, Vec<u8>>,
}

fn release(tag: &str, files: Vec<(String, Vec<u8>)>) -> Rel {
    let mut assets: BTreeMap<String, Vec<u8>> = files.into_iter().collect();
    assets.insert("VERSION".into(), format!("{tag}\n").into_bytes());
    let mut sums = String::new();
    for (name, bytes) in &assets {
        sums.push_str(&format!(
            "{}  {name}\n",
            pt_hash::to_hex(&pt_hash::sha256(bytes))
        ));
    }
    assets.insert("SHA256SUMS".into(), sums.into_bytes());
    Rel {
        tag: tag.into(),
        assets,
    }
}

/// GitHub's releases API for `o/r`, as much of it as pt-update reads.
struct Fake {
    url: String,
    handle: ShutdownHandle,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.handle.shutdown(Duration::from_secs(1));
    }
}

fn release_json(base: &str, r: &Rel) -> String {
    let assets: Vec<String> = r
        .assets
        .iter()
        .map(|(name, bytes)| {
            format!(
                r#"{{"name":"{name}","size":{},"browser_download_url":"{base}/dl/{}/{name}"}}"#,
                bytes.len(),
                r.tag
            )
        })
        .collect();
    format!(
        r#"{{"tag_name":"{}","draft":false,"prerelease":false,"assets":[{}]}}"#,
        r.tag,
        assets.join(",")
    )
}

fn route(base: &str, rels: &[Rel], req: &Request) -> Response {
    let path = req.path();
    if path == "/repos/o/r/releases" {
        let all: Vec<String> = rels.iter().map(|r| release_json(base, r)).collect();
        return Response::json(200, format!("[{}]", all.join(",")));
    }
    if let Some(tag) = path.strip_prefix("/repos/o/r/releases/tags/") {
        return match rels.iter().find(|r| r.tag == tag) {
            Some(r) => Response::json(200, release_json(base, r)),
            None => Response::not_found(),
        };
    }
    let found = path
        .strip_prefix("/dl/")
        .and_then(|rest| rest.split_once('/'))
        .and_then(|(tag, name)| rels.iter().find(|r| r.tag == tag)?.assets.get(name));
    match found {
        Some(bytes) => Response::bytes(200, "application/octet-stream", bytes.clone()),
        None => Response::not_found(),
    }
}

fn serve_releases(rels: Vec<Rel>) -> Fake {
    let rels = Arc::new(rels);
    let base = Arc::new(std::sync::OnceLock::<String>::new());
    let (r2, b2) = (rels.clone(), base.clone());
    let server = Server::bind_tcp(
        "127.0.0.1:0".parse().unwrap(),
        Config::default(),
        move |req, _| route(b2.get().expect("base url"), &r2, req),
    )
    .unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    base.set(url.clone()).unwrap();
    let handle = server.shutdown_handle();
    thread::spawn(move || server.run());
    Fake { url, handle }
}

/// Installs `bytes` as release `tag` of `program` under `<root>/releases` and points
/// `current` at it, as an installer does.
fn install(root: &Path, tag: &str, program: &str, bytes: &[u8]) {
    let d = root.join("releases").join(tag);
    fs::create_dir_all(&d).unwrap();
    write_exec(&d.join(program), std::str::from_utf8(bytes).unwrap());
    std::os::unix::fs::symlink(tag, root.join("releases/current")).unwrap();
}

fn pointer(root: &Path, name: &str) -> Option<String> {
    fs::read_link(root.join("releases").join(name))
        .ok()
        .map(|p| p.display().to_string())
}

fn probation(root: &Path) -> Value {
    fs::read_to_string(root.join("releases/probation.json"))
        .ok()
        .and_then(|t| pt_json::parse(&t).ok())
        .unwrap_or(Value::Null)
}

/// One start: the release `current` named, and how it exited (None while running).
#[derive(Debug, Clone)]
struct Start {
    release: String,
    code: Option<i32>,
}

/// Does what the systemd units do: before each start the previous release's `probation`
/// command, then `releases/current/<program> <args>` with that path as argv[0]; restart after
/// `delay` on any exit but 0.
struct Supervisor {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    starts: Arc<Mutex<Vec<Start>>>,
    log: PathBuf,
    thread: Option<thread::JoinHandle<()>>,
}

impl Supervisor {
    fn start(
        root: &Path,
        program: &'static str,
        prestart: Vec<String>,
        args: Vec<String>,
        env: Vec<(String, String)>,
        delay: Duration,
    ) -> Supervisor {
        let mut s = Supervisor {
            stop: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
            starts: Arc::new(Mutex::new(Vec::new())),
            log: root.join("supervised.log"),
            thread: None,
        };
        let (stop, child, starts) = (s.stop.clone(), s.child.clone(), s.starts.clone());
        let root = root.to_path_buf();
        let log = s.log.clone();
        s.thread = Some(thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let previous = root.join("releases/previous").join(program);
                if previous.exists() {
                    let out = Command::new(&previous).args(&prestart).output().unwrap();
                    append(
                        &log,
                        &format!(
                            "prestart: {}{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        ),
                    );
                }
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let release = pointer(&root, "current").unwrap_or_default();
                append(&log, &format!("start {release}\n"));
                let err = fs::OpenOptions::new().append(true).open(&log).unwrap();
                let mut cmd = Command::new(root.join("releases/current").join(program));
                cmd.args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(err);
                for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("CLAUDE")) {
                    cmd.env_remove(k);
                }
                cmd.env_remove("PROTECTORATE_TOKEN")
                    .env_remove("PROTECTORATE_SOCKET");
                for (k, v) in &env {
                    cmd.env(k, v);
                }
                let c = cmd.spawn().unwrap();
                let i = {
                    let mut st = starts.lock().unwrap();
                    st.push(Start {
                        release,
                        code: None,
                    });
                    st.len() - 1
                };
                *child.lock().unwrap() = Some(c);
                let code = loop {
                    let mut g = child.lock().unwrap();
                    let Some(c) = g.as_mut() else { return };
                    if stop.load(Ordering::SeqCst) {
                        let _ = c.kill();
                        let _ = c.wait();
                        *g = None;
                        return;
                    }
                    if let Ok(Some(st)) = c.try_wait() {
                        *g = None;
                        break st.code().unwrap_or(-1);
                    }
                    drop(g);
                    thread::sleep(Duration::from_millis(50));
                };
                starts.lock().unwrap()[i].code = Some(code);
                append(&log, &format!("exited {code}\n"));
                if code == 0 {
                    return;
                }
                thread::sleep(delay);
            }
        }));
        s
    }

    fn starts(&self) -> Vec<Start> {
        self.starts.lock().unwrap().clone()
    }

    fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Waits for the supervised process to be running `release` after `after` starts.
    fn running(&self, release: &str, after: usize, secs: u64) {
        let what = format!("{release} running after start {after}");
        let ok = (0..secs * 10).any(|_| {
            let st = self.starts();
            let hit = st
                .get(after..)
                .is_some_and(|s| s.iter().any(|s| s.release == release && s.code.is_none()));
            if !hit {
                thread::sleep(Duration::from_millis(100));
            }
            hit
        });
        assert!(
            ok,
            "{what}: {:?}\n--- supervised log ---\n{}",
            self.starts(),
            tail(&self.log_text(), 60)
        );
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn append(path: &Path, text: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
    }
}

fn cli(home: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .arg("--home")
        .arg(home)
        .args(args)
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_SOCKET")
        .output()
        .unwrap();
    Run {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn hub_started_ms(home: &Path) -> Option<u64> {
    let r = cli(home, &["--json", "hub", "info"]);
    (r.status == 0).then(|| r.json()["started_at_ms"].as_u64())?
}

/// Waits until a probation that started after `after_ms` is confirmed; returns its start.
fn confirmed_after(home: &Path, after_ms: u64, what: &str) -> u64 {
    eventually(what, Duration::from_secs(60), || {
        let p = probation(home);
        let switched = p["switched_ms"].as_u64()?;
        (switched > after_ms && p["contacted_ms"].as_u64().is_some()).then_some(switched)
    })
}

fn events(home: &Path) -> Vec<String> {
    let v = cli(home, &["--json", "event", "list", "--limit", "1000"]).json();
    v["events"]
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|e| e["text"].as_str().map(String::from))
        .collect()
}

#[test]
fn hub_update_switches_on_probation_and_rolls_back_bad_releases() {
    let real = PathBuf::from(bin());
    let a = arch();
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    let asset = format!("protectorate-linux-{a}");
    let hub_rel = |tag: &str, extra: &str, crash: bool| {
        release(
            tag,
            vec![(
                asset.clone(),
                wrapper("protectorate", "version", tag, &real, extra, crash),
            )],
        )
    };
    let fake = serve_releases(vec![
        hub_rel("v0.0.2", "", false),
        hub_rel("v0.0.3", "", true),
        hub_rel("v0.0.4", &format!("--listen 127.0.0.1:{busy_port}"), false),
    ]);

    let root = dir("hub");
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let config = obj! {
        // Envoy's harness is refused by doctor, so it parks instead of launching anything.
        "envoy" => obj! { "harness" => "claude" },
        "claude_program" => "/nonexistent/program",
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
        "release_api" => fake.url.as_str(),
        "release_repo" => "o/r",
    };
    fs::write(home.join("config.json"), config.to_string()).unwrap();
    install(
        &home,
        "v0.0.1",
        "protectorate",
        &wrapper("protectorate", "version", "v0.0.1", &real, "", false),
    );
    let home_s = home.display().to_string();
    let sup = Supervisor::start(
        &home,
        "protectorate",
        vec![
            "--home".into(),
            home_s.clone(),
            "hub".into(),
            "probation".into(),
        ],
        vec![
            "--home".into(),
            home_s.clone(),
            "serve".into(),
            "--listen".into(),
            "off".into(),
        ],
        Vec::new(),
        Duration::from_millis(300),
    );
    let up = |what: &str| {
        eventually(what, Duration::from_secs(60), || hub_started_ms(&home));
    };
    up("the hub on v0.0.1");
    assert!(
        fs::read_link(home.join("run/bin/protectorate"))
            .is_ok_and(|t| t.ends_with("releases/current/protectorate")),
        "the CLI link follows releases/current"
    );

    let check = cli(
        &home,
        &["--json", "hub", "update", "--check", "--version", "v0.0.2"],
    )
    .json();
    assert_eq!(check["available"].as_bool(), Some(true), "{check}");
    assert_eq!(check["restarting"].as_bool(), Some(false), "{check}");
    assert_eq!(check["current"].as_str(), Some("v0.0.1"), "{check}");

    // A good release: staged, switched, restarted, kept once its health check passes.
    let n = sup.starts().len();
    let u = cli(&home, &["--json", "hub", "update", "--version", "v0.0.2"]).json();
    assert_eq!(u["restarting"].as_bool(), Some(true), "{u}");
    sup.running("v0.0.2", n, 60);
    assert_eq!(sup.starts()[n - 1].code, Some(75), "the old hub exits 75");
    let first = confirmed_after(&home, 0, "v0.0.2 confirmed");
    up("the hub on v0.0.2");
    assert_eq!(pointer(&home, "previous").as_deref(), Some("v0.0.1"));
    assert!(
        events(&home)
            .iter()
            .any(|e| e.contains("v0.0.2 passed its health check")),
        "{:?}",
        events(&home)
    );

    // Rollback swaps the pointers and restarts; updating again reuses the staged release.
    let n = sup.starts().len();
    let r = cli(&home, &["--json", "hub", "update", "--rollback"]).json();
    assert_eq!(r["target"].as_str(), Some("v0.0.1"), "{r}");
    sup.running("v0.0.1", n, 60);
    up("the hub back on v0.0.1");
    let n = sup.starts().len();
    let again = cli(&home, &["--json", "hub", "update", "--version", "v0.0.2"]).json();
    assert!(
        again["note"]
            .as_str()
            .unwrap_or("")
            .contains("already staged"),
        "{again}"
    );
    sup.running("v0.0.2", n, 60);
    confirmed_after(&home, first, "v0.0.2 confirmed again");
    up("the hub on v0.0.2 again");

    // A release whose serve dies at once: the prestart counts three starts, then switches back.
    let n = sup.starts().len();
    let u = cli(&home, &["--json", "hub", "update", "--version", "v0.0.3"]).json();
    assert_eq!(u["restarting"].as_bool(), Some(true), "{u}");
    sup.running("v0.0.2", n + 1, 90);
    let crashed = sup.starts()[n..]
        .iter()
        .filter(|s| s.release == "v0.0.3")
        .count();
    assert_eq!(crashed, 3, "{:?}", sup.starts());
    let p = probation(&home);
    assert_eq!(p["version"].as_str(), Some("v0.0.3"), "{p}");
    assert!(p["rolled_back_ms"].as_u64().is_some(), "{p}");
    assert!(p["reason"].as_str().unwrap_or("").contains("starts"), "{p}");
    up("the hub after the crash rollback");

    // A release that cannot open its listener switches itself back before it exits.
    let n = sup.starts().len();
    cli(&home, &["--json", "hub", "update", "--version", "v0.0.4"]).json();
    sup.running("v0.0.2", n + 1, 90);
    let p = probation(&home);
    assert_eq!(p["version"].as_str(), Some("v0.0.4"), "{p}");
    assert!(
        p["reason"]
            .as_str()
            .unwrap_or("")
            .contains("failed to start"),
        "{p}"
    );
    let v4: Vec<_> = sup.starts()[n..]
        .iter()
        .filter(|s| s.release == "v0.0.4")
        .cloned()
        .collect();
    assert_eq!(v4.len(), 1, "{:?}", sup.starts());
    assert_ne!(v4[0].code, Some(0));
    up("the hub after the failed start");
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.2"));
    let not_from_releases = Command::new(bin())
        .args(["--home", &home_s, "hub", "probation"])
        .output()
        .unwrap();
    assert!(not_from_releases.status.success());
    drop(busy);
    drop(sup);
}

/// A hub killed at each step of a switch on probation, or of the rollback that ends one
/// (`PT_TEST_CRASH_AT`, the store's crash knob), comes back on a release that worked with the
/// way back intact: never on the new release with no trial, never with `previous` lost.
#[test]
fn release_switches_survive_a_crash_at_each_step() {
    let real = PathBuf::from(bin());
    let asset = format!("protectorate-linux-{}", arch());
    let wrap = |tag: &str| wrapper("protectorate", "version", tag, &real, "", false);
    let fake = serve_releases(vec![release("v0.0.2", vec![(asset, wrap("v0.0.2"))])]);
    let root = dir("crash");
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let config = obj! {
        "envoy" => obj! { "harness" => "claude" },
        "claude_program" => "/nonexistent/program",
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
        "release_api" => fake.url.as_str(),
        "release_repo" => "o/r",
    };
    fs::write(home.join("config.json"), config.to_string()).unwrap();
    install(&home, "v0.0.1", "protectorate", &wrap("v0.0.1"));
    let home_s = home.display().to_string();
    let words = |w: &[&str]| -> Vec<String> {
        ["--home", home_s.as_str()]
            .iter()
            .chain(w)
            .map(|s| s.to_string())
            .collect()
    };

    // Killed right after the probation is written: still on v0.0.1, and no trial is open.
    let sup = Supervisor::start(
        &home,
        "protectorate",
        words(&["hub", "probation"]),
        words(&["serve", "--listen", "off"]),
        vec![("PT_TEST_CRASH_AT".into(), "probation-written".into())],
        Duration::from_millis(300),
    );
    eventually("the hub on v0.0.1", Duration::from_secs(60), || {
        hub_started_ms(&home)
    });
    let n = sup.starts().len();
    cli(&home, &["--json", "hub", "update", "--version", "v0.0.2"]);
    eventually("the hub to die mid-switch", Duration::from_secs(60), || {
        sup.starts().get(n - 1)?.code.filter(|c| *c == 86)
    });
    sup.running("v0.0.1", n, 60);
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
    let p = probation(&home);
    assert_eq!(p["version"].as_str(), Some("v0.0.2"), "{p}");
    let idle = cli(&home, &["hub", "probation"]);
    assert!(
        idle.stdout.contains("no release on trial"),
        "{}",
        idle.stdout
    );
    drop(sup);

    let sup = Supervisor::start(
        &home,
        "protectorate",
        words(&["hub", "probation"]),
        words(&["serve", "--listen", "off"]),
        vec![("PT_TEST_CRASH_AT".into(), "switch-previous".into())],
        Duration::from_millis(300),
    );
    eventually("hub before pointer switch", Duration::from_secs(60), || {
        hub_started_ms(&home)
    });
    let n = sup.starts().len();
    cli(&home, &["--json", "hub", "update", "--version", "v0.0.2"]);
    eventually(
        "hub to die after previous was synced",
        Duration::from_secs(60),
        || sup.starts().get(n - 1)?.code.filter(|c| *c == 86),
    );
    sup.running("v0.0.1", n, 60);
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
    assert_eq!(pointer(&home, "previous").as_deref(), Some("v0.0.1"));
    drop(sup);

    // On trial with its window passed, the prestart dies after switching current back: both
    // pointers still name v0.0.1, the release that worked.
    fs::create_dir_all(home.join("releases/v0.0.2")).unwrap();
    write_exec(
        &home.join("releases/v0.0.2/protectorate"),
        std::str::from_utf8(&wrap("v0.0.2")).unwrap(),
    );
    let link = |name: &str, tag: &str| {
        let tmp = home.join(format!("releases/.{name}-test"));
        std::os::unix::fs::symlink(tag, &tmp).unwrap();
        fs::rename(&tmp, home.join("releases").join(name)).unwrap();
    };
    link("previous", "v0.0.1");
    link("current", "v0.0.2");
    let trial = obj! {
        "version" => "v0.0.2",
        "previous" => "v0.0.1",
        "switched_ms" => 1u64,
        "deadline_ms" => 2u64,
    };
    fs::write(home.join("releases/probation.json"), trial.to_string()).unwrap();
    let out = Command::new(home.join("releases/previous/protectorate"))
        .args(words(&["hub", "probation"]))
        .env("PT_TEST_CRASH_AT", "rollback-current")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(86), "{out:?}");
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
    assert_eq!(pointer(&home, "previous").as_deref(), Some("v0.0.1"));
    let after = cli(&home, &["hub", "probation"]);
    assert_eq!(after.status, 0, "{}", after.stderr);
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
}

#[test]
fn prestart_restores_headscale_snapshot_before_rollback() {
    let root = dir("restore");
    let home = root.join("h");
    fs::create_dir_all(&home).unwrap();
    let real = PathBuf::from(bin());
    install(
        &home,
        "v0.0.2",
        "protectorate",
        &wrapper("protectorate", "version", "v0.0.2", &real, "", false),
    );
    fs::create_dir_all(home.join("releases/v0.0.1")).unwrap();
    write_exec(
        &home.join("releases/v0.0.1/protectorate"),
        std::str::from_utf8(&wrapper(
            "protectorate",
            "version",
            "v0.0.1",
            &real,
            "",
            false,
        ))
        .unwrap(),
    );
    std::os::unix::fs::symlink("v0.0.1", home.join("releases/previous")).unwrap();
    let fleet = root.join("fleet");
    let dbdir = fleet.join("headscale");
    let snap = fleet.join("snapshots/before-v0.0.2");
    fs::create_dir_all(&dbdir).unwrap();
    fs::create_dir_all(&snap).unwrap();
    let db = dbdir.join("db.sqlite");
    let sql = |query: &str| {
        Command::new("sqlite3")
            .arg(&db)
            .arg(query)
            .output()
            .unwrap()
    };
    assert!(
        sql("create table nodes (name text); insert into nodes values ('before');")
            .status
            .success()
    );
    fs::copy(&db, snap.join("db.sqlite")).unwrap();
    assert!(
        sql("insert into nodes values ('during-trial');")
            .status
            .success()
    );
    fs::write(dbdir.join("db.sqlite-wal"), "stale trial wal").unwrap();
    let trial = obj! { "version" => "v0.0.2", "previous" => "v0.0.1", "switched_ms" => 1u64, "deadline_ms" => 2u64, "headscale_snapshot" => snap.display().to_string() };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let missing = obj! { "version" => "v0.0.2", "previous" => "v0.0.1", "switched_ms" => now, "deadline_ms" => now + 60_000u64, "starts" => 3u64, "headscale_snapshot" => fleet.join("snapshots/missing").display().to_string() };
    fs::write(home.join("releases/probation.json"), missing.to_string()).unwrap();
    let failed = Command::new(home.join("releases/previous/protectorate"))
        .arg("--home")
        .arg(&home)
        .args(["hub", "probation"])
        .output()
        .unwrap();
    assert!(!failed.status.success(), "{failed:?}");
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.2"));
    assert!(probation(&home)["reason"].as_str().is_some());
    let start = Command::new(home.join("releases/current/protectorate"))
        .arg("--home")
        .arg(&home)
        .args(["serve", "--listen", "off"])
        .output()
        .unwrap();
    assert!(!start.status.success(), "{start:?}");
    assert!(
        String::from_utf8_lossy(&start.stderr).contains("needs rollback"),
        "{start:?}"
    );
    fs::write(home.join("releases/probation.json"), trial.to_string()).unwrap();
    let out = Command::new(home.join("releases/previous/protectorate"))
        .arg("--home")
        .arg(&home)
        .args(["hub", "probation"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("nodes enrolled after this snapshot are lost")
    );
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
    assert_eq!(pointer(&home, "previous").as_deref(), Some("v0.0.1"));
    let out = sql("select name from nodes order by name;");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n");
}

#[test]
fn implicit_hub_update_requires_higher_precedence() {
    let real = PathBuf::from(bin());
    let asset = format!("protectorate-linux-{}", arch());
    let fake = serve_releases(vec![release(
        "v0.6.0",
        vec![(
            asset,
            wrapper("protectorate", "version", "v0.6.0", &real, "", false),
        )],
    )]);
    let home = dir("downgrade");
    let config = obj! { "envoy" => obj! { "harness" => "claude" }, "claude_program" => "/nonexistent/program", "quota_poll_secs" => 0u64, "harness_update_hours" => 0u64, "release_api" => fake.url.as_str(), "release_repo" => "o/r" };
    fs::write(home.join("config.json"), config.to_string()).unwrap();
    install(
        &home,
        "v0.7.0-dev.3",
        "protectorate",
        &wrapper("protectorate", "version", "v0.7.0-dev.3", &real, "", false),
    );
    let state = home.display().to_string();
    let sup = Supervisor::start(
        &home,
        "protectorate",
        vec![
            "--home".into(),
            state.clone(),
            "hub".into(),
            "probation".into(),
        ],
        vec![
            "--home".into(),
            state,
            "serve".into(),
            "--listen".into(),
            "off".into(),
        ],
        Vec::new(),
        Duration::from_millis(200),
    );
    eventually("hub ready", Duration::from_secs(30), || {
        hub_started_ms(&home)
    });
    let check = cli(&home, &["--json", "hub", "update", "--check"]).json();
    assert_eq!(check["available"].as_bool(), Some(false), "{check}");
    let implicit = cli(&home, &["--json", "hub", "update"]).json();
    assert_eq!(implicit["restarting"].as_bool(), Some(false), "{implicit}");
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.7.0-dev.3"));
    let explicit = cli(&home, &["--json", "hub", "update", "--version", "v0.6.0"]).json();
    assert!(
        explicit["note"]
            .as_str()
            .unwrap_or("")
            .contains("downgrade"),
        "{explicit}"
    );
    sup.running("v0.6.0", 1, 30);
    confirmed_after(&home, 0, "explicit downgrade confirmed");
    drop(sup);
}

#[test]
fn binary_check_times_out_when_descendant_keeps_stdout() {
    let real = PathBuf::from(bin());
    let home = dir("check-hang");
    let pidfile = home.join("child.pid");
    let bad = format!(
        "#!/bin/bash\nif [ \"$1\" = version ]; then sleep 30 & echo $! > '{}'; echo 'protectorate v0.0.2'; exit 0; fi\nexit 1\n",
        pidfile.display()
    );
    let asset = format!("protectorate-linux-{}", arch());
    let fake = serve_releases(vec![release("v0.0.2", vec![(asset, bad.into_bytes())])]);
    let config = obj! { "envoy" => obj! { "harness" => "claude" }, "claude_program" => "/nonexistent/program", "quota_poll_secs" => 0u64, "harness_update_hours" => 0u64, "release_api" => fake.url.as_str(), "release_repo" => "o/r" };
    fs::write(home.join("config.json"), config.to_string()).unwrap();
    install(
        &home,
        "v0.0.1",
        "protectorate",
        &wrapper("protectorate", "version", "v0.0.1", &real, "", false),
    );
    let state = home.display().to_string();
    let sup = Supervisor::start(
        &home,
        "protectorate",
        vec![
            "--home".into(),
            state.clone(),
            "hub".into(),
            "probation".into(),
        ],
        vec![
            "--home".into(),
            state,
            "serve".into(),
            "--listen".into(),
            "off".into(),
        ],
        vec![("PT_TEST_CHECK_TIMEOUT_MS".into(), "1000".into())],
        Duration::from_millis(200),
    );
    eventually("hub ready", Duration::from_secs(30), || {
        hub_started_ms(&home)
    });
    let start = std::time::Instant::now();
    let out = cli(&home, &["hub", "update", "--version", "v0.0.2"]);
    assert_ne!(out.status, 0, "{}", out.stdout);
    assert!(
        out.stderr.contains("did not finish"),
        "{} {}",
        out.stdout,
        out.stderr
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    assert_eq!(pointer(&home, "current").as_deref(), Some("v0.0.1"));
    let pid = fs::read_to_string(pidfile).unwrap();
    eventually("check descendant killed", Duration::from_secs(5), || {
        let status = fs::read_to_string(format!("/proc/{}/status", pid.trim())).unwrap_or_default();
        (status.is_empty()
            || status
                .lines()
                .any(|line| line.starts_with("State:") && line.contains("Z")))
        .then_some(())
    });
    drop(sup);
}

fn node_bin() -> Option<PathBuf> {
    let p = Path::new(bin()).with_file_name("protectorate-node");
    if p.is_file() {
        return Some(p);
    }
    eprintln!(
        "SKIP: {} is missing; run cargo test --workspace",
        p.display()
    );
    None
}

fn node_rpc(node: &Path, sock: &Path, method: &str, fields: &str) -> Run {
    let out = Command::new(node)
        .arg("rpc")
        .arg("--connect")
        .arg(sock)
        .args([method, fields])
        .output()
        .unwrap();
    Run {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn node_update_admission_lost_reply_and_confirmation() {
    use pt_frame::{Kind, wire::write_frame};
    use std::net::Shutdown;
    use std::os::unix::net::UnixStream;

    let node = node_bin().expect("cargo build --workspace must build protectorate-node");
    let asset = format!("protectorate-node-linux-{}", arch());
    let rel = |tag: &str| {
        release(
            tag,
            vec![(
                asset.clone(),
                wrapper("protectorate-node", "--version", tag, &node, "", false),
            )],
        )
    };
    let fake = serve_releases(vec![rel("v0.0.2"), rel("v0.0.3")]);
    let root = dir("node-direct");
    install(
        &root,
        "v0.0.1",
        "protectorate-node",
        &wrapper("protectorate-node", "--version", "v0.0.1", &node, "", false),
    );
    let pause = root.join("pause");
    fs::write(&pause, "0").unwrap();
    fs::write(root.join(".curlrc"), "this-option-does-not-exist\n").unwrap();
    let sock = root.join("a.sock");
    let state = root.display().to_string();
    let sup = Supervisor::start(
        &root,
        "protectorate-node",
        vec!["probation".into(), "--state-dir".into(), state.clone()],
        vec![
            "serve".into(),
            "--state-dir".into(),
            state.clone(),
            "--forward-socket".into(),
            sock.display().to_string(),
            "--no-scope".into(),
            "--no-member".into(),
        ],
        vec![
            (
                "PROTECTORATE_NODE_TEST_UPDATE_PAUSE".into(),
                pause.display().to_string(),
            ),
            ("CURL_HOME".into(), state.clone()),
        ],
        Duration::from_millis(200),
    );
    eventually("node socket", Duration::from_secs(30), || {
        std::os::unix::net::UnixStream::connect(&sock).ok()
    });

    for api in [
        "http://127.0.0.1:80@example.invalid",
        "http://localhost@example.invalid",
        "http://[::1]evil",
        "https://user@example.invalid",
    ] {
        let fields = obj! { "version" => "v0.0.2", "api" => api, "repo" => "o/r", "probation_secs" => 30u64 }.to_string();
        let out = node_rpc(&node, &sock, "Update", &fields);
        assert_eq!(out.status, 1, "{api}: {} {}", out.stdout, out.stderr);
        assert!(
            out.stdout.contains("authority") || out.stdout.contains("loopback"),
            "{api}: {}",
            out.stdout
        );
    }

    fs::write(&pause, "1").unwrap();
    let fields = obj! { "type" => "Update", "version" => "v0.0.2", "api" => fake.url.as_str(), "repo" => "o/r", "probation_secs" => 30u64 }.to_string();
    let mut link = UnixStream::connect(&sock).unwrap();
    link.write_all(b"{\"tags\":[\"tag:hub\"]}\n").unwrap();
    write_frame(&mut link, Kind::Request, 1, fields.as_bytes()).unwrap();
    link.shutdown(Shutdown::Both).unwrap();
    drop(link);
    eventually("update admitted", Duration::from_secs(10), || {
        sup.log_text()
            .contains("update admitted for testing")
            .then_some(())
    });
    let submit = node_rpc(
        &node,
        &sock,
        "SubmitJob",
        r#"{"job_id":"during-update","attempt":1,"script":"echo unexpected"}"#,
    );
    assert_eq!(submit.status, 1, "{} {}", submit.stdout, submit.stderr);
    assert!(submit.stdout.contains("busy"), "{}", submit.stdout);
    fs::write(&pause, "0").unwrap();
    sup.running("v0.0.2", 1, 45);
    assert_eq!(
        sup.starts()[0].code,
        Some(75),
        "{:?}\n{}",
        sup.starts(),
        sup.log_text()
    );
    eventually("updated node socket", Duration::from_secs(10), || {
        let r = node_rpc(&node, &sock, "Hello", "{}");
        (r.status == 0).then_some(())
    });
    assert!(probation(&root)["contacted_ms"].is_null());
    let mut lost = UnixStream::connect(&sock).unwrap();
    lost.write_all(b"{\"tags\":[\"tag:hub\"]}\n").unwrap();
    write_frame(&mut lost, Kind::Request, 22, br#"{"type":"Presence"}"#).unwrap();
    lost.shutdown(Shutdown::Both).unwrap();
    drop(lost);
    eventually("failed non-Hello reply", Duration::from_secs(10), || {
        sup.log_text().contains("request 22").then_some(())
    });
    assert!(probation(&root)["contacted_ms"].is_null());
    let bad = node_rpc(&node, &sock, "JobStatus", r#"{"job_id":"missing"}"#);
    assert_eq!(bad.status, 0, "{}", bad.stdout);
    // A valid response, including an empty job status, is enough to confirm.
    eventually(
        "node confirmed after a non-Hello reply",
        Duration::from_secs(10),
        || probation(&root)["contacted_ms"].as_u64(),
    );
    drop(sup);
}

#[test]
fn node_trial_refuses_overlap_retries_confirmation_and_rejects_expiry() {
    let node = node_bin().expect("cargo build --workspace must build protectorate-node");
    let asset = format!("protectorate-node-linux-{}", arch());
    let rel = |tag: &str| {
        release(
            tag,
            vec![(
                asset.clone(),
                wrapper("protectorate-node", "--version", tag, &node, "", false),
            )],
        )
    };
    let fake = serve_releases(vec![rel("v0.0.3")]);
    let root = dir("node-trial");
    install(
        &root,
        "v0.0.2",
        "protectorate-node",
        &wrapper("protectorate-node", "--version", "v0.0.2", &node, "", false),
    );
    fs::create_dir_all(root.join("releases/v0.0.1")).unwrap();
    write_exec(
        &root.join("releases/v0.0.1/protectorate-node"),
        std::str::from_utf8(&wrapper(
            "protectorate-node",
            "--version",
            "v0.0.1",
            &node,
            "",
            false,
        ))
        .unwrap(),
    );
    std::os::unix::fs::symlink("v0.0.1", root.join("releases/previous")).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let trial = obj! { "version" => "v0.0.2", "previous" => "v0.0.1", "switched_ms" => now, "deadline_ms" => now + 60_000u64 };
    fs::write(root.join("releases/probation.json"), trial.to_string()).unwrap();
    let sock = root.join("b.sock");
    let state = root.display().to_string();
    let sup = Supervisor::start(
        &root,
        "protectorate-node",
        vec!["probation".into(), "--state-dir".into(), state.clone()],
        vec![
            "serve".into(),
            "--state-dir".into(),
            state.clone(),
            "--forward-socket".into(),
            sock.display().to_string(),
            "--no-scope".into(),
            "--no-member".into(),
        ],
        vec![("PROTECTORATE_NODE_TEST_PAUSE_WATCH".into(), "1".into())],
        Duration::from_millis(200),
    );
    eventually("trial node socket", Duration::from_secs(20), || {
        UnixStream::connect(&sock).ok()
    });
    assert_eq!(node_rpc(&node, &sock, "Hello", "{}").status, 0);
    assert!(probation(&root)["contacted_ms"].is_null());
    let fields = obj! { "version" => "v0.0.3", "api" => fake.url.as_str(), "repo" => "o/r", "probation_secs" => 30u64 }.to_string();
    let overlap = node_rpc(&node, &sock, "Update", &fields);
    assert_eq!(overlap.status, 1, "{} {}", overlap.stdout, overlap.stderr);
    assert!(overlap.stdout.contains("probation"), "{}", overlap.stdout);
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.2"));
    assert_eq!(pointer(&root, "previous").as_deref(), Some("v0.0.1"));
    assert!(probation(&root)["contacted_ms"].is_null());

    fs::set_permissions(root.join("releases"), fs::Permissions::from_mode(0o555)).unwrap();
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually(
        "failed confirmation logged",
        Duration::from_secs(10),
        || {
            sup.log_text()
                .contains("confirming the update")
                .then_some(())
        },
    );
    fs::set_permissions(root.join("releases"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(probation(&root)["contacted_ms"].is_null());
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually("confirmation retried", Duration::from_secs(10), || {
        probation(&root)["contacted_ms"].as_u64()
    });
    drop(sup);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let expired = obj! { "version" => "v0.0.2", "previous" => "v0.0.1", "switched_ms" => now, "deadline_ms" => now + 1500u64 };
    fs::write(root.join("releases/probation.json"), expired.to_string()).unwrap();
    let sup = Supervisor::start(
        &root,
        "protectorate-node",
        vec!["probation".into(), "--state-dir".into(), state.clone()],
        vec![
            "serve".into(),
            "--state-dir".into(),
            state.clone(),
            "--forward-socket".into(),
            sock.display().to_string(),
            "--no-scope".into(),
            "--no-member".into(),
        ],
        vec![("PROTECTORATE_NODE_TEST_PAUSE_WATCH".into(), "1".into())],
        Duration::from_millis(200),
    );
    eventually("second trial node", Duration::from_secs(10), || {
        (node_rpc(&node, &sock, "Hello", "{}").status == 0).then_some(())
    });
    thread::sleep(Duration::from_millis(1800));
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    assert!(
        probation(&root)["contacted_ms"].is_null(),
        "{}",
        probation(&root)
    );
    let out = Command::new(root.join("releases/previous/protectorate-node"))
        .args(["probation", "--state-dir", &state])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.1"));
    drop(sup);
}

/// A TCP port whose next port is free too: headscale binds STUN there.
fn free_pair() -> u16 {
    loop {
        let p = free_port();
        if p < u16::MAX && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
            return p;
        }
    }
}

#[test]
fn node_update_is_confirmed_by_the_hub_and_rolled_back_without_it() {
    let version = Command::new(bin()).arg("version").output().unwrap();
    if String::from_utf8_lossy(&version.stdout).contains("missing") {
        eprintln!("SKIP: this protectorate does not embed headscale and member");
        return;
    }
    let Some(node_bin) = node_bin() else { return };
    let asset = format!("protectorate-node-linux-{}", arch());
    let node_rel = |tag: &str| {
        release(
            tag,
            vec![(
                asset.clone(),
                wrapper("protectorate-node", "--version", tag, &node_bin, "", false),
            )],
        )
    };
    let fake = serve_releases(vec![node_rel("v0.0.2"), node_rel("v0.0.3")]);
    let port = free_pair();
    let listen = format!("127.0.0.1:{port}");
    let tailnet = std::env::temp_dir().join(format!("pt-nodeupd-{}-{port}", std::process::id()));
    let mut hub = Hub::start_config(
        "nodeupd",
        "pi_local",
        &[
            (
                "fleet",
                obj! {
                    "listen" => listen.as_str(),
                    "dir" => tailnet.to_str().unwrap(),
                    "poll_secs" => 2u64,
                },
            ),
            ("release_api", fake.url.as_str().into()),
            ("release_repo", "o/r".into()),
        ],
        &[],
        &[],
    );
    eventually("the fleet to come up", Duration::from_secs(120), || {
        (hub.cli(&["--json", "node", "add"]).status == 0).then_some(())
    });

    let node_dir = hub.home.with_file_name("node");
    fs::create_dir_all(&node_dir).unwrap();
    let mut join = Command::new(&node_bin)
        .args([
            "join",
            &format!("http://{listen}"),
            "--name",
            "e2e-upd",
            "--state-dir",
        ])
        .arg(&node_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let out = join.stdout.take().unwrap();
    thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let payload = loop {
        let l = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("join shows its registration");
        if let Some(p) = l.trim_start().strip_prefix("paste") {
            break p.trim().to_string();
        }
    };
    let pending = hub.cli(&["--json", "node", "add", &payload]).json();
    let auth = pending["auth_id"].as_str().unwrap().to_string();
    hub.cli(&["--json", "node", "approve", &auth]).json();
    let joined = eventually("join to exit", Duration::from_secs(60), || {
        join.try_wait().unwrap()
    });
    assert!(joined.success());

    install(
        &node_dir,
        "v0.0.1",
        "protectorate-node",
        &wrapper(
            "protectorate-node",
            "--version",
            "v0.0.1",
            &node_bin,
            "",
            false,
        ),
    );
    let presence = node_dir.join("presence");
    fs::write(&presence, "AWAY").unwrap();
    let state = node_dir.display().to_string();
    let sup = Supervisor::start(
        &node_dir,
        "protectorate-node",
        vec!["probation".into(), "--state-dir".into(), state.clone()],
        vec![
            "serve".into(),
            "--state-dir".into(),
            state.clone(),
            "--forward-socket".into(),
            format!("{state}/agent.sock"),
            "--no-scope".into(),
        ],
        vec![(
            "PROTECTORATE_NODE_TEST_PRESENCE".into(),
            presence.display().to_string(),
        )],
        Duration::from_secs(2),
    );
    eventually("the node online", Duration::from_secs(120), || {
        let v = hub.cli(&["--json", "node", "ls"]).json();
        v["nodes"]
            .as_array()?
            .iter()
            .any(|n| n["name"].as_str() == Some("e2e-upd") && n["online"].as_bool() == Some(true))
            .then_some(())
    });

    // The hub has the node fetch v0.0.2 from the fake GitHub, then waits for it to answer
    // Hello and Presence from it, which is the contact that keeps it.
    let n = sup.starts().len();
    let r = hub.cli(&[
        "--json",
        "node",
        "update",
        "e2e-upd",
        "--version",
        "v0.0.2",
        "--probation",
        "90",
    ]);
    assert_eq!(
        r.status,
        0,
        "{}{}\n--- node ---\n{}\n--- hub ---\n{}",
        r.stdout,
        r.stderr,
        tail(&sup.log_text(), 40),
        tail(&hub.log_text(), 40)
    );
    let u = r.json();
    let row = &u["nodes"][0];
    assert_eq!(row["from"].as_str(), Some("v0.0.1"), "{u}");
    assert_eq!(row["to"].as_str(), Some("v0.0.2"), "{u}");
    assert!(
        row["detail"]
            .as_str()
            .unwrap_or("")
            .contains("answered Presence"),
        "{u}"
    );
    sup.running("v0.0.2", n, 30);
    assert_eq!(sup.starts()[n - 1].code, Some(75), "{:?}", sup.starts());
    eventually(
        "the node to confirm v0.0.2",
        Duration::from_secs(30),
        || probation(&node_dir)["contacted_ms"].as_u64(),
    );
    assert!(
        node_dir.join("releases/v0.0.2/SHA256SUMS").is_file(),
        "staged with its sums"
    );

    // Switched, then the hub goes silent: the node switches back when its window passes.
    let n = sup.starts().len();
    let r = hub.cli(&[
        "--json",
        "node",
        "update",
        "e2e-upd",
        "--version",
        "v0.0.3",
        "--probation",
        "8",
        "--no-wait",
    ]);
    hub.kill9();
    assert_eq!(r.status, 0, "{}{}", r.stdout, r.stderr);
    sup.running("v0.0.3", n, 30);
    sup.running("v0.0.2", n + 1, 60);
    let p = probation(&node_dir);
    assert_eq!(p["version"].as_str(), Some("v0.0.3"), "{p}");
    assert!(p["contacted_ms"].is_null(), "{p}");
    assert!(
        p["reason"].as_str().unwrap_or("").contains("not confirmed"),
        "{p}"
    );
    let v3: Vec<_> = sup.starts()[n..]
        .iter()
        .filter(|s| s.release == "v0.0.3")
        .cloned()
        .collect();
    assert_eq!(v3.len(), 1, "{:?}", sup.starts());
    assert_eq!(v3[0].code, Some(75));
    assert_eq!(pointer(&node_dir, "current").as_deref(), Some("v0.0.2"));
    drop(sup);
    // Nothing of the hub's fleet may outlive the test: restart and stop it cleanly.
    hub.restart();
    hub.stop();
}

/// The fake claude: `--version` fails while `<dir>/health` says broken; anything else exits 1.
fn fake_claude(dir: &Path) -> PathBuf {
    let p = dir.join("claude");
    write_exec(
        &p,
        &format!(
            "#!/bin/bash\nif [ \"$1\" = --version ]; then\n  if grep -q broken '{0}/health'; then echo 'claude: broken install' >&2; exit 3; fi\n  echo '2.0.0 (Claude Code)'; exit 0\nfi\necho 'fake claude does not run' >&2\nexit 1\n",
            dir.display()
        ),
    );
    p
}

#[test]
fn doctor_refuses_an_ineligible_harness_and_envoy_waits_for_it() {
    for tool in ["bwrap", "systemd-run", "git"] {
        if !common::on_path(tool) {
            eprintln!("SKIP: {tool} is not installed");
            return;
        }
    }
    let fakes = dir("doctor");
    fs::write(fakes.join("health"), "broken\n").unwrap();
    let claude = fake_claude(&fakes);
    // The owner's own claude login does not count: only the hub's, which comes later.
    let owner = fakes.join("owner");
    fs::create_dir_all(owner.join(".claude")).unwrap();
    fs::write(owner.join(".claude/.credentials.json"), "{}").unwrap();
    let hub = Hub::start_config(
        "doctor",
        "claude",
        &[("claude_program", claude.to_str().unwrap().into())],
        &[],
        &[("HOME", owner.to_str().unwrap())],
    );
    eventually("Envoy to park", Duration::from_secs(90), || {
        (hub.sleeve("envoy")["lifecycle"].as_str() == Some("parked")).then_some(())
    });
    let ev = events(&hub.home);
    assert!(
        ev.iter().any(
            |e| e.contains("launch refused: harness claude is ineligible")
                && e.contains("broken install")
        ),
        "{ev:?}"
    );

    let d = hub.cli(&["--json", "doctor"]).json();
    let h = d["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["name"].as_str() == Some("claude"))
        .cloned()
        .unwrap();
    assert_eq!(h["eligible"].as_bool(), Some(false), "{d}");
    assert!(
        h["reasons"][0].as_str().unwrap_or("").contains("--version"),
        "{d}"
    );
    assert!(
        h["reasons"][1]
            .as_str()
            .unwrap_or("")
            .contains("run `protectorate harness login claude`"),
        "{d}"
    );
    assert!(fs::metadata(hub.home.join("doctor.json")).is_ok());
    let human = hub.cli(&["doctor"]);
    assert!(human.ok().contains("claude"), "{}", human.stdout);

    let repo = temp_repo("doctor");
    let spawn = hub.cli(&[
        "sleeve",
        "spawn",
        "nope",
        "--path",
        repo.to_str().unwrap(),
        "--harness",
        "claude",
    ]);
    assert_ne!(spawn.status, 0);
    assert!(
        spawn.stderr.contains("409") && spawn.stderr.contains("harness claude is ineligible"),
        "{}",
        spawn.stderr
    );
    let start = hub.cli(&["sleeve", "start", "envoy"]);
    assert_ne!(start.status, 0);
    assert!(start.stderr.contains("ineligible"), "{}", start.stderr);

    // Fixed, and logged in for the hub: the next doctor run finds claude eligible and starts
    // Envoy again.
    fs::write(fakes.join("health"), "fixed\n").unwrap();
    fs::create_dir_all(hub.home.join("logins/.claude")).unwrap();
    fs::write(hub.home.join("logins/.claude/.credentials.json"), "{}").unwrap();
    let d = hub.cli(&["--json", "doctor", "--refresh"]).json();
    assert!(
        d["harnesses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["name"].as_str() == Some("claude") && h["eligible"].as_bool() == Some(true)),
        "{d}"
    );
    eventually("Envoy to start again", Duration::from_secs(30), || {
        events(&hub.home)
            .iter()
            .any(|e| e.contains("claude is eligible again; starting Envoy"))
            .then_some(())
    });
}

/// A fake harness program: `--version` prints `<dir>/<name>.version`; the update command
/// records itself in `<dir>/calls` and bumps the version, or fails when `fails`. Anything else
/// runs `real` when given.
fn fake_program(dir: &Path, name: &str, fails: bool, real: Option<&Path>) -> PathBuf {
    let p = dir.join(name);
    let version = dir.join(format!("{name}.version"));
    fs::write(&version, "1.0.0\n").unwrap();
    let update = if fails {
        "echo 'update server unreachable' >&2; exit 1".to_string()
    } else {
        format!(
            "echo 'updated to 2.0.0'; echo 2.0.0 > '{}'; exit 0",
            version.display()
        )
    };
    let rest = match real {
        Some(r) => format!("exec '{}' \"$@\"", r.display()),
        None => "exit 1".into(),
    };
    write_exec(
        &p,
        &format!(
            "#!/bin/bash\ncase \"$1\" in\n  --version) cat '{v}'; exit 0;;\n  update) echo \"{name} $*\" >> '{calls}'; {update};;\nesac\n{rest}\n",
            v = version.display(),
            calls = dir.join("calls").display(),
        ),
    );
    p
}

fn calls(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn harness_updates_run_daily_and_wait_for_a_sleeve_mid_turn() {
    if !common::have_runtime() {
        return;
    }
    let real_pi = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("pi"))
        .find(|p| p.is_file())
        .unwrap();
    let fakes = dir("harness");
    let claude = fake_program(&fakes, "claude", false, None);
    let codex = fake_program(&fakes, "codex", false, None);
    let grok = fake_program(&fakes, "grok", true, None);
    let pi = fake_program(&fakes, "pi", false, Some(&real_pi));
    let mut hub = Hub::start_config(
        "harnessupd",
        "pi_local",
        &[
            ("harness_update_hours", 24u64.into()),
            ("claude_program", claude.to_str().unwrap().into()),
            ("codex_program", codex.to_str().unwrap().into()),
            ("grok_program", grok.to_str().unwrap().into()),
            ("pi_program", pi.to_str().unwrap().into()),
        ],
        &[],
        &[],
    );
    let want = [
        "claude update",
        "codex update",
        "grok update",
        "pi update self",
    ];
    eventually(
        "every program's update to run",
        Duration::from_secs(120),
        || {
            let c = calls(&fakes);
            want.iter().all(|w| c.iter().any(|l| l == w)).then_some(())
        },
    );
    let ev = eventually("the update events", Duration::from_secs(30), || {
        let ev = events(&hub.home);
        ev.iter().any(|e| e.starts_with("pi: ")).then_some(ev)
    });
    assert!(
        ev.iter()
            .any(|e| e == "claude: 1.0.0 -> 2.0.0 (claude update)"),
        "{ev:?}"
    );
    assert!(
        ev.iter()
            .any(|e| e.starts_with("grok: grok update failed") && e.contains("now 1.0.0")),
        "{ev:?}"
    );
    assert!(
        hub.log_text()
            .contains("harness update claude: updated to 2.0.0"),
        "the command's output is in the hub log"
    );
    let d = hub.cli(&["--json", "doctor", "--refresh"]).json();
    let claude_row = d["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["name"].as_str() == Some("claude"))
        .cloned()
        .unwrap();
    assert_eq!(claude_row["version"].as_str(), Some("2.0.0"), "{d}");
    assert!(claude_row["updated_at_ms"].as_u64().is_some(), "{d}");

    // The last runs are kept: a restart within the day runs nothing again.
    let before = calls(&fakes).len();
    hub.restart();
    thread::sleep(Duration::from_secs(5));
    assert_eq!(calls(&fakes).len(), before, "{:?}", calls(&fakes));

    // Asked for while a pi sleeve is mid-turn, pi's update waits for the turn to end.
    let repo = temp_repo("harnessupd");
    hub.spawn_pi("busy", &repo);
    hub.send("busy", LONG);
    eventually("the long turn to start", Duration::from_secs(60), || {
        let s = hub.sleeve("busy");
        (s["incarnation"]["activity"]["state"].as_str() == Some("working")).then_some(())
    });
    let r = hub
        .cli(&["--json", "harness", "update", "pi", "claude", "missing"])
        .json();
    let state = |p: &str| {
        r["programs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["program"].as_str() == Some(p))
            .and_then(|x| x["state"].as_str())
            .map(String::from)
    };
    assert_eq!(state("pi").as_deref(), Some("deferred"), "{r}");
    assert_eq!(state("claude").as_deref(), Some("started"), "{r}");
    assert_eq!(state("missing").as_deref(), Some("skipped"), "{r}");
    let pi_runs = || {
        calls(&fakes)
            .iter()
            .filter(|l| *l == "pi update self")
            .count()
    };
    eventually("claude's update", Duration::from_secs(30), || {
        (calls(&fakes)
            .iter()
            .filter(|l| *l == "claude update")
            .count()
            == 2)
            .then_some(())
    });
    let busy_now = hub.sleeve("busy")["incarnation"]["activity"]["state"]
        .as_str()
        .map(String::from);
    if busy_now.as_deref() == Some("working") {
        assert_eq!(pi_runs(), 1, "pi waits while the sleeve works");
    }
    hub.wait("busy", "idle", 300);
    eventually(
        "pi's update after the turn",
        Duration::from_secs(30),
        || (pi_runs() == 2).then_some(()),
    );
}

/// `protectorate-node supervise`, the loop the Windows service runs in place of systemd, with
/// release pointers switched the Windows way (`PT_TEST_POINTER_ASIDE`: the old link renamed
/// aside, then the new one into place).
struct Supervise {
    child: Child,
    log: PathBuf,
}

impl Supervise {
    fn start(node: &Path, root: &Path, sock: &Path, env: &[(&str, &str)]) -> Supervise {
        use std::os::unix::process::CommandExt;
        let log = root.join("supervise.log");
        let err = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .unwrap();
        let mut cmd = Command::new(node);
        cmd.arg("supervise")
            .arg("--state-dir")
            .arg(root)
            .arg("--forward-socket")
            .arg(sock)
            .args(["--no-scope", "--no-member"])
            .stdin(Stdio::null())
            .stdout(err.try_clone().unwrap())
            .stderr(err)
            .process_group(0)
            .env("PT_TEST_POINTER_ASIDE", "1")
            .env("CURL_HOME", root)
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_SOCKET");
        for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("CLAUDE")) {
            cmd.env_remove(k);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        Supervise {
            child: cmd.spawn().unwrap(),
            log,
        }
    }

    fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn wait_log(&self, what: &str, needle: &str, secs: u64) {
        let ok = (0..secs * 10).any(|_| {
            let hit = self.log_text().contains(needle);
            if !hit {
                thread::sleep(Duration::from_millis(100));
            }
            hit
        });
        assert!(
            ok,
            "{what}: no {needle:?}\n--- supervise log ---\n{}",
            tail(&self.log_text(), 60)
        );
    }

    /// SIGTERM, as a service stop; true when it exited 0 in time.
    fn stop(&mut self) -> bool {
        let _ = pt_sys::kill_group(self.child.id(), pt_sys::SIGTERM);
        for _ in 0..150 {
            if let Ok(Some(st)) = self.child.try_wait() {
                return st.success();
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Supervise {
    fn drop(&mut self) {
        if !self.stop() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn no_pointer_leftovers(root: &Path) {
    let names: Vec<String> = fs::read_dir(root.join("releases"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with("-new") || n.ends_with("-old"))
        .collect();
    assert!(names.is_empty(), "left over: {names:?}");
}

#[test]
fn supervised_node_updates_rolls_back_a_crashing_release_and_survives_a_torn_switch() {
    // Also: a watch that meets a half-done switch, a hung probation of the previous release,
    // and a stop while a probation runs.
    let node = node_bin().expect("cargo build --workspace must build protectorate-node");
    let asset = format!("protectorate-node-linux-{}", arch());
    let rel = |tag: &str, crash: bool| {
        release(
            tag,
            vec![(
                asset.clone(),
                wrapper("protectorate-node", "--version", tag, &node, "", crash),
            )],
        )
    };
    let root = dir("node-supervise");
    // v0.0.5's `probation` hangs, under a name this run can look for.
    let hung = format!("pt-hung-{}", root.file_name().unwrap().to_string_lossy());
    let hanging = {
        let w = String::from_utf8(wrapper(
            "protectorate-node",
            "--version",
            "v0.0.5",
            &node,
            "",
            false,
        ))
        .unwrap();
        let (bang, rest) = w.split_once('\n').unwrap();
        format!("{bang}\n[ \"$1\" = probation ] && exec -a {hung} sleep 300\n{rest}").into_bytes()
    };
    let fake = serve_releases(vec![
        rel("v0.0.2", false),
        rel("v0.0.3", true),
        rel("v0.0.4", false),
        release("v0.0.5", vec![(asset.clone(), hanging)]),
        rel("v0.0.6", false),
    ]);
    let hung_alive = || {
        Command::new("pgrep")
            .args(["-f", &hung])
            .output()
            .unwrap()
            .status
            .success()
    };
    install(
        &root,
        "v0.0.1",
        "protectorate-node",
        &wrapper("protectorate-node", "--version", "v0.0.1", &node, "", false),
    );
    // install-service names the installed release previous too.
    std::os::unix::fs::symlink("v0.0.1", root.join("releases/previous")).unwrap();
    let sock = root.join("s.sock");
    let update = |tag: &str| {
        let fields = obj! { "version" => tag, "api" => fake.url.as_str(), "repo" => "o/r", "probation_secs" => 60u64 }.to_string();
        node_rpc(&node, &sock, "Update", &fields)
    };
    let hello = || (node_rpc(&node, &sock, "Hello", "{}").status == 0).then_some(());

    let mut sup = Supervise::start(&node, &root, &sock, &[]);
    sup.wait_log("first start", "start 1, release v0.0.1", 20);
    eventually("node v0.0.1", Duration::from_secs(20), hello);

    // An update: the node switches and exits 75; the supervisor runs the previous release's
    // probation and starts the new one, which a non-Hello request confirms.
    let out = update("v0.0.2");
    assert_eq!(out.status, 0, "{} {}", out.stdout, out.stderr);
    sup.wait_log("restart after the update", "start 2, release v0.0.2", 20);
    assert!(
        sup.log_text().contains("the node exited 75"),
        "{}",
        sup.log_text()
    );
    eventually("node v0.0.2", Duration::from_secs(20), hello);
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually("v0.0.2 confirmed", Duration::from_secs(10), || {
        probation(&root)["contacted_ms"].as_u64()
    });
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.2"));
    assert_eq!(pointer(&root, "previous").as_deref(), Some("v0.0.1"));
    no_pointer_leftovers(&root);

    // A release whose serve crashes at once: after three starts the probation the supervisor
    // runs from previous switches back, and the supervisor starts v0.0.2 again.
    let out = update("v0.0.3");
    assert_eq!(out.status, 0, "{} {}", out.stdout, out.stderr);
    sup.wait_log("v0.0.3 tried", "start 3, release v0.0.3", 20);
    sup.wait_log("rolled back", "rolled back to v0.0.2", 60);
    sup.wait_log("v0.0.2 again", "start 6, release v0.0.2", 20);
    eventually("node v0.0.2 again", Duration::from_secs(20), hello);
    let p = probation(&root);
    assert_eq!(p["version"].as_str(), Some("v0.0.3"), "{p}");
    assert!(p["rolled_back_ms"].as_u64().is_some(), "{p}");
    assert!(p["reason"].as_str().unwrap_or("").contains("starts"), "{p}");
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.2"));
    no_pointer_leftovers(&root);

    // Stopping the supervisor ends the node.
    assert!(sup.stop(), "{}", sup.log_text());
    eventually("node stopped", Duration::from_secs(10), || {
        UnixStream::connect(&sock).is_err().then_some(())
    });
    drop(sup);

    // An update torn between renaming the old `current` aside and the new one into place (the
    // node dies there, as a power loss would): there is no `current` until the supervisor's
    // next start finishes the switch, and the release on trial then starts and is confirmed.
    let mut sup = Supervise::start(
        &node,
        &root,
        &sock,
        &[("PT_TEST_CRASH_AT", "current-aside")],
    );
    sup.wait_log("restart", "start 1, release v0.0.2", 20);
    eventually(
        "node v0.0.2 under the crash knob",
        Duration::from_secs(20),
        hello,
    );
    let out = update("v0.0.4");
    assert_ne!(
        out.status, 0,
        "the reply dies with the node: {}",
        out.stdout
    );
    sup.wait_log("torn switch", "the node exited 86", 20);
    sup.wait_log("switch finished", "finished an interrupted switch", 20);
    sup.wait_log("v0.0.4 started", "start 2, release v0.0.4", 20);
    eventually("node v0.0.4", Duration::from_secs(20), hello);
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually("v0.0.4 confirmed", Duration::from_secs(10), || {
        let p = probation(&root);
        (p["version"].as_str() == Some("v0.0.4"))
            .then(|| p["contacted_ms"].as_u64())
            .flatten()
    });
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.4"));
    assert_eq!(pointer(&root, "previous").as_deref(), Some("v0.0.2"));
    no_pointer_leftovers(&root);
    assert!(sup.stop(), "{}", sup.log_text());
    drop(sup);

    let mut sup = Supervise::start(
        &node,
        &root,
        &sock,
        &[("PT_TEST_PROBATION_TIMEOUT_MS", "2000")],
    );
    sup.wait_log("restart", "start 1, release v0.0.4", 20);
    eventually("node v0.0.4 again", Duration::from_secs(20), hello);

    // A switch found half done while a release is on trial (no `current`, `.current-new` naming
    // the trial): the trial's own watch finishes it instead of reading the trial as over, so
    // hub contact still confirms the release.
    let out = update("v0.0.5");
    assert_eq!(out.status, 0, "{} {}", out.stdout, out.stderr);
    sup.wait_log("v0.0.5 started", "start 2, release v0.0.5", 20);
    eventually("node v0.0.5", Duration::from_secs(20), hello);
    let finished = sup
        .log_text()
        .matches("finished an interrupted switch")
        .count();
    fs::rename(
        root.join("releases/current"),
        root.join("releases/.current-new"),
    )
    .unwrap();
    eventually("watch finished the switch", Duration::from_secs(10), || {
        (sup.log_text()
            .matches("finished an interrupted switch")
            .count()
            > finished)
            .then_some(())
    });
    assert_eq!(pointer(&root, "current").as_deref(), Some("v0.0.5"));
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually("v0.0.5 confirmed", Duration::from_secs(10), || {
        let p = probation(&root);
        (p["version"].as_str() == Some("v0.0.5"))
            .then(|| p["contacted_ms"].as_u64())
            .flatten()
    });
    no_pointer_leftovers(&root);

    // The next update makes v0.0.5 previous, and its probation hangs: the supervisor kills it
    // at the timeout, counts the start itself and starts the new release.
    let out = update("v0.0.6");
    assert_eq!(out.status, 0, "{} {}", out.stdout, out.stderr);
    sup.wait_log(
        "hung probation",
        "probation did not finish in 2 s and was killed",
        20,
    );
    sup.wait_log(
        "fallback",
        "counting the start with this supervisor's probation",
        10,
    );
    sup.wait_log("v0.0.6 started", "start 3, release v0.0.6", 20);
    assert!(!hung_alive(), "the hung probation outlived its timeout");
    eventually("node v0.0.6", Duration::from_secs(20), hello);
    assert_eq!(node_rpc(&node, &sock, "Presence", "{}").status, 0);
    eventually("v0.0.6 confirmed", Duration::from_secs(10), || {
        let p = probation(&root);
        (p["version"].as_str() == Some("v0.0.6"))
            .then(|| p["contacted_ms"].as_u64())
            .flatten()
    });
    assert!(sup.stop(), "{}", sup.log_text());
    drop(sup);

    // Stopping the supervisor while a probation runs ends that probation too, and starts nothing.
    let starts = |sup: &Supervise| sup.log_text().matches("supervise: start ").count();
    let mut sup = Supervise::start(&node, &root, &sock, &[]);
    eventually("probation hanging", Duration::from_secs(20), || {
        hung_alive().then_some(())
    });
    let before = starts(&sup);
    assert!(sup.stop(), "{}", sup.log_text());
    eventually("probation ended", Duration::from_secs(10), || {
        (!hung_alive()).then_some(())
    });
    assert_eq!(starts(&sup), before, "{}", sup.log_text());
}
