//! Runs the built binary on this machine and checks the shape and sanity of what it prints.

mod common;

use common::{check_inventory, check_presence_value, check_sample, json};

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_protectorate-node"))
        .args(args)
        .output()
        .expect("spawn protectorate-node")
}

fn stdout_ok(args: &[&str]) -> String {
    let out = run(args);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn version() {
    let out = stdout_ok(&["--version"]);
    let first = out.lines().next().unwrap_or("");
    let v = first.strip_prefix("protectorate-node ").expect(&out);
    assert!(!v.is_empty() && v != "0.0.0", "{out}");
    assert!(out.contains("\nmember: "), "{out}");
}

#[test]
fn unknown_command_exits_2() {
    let out = run(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command"));
}

#[test]
fn inventory_is_sane() {
    let out = json(&stdout_ok(&["inventory"]));
    check_inventory(&out);
}

#[test]
fn sample_is_sane() {
    check_sample(&json(&stdout_ok(&["sample"])));
}

fn check_presence(out: &str, away_after: u64) {
    check_presence_value(&json(out), away_after);
}

#[test]
fn presence_names_its_source() {
    check_presence(&stdout_ok(&["presence"]), 600);
}

#[test]
fn presence_threshold_is_configurable() {
    check_presence(&stdout_ok(&["presence", "--away-after", "5"]), 5);
}

#[test]
fn presence_rejects_bad_flags() {
    assert_eq!(run(&["presence", "--away-after"]).status.code(), Some(2));
    assert_eq!(run(&["presence", "--bogus"]).status.code(), Some(2));
}

#[test]
fn watch_presence_prints_first_state() {
    let out = stdout_ok(&["watch-presence", "--limit", "1", "--interval", "1"]);
    assert_eq!(out.lines().count(), 1, "{out}");
    check_presence(out.trim(), 600);
}

#[cfg(target_os = "linux")]
#[test]
fn lower_idle_sets_nice_19() {
    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id().to_string();
    let result = std::panic::catch_unwind(|| {
        stdout_ok(&["lower-idle", &pid]);
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("stat");
        let after_comm = stat.rsplit_once(')').expect("comm").1;
        let nice: i32 = after_comm
            .split_whitespace()
            .nth(16)
            .expect("nice")
            .parse()
            .expect("nice number");
        assert_eq!(nice, 19);
    });
    let _ = child.kill();
    let _ = child.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn lower_idle_missing_process_fails() {
    let out = run(&["lower-idle", "4000000"]);
    assert_eq!(out.status.code(), Some(2));
}

/// Presence through a stand-in `loginctl` on PATH, so every logind branch runs against the real
/// binary without a desktop session. Sessions: "s1" (local seat) and "s2" (ssh, no seat).
#[cfg(target_os = "linux")]
mod fake_logind {
    use crate::common::{json, num, text};
    use pt_json::Value;
    use std::fs;
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::OnceLock;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn now_usec() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    /// The stand-in, written once. While it is open for writing, a child that another test
    /// thread forks holds it until that child execs, and running it then fails with ETXTBSY;
    /// so it is run here until it runs, and never written again.
    fn loginctl_dir() -> &'static PathBuf {
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        DIR.get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fake-logind-bin");
            fs::create_dir_all(&dir).unwrap();
            let script = dir.join("loginctl");
            fs::write(
                &script,
                "#!/bin/sh\ncase \"$1\" in\n list-sessions) echo \"s1 1000 u seat0 tty1\"; echo \"s2 1000 u - pts/0\";;\n show-session) cat \"$FAKE_DIR/$2\";;\nesac\n",
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
            for _ in 0..100 {
                match Command::new(&script).output() {
                    Err(e) if e.kind() == ErrorKind::ExecutableFileBusy => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    _ => break,
                }
            }
            dir
        })
    }

    fn presence(case: &str, s1: &str) -> Value {
        let bin = loginctl_dir();
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("fake-logind-{case}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("s1"), s1).unwrap();
        fs::write(
            dir.join("s2"),
            "Id=s2\nSeat=\nClass=user\nActive=yes\nState=active\nRemote=yes\nIdleHint=no\nIdleSinceHint=0\nLockedHint=no\n",
        )
        .unwrap();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new(env!("CARGO_BIN_EXE_protectorate-node"))
            .args(["presence", "--away-after", "600"])
            .env("PATH", path)
            .env("FAKE_DIR", &dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        json(&String::from_utf8(out.stdout).unwrap())
    }

    fn session(idle_hint: &str, since_secs_ago: Option<u64>, locked: &str) -> String {
        let since = since_secs_ago.map_or(0, |s| now_usec() - s * 1_000_000);
        format!(
            "Id=s1\nSeat=seat0\nClass=user\nActive=yes\nState=active\nRemote=no\nIdleHint={idle_hint}\nIdleSinceHint={since}\nLockedHint={locked}\n"
        )
    }

    #[test]
    fn long_idle_session_is_away() {
        let j = presence("idle", &session("yes", Some(1200), "no"));
        assert_eq!(text(&j, "state"), "AWAY");
        assert!(num(&j, "idle_secs") >= 1200);
        assert_eq!(text(&j, "source"), "logind");
    }

    #[test]
    fn recent_idle_hint_is_present() {
        let j = presence("recent", &session("yes", Some(30), "no"));
        assert_eq!(text(&j, "state"), "PRESENT");
    }

    #[test]
    fn active_idle_hint_no_is_present() {
        let j = presence("active", &session("no", None, "no"));
        assert_eq!(text(&j, "state"), "PRESENT");
        assert!(j.get("idle_secs").is_none(), "{j}");
    }

    #[test]
    fn locked_and_idle_past_grace_is_away() {
        let j = presence("locked", &session("yes", Some(120), "yes"));
        assert_eq!(text(&j, "state"), "AWAY");
        assert_eq!(j["locked"].as_bool(), Some(true));
    }

    #[test]
    fn future_idle_since_is_unknown() {
        let s = format!(
            "Id=s1\nSeat=seat0\nClass=user\nActive=yes\nState=active\nRemote=no\nIdleHint=yes\nIdleSinceHint={}\nLockedHint=no\n",
            now_usec() + 3_600_000_000
        );
        let j = presence("future", &s);
        assert_eq!(text(&j, "state"), "UNKNOWN");
    }

    #[test]
    fn greeter_only_is_away_and_ssh_never_counts() {
        let j = presence(
            "greeter",
            "Id=s1\nSeat=seat0\nClass=greeter\nActive=yes\nState=active\nRemote=no\nIdleHint=no\nIdleSinceHint=0\nLockedHint=no\n",
        );
        assert_eq!(text(&j, "state"), "AWAY");
    }
}
