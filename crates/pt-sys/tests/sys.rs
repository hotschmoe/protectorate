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
