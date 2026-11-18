#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg(target_os = "linux")]
    if let Some(&choice) = data.first() {
        check_errno(choice);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = data;
});

#[cfg(target_os = "linux")]
fn check_errno(choice: u8) {
    use std::os::unix::thread::JoinHandleExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    pt_sys::install_term_handler().expect("install termination handler");
    for _ in 0..70_000 {
        // SAFETY: raise sends a signal to this process.
        unsafe { libc::raise(libc::SIGTERM) };
    }
    let stop = Arc::new(AtomicBool::new(false));
    let bad = Arc::new(AtomicU64::new(0));
    let (thread_stop, thread_bad) = (stop.clone(), bad.clone());
    let work = 2_000 + u32::from(choice) * 16;
    let checker = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            // SAFETY: close(-1) fails without closing a descriptor.
            unsafe { libc::close(-1) };
            for i in 0..work {
                std::hint::black_box(i);
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EBADF) {
                thread_bad.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let tid = checker.as_pthread_t();
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end {
        // SAFETY: tid remains live until stop is set below.
        unsafe { libc::pthread_kill(tid, libc::SIGTERM) };
    }
    stop.store(true, Ordering::Relaxed);
    checker.join().expect("errno checker");
    assert_eq!(
        bad.load(Ordering::Relaxed),
        0,
        "signal handler changed errno"
    );
}
