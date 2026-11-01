//! The fleet on one machine, all real binaries: pt-fleet runs headscale (in bwrap) and the hub
//! member; a node (`protectorate-node join`, then `serve` supervising its own member) joins
//! through the QR registration and the hub's approval; the poller and the client then reach it
//! over the tailnet for Hello, Inventory, Presence and a job's log. Then the supervisors are
//! tested: the node's member, the hub member and headscale are each killed and come back, a
//! headscale snapshot is taken, and the node is renamed and deleted.
//!
//! Needs bwrap and go/bin's headscale and member for this host (built with scripts/net-build.sh
//! when Go is on PATH, otherwise the test is skipped). `PT_E2E_BIND` picks headscale's address
//! (default 127.0.0.1).

use std::fs;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pt_fleet::{Config, Fleet, FleetEvent, PollConfig, Registration, TAG_HUB, TAG_NODE};
use pt_frame::msg::{JobState, SubmitJob};
use pt_json::ToJson;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn go_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}

/// headscale and member for this host, building them once when Go is available.
fn go_bins() -> Option<(PathBuf, PathBuf)> {
    let bin = root().join("go/bin");
    let hs = bin.join(format!("headscale-linux-{}", go_arch()));
    let member = bin.join(format!("member-linux-{}", go_arch()));
    if !(hs.is_file() && member.is_file()) {
        eprintln!("building headscale and member with scripts/net-build.sh host");
        let built = Command::new(root().join("scripts/net-build.sh"))
            .arg("host")
            .status()
            .is_ok_and(|s| s.success());
        if !built {
            eprintln!("SKIP: no go/bin binaries and scripts/net-build.sh failed (is Go on PATH?)");
            return None;
        }
    }
    Some((hs, member))
}

/// protectorate-node from the same build (cargo builds every workspace binary before tests).
fn node_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().unwrap();
    let p = exe.parent()?.parent()?.join("protectorate-node");
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

fn free_tcp(ip: IpAddr) -> u16 {
    TcpListener::bind((ip, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn free_udp() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait<T>(what: &str, secs: u64, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {secs} s waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

struct Kill(Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Children of `pid`, from /proc.
fn children(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for e in fs::read_dir("/proc").unwrap().flatten() {
        let Some(p) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        let ppid = stat
            .rsplit_once(')')
            .and_then(|(_, r)| r.split_whitespace().nth(1))
            .and_then(|p| p.parse::<u32>().ok());
        if ppid == Some(pid) {
            out.push(p);
        }
    }
    out
}

/// The QR lines `join` printed (half blocks, light modules as blocks) back into dark modules,
/// written as a scaled PBM; zbarimg decodes it when installed.
fn zbar_check(qr_lines: &[String], payload: &str, dir: &Path) {
    let rows: Vec<Vec<bool>> = qr_lines
        .iter()
        .flat_map(|l| {
            let (top, bottom): (Vec<bool>, Vec<bool>) = l
                .chars()
                .map(|c| match c {
                    '\u{2588}' => (false, false),
                    '\u{2580}' => (false, true),
                    '\u{2584}' => (true, false),
                    _ => (true, true),
                })
                .unzip();
            [top, bottom]
        })
        .collect();
    let (w, h, scale) = (rows[0].len(), rows.len(), 6);
    let mut pbm = format!("P1\n{} {}\n", w * scale, h * scale);
    for row in &rows {
        let line: String = row
            .iter()
            .flat_map(|&d| std::iter::repeat_n(if d { "1 " } else { "0 " }, scale))
            .collect();
        for _ in 0..scale {
            pbm.push_str(&line);
            pbm.push('\n');
        }
    }
    let path = dir.join("join-qr.pbm");
    fs::write(&path, pbm).unwrap();
    match Command::new("zbarimg")
        .args(["--raw", "-q"])
        .arg(&path)
        .output()
    {
        Ok(out) => {
            let decoded = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                decoded.trim(),
                payload,
                "zbarimg decoded the join QR differently"
            );
            eprintln!("zbarimg decoded the join QR to the registration");
        }
        Err(_) => eprintln!("zbarimg is not installed: QR not decoded (REVISIT.md)"),
    }
}

#[test]
fn local_fleet_end_to_end() {
    let Some((headscale, member)) = go_bins() else {
        return;
    };
    let Some(node_bin) = node_bin() else { return };
    if Command::new("bwrap").arg("--version").output().is_err() {
        eprintln!("SKIP: bwrap is not on PATH");
        return;
    }
    let ip: IpAddr = std::env::var("PT_E2E_BIND")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse()
        .expect("PT_E2E_BIND is an IP");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("f/{:x}", nanos & 0xffff_ffff));
    fs::create_dir_all(&base).unwrap();
    eprintln!("e2e dir {}", base.display());

    // Hub side.
    let listen = SocketAddr::new(ip, free_tcp(ip));
    let mut cfg = Config::lan_direct(base.join("hub/tailnet"), headscale, member.clone(), listen);
    cfg.stun_listen = SocketAddr::from(([127, 0, 0, 1], free_udp()));
    let fleet = Fleet::start(cfg).expect("fleet starts");
    let st = fleet.status().unwrap();
    assert_eq!(st.backend_state, "Running");
    let fp = fleet.control_fingerprint().unwrap();
    assert_eq!(st.control_key, fp, "hub member pinned another key");
    let hub_pin = fs::read_to_string(base.join("hub/tailnet/member/control-key.pin")).unwrap();
    assert_eq!(hub_pin.trim(), fp);
    let hs_nodes = fleet.admin().nodes().unwrap();
    assert!(
        hs_nodes
            .iter()
            .any(|n| n.has_tag(TAG_HUB) && n.name == "hub"),
        "no tag:hub node in {hs_nodes:?}"
    );

    // The node joins: the QR and its text, then the hub approves.
    let node_dir = base.join("node");
    fs::create_dir_all(&node_dir).unwrap();
    let mut join = Command::new(&node_bin)
        .args([
            "join",
            &format!("http://{listen}"),
            "--name",
            "e2e-node",
            "--state-dir",
        ])
        .arg(&node_dir)
        .arg("--member")
        .arg(&member)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let out = join.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines() {
            let Ok(l) = l else { break };
            println!("join| {l}");
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let mut qr = Vec::new();
    let mut approve_line = String::new();
    let payload = loop {
        let l = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("join shows its registration");
        if l.contains('\u{2588}') {
            qr.push(l);
        } else if l.trim_start().starts_with("protectorate node approve ") {
            approve_line = l.trim().to_string();
        } else if let Some(p) = l.trim_start().strip_prefix("paste") {
            break p.trim().to_string();
        }
    };
    let reg = Registration::parse(&payload).expect("the pasted text is a registration");
    assert_eq!(reg.name, "e2e-node");
    assert_eq!(reg.control_key, fp, "the node saw another control key");
    assert_eq!(
        approve_line,
        format!(
            "protectorate node approve {} --name e2e-node --fingerprint {fp}",
            reg.auth_id
        )
    );
    assert!(qr.len() >= 15, "QR has {} lines", qr.len());
    zbar_check(&qr, &payload, &base);

    let bogus = format!("sha256:{}", "0".repeat(64));
    let refused = fleet.approve(&reg.auth_id, "e2e-node", &bogus).unwrap_err();
    assert!(
        refused.to_string().contains("control key mismatch"),
        "{refused}"
    );
    fleet.record_pending(reg.clone()).unwrap();
    assert!(
        fleet
            .pending()
            .unwrap()
            .iter()
            .any(|p| p.registration == reg)
    );
    let approved = fleet
        .approve(&reg.auth_id, &reg.name, &reg.control_key)
        .expect("approve");
    assert_eq!(approved.name, "e2e-node");
    assert!(approved.has_tag(TAG_NODE));
    assert!(fleet.pending().unwrap().is_empty());
    assert!(
        fleet
            .enrolled()
            .unwrap()
            .iter()
            .any(|e| e.id == approved.id && e.control_key == fp)
    );
    let status = wait("join to exit", 60, || join.try_wait().unwrap());
    assert!(status.success(), "join exited {status}");
    let node_cfg = fs::read_to_string(node_dir.join("node.json")).unwrap();
    assert!(node_cfg.contains(&fp), "{node_cfg}");
    let node_pin = fs::read_to_string(node_dir.join("member/control-key.pin")).unwrap();
    assert_eq!(node_pin.trim(), fp);

    // serve runs the agent and supervises the node's member.
    fs::write(node_dir.join("presence"), "AWAY").unwrap();
    let serve_log = fs::File::create(node_dir.join("serve.log")).unwrap();
    let serve = Kill(
        Command::new(&node_bin)
            .args(["serve", "--state-dir"])
            .arg(&node_dir)
            .arg("--forward-socket")
            .arg(node_dir.join("agent.sock"))
            .arg("--member")
            .arg(&member)
            .arg("--no-scope")
            .env("PROTECTORATE_NODE_TEST_PRESENCE", node_dir.join("presence"))
            .stdout(Stdio::null())
            .stderr(serve_log)
            .spawn()
            .unwrap(),
    );

    let (etx, erx) = mpsc::channel();
    let poller = fleet
        .poll(
            PollConfig {
                presence_every: Duration::from_secs(2),
                sample_every: Duration::from_secs(3),
                inventory_every: Duration::from_secs(30),
                list_every: Duration::from_secs(1),
                call_timeout: Duration::from_secs(10),
            },
            move |e| {
                let _ = etx.send(e);
            },
        )
        .unwrap();
    let id = approved.id.to_string();
    // The first event, at or after `since`, of the node online with an inventory: the poller
    // reached it after the disruption being tested.
    let online = |what: &str, since: pt_api::EpochMs| {
        wait(what, 120, || {
            match erx.recv_timeout(Duration::from_secs(1)) {
                Ok(FleetEvent::Node(n))
                    if n.id == id
                        && n.online
                        && n.inventory.is_some()
                        && n.last_seen_ms >= since =>
                {
                    Some(n)
                }
                _ => None,
            }
        })
    };
    let node = online("the node online with an inventory", pt_api::EpochMs(0));
    println!("node: {}", node.to_json());
    assert_eq!(node.name, "e2e-node");
    assert_eq!(node.presence.state, pt_api::PresenceState::Away);
    assert_eq!(node.presence.source, "test-override");
    assert!(node.inventory.as_ref().unwrap().ram_bytes > 0);
    let view = poller.node(&id).unwrap();
    let agent_id = view.hello.as_ref().unwrap().node_id.clone();
    let ip = view.ip.clone().unwrap();
    assert!(ip.starts_with("100."), "{ip}");

    // A job over the tailnet: submit, wait, collect the log, acknowledge.
    let mut c = fleet.connect(&ip).unwrap();
    let hello = c.hello().unwrap();
    assert_eq!(hello.node_id, agent_id);
    let reply = c
        .submit_job(SubmitJob {
            job_id: "e2e-uname".into(),
            attempt: 1,
            argv: Some(vec!["uname".into(), "-a".into()]),
            script: None,
            cwd: None,
            env: Default::default(),
            timeout_secs: 60,
            log_cap: 1 << 20,
            even_if_present: false,
            on_present: pt_frame::msg::OnPresent::Lower,
            token: None,
            maintenance: false,
        })
        .expect("submit");
    assert_eq!(reply.journal_epoch, hello.journal_epoch);
    let done = wait("the job to end", 60, || {
        let j = c.job_status("e2e-uname", Some(1)).unwrap().job?;
        j.state.is_terminal().then_some(j)
    });
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    let (log, end) = c
        .fetch_log(pt_frame::msg::FetchLog {
            job_id: "e2e-uname".into(),
            attempt: 1,
            offset: 0,
            max: 1 << 20,
            token: None,
        })
        .unwrap();
    let log = String::from_utf8_lossy(&log).into_owned();
    println!("job log: {}", log.trim());
    assert!(log.contains("Linux") && end.done, "{log:?} {end:?}");
    assert!(
        c.list_jobs()
            .unwrap()
            .jobs
            .iter()
            .any(|j| j.job_id == "e2e-uname")
    );
    c.ack_collected(pt_frame::msg::JobRef {
        job_id: "e2e-uname".into(),
        attempt: 1,
        token: None,
    })
    .unwrap();
    assert!(
        c.list_jobs().unwrap().jobs.is_empty(),
        "an acknowledged job is kept"
    );
    drop(c);

    // The node's member dies: serve restarts it, and the node comes back with the same identity.
    let node_member = wait("serve's member", 30, || {
        children(serve.0.id()).into_iter().find(|p| {
            fs::read_to_string(format!("/proc/{p}/cmdline"))
                .is_ok_and(|c| c.contains("--mode\0node"))
        })
    });
    let t = pt_api::EpochMs::now();
    Command::new("kill")
        .args(["-KILL", &node_member.to_string()])
        .status()
        .unwrap();
    wait("a new node member", 30, || {
        children(serve.0.id()).into_iter().find(|p| {
            *p != node_member
                && fs::read_to_string(format!("/proc/{p}/cmdline"))
                    .is_ok_and(|c| c.contains("--mode\0node"))
        })
    });
    let _ = online("the node online after its member restarted", t);

    // The hub member dies uncleanly: its socket stays, the next start takes a fresh name.
    let first_socket = fleet.member_socket();
    let starts = fleet.member_state().starts;
    let t = pt_api::EpochMs::now();
    fleet.kill_child("member").unwrap();
    wait("the hub member restarted", 60, || {
        let s = fleet.member_state();
        (s.starts > starts && s.healthy).then_some(())
    });
    assert_ne!(fleet.member_socket(), first_socket);
    let _ = online("the node online after the hub member restarted", t);

    // headscale dies: restarted with the same database and key.
    let starts = fleet.headscale_state().starts;
    fleet.kill_child("headscale").unwrap();
    wait("headscale restarted", 60, || {
        let s = fleet.headscale_state();
        (s.starts > starts && s.healthy).then_some(())
    });
    assert_eq!(fleet.control_fingerprint().unwrap(), fp);
    assert!(
        fleet
            .admin()
            .nodes()
            .unwrap()
            .iter()
            .any(|n| n.id == approved.id)
    );

    let t = pt_api::EpochMs::now();
    let snap = fleet.snapshot_headscale("e2e").unwrap();
    assert!(snap.join("db.sqlite").is_file() && snap.join("noise_private.key").is_file());
    let after = online("the node online after the snapshot", t);
    assert_eq!(poller.node(&id).unwrap().hello.unwrap().node_id, agent_id);
    assert_eq!(after.id, id);

    // Rename, expire and delete through headscale's CLI.
    fleet.admin().rename(approved.id, "e2e-renamed").unwrap();
    wait("the new name", 30, || {
        match erx.recv_timeout(Duration::from_secs(1)) {
            Ok(FleetEvent::Node(n)) if n.id == id && n.name == "e2e-renamed" => Some(()),
            _ => None,
        }
    });
    fleet.admin().expire(approved.id).unwrap();
    wait("the node offline after expiry", 60, || {
        match erx.recv_timeout(Duration::from_secs(1)) {
            Ok(FleetEvent::Node(n)) if n.id == id && !n.online => Some(()),
            _ => None,
        }
    });
    fleet.admin().delete(approved.id).unwrap();
    wait("the node removed", 30, || {
        match erx.recv_timeout(Duration::from_secs(1)) {
            Ok(FleetEvent::Removed(r)) if r == id => Some(()),
            _ => None,
        }
    });
    assert!(
        fleet
            .admin()
            .nodes()
            .unwrap()
            .iter()
            .all(|n| n.id != approved.id)
    );
    drop(poller);
    drop(serve);
    drop(fleet);
}
