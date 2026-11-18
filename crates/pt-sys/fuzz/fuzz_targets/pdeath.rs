#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("PT_SYS_FUZZ_PDEATH_HELPER").is_some() {
            pre_exec_helper();
            std::process::exit(0);
        }
        if let Some(&choice) = data.first() {
            if choice % 3 == 2 {
                check_parent_dies_before_exec();
            } else {
                check_thread_exit(choice);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = data;
});

#[cfg(target_os = "linux")]
fn check_thread_exit(choice: u8) {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    let signal = if choice & 1 == 0 {
        pt_sys::SIGKILL
    } else {
        pt_sys::SIGTERM
    };
    let mut child = std::thread::spawn(move || {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        pt_sys::pdeathsig_on_spawn(&mut cmd, signal).expect("set parent-death signal");
        cmd.spawn().expect("spawn child")
    })
    .join()
    .expect("spawning thread");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for child") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child survived its spawning thread");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.signal(), Some(signal));
}

#[cfg(target_os = "linux")]
fn pre_exec_helper() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    pt_sys::install_term_handler().expect("install termination handler");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo $$ > \"$PT_SYS_FUZZ_MARKER\"; exec sleep 1"]);
    pt_sys::pdeathsig_on_spawn(&mut cmd, pt_sys::SIGTERM).expect("set parent-death signal");
    // SAFETY: write is signal-safe; the sleep holds the child before exec.
    unsafe {
        cmd.pre_exec(|| {
            let ready = b"READY\n";
            if libc::write(libc::STDOUT_FILENO, ready.as_ptr().cast(), ready.len())
                != ready.len() as isize
            {
                return Err(std::io::Error::last_os_error());
            }
            std::thread::sleep(Duration::from_secs(2));
            Ok(())
        })
    };
    let _ = cmd.spawn().map(|mut child| child.wait());
}

#[cfg(target_os = "linux")]
fn check_parent_dies_before_exec() {
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{OnceLock, mpsc};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static MARKER: OnceLock<PathBuf> = OnceLock::new();
    let marker = MARKER.get_or_init(|| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pt-pdeath-{}-{now}", std::process::id()))
    });
    assert!(!marker.exists(), "a prior child reached exec");
    let mut helper = Command::new(std::env::current_exe().expect("fuzz executable"))
        .arg("-runs=1")
        .env("PT_SYS_FUZZ_PDEATH_HELPER", "1")
        .env("PT_SYS_FUZZ_MARKER", marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn helper");
    let (ready_tx, ready_rx) = mpsc::channel();
    let stdout = helper.stdout.take().expect("helper stdout");
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line == "READY" {
                let _ = ready_tx.send(());
                break;
            }
        }
    });
    if ready_rx.recv_timeout(Duration::from_secs(10)).is_err() {
        let _ = helper.kill();
        let _ = helper.wait();
        let _ = reader.join();
        panic!("pre-exec child did not become ready");
    }
    helper.kill().expect("kill helper");
    helper.wait().expect("wait for helper");
    reader.join().expect("helper reader");
    std::thread::sleep(Duration::from_secs(3));
    assert!(!marker.exists(), "child reached exec after its parent died");
}
