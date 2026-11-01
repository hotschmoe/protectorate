//! Real sockets, real signals from the `kill` binary, real children.

use pt_sys::{SIGKILL, SIGTERM, install_term_handler, pdeathsig_on_spawn, peer_cred};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn peer_cred_names_this_process() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let marker = dir.join(format!("pt-sys-uid-{}", std::process::id()));
    std::fs::write(&marker, b"").unwrap();
    let meta = std::fs::metadata(&marker).unwrap();

    let sock = dir.join(format!("pt-sys-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&sock).unwrap();
    let client = UnixStream::connect(&sock).unwrap();
    let (server, _) = listener.accept().unwrap();
    for s in [&client, &server] {
        let cred = peer_cred(s).unwrap();
        assert_eq!(cred.pid as u32, std::process::id());
        assert_eq!(cred.uid, meta.uid());
        assert_eq!(cred.gid, meta.gid());
    }
}

#[test]
fn term_flag_sees_sigterm() {
    let flag = install_term_handler().unwrap();
    let again = install_term_handler().unwrap();
    assert!(!flag.is_set());
    assert!(!flag.wait_timeout(Duration::from_millis(50)));
    let waiter = std::thread::spawn(move || {
        again.wait();
        again.signal()
    });
    let status = Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(flag.wait_timeout(Duration::from_secs(5)));
    assert_eq!(flag.signal(), Some(SIGTERM));
    assert_eq!(waiter.join().unwrap(), Some(SIGTERM));
    assert_eq!(pt_sys::wait_term().unwrap(), SIGTERM);
}

#[test]
fn pdeathsig_kills_child_when_spawning_thread_exits() {
    let mut child = std::thread::spawn(|| {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        pdeathsig_on_spawn(&mut cmd, SIGKILL).unwrap();
        cmd.spawn().unwrap()
    })
    .join()
    .unwrap();
    let start = Instant::now();
    let status = child.wait().unwrap();
    assert_eq!(status.signal(), Some(SIGKILL));
    assert!(start.elapsed() < Duration::from_secs(10));
}

// The scenarios below run in a fresh copy of this test binary (`helper`), because the
// termination flag is process-wide and `term_flag_sees_sigterm` signals this whole process.

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

fn cpu_ticks() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let f: Vec<&str> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    f[11].parse::<u64>().unwrap() + f[12].parse::<u64>().unwrap()
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
fn helper_spin() {
    if !is_helper("helper_spin") {
        return;
    }
    let flag = install_term_handler().unwrap();
    let spawner = std::thread::spawn(|| {
        let mut cmd = Command::new("true");
        pre_exec_sleep(&mut cmd, Duration::from_secs(1));
        let _ = cmd.spawn().map(|mut c| c.wait());
    });
    let me = std::process::id().to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    let child = loop {
        let found = std::fs::read_dir("/proc").unwrap().flatten().find_map(|e| {
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            let ppid = stat
                .rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(1)?
                .to_string();
            (ppid == me).then(|| e.file_name().to_string_lossy().into_owned())
        });
        if let Some(pid) = found {
            break pid;
        }
        assert!(Instant::now() < deadline, "no pre-exec child appeared");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        Command::new("kill")
            .args(["-TERM", &child])
            .status()
            .unwrap()
            .success()
    );
    let before = cpu_ticks();
    let woke = flag.wait_timeout(Duration::from_millis(1500));
    let spent = cpu_ticks() - before;
    spawner.join().unwrap();
    println!("RESULT {woke} {spent}");
}

/// A SIGTERM aimed at a child between fork and exec is not the parent's signal: the parent's
/// flag stays clear and its waiter sleeps instead of spinning on the shared self-pipe.
#[test]
fn a_signal_to_a_pre_exec_child_leaves_the_parent_asleep() {
    let out = helper("helper_spin").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let r = result(&out);
    let (woke, spent) = r.split_once(' ').unwrap();
    assert_eq!(
        woke, "false",
        "the parent's flag was set by its child's signal"
    );
    let spent: u64 = spent.parse().unwrap();
    assert!(spent < 30, "the waiter spun for {spent} ticks of 150");
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
