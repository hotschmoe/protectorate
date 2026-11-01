//! The hub runs the fleet, all real binaries: `protectorate serve` unpacks its embedded
//! headscale and member and runs them; a real `protectorate-node` on this machine joins through
//! the hub's headscale (`join`, `node add`, `node approve`) and serves jobs over the tailnet. The
//! test drives everything through the CLI: node listing, a `uname -a` job and its log by
//! offset, a job that leaves an artifact, presence gating, cancelling a long job, a hub crash
//! mid-job that the restarted hub reconciles, and a planned stop that leaves no headscale or
//! member behind.
//!
//! Skips when the protectorate binary does not embed headscale and member (no Go to build
//! them), or when bwrap or protectorate-node is missing.

mod common;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use common::{Hub, Run, bin, eventually, tail};
use pt_json::{Value, obj};

struct Kill(Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn node_bin() -> Option<PathBuf> {
    let p = Path::new(bin()).with_file_name("protectorate-node");
    if p.is_file() {
        Some(p)
    } else {
        eprintln!(
            "SKIP: {} is missing; run cargo test --workspace",
            p.display()
        );
        None
    }
}

/// A TCP port whose next port is free too: headscale binds STUN there.
fn free_pair() -> u16 {
    loop {
        let p = common::free_port();
        if p < u16::MAX && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
            return p;
        }
    }
}

/// Every process whose command line mentions `needle`.
fn processes_with(needle: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for e in fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        let Ok(c) = fs::read(e.path().join("cmdline")) else {
            continue;
        };
        let c = String::from_utf8_lossy(&c).replace('\0', " ");
        if c.contains(needle) {
            out.push((pid, c));
        }
    }
    out
}

fn job_json(hub: &Hub, id: &str) -> Value {
    hub.cli(&["--json", "job", "show", id]).json()
}

fn wait_job(hub: &Hub, id: &str, states: &[&str], secs: u64) -> Value {
    let mut last = Value::Null;
    let found = (0..secs * 5).find_map(|_| {
        let j = job_json(hub, id);
        if states.contains(&j["state"].as_str().unwrap_or("")) {
            return Some(j);
        }
        last = j;
        std::thread::sleep(Duration::from_millis(200));
        None
    });
    found.unwrap_or_else(|| {
        panic!(
            "job {id} never reached {states:?}: {last}\n--- hub log tail ---\n{}",
            tail(&hub.log_text(), 60)
        )
    })
}

fn run_job(hub: &Hub, extra: &[&str], argv: &[&str]) -> String {
    let mut args = vec!["--json", "job", "run", "--node", "e2e-hubfleet"];
    args.extend(extra);
    args.push("--");
    args.extend(argv);
    let r: Run = hub.cli(&args);
    let v = r.json();
    v["id"].as_str().expect("job id").to_string()
}

fn log_text(hub: &Hub, id: &str, offset: u64) -> String {
    let v = hub
        .cli(&["--json", "job", "log", id, "--offset", &offset.to_string()])
        .json();
    v["text"].as_str().unwrap_or("").to_string()
}

#[test]
fn hub_runs_the_fleet_and_node_jobs() {
    let version = Command::new(bin()).arg("version").output().unwrap();
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    if version.contains("missing") {
        eprintln!("SKIP: this protectorate does not embed headscale and member: {version}");
        return;
    }
    let Some(node_bin) = node_bin() else { return };
    if Command::new("bwrap").arg("--version").output().is_err() {
        eprintln!("SKIP: bwrap is not on PATH");
        return;
    }
    let port = free_pair();
    let listen = format!("127.0.0.1:{port}");
    // member's socket lives in the tailnet dir: the target dir is too deep for a unix socket.
    let tailnet = std::env::temp_dir().join(format!("pt-hubfleet-{}-{port}", std::process::id()));
    let mut hub = Hub::start_config(
        "fleet",
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
        &[],
    );
    let enrollment = eventually("the fleet to come up", Duration::from_secs(120), || {
        let r = hub.cli(&["--json", "node", "add"]);
        (r.status == 0).then(|| r.json())
    });
    let url = format!("http://{listen}");
    assert_eq!(enrollment["server_url"].as_str(), Some(url.as_str()));
    let fp = enrollment["control_key"].as_str().unwrap().to_string();
    assert!(fp.starts_with("sha256:"), "{enrollment}");
    let member = fs::read_dir(hub.home.join("net"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("member-"))
        })
        .expect("the hub unpacked member");

    // The node joins: join shows its registration, the owner pastes it and approves.
    let node_dir = hub.home.with_file_name("node");
    fs::create_dir_all(&node_dir).unwrap();
    // Its own copy, so the hub's member is told apart from the node's on the process list.
    let member = {
        let own = node_dir.with_file_name("node-member");
        fs::copy(&member, &own).unwrap();
        own
    };
    let mut join = Kill(
        Command::new(&node_bin)
            .args(["join", &url, "--name", "e2e-hubfleet", "--state-dir"])
            .arg(&node_dir)
            .arg("--member")
            .arg(&member)
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
    let pending = hub.cli(&["--json", "node", "add", &payload]).json();
    assert_eq!(pending["name"].as_str(), Some("e2e-hubfleet"));
    assert_eq!(pending["key_matches"].as_bool(), Some(true), "{pending}");
    let auth = pending["auth_id"].as_str().unwrap().to_string();
    let listed = hub.cli(&["--json", "node", "add"]).json();
    assert!(
        listed["pending"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["auth_id"].as_str() == Some(&auth)),
        "{listed}"
    );
    let bogus = format!("sha256:{}", "0".repeat(64));
    let refused = hub.cli(&["node", "approve", &auth, "--fingerprint", &bogus]);
    assert_ne!(refused.status, 0);
    assert!(
        refused.stderr.contains("control key mismatch"),
        "{}",
        refused.stderr
    );
    let approved = hub.cli(&["--json", "node", "approve", &auth]).json();
    assert_eq!(approved["name"].as_str(), Some("e2e-hubfleet"));
    let node_id = approved["node_id"].as_str().unwrap().to_string();
    let status = eventually("join to exit", Duration::from_secs(60), || {
        join.0.try_wait().unwrap()
    });
    assert!(status.success(), "join exited {status}");

    let presence = node_dir.join("presence");
    fs::write(&presence, "AWAY").unwrap();
    let work = node_dir.join("jobs-cwd");
    fs::create_dir_all(&work).unwrap();
    let serve_log = fs::File::create(node_dir.join("serve.log")).unwrap();
    let _serve = Kill(
        Command::new(&node_bin)
            .args(["serve", "--state-dir"])
            .arg(&node_dir)
            .arg("--forward-socket")
            .arg(node_dir.join("agent.sock"))
            .arg("--member")
            .arg(&member)
            .args(["--no-scope", "--presence-interval", "1"])
            .env("PROTECTORATE_NODE_TEST_PRESENCE", &presence)
            .stdout(Stdio::null())
            .stderr(serve_log)
            .spawn()
            .unwrap(),
    );

    // ListNodes from the poller's views.
    let node = eventually("the node online", Duration::from_secs(120), || {
        let v = hub.cli(&["--json", "node", "ls"]).json();
        v["nodes"]
            .as_array()?
            .iter()
            .find(|n| {
                n["id"].as_str() == Some(&node_id)
                    && n["online"].as_bool() == Some(true)
                    && n["inventory"].as_object().is_some()
                    && n["presence"]["state"].as_str() == Some("away")
            })
            .cloned()
    });
    assert_eq!(node["name"].as_str(), Some("e2e-hubfleet"));
    assert!(!node["node_version"].as_str().unwrap_or("").is_empty());
    let shown = hub.cli(&["--json", "node", "show", "e2e-hubfleet"]).json();
    assert_eq!(shown["id"].as_str(), Some(node_id.as_str()));
    let human = hub.cli(&["node", "ls"]);
    assert!(human.ok().contains("e2e-hubfleet"), "{}", human.stdout);
    let state = hub.state();
    assert!(
        state["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"].as_str() == Some(&node_id)),
        "GetState has the node"
    );
    let events = hub.cli(&["--json", "event", "list"]).json();
    assert!(
        events["events"].as_array().unwrap().iter().any(|e| {
            e["kind"].as_str() == Some("node")
                && e["text"]
                    .as_str()
                    .unwrap_or("")
                    .contains("e2e-hubfleet is online")
        }),
        "a node event: {events}"
    );

    // uname -a, and its log by offset.
    let uname = run_job(&hub, &[], &["uname", "-a"]);
    let done = wait_job(&hub, &uname, &["succeeded", "failed"], 90);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    assert_eq!(done["exit_code"].as_i64(), Some(0));
    assert_eq!(done["node_id"].as_str(), Some(node_id.as_str()));
    let log = log_text(&hub, &uname, 0);
    assert!(log.contains("Linux"), "{log:?}");
    assert_eq!(log_text(&hub, &uname, 6), log[6..]);
    let page = hub
        .cli(&[
            "--json", "job", "log", &uname, "--offset", "0", "--limit", "5",
        ])
        .json();
    assert_eq!(page["next_offset"].as_u64(), Some(5));
    assert_eq!(page["eof"].as_bool(), Some(false));
    let end = hub
        .cli(&[
            "--json",
            "job",
            "log",
            &uname,
            "--offset",
            &log.len().to_string(),
        ])
        .json();
    assert_eq!(end["eof"].as_bool(), Some(true), "{end}");

    // A job that leaves an artifact in its directory.
    let cwd = work.to_str().unwrap();
    let art = run_job(
        &hub,
        &["--cwd", cwd, "--artifact", "out.bin"],
        &[
            "sh",
            "-c",
            "printf 'artifact\\001\\377bytes' > out.bin; echo wrote",
        ],
    );
    let done = wait_job(&hub, &art, &["succeeded", "failed"], 120);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    let expected = b"artifact\x01\xffbytes";
    let a = &done["artifacts"][0];
    assert_eq!(a["path"].as_str(), Some("out.bin"));
    assert_eq!(
        a["size_bytes"].as_u64(),
        Some(expected.len() as u64),
        "{done}"
    );
    assert_eq!(
        a["sha256"].as_str(),
        Some(pt_hash::to_hex(&pt_hash::sha256(expected)).as_str())
    );
    let got = hub.home.with_file_name("artifact.out");
    hub.cli(&[
        "job",
        "artifact",
        &art,
        "out.bin",
        "--out",
        got.to_str().unwrap(),
    ])
    .ok();
    assert_eq!(fs::read(&got).unwrap(), expected);
    assert!(log_text(&hub, &art, 0).contains("wrote"));
    // A missing artifact fails the job after its verify attempts.
    let missing = run_job(&hub, &["--cwd", cwd, "--artifact", "nope.txt"], &["true"]);
    let done = wait_job(&hub, &missing, &["succeeded", "failed"], 120);
    assert_eq!(done["state"].as_str(), Some("failed"), "{done}");

    // Presence gates: nothing starts while someone is at the node, unless asked to.
    fs::write(&presence, "PRESENT").unwrap();
    eventually("the node PRESENT", Duration::from_secs(30), || {
        let v = hub.cli(&["--json", "node", "show", &node_id]).json();
        (v["presence"]["state"].as_str() == Some("present")).then_some(())
    });
    let gated = run_job(&hub, &[], &["echo", "gated"]);
    std::thread::sleep(Duration::from_secs(6));
    assert_eq!(job_json(&hub, &gated)["state"].as_str(), Some("queued"));
    let urgent = run_job(&hub, &["--even-if-present"], &["echo", "urgent"]);
    let done = wait_job(&hub, &urgent, &["succeeded", "failed"], 90);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    fs::write(&presence, "AWAY").unwrap();
    let done = wait_job(&hub, &gated, &["succeeded", "failed"], 90);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    assert!(log_text(&hub, &gated, 0).contains("gated"));

    // A long job, cancelled.
    let long = run_job(&hub, &[], &["sleep", "600"]);
    wait_job(&hub, &long, &["running"], 60);
    let r = hub.cli(&["--json", "job", "cancel", &long]).json();
    assert_eq!(r["state"].as_str(), Some("cancelling"), "{r}");
    let done = wait_job(&hub, &long, &["cancelled"], 90);
    assert!(done["ended_at_ms"].as_u64().is_some(), "{done}");

    // The hub crashes mid-job; the restarted hub polls the job and collects it.
    let slow = run_job(&hub, &[], &["sh", "-c", "sleep 8; echo finished"]);
    wait_job(&hub, &slow, &["running"], 60);
    hub.restart();
    let done = wait_job(&hub, &slow, &["succeeded", "failed", "lost"], 180);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    assert!(log_text(&hub, &slow, 0).contains("finished"));
    let jobs = hub.cli(&["--json", "job", "ls"]).json();
    let ids: Vec<&str> = jobs["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|j| j["id"].as_str())
        .collect();
    for id in [&uname, &art, &gated, &urgent, &long, &slow] {
        assert!(ids.contains(&id.as_str()), "job ls lists {id}: {ids:?}");
    }
    let human = hub.cli(&["job", "ls", "--node", "e2e-hubfleet"]);
    assert!(human.ok().contains("sleep 600"), "{}", human.stdout);

    // A planned stop takes headscale and member with it.
    let needle = tailnet.to_str().unwrap().to_string();
    assert!(
        processes_with(&needle).len() >= 2,
        "headscale and member run: {:?}",
        processes_with(&needle)
    );
    let status = hub.stop().expect("the hub ran");
    assert!(status.success(), "the hub exited {status}");
    let left = processes_with(&needle);
    assert!(left.is_empty(), "left behind: {left:?}");
    let net = hub.home.join("net");
    let left = processes_with(net.to_str().unwrap());
    assert!(left.is_empty(), "left behind: {left:?}");
}
