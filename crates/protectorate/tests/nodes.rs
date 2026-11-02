//! Removing and renaming fleet nodes, all real binaries: a hub with its embedded headscale and
//! member, and two real `protectorate-node`s on this machine that join it. The scenario is the
//! reinstalled machine: `e2e-old` goes offline for good, the same machine came back as `e2e-new`,
//! the owner removes the stale entry and renames the new one into the freed name. Along the way:
//! name validation and collisions, refusing to remove an online node, queued jobs pinned to the
//! removed node failing (by the scheduler on its own when the removal stopped short), a job
//! queued by name that keeps to the node's id, digits-only names refused at join, approval and
//! rename, the enrollment record failing to write after headscale changed (the rename is put
//! back, the removal is finished by retrying it by id), a rename repeated after a crash
//! repairing the record, the event log, a removed node that cannot come back on its old
//! key, and everything surviving a hub restart, which reads headscale's own database again and
//! completes a hub member enrollment that stopped before its marker.
//!
//! Skips when the protectorate binary does not embed headscale and member, or when bwrap or
//! protectorate-node is missing.

mod common;

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use common::{Hub, bin, eventually, tail};
use pt_json::{Value, obj};

struct Kill(Child);

impl Kill {
    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Kill {
    fn drop(&mut self) {
        self.stop();
    }
}

fn free_pair() -> u16 {
    loop {
        let p = common::free_port();
        if p < u16::MAX && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
            return p;
        }
    }
}

/// `protectorate-node join`, the owner pasting its registration and approving it; the node id.
fn join(hub: &Hub, node_bin: &Path, url: &str, name: &str, dir: &Path) -> String {
    fs::create_dir_all(dir).unwrap();
    let digits = Command::new(node_bin)
        .args(["join", url, "--name", "1234", "--state-dir"])
        .arg(dir)
        .output()
        .unwrap();
    assert!(!digits.status.success());
    assert!(
        String::from_utf8_lossy(&digits.stderr).contains("reads as a node id"),
        "{}",
        String::from_utf8_lossy(&digits.stderr)
    );
    let mut join = Kill(
        Command::new(node_bin)
            .args(["join", url, "--name", name, "--state-dir"])
            .arg(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let (tx, rx) = mpsc::channel();
    let out = join.0.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines() {
            let Ok(l) = l else { break };
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
    let auth = hub.cli(&["--json", "node", "add", &payload]).json()["auth_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = hub.cli(&["node", "approve", &auth, "--name", "1234"]);
    assert!(r.stderr.contains("reads as a node id"), "{}", r.stderr);
    let approved = hub.cli(&["--json", "node", "approve", &auth]).json();
    assert_eq!(approved["name"].as_str(), Some(name), "{approved}");
    let status = eventually("join to exit", Duration::from_secs(60), || {
        join.0.try_wait().unwrap()
    });
    assert!(status.success(), "join exited {status}");
    approved["node_id"].as_str().unwrap().to_string()
}

fn serve(node_bin: &Path, dir: &Path) -> Kill {
    let presence = dir.join("presence");
    fs::write(&presence, "AWAY").unwrap();
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("serve.log"))
        .unwrap();
    Kill(
        Command::new(node_bin)
            .args(["serve", "--state-dir"])
            .arg(dir)
            .arg("--forward-socket")
            .arg(dir.join("agent.sock"))
            .args(["--no-scope", "--presence-interval", "1"])
            .env("PROTECTORATE_NODE_TEST_PRESENCE", &presence)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    )
}

fn nodes(hub: &Hub) -> Vec<Value> {
    hub.cli(&["--json", "node", "ls"]).json()["nodes"]
        .as_array()
        .map(<[Value]>::to_vec)
        .unwrap_or_default()
}

fn listed(hub: &Hub, id: &str) -> Option<Value> {
    nodes(hub)
        .into_iter()
        .find(|n| n["id"].as_str() == Some(id))
}

/// Waits through a fleet that is still starting.
fn online(hub: &Hub, id: &str, want: bool) -> Value {
    eventually(
        &format!("node {id} online={want}"),
        Duration::from_secs(120),
        || {
            let r = hub.cli(&["--json", "node", "show", id]);
            let n = (r.status == 0).then(|| r.json())?;
            (n["online"].as_bool() == Some(want)).then_some(n)
        },
    )
}

fn job(hub: &Hub, id: &str) -> Value {
    hub.cli(&["--json", "job", "show", id]).json()
}

fn wait_job(hub: &Hub, id: &str, states: &[&str], secs: u64) -> Value {
    let mut last = Value::Null;
    for _ in 0..secs * 5 {
        let j = job(hub, id);
        if states.contains(&j["state"].as_str().unwrap_or("")) {
            return j;
        }
        last = j;
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!(
        "job {id} never reached {states:?}: {last}\n{}",
        tail(&hub.log_text(), 60)
    )
}

fn run_job(hub: &Hub, node: &str, argv: &[&str]) -> Value {
    let mut args = vec!["--json", "job", "run", "--node", node, "--"];
    args.extend(argv);
    hub.cli(&args).json()
}

/// `(id, name)` of every record in the hub's enrollment file.
fn enrolled(tailnet: &Path) -> Vec<(String, String)> {
    let v = pt_json::parse(&fs::read_to_string(tailnet.join("enrolled.json")).unwrap()).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["id"].as_u64().unwrap().to_string(),
                e["name"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn events(hub: &Hub) -> Vec<Value> {
    hub.cli(&["--json", "event", "list", "--limit", "1000"])
        .json()["events"]
        .as_array()
        .map(<[Value]>::to_vec)
        .unwrap_or_default()
}

fn has_event(hub: &Hub, level: &str, text: &str) -> bool {
    events(hub).iter().any(|e| {
        e["kind"].as_str() == Some("node")
            && e["level"].as_str() == Some(level)
            && e["text"].as_str().unwrap_or("").contains(text)
    })
}

/// A directory where the enrollment file's temporary copy goes: writing the record fails, while
/// reading it and every headscale call still work.
struct Blocker(PathBuf);

impl Blocker {
    fn new(tailnet: &Path) -> Blocker {
        let tmp = tailnet.join("enrolled.tmp");
        fs::create_dir_all(tmp.join("blocker")).unwrap();
        Blocker(tmp)
    }

    fn lift(self) {
        fs::rename(&self.0, self.0.with_file_name("enrolled.tmp-aside")).unwrap();
    }
}

#[test]
fn owner_removes_a_stale_node_and_renames_its_successor() {
    let version = Command::new(bin()).arg("version").output().unwrap();
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    if version.contains("missing") {
        eprintln!("SKIP: this protectorate does not embed headscale and member: {version}");
        return;
    }
    let node_bin = Path::new(bin()).with_file_name("protectorate-node");
    if !node_bin.is_file() {
        eprintln!("SKIP: {} is missing", node_bin.display());
        return;
    }
    if Command::new("bwrap").arg("--version").output().is_err() {
        eprintln!("SKIP: bwrap is not on PATH");
        return;
    }
    let port = free_pair();
    let listen = format!("127.0.0.1:{port}");
    let tailnet = std::env::temp_dir().join(format!("pt-noderm-{}-{port}", std::process::id()));
    let mut hub = Hub::start_config(
        "noderm",
        "pi_local",
        &[(
            "fleet",
            obj! {
                "listen" => listen.as_str(),
                "dir" => tailnet.to_str().unwrap(),
                "poll_secs" => 2u64,
            },
        )],
        &[],
        &[("PROTECTORATE_FLEET_TEST_MEMBER_TIMEOUT_MS", "1000")],
    );
    eventually("the fleet to come up", Duration::from_secs(120), || {
        (hub.cli(&["node", "add"]).status == 0).then_some(())
    });
    let url = format!("http://{listen}");
    // Short paths: each node's agent socket lives in its state dir.
    let old_dir = tailnet.with_file_name(format!("pt-noderm-{}-{port}-old", std::process::id()));
    let new_dir = tailnet.with_file_name(format!("pt-noderm-{}-{port}-new", std::process::id()));
    let old = join(&hub, &node_bin, &url, "e2e-old", &old_dir);
    let new = join(&hub, &node_bin, &url, "e2e-new", &new_dir);
    let mut old_serve = serve(&node_bin, &old_dir);
    let _new_serve = serve(&node_bin, &new_dir);
    online(&hub, &old, true);
    online(&hub, &new, true);

    // Names: a tailnet label, not all digits, not another node's (the hub's own member counts).
    for (bad, why) in [
        ("Bad_Name", "lowercase letters"),
        ("1234", "reads as a node id"),
        ("e2e-old", "already exists"),
        ("hub", "already exists"),
    ] {
        let r = hub.cli(&["node", "rename", "e2e-new", bad]);
        assert_ne!(r.status, 0, "rename to {bad} succeeded");
        assert!(r.stderr.contains(why), "rename to {bad}: {}", r.stderr);
    }
    let r = hub.cli(&["node", "rename", "e2e-nope", "e2e-x"]);
    assert!(r.stderr.contains("no node"), "{}", r.stderr);

    // An online node is not removed without force.
    let r = hub.cli(&["node", "rm", "e2e-old"]);
    assert_ne!(r.status, 0);
    assert!(r.stderr.contains("is online"), "{}", r.stderr);

    // The old machine goes away for good; a job queued for it by name is kept by its id.
    old_serve.stop();
    online(&hub, &old, false);
    let pinned = run_job(&hub, "e2e-old", &["echo", "never"]);
    assert_eq!(
        pinned["target"]["node_id"].as_str(),
        Some(old.as_str()),
        "{pinned}"
    );
    let pinned = pinned["id"].as_str().unwrap().to_string();
    let long = run_job(&hub, "e2e-new", &["sleep", "600"]);
    let long = long["id"].as_str().unwrap().to_string();
    wait_job(&hub, &long, &["running"], 90);
    let r = hub.cli(&["job", "run", "--node", "e2e-nope", "--", "true"]);
    assert!(r.stderr.contains("no node"), "{}", r.stderr);

    // The rename lands in headscale, then the record cannot be written: headscale's name is put
    // back and stays back across poll rounds.
    let blocker = Blocker::new(&tailnet);
    let r = hub.cli(&["node", "rename", "e2e-new", "e2e-newer"]);
    assert_ne!(r.status, 0);
    assert!(
        r.stderr.contains("headscale's name is back to e2e-new"),
        "{}",
        r.stderr
    );
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(
        listed(&hub, &new).unwrap()["name"].as_str(),
        Some("e2e-new")
    );

    // The removal deletes the node in headscale, then the record cannot be dropped: the error
    // says so and names the id to retry with, and the node is gone from the list. The handler
    // stopped before its jobs, as a crash there would: the scheduler fails the job pinned to the
    // node headscale no longer lists on its own.
    let r = hub.cli(&["node", "rm", "e2e-old"]);
    assert_ne!(r.status, 0);
    let retry = format!("finish with: protectorate node rm {old}");
    assert!(r.stderr.contains(&retry), "{}", r.stderr);
    assert!(listed(&hub, &old).is_none());
    assert!(has_event(&hub, "alarm", &retry));
    let failed = wait_job(&hub, &pinned, &["failed"], 30);
    let gone = format!("node {old} was removed");
    assert_eq!(failed["error"].as_str(), Some(gone.as_str()), "{failed}");
    assert!(
        events(&hub).iter().any(|e| e["text"].as_str()
            == Some(format!("job {pinned} failed: node {old} was removed").as_str())),
        "{:?}",
        events(&hub)
    );
    let r = hub.cli(&["job", "run", "--node", &old, "--", "true"]);
    assert!(r.stderr.contains("no node"), "{}", r.stderr);
    blocker.lift();
    let removed = hub.cli(&["--json", "node", "rm", &old]).json();
    assert_eq!(removed["node_id"].as_str(), Some(old.as_str()), "{removed}");
    assert_eq!(removed["record_only"].as_bool(), Some(true), "{removed}");
    assert_eq!(
        removed["failed_jobs"],
        Value::Array(Vec::new()),
        "{removed}"
    );
    assert!(!enrolled(&tailnet).iter().any(|(id, _)| *id == old));
    assert!(has_event(&hub, "info", "node e2e-old removed"));
    let r = hub.cli(&["node", "rm", &old]);
    assert!(r.stderr.contains("no node"), "{}", r.stderr);

    // The new machine takes the freed name; its running job keeps to it by id.
    let renamed = hub
        .cli(&["--json", "node", "rename", "e2e-new", "e2e-old"])
        .json();
    assert_eq!(renamed["node_id"].as_str(), Some(new.as_str()), "{renamed}");
    assert_eq!(renamed["from"].as_str(), Some("e2e-new"));
    assert_eq!(renamed["to"].as_str(), Some("e2e-old"));
    assert_eq!(
        listed(&hub, &new).unwrap()["name"].as_str(),
        Some("e2e-old")
    );
    assert!(has_event(&hub, "info", "node e2e-new renamed e2e-old"));
    assert!(
        enrolled(&tailnet).contains(&(new.clone(), "e2e-old".to_string())),
        "{:?}",
        enrolled(&tailnet)
    );
    // A rename stopped after headscale and before its record (a crash), repeated: headscale
    // already has the name, and the record is repaired.
    let stale = fs::read_to_string(tailnet.join("enrolled.json"))
        .unwrap()
        .replace("\"e2e-old\"", "\"e2e-new\"");
    fs::write(tailnet.join("enrolled.json"), stale).unwrap();
    assert!(enrolled(&tailnet).contains(&(new.clone(), "e2e-new".to_string())));
    let human = hub.cli(&["node", "rename", "e2e-old", "e2e-old"]);
    assert!(human.ok().contains("from e2e-old to e2e-old"));
    assert!(
        enrolled(&tailnet).contains(&(new.clone(), "e2e-old".to_string())),
        "{:?}",
        enrolled(&tailnet)
    );
    let still = job(&hub, &long);
    assert_eq!(still["state"].as_str(), Some("running"), "{still}");
    assert_eq!(still["node_id"].as_str(), Some(new.as_str()));
    hub.cli(&["job", "cancel", &long]).ok();
    wait_job(&hub, &long, &["cancelled"], 90);
    let echo = run_job(&hub, "e2e-old", &["echo", "renamed"]);
    let echo = echo["id"].as_str().unwrap().to_string();
    let done = wait_job(&hub, &echo, &["succeeded", "failed"], 90);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    assert_eq!(done["node_id"].as_str(), Some(new.as_str()));

    // The removed machine comes back on its old key: headscale no longer knows it.
    let _old_serve = serve(&node_bin, &old_dir);
    std::thread::sleep(Duration::from_secs(10));
    let ids: Vec<String> = nodes(&hub)
        .iter()
        .filter_map(|n| n["id"].as_str().map(String::from))
        .collect();
    assert_eq!(ids, vec![new.clone()], "{}", tail(&hub.log_text(), 40));

    // A restarted hub reads headscale's database again: one node, under its new name. Its
    // member's enrollment marker is gone, as after a first start cut short once the member was
    // registered: the member reports itself registered, and the hub finds it and marks it.
    let marker = tailnet.join("member").join("hub-enrolled");
    let hub_id = fs::read_to_string(&marker).unwrap();
    fs::remove_file(&marker).unwrap();
    hub.restart();
    let n = online(&hub, &new, true);
    assert_eq!(fs::read_to_string(&marker).unwrap(), hub_id);
    assert_eq!(n["name"].as_str(), Some("e2e-old"));
    assert_eq!(nodes(&hub).len(), 1);

    // Force removes an online node with a job in flight and says so.
    let flight = run_job(&hub, "e2e-old", &["sleep", "600"]);
    let flight = flight["id"].as_str().unwrap().to_string();
    wait_job(&hub, &flight, &["running"], 90);
    let r = hub.cli(&["node", "rm", &new]);
    assert!(r.stderr.contains("is online"), "{}", r.stderr);
    let removed = hub.cli(&["--json", "node", "rm", &new, "--force"]).json();
    assert_eq!(removed["record_only"].as_bool(), Some(false), "{removed}");
    assert_eq!(
        removed["orphaned_jobs"],
        Value::Array(vec![Value::from(flight.as_str())]),
        "{removed}"
    );
    assert!(nodes(&hub).is_empty());
    assert!(enrolled(&tailnet).is_empty());
    std::thread::sleep(Duration::from_secs(5));
    assert!(nodes(&hub).is_empty());
    assert!(
        !events(&hub).iter().any(|e| e["text"]
            .as_str()
            .unwrap_or("")
            .contains("left the tailnet")),
        "an owner's removal is not reported as the node leaving"
    );
    let status = hub.stop().expect("the hub ran");
    assert!(status.success(), "the hub exited {status}");
}
