//! `protectorate-node serve`: the node end of the hub link. The tailnet `member` accepts each
//! hub connection and splices it to this agent's unix socket (or loopback TCP port), after one
//! header line naming the peer. Anything but a `tag:hub` peer is closed unanswered; then each
//! connection gets a thread that answers request frames in order. With `--stdio` the hub
//! reaches the agent through sshd instead (see `stdio.rs`).

use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
#[cfg(debug_assertions)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use pt_frame::Kind;
use pt_frame::msg::{
    self, ErrorCode, ErrorReply, HUB_TAG, HelloReply, JobList, JobReply, MAX_HEADER_LINE, Method,
    PeerHeader, Request,
};
use pt_frame::wire::{FrameReader, write_frame};
use pt_json::{FromJson, ToJson};
use pt_node::jobs::{JobError, Options};
use pt_node::presence::Config;
use pt_node::{Detector, Runner, Sampler, inventory};

use crate::flags::{Flags, parse};

const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

pub const USAGE: &str =
    "  serve --state-dir DIR (--forward-socket PATH | --listen-tcp 127.0.0.1:PORT | --stdio)...
        [--presence-interval SECS] [--away-after SECS] [--no-scope] [--idle-exit SECS]
        [--member PATH | --no-member]
                                    serve the hub link: one header line from member, then frames;
                                    once joined (DIR/node.json), also run member (the embedded
                                    one unless --member) and restart it;
                                    --stdio: frames on stdin/stdout for an sshd forced command,
                                    through a resident agent that keeps jobs across sessions;
                                    PROTECTORATE_NODE_TEST_PRESENCE=FILE forces presence (tests)";

struct Node {
    runner: Runner,
    releases: crate::update::Releases,
    sampler: Mutex<Sampler>,
    node_id: String,
    connections: AtomicUsize,
    #[cfg(debug_assertions)]
    dropped_submit: AtomicBool,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let f = parse(args)?;
    if f.stdio {
        return crate::stdio::run(&f);
    }
    serve(f)
}

fn serve(f: Flags) -> Result<(), String> {
    let dir = f.state_dir.clone().ok_or("serve needs --state-dir")?;
    let detector = Arc::new(Mutex::new(Detector::new(Config {
        away_after_secs: f.away_after,
        ..Config::default()
    })));
    let runner = Runner::open(
        &dir,
        detector,
        Options {
            allow_scope: f.allow_scope,
            presence_interval: Duration::from_secs(f.presence_interval),
        },
    )?;
    let node = Arc::new(Node {
        releases: crate::update::Releases::open(&dir),
        node_id: node_id(&dir)?,
        sampler: Mutex::new(Sampler::new()),
        runner,
        connections: AtomicUsize::new(0),
        #[cfg(debug_assertions)]
        dropped_submit: AtomicBool::new(false),
    });
    eprintln!(
        "protectorate-node: node {} journal epoch {}; jobs run in {}",
        node.node_id,
        node.runner.epoch(),
        node.runner.isolation()
    );

    let mut accepts = Vec::new();
    for path in &f.sockets {
        accepts.push(listen_unix(&node, path)?);
    }
    for addr in &f.tcp {
        let l = TcpListener::bind(addr).map_err(|e| format!("listening on {addr}: {e}"))?;
        let bound = l.local_addr().map_err(|e| e.to_string())?;
        eprintln!("protectorate-node: listening on {bound}");
        let node = node.clone();
        accepts.push(thread::spawn(move || {
            accept_loop(&node, || l.accept().map(|(s, a)| (s, a.to_string())))
        }));
    }
    if let Some(secs) = f.idle_exit {
        let node = node.clone();
        thread::spawn(move || idle_exit(&node, Duration::from_secs(secs)));
    }
    if !f.no_member {
        start_member(&f, &dir)?;
    }
    for a in accepts {
        let _ = a.join();
    }
    Ok(())
}

/// Once joined, member runs as this agent's child and forwards to its first listener.
fn start_member(f: &Flags, dir: &Path) -> Result<(), String> {
    let Some(cfg) = crate::member::read_config(dir)? else {
        return match &f.member {
            Some(_) => Err(format!(
                "--member given, but {} has no node.json: run join first",
                dir.display()
            )),
            None => Ok(()),
        };
    };
    let bin = crate::member::member_binary(f.member.as_deref(), dir)?;
    let forward = match (f.sockets.first(), f.tcp.first()) {
        (Some(p), _) => p.display().to_string(),
        (None, Some(a)) => a.to_string(),
        (None, None) => return Err("member needs a listener to forward to".into()),
    };
    eprintln!(
        "protectorate-node: joined {} as {}; running {}",
        cfg.server_url,
        cfg.name,
        bin.display()
    );
    let dir = dir.to_path_buf();
    thread::Builder::new()
        .name("member".into())
        .spawn(move || crate::member::supervise(bin, dir, cfg, forward))
        .map(drop)
        .map_err(|e| format!("starting the member supervisor: {e}"))
}

/// Ends the process after `after` with no running job and no open connection. A connection
/// accepted in the instant before the exit is cut (REVISIT).
fn idle_exit(node: &Node, after: Duration) {
    let mut idle_since = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(200));
        if node.connections.load(Ordering::SeqCst) > 0 || node.runner.running() > 0 {
            idle_since = Instant::now();
        } else if idle_since.elapsed() >= after {
            eprintln!("protectorate-node: idle for {} s, exiting", after.as_secs());
            std::process::exit(0);
        }
    }
}

#[cfg(unix)]
fn listen_unix(node: &Arc<Node>, path: &Path) -> Result<thread::JoinHandle<()>, String> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    let l = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            // A socket left by an agent that died is removed; a live one is never taken over.
            let is_socket = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_socket());
            if !is_socket || UnixStream::connect(path).is_ok() {
                return Err(format!("{}: in use", path.display()));
            }
            fs::remove_file(path)
                .and_then(|()| UnixListener::bind(path))
                .map_err(|e| format!("replacing stale socket {}: {e}", path.display()))?
        }
        Err(e) => return Err(format!("listening on {}: {e}", path.display())),
    };
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("restricting {}: {e}", path.display()))?;
    eprintln!("protectorate-node: listening on {}", path.display());
    let node = node.clone();
    let name = path.display().to_string();
    Ok(thread::spawn(move || {
        accept_loop(&node, || l.accept().map(|(s, _)| (s, name.clone())))
    }))
}

#[cfg(not(unix))]
fn listen_unix(_node: &Arc<Node>, path: &Path) -> Result<thread::JoinHandle<()>, String> {
    Err(format!(
        "--forward-socket {}: unix sockets are not supported here; use --listen-tcp",
        path.display()
    ))
}

fn accept_loop<S: Conn>(node: &Arc<Node>, mut accept: impl FnMut() -> io::Result<(S, String)>) {
    loop {
        match accept() {
            Ok((s, peer)) => {
                // Counted before the thread starts, so the idle exit never races an accept.
                node.connections.fetch_add(1, Ordering::SeqCst);
                let n = node.clone();
                let spawned = thread::Builder::new()
                    .name("conn".into())
                    .spawn(move || connection(&n, s, &peer));
                if let Err(e) = spawned {
                    node.connections.fetch_sub(1, Ordering::SeqCst);
                    eprintln!("protectorate-node: starting a connection thread: {e}");
                }
            }
            Err(e) => {
                eprintln!("protectorate-node: accept: {e}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// The two stream kinds the agent serves.
trait Conn: Read + Write + Send + Sized + 'static {
    fn duplicate(&self) -> io::Result<Self>;
    fn read_timeout(&self, t: Option<Duration>) -> io::Result<()>;
}

impl Conn for TcpStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(t)
    }
}

#[cfg(unix)]
impl Conn for std::os::unix::net::UnixStream {
    fn duplicate(&self) -> io::Result<Self> {
        self.try_clone()
    }
    fn read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(t)
    }
}

fn connection<S: Conn>(node: &Node, mut stream: S, from: &str) {
    if let Err(e) = serve_conn(node, &mut stream) {
        eprintln!("protectorate-node: connection on {from}: {e}");
    }
    node.connections.fetch_sub(1, Ordering::SeqCst);
}

fn serve_conn<S: Conn>(node: &Node, stream: &mut S) -> Result<(), String> {
    stream
        .read_timeout(Some(HEADER_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let (header, rest) = read_header(stream)?;
    if !header.tags.iter().any(|t| t == HUB_TAG) {
        return Err(format!(
            "refused peer {:?} at {:?} with tags {:?}: not {HUB_TAG}",
            header.peer_name, header.ip, header.tags
        ));
    }
    stream
        .read_timeout(Some(IDLE_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut reader = FrameReader::new(stream.duplicate().map_err(|e| e.to_string())?, &rest);
    let mut w = BufWriter::new(stream.duplicate().map_err(|e| e.to_string())?);
    loop {
        let frame = match reader.read() {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(());
            }
            Err(e) => return Err(format!("reading a frame: {e}")),
        };
        if frame.kind != Kind::Request {
            return Err(format!("protocol: a {:?} frame from the hub", frame.kind));
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("PROTECTORATE_NODE_TEST_DROP_SUBMIT")
            .is_some_and(|p| Path::new(&p).is_file())
            && matches!(parse_request(&frame.payload), Ok(Request::SubmitJob(_)))
            && !node.dropped_submit.swap(true, Ordering::SeqCst)
        {
            answer(node, frame.id, &frame.payload, &mut io::sink())
                .map_err(|e| format!("answering dropped submit: {e}"))?;
            eprintln!("protectorate-node: dropped a submit reply for testing");
            return Ok(());
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("PROTECTORATE_NODE_TEST_SLOW_PRESENCE")
            .and_then(|p| fs::read_to_string(p).ok())
            .is_some_and(|s| s.trim() == "1")
            && matches!(parse_request(&frame.payload), Ok(Request::Presence))
        {
            let mut response = Vec::new();
            answer(node, frame.id, &frame.payload, &mut response)
                .map_err(|e| format!("preparing slow presence: {e}"))?;
            for byte in response {
                w.write_all(&[byte])
                    .and_then(|()| w.flush())
                    .map_err(|e| format!("sending slow presence: {e}"))?;
                thread::sleep(Duration::from_millis(250));
            }
            continue;
        }
        let non_hello =
            matches!(parse_request(&frame.payload), Ok(r) if !matches!(r, Request::Hello(_)));
        let success = answer(node, frame.id, &frame.payload, &mut w)
            .map_err(|e| format!("answering request {}: {e}", frame.id))?;
        w.flush()
            .map_err(|e| format!("flushing request {}: {e}", frame.id))?;
        if non_hello && success {
            node.releases.contact();
        }
    }
}

/// The header line and any bytes after it, which already belong to the frame stream.
fn read_header(r: &mut impl Read) -> Result<(PeerHeader, Vec<u8>), String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    let end = loop {
        if let Some(i) = buf.iter().position(|&b| b == b'\n') {
            break i;
        }
        if buf.len() >= MAX_HEADER_LINE {
            return Err(format!("header line longer than {MAX_HEADER_LINE} bytes"));
        }
        let want = chunk.len().min(MAX_HEADER_LINE + 1 - buf.len());
        match r.read(&mut chunk[..want]) {
            Ok(0) => return Err("closed before the header line ended".into()),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("reading the header line: {e}")),
        }
    };
    let header =
        pt_json::from_bytes::<PeerHeader>(&buf[..end]).map_err(|e| format!("header line: {e}"))?;
    Ok((header, buf[end + 1..].to_vec()))
}

fn reported_protocol() -> u32 {
    #[cfg(debug_assertions)]
    if let Some(protocol) = std::env::var("PROTECTORATE_NODE_TEST_PROTOCOL")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return protocol;
    }
    msg::PROTOCOL
}

fn reported_version() -> String {
    #[cfg(debug_assertions)]
    if let Ok(version) = std::env::var("PROTECTORATE_NODE_TEST_VERSION") {
        return version;
    }
    pt_version::VERSION.into()
}

struct ReplyWriter<'a, W> {
    inner: &'a mut W,
    success: bool,
}

impl<W: Write> Write for ReplyWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn answer(node: &Node, id: u64, payload: &[u8], w: &mut impl Write) -> io::Result<bool> {
    let mut reply = ReplyWriter {
        inner: w,
        success: true,
    };
    answer_inner(node, id, payload, &mut reply)?;
    Ok(reply.success)
}

fn answer_inner(
    node: &Node,
    id: u64,
    payload: &[u8],
    w: &mut ReplyWriter<'_, impl Write>,
) -> io::Result<()> {
    let req = match parse_request(payload) {
        Ok(r) => r,
        Err((code, message)) => return error(w, id, code, message, None),
    };
    let epoch = node.runner.epoch().to_string();
    let r = &node.runner;
    let job_result = |w: &mut _, res: Result<msg::JobInfo, JobError>| match res {
        Ok(info) => respond(
            w,
            id,
            &JobReply {
                journal_epoch: epoch.clone(),
                job: Some(info),
            },
        ),
        Err(e) => error(w, id, e.code, e.message, Some(epoch.clone())),
    };
    match req {
        Request::Hello(_) => respond(
            w,
            id,
            &HelloReply {
                protocol: reported_protocol(),
                node_version: reported_version(),
                node_id: node.node_id.clone(),
                journal_epoch: epoch.clone(),
                hostname: pt_node::hostname(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                release: node.releases.started(),
            },
        ),
        Request::Inventory => respond(w, id, &inventory::collect()),
        Request::Sample => {
            let s = node
                .sampler
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .sample();
            respond(w, id, &s)
        }
        Request::Presence => respond(w, id, &r.detect()),
        Request::SubmitJob(spec) => {
            let (job_id, attempt) = (spec.job_id.clone(), spec.attempt);
            let res = node.releases.submit(r, spec);
            match &res {
                Ok(info) => eprintln!(
                    "protectorate-node: job {job_id} attempt {attempt}: {}",
                    info.state
                ),
                Err(e) => eprintln!(
                    "protectorate-node: job {job_id} attempt {attempt} refused: {}",
                    e.message
                ),
            }
            job_result(w, res)
        }
        Request::JobStatus(q) => respond(
            w,
            id,
            &JobReply {
                journal_epoch: epoch.clone(),
                job: r.status(&q.job_id, q.attempt),
            },
        ),
        Request::FetchLog(q) => {
            match r.read_log(&q, |chunk| write_frame(w, Kind::Chunk, id, chunk)) {
                Ok(end) => write_frame(w, Kind::End, id, &msg::payload(&end)),
                Err(e) => error(w, id, e.code, e.message, Some(epoch.clone())),
            }
        }
        Request::FetchArtifact(q) => {
            match r.read_artifact(&q, |chunk| write_frame(w, Kind::Chunk, id, chunk)) {
                Ok(end) => write_frame(w, Kind::End, id, &msg::payload(&end)),
                Err(e) => {
                    eprintln!(
                        "protectorate-node: job {} attempt {} artifact {:?} refused: {}",
                        q.job_id, q.attempt, q.path, e.message
                    );
                    error(w, id, e.code, e.message, Some(epoch.clone()))
                }
            }
        }
        Request::CancelJob(j) => job_result(w, r.cancel(&j.job_id, j.attempt, j.token.as_deref())),
        Request::AckCollected(j) => job_result(w, r.ack(&j.job_id, j.attempt, j.token.as_deref())),
        Request::ListJobs => respond(
            w,
            id,
            &JobList {
                journal_epoch: epoch.clone(),
                jobs: r.list(),
            },
        ),
        Request::Update(u) => match node.releases.update(&u, r) {
            Ok(reply) => respond(w, id, &reply),
            Err((code, message)) => {
                eprintln!(
                    "protectorate-node: update to {} refused: {message}",
                    u.version
                );
                error(w, id, code, message, None)
            }
        },
    }
}

fn parse_request(payload: &[u8]) -> Result<Request, (ErrorCode, String)> {
    let v = pt_json::parse_bytes(payload).map_err(|e| (ErrorCode::BadRequest, e.to_string()))?;
    if let Some(t) = v.get("type").and_then(|t| t.as_str())
        && t.parse::<Method>().is_err()
    {
        let shown: String = t.chars().take(64).collect();
        return Err((
            ErrorCode::UnknownMethod,
            format!("unknown method {shown:?}"),
        ));
    }
    Request::from_json(&v).map_err(|e| (ErrorCode::BadRequest, e.to_string()))
}

fn respond(w: &mut impl Write, id: u64, v: &impl ToJson) -> io::Result<()> {
    write_frame(w, Kind::Response, id, &msg::payload(v))
}

fn error(
    w: &mut ReplyWriter<'_, impl Write>,
    id: u64,
    code: ErrorCode,
    message: String,
    journal_epoch: Option<String>,
) -> io::Result<()> {
    w.success = false;
    let e = ErrorReply {
        code,
        message,
        journal_epoch,
    };
    write_frame(w, Kind::Error, id, &msg::payload(&e))
}

/// A stable id for this node, made once per state directory.
fn node_id(dir: &Path) -> Result<String, String> {
    let path = dir.join("node-id");
    match fs::read_to_string(&path) {
        Ok(s) if s.trim().len() == 32 && s.trim().bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(s.trim().to_string())
        }
        Ok(_) => Err(format!("{}: not a node id", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let id = pt_node::random_id();
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .and_then(|mut f| {
                    f.write_all(format!("{id}\n").as_bytes())?;
                    f.sync_all()
                })
                .map_err(|e| format!("writing {}: {e}", path.display()))?;
            Ok(id)
        }
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}
