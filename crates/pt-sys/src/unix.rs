use crate::cred::PeerCred;
use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd};
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
#[cfg(target_os = "linux")]
use std::path::Path;
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

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn set_malloc_arena_max(max: i32) -> bool {
    // SAFETY: mallopt takes integer parameters and does not borrow any memory.
    unsafe { libc::mallopt(libc::M_ARENA_MAX, max) != 0 }
}

/// Waits for a priority change on a file, as used by cgroup event files.
pub fn poll_priority(file: &impl AsFd, timeout: Duration) -> io::Result<bool> {
    let mut pfd = libc::pollfd {
        fd: file.as_fd().as_raw_fd(),
        events: libc::POLLPRI,
        revents: 0,
    };
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: pfd points to one valid pollfd and its borrowed descriptor stays open.
    let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
    if rc < 0 {
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(e);
    }
    Ok(rc > 0)
}

#[cfg(target_os = "linux")]
pub fn is_memory_filesystem(path: &Path) -> io::Result<bool> {
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL"))?;
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: statfs is initialized by the kernel on success, and name is NUL-terminated.
    cvt(unsafe { libc::statfs(name.as_ptr(), stats.as_mut_ptr()) })?;
    // SAFETY: statfs succeeded and initialized stats.
    let kind = unsafe { stats.assume_init() }.f_type as u32;
    Ok(kind == 0x0102_1994 || kind == 0x8584_58f6)
}

#[cfg(not(target_os = "linux"))]
pub fn is_memory_filesystem(_path: &std::path::Path) -> io::Result<bool> {
    Err(crate::unsupported("filesystem type"))
}

/// Sends a signal to a child process group.
pub fn kill_group(pgid: u32, signal: i32) -> io::Result<()> {
    let pgid = libc::pid_t::try_from(pgid)
        .ok()
        .filter(|&p| p > 1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid process group"))?;
    // SAFETY: a negative, validated process group ID addresses that group only.
    cvt(unsafe { libc::kill(-pgid, signal) }).map(|_| ())
}

#[cfg(target_os = "linux")]
/// Connects even when the listener's accept queue is full, up to the deadline.
pub fn connect_unix_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: sockaddr_un is a plain C structure, and zero is a valid initial value.
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.contains(&0) || bytes.len() >= addr.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid unix socket path",
        ));
    }
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (out, byte) in addr.sun_path.iter_mut().zip(bytes) {
        *out = *byte as libc::c_char;
    }
    let len =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    // SAFETY: the arguments specify a new Unix stream socket with no borrowed pointers.
    let fd = cvt(unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    })?;
    // SAFETY: socket returned a new fd, now owned exactly once by OwnedFd.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "unix connect timeout is too large",
        )
    })?;
    loop {
        // SAFETY: fd is live, and addr contains a valid Unix address of len bytes.
        let rc = unsafe { libc::connect(fd.as_raw_fd(), (&raw const addr).cast(), len) };
        if rc == 0 {
            break;
        }
        let e = io::Error::last_os_error();
        // Linux returns EAGAIN for a full Unix listener queue.
        match e.raw_os_error() {
            Some(libc::EISCONN) => break,
            Some(libc::EAGAIN | libc::EINPROGRESS | libc::EALREADY | libc::EINTR) => {}
            _ => return Err(e),
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "unix connect timed out",
            ));
        }
        std::thread::sleep(left.min(Duration::from_millis(20)));
    }
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(false)?;
    Ok(stream)
}

#[cfg(not(target_os = "linux"))]
pub fn connect_unix_timeout(_path: &std::path::Path, _timeout: Duration) -> io::Result<UnixStream> {
    Err(crate::unsupported("unix connect timeout"))
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
/// The pid that installed the handler. A forked child keeps the handler until it execs, but a
/// signal there is the child's own, so the handler gives it the default action instead.
static OWNER: AtomicI32 = AtomicI32::new(0);
static INSTALLED: Mutex<Option<TermFlag>> = Mutex::new(None);

const TERM_SIGNALS: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

extern "C" fn on_term(sig: libc::c_int) {
    let errno = Errno::save();
    // SAFETY: getpid(2) takes no arguments, cannot fail and is async-signal-safe.
    if unsafe { libc::getpid() } != OWNER.load(Ordering::SeqCst) {
        // The signal stays blocked until the handler returns, so the raised copy is delivered
        // then, with the default action: the child terminates as if no handler existed.
        if set_default(sig).is_ok() {
            // SAFETY: raise(3) takes a plain signal number and is async-signal-safe.
            unsafe { libc::raise(sig) };
        }
        errno.restore();
        return;
    }
    let _ = SIGNAL.compare_exchange(0, sig, Ordering::SeqCst, Ordering::SeqCst);
    wake();
    errno.restore();
}

/// Makes the self-pipe readable. write(2) is async-signal-safe; the fd is the nonblocking write
/// end, stored before the handler was installed and never closed. A full pipe drops the byte,
/// which is fine because one unread byte keeps the read end readable.
fn wake() {
    let fd = PIPE_WRITE.load(Ordering::SeqCst);
    let byte = 1u8;
    // SAFETY: as above; `byte` is a live local and the length is 1.
    unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
}

/// errno of the interrupted code, which a handler must hand back unchanged.
struct Errno(libc::c_int);

impl Errno {
    fn save() -> Errno {
        Errno(io::Error::last_os_error().raw_os_error().unwrap_or(0))
    }

    fn restore(self) {
        // SAFETY: the errno location is this thread's own errno, valid for the thread's life.
        unsafe { *errno_location() = self.0 };
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn errno_location() -> *mut libc::c_int {
    // SAFETY: __errno_location takes no arguments and returns this thread's errno.
    unsafe { libc::__errno_location() }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn errno_location() -> *mut libc::c_int {
    // SAFETY: __error takes no arguments and returns this thread's errno.
    unsafe { libc::__error() }
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
        // Once the flag is set the byte is never read, so every waiter sees the read end stay
        // readable.
        let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
        if self.is_set() {
            return true;
        }
        if rc > 0 {
            // A byte without the flag: drain it so the next poll sleeps. The flag is set before
            // a real byte is written, so a real byte drained here is put back for other waiters.
            self.drain();
            if self.is_set() {
                wake();
            }
        }
        self.is_set()
    }

    fn drain(&self) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: `buf` is a writable local of the length passed; the read end is
            // nonblocking, so this returns -1 with EAGAIN once the pipe is empty.
            let n =
                unsafe { libc::read(self.read.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return;
            }
        }
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
    set_flags(read.as_raw_fd(), libc::O_NONBLOCK).map_err(|e| ctx("fcntl", e))?;
    // The write end lives for the rest of the process: the handler may run at any time.
    PIPE_WRITE.store(std::os::fd::IntoRawFd::into_raw_fd(write), Ordering::SeqCst);
    // SAFETY: getpid(2) takes no arguments and cannot fail.
    OWNER.store(unsafe { libc::getpid() }, Ordering::SeqCst);
    for sig in TERM_SIGNALS {
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
    // SAFETY: `sa` is fully initialized and `on_term` only touches atomics and errno and calls
    // getpid(2), sigaction(2), raise(3) and write(2), which keeps the handler
    // async-signal-safe; a null old-action pointer is allowed.
    cvt(unsafe { libc::sigaction(sig, &sa, std::ptr::null_mut()) }).map(drop)
}

/// Restores the default SIGPIPE, which Rust's runtime ignores, so a short-lived command whose
/// output pipe closes (`| head`) ends quietly like other Unix tools instead of panicking in a
/// print. Never for a server, which must keep getting EPIPE from a dead peer.
pub fn default_sigpipe() -> io::Result<()> {
    set_default(libc::SIGPIPE)
}

/// Restores the default action for `sig`. Async-signal-safe: it only calls sigaction(2).
fn set_default(sig: libc::c_int) -> io::Result<()> {
    // SAFETY: sigaction is plain old data; all-zero is an empty mask, no flags and SIG_DFL.
    let sa: libc::sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: `sa` is a valid SIG_DFL action; a null old-action pointer is allowed.
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
        // The parent's handlers stay installed until exec, so a death signal arriving before it
        // would run them instead of ending the child.
        let own = Some(sig).filter(|&s| s != libc::SIGKILL && s != libc::SIGSTOP);
        for s in TERM_SIGNALS.into_iter().chain(own) {
            set_default(s)?;
        }
        set_pdeathsig(sig)?;
        // SAFETY: getppid(2) takes no arguments and cannot fail.
        if unsafe { libc::getppid() } != parent {
            return Err(io::Error::from_raw_os_error(libc::ESRCH));
        }
        Ok(())
    };
    // SAFETY: the hook runs between fork and exec, where only async-signal-safe calls are allowed:
    // it calls sigaction(2), prctl(2) and getppid(2) and builds io::Errors from raw codes, which
    // do not allocate.
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
