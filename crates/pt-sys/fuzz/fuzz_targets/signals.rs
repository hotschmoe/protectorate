#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg(target_os = "linux")]
    if let Some(&choice) = data.first() {
        check_pre_exec_signal(choice);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = data;
});

#[cfg(target_os = "linux")]
fn check_pre_exec_signal(choice: u8) {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    let flag = pt_sys::install_term_handler().expect("install termination handler");
    let delay = Duration::from_millis(1000 + u64::from(choice & 1) * 100);
    let (mut receiver, sender) = UnixStream::pair().expect("notification socket");
    receiver
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let spawner = std::thread::spawn(move || {
        let mut cmd = Command::new("true");
        let fd = sender.as_raw_fd();
        // SAFETY: this deliberately holds the child between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                let pid = libc::getpid().to_ne_bytes();
                if libc::write(fd, pid.as_ptr().cast(), pid.len()) != pid.len() as isize {
                    return Err(std::io::Error::last_os_error());
                }
                std::thread::sleep(delay);
                Ok(())
            })
        };
        let _ = cmd.spawn().map(|mut child| child.wait());
    });
    let mut pid = [0; std::mem::size_of::<libc::pid_t>()];
    receiver.read_exact(&mut pid).expect("pre-exec child pid");
    let child = libc::pid_t::from_ne_bytes(pid).to_string();
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
    spawner.join().expect("spawning thread");
    assert!(!woke, "the child's signal woke the parent");
    assert!(spent < 30, "the parent spun for {spent} CPU ticks");
}

#[cfg(target_os = "linux")]
fn cpu_ticks() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}
