#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg(target_os = "linux")]
    if let Some(&choice) = data.first() {
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
    #[cfg(not(target_os = "linux"))]
    let _ = data;
});
