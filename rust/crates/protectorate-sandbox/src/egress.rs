//! Sandboxed workloads get their own network namespace with only loopback.
//! Their way out is one host-side HTTP CONNECT proxy per sandbox, reached
//! through a Unix socket bound into the sandbox and a relay on 127.0.0.1 that
//! the helper runs inside the namespace. The proxy admits an allowlisted
//! host name, resolves it on the host and connects only to addresses its
//! rule permits. It never terminates TLS; it sees only the CONNECT target.
use crate::{public_address, EgressPolicy};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    os::{
        fd::AsRawFd,
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// The loopback port the workload's proxy environment names.
pub const PROXY_PORT: u16 = 3128;
pub(crate) const SOCKET_DIR: &str = "/run/protectorate/egress";
const MAX_HEAD: usize = 8 << 10;
const MAX_CONNECTIONS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    /// Admitted by policy, but the destination could not be reached.
    Error,
}

/// One CONNECT outcome, reported to the launcher's egress log.
#[derive(Clone, Debug)]
pub struct Decision {
    pub host: String,
    pub port: u16,
    pub verdict: Verdict,
    pub reason: String,
}

pub type Log = Arc<dyn Fn(&Decision) + Send + Sync>;

/// The host side of one sandbox's egress: a CONNECT listener, plus a plain
/// tunnel to the hub when the hub listens on loopback, where the namespace
/// relay reproduces its port.
pub(crate) struct Proxy {
    dir: tempfile::TempDir,
    pub(crate) sockets: Vec<(PathBuf, PathBuf)>,
    pub(crate) relays: Vec<(u16, PathBuf)>,
    stop: Arc<AtomicBool>,
    listeners: Vec<(UnixListener, JoinHandle<()>)>,
}

impl Proxy {
    pub(crate) fn start(
        policy: &EgressPolicy,
        runtime: &Path,
        log: Option<Log>,
    ) -> io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("egress-")
            .tempdir_in(runtime)?;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
        let stop = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let mut proxy = Self {
            dir,
            sockets: Vec::new(),
            relays: Vec::new(),
            stop,
            listeners: Vec::new(),
        };
        let hub_tunnel = policy
            .hub()
            .filter(|(host, _)| {
                *host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
            })
            .map(|(host, port)| (host.to_owned(), port));
        let proxy_port = if hub_tunnel
            .as_ref()
            .is_some_and(|(_, port)| *port == PROXY_PORT)
        {
            PROXY_PORT + 1
        } else {
            PROXY_PORT
        };
        let policy = Arc::new(policy.clone());
        let log = log.unwrap_or_else(|| Arc::new(|_: &Decision| {}));
        {
            let (policy, log, active) =
                (Arc::clone(&policy), Arc::clone(&log), Arc::clone(&active));
            proxy.listen("proxy", proxy_port, move |stream| {
                serve_connect(stream, &policy, &log, &active)
            })?;
        }
        if let Some((host, port)) = hub_tunnel {
            proxy.listen("hub", port, move |stream| {
                let addresses = (host.as_str(), port)
                    .to_socket_addrs()
                    .map(Vec::from_iter)
                    .unwrap_or_default();
                if let Ok(upstream) = connect_any(&addresses) {
                    splice(stream, upstream);
                }
            })?;
        }
        Ok(proxy)
    }

    fn listen(
        &mut self,
        name: &str,
        port: u16,
        serve: impl Fn(UnixStream) + Send + Sync + 'static,
    ) -> io::Result<()> {
        let path = self.dir.path().join(name);
        let listener = UnixListener::bind(&path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("bind egress socket {}: {e}", path.display()),
            )
        })?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let accept = listener.try_clone()?;
        let stop = Arc::clone(&self.stop);
        let serve = Arc::new(serve);
        let thread = thread::Builder::new()
            .name(format!("egress-{name}"))
            .spawn(move || {
                for stream in accept.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else {
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    };
                    let serve = Arc::clone(&serve);
                    let _ = thread::Builder::new()
                        .name("egress-conn".into())
                        .spawn(move || serve(stream));
                }
            })?;
        let inside = Path::new(SOCKET_DIR).join(name);
        self.sockets.push((path, inside.clone()));
        self.relays.push((port, inside));
        self.listeners.push((listener, thread));
        Ok(())
    }

    /// The sandbox holds its own bind of each socket; the host names are no
    /// longer needed once setup finishes, and must not outlive a supervisor
    /// that is killed without running its destructors.
    pub(crate) fn unlink(&self) -> io::Result<()> {
        for (path, _) in &self.sockets {
            std::fs::remove_file(path)?;
        }
        std::fs::remove_dir(self.dir.path())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for (listener, thread) in self.listeners.drain(..) {
            // SAFETY: the listener descriptor stays open until after the join.
            unsafe { libc::shutdown(listener.as_raw_fd(), libc::SHUT_RDWR) };
            let _ = thread.join();
        }
    }
}

fn respond(stream: &mut UnixStream, status: &str, reason: &str) {
    let body = format!("protectorate egress: {reason}\n");
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn serve_connect(mut stream: UnixStream, policy: &EgressPolicy, log: &Log, active: &AtomicUsize) {
    struct Slot<'a>(&'a AtomicUsize);
    impl Drop for Slot<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let _slot = Slot(active);
    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
        return respond(
            &mut stream,
            "503 Service Unavailable",
            "too many open connections",
        );
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut head = Vec::new();
    let mut buffer = [0u8; 2048];
    let end = loop {
        if let Some(end) = head.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
        if head.len() > MAX_HEAD {
            return respond(
                &mut stream,
                "431 Request Header Fields Too Large",
                "request head too large",
            );
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buffer[..n]),
        }
    };
    let line = String::from_utf8_lossy(&head[..head.iter().position(|b| *b == b'\r').unwrap_or(0)])
        .into_owned();
    let mut parts = line.split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let decide = |host: &str, port: u16, verdict, reason: String| {
        log(&Decision {
            host: host.chars().take(256).collect(),
            port,
            verdict,
            reason,
        })
    };
    if method != "CONNECT" {
        let reason = format!(
            "only CONNECT is supported, not {}",
            method.chars().take(16).collect::<String>()
        );
        decide(target, 0, Verdict::Deny, reason.clone());
        return respond(&mut stream, "405 Method Not Allowed", &reason);
    }
    let Some((host, port)) = target
        .rsplit_once(':')
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
    else {
        decide(target, 0, Verdict::Deny, "malformed CONNECT target".into());
        return respond(&mut stream, "400 Bad Request", "CONNECT needs host:port");
    };
    let admission = match policy.admit(host, port) {
        Ok(admission) => admission,
        Err(reason) => {
            decide(host, port, Verdict::Deny, reason.clone());
            return respond(&mut stream, "403 Forbidden", &reason);
        }
    };
    let name = host.trim_start_matches('[').trim_end_matches(']');
    let resolved = match (name, port).to_socket_addrs() {
        Ok(addresses) => addresses.collect::<Vec<_>>(),
        Err(error) => {
            let reason = format!("{} rule; DNS: {error}", admission.rule);
            decide(host, port, Verdict::Error, reason.clone());
            return respond(&mut stream, "502 Bad Gateway", &reason);
        }
    };
    let permitted = resolved
        .iter()
        .copied()
        .filter(|address| {
            let ip = address.ip();
            public_address(ip) || (admission.private && !ip.is_unspecified() && !ip.is_multicast())
        })
        .collect::<Vec<_>>();
    if permitted.is_empty() {
        let shown = resolved
            .iter()
            .map(|a| a.ip().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let reason = format!("{} resolves only to refused addresses ({shown})", host);
        decide(host, port, Verdict::Deny, reason.clone());
        return respond(&mut stream, "403 Forbidden", &reason);
    }
    let upstream = match connect_any(&permitted) {
        Ok(upstream) => upstream,
        Err(error) => {
            let reason = format!("{} rule; connect: {error}", admission.rule);
            decide(host, port, Verdict::Error, reason.clone());
            return respond(&mut stream, "502 Bad Gateway", &reason);
        }
    };
    let address = upstream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    decide(
        host,
        port,
        Verdict::Allow,
        format!("{} rule; {address}", admission.rule),
    );
    let _ = stream.set_read_timeout(None);
    if stream
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
        || (&upstream).write_all(&head[end..]).is_err()
    {
        return;
    }
    splice(stream, upstream);
}

fn connect_any(addresses: &[SocketAddr]) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for address in addresses.iter().take(4) {
        match TcpStream::connect_timeout(address, Duration::from_secs(10)) {
            Ok(stream) => return Ok(stream),
            Err(error) => last = error,
        }
    }
    Err(last)
}

pub(crate) trait Duplex: Read + Write + Send + Sized + 'static {
    fn duplicate(&self) -> io::Result<Self>;
    fn close_write(&self);
}
impl Duplex for TcpStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}
impl Duplex for UnixStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

/// Copy both directions until each side finishes, preserving half-close.
pub(crate) fn splice<A: Duplex, B: Duplex>(mut a: A, mut b: B) {
    let (Ok(mut a2), Ok(mut b2)) = (a.duplicate(), b.duplicate()) else {
        return;
    };
    let forward = thread::spawn(move || {
        let _ = io::copy(&mut a2, &mut b2);
        b2.close_write();
    });
    let _ = io::copy(&mut b, &mut a);
    a.close_write();
    let _ = forward.join();
}

/// Bring up loopback in the helper's fresh network namespace. Requires the
/// capabilities the helper holds in its own user namespace.
pub(crate) fn loopback_up() -> io::Result<()> {
    // SAFETY: plain socket and interface ioctls on a zeroed ifreq naming "lo".
    unsafe {
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if socket < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut request: libc::ifreq = std::mem::zeroed();
        for (slot, byte) in request.ifr_name.iter_mut().zip(b"lo") {
            *slot = *byte as libc::c_char;
        }
        let result = if libc::ioctl(socket, libc::SIOCGIFFLAGS, &mut request) < 0 {
            -1
        } else {
            request.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
            libc::ioctl(socket, libc::SIOCSIFFLAGS, &request)
        };
        let error = io::Error::last_os_error();
        libc::close(socket);
        if result < 0 {
            return Err(error);
        }
    }
    Ok(())
}

/// Bind each relay port on namespace loopback before the workload starts.
pub(crate) fn bind_relays(relays: &[(u16, PathBuf)]) -> io::Result<Vec<(TcpListener, PathBuf)>> {
    relays
        .iter()
        .map(|(port, socket)| Ok((TcpListener::bind(("127.0.0.1", *port))?, socket.clone())))
        .collect()
}

/// Serve the namespace relays until the workload's PID namespace ends. The
/// sockets resolve inside the sandbox root, which this process inherits once
/// the workload's init has pivoted.
pub(crate) fn serve_relays(listeners: Vec<(TcpListener, PathBuf)>) {
    let threads = listeners
        .into_iter()
        .map(|(listener, socket)| {
            thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let socket = socket.clone();
                    thread::spawn(move || {
                        if let Ok(upstream) = UnixStream::connect(&socket) {
                            splice(stream, upstream);
                        }
                    });
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        let _ = thread.join();
    }
}
