//! `serve` and `rpc` against each other: the real binary on real sockets and processes. Presence
//! is forced with PROTECTORATE_NODE_TEST_PRESENCE (a file holding PRESENT, AWAY or UNKNOWN), so
//! the job gate and the mid-job watcher run the same on a desk or a headless box. Linux only:
//! the Windows path is checked by hand on a Windows machine (REVISIT).
#![cfg(target_os = "linux")]

mod common;

use common::{Agent, alive, check_inventory, check_presence_value, check_sample, num, text};
use std::time::Duration;

const SECS: Duration = Duration::from_secs(1);

#[test]
fn a_peer_without_the_hub_tag_is_refused() {
    let a = Agent::start("tag", &[]);
    for header in [
        r#"{"peer_name":"laptop","tags":["tag:node"],"ip":"100.64.0.9"}"#,
        r#"{"peer_name":"hub"}"#,
        r#"{"tags":"tag:hub"}"#,
        "not json",
    ] {
        let out = a.rpc_with(&["--header", header], "Hello", "{}");
        assert_eq!(out.status.code(), Some(3), "header {header} was served");
        assert!(out.stdout.is_empty(), "header {header} got an answer");
    }
    std::fs::write(a.dir.join("header.json"), r#"{"tags":["tag:node"]}"#).unwrap();
    let out = a.rpc_with(&["--header", "@header.json"], "Hello", "{}");
    assert_eq!(
        out.status.code(),
        Some(3),
        "a header from a file was served"
    );
    assert!(a.log().contains("not tag:hub"), "{}", a.log());
    std::fs::write(a.dir.join("fields.json"), r#"{"hub_version":"test"}"#).unwrap();
    assert_eq!(
        a.rpc_with(&[], "Hello", "@fields.json").status.code(),
        Some(0)
    );
    let hello = a.ok("Hello", "{}");
    assert_eq!(num(&hello, "protocol"), 2);
    let version = std::process::Command::new(common::BIN)
        .arg("--version")
        .output()
        .unwrap();
    let version = String::from_utf8_lossy(&version.stdout);
    let first = version.lines().next().unwrap_or("");
    assert_eq!(
        Some(text(&hello, "node_version")),
        first.strip_prefix("protectorate-node "),
        "Hello reports the build's version"
    );
    assert_eq!(text(&hello, "node_id").len(), 32);
    assert_eq!(text(&hello, "journal_epoch").len(), 32);
}

#[test]
fn inventory_sample_and_presence_answer_over_tcp() {
    let a = Agent::start_tcp("collect", &[]);
    assert!(a.connect.starts_with("127.0.0.1:"), "{}", a.connect);
    check_inventory(&a.ok("Inventory", "{}"));
    check_sample(&a.ok("Sample", "{}"));
    let p = a.ok("Presence", "{}");
    check_presence_value(&p, 600);
    assert_eq!(text(&p, "state"), "AWAY");
    assert_eq!(text(&p, "source"), "test-override");
    a.set_presence("PRESENT");
    assert_eq!(text(&a.ok("Presence", "{}"), "state"), "PRESENT");
    // An agent not started as <state>/releases/current/protectorate-node cannot switch.
    let e = a.err(
        "Update",
        r#"{"version":"v1","api":"https://x","repo":"o/r"}"#,
    );
    assert_eq!(text(&e, "code"), "unsupported");
    assert!(text(&e, "message").contains("releases/current"), "{e}");
}

#[test]
fn a_job_runs_and_its_log_is_fetched_by_offset() {
    let a = Agent::start("run", &[]);
    let submit = r#"{"job_id":"echo","attempt":1,"script":"echo hi; sleep 1; echo bye"}"#;
    let j = a.submit(submit);
    assert_eq!(text(&j, "state"), "running");
    let pid = num(&j, "pid");
    std::thread::sleep(Duration::from_millis(400));
    let (early, end) = a.fetch("echo", 1, 0, 1 << 20);
    assert_eq!(early, b"hi\n");
    assert_eq!(end["done"].as_bool(), Some(false), "{end}");

    let j = a.wait_state("echo", 1, &["succeeded"], 10 * SECS);
    assert_eq!(j["exit_code"].as_i64(), Some(0));
    assert_eq!(num(&j, "log_bytes"), 7);
    let (head, end) = a.fetch("echo", 1, 0, 2);
    assert_eq!(head, b"hi");
    assert_eq!((num(&end, "offset"), num(&end, "size")), (2, 7));
    assert_eq!(end["done"].as_bool(), Some(false));
    let (tail, end) = a.fetch("echo", 1, num(&end, "offset"), 1 << 20);
    assert_eq!(tail, b"\nbye\n");
    assert_eq!(end["done"].as_bool(), Some(true));
    let (past, end) = a.fetch("echo", 1, 100, 10);
    assert!(past.is_empty());
    assert_eq!(num(&end, "offset"), 7);

    // The same (job, attempt) again is answered from the record, never run twice.
    let again = a.submit(submit);
    assert_eq!(text(&again, "state"), "succeeded");
    assert_eq!(num(&again, "pid"), pid);
    assert_eq!(a.journal().matches(r#""type":"start""#).count(), 1);

    let argv = a.submit(r#"{"job_id":"argv","attempt":1,"argv":["sh","-c","printf %s \"$GREETING\"; exit 3"],"env":{"GREETING":"hello"}}"#);
    assert_eq!(text(&argv, "state"), "running");
    let j = a.wait_state("argv", 1, &["failed"], 10 * SECS);
    assert_eq!(j["exit_code"].as_i64(), Some(3));
    assert_eq!(a.fetch("argv", 1, 0, 100).0, b"hello");
}

#[test]
fn a_job_past_its_timeout_is_killed() {
    let a = Agent::start("timeout", &[]);
    // TERM is ignored by the shell and the sleep, so only the SIGKILL after the grace ends it.
    let j = a.submit(r#"{"job_id":"slow","attempt":1,"script":"trap '' TERM; echo start; sleep 20","timeout_secs":1}"#);
    let pid = num(&j, "pid");
    let j = a.wait_state("slow", 1, &["failed"], 10 * SECS);
    assert_eq!(j["exit_code"].as_i64(), Some(124), "{j}");
    assert!(text(&j, "reason").contains("timed out"), "{j}");
    assert!(!alive(pid), "the job's leader still runs");
    assert_eq!(a.fetch("slow", 1, 0, 100).0, b"start\n");
}

#[test]
fn a_cancelled_job_stops() {
    let a = Agent::start("cancel", &["--no-scope"]);
    let j = a.submit(r#"{"job_id":"c","attempt":1,"script":"sleep 20"}"#);
    assert_eq!(text(&j, "isolation"), "process-group");
    let pid = num(&j, "pid");
    let c = a.ok("CancelJob", r#"{"job_id":"c","attempt":1}"#)["job"].clone();
    assert!(
        ["cancelling", "cancelled"].contains(&text(&c, "state")),
        "{c}"
    );
    let j = a.wait_state("c", 1, &["cancelled"], 10 * SECS);
    assert_eq!(j["exit_code"].as_i64(), Some(130));
    assert!(!alive(pid));
    let e = a.err("CancelJob", r#"{"job_id":"nope","attempt":1}"#);
    assert_eq!(text(&e, "code"), "not_found");
    assert_eq!(
        text(&e, "journal_epoch"),
        a.ok("Hello", "{}")["journal_epoch"].as_str().unwrap()
    );
}

fn restart_mid_job(extra: &[&str]) {
    let mut a = Agent::start("restart", extra);
    let epoch = a.ok("Hello", "{}")["journal_epoch"]
        .as_str()
        .unwrap()
        .to_string();
    let submit = r#"{"job_id":"long","attempt":1,"script":"sleep 20"}"#;
    let pid = num(&a.submit(submit), "pid");
    let short = a.submit(r#"{"job_id":"short","attempt":1,"script":"sleep 1"}"#);
    a.kill();
    assert!(alive(pid), "the job died with the agent");
    std::thread::sleep(2 * SECS);
    a.restart();

    assert_eq!(
        a.ok("Hello", "{}")["journal_epoch"].as_str(),
        Some(epoch.as_str())
    );
    let long = a.status("long", 1);
    assert_eq!(text(&long, "state"), "interrupted", "{long}");
    assert!(!alive(pid), "the interrupted job still runs");
    let short_now = a.status("short", 1);
    assert_eq!(text(&short_now, "state"), "lost", "{short_now}");
    assert_eq!(num(&short_now, "pid"), num(&short, "pid"));

    // Neither runs again, by itself or on a repeated submit; a new attempt does run.
    assert_eq!(text(&a.submit(submit), "state"), "interrupted");
    assert_eq!(a.journal().matches(r#""type":"start""#).count(), 2);
    let next = a.submit(r#"{"job_id":"long","attempt":2,"script":"exit 0"}"#);
    assert_ne!(num(&next, "pid"), pid);
    a.wait_state("long", 2, &["succeeded"], 10 * SECS);
}

#[test]
fn a_restart_reports_running_jobs_interrupted_or_lost_in_scopes() {
    restart_mid_job(&[]);
}

#[test]
fn a_restart_reports_running_jobs_interrupted_or_lost_in_process_groups() {
    restart_mid_job(&["--no-scope"]);
}

#[test]
fn a_tombstone_stays_until_ack_collected() {
    let mut a = Agent::start("ack", &[]);
    a.submit(r#"{"job_id":"t","attempt":1,"script":"echo done"}"#);
    let running = a.submit(r#"{"job_id":"r","attempt":1,"script":"sleep 20"}"#);
    a.wait_state("t", 1, &["succeeded"], 10 * SECS);
    let e = a.err("AckCollected", r#"{"job_id":"r","attempt":1}"#);
    assert_eq!(text(&e, "code"), "not_finished");

    std::thread::sleep(2 * SECS);
    a.restart();
    assert_eq!(text(&a.status("t", 1), "state"), "succeeded");
    assert_eq!(a.fetch("t", 1, 0, 100).0, b"done\n");
    let jobs = a.ok("ListJobs", "{}");
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 2, "{jobs}");

    let acked = a.ok("AckCollected", r#"{"job_id":"t","attempt":1}"#);
    assert_eq!(text(&acked["job"], "state"), "succeeded");
    let gone = a.ok("JobStatus", r#"{"job_id":"t","attempt":1}"#);
    assert!(gone.get("job").is_none(), "{gone}");
    assert_eq!(gone["journal_epoch"], acked["journal_epoch"]);
    let e = a.err("FetchLog", r#"{"job_id":"t","attempt":1}"#);
    assert_eq!(text(&e, "code"), "not_found");
    assert!(!a.state_dir().join("logs").join("t.1.log").exists());

    a.restart();
    assert!(a.ok("JobStatus", r#"{"job_id":"t"}"#).get("job").is_none());
    assert_eq!(text(&a.status("r", 1), "state"), "interrupted");
    assert!(!alive(num(&running, "pid")));
}

#[test]
fn a_job_is_refused_while_a_person_is_present() {
    let a = Agent::start("present", &[]);
    for state in ["PRESENT", "UNKNOWN"] {
        a.set_presence(state);
        let e = a.err(
            "SubmitJob",
            r#"{"job_id":"p","attempt":1,"script":"echo no"}"#,
        );
        assert_eq!(text(&e, "code"), "present", "{e}");
        assert!(text(&e, "message").contains("test-override"), "{e}");
        let never = a.ok("JobStatus", r#"{"job_id":"p"}"#);
        assert!(
            never.get("job").is_none(),
            "a refused job was recorded: {never}"
        );
    }
    assert!(!a.journal().contains(r#""type":"submit""#));
    let j = a.submit(r#"{"job_id":"p","attempt":1,"script":"echo yes","even_if_present":true}"#);
    assert_eq!(text(&j, "state"), "running");
    a.wait_state("p", 1, &["succeeded"], 10 * SECS);
}

fn nice_of(pid: u64) -> i64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let rest = stat.rsplit_once(')').unwrap().1;
    rest.split_whitespace().nth(16).unwrap().parse().unwrap()
}

#[test]
fn a_person_arriving_lowers_or_kills_a_running_job() {
    let a = Agent::start("arrive", &[]);
    let lower = a.submit(r#"{"job_id":"low","attempt":1,"script":"sleep 20"}"#);
    let kill = a.submit(r#"{"job_id":"kill","attempt":1,"script":"sleep 20","on_present":"kill"}"#);
    a.set_presence("PRESENT");
    let k = a.wait_state("kill", 1, &["failed"], 10 * SECS);
    assert_eq!(k["exit_code"].as_i64(), Some(137), "{k}");
    assert!(!alive(num(&kill, "pid")));
    let deadline = std::time::Instant::now() + 10 * SECS;
    while a.status("low", 1)["lowered"].as_bool() != Some(true) {
        assert!(std::time::Instant::now() < deadline, "never lowered");
        std::thread::sleep(Duration::from_millis(100));
    }
    let l = a.status("low", 1);
    assert_eq!(text(&l, "state"), "running");
    assert_eq!(nice_of(num(&lower, "pid")), 19);
    a.ok("CancelJob", r#"{"job_id":"low","attempt":1}"#);
    a.wait_state("low", 1, &["cancelled"], 10 * SECS);
}

#[test]
fn bad_and_conflicting_submits_are_refused() {
    let a = Agent::start("refuse", &[]);
    for (fields, why) in [
        (r#"{"job_id":"../x","attempt":1,"script":"true"}"#, "job_id"),
        (r#"{"job_id":"x","attempt":1}"#, "exactly one"),
        (
            r#"{"job_id":"x","attempt":1,"script":"true","argv":["true"]}"#,
            "exactly one",
        ),
        (
            r#"{"job_id":"x","attempt":1,"script":"true","timeout_secs":0}"#,
            "timeout_secs",
        ),
        (
            r#"{"job_id":"x","attempt":1,"script":"true","cwd":"relative"}"#,
            "absolute",
        ),
        (r#"{"job_id":"x","attempt":"1","script":"true"}"#, "attempt"),
        (
            r#"{"job_id":"x","attempt":1,"script":"true","on_present":"ignore"}"#,
            "on_present",
        ),
    ] {
        let e = a.err_raw("SubmitJob", fields);
        assert_eq!(text(&e, "code"), "bad_request", "{fields}: {e}");
        assert!(text(&e, "message").contains(why), "{fields}: {e}");
    }
    let e = a.err_raw("Reboot", "{}");
    assert_eq!(text(&e, "code"), "unknown_method");
    let e = a.err_raw("JobStatus", r#"{"attempt":1}"#);
    assert_eq!(text(&e, "code"), "bad_request");
    let missing = a.submit(r#"{"job_id":"nobin","attempt":1,"argv":["/nonexistent/program"]}"#);
    let j = a.wait_state("nobin", 1, &["failed"], 10 * SECS);
    assert!(j["exit_code"].as_i64() != Some(0), "{missing} {j}");

    let x = a.submit(r#"{"job_id":"x","attempt":2,"script":"sleep 20"}"#);
    let e = a.err("SubmitJob", r#"{"job_id":"x","attempt":1,"script":"true"}"#);
    assert_eq!(text(&e, "code"), "stale_attempt");
    let e = a.err("SubmitJob", r#"{"job_id":"x","attempt":3,"script":"true"}"#);
    assert_eq!(text(&e, "code"), "busy");
    a.ok("CancelJob", r#"{"job_id":"x","attempt":2}"#);
    a.wait_state("x", 2, &["cancelled"], 10 * SECS);
    // A cancel right after the submit must not strand an empty scope.
    if let Some(scope) = x["scope"].as_str() {
        let deadline = std::time::Instant::now() + 5 * SECS;
        while scope_active(scope) {
            assert!(
                std::time::Instant::now() < deadline,
                "{scope} is still active"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn scope_active(scope: &str) -> bool {
    let out = std::process::Command::new("systemctl")
        .args(["--user", "is-active", scope])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim() == "active"
}

#[test]
fn a_log_over_its_cap_is_truncated_and_the_job_still_finishes() {
    let a = Agent::start("cap", &[]);
    a.submit(r#"{"job_id":"big","attempt":1,"script":"head -c 300000 /dev/zero; echo end","log_cap":1000}"#);
    let j = a.wait_state("big", 1, &["succeeded"], 20 * SECS);
    assert_eq!(num(&j, "log_bytes"), 1000);
    assert_eq!(j["log_truncated"].as_bool(), Some(true));
    let (bytes, end) = a.fetch("big", 1, 0, 1 << 20);
    assert_eq!(bytes.len(), 1000);
    assert_eq!(end["truncated"].as_bool(), Some(true));
}

/// FetchArtifact end to end: a 3 MB binary file in slices with its SHA-256, every way out of the
/// job's directory refused, and the submit token fencing reads, cancels and acks.
#[test]
fn artifacts_come_only_from_inside_the_job_dir_and_tokens_fence() {
    let a = Agent::start("artifact", &[]);
    let outside = a.dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret"), "secret").unwrap();
    let o = outside.display();
    let script = format!(
        "head -c 3000000 /dev/urandom > big.bin; mkdir sub; printf small > sub/small.txt; \
         ln -s {o}/secret esc; ln -s {o} escdir; ln -s sub/small.txt inside; mkfifo fifo; echo made"
    );
    let submit = pt_json::obj! {
        "job_id" => "art", "attempt" => 1u64, "script" => script.as_str(), "token" => "tok-1",
    };
    let j = a.submit(&submit.to_string());
    assert_eq!(text(&j, "token"), "tok-1");
    let j = a.wait_state("art", 1, &["succeeded"], 20 * SECS);
    assert_eq!(text(&j, "token"), "tok-1", "{j}");
    let dir = a.state_dir().join("work").join("art.1");
    let big = std::fs::read(dir.join("big.bin")).unwrap();
    assert_eq!(big.len(), 3_000_000);

    let fetch = |path: &str, offset: u64, max: u64, token: &str| {
        let f = pt_json::obj! {
            "job_id" => "art", "attempt" => 1u64, "path" => path, "offset" => offset,
            "max" => max, "token" => token,
        };
        a.stream("FetchArtifact", &f.to_string())
    };
    let mut got = Vec::new();
    let end = loop {
        let (bytes, end) = fetch("big.bin", got.len() as u64, 1 << 20, "tok-1");
        assert!(bytes.len() <= 1 << 20);
        got.extend_from_slice(&bytes);
        assert_eq!(num(&end, "offset"), got.len() as u64, "{end}");
        assert_eq!(num(&end, "size"), 3_000_000);
        if end["done"].as_bool() == Some(true) {
            break end;
        }
        assert!(end.get("sha256").is_none(), "{end}");
    };
    assert!(got == big, "the slices differ from the file");
    assert_eq!(
        text(&end, "sha256"),
        pt_hash::to_hex(&pt_hash::sha256(&big))
    );
    let (small, end) = fetch("inside", 0, 100, "tok-1");
    assert_eq!(small, b"small");
    assert_eq!(
        text(&end, "sha256"),
        pt_hash::to_hex(&pt_hash::sha256(b"small"))
    );
    let (past, end) = fetch("sub/small.txt", 99, 10, "tok-1");
    assert!(past.is_empty());
    assert_eq!(
        (num(&end, "offset"), end["done"].as_bool()),
        (5, Some(true))
    );

    let artifact = |path: &str, token: &str| {
        let f = pt_json::obj! {
            "job_id" => "art", "attempt" => 1u64, "path" => path, "token" => token,
        };
        a.err("FetchArtifact", &f.to_string())
    };
    let secret = outside.join("secret");
    for path in [
        "esc",
        "escdir/secret",
        "../../logs/art.1.log",
        "sub/../../art.1/../../node-id",
        secret.to_str().unwrap(),
    ] {
        let e = artifact(path, "tok-1");
        assert_eq!(text(&e, "code"), "bad_request", "{path}: {e}");
        assert!(text(&e, "message").contains("outside"), "{path}: {e}");
    }
    for path in ["fifo", "sub", "."] {
        let e = artifact(path, "tok-1");
        assert_eq!(text(&e, "code"), "bad_request", "{path}: {e}");
        assert!(
            text(&e, "message").contains("not a regular file"),
            "{path}: {e}"
        );
    }
    assert_eq!(text(&artifact("nope", "tok-1"), "code"), "not_found");
    assert!(a.log().contains("refused"), "{}", a.log());

    // Another submit's token gets nothing; without a token (protocol 1 hubs) it still answers.
    assert_eq!(text(&artifact("big.bin", "tok-2"), "code"), "stale_attempt");
    let e = a.err(
        "FetchLog",
        r#"{"job_id":"art","attempt":1,"token":"tok-2"}"#,
    );
    assert_eq!(text(&e, "code"), "stale_attempt");
    assert_eq!(a.fetch("art", 1, 0, 100).0, b"made\n");
    let e = a.err(
        "AckCollected",
        r#"{"job_id":"art","attempt":1,"token":"tok-2"}"#,
    );
    assert_eq!(text(&e, "code"), "stale_attempt");
    // The same (job, attempt) resubmitted with another token answers the record it holds.
    let again = a.submit(r#"{"job_id":"art","attempt":1,"script":"true","token":"tok-2"}"#);
    assert_eq!(text(&again, "token"), "tok-1");

    let run = a.submit(r#"{"job_id":"run","attempt":1,"script":"sleep 20","token":"r-1"}"#);
    assert_eq!(text(&run, "token"), "r-1");
    let e = a.err(
        "FetchArtifact",
        r#"{"job_id":"run","attempt":1,"path":"x","token":"r-1"}"#,
    );
    assert_eq!(text(&e, "code"), "not_finished");
    let e = a.err("CancelJob", r#"{"job_id":"run","attempt":1,"token":"r-2"}"#);
    assert_eq!(text(&e, "code"), "stale_attempt");
    assert_eq!(text(&a.status("run", 1), "state"), "running");
    a.ok("CancelJob", r#"{"job_id":"run","attempt":1,"token":"r-1"}"#);
    a.wait_state("run", 1, &["cancelled"], 10 * SECS);

    // The ack drops the node's own directory with the tombstone.
    a.ok(
        "AckCollected",
        r#"{"job_id":"art","attempt":1,"token":"tok-1"}"#,
    );
    assert!(!dir.exists(), "{} survived the ack", dir.display());
    assert_eq!(text(&artifact("big.bin", "tok-1"), "code"), "not_found");
    assert!(secret.exists());
    let bad = a.err_raw(
        "SubmitJob",
        r#"{"job_id":"t","attempt":1,"script":"true","token":"has space"}"#,
    );
    assert!(text(&bad, "message").contains("token"), "{bad}");
}
