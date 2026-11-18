//! Real sockets, real signals from the `kill` binary, real children.

use pt_sys::{SIGTERM, install_term_handler, pdeathsig_on_spawn};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const HELPER: &str = "PT_SYS_HELPER";

fn helper(name: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(HELPER, name)
        .stdout(std::process::Stdio::piped());
    c
}

fn is_helper(name: &str) -> bool {
    std::env::var(HELPER).as_deref() == Ok(name)
}

/// The helper's `RESULT <value>` line.
fn result(out: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| Some(l.split_once("RESULT ")?.1.trim()))
        .unwrap_or_else(|| panic!("no result from the helper:\n{text}"))
        .to_string()
}

fn pre_exec_sleep(cmd: &mut Command, d: Duration) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the hook only sleeps, which is nanosleep(2) in a retry loop.
    unsafe {
        cmd.pre_exec(move || {
            std::thread::sleep(d);
            Ok(())
        })
    };
}

#[test]
fn helper_pdeath_parent() {
    if !is_helper("helper_pdeath_parent") {
        return;
    }
    install_term_handler().unwrap();
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo $$ > \"$PT_MARKER\"; exec sleep 30"]);
    pdeathsig_on_spawn(&mut cmd, SIGTERM).unwrap();
    pre_exec_sleep(&mut cmd, Duration::from_secs(2));
    println!("SPAWNING");
    let _ = cmd.spawn();
    println!("SPAWNED");
}

/// The parent dies after the child set PR_SET_PDEATHSIG but before it execs. The child must
/// still die, not run the parent's inherited SIGTERM handler and carry on.
#[test]
fn a_child_whose_parent_dies_before_exec_still_dies() {
    use std::io::BufRead;
    let marker = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pt-sys-pdeath-{}", std::process::id()));
    let mut parent = helper("helper_pdeath_parent")
        .env("PT_MARKER", &marker)
        .spawn()
        .unwrap();
    let mut lines = std::io::BufReader::new(parent.stdout.take().unwrap()).lines();
    assert!(
        lines.any(|l| l.unwrap().contains("SPAWNING")),
        "the helper did not spawn"
    );
    std::thread::sleep(Duration::from_millis(500));
    parent.kill().unwrap();
    parent.wait().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    if let Ok(pid) = std::fs::read_to_string(&marker) {
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
        panic!(
            "the child survived its parent and ran as pid {}",
            pid.trim()
        );
    }
}

#[test]
fn helper_errno() {
    use std::os::unix::thread::JoinHandleExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    if !is_helper("helper_errno") {
        return;
    }
    install_term_handler().unwrap();
    // Nobody reads the self-pipe here: fill it so every later handler write fails with EAGAIN.
    for _ in 0..70_000 {
        // SAFETY: raise(3) takes a plain signal number; the handler is installed.
        unsafe { libc::raise(libc::SIGTERM) };
    }
    let stop = Arc::new(AtomicBool::new(false));
    let bad = Arc::new(AtomicU64::new(0));
    let (s, b) = (stop.clone(), bad.clone());
    let checker = std::thread::spawn(move || {
        while !s.load(Ordering::Relaxed) {
            // SAFETY: close(-1) touches no fd of ours and fails with EBADF.
            unsafe { libc::close(-1) };
            for i in 0..2_000u32 {
                std::hint::black_box(i);
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EBADF) {
                b.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let tid = checker.as_pthread_t();
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end {
        // SAFETY: `tid` names the checker thread, which runs until `stop` is set below.
        unsafe { libc::pthread_kill(tid, libc::SIGTERM) };
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    checker.join().unwrap();
    println!("RESULT {}", bad.load(std::sync::atomic::Ordering::Relaxed));
}

/// The handler runs between a failing call and the caller's read of errno; its own failed write
/// to the full self-pipe must not change what the caller reads.
#[test]
fn the_term_handler_keeps_errno() {
    let out = helper("helper_errno").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(result(&out), "0", "errno was clobbered that many times");
}
