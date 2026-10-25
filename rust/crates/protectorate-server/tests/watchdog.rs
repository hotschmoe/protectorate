//! The hang watchdog in the real hub binary: a process whose runtime threads
//! are all stuck exits 70 with a diagnostic, and a process that is merely
//! frozen for longer than the limit does not.
#![cfg(target_os = "linux")]
use crate::support::{binary, bound, die_with_parent, until, Process, OWNER};

use protectorate_connect_client::Client;
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LIMIT: Duration = Duration::from_secs(5);

struct Running {
    process: Process,
    client: Client,
    log: std::path::PathBuf,
}

fn start(root: &Path) -> Running {
    let user_home = root.join("user");
    let home = root.join("hub");
    fs::create_dir_all(&user_home).unwrap();
    fs::create_dir_all(&home).unwrap();
    let token = root.join("owner-token");
    fs::write(&token, OWNER).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let log = root.join("hub.log");
    let mut process = Process(
        die_with_parent(Command::new(binary()).args(["serve", "--home"]))
            .arg(&home)
            .arg("--owner-token-file")
            .arg(&token)
            .args([
                "--listen",
                "127.0.0.1:0",
                "--release-api-url",
                "http://127.0.0.1:1",
            ])
            .arg("--tmux-socket")
            .arg(format!("p-watchdog-{}", std::process::id()))
            .env("HOME", &user_home)
            .env_remove("PROTECTORATE_HOME")
            .env("PROTECTORATE_HUB_GUARD_CHILD", "1")
            .env(
                "PROTECTORATE_WATCHDOG_TIMEOUT",
                format!("{}s", LIMIT.as_secs()),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&log).unwrap()))
            .spawn()
            .unwrap(),
    );
    until("hub start", &log, || {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "hub exited: {}",
            fs::read_to_string(&log).unwrap_or_default()
        );
        bound(&log, "serving").is_some()
    });
    let client = Client::new(
        &format!("http://{}", bound(&log, "serving").unwrap()),
        OWNER,
        Duration::from_secs(5),
    )
    .unwrap();
    until("hub status", &log, || {
        client
            .call("/protectorate.v1.HubService/GetStatus", &json!({}))
            .is_ok()
    });
    Running {
        process,
        client,
        log,
    }
}

fn threads(pid: u32) -> Vec<(i32, String)> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .flatten()
        .filter_map(|task| {
            let tid = task.file_name().to_string_lossy().parse().ok()?;
            let comm = fs::read_to_string(task.path().join("comm")).ok()?;
            Some((tid, comm.trim().to_owned()))
        })
        .collect()
}

/// Stops every thread of `pid` except the watchdog's, as a deadlock of all of
/// them would, using ptrace from outside. Returns when they are all stopped;
/// the tracer thread holds them until `release` is set.
fn wedge_all_but_watchdog(pid: u32, release: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    let (ready, wait_ready) = std::sync::mpsc::channel();
    let tracer = thread::spawn(move || {
        let mut stopped = BTreeSet::new();
        loop {
            let mut fresh = false;
            for (tid, comm) in threads(pid) {
                if comm == "pr-watchdog" || stopped.contains(&tid) {
                    continue;
                }
                unsafe {
                    if libc::ptrace(libc::PTRACE_SEIZE, tid, 0usize, 0usize) != 0 {
                        continue;
                    }
                    libc::ptrace(libc::PTRACE_INTERRUPT, tid, 0usize, 0usize);
                    let mut status = 0;
                    libc::waitpid(tid, &mut status, libc::__WALL);
                }
                stopped.insert(tid);
                fresh = true;
            }
            if !fresh {
                break;
            }
        }
        ready.send(stopped.len()).unwrap();
        // Traced threads stay zombies until their tracer waits for them, and
        // the process is not reaped by its parent before that.
        while !release.load(Ordering::Relaxed) {
            for tid in &stopped {
                let mut status = 0;
                unsafe { libc::waitpid(*tid, &mut status, libc::__WALL | libc::WNOHANG) };
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    let count = wait_ready.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(
        count > 2,
        "only {count} threads stopped; is ptrace allowed?"
    );
    tracer
}

fn hub_pid(running: &Running) -> u32 {
    running.process.0.id()
}

#[test]
fn watchdog_exits_a_hub_whose_runtime_is_wedged() {
    let root = tempfile::tempdir().unwrap();
    let mut running = start(root.path());
    let pid = hub_pid(&running);
    let release = Arc::new(AtomicBool::new(false));
    let wedged = Instant::now();
    let tracer = wedge_all_but_watchdog(pid, Arc::clone(&release));
    let status = loop {
        if let Some(status) = running.process.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            wedged.elapsed() < LIMIT * 6,
            "wedged hub was not stopped: {}",
            fs::read_to_string(&running.log).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(50));
    };
    release.store(true, Ordering::Relaxed);
    tracer.join().unwrap();
    let log = fs::read_to_string(&running.log).unwrap();
    assert_eq!(status.code(), Some(70), "{log}");
    assert!(
        wedged.elapsed() >= LIMIT - Duration::from_secs(1),
        "exited after {:?}, before the limit: {log}",
        wedged.elapsed()
    );
    // Every probe stalls at once, so whichever the watchdog reads first names
    // the exit; the dump lists the runtime heartbeat with the rest.
    assert!(log.contains("made no progress for"), "{log}");
    assert!(
        log.contains("watchdog:   runtime heartbeat: last progress"),
        "{log}"
    );
    assert!(log.contains("watchdog:   thread "), "no thread dump: {log}");
}

#[test]
fn watchdog_forgives_a_process_frozen_longer_than_its_limit() {
    let root = tempfile::tempdir().unwrap();
    let mut running = start(root.path());
    let pid = hub_pid(&running) as i32;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
    thread::sleep(LIMIT + Duration::from_secs(3));
    assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    // Past one more full limit, so a false positive would have fired.
    let until = Instant::now() + LIMIT + Duration::from_secs(2);
    while Instant::now() < until {
        assert!(
            running.process.0.try_wait().unwrap().is_none(),
            "hub exited after a freeze: {}",
            fs::read_to_string(&running.log).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(200));
    }
    running
        .client
        .call("/protectorate.v1.HubService/GetStatus", &json!({}))
        .unwrap();
    let log = fs::read_to_string(&running.log).unwrap();
    assert!(!log.contains("watchdog:"), "{log}");
}
