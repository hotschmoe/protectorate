//! The server: accept loop, one thread per connection, keep-alive and graceful shutdown.

use crate::wire::{self, ChunkedWriter, Error, Framing, Stream, Timed};
use crate::{Body, Peer, Request, Response, WebSocket, ws};
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

pub type Handler = Arc<dyn Fn(&Request, &Peer) -> Response + Send + Sync>;

/// Limits and timeouts. Every limit answers with its own 4xx.
#[derive(Debug, Clone)]
pub struct Config {
    /// Connections beyond this are answered 503 and closed.
    pub max_connections: usize,
    /// Request line plus header fields (431).
    pub max_head: usize,
    /// Request body (413).
    pub max_body: usize,
    /// From the first byte of a request to the end of its head (408).
    pub header_timeout: Duration,
    /// Reading the body (408).
    pub body_timeout: Duration,
    /// Waiting for the next request on a kept-alive connection.
    pub idle_timeout: Duration,
    pub write_timeout: Duration,
    pub ws_max_message: usize,
    /// Ping a WebSocket peer after this long without a frame; drop it if this long passes again
    /// without a pong (other frames do not count), or if one frame takes longer than this.
    pub ws_ping_interval: Option<Duration>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_connections: 256,
            max_head: 16 << 10,
            max_body: 4 << 20,
            header_timeout: Duration::from_secs(10),
            body_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(30),
            ws_max_message: 16 << 20,
            ws_ping_interval: Some(Duration::from_secs(30)),
        }
    }
}

enum Listener {
    Tcp(TcpListener),
    Unix(UnixListener),
}

const IDLE: u8 = 0;
const BUSY: u8 = 1;
const WS: u8 = 2;

struct Slot {
    stream: Stream,
    phase: AtomicU8,
}

struct Shared {
    config: Config,
    handler: Handler,
    stopping: AtomicBool,
    deadline: Mutex<Option<Instant>>,
    conns: Mutex<HashMap<u64, Arc<Slot>>>,
    wake: Wake,
}

/// Where to connect to wake the blocking accept. Not a listener clone, which would keep the
/// socket accepting after `run` drops its listener.
enum Wake {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl Shared {
    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    fn conns(&self) -> MutexGuard<'_, HashMap<u64, Arc<Slot>>> {
        self.conns.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct Server {
    listener: Listener,
    shared: Arc<Shared>,
}

/// Stops a running server from another thread.
#[derive(Clone)]
pub struct ShutdownHandle(Arc<Shared>);

impl ShutdownHandle {
    /// Stops accepting at once and returns. `run` then closes idle connections and WebSockets,
    /// lets in-flight requests finish until `grace` passes, closes the rest and returns.
    pub fn shutdown(&self, grace: Duration) {
        *self.0.deadline.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + grace);
        self.0.stopping.store(true, Ordering::SeqCst);
        // Wake the blocking accept with a connection of our own.
        let _ = match &self.0.wake {
            Wake::Tcp(addr) => {
                let mut a = *addr;
                if a.ip().is_unspecified() {
                    a.set_ip(if a.is_ipv4() {
                        [127, 0, 0, 1].into()
                    } else {
                        std::net::Ipv6Addr::LOCALHOST.into()
                    });
                }
                TcpStream::connect_timeout(&a, Duration::from_secs(1)).map(drop)
            }
            Wake::Unix(path) => UnixStream::connect(path).map(drop),
        };
    }
}

type HandlerFn = dyn Fn(&Request, &Peer) -> Response + Send + Sync + 'static;

impl Server {
    pub fn bind_tcp(
        addr: SocketAddr,
        config: Config,
        handler: impl Fn(&Request, &Peer) -> Response + Send + Sync + 'static,
    ) -> io::Result<Server> {
        let l = TcpListener::bind(addr)
            .map_err(|e| io::Error::new(e.kind(), format!("bind {addr}: {e}")))?;
        let wake = Wake::Tcp(l.local_addr()?);
        Ok(Server::new(
            Listener::Tcp(l),
            wake,
            config,
            Arc::new(handler),
        ))
    }

    /// Binds a unix socket at `path` with mode 0600. The path must not exist.
    pub fn bind_unix(
        path: &Path,
        config: Config,
        handler: impl Fn(&Request, &Peer) -> Response + Send + Sync + 'static,
    ) -> io::Result<Server> {
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("bind {}: {e}", path.display()));
        let l = UnixListener::bind(path).map_err(ctx)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(ctx)?;
        Ok(Server::new(
            Listener::Unix(l),
            Wake::Unix(path.to_path_buf()),
            config,
            Arc::new(handler),
        ))
    }

    fn new(listener: Listener, wake: Wake, config: Config, handler: Arc<HandlerFn>) -> Server {
        let shared = Shared {
            config,
            handler,
            stopping: AtomicBool::new(false),
            deadline: Mutex::new(None),
            conns: Mutex::new(HashMap::new()),
            wake,
        };
        Server {
            listener,
            shared: Arc::new(shared),
        }
    }

    /// The bound TCP address (useful after binding port 0).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        match &self.listener {
            Listener::Tcp(l) => l.local_addr().ok(),
            Listener::Unix(_) => None,
        }
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle(self.shared.clone())
    }

    /// Accepts until shut down, then drains (see [`ShutdownHandle::shutdown`]).
    pub fn run(self) -> io::Result<()> {
        let mut next_id = 0u64;
        while !self.shared.stopping() {
            let accepted = match &self.listener {
                Listener::Tcp(l) => l.accept().map(|(s, a)| {
                    let _ = s.set_nodelay(true);
                    (Stream::Tcp(s), Ok(Peer::Tcp(a)))
                }),
                Listener::Unix(l) => l.accept().map(|(s, _)| {
                    let cred = pt_sys::peer_cred(&s).map(Peer::Unix);
                    (Stream::Unix(s), cred)
                }),
            };
            if self.shared.stopping() {
                break;
            }
            match accepted {
                Ok((stream, Ok(peer))) => {
                    next_id += 1;
                    self.spawn(stream, peer, next_id);
                }
                // The peer's credentials are unknown, so it is refused.
                Ok((_, Err(_))) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) => {}
                // Out of fds or memory: back off instead of spinning.
                Err(_) => thread::sleep(Duration::from_millis(20)),
            }
        }
        let Server { listener, shared } = self;
        drop(listener);
        drain(&shared);
        Ok(())
    }

    fn spawn(&self, stream: Stream, peer: Peer, id: u64) {
        let Ok(clone) = stream.try_clone() else {
            return;
        };
        let slot = Arc::new(Slot {
            stream: clone,
            phase: AtomicU8::new(BUSY),
        });
        {
            let mut conns = self.shared.conns();
            if conns.len() >= self.shared.config.max_connections {
                drop(conns);
                let mut stream = stream;
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                let _ = error_response(&mut stream, 503, "too many connections");
                return;
            }
            conns.insert(id, slot.clone());
        }
        // Moved into the thread, so the slot is released however it ends: by returning, by
        // unwinding from a panicking handler, or by never starting.
        let guard = SlotGuard(self.shared.clone(), id);
        let _ = thread::Builder::new()
            .name("http-conn".into())
            .spawn(move || {
                let guard = guard;
                serve(&guard.0, stream, &peer, &slot);
            });
    }
}

struct SlotGuard(Arc<Shared>, u64);

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.0.conns().remove(&self.1);
    }
}

fn drain(shared: &Shared) {
    let deadline = shared
        .deadline
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_else(Instant::now);
    let poll = Duration::from_millis(10);
    while Instant::now() < deadline && !shared.conns().is_empty() {
        for slot in shared.conns().values() {
            if slot.phase.load(Ordering::SeqCst) != BUSY {
                slot.stream.shutdown(Shutdown::Read);
            }
        }
        thread::sleep(poll);
    }
    for slot in shared.conns().values() {
        slot.stream.shutdown(Shutdown::Both);
    }
    // Threads leave promptly once their sockets are shut; a handler stuck elsewhere is abandoned.
    let abandon = Instant::now() + Duration::from_secs(1);
    while Instant::now() < abandon && !shared.conns().is_empty() {
        thread::sleep(poll);
    }
}

enum Next {
    Again,
    Close,
    Upgrade(crate::WebSocketFn),
}

fn serve(shared: &Shared, stream: Stream, peer: &Peer, slot: &Slot) {
    let cfg = &shared.config;
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let _ = out.set_write_timeout(Some(cfg.write_timeout));
    let mut reader = BufReader::with_capacity(
        8 << 10,
        Timed {
            stream,
            deadline: None,
        },
    );
    let mut wait = cfg.header_timeout;
    loop {
        slot.phase.store(IDLE, Ordering::SeqCst);
        if shared.stopping() {
            return;
        }
        reader.get_mut().deadline = Some(Instant::now() + wait);
        if !matches!(io::BufRead::fill_buf(&mut reader), Ok(b) if !b.is_empty()) {
            return;
        }
        slot.phase.store(BUSY, Ordering::SeqCst);
        wait = cfg.idle_timeout;
        match handle(shared, &mut reader, &mut out, peer) {
            Ok(Next::Again) => {}
            Ok(Next::Close) | Err(Error::Io(_)) => return,
            Ok(Next::Upgrade(f)) => {
                slot.phase.store(WS, Ordering::SeqCst);
                if shared.stopping() {
                    return;
                }
                if let Ok(ws) =
                    WebSocket::new(reader, false, cfg.ws_max_message, cfg.ws_ping_interval)
                {
                    let writer = ws.writer();
                    f(ws);
                    writer.shutdown();
                }
                return;
            }
            Err(Error::Status(status, why)) => {
                if error_response(&mut out, status, why).is_ok() {
                    // Read what the client is still sending, so closing does not reset the
                    // connection before it reads the error.
                    out.shutdown(Shutdown::Write);
                    reader.get_mut().deadline = Some(Instant::now() + Duration::from_secs(1));
                    let _ = io::copy(
                        &mut reader.by_ref().take(cfg.max_body as u64 + (1 << 20)),
                        &mut io::sink(),
                    );
                }
                return;
            }
        }
    }
}

fn handle(
    shared: &Shared,
    reader: &mut BufReader<Timed>,
    out: &mut Stream,
    peer: &Peer,
) -> Result<Next, Error> {
    let cfg = &shared.config;
    reader.get_mut().deadline = Some(Instant::now() + cfg.header_timeout);
    let head = wire::read_head(reader, cfg.max_head)?.ok_or(Error::Status(400, "empty request"))?;
    let mut req = wire::parse_request(head)?;
    let framing = wire::framing(&req.headers)?;
    if let Some(expect) = req.header("Expect") {
        if !expect.eq_ignore_ascii_case("100-continue") {
            return Err(Error::Status(417, "unsupported expectation"));
        }
        if matches!(framing, Framing::Length(n) if n > cfg.max_body as u64) {
            return Err(Error::Status(413, "content too large"));
        }
        out.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
    }
    reader.get_mut().deadline = Some(Instant::now() + cfg.body_timeout);
    req.body = wire::read_body(reader, framing, cfg.max_body, cfg.max_head)?;
    let keep = !req.http10 && !req.headers.has_token("Connection", "close");

    let mut resp = (shared.handler)(&req, peer);
    for h in [
        "Content-Length",
        "Transfer-Encoding",
        "Connection",
        "Upgrade",
        "Sec-WebSocket-Accept",
    ] {
        resp.headers.remove(h);
    }
    if let Body::WebSocket(_) = resp.body {
        let accept = match ws::check_handshake(&req) {
            Ok(accept) => accept,
            Err(refusal) => {
                return write_response(out, &req, refusal, false)
                    .map(|()| Next::Close)
                    .map_err(Error::Io);
            }
        };
        let upgrade = format!(
            "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n"
        );
        out.write_all(start(101, &resp.headers, &upgrade).as_bytes())?;
        let Body::WebSocket(f) = resp.body else {
            unreachable!()
        };
        return Ok(Next::Upgrade(f));
    }
    let keep = keep && !shared.stopping();
    write_response(out, &req, resp, keep)?;
    Ok(if keep { Next::Again } else { Next::Close })
}

fn start(status: u16, headers: &crate::Headers, framing: &str) -> String {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", wire::reason(status));
    for (n, v) in headers.iter() {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    head + framing + "\r\n"
}

fn write_response(out: &mut Stream, req: &Request, resp: Response, keep: bool) -> io::Result<()> {
    let no_body = matches!(resp.status, 100..200 | 204 | 304);
    let mut framing = match &resp.body {
        _ if no_body => String::new(),
        Body::Bytes(b) => format!("Content-Length: {}\r\n", b.len()),
        Body::File(_, n) => format!("Content-Length: {n}\r\n"),
        Body::Stream(_) if !req.http10 => "Transfer-Encoding: chunked\r\n".into(),
        Body::Stream(_) | Body::WebSocket(_) => String::new(),
    };
    if !keep {
        framing.push_str("Connection: close\r\n");
    }
    out.write_all(start(resp.status, &resp.headers, &framing).as_bytes())?;
    if no_body || req.method == "HEAD" {
        return Ok(());
    }
    match resp.body {
        Body::Bytes(b) => out.write_all(&b),
        Body::File(f, n) => match io::copy(&mut f.take(n), out)? {
            sent if sent == n => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "file shrank while sending",
            )),
        },
        Body::WebSocket(_) => Ok(()),
        Body::Stream(f) if req.http10 => f(out),
        Body::Stream(f) => {
            let mut chunked = ChunkedWriter(out);
            f(&mut chunked)?;
            chunked.finish()
        }
    }
}

fn error_response(out: &mut Stream, status: u16, why: &str) -> io::Result<()> {
    let mut headers = crate::Headers::default();
    headers.add("Content-Type", "text/plain; charset=utf-8");
    let framing = format!("Content-Length: {}\r\nConnection: close\r\n", why.len() + 1);
    out.write_all((start(status, &headers, &framing) + why + "\n").as_bytes())
}
