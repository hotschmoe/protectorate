//! Whole scenarios against the real hub binary: sleeves in real bwrap sandboxes and systemd
//! scopes, real Pi on the local model, the journal on disk, the CLI over the unix socket.

mod common;

use std::collections::HashSet;
use std::os::unix::process::CommandExt;
use std::time::Duration;

use common::*;

/// Spawn, prompt, transcript, checkpoint, stop: and the worked-on repo shows nothing of ours.
#[test]
fn sleeve_lifecycle() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start("lifecycle");
    let st = hub.state();
    let roles: Vec<&str> = st["sleeves"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["role"].as_str())
        .collect();
    assert_eq!(roles, ["envoy"], "a fresh hub has Envoy and nothing else");

    let repo = temp_repo("lifecycle");
    let s = hub.spawn_pi("one", &repo);
    let id = s["id"].as_str().unwrap().to_string();
    let rid = hub.send("one", READY);
    hub.wait("one", "idle", 180);

    let entries = hub.transcript("one");
    assert!(
        assistant_text(&entries).contains("READY"),
        "the reply is in the transcript"
    );
    let users = of_type(&entries, "user_message");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["entry"]["request_id"].as_str(), Some(rid.as_str()));
    let seqs: Vec<u64> = entries.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(
        seqs.windows(2).all(|w| w[1] == w[0] + 1),
        "seq is gapless: {seqs:?}"
    );

    let scope = hub.scope(&id, 1);
    assert!(!scope_pids(&scope).is_empty(), "{scope} holds the harness");
    let checkpoints = hub.home.join(format!("sleeves/{id}/checkpoints.jsonl"));
    eventually(
        "a checkpoint after the turn",
        Duration::from_secs(20),
        || {
            std::fs::read_to_string(&checkpoints)
                .ok()
                .filter(|t| t.contains("\"g\":1"))
        },
    );

    hub.cli(&["sleeve", "stop", "one"]).ok();
    hub.wait("one", "ended", 60);
    assert!(scope_pids(&scope).is_empty(), "{scope} is gone after stop");
    // Removed, the sleeve keeps nothing open in the hub: its executor ends, its transcript
    // closes.
    hub.cli(&["sleeve", "rm", "one"]).ok();
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    let log = hub.home.join(format!("sleeves/{id}/transcript.jsonl"));
    let pid = serve_pid(&hub.home);
    let open = || {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .map(|d| {
                d.flatten()
                    .any(|f| std::fs::read_link(f.path()).ok() == Some(log.clone()))
            })
            .unwrap_or(false)
    };
    let threads = || {
        std::fs::read_dir(format!("/proc/{pid}/task"))
            .map(|d| {
                d.flatten()
                    .filter(|t| {
                        std::fs::read_to_string(t.path().join("comm"))
                            .is_ok_and(|c| c.trim() == format!("pt-exec-{id}"))
                    })
                    .count()
            })
            .unwrap_or(0)
    };
    eventually(
        "the removed sleeve's files and thread to go",
        Duration::from_secs(10),
        || (!open() && threads() == 0 && !scratch.exists()).then_some(()),
    );

    assert_eq!(
        git_out(&repo, &["status", "--porcelain"]),
        "",
        "no file of ours in the repo"
    );
    assert_eq!(
        git_out(&repo, &["for-each-ref", "--format=%(refname)"]),
        "refs/heads/main",
        "no ref of ours in the repo"
    );
}

/// kill -9 of the hub mid-turn: on restart the journal replays, HubRecovered probes the scope,
/// the sleeve resumes natively as g2, the transcript continues, and the prompt that was in
/// flight is never delivered twice.
#[test]
fn crash_mid_turn_recovers() {
    if !have_runtime() {
        return;
    }
    let mut hub = Hub::start("crash");
    let repo = temp_repo("crash");
    let s = hub.spawn_pi("two", &repo);
    let id = s["id"].as_str().unwrap().to_string();
    let long = hub.send("two", LONG);
    eventually("the long turn to start", Duration::from_secs(60), || {
        let s = hub.sleeve("two");
        (s["incarnation"]["activity"]["state"].as_str() == Some("working")).then_some(())
    });
    std::thread::sleep(Duration::from_secs(1));
    hub.kill9();
    eventually(
        "the sandbox to die with its hub",
        Duration::from_secs(20),
        || scope_pids(&hub.scope(&id, 1)).is_empty().then_some(()),
    );
    // A systemd-run the dead hub started that never registered its scope: recovery kills it
    // before probing, or it would register g1 after the probe found g1 empty.
    let mut stale = Reaped(
        std::process::Command::new("sh")
            .arg0("systemd-run")
            .args(["-c", "while :; do sleep 1; done", "--unit"])
            .arg(hub.scope(&id, 1))
            .spawn()
            .unwrap(),
    );

    hub.restart();
    eventually(
        "recovery to kill the unregistered systemd-run",
        Duration::from_secs(30),
        || stale.0.try_wait().unwrap().map(drop),
    );
    let log = hub.log_text();
    assert!(
        log.contains("apply HubRecovered"),
        "HubRecovered is applied on start"
    );
    assert!(
        !log.contains("replayed 0 events"),
        "the journal is replayed"
    );
    hub.wait("two", "idle", 180);
    let s = hub.sleeve("two");
    assert_eq!(s["lifecycle"].as_str(), Some("running"));
    let g = s["incarnation"]["g"].as_u64().unwrap();
    assert!(g >= 2, "a new incarnation after the crash, got g{g}");

    let again = hub.send("two", READY);
    hub.wait("two", "idle", 180);
    let entries = hub.transcript("two");
    let seqs: Vec<u64> = entries.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    let unique: HashSet<u64> = seqs.iter().copied().collect();
    assert_eq!(
        unique.len(),
        seqs.len(),
        "no seq repeats across the restart"
    );
    assert!(
        seqs.windows(2).all(|w| w[1] > w[0]),
        "seq is monotonic: {seqs:?}"
    );
    let delivered: Vec<&str> = of_type(&entries, "user_message")
        .iter()
        .filter_map(|e| e["entry"]["request_id"].as_str())
        .collect();
    assert_eq!(
        delivered.iter().filter(|r| **r == long).count(),
        1,
        "the in-flight prompt went to the harness once: {delivered:?}"
    );
    assert!(delivered.contains(&again.as_str()));
    let sessions: HashSet<u64> = of_type(&entries, "session_info")
        .iter()
        .filter_map(|e| e["g"].as_u64())
        .collect();
    assert!(
        sessions.contains(&1) && sessions.contains(&g),
        "both incarnations: {sessions:?}"
    );
    let notices: Vec<&str> = of_type(&entries, "notice")
        .iter()
        .filter_map(|e| e["entry"]["text"].as_str())
        .collect();
    assert!(
        notices
            .iter()
            .any(|n| n.starts_with("resumed natively") || n.starts_with("resleeved")),
        "the recovery is in the chat: {notices:?}"
    );
    let after_crash = entries
        .iter()
        .skip_while(|e| e["g"].as_u64() == Some(1))
        .map(|e| e["entry"]["text"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        after_crash.contains("READY"),
        "the resumed incarnation answers"
    );
}

/// A child killed and reaped when the test ends, passing or not.
struct Reaped(std::process::Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The Envoy round trip a vendor general agent uses: send, wait for the settled reply, read.
#[test]
fn envoy_round_trip() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start("envoy");
    hub.wait("envoy", "idle", 120);
    let rid = hub
        .cli(&[
            "envoy",
            "send",
            "Reply with exactly the word PONG and nothing else.",
        ])
        .ok()
        .trim()
        .to_string();
    assert!(rid.starts_with("owner:"), "request id {rid}");
    let reply = hub.cli(&["envoy", "wait", &rid, "--timeout", "180"]);
    assert!(
        reply.ok().contains("PONG"),
        "Envoy answered: {}",
        reply.stdout
    );
    let again = hub.cli(&["envoy", "wait", "--timeout", "10"]);
    assert!(
        again.ok().contains("PONG"),
        "wait without an id finds the latest message"
    );
    let read = hub.cli(&["envoy", "read"]);
    assert!(
        read.ok()
            .contains("owner: Reply with exactly the word PONG")
    );
    // A vendor agent reads its own exchanges with Envoy, never the owner's.
    let agent_token = hub.printed("envoy-only token: ");
    let as_agent = [("PROTECTORATE_TOKEN", agent_token.as_str())];
    hub.cli_env(
        &as_agent,
        &[
            "envoy",
            "send",
            "Reply with exactly the word BETA and nothing else.",
        ],
    )
    .ok();
    let beta = hub.cli_env(&as_agent, &["envoy", "wait", "--timeout", "180"]);
    assert!(
        beta.ok().contains("BETA"),
        "Envoy answered: {}",
        beta.stdout
    );
    let theirs = hub.cli_env(&as_agent, &["envoy", "read"]);
    let theirs = theirs.ok();
    assert!(
        theirs.contains("BETA") && !theirs.contains("PONG"),
        "an agent's read: {theirs}"
    );
    assert!(
        std::fs::read_to_string(hub.home.join("envoy/CLAUDE.md"))
            .unwrap()
            .contains("`sleeve spawn <name> --path <dir>"),
        "Envoy's workspace describes the CLI"
    );
}

/// A child gets its own worktree and branch; stopping the root stops the child in the same step.
#[test]
fn root_stop_stops_child() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start("tree");
    let repo = temp_repo("tree");
    let root = hub.spawn_pi("root4", &repo);
    let kid = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "kid4",
            "--parent",
            "root4",
            "--harness",
            "pi_local",
        ])
        .json();
    hub.wait("kid4", "idle", 120);
    assert_eq!(kid["role"].as_str(), Some("child"));
    assert_eq!(kid["view"].as_str(), Some("worktree"));
    assert_eq!(kid["branch"].as_str(), Some("pt/kid4"));
    let path = kid["path"].as_str().unwrap();
    assert_eq!(
        git_out(
            std::path::Path::new(path),
            &["rev-parse", "--abbrev-ref", "HEAD"]
        ),
        "pt/kid4"
    );
    assert!(git_out(&repo, &["worktree", "list"]).contains(path));

    let (rid, kid_id) = (root["id"].as_str().unwrap(), kid["id"].as_str().unwrap());
    let (rscope, kscope) = (hub.scope(rid, 1), hub.scope(kid_id, 1));
    assert!(!scope_pids(&kscope).is_empty());
    hub.cli(&["sleeve", "stop", "root4"]).ok();
    let st = hub.state();
    for name in ["root4", "kid4"] {
        let s = st["sleeves"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
            .unwrap();
        let life = s["lifecycle"].as_str().unwrap();
        assert!(
            matches!(life, "stopping" | "stopped"),
            "{name} is {life} right after the root's stop"
        );
    }
    hub.wait("kid4", "ended", 60);
    hub.wait("root4", "ended", 60);
    assert!(scope_pids(&rscope).is_empty() && scope_pids(&kscope).is_empty());
}

/// Capabilities: a sleeve's own token sees only its tree, an envoy-only token reaches only
/// Envoy, and the loopback listener wants a token.
#[test]
fn capabilities_are_scoped() {
    if !have_runtime() {
        return;
    }
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let hub = Hub::start_with("caps", "pi_local", &["--listen", &listen], &[]);
    let (ra, rb) = (temp_repo("caps-a"), temp_repo("caps-b"));
    let a = hub.spawn_pi("a5", &ra);
    hub.spawn_pi("b5", &rb);

    let token = hub.sleeve_token(a["id"].as_str().unwrap(), 1);
    assert!(token.starts_with("pc1."));
    let as_a = [("PROTECTORATE_TOKEN", token.as_str())];
    let list = hub.cli_env(&as_a, &["--json", "sleeve", "list"]).json();
    let names: Vec<&str> = list["sleeves"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(
        names.contains(&"a5") && !names.contains(&"b5"),
        "a5 sees {names:?}"
    );
    for args in [
        &["sleeve", "stop", "b5"][..],
        &["sleeve", "send", "b5", "hello"],
        &["sleeve", "transcript", "b5"],
        &["sleeve", "wait", "b5", "--timeout", "1"],
        &["envoy", "send", "hello"],
        &["envoy", "read"],
        &["envoy", "wait", "--timeout", "1"],
    ] {
        let r = hub.cli_env(&as_a, args);
        assert_eq!(
            r.status, 4,
            "a5 must not {args:?}: {} {}",
            r.stdout, r.stderr
        );
    }
    hub.cli_env(&as_a, &["sleeve", "transcript", "a5"]).ok();
    assert_eq!(hub.sleeve("b5")["lifecycle"].as_str(), Some("running"));

    // A sleeve's node jobs, maintenance ones included, are not asked about (SPEC 2, threat
    // model): with no fleet they fail at once.
    for extra in [&[][..], &["--maintenance"]] {
        let mut args = vec!["job", "run", "--arch", "amd64"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--", "true"]);
        let r = hub.cli_env(&as_a, &args);
        assert_ne!(r.status, 0, "{} {}", r.stdout, r.stderr);
        assert!(r.stderr.contains("no fleet is configured"), "{}", r.stderr);
    }
    let v = hub.cli(&["--json", "approval", "list"]).json();
    assert!(!v.to_string().contains("node job"), "{v}");

    let envoy_only = hub.printed("envoy-only token: ");
    let as_agent = [("PROTECTORATE_TOKEN", envoy_only.as_str())];
    for args in [
        &["sleeve", "list"][..],
        &["hub", "state"],
        &["sleeve", "stop", "a5"],
        &["sleeve", "transcript", "envoy"],
    ] {
        let r = hub.cli_env(&as_agent, args);
        assert_eq!(r.status, 4, "envoy-only must not {args:?}: {}", r.stderr);
    }
    hub.cli_env(&as_agent, &["envoy", "read"]).ok();
    let bogus = [("PROTECTORATE_TOKEN", "pc1.nope")];
    assert_eq!(hub.cli_env(&bogus, &["envoy", "read"]).status, 4);

    let owner = hub.printed("owner token (shown once): ");
    let curl = |auth: Option<&str>| {
        let mut c = std::process::Command::new("curl");
        c.args([
            "-q",
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-X",
            "POST",
        ]);
        if let Some(t) = auth {
            c.args(["-H", &format!("Authorization: Bearer {t}")]);
        }
        c.arg(format!("http://{listen}/api/GetState"));
        String::from_utf8(c.output().expect("curl").stdout).unwrap()
    };
    if on_path("curl") {
        assert_eq!(curl(None), "401", "loopback needs a token");
        assert_eq!(curl(Some(&owner)), "200");
        assert_eq!(curl(Some(&envoy_only)), "403");
    }
}

/// Forced resleeve end to end, through the test-only `PT_TEST_FAULT` knob: the first turn's
/// settle is reported as a usage limit (G1), the sleeve quiesces, its scope empties, the hub
/// writes the brief (with pre-run history hits from g1's log), picks pi_local from `resleeve_to`,
/// launches g2 and lands the brief prompt.
#[test]
fn forced_resleeve_with_brief() {
    if !have_runtime() {
        return;
    }
    let hub = Hub::start_with(
        "resleeve",
        "pi_local",
        &[],
        &[("PT_TEST_FAULT", "usage_limit:six")],
    );
    let repo = temp_repo("resleeve");
    // pi_vendor on the local provider: the same free model under a different harness, so the
    // resleeve has somewhere to go.
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "six",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "pi_vendor",
            "--provider",
            &pi_provider(),
            "--model",
            &pi_model(),
            "--effort",
            "low",
            "--resleeve-to",
            "pi_local",
        ])
        .json();
    hub.wait("six", "idle", 120);
    assert_eq!(
        hub.sleeve("six")["incarnation"]["effort"].as_str(),
        Some("low"),
        "the requested effort reaches the harness and shows in the view"
    );
    let id = s["id"].as_str().unwrap().to_string();
    hub.send("six", READY);
    let s = eventually("the resleeve to land", Duration::from_secs(240), || {
        let s = hub.sleeve("six");
        let inc = &s["incarnation"];
        let done = s["lifecycle"].as_str() == Some("running")
            && inc["g"].as_u64() == Some(2)
            && inc["harness"].as_str() == Some("pi_local");
        done.then_some(s)
    });
    assert_eq!(s["resleeve_to"].as_array().map(<[_]>::len), Some(1));
    assert_ne!(
        s["incarnation"]["effort"].as_str(),
        Some("low"),
        "a resleeve to another harness starts on that harness's default effort"
    );
    assert!(
        scope_pids(&hub.scope(&id, 1)).is_empty(),
        "g1's scope is empty"
    );

    let brief = std::fs::read_to_string(repo.join(".protectorate/BRIEF.md")).expect("the brief");
    for section in [
        "# Continuation brief",
        "Reason: guardrail G1",
        "## Task",
        READY,
        "## Where it stopped",
        "## Workspace",
        "## Effects that may have been applied",
        "## Child roster",
        // The pre-run search over g1's session, which the index tailed from Pi's own log.
        "## From earlier in this session and others",
        "] assistant, turn 1, session ",
        "**READY**",
        "protectorate history search --sleeve 'six' --mode any ",
        "From its log: 1 turns, context ",
    ] {
        assert!(
            brief.contains(section),
            "the brief has {section:?}:\n{brief}"
        );
    }
    assert_eq!(
        git_out(&repo, &["status", "--porcelain"]),
        "",
        "the brief is git-excluded"
    );

    hub.wait("six", "idle", 180);
    let entries = hub.transcript("six");
    let users: Vec<&str> = of_type(&entries, "user_message")
        .iter()
        .filter_map(|e| e["entry"]["request_id"].as_str())
        .collect();
    assert!(
        users.contains(&"resleeve:six:2"),
        "the brief prompt landed: {users:?}"
    );
    let notices = of_type(&entries, "notice")
        .iter()
        .filter_map(|e| e["entry"]["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(notices.contains("guardrail G1 tripped"), "{notices}");
    assert!(notices.contains("resleeved as g2"), "{notices}");
}

fn write_script(path: &std::path::Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod script");
}

#[test]
fn scratch_survives_a_restart_and_is_removed_with_the_sleeve() {
    let dir = unique("scratch");
    let home = dir.join("home");
    let fake = dir.join("claude");
    std::fs::create_dir_all(home.join("logins/.claude")).unwrap();
    std::fs::write(home.join("logins/.claude/.credentials.json"), "{}").unwrap();
    write_script(
        &fake,
        r#"if [ "$1" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi
if [ -f "$CARGO_TARGET_DIR/probe" ]; then echo kept > "$TMPDIR/seen"; fi
echo build > "$CARGO_TARGET_DIR/probe"
echo temp > "$TMPDIR/probe"
exec sleep 300"#,
    );
    // Scopes are off in this test; satisfy the doctor's user-scope probe in a sleeve with no bus.
    write_script(&dir.join("systemd-run"), "exit 0");
    let config = pt_json::obj! {
        "envoy" => pt_json::obj! { "harness" => "claude" },
        "claude_program" => fake.to_str().unwrap(),
        "scopes" => false,
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
    };
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let mut hub = Hub::open(home, &[], &[("PATH", &path)]);
    let repo = temp_repo("scratch");
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "scratch",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "claude",
        ])
        .json();
    let id = s["id"].as_str().unwrap();
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    eventually("the scratch writes", Duration::from_secs(60), || {
        scratch.join("tmp/probe").exists().then_some(())
    });
    assert_eq!(
        std::fs::read_to_string(scratch.join("target/probe"))
            .unwrap()
            .trim(),
        "build"
    );
    hub.cli(&["sleeve", "stop", "scratch"]).ok();
    hub.wait("scratch", "ended", 60);
    hub.cli(&["sleeve", "start", "scratch"]).ok();
    eventually(
        "the next incarnation to reuse scratch",
        Duration::from_secs(60),
        || scratch.join("tmp/seen").exists().then_some(()),
    );
    hub.cli(&["sleeve", "rm", "scratch"]).ok();
    eventually(
        "removed sleeve scratch to be deleted",
        Duration::from_secs(60),
        || (!scratch.exists()).then_some(()),
    );
    hub.kill9();
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(scratch.join("left-after-crash"), "scratch").unwrap();
    hub.restart();
    eventually(
        "scratch cleanup to be retried after a hub crash",
        Duration::from_secs(20),
        || (!scratch.exists()).then_some(()),
    );
    let fs = std::process::Command::new("stat")
        .args(["-f", "-c", "%t", "/tmp"])
        .output()
        .unwrap();
    if String::from_utf8_lossy(&fs.stdout).trim() == "1021994" {
        let memory_home =
            std::path::PathBuf::from(format!("/tmp/pt-scratch-test-{}/home", std::process::id()));
        std::fs::create_dir_all(memory_home.join("logins/.claude")).unwrap();
        std::fs::write(memory_home.join("logins/.claude/.credentials.json"), "{}").unwrap();
        std::fs::write(memory_home.join("config.json"), config.to_string()).unwrap();
        let memory_hub = Hub::open(memory_home, &[], &[("PATH", &path)]);
        eventually(
            "the tmpfs hub home to be refused",
            Duration::from_secs(60),
            || {
                memory_hub
                    .log_text()
                    .contains("is on a memory filesystem")
                    .then_some(())
            },
        );
    }
}

#[test]
fn sleeve_devices_are_bound_and_missing_ones_are_logged() {
    let dir = unique("devices");
    let home = dir.join("home");
    let fake = dir.join("claude");
    std::fs::create_dir_all(&home).unwrap();
    let node = home.join("devnode");
    std::fs::write(&node, "NPU").unwrap();
    let node = std::fs::canonicalize(&node).unwrap();
    let missing = home.join("no-such-device");
    std::fs::create_dir_all(home.join("logins/.claude")).unwrap();
    std::fs::write(home.join("logins/.claude/.credentials.json"), "{}").unwrap();
    write_script(
        &fake,
        &format!(
            r#"if [ "$1" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi
cat '{}' > "$CARGO_TARGET_DIR/saw"
exec sleep 300"#,
            node.display()
        ),
    );
    write_script(&dir.join("systemd-run"), "exit 0");
    let config = pt_json::obj! {
        "envoy" => pt_json::obj! { "harness" => "claude" },
        "claude_program" => fake.to_str().unwrap(),
        "scopes" => false,
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
        "sleeve_devices" => vec![
            pt_json::Value::from(node.to_str().unwrap()),
            pt_json::Value::from(missing.to_str().unwrap()),
        ],
    };
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let hub = Hub::open(home, &[], &[("PATH", &path)]);
    let repo = temp_repo("devices");
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "devices",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "claude",
        ])
        .json();
    let id = s["id"].as_str().unwrap();
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    let saw = eventually(
        "the bound device to be readable",
        Duration::from_secs(60),
        || std::fs::read_to_string(scratch.join("target/saw")).ok(),
    );
    assert_eq!(saw.trim(), "NPU");
    assert!(
        hub.log_text().contains(&format!(
            "device {} is missing; not bound",
            missing.display()
        )),
        "{}",
        tail(&hub.log_text(), 40)
    );
}

#[test]
fn child_oom_then_harness_exit_does_not_trip_g4() {
    let bus = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output();
    if !bus.is_ok_and(|o| o.status.success()) {
        eprintln!("SKIP: no systemd user bus for scope OOM test");
        return;
    }
    let dir = unique("oom");
    let home = dir.join("home");
    let fake = dir.join("claude");
    std::fs::create_dir_all(home.join("logins/.claude")).unwrap();
    std::fs::write(home.join("logins/.claude/.credentials.json"), "{}").unwrap();
    write_script(
        &fake,
        r#"if [ "$1" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi
g=1
if [ -f "$HOME/g" ]; then g=$(($(cat "$HOME/g") + 1)); fi
echo "$g" > "$HOME/g"
touch "$CARGO_TARGET_DIR/ready$g"
while [ ! -e "$TMPDIR/go$g" ]; do sleep 0.1; done
python3 -c 'a=[]
while True: a.append(bytearray(4*1024*1024))'
touch "$CARGO_TARGET_DIR/child-killed$g"
while [ ! -e "$TMPDIR/exit$g" ]; do sleep 0.1; done
if [ "$g" = 1 ]; then exit 1; fi
exit 2"#,
    );
    let config = pt_json::obj! {
        "envoy" => pt_json::obj! { "harness" => "claude" },
        "claude_program" => fake.to_str().unwrap(),
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
    };
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let hub = Hub::open(home, &[], &[]);
    let repo = temp_repo("oom");
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "oom",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "claude",
        ])
        .json();
    let id = s["id"].as_str().unwrap();
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    for g in 1..=2 {
        eventually("the test harness to start", Duration::from_secs(60), || {
            scratch
                .join(format!("target/ready{g}"))
                .exists()
                .then_some(())
        });
        let scope = hub.scope(id, g);
        let status = std::process::Command::new("systemctl")
            .args([
                "--user",
                "set-property",
                "--runtime",
                &scope,
                "MemoryMax=96M",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "set memory limit on {scope}");
        std::thread::sleep(Duration::from_secs(1));
        std::fs::write(scratch.join(format!("tmp/go{g}")), "go").unwrap();
        eventually(
            "the OOM-killed child to leave the harness running",
            Duration::from_secs(30),
            || {
                scratch
                    .join(format!("target/child-killed{g}"))
                    .exists()
                    .then_some(())
            },
        );
        let out = std::process::Command::new("systemctl")
            .args(["--user", "show", "-p", "ControlGroup", "--value", &scope])
            .output()
            .unwrap();
        let cg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(!cg.is_empty(), "scope disappeared after child OOM");
        let events = std::fs::read_to_string(format!("/sys/fs/cgroup{cg}/memory.events")).unwrap();
        let killed: u64 = events
            .lines()
            .find_map(|line| line.strip_prefix("oom_kill "))
            .unwrap()
            .parse()
            .unwrap();
        assert!(killed > 0, "no OOM kill in {scope}: {events}");
        std::fs::write(scratch.join(format!("tmp/exit{g}")), "exit").unwrap();
    }
    eventually(
        "the harness to resume after both unrelated exits",
        Duration::from_secs(60),
        || scratch.join("target/ready3").exists().then_some(()),
    );
    let entries = hub.transcript("oom");
    let notices = of_type(&entries, "notice")
        .iter()
        .filter_map(|e| e["entry"]["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!notices.contains("guardrail G4"), "{notices}");
    assert_ne!(hub.sleeve("oom")["lifecycle"].as_str(), Some("parked"));
}

#[test]
fn repeated_scope_oom_parks_and_reports_the_memory_limit() {
    let bus = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output();
    if !bus.is_ok_and(|o| o.status.success()) {
        eprintln!("SKIP: no systemd user bus for scope OOM test");
        return;
    }
    let dir = unique("oom-kill");
    let home = dir.join("home");
    let fake = dir.join("claude");
    std::fs::create_dir_all(home.join("logins/.claude")).unwrap();
    std::fs::write(home.join("logins/.claude/.credentials.json"), "{}").unwrap();
    write_script(
        &fake,
        r#"if [ "$1" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi
g=1
if [ -f "$HOME/g" ]; then g=$(($(cat "$HOME/g") + 1)); fi
echo "$g" > "$HOME/g"
touch "$CARGO_TARGET_DIR/ready$g"
while [ ! -e "$TMPDIR/go$g" ]; do sleep 0.1; done
exec python3 -c 'a=[]
while True: a.append(bytearray(4*1024*1024))'"#,
    );
    let config = pt_json::obj! {
        "envoy" => pt_json::obj! { "harness" => "claude" },
        "claude_program" => fake.to_str().unwrap(),
        "quota_poll_secs" => 0u64,
        "harness_update_hours" => 0u64,
    };
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let hub = Hub::open(home, &[], &[]);
    let repo = temp_repo("oom-kill");
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "oom",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "claude",
        ])
        .json();
    let id = s["id"].as_str().unwrap();
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    for g in 1..=2 {
        eventually("the test harness to start", Duration::from_secs(60), || {
            scratch
                .join(format!("target/ready{g}"))
                .exists()
                .then_some(())
        });
        let scope = hub.scope(id, g);
        let status = std::process::Command::new("systemctl")
            .args([
                "--user",
                "set-property",
                "--runtime",
                &scope,
                "MemoryMax=96M",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "set memory limit on {scope}");
        std::thread::sleep(Duration::from_secs(1));
        std::fs::write(scratch.join(format!("tmp/go{g}")), "go").unwrap();
    }
    eventually(
        "the second OOM to park the sleeve",
        Duration::from_secs(60),
        || (hub.sleeve("oom")["lifecycle"].as_str() == Some("parked")).then_some(()),
    );
    eventually("both OOM notices", Duration::from_secs(10), || {
        let entries = hub.transcript("oom");
        let notices = of_type(&entries, "notice")
            .iter()
            .filter_map(|e| e["entry"]["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (notices
            .matches("guardrail G4 tripped: the memory limit")
            .count()
            == 2)
            .then_some(())
    });
}

/// A sleeve controls its repo's config and `.gitattributes` (a child, the shared git dir): the
/// hub's checkpoint, worktree add and brief facts must run none of its filters, hooks,
/// fsmonitor or textconv. A child path through a symlink never reaches another worktree.
#[test]
fn workspace_git_config_runs_nothing() {
    if !have_runtime() {
        return;
    }
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let hub = Hub::start_with("gitcfg", "pi_local", &["--listen", &listen], &[]);
    let repo = temp_repo("gitcfg");
    let traps = unique("gitcfg-traps");
    std::fs::create_dir_all(traps.join("hooks")).unwrap();
    let marker = |what: &str| traps.join(format!("ran-{what}"));
    for (name, what) in [
        ("clean", "clean"),
        ("smudge", "smudge"),
        ("fsmonitor", "fsmonitor"),
        ("textconv", "textconv"),
        ("hooks/post-checkout", "post-checkout"),
        ("hooks/reference-transaction", "reference-transaction"),
    ] {
        let tail = if matches!(what, "clean" | "smudge") {
            "cat"
        } else {
            "exit 0"
        };
        write_script(
            &traps.join(name),
            &format!("touch '{}'\n{tail}", marker(what).display()),
        );
    }
    std::fs::write(repo.join(".gitattributes"), "* filter=evil diff=evil\n").unwrap();
    assert!(git(&repo, &["add", ".gitattributes"]).status.success());
    assert!(
        git(&repo, &["commit", "-qm", "attributes"])
            .status
            .success()
    );
    let t = |n: &str| traps.join(n).to_string_lossy().into_owned();
    for (k, v) in [
        ("filter.evil.clean", t("clean")),
        ("filter.evil.smudge", t("smudge")),
        ("filter.evil.required", "true".into()),
        ("diff.evil.textconv", t("textconv")),
        ("core.fsmonitor", t("fsmonitor")),
        ("core.hooksPath", t("hooks")),
    ] {
        assert!(
            git(&repo, &["config", k, &v]).status.success(),
            "config {k}"
        );
    }
    std::fs::write(repo.join("notes.txt"), "changed by the sleeve\n").unwrap();

    let root = hub.spawn_pi("gitroot", &repo);
    let id = root["id"].as_str().unwrap().to_string();
    hub.send("gitroot", READY);
    hub.wait("gitroot", "idle", 180);
    let checkpoints = hub.home.join(format!("sleeves/{id}/checkpoints.jsonl"));
    let line = eventually(
        "a checkpoint after the turn",
        Duration::from_secs(30),
        || {
            std::fs::read_to_string(&checkpoints)
                .ok()
                .filter(|t| t.contains("\"g\":1"))
        },
    );
    let commit = pt_json::parse(line.lines().last().unwrap()).unwrap()["commit"]
        .as_str()
        .unwrap()
        .to_string();
    let objects = hub.home.join(format!("sleeves/{id}/objects"));
    let show = std::process::Command::new("git")
        .args([
            "-C",
            repo.to_str().unwrap(),
            "show",
            &format!("{commit}:notes.txt"),
        ])
        .env("GIT_OBJECT_DIRECTORY", &objects)
        .env(
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            repo.join(".git/objects"),
        )
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&show.stdout),
        "changed by the sleeve\n",
        "the checkpoint holds the work tree's bytes"
    );

    let kid = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "gitkid",
            "--parent",
            "gitroot",
            "--harness",
            "pi_local",
        ])
        .json();
    hub.wait("gitkid", "idle", 120);
    let kid_path = std::path::PathBuf::from(kid["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(kid_path.join("notes.txt")).unwrap(),
        "The secret word is PELICAN.\n",
        "the child's worktree is checked out"
    );
    assert_eq!(
        git_out(&kid_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "pt/gitkid"
    );

    // A second child whose path is a symlink to the first one's worktree.
    let links = unique("gitcfg-links");
    std::fs::create_dir_all(&links).unwrap();
    let link = links.join("kid2");
    std::os::unix::fs::symlink(&kid_path, &link).unwrap();
    // Over the API: the CLI would resolve the link itself.
    let owner = hub.printed("owner token (shown once): ");
    let body = pt_json::obj! {
        "name" => "gitkid2",
        "harness" => "pi_local",
        "parent_id" => "gitroot",
        "path" => link.to_str().unwrap(),
    };
    let (status, v) = post(port, "SpawnSleeve", Some(&owner), &body.to_string());
    assert_eq!(status, 400, "a symlinked child path is refused: {v}");
    assert_eq!(hub.sleeve("gitkid2"), pt_json::Value::Null);

    for what in [
        "clean",
        "smudge",
        "fsmonitor",
        "textconv",
        "post-checkout",
        "reference-transaction",
    ] {
        assert!(
            !marker(what).exists(),
            "the hub ran the workspace's {what} with its own authority"
        );
    }
}

/// A child's `git worktree add` cut short by a hub crash: the worktree is registered but has no
/// files. Recovery must not take the directory for a finished provision; it completes it.
#[test]
fn cut_short_worktree_is_finished() {
    if !have_runtime() {
        return;
    }
    let real_git = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real_git = String::from_utf8_lossy(&real_git.stdout).trim().to_string();
    let wrap = unique("cutshort-bin");
    std::fs::create_dir_all(&wrap).unwrap();
    let once = wrap.join("hung");
    // The first `worktree add` registers the worktree without its files, then hangs as if the
    // hub died mid-checkout.
    write_script(
        &wrap.join("git"),
        &format!(
            r#"case " $* " in
  *" worktree add "*)
    if [ ! -e '{once}' ]; then
      touch '{once}'
      for a in "$@"; do
        shift
        set -- "$@" "$a"
        [ "$a" = add ] && set -- "$@" --no-checkout
      done
      '{real_git}' "$@"
      exec sleep 600
    fi;;
esac
exec '{real_git}' "$@""#,
            once = once.display()
        ),
    );
    let path = format!(
        "{}:{}",
        wrap.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut hub = Hub::start_with("cutshort", "pi_local", &[], &[("PATH", path.as_str())]);
    let repo = temp_repo("cutshort");
    hub.spawn_pi("cutroot", &repo);
    hub.cli(&[
        "sleeve",
        "spawn",
        "cutkid",
        "--parent",
        "cutroot",
        "--harness",
        "pi_local",
    ])
    .ok();
    let kid_path = eventually(
        "the worktree to be registered",
        Duration::from_secs(30),
        || {
            let p = hub.sleeve("cutkid")["path"].as_str()?.to_string();
            git_out(&repo, &["worktree", "list"])
                .contains(&p)
                .then_some(p)
        },
    );
    let kid_path = std::path::PathBuf::from(kid_path);
    assert!(once.exists(), "the add was cut short");
    assert!(!kid_path.join("notes.txt").exists());
    hub.restart();
    hub.wait("cutkid", "idle", 120);
    assert_eq!(
        std::fs::read_to_string(kid_path.join("notes.txt")).unwrap_or_default(),
        "The secret word is PELICAN.\n",
        "recovery finished the checkout"
    );
    assert_eq!(
        git_out(&kid_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "pt/cutkid"
    );
    assert_eq!(git_out(&kid_path, &["status", "--porcelain"]), "");
}

/// What a sleeve process can reach, probed from inside a real sandbox by a stand-in `pi` that
/// then runs the real one: not the owner's socket, nothing on the sleeves' socket without its
/// capability (even with scopes off, where no cgroup names it), no write to the hub's `run/`,
/// its own harness's shared hub login and no other, and never the owner's own logins. A token
/// a sleeve refreshes lands in the shared login. A harness program under a masked dir runs.
#[test]
fn sandbox_reaches_only_its_own() {
    if !have_runtime() {
        return;
    }
    let real_pi = std::process::Command::new("sh")
        .args(["-c", "command -v pi"])
        .output()
        .unwrap();
    let real_pi = String::from_utf8_lossy(&real_pi.stdout).trim().to_string();
    let dir = unique("sbx");
    let owner = dir.join("owner");
    let real_home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    for rel in [
        ".pi/agent/models.json",
        ".pi/agent/settings.json",
        ".pi/agent/auth.json",
    ] {
        if real_home.join(rel).is_file() {
            std::fs::create_dir_all(owner.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(real_home.join(rel), owner.join(rel)).unwrap();
        }
    }
    std::fs::create_dir_all(owner.join(".claude")).unwrap();
    std::fs::write(owner.join(".claude/.credentials.json"), "OWNER-CLAUDE").unwrap();
    // The owner's toolchain dirs and git identity, which a sleeve's private home must not lose.
    std::fs::create_dir_all(owner.join(".cargo")).unwrap();
    std::fs::create_dir_all(owner.join(".rustup")).unwrap();
    std::fs::write(
        owner.join(".gitconfig"),
        "[user]\n\tname = \"Owner \\\"Q\\\" Name\"\n\temail = owner@example.test\n",
    )
    .unwrap();
    let (repo, other) = (temp_repo("sbx"), temp_repo("sbx-other"));
    // Stands in for a Claude Code that reads its login, refreshes it and fails.
    let fake_claude = dir.join("claude");
    write_script(
        &fake_claude,
        &format!(
            r#"if [ "$1" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi
logins="${{PROTECTORATE_SOCKET%%/run/*}}/logins"
[ -e "$HOME/claude-probe.txt" ] || {{
  cat "$CLAUDE_SECURESTORAGE_CONFIG_DIR/.credentials.json" 2>/dev/null; echo
  [ -e '{owner}/.claude/.credentials.json' ] && echo owner_login=visible || echo owner_login=hidden
  [ -r "$logins/.pi/agent/models.json" ] && echo pi_login=visible || echo pi_login=hidden
  echo "codex_login=$(cat "$HOME/.codex/auth.json" 2>/dev/null)"
}} > "$HOME/claude-probe.txt"
printf REFRESHED > "$CLAUDE_SECURESTORAGE_CONFIG_DIR/.credentials.json"
exit 1"#,
            owner = owner.display()
        ),
    );
    // The stand-in lives under a masked dir behind a link, as grok does (~/.grok/downloads).
    let fake = dir.join("pi");
    let real_fake = owner.join(".grok/downloads/pi");
    std::fs::create_dir_all(real_fake.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real_fake, &fake).unwrap();
    write_script(
        &real_fake,
        &format!(
            r#"if [ "$1" != --version ]; then
  run="${{PROTECTORATE_SOCKET%%/run/*}}/run"
  {{
    [ -S "$run/api.sock" ] && echo owner_socket=visible || echo owner_socket=hidden
    env -u PROTECTORATE_TOKEN protectorate --json sleeve list >/dev/null 2>&1 && echo tokenless=ok || echo tokenless=refused
    protectorate --json sleeve list >/dev/null 2>&1 && echo token=ok || echo token=refused
    touch "$run/bin/probe" 2>/dev/null && echo bin=writable || echo bin=readonly
    touch "$run/sleeves/probe" 2>/dev/null && echo sleeves=writable || echo sleeves=readonly
    [ -e "$HOME/.claude/.credentials.json" ] && echo claude_login=visible || echo claude_login=hidden
    [ -e "${{run%/run}}/logins/.claude/.credentials.json" ] && echo hub_claude=visible || echo hub_claude=hidden
    echo "codex_login=$(cat "$HOME/.codex/auth.json" 2>/dev/null)"
    echo "claude_store=$CLAUDE_SECURESTORAGE_CONFIG_DIR"
    bash -lc 'command -v protectorate' >/dev/null 2>&1 && echo login_shell_cli=ok || echo login_shell_cli=lost
    echo "login_shell_cargo_home=$(bash -lc 'echo $CARGO_HOME')"
    echo "login_shell_rustup_home=$(bash -lc 'echo $RUSTUP_HOME')"
    echo "target=$CARGO_TARGET_DIR tmp=$TMPDIR"
    echo build > "$CARGO_TARGET_DIR/probe"
    echo temp > "$TMPDIR/probe"
    [ -L "$HOME/.pi/agent/models.json" ] && [ -r "$HOME/.pi/agent/models.json" ] && echo pi_login=shared || echo pi_login=missing
    [ -r '{other}/notes.txt' ] && echo other_root=visible || echo other_root=hidden
    git commit -q --allow-empty -m "probe $PROTECTORATE_SLEEVE" >/dev/null 2>&1 \
      && echo commit=ok || echo commit=failed
    echo "committer=$(git log -1 --format='%cn <%ce>')"
  }} < /dev/null > "$HOME/probe.txt"
fi
exec '{real_pi}' "$@""#,
            other = other.display()
        ),
    );
    let hub = Hub::start_config(
        "sbx",
        "pi_local",
        &[
            ("scopes", pt_json::Value::from(false)),
            ("pi_program", fake.to_str().unwrap().into()),
            ("claude_program", fake_claude.to_str().unwrap().into()),
        ],
        &[],
        &[
            ("HOME", owner.to_str().unwrap()),
            ("CARGO_HOME", owner.join(".cargo").to_str().unwrap()),
            ("RUSTUP_HOME", owner.join(".rustup").to_str().unwrap()),
        ],
    );
    std::fs::create_dir_all(hub.home.join("logins/.claude")).unwrap();
    std::fs::write(
        hub.home.join("logins/.claude/.credentials.json"),
        "HUB-CLAUDE",
    )
    .unwrap();
    std::fs::create_dir_all(hub.home.join("logins/.codex")).unwrap();
    std::fs::write(hub.home.join("logins/.codex/auth.json"), "HUB-CODEX").unwrap();
    hub.cli(&["doctor", "--refresh"]).ok();
    hub.spawn_pi("other", &other);
    let s = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "sbx",
            "--path",
            repo.to_str().unwrap(),
            "--harness",
            "pi_local",
            "--resleeve-to",
            "claude",
        ])
        .json();
    hub.wait("sbx", "idle", 120);
    let id = s["id"].as_str().unwrap().to_string();
    let home = hub.home.join(format!("sleeves/{id}/home"));
    let probe = std::fs::read_to_string(home.join("probe.txt")).expect("the probe ran");
    for want in [
        "owner_socket=hidden",
        "tokenless=refused",
        "token=ok",
        "bin=readonly",
        "sleeves=readonly",
        "claude_login=hidden",
        "hub_claude=visible",
        "codex_login=HUB-CODEX",
        "pi_login=shared",
        "login_shell_cli=ok",
        "target=/tmp/pt/target tmp=/tmp/pt/tmp",
        "other_root=hidden",
        "commit=ok",
        "committer=Owner \"Q\" Name <owner@example.test>",
    ] {
        assert!(probe.lines().any(|l| l == want), "want {want}:\n{probe}");
    }
    let scratch = hub.home.join(format!("sleeves/{id}/scratch"));
    assert_eq!(
        std::fs::read_to_string(scratch.join("target/probe"))
            .unwrap()
            .trim(),
        "build"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.join("tmp/probe"))
            .unwrap()
            .trim(),
        "temp"
    );
    // A worktree child sees its parent's git dirs, not the other root, and commits.
    let kid = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "sbxkid",
            "--parent",
            "sbx",
            "--harness",
            "pi_local",
        ])
        .json();
    hub.wait("sbxkid", "idle", 120);
    let kid_id = kid["id"].as_str().unwrap();
    let kid_probe =
        std::fs::read_to_string(hub.home.join(format!("sleeves/{kid_id}/home/probe.txt")))
            .expect("the child's probe ran");
    for want in [
        "other_root=hidden",
        "commit=ok",
        "committer=Owner \"Q\" Name <owner@example.test>",
    ] {
        assert!(
            kid_probe.lines().any(|l| l == want),
            "want {want}:\n{kid_probe}"
        );
    }
    assert_eq!(
        git_out(&repo, &["log", "-1", "--format=%s", "pt/sbxkid"]),
        "probe sbxkid",
        "the child's commit is on its branch"
    );
    assert!(
        !home.join(".claude/.credentials.json").exists(),
        "Claude's login is reached through its store dir, not a file in the home"
    );
    for (key, dir) in [("cargo_home", ".cargo"), ("rustup_home", ".rustup")] {
        let want = format!("login_shell_{key}={}", owner.join(dir).display());
        assert!(probe.lines().any(|l| l == want), "want {want}:\n{probe}");
    }
    let store = format!("claude_store={}", hub.home.join("logins/.claude").display());
    assert!(probe.lines().any(|l| l == store), "want {store}:\n{probe}");

    // A file an earlier hub left where a shared login belongs is replaced by the link at the
    // next launch (after a turn, so the native session exists to resume).
    hub.send("sbx", READY);
    hub.wait("sbx", "idle", 180);
    hub.cli(&["sleeve", "stop", "sbx"]).ok();
    hub.wait("sbx", "ended", 60);
    let models = home.join(".pi/agent/models.json");
    std::fs::remove_file(&models).unwrap();
    std::fs::write(&models, "OLD-COPY").unwrap();
    hub.cli(&["sleeve", "start", "sbx"]).ok();
    hub.wait("sbx", "idle", 120);
    assert_eq!(
        std::fs::read_link(&models).ok(),
        Some(hub.home.join("logins/.pi/agent/models.json")),
        "the copy became the shared login again"
    );

    // A Claude sleeve reads the hub's login, not the owner's, reaches the other harnesses' shared
    // logins too, and its refresh lands in the one shared login.
    let c = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "sbxclaude",
            "--path",
            temp_repo("sbx-claude").to_str().unwrap(),
            "--harness",
            "claude",
        ])
        .json();
    let c_id = c["id"]
        .as_str()
        .expect("the claude sleeve spawned")
        .to_string();
    let c_probe = hub
        .home
        .join(format!("sleeves/{c_id}/home/claude-probe.txt"));
    let c_probe = eventually("the claude probe", Duration::from_secs(60), || {
        std::fs::read_to_string(&c_probe).ok()
    });
    for want in [
        "HUB-CLAUDE",
        "owner_login=hidden",
        "pi_login=visible",
        "codex_login=HUB-CODEX",
    ] {
        assert!(
            c_probe.lines().any(|l| l == want),
            "want {want}:\n{c_probe}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(hub.home.join("logins/.claude/.credentials.json")).unwrap(),
        "REFRESHED"
    );
    assert_eq!(
        std::fs::read_to_string(owner.join(".claude/.credentials.json")).unwrap(),
        "OWNER-CLAUDE"
    );
}

/// A home too deep for a unix socket address (`sun_path`, 107 bytes) still serves the owner's
/// socket and the sleeves' one.
#[test]
fn deep_home_serves_its_sockets() {
    if !have_runtime() {
        return;
    }
    let real_pi = std::process::Command::new("sh")
        .args(["-c", "command -v pi"])
        .output()
        .unwrap();
    let real_pi = String::from_utf8_lossy(&real_pi.stdout).trim().to_string();
    let dir = unique("deep");
    std::fs::create_dir_all(&dir).unwrap();
    let fake = dir.join("pi");
    write_script(
        &fake,
        &format!(
            r#"if [ "$1" != --version ]; then
  protectorate --json sleeve list >/dev/null 2>&1 && echo sleeve_socket=ok || echo sleeve_socket=failed
  echo "socket_len=${{#PROTECTORATE_SOCKET}}"
fi > "$HOME/probe.txt"
exec '{real_pi}' "$@""#
        ),
    );
    let name = format!("deep-{}", "d".repeat(80));
    let hub = Hub::start_config(
        &name,
        "pi_local",
        &[("pi_program", fake.to_str().unwrap().into())],
        &[],
        &[],
    );
    assert!(
        hub.home.join("run/sleeves/api.sock").as_os_str().len() > 107,
        "the test home is deep enough"
    );
    hub.wait("envoy", "idle", 120);
    hub.cli(&["--json", "sleeve", "list"]).json();
    let id = hub.sleeve("envoy")["id"].as_str().unwrap().to_string();
    let probe =
        std::fs::read_to_string(hub.home.join(format!("sleeves/{id}/home/probe.txt"))).unwrap();
    assert!(probe.lines().any(|l| l == "sleeve_socket=ok"), "{probe}");
}

/// A new project root is created only under `projects_dir`, as an empty repo a child can branch
/// from. An overlap names the claim, and a path the hub cannot see says so. The stand-in claude
/// is the one the doctor tests use: `--version` succeeds, and it stays up so a sleeve can idle.
#[test]
fn new_project_root() {
    for tool in ["bwrap", "systemd-run", "git"] {
        if !on_path(tool) {
            eprintln!("SKIP: {tool} is not installed");
            return;
        }
    }
    let fakes = unique("claude-fake");
    std::fs::create_dir_all(&fakes).unwrap();
    let claude = fakes.join("claude");
    write_script(
        &claude,
        "if [ \"$1\" = --version ]; then echo '2.0.0 (Claude Code)'; exit 0; fi\nsleep 3600",
    );
    let program = claude.to_str().unwrap();
    {
        let off = Hub::start_config(
            "noproj",
            "claude",
            &[("claude_program", program.into())],
            &[],
            &[],
        );
        claude_ready(&off);
        let r = off.cli(&[
            "sleeve",
            "spawn",
            "x",
            "--new-path",
            "x",
            "--harness",
            "claude",
        ]);
        assert_ne!(r.status, 0, "{}", r.stderr);
        assert!(r.stderr.contains("projects_dir is not set"), "{}", r.stderr);
    }
    let projects = unique("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let projects = std::fs::canonicalize(&projects).unwrap();
    let proj = projects.to_str().unwrap();
    let ident_dir = unique("git-ident");
    std::fs::create_dir_all(&ident_dir).unwrap();
    let ident = ident_dir.join("gitconfig");
    std::fs::write(
        &ident,
        "[user]\n\tname = Project Owner\n\temail = project-owner@example.test\n",
    )
    .unwrap();
    let hub = Hub::start_config(
        "newproj",
        "claude",
        &[
            ("projects_dir", proj.into()),
            ("claude_program", program.into()),
        ],
        &[],
        &[("GIT_CONFIG_GLOBAL", ident.to_str().unwrap())],
    );
    claude_ready(&hub);
    let both = hub.cli(&[
        "sleeve",
        "spawn",
        "x",
        "--path",
        proj,
        "--new-path",
        "x",
        "--harness",
        "claude",
    ]);
    assert_eq!(both.status, 2, "{}", both.stderr);
    assert!(
        both.stderr.contains("mutually exclusive"),
        "{}",
        both.stderr
    );

    let missing = projects.join("no-such-dir");
    let gone = hub.cli(&[
        "sleeve",
        "spawn",
        "gone",
        "--path",
        missing.to_str().unwrap(),
        "--harness",
        "claude",
    ]);
    assert!(
        gone.stderr
            .contains(&format!("the hub cannot see {}", missing.display())),
        "{}",
        gone.stderr
    );

    let v = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "voice",
            "--new-path",
            "voice",
            "--harness",
            "claude",
        ])
        .json();
    hub.wait("voice", "idle", 120);
    let path = projects.join("voice");
    assert_eq!(v["path"].as_str(), Some(path.to_str().unwrap()));
    assert_eq!(
        git_out(&path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "main"
    );
    assert_eq!(
        git_out(&path, &["log", "-1", "--format=%s"]),
        "Initial commit"
    );
    assert_eq!(
        git_out(&path, &["log", "-1", "--format=%an <%ae>"]),
        "Project Owner <project-owner@example.test>"
    );
    assert!(git_out(&path, &["ls-tree", "HEAD"]).is_empty());

    // A name already in use is refused before the directory is created. The live root stays.
    let taken = hub.cli(&[
        "sleeve",
        "spawn",
        "voice",
        "--new-path",
        "voice-again",
        "--harness",
        "claude",
    ]);
    assert_ne!(taken.status, 0, "{}", taken.stderr);
    assert!(
        taken.stderr.contains("a sleeve has that name"),
        "{}",
        taken.stderr
    );
    assert!(!projects.join("voice-again").exists(), "{}", taken.stderr);
    assert_eq!(
        git_out(&path, &["log", "-1", "--format=%s"]),
        "Initial commit"
    );

    let kid = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "voice-kid",
            "--parent",
            "voice",
            "--harness",
            "claude",
        ])
        .json();
    hub.wait("voice-kid", "idle", 120);
    assert_eq!(kid["branch"].as_str(), Some("pt/voice-kid"));
    let kid_path = std::path::PathBuf::from(kid["path"].as_str().unwrap());
    assert_eq!(
        git_out(&kid_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "pt/voice-kid"
    );

    let again = hub.cli(&[
        "sleeve",
        "spawn",
        "voice2",
        "--path",
        path.to_str().unwrap(),
        "--harness",
        "claude",
    ]);
    assert!(again.stderr.contains("409"), "{}", again.stderr);
    assert!(
        again.stderr.contains("overlaps sleeve voice")
            && again.stderr.contains(path.to_str().unwrap()),
        "{}",
        again.stderr
    );

    let exists = hub.cli(&[
        "sleeve",
        "spawn",
        "voice3",
        "--new-path",
        "voice",
        "--harness",
        "claude",
    ]);
    assert!(
        exists.stderr.contains("overlaps sleeve voice")
            && exists.stderr.contains(path.to_str().unwrap()),
        "{}",
        exists.stderr
    );

    let unborn = projects.join("unborn");
    std::fs::create_dir(&unborn).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&unborn)
            .status()
            .unwrap()
            .success()
    );
    let hint = hub.cli(&[
        "sleeve",
        "spawn",
        "unborn",
        "--new-path",
        "unborn",
        "--harness",
        "claude",
    ]);
    assert!(hint.stderr.contains("has no commit"), "{}", hint.stderr);
    assert!(unborn.join(".git").is_dir());

    let slashed = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "slashed",
            "--new-path",
            "slashed/",
            "--harness",
            "claude",
        ])
        .json();
    assert_eq!(
        slashed["path"].as_str(),
        Some(projects.join("slashed").to_str().unwrap())
    );

    let nested = hub.cli(&[
        "sleeve",
        "spawn",
        "nested",
        "--new-path",
        "voice/nested",
        "--harness",
        "claude",
    ]);
    assert!(
        nested.stderr.contains("overlaps sleeve voice"),
        "{}",
        nested.stderr
    );
    assert!(!path.join("nested").exists());

    let escaped = projects
        .parent()
        .unwrap()
        .join(format!("escaped-{}", std::process::id()));
    let trick_rel = format!("../{}", escaped.file_name().unwrap().to_str().unwrap());
    let trick = hub.cli(&[
        "sleeve",
        "spawn",
        "trick",
        "--new-path",
        &trick_rel,
        "--harness",
        "claude",
    ]);
    assert_ne!(trick.status, 0, "{}", trick.stderr);
    assert!(!escaped.exists(), "a .. path was created: {}", trick.stderr);

    let elsewhere = unique("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, projects.join("link")).unwrap();
    let sym = hub.cli(&[
        "sleeve",
        "spawn",
        "sym",
        "--new-path",
        "link/evil",
        "--harness",
        "claude",
    ]);
    assert!(sym.stderr.contains("symlink"), "{}", sym.stderr);
    assert!(!elsewhere.join("evil").exists());

    let outside = hub.cli(&[
        "sleeve",
        "spawn",
        "out",
        "--new-path",
        "/etc/protectorate-not-a-project",
        "--harness",
        "claude",
    ]);
    assert!(
        outside.stderr.contains("outside projects_dir"),
        "{}",
        outside.stderr
    );
    assert!(!std::path::Path::new("/etc/protectorate-not-a-project").exists());

    hub.wait("envoy", "idle", 120);
    let envoy = hub.sleeve("envoy");
    let envoy_id = envoy["id"].as_str().unwrap();
    let g = envoy["incarnation"]["g"].as_u64().unwrap();
    let token = hub.sleeve_token(envoy_id, g);
    let as_envoy = [("PROTECTORATE_TOKEN", token.as_str())];
    let by_envoy = hub
        .cli_env(
            &as_envoy,
            &[
                "--json",
                "sleeve",
                "spawn",
                "envoy-made",
                "--new-path",
                "envoy-made",
                "--harness",
                "claude",
            ],
        )
        .json();
    assert_eq!(
        by_envoy["path"].as_str(),
        Some(projects.join("envoy-made").to_str().unwrap())
    );

    let voice_id = v["id"].as_str().unwrap();
    let voice_g = hub.sleeve("voice")["incarnation"]["g"].as_u64().unwrap();
    let sleeve_token = hub.sleeve_token(voice_id, voice_g);
    let as_sleeve = [("PROTECTORATE_TOKEN", sleeve_token.as_str())];
    let sneaky = hub.cli_env(
        &as_sleeve,
        &[
            "sleeve",
            "spawn",
            "sneak",
            "--new-path",
            "sneak",
            "--harness",
            "claude",
        ],
    );
    assert_eq!(sneaky.status, 4, "{}", sneaky.stderr);
    assert!(
        sneaky
            .stderr
            .contains("only the owner, a paired device or Envoy"),
        "{}",
        sneaky.stderr
    );
    assert!(!projects.join("sneak").exists());
}

fn claude_ready(hub: &Hub) {
    let dir = hub.home.join("logins/.claude");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".credentials.json"), "{}\n").unwrap();
    let d = hub.cli(&["--json", "doctor", "--refresh"]).json();
    let ok = d["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["name"].as_str() == Some("claude") && h["eligible"].as_bool() == Some(true));
    assert!(ok, "claude ineligible: {d}");
}

/// The pid of the `serve` running on `home`.
fn serve_pid(home: &std::path::Path) -> u32 {
    let want = format!("--home\0{}\0serve\0", home.display());
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .find(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .is_ok_and(|c| String::from_utf8_lossy(&c).contains(&want))
        })
        .expect("the hub's serve process")
}
