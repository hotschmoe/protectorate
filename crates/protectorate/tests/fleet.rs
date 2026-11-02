//! The hub runs the fleet, all real binaries: `protectorate serve` unpacks its embedded
//! headscale and member and runs them; a real `protectorate-node` on this machine joins through
//! the hub's headscale (`join`, `node add`, `node approve`) with the member it embeds, and serves
//! jobs over the tailnet. The test drives everything through the CLI: node listing with the
//! node's version, a `uname -a` job and its log by offset, artifacts through FetchArtifact (a
//! 3 MB binary, and symlink and `..` escapes that fail the job), link recovery after restarts,
//! lost replies, slow responses and a full member queue, protocol mismatches, submit fencing,
//! presence gating, cancelling a long job, hub recovery and a clean stop.
//!
//! Skips when the protectorate binary does not embed headscale and member (no Go to build
//! them), or when bwrap or protectorate-node is missing.

mod common;

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::{Hub, Run, bin, eventually, tail};
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

struct Resume(Option<u32>);

impl Drop for Resume {
    fn drop(&mut self) {
        if let Some(pid) = self.0.take() {
            let _ = Command::new("kill")
                .args(["-CONT", &pid.to_string()])
                .status();
        }
    }
}

fn start_node(
    bin: &Path,
    dir: &Path,
    sock: &Path,
    presence: &Path,
    version: Option<&str>,
    protocol: Option<&str>,
    drop_submit: Option<&Path>,
) -> Kill {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("serve.log"))
        .unwrap();
    let mut cmd = Command::new(bin);
    cmd.args(["serve", "--state-dir"])
        .arg(dir)
        .arg("--forward-socket")
        .arg(sock)
        .args(["--no-scope", "--presence-interval", "1"])
        .env("PROTECTORATE_NODE_TEST_PRESENCE", presence)
        .env(
            "PROTECTORATE_NODE_TEST_SLOW_PRESENCE",
            dir.join("slow-presence"),
        )
        .stdout(Stdio::null())
        .stderr(log);
    if let Some(v) = version {
        cmd.env("PROTECTORATE_NODE_TEST_VERSION", v);
    }
    if let Some(p) = protocol {
        cmd.env("PROTECTORATE_NODE_TEST_PROTOCOL", p);
    }
    if let Some(p) = drop_submit {
        cmd.env("PROTECTORATE_NODE_TEST_DROP_SUBMIT", p);
    }
    Kill(cmd.spawn().unwrap())
}

#[test]
fn version_flag_matches_version_command() {
    let command = Command::new(bin()).arg("version").output().unwrap();
    let flag = Command::new(bin()).arg("--version").output().unwrap();
    assert!(command.status.success());
    assert!(
        flag.status.success(),
        "{}",
        String::from_utf8_lossy(&flag.stderr)
    );
    assert_eq!(flag.stdout, command.stdout);
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

/// `protectorate-node rpc` straight to the agent's socket, as a hub would send it.
fn node_rpc(node_bin: &Path, sock: &Path, method: &str, fields: &str) -> Value {
    let out = Command::new(node_bin)
        .arg("rpc")
        .arg("--connect")
        .arg(sock)
        .args([method, fields])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "rpc {method} {fields}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    pt_json::parse(String::from_utf8_lossy(&out.stdout).trim()).unwrap()
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
                // A bare address: headscale refuses to start unless the hub widens it to a prefix.
                "trusted_proxies" => vec![Value::from("127.0.0.2")],
            },
        )],
        &[],
        &[("PROTECTORATE_FLEET_TEST_MEMBER_TIMEOUT_MS", "1000")],
    );
    let enrollment = eventually("the fleet to come up", Duration::from_secs(120), || {
        let r = hub.cli(&["--json", "node", "add"]);
        (r.status == 0).then(|| r.json())
    });
    let url = format!("http://{listen}");
    assert_eq!(enrollment["server_url"].as_str(), Some(url.as_str()));
    let fp = enrollment["control_key"].as_str().unwrap().to_string();
    assert!(fp.starts_with("sha256:"), "{enrollment}");
    let unpacked = |dir: &Path| {
        fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .find(|n| n.starts_with("member-"))
    };
    let hub_member = unpacked(&hub.home.join("net")).expect("the hub unpacked member");

    // The node joins with the member it embeds: join shows its registration, the owner pastes
    // it and approves.
    let node_dir = hub.home.with_file_name("node");
    fs::create_dir_all(&node_dir).unwrap();
    let mut join = Kill(
        Command::new(&node_bin)
            .args(["join", &url, "--name", "e2e-hubfleet", "--state-dir"])
            .arg(&node_dir)
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
    assert_eq!(
        unpacked(&node_dir.join("net")).as_deref(),
        Some(hub_member.as_str()),
        "the node unpacked the same member the hub carries"
    );

    let presence = node_dir.join("presence");
    fs::write(&presence, "AWAY").unwrap();
    let work = node_dir.join("jobs-cwd");
    fs::create_dir_all(&work).unwrap();
    let sock = node_dir.join("agent.sock");
    let mut serve = start_node(&node_bin, &node_dir, &sock, &presence, None, None, None);

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
    let printed = Command::new(&node_bin).arg("--version").output().unwrap();
    let printed = String::from_utf8_lossy(&printed.stdout).into_owned();
    let version = printed
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("protectorate-node "))
        .expect("protectorate-node --version")
        .to_string();
    assert_ne!(version, "0.0.0");
    assert_eq!(node["node_version"].as_str(), Some(version.as_str()));
    let shown = hub.cli(&["--json", "node", "show", "e2e-hubfleet"]).json();
    assert_eq!(shown["id"].as_str(), Some(node_id.as_str()));
    let human = hub.cli(&["node", "ls"]);
    let row = human
        .ok()
        .lines()
        .find(|l| l.contains("e2e-hubfleet"))
        .unwrap_or_else(|| panic!("{}", human.stdout))
        .to_string();
    assert!(
        row.contains(&version),
        "VERSION in node ls: {}",
        human.stdout
    );
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

    #[cfg(debug_assertions)]
    {
        let old_seen = node["last_seen_ms"].as_u64().unwrap();
        let before_log = fs::read_to_string(node_dir.join("serve.log"))
            .unwrap()
            .len();
        serve.stop();
        serve = start_node(
            &node_bin,
            &node_dir,
            &sock,
            &presence,
            Some("restart-test"),
            None,
            Some(&node_dir.join("drop-submit")),
        );
        eventually(
            "restarted node member to listen",
            Duration::from_secs(60),
            || {
                let log = fs::read_to_string(node_dir.join("serve.log")).ok()?;
                log[before_log..]
                    .contains("started mode=node")
                    .then_some(())
            },
        );
        // The tailnet takes 4 to 8 s here to reconverge on the restarted member (registration,
        // the hub member's netmap, a fresh handshake); a poller stuck on the dead connection
        // (the 20 s call timeout) would not make it.
        let ready = Instant::now();
        let fresh = eventually(
            "restarted node's fresh Hello",
            Duration::from_secs(15),
            || {
                let v = hub.cli(&["--json", "node", "show", &node_id]).json();
                (v["node_version"].as_str() == Some("restart-test")
                    && v["online"].as_bool() == Some(true)
                    && v["last_seen_ms"].as_u64()? > old_seen)
                    .then_some(v)
            },
        );
        assert!(ready.elapsed() < Duration::from_secs(15), "{fresh}");
        let effect_log = hub.log_text().len();
        let restarted = run_job(&hub, &[], &["echo", "after-restart"]);
        let done = wait_job(&hub, &restarted, &["succeeded", "failed", "lost"], 90);
        assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
        let log = hub.log_text();
        assert!(
            !log[effect_log..]
                .lines()
                .any(|l| (l.contains("effect ") && l.contains("SubmitJob"))
                    || l.contains("apply Lost {")
                    || l.contains("apply Nacked {")),
            "{}",
            &log[effect_log..]
        );

        fs::write(node_dir.join("drop-submit"), "drop").unwrap();
        let replay = run_job(
            &hub,
            &["--cwd", work.to_str().unwrap()],
            &["sh", "-c", "printf x >> exactly-once"],
        );
        let done = wait_job(&hub, &replay, &["succeeded", "failed", "lost"], 90);
        assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
        assert_eq!(fs::read(work.join("exactly-once")).unwrap(), b"x");
        assert!(
            fs::read_to_string(node_dir.join("serve.log"))
                .unwrap()
                .contains("dropped a submit reply for testing")
        );

        fs::write(node_dir.join("slow-presence"), "1").unwrap();
        eventually(
            "a trickling poll to time out",
            Duration::from_secs(12),
            || {
                let n = hub.cli(&["--json", "node", "show", &node_id]).json();
                n["error"]
                    .as_str()?
                    .contains("Presence: timed out")
                    .then_some(())
            },
        );
        fs::write(node_dir.join("slow-presence"), "0").unwrap();
        eventually(
            "polling to recover after a slow response",
            Duration::from_secs(15),
            || {
                let n = hub.cli(&["--json", "node", "show", &node_id]).json();
                (n["online"].as_bool() == Some(true) && n["error"].is_null()).then_some(())
            },
        );

        let member_path = tailnet.join("member");
        let member = processes_with(member_path.to_str().unwrap())
            .into_iter()
            .find(|(_, cmd)| cmd.contains("--mode hub"))
            .expect("the hub's member")
            .0;
        let before = hub.log_text().len();
        assert!(
            Command::new("kill")
                .args(["-STOP", &member.to_string()])
                .status()
                .unwrap()
                .success()
        );
        let resume = Resume(Some(member));
        let socket = tailnet.join("member.sock");
        let mut queued = Vec::new();
        let mut full = false;
        for _ in 0..5000 {
            match pt_sys::connect_unix_timeout(&socket, Duration::from_millis(100)) {
                Ok(s) => queued.push(s),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    full = true;
                    break;
                }
                Err(e) => panic!("filling member's accept queue: {e}"),
            }
        }
        assert!(full, "member's accept queue did not fill");
        eventually(
            "a full member queue to time out",
            Duration::from_secs(15),
            || {
                let log = hub.log_text();
                log[before..]
                    .lines()
                    .any(|l| l.contains("poller:") && l.contains("unix connect timed out"))
                    .then_some(())
            },
        );
        drop(queued);
        drop(resume);
        eventually(
            "polling after the member resumes",
            Duration::from_secs(30),
            || {
                let n = hub.cli(&["--json", "node", "show", &node_id]).json();
                (n["online"].as_bool() == Some(true) && n["error"].is_null()).then_some(())
            },
        );

        for protocol in ["2", "9"] {
            let before_log = fs::read_to_string(node_dir.join("serve.log"))
                .unwrap()
                .len();
            serve.stop();
            serve = start_node(
                &node_bin,
                &node_dir,
                &sock,
                &presence,
                None,
                Some(protocol),
                None,
            );
            let message =
                format!("node speaks protocol {protocol}, the hub speaks 3: reinstall the node");
            eventually("incompatible protocol", Duration::from_secs(60), || {
                let v = hub.cli(&["--json", "node", "show", &node_id]).json();
                (v["error"].as_str() == Some(message.as_str())).then_some(())
            });
            assert!(hub.cli(&["node", "ls"]).ok().contains(&message));
            assert!(hub.cli(&["node", "show", &node_id]).ok().contains(&message));
            let r = hub.cli(&["job", "run", "--node", "e2e-hubfleet", "--", "true"]);
            assert_ne!(r.status, 0);
            assert!(r.stderr.contains(&message), "{}", r.stderr);
            let r = hub.cli(&["node", "update", "e2e-hubfleet", "--version", "v0.0.1"]);
            assert_ne!(r.status, 0);
            assert!(r.stderr.contains(&message), "{} {}", r.stdout, r.stderr);
            let log = fs::read_to_string(node_dir.join("serve.log")).unwrap();
            assert!(
                !log[before_log..].contains("incompatible protocol received"),
                "{}",
                &log[before_log..]
            );
        }

        serve.stop();
        hub.kill9();
        serve = start_node(&node_bin, &node_dir, &sock, &presence, None, None, None);
        hub.restart();
        let first = eventually(
            "polling after hub and node restart",
            Duration::from_secs(120),
            || {
                let v = hub.cli(&["--json", "node", "show", &node_id]);
                if v.status != 0 {
                    return None;
                }
                let n = v.json();
                (n["online"].as_bool() == Some(true)
                    && n["node_version"].as_str() == Some(version.as_str()))
                .then_some(n)
            },
        );
        let last_seen = first["last_seen_ms"].as_u64().unwrap();
        eventually(
            "polling to continue after restart",
            Duration::from_secs(12),
            || {
                let n = hub.cli(&["--json", "node", "show", &node_id]).json();
                (n["online"].as_bool() == Some(true) && n["last_seen_ms"].as_u64()? > last_seen)
                    .then_some(())
            },
        );
        assert!(serve.0.try_wait().unwrap().is_none());
    }

    let waited = hub.cli(&[
        "job",
        "run",
        "--node",
        "e2e-hubfleet",
        "--wait",
        "--",
        "sh",
        "-c",
        "sleep 1; echo waited; exit 23",
    ]);
    assert_eq!(waited.status, 23, "{} {}", waited.stdout, waited.stderr);
    assert_eq!(waited.stdout, "waited\n");
    let waited = hub.cli(&[
        "job",
        "run",
        "--node",
        "e2e-hubfleet",
        "--wait",
        "--",
        "sh",
        "-c",
        "echo success",
    ]);
    assert_eq!(waited.status, 0, "{}", waited.stderr);
    assert_eq!(waited.stdout, "success\n");

    // A job that leaves an artifact in its directory.
    let cwd = work.to_str().unwrap();
    let abs = format!("{cwd}/out.bin");
    let art = run_job(
        &hub,
        &["--cwd", cwd, "--artifact", "out.bin", "--artifact", &abs],
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
    // An absolute path inside the job's directory names the same file.
    assert_eq!(done["artifacts"][1]["path"].as_str(), Some(abs.as_str()));
    assert_eq!(done["artifacts"][1]["sha256"], a["sha256"], "{done}");
    assert!(log_text(&hub, &art, 0).contains("wrote"));
    // The hub judges a cwd by the node's OS: this node is not Windows.
    let win = hub.cli(&[
        "job",
        "run",
        "--node",
        "e2e-hubfleet",
        "--cwd",
        r"C:\dev\x",
        "--",
        "true",
    ]);
    assert_ne!(win.status, 0, "{}", win.stdout);
    assert!(
        win.stderr.contains("absolute path on node"),
        "{}",
        win.stderr
    );
    // A permanent refusal is not retried.
    let missing = run_job(&hub, &["--cwd", cwd, "--artifact", "nope.txt"], &["true"]);
    let done = wait_job(&hub, &missing, &["succeeded", "failed"], 120);
    assert_eq!(done["state"].as_str(), Some("failed"), "{done}");
    let log = fs::read_to_string(node_dir.join("serve.log")).unwrap_or_default();
    assert_eq!(
        log.matches("artifact \"nope.txt\" refused").count(),
        1,
        "{log}"
    );

    // A 3 MB binary artifact in FetchArtifact slices, from the job's own directory (no --cwd).
    let big = run_job(
        &hub,
        &["--artifact", "big.bin", "--artifact", "./sub/small.txt"],
        &[
            "sh",
            "-c",
            "head -c 3000000 /dev/urandom > big.bin; mkdir sub; printf tiny > sub/small.txt; \
             sha256sum big.bin",
        ],
    );
    let done = wait_job(&hub, &big, &["succeeded", "failed"], 120);
    assert_eq!(done["state"].as_str(), Some("succeeded"), "{done}");
    let node_sha = log_text(&hub, &big, 0)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let a = &done["artifacts"][0];
    assert_eq!(a["size_bytes"].as_u64(), Some(3_000_000), "{done}");
    assert_eq!(a["sha256"].as_str(), Some(node_sha.as_str()), "{done}");
    hub.cli(&[
        "job",
        "artifact",
        &big,
        "big.bin",
        "--out",
        got.to_str().unwrap(),
    ])
    .ok();
    let bytes = fs::read(&got).unwrap();
    assert_eq!(bytes.len(), 3_000_000);
    assert_eq!(pt_hash::to_hex(&pt_hash::sha256(&bytes)), node_sha);
    assert_eq!(done["artifacts"][1]["size_bytes"].as_u64(), Some(4));
    assert!(
        hub.log_text()
            .contains(&format!("job {big}: artifact big.bin: 3000000 bytes")),
        "{}",
        tail(&hub.log_text(), 40)
    );
    let node_log = || fs::read_to_string(node_dir.join("serve.log")).unwrap_or_default();
    assert!(
        !node_log().contains(&format!("-j{big}-a")),
        "an artifact went through a cat job"
    );

    // A symlink or `..` out of the job's directory is refused by the node; the job fails.
    let outside = hub.home.with_file_name("outside.txt");
    fs::write(&outside, "not for the hub").unwrap();
    for (name, script) in [
        ("esc", format!("ln -s {} esc", outside.display())),
        ("../../logs/x", "true".to_string()),
    ] {
        let id = run_job(&hub, &["--artifact", name], &["sh", "-c", &script]);
        let done = wait_job(&hub, &id, &["succeeded", "failed"], 120);
        assert_eq!(done["state"].as_str(), Some("failed"), "{name}: {done}");
        assert!(done["artifacts"][0]["sha256"].is_null(), "{done}");
        assert!(
            node_log().contains(&format!("artifact {name:?} refused")),
            "{name}: {}",
            tail(&node_log(), 20)
        );
        assert_eq!(
            node_log()
                .matches(&format!("artifact {name:?} refused"))
                .count(),
            1
        );
    }
    assert!(
        hub.log_text().contains("outside the job's directory"),
        "{}",
        tail(&hub.log_text(), 40)
    );

    // Another submit already holds the node job id the hub's next job gets: its status (and
    // its output) are not this job's, so the hub ignores it and the job fails.
    let next: u64 = hub.cli(&["--json", "job", "ls"]).json()["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|j| j["id"].as_str()?.parse::<u64>().ok())
        .max()
        .unwrap()
        + 1;
    let planted_id = format!("{}-j{next}", hub.hub_tag());
    let planted = pt_json::obj! {
        "job_id" => planted_id.as_str(), "attempt" => 1u64, "script" => "echo planted",
        "token" => "planted", "even_if_present" => true,
    };
    node_rpc(&node_bin, &sock, "SubmitJob", &planted.to_string());
    let real = run_job(&hub, &[], &["echo", "real"]);
    assert_eq!(real, next.to_string(), "the hub's next job id");
    let done = wait_job(&hub, &real, &["succeeded", "failed"], 90);
    assert_eq!(done["state"].as_str(), Some("failed"), "{done}");
    assert!(!log_text(&hub, &real, 0).contains("planted"));
    assert!(
        hub.log_text().contains(&format!(
            "job {real}: node {node_id} holds another submit of attempt 1"
        )),
        "{}",
        tail(&hub.log_text(), 40)
    );
    let held = node_rpc(
        &node_bin,
        &sock,
        "JobStatus",
        &format!(r#"{{"job_id":"{planted_id}","attempt":1}}"#),
    );
    assert_eq!(held["job"]["token"].as_str(), Some("planted"), "{held}");

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
    for id in [&uname, &art, &big, &real, &gated, &urgent, &long, &slow] {
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
    serve.stop();
    eventually(
        "the node and its member to stop",
        Duration::from_secs(15),
        || {
            processes_with(node_dir.to_str().unwrap())
                .is_empty()
                .then_some(())
        },
    );
}
