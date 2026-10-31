use crate::PeerCred;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn cvt(rc: libc::c_int) -> io::Result<libc::c_int> {
    if rc == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

#[cfg(target_os = "linux")]
pub fn peer_cred(sock: &UnixStream) -> io::Result<PeerCred> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the fd is a live socket borrowed from `sock`; `cred` and `len` are writable locals
    // and `len` holds the size of `cred`, which is what SO_PEERCRED writes.
    let rc = unsafe {
        libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast(),
            &mut len,
        )
    };
    cvt(rc).map_err(|e| io::Error::new(e.kind(), format!("SO_PEERCRED: {e}")))?;
    Ok(PeerCred {
        pid: cred.pid,
        uid: cred.uid,
        gid: cred.gid,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn peer_cred(_sock: &UnixStream) -> io::Result<PeerCred> {
    Err(crate::unsupported("SO_PEERCRED"))
}

static SIGNAL: AtomicI32 = AtomicI32::new(0);
static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);
static INSTALLED: Mutex<Option<TermFlag>> = Mutex::new(None);

extern "C" fn on_term(sig: libc::c_int) {
    let _ = SIGNAL.compare_exchange(0, sig, Ordering::SeqCst, Ordering::SeqCst);
    let fd = PIPE_WRITE.load(Ordering::SeqCst);
    let byte = 1u8;
    // SAFETY: write(2) is async-signal-safe; `fd` is the nonblocking write end of the self-pipe,
    // stored before the handler was installed and never closed. A full pipe drops the byte, which
    // is fine because one unread byte keeps the read end readable.
    unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
}

/// Set once SIGTERM, SIGINT or SIGHUP arrives. Clones share the same flag.
#[derive(Clone)]
pub struct TermFlag {
    read: Arc<OwnedFd>,
}

impl TermFlag {
    pub fn is_set(&self) -> bool {
        SIGNAL.load(Ordering::SeqCst) != 0
    }

    /// The first termination signal received.
    pub fn signal(&self) -> Option<i32> {
        Some(SIGNAL.load(Ordering::SeqCst)).filter(|&s| s != 0)
    }

    pub fn wait(&self) {
        while !self.poll(-1) {}
    }

    /// Waits up to `timeout`; true when the flag is set.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let ms = left.as_millis().min(i32::MAX as u128) as i32;
            if self.poll(ms) || left.is_zero() {
                return self.is_set();
            }
        }
    }

    fn poll(&self, ms: libc::c_int) -> bool {
        if self.is_set() {
            return true;
        }
        let mut pfd = libc::pollfd {
            fd: self.read.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `pfd` is one valid pollfd on a live fd owned by `self.read`, and nfds is 1.
        // The byte is never read, so every waiter sees the read end stay readable.
        unsafe { libc::poll(&mut pfd, 1, ms) };
        self.is_set()
    }
}

/// Installs handlers for SIGTERM, SIGINT and SIGHUP (idempotent). Server threads poll the flag or
/// block on it. SA_RESTART keeps the signal from failing other threads' blocking calls.
pub fn install_term_handler() -> io::Result<TermFlag> {
    let mut installed = INSTALLED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(flag) = &*installed {
        return Ok(flag.clone());
    }
    let ctx =
        |what: &str, e: io::Error| io::Error::new(e.kind(), format!("term handler: {what}: {e}"));
    let (read, write) = pipe().map_err(|e| ctx("pipe", e))?;
    set_flags(write.as_raw_fd(), libc::O_NONBLOCK).map_err(|e| ctx("fcntl", e))?;
    // The write end lives for the rest of the process: the handler may run at any time.
    PIPE_WRITE.store(std::os::fd::IntoRawFd::into_raw_fd(write), Ordering::SeqCst);
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        sigaction(sig).map_err(|e| ctx("sigaction", e))?;
    }
    let flag = TermFlag {
        read: Arc::new(read),
    };
    *installed = Some(flag.clone());
    Ok(flag)
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: `fds` is a writable array of two fds, as pipe(2) requires.
    cvt(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
    // SAFETY: pipe(2) succeeded, so both fds are open and owned by nobody else.
    let (r, w) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    set_cloexec(r.as_raw_fd())?;
    set_cloexec(w.as_raw_fd())?;
    Ok((r, w))
}

fn set_cloexec(fd: RawFd) -> io::Result<()> {
    // SAFETY: F_SETFD on an fd the caller owns; no pointers involved.
    cvt(unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) }).map(drop)
}

fn set_flags(fd: RawFd, flags: libc::c_int) -> io::Result<()> {
    // SAFETY: F_GETFL on an fd the caller owns; no pointers involved.
    let old = cvt(unsafe { libc::fcntl(fd, libc::F_GETFL) })?;
    // SAFETY: as above, F_SETFL with an int argument.
    cvt(unsafe { libc::fcntl(fd, libc::F_SETFL, old | flags) }).map(drop)
}

fn sigaction(sig: libc::c_int) -> io::Result<()> {
    // SAFETY: sigaction is plain old data, and all-zero is a valid empty value for it.
    let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
    sa.sa_sigaction = on_term as extern "C" fn(libc::c_int) as libc::sighandler_t;
    sa.sa_flags = libc::SA_RESTART;
    // SAFETY: `sa.sa_mask` is a valid sigset_t owned by this frame.
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };
    // SAFETY: `sa` is fully initialized and `on_term` only touches atomics and calls write(2),
    // which keeps the handler async-signal-safe; a null old-action pointer is allowed.
    cvt(unsafe { libc::sigaction(sig, &sa, std::ptr::null_mut()) }).map(drop)
}

/// Asks the kernel to send `sig` to this process when its parent thread exits.
#[cfg(target_os = "linux")]
pub fn set_pdeathsig(sig: i32) -> io::Result<()> {
    // SAFETY: PR_SET_PDEATHSIG takes the signal as an integer argument; no pointers.
    cvt(unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, sig as libc::c_ulong) }).map(drop)
}

/// Makes the child spawned from `cmd` receive `sig` when the spawning **thread** exits (Linux
/// semantics), so spawn long-lived children from a thread that lives as long as they should.
#[cfg(target_os = "linux")]
pub fn pdeathsig_on_spawn(cmd: &mut Command, sig: i32) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    let parent = std::process::id() as libc::pid_t;
    let hook = move || {
        set_pdeathsig(sig)?;
        // SAFETY: getppid(2) takes no arguments and cannot fail.
        if unsafe { libc::getppid() } != parent {
            return Err(io::Error::from_raw_os_error(libc::ESRCH));
        }
        Ok(())
    };
    // SAFETY: the hook runs between fork and exec, where only async-signal-safe calls are allowed:
    // it calls prctl(2) and getppid(2) and builds io::Errors from raw codes, which do not allocate.
    unsafe { cmd.pre_exec(hook) };
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn set_pdeathsig(_sig: i32) -> io::Result<()> {
    Err(crate::unsupported("PR_SET_PDEATHSIG"))
}

#[cfg(not(target_os = "linux"))]
pub fn pdeathsig_on_spawn(_cmd: &mut Command, _sig: i32) -> io::Result<()> {
    Err(crate::unsupported("PR_SET_PDEATHSIG"))
}
