//! Real sockets, real signals from the `kill` binary, real children.

use pt_sys::{SIGTERM, install_term_handler, pdeathsig_on_spawn};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

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
