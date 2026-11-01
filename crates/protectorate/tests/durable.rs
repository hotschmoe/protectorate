//! What survives a hub crash: pending approvals (a tool approval fenced to the incarnation that
//! asked, a D9 approval whose waiting call died), the event stream resumed by seq across the
//! restart, and the live hub's own journal replayed by this binary.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use common::*;
use pt_json::Value;

fn approvals(hub: &Hub) -> Vec<Value> {
    let v = hub.cli(&["--json", "approval", "list"]).json();
    v["approvals"].as_array().unwrap_or(&[]).to_vec()
}

fn approval(hub: &Hub, id: &str) -> Value {
    approvals(hub)
        .into_iter()
        .find(|a| a["id"].as_str() == Some(id))
        .unwrap_or(Value::Null)
}

fn pending(hub: &Hub, pick: impl Fn(&Value) -> bool) -> Value {
    eventually("a pending approval", Duration::from_secs(180), || {
        approvals(hub)
            .into_iter()
            .find(|a| a["state"].as_str() == Some("pending") && pick(a))
    })
}

/// A kept message's seq, or None for a streaming delta (it repeats the last kept seq).
fn kept_seq(m: &Value) -> Option<u64> {
    let delta =
        m["type"].as_str() == Some("entry") && m["entry"]["entry"]["delta"].as_bool() == Some(true);
    (!delta).then(|| m["seq"].as_u64().expect("a seq"))
}

/// A client follows the stream, the hub is SIGKILLed and restarts, and the client resumes from
/// the first seq it saw: every kept message comes back in order, identical, with no gap or
/// duplicate, and seqs continue after the restart. Hub events outlive the restart.
#[test]
fn stream_resumes_across_restart() {
    if !have_runtime() {
        return;
    }
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let mut hub = Hub::start_with("stream", "pi_local", &["--listen", &listen], &[]);
    let owner = hub.printed("owner token (shown once): ");
    let auth = format!("?access_token={owner}");

    let mut s1 = ws(port, &auth, &[]).expect("events");
    let snap = next(&mut s1);
    assert_eq!(snap["type"].as_str(), Some("snapshot"));
    let start = snap["seq"].as_u64().unwrap();
    pair(port, &owner, "phone");
    let phone = |m: &Value| m["event"]["text"].as_str() == Some("device phone paired");
    let before: BTreeMap<u64, Value> = until(&mut s1, "event", phone)
        .into_iter()
        .filter_map(|m| kept_seq(&m).map(|s| (s, m)))
        .collect();
    let last = *before.keys().next_back().unwrap();
    let phone_seq = before.values().find(|m| phone(m)).unwrap()["event"]["seq"]
        .as_u64()
        .unwrap();
    drop(s1);

    hub.restart();
    pair(port, &owner, "tablet");
    let mut s2 = ws(port, &format!("{auth}&from_seq={}", start + 1), &[]).unwrap();
    let tablet = |m: &Value| m["event"]["text"].as_str() == Some("device tablet paired");
    let after = until(&mut s2, "event", tablet);
    assert_ne!(
        after[0]["type"].as_str(),
        Some("snapshot"),
        "a resume across the restart replays: {}",
        after[0]
    );
    let mut want = start + 1;
    for m in &after {
        let Some(seq) = kept_seq(m) else { continue };
        assert_eq!(
            seq, want,
            "kept messages one seq apart across the restart: {m}"
        );
        if let Some(old) = before.get(&seq) {
            assert_eq!(old, m, "seq {seq} replayed as it was first sent");
        }
        want += 1;
    }
    assert!(
        want > last + 1,
        "messages after the restart continue the seqs"
    );

    let (s, ev) = post(port, "ListEvents", Some(&owner), "{}");
    assert_eq!(s, 200, "{ev}");
    let texts: Vec<(u64, &str)> = ev["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["seq"].as_u64().unwrap(), e["text"].as_str().unwrap()))
        .collect();
    assert!(
        texts.contains(&(phone_seq, "device phone paired")),
        "the event from before the crash keeps its seq: {texts:?}"
    );
    assert!(texts.iter().any(|(_, t)| *t == "device tablet paired"));
    let mut s3 = ws(port, &auth, &[]).unwrap();
    let snap = next(&mut s3);
    assert!(
        snap["state"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["seq"].as_u64() == Some(phone_seq)),
        "a fresh snapshot carries the older event"
    );
}

/// D9 across a crash: the pending approval stays visible with a note, a decision on it is
/// recorded as abandoned and removes nothing, and it is decided once. One whose deadline passed
/// while the hub was down is ended at recovery.
#[test]
fn d9_approval_across_restart() {
    if !have_runtime() {
        return;
    }
    let mut hub = Hub::start_config(
        "d9-restart",
        "pi_local",
        &[("d9_approval_timeout_secs", Value::from(600u64))],
        &[],
        &[],
    );
    hub.wait("envoy", "idle", 120);
    let envoy_id = hub.sleeve("envoy")["id"].as_str().unwrap().to_string();
    let agent = hub.printed("envoy-only token: ");
    let victim_repo = temp_repo("d9r-victim");
    hub.spawn_pi("victim", &victim_repo);

    let long = "Write the numbers from 1 to 1500 as words, one per line, with no other text.";
    // In an agent's turn, Envoy's own `sleeve rm victim` waits for the owner.
    let blocked_rm = |hub: &Hub| {
        hub.cli_env(
            &[("PROTECTORATE_TOKEN", agent.as_str())],
            &["envoy", "send", "--as", "grokbot", long],
        )
        .ok();
        eventually("Envoy working", Duration::from_secs(90), || {
            let s = hub.sleeve("envoy");
            (s["incarnation"]["activity"]["state"].as_str() == Some("working")).then_some(())
        });
        let g = hub.sleeve("envoy")["incarnation"]["g"].as_u64().unwrap();
        let token = hub.sleeve_token(&envoy_id, g);
        let (tx, rx) = mpsc::channel();
        let home = hub.home.clone();
        std::thread::spawn(move || {
            let out = Command::new(bin())
                .arg("--home")
                .arg(home)
                .args(["sleeve", "rm", "victim"])
                .env("PROTECTORATE_TOKEN", token)
                .output()
                .expect("run protectorate");
            let _ = tx.send(out);
        });
        let a = pending(hub, |a| {
            a["kind"].as_str() == Some("action")
                && a["title"].as_str() == Some("remove sleeve victim")
        });
        (a["id"].as_str().unwrap().to_string(), rx)
    };

    let (id, rm) = blocked_rm(&hub);
    hub.restart();
    let out = rm
        .recv_timeout(Duration::from_secs(30))
        .expect("rm returns");
    assert!(!out.status.success(), "the call died with the hub: {out:?}");
    let a = approval(&hub, &id);
    assert_eq!(a["state"].as_str(), Some("pending"), "{a}");
    assert!(
        a["note"].as_str().unwrap().contains("hub restarted"),
        "the pending approval says its call is gone: {a}"
    );
    let r = hub.cli(&["approval", "decide", &id, "allow"]);
    assert!(r.ok().contains("abandoned"), "{}", r.stdout);
    let a = approval(&hub, &id);
    assert_eq!(a["state"].as_str(), Some("abandoned"), "{a}");
    assert_eq!(a["decision"].as_str(), Some("allow_once"));
    assert_eq!(a["decided_by"].as_str(), Some("owner"));
    assert!(
        a["note"].as_str().unwrap().contains("nothing was done"),
        "{a}"
    );
    let again = hub.cli(&["approval", "decide", &id, "deny"]);
    assert_ne!(again.status, 0, "decided once: {}", again.stdout);
    assert!(again.stderr.contains("already"), "{}", again.stderr);
    std::thread::sleep(Duration::from_secs(3));
    kept(&hub);

    // A deadline that passes while the hub is down ends the approval at recovery.
    // Envoy may still be on the agent's message it resumed after the crash.
    hub.cli(&["sleeve", "interrupt", "envoy"]);
    hub.wait("envoy", "idle", 120);
    let config = hub.home.join("config.json");
    let text = std::fs::read_to_string(&config).unwrap();
    let mut c = pt_json::parse(&text).unwrap();
    c.as_object_mut()
        .unwrap()
        .insert("d9_approval_timeout_secs", Value::from(3u64));
    std::fs::write(&config, c.to_string()).unwrap();
    hub.restart();
    hub.wait("envoy", "idle", 120);
    let (id, _rm) = blocked_rm(&hub);
    hub.kill9();
    std::thread::sleep(Duration::from_secs(5));
    hub.restart();
    let a = approval(&hub, &id);
    assert_eq!(a["state"].as_str(), Some("abandoned"), "{a}");
    assert_eq!(a["decision"], Value::Null, "{a}");
    assert!(
        a["note"].as_str().unwrap().contains("nothing was done"),
        "{a}"
    );
    kept(&hub);
}

/// The victim was never stopped or removed. (Its own restart may park it: an idle Pi session
/// with no turn yet does not resume.)
fn kept(hub: &Hub) {
    let life = hub.sleeve("victim")["lifecycle"].clone();
    assert!(
        !matches!(
            life.as_str(),
            None | Some("stopping" | "stopped" | "removed")
        ),
        "the victim is {life}"
    );
}

/// A supervised Claude sleeve's tool approval: decided while its incarnation lives, the answer
/// reaches the harness; pending across a hub crash, it stays pending, and a decision after the
/// restart is fenced and runs nothing. Spends two small Claude turns.
#[test]
fn tool_approval_across_restart_paid() {
    if std::env::var_os("PT_E2E_PAID").is_none() {
        eprintln!("SKIP claude: spends subscription quota; set PT_E2E_PAID=1");
        return;
    }
    if !have_runtime() || !on_path("claude") {
        return;
    }
    let mut hub = Hub::start_config(
        "tool-restart",
        "pi_local",
        &[("supervised", Value::from(vec![Value::from("claude")]))],
        &[],
        &[],
    );
    let repo = temp_repo("tool-restart");
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "tooly",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "claude",
            "--model",
            "haiku",
        ])
        .json();
    let sid = s["id"].as_str().unwrap().to_string();
    hub.wait("tooly", "idle", 120);
    let touch = |f: &str| {
        format!("Use the Bash tool to run exactly this command: touch {f}\nThen reply DONE.")
    };
    let tool = |a: &Value| {
        a["kind"].as_str() == Some("tool") && a["sleeve_id"].as_str() == Some(sid.as_str())
    };

    hub.send("tooly", &touch("first.txt"));
    let a1 = pending(&hub, tool);
    let id1 = a1["id"].as_str().unwrap().to_string();
    assert!(!repo.join("first.txt").exists());
    let r = hub.cli(&["approval", "decide", &id1, "allow"]);
    assert!(r.ok().contains("approved"), "{}", r.stdout);
    eventually("the approved command ran", Duration::from_secs(120), || {
        repo.join("first.txt").exists().then_some(())
    });
    hub.wait("tooly", "idle", 180);

    hub.send("tooly", &touch("second.txt"));
    let a2 = pending(&hub, |a| tool(a) && a["id"].as_str() != Some(id1.as_str()));
    let id2 = a2["id"].as_str().unwrap().to_string();
    hub.restart();
    let a = eventually(
        "the asking incarnation ended",
        Duration::from_secs(60),
        || {
            let a = approval(&hub, &id2);
            a["note"]
                .as_str()
                .is_some_and(|n| n.contains("ended before a decision"))
                .then_some(a)
        },
    );
    assert_eq!(a["state"].as_str(), Some("pending"), "{a}");
    let r = hub.cli(&["approval", "decide", &id2, "allow"]);
    assert!(r.ok().contains("fenced"), "{}", r.stdout);
    let a = approval(&hub, &id2);
    assert_eq!(a["state"].as_str(), Some("fenced"), "{a}");
    assert!(
        a["note"].as_str().unwrap().contains("nothing was sent"),
        "{a}"
    );
    std::thread::sleep(Duration::from_secs(10));
    hub.cli(&["sleeve", "stop", "tooly"]).ok();
    hub.wait("tooly", "ended", 60);
    assert!(
        !repo.join("second.txt").exists(),
        "a fenced decision ran nothing"
    );
}

/// The live hub's journal (when this machine has one), copied and opened by this binary: every
/// frame decodes and the hub's sleeves are the ones an independent replay finds. The copy runs
/// with the owner's home read-only and no harness programs, since its sleeves' paths are the
/// live hub's.
#[test]
fn live_journal_replays() {
    let Some(owner_home) = std::env::var_os("HOME") else {
        return;
    };
    let live = Path::new(&owner_home).join(".local/share/protectorate-next/journal");
    if !live.is_dir() || !on_path("bwrap") {
        eprintln!("SKIP: no live journal at {}", live.display());
        return;
    }
    let dir = unique("live");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    for to in [home.join("journal"), dir.join("check")] {
        let ok = Command::new("cp")
            .arg("-a")
            .arg(&live)
            .arg(&to)
            .status()
            .unwrap()
            .success();
        assert!(ok, "copy the live journal");
    }
    let (_j, rec) = pt_journal::Journal::open(&dir.join("check")).expect("open the copy");
    assert!(
        rec.snapshot.is_none(),
        "this test predates journal snapshots"
    );
    let events: Vec<pt_core::Event> = rec
        .frames
        .iter()
        .enumerate()
        .map(|(i, f)| pt_core::codec::decode(f).unwrap_or_else(|e| panic!("frame {i}: {e}")))
        .collect();
    let replayed = pt_core::replay(&events);

    let nowhere = "/nonexistent/harness";
    let config = pt_json::obj! {
        "quota_poll_secs" => 0u64,
        "claude_program" => nowhere,
        "codex_program" => nowhere,
        "grok_program" => nowhere,
        "pi_program" => nowhere,
    };
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let (h, d) = (
        owner_home.to_str().unwrap().to_string(),
        dir.to_str().unwrap().to_string(),
    );
    let wrap = [
        "/usr/bin/bwrap",
        "--dev-bind",
        "/",
        "/",
        "--ro-bind",
        &h,
        &h,
        "--bind",
        &d,
        &d,
        "--",
    ];
    let hub = Hub::open(home, &wrap, &[("PATH", "/usr/bin:/bin")]);
    let log = hub.log_text();
    assert!(
        log.contains(&format!("replayed {} events", events.len())),
        "{}",
        tail(&log, 20)
    );
    let st = hub.state();
    let sleeves = st["sleeves"].as_array().unwrap();
    for s in replayed.sleeves.values() {
        if s.life == pt_core::Life::Removed {
            continue;
        }
        let got = sleeves
            .iter()
            .find(|v| v["id"].as_str() == Some(s.id.0.to_string().as_str()))
            .unwrap_or_else(|| panic!("sleeve {} ({}) is missing", s.id.0, s.name));
        assert_eq!(got["name"].as_str(), Some(s.name.as_str()), "{got}");
        assert_eq!(got["path"].as_str(), Some(s.claim.path.as_str()), "{got}");
        let g = got["incarnation"]["g"].as_u64().unwrap_or(0);
        assert!(g >= s.g.0, "{} went back from g{} to g{g}", s.name, s.g.0);
        if s.session.is_some() {
            assert_eq!(
                got["incarnation"]["native_id"].as_str(),
                s.session.as_deref(),
                "{} keeps its harness session",
                s.name
            );
        }
    }
}
