//! A fake GitHub release served by pt-http on loopback, fetched through the real curl. Staged
//! "binaries" are shell scripts that print their version, so every switch is checked by running
//! what `releases/current` points at.

use pt_http::{Config, Request, Response, Server, ShutdownHandle};
use pt_update::{Channel, ErrorKind, Source, Store, Verdict};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Good,
    /// SHA256SUMS lists another digest for the node binary.
    BadSum,
    /// The node binary's transfer stops halfway through a chunked body.
    Abort,
    /// The node binary is served complete but short of the size the API reports.
    Short,
}

struct Rel {
    tag: String,
    draft: bool,
    mode: Mode,
    assets: BTreeMap<String, Vec<u8>>,
}

const NODE: &str = "protectorate-node-linux-amd64";
const MEMBER: &str = "member-linux-amd64";

fn script(line: &str) -> Vec<u8> {
    let mut s = format!("#!/bin/sh\necho '{line}'\nexit 0\n").into_bytes();
    // Large enough that curl's truncation is a real mid-transfer cut, not one write.
    while s.len() < 300_000 {
        s.extend_from_slice(b"# padding padding padding padding padding padding padding\n");
    }
    s
}

fn rel(tag: &str, mode: Mode) -> Rel {
    let mut assets = BTreeMap::new();
    assets.insert(NODE.to_string(), script(&format!("node {tag}")));
    assets.insert(MEMBER.to_string(), script(&format!("member {tag}")));
    assets.insert("VERSION".to_string(), format!("{tag}\n").into_bytes());
    let mut sums = String::new();
    for (name, bytes) in &assets {
        let digest = if mode == Mode::BadSum && name == NODE {
            pt_hash::sha256(b"something else")
        } else {
            pt_hash::sha256(bytes)
        };
        sums.push_str(&format!("{}  {name}\n", pt_hash::to_hex(&digest)));
    }
    assets.insert("SHA256SUMS".to_string(), sums.into_bytes());
    Rel {
        tag: tag.to_string(),
        draft: false,
        mode,
        assets,
    }
}

struct Fake {
    url: String,
    handle: ShutdownHandle,
    thread: Option<thread::JoinHandle<io::Result<()>>>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.handle.shutdown(Duration::from_secs(1));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
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
        r#"{{"tag_name":"{}","draft":{},"prerelease":false,"assets":[{}]}}"#,
        r.tag,
        r.draft,
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
    let Some((tag, name)) = path
        .strip_prefix("/dl/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Response::not_found();
    };
    let Some(r) = rels.iter().find(|r| r.tag == tag) else {
        return Response::not_found();
    };
    let Some(bytes) = r.assets.get(name).cloned() else {
        return Response::not_found();
    };
    match (r.mode, name == NODE) {
        (Mode::Abort, true) => Response::stream(200, "application/octet-stream", move |w| {
            w.write_all(&bytes[..bytes.len() / 2])?;
            w.flush()?;
            Err(io::Error::other("cut"))
        }),
        (Mode::Short, true) => Response::bytes(
            200,
            "application/octet-stream",
            bytes[..bytes.len() - 1000].to_vec(),
        ),
        _ => Response::bytes(200, "application/octet-stream", bytes),
    }
}

fn start(rels: Vec<Rel>) -> Fake {
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
    let thread = Some(thread::spawn(move || server.run()));
    Fake {
        url,
        handle,
        thread,
    }
}

fn standard() -> Vec<Rel> {
    let mut draft = rel("v9.9.9", Mode::Good);
    draft.draft = true;
    vec![
        rel("v1.0.0", Mode::Good),
        rel("v1.2.0-rc.1", Mode::Good),
        rel("v1.1.0", Mode::Good),
        rel("v0.9.0-bad", Mode::BadSum),
        rel("v0.9.0-abort", Mode::Abort),
        rel("v0.9.0-short", Mode::Short),
        draft,
        rel("nightly", Mode::Good),
    ]
}

fn root(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pt-update-{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Runs a staged program and returns its output line.
fn run(path: &Path) -> String {
    let out = Command::new(path).output().expect("run staged binary");
    assert!(out.status.success(), "{} failed", path.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn current_node(store: &Store) -> String {
    run(&store.current_dir().join("protectorate-node"))
}

fn picks() -> Vec<pt_update::Pick> {
    pt_update::node_picks("linux", "amd64")
}

/// Nothing but finished versions, pointers, the lock and probation in releases/.
fn assert_no_leftovers(store: &Store) {
    for e in std::fs::read_dir(store.dir()).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        assert!(
            !name.contains("staging"),
            "staging dir {name} left in {}",
            store.dir().display()
        );
    }
}

#[test]
fn stage_switch_and_refusals() {
    let fake = start(standard());
    let src = Source::new(&fake.url, "o/r").unwrap();
    assert_eq!(src.latest(Channel::Stable).unwrap().tag, "v1.1.0");
    assert_eq!(src.latest(Channel::Dev).unwrap().tag, "v1.2.0-rc.1");

    let store = Store::open(&root("main")).unwrap();
    assert_eq!(store.current().unwrap(), None);

    let staged = store
        .stage(&src, &src.release("v1.0.0").unwrap(), &picks())
        .unwrap();
    assert!(!staged.reused);
    assert_eq!(run(&staged.dir.join("protectorate-node")), "node v1.0.0");
    assert!(
        !staged.dir.join("member").exists(),
        "only the picked asset is staged"
    );
    let sw = store.switch("v1.0.0").unwrap();
    assert_eq!(sw.from, None);
    assert_eq!(current_node(&store), "node v1.0.0");
    assert_eq!(store.previous().unwrap(), None);

    let latest = src.latest(Channel::Stable).unwrap();
    store.stage(&src, &latest, &picks()).unwrap();
    let sw = store.switch("v1.1.0").unwrap();
    assert_eq!(sw.from.as_deref(), Some("v1.0.0"));
    assert_eq!(current_node(&store), "node v1.1.0");
    assert_eq!(store.previous().unwrap().as_deref(), Some("v1.0.0"));

    let again = store
        .stage(&src, &src.release("v1.0.0").unwrap(), &picks())
        .unwrap();
    assert!(again.reused);
    assert_eq!(store.versions().unwrap(), ["v1.0.0", "v1.1.0"]);

    for (tag, kind, says) in [
        ("v0.9.0-bad", ErrorKind::Verify, "does not match SHA256SUMS"),
        ("v0.9.0-abort", ErrorKind::Fetch, "(18)"),
        ("v0.9.0-short", ErrorKind::Verify, "the release says"),
    ] {
        let e = store
            .stage(&src, &src.release(tag).unwrap(), &picks())
            .unwrap_err();
        assert_eq!(e.kind, kind, "{tag}: {e}");
        assert!(e.message.contains(says), "{tag}: {e}");
        assert!(!store.version_dir(tag).exists(), "{tag} was staged");
        assert_no_leftovers(&store);
        assert_eq!(current_node(&store), "node v1.1.0");
    }

    // A version already on disk that no longer matches the release is refused, not replaced.
    let node = store.version_dir("v1.0.0").join("protectorate-node");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&node)
        .unwrap();
    f.write_all(b"# changed\n").unwrap();
    drop(f);
    let e = store
        .stage(&src, &src.release("v1.0.0").unwrap(), &picks())
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Verify, "{e}");
    assert!(e.message.contains("exists and differs"), "{e}");

    let e = src.release("v4.0.0").unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch, "{e}");
    let e = store.switch("v0.9.0-bad").unwrap_err();
    assert_eq!(e.kind, ErrorKind::Store, "{e}");
}

#[test]
fn health_check_and_probation() {
    let fake = start(standard());
    let src = Source::new(&fake.url, "o/r").unwrap();
    let store = Store::open(&root("trial")).unwrap();
    for tag in ["v1.0.0", "v1.1.0"] {
        store
            .stage(&src, &src.release(tag).unwrap(), &picks())
            .unwrap();
    }
    store.switch("v1.0.0").unwrap();

    // No hub contact within the window: the node reverts.
    store.switch("v1.0.0").unwrap();
    store
        .switch_on_probation("v1.1.0", Duration::from_millis(400))
        .unwrap();
    assert_eq!(current_node(&store), "node v1.1.0");
    assert!(matches!(
        store.check_probation().unwrap(),
        Verdict::Waiting { .. }
    ));
    thread::sleep(Duration::from_millis(500));
    match store.check_probation().unwrap() {
        Verdict::RolledBack(sw) => {
            assert_eq!(
                (sw.from.as_deref(), sw.to.as_str()),
                (Some("v1.1.0"), "v1.0.0")
            )
        }
        v => panic!("expected a rollback, got {v:?}"),
    }
    assert_eq!(current_node(&store), "node v1.0.0");
    assert_eq!(store.check_probation().unwrap(), Verdict::Idle);
    assert!(!store.confirm("v1.1.0").unwrap());
    let p = store.probation().unwrap().unwrap();
    assert!(p.rolled_back_ms.is_some() && p.contacted_ms.is_none());
    assert!(
        p.reason.as_deref().unwrap().contains("not confirmed"),
        "{p:?}"
    );

    // A release that keeps failing to start goes back after MAX_TRIAL_STARTS, long before
    // its window ends; the counting runs from the previous release, before each start.
    store
        .switch_on_probation("v1.1.0", Duration::from_secs(3600))
        .unwrap();
    for _ in 0..pt_update::MAX_TRIAL_STARTS {
        assert!(matches!(
            store.start_attempt(pt_update::MAX_TRIAL_STARTS).unwrap(),
            Verdict::Waiting { .. }
        ));
        assert_eq!(current_node(&store), "node v1.1.0");
    }
    match store.start_attempt(pt_update::MAX_TRIAL_STARTS).unwrap() {
        Verdict::RolledBack(sw) => assert_eq!(sw.to, "v1.0.0"),
        v => panic!("expected a rollback, got {v:?}"),
    }
    assert_eq!(current_node(&store), "node v1.0.0");
    assert_eq!(
        store.start_attempt(pt_update::MAX_TRIAL_STARTS).unwrap(),
        Verdict::Idle
    );

    // A failed health check rolls back at once.
    store
        .switch_on_probation("v1.1.0", Duration::from_secs(3600))
        .unwrap();
    let sw = store.fail_probation("no answer on the socket").unwrap();
    assert_eq!(sw.map(|s| s.to), Some("v1.0.0".to_string()));
    assert_eq!(current_node(&store), "node v1.0.0");
    let p = store.probation().unwrap().unwrap();
    assert_eq!(p.reason.as_deref(), Some("no answer on the socket"));
    assert_eq!(store.fail_probation("again").unwrap(), None);

    // Contact in time: the update stays past the deadline.
    store
        .switch_on_probation("v1.1.0", Duration::from_millis(400))
        .unwrap();
    // A check that passed for another release confirms nothing.
    assert!(!store.confirm("v1.0.0").unwrap());
    assert!(store.confirm("v1.1.0").unwrap());
    assert!(!store.confirm("v1.1.0").unwrap());
    thread::sleep(Duration::from_millis(500));
    assert_eq!(store.check_probation().unwrap(), Verdict::Confirmed);
    assert_eq!(current_node(&store), "node v1.1.0");

    // A first install has nothing to fall back to.
    let fresh = Store::open(&root("fresh")).unwrap();
    fresh
        .stage(&src, &src.release("v1.0.0").unwrap(), &picks())
        .unwrap();
    let e = fresh
        .switch_on_probation("v1.0.0", Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Store, "{e}");
}
