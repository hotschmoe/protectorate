use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::emit::Out;
use crate::event::{AckKind, Decision, Entry, ErrorClass, Event, Harness, classify};
use crate::{DriverError, Protocol, SpawnSpec, plan};

const MAX_LINE: usize = 8 << 20;
const MAX_MESSAGE: usize = 1 << 20;
const STDERR_RING: usize = 64 << 10;
const STDERR_TAIL: usize = 4 << 10;

/// What the protocol said about a sent message so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ack {
    Confirmed {
        native_turn_id: Option<String>,
    },
    Rejected {
        detail: Option<String>,
    },
    /// No verdict yet: after a deadline this is the effect record's Uncertain.
    Pending,
}

/// One running harness process. Dropping it kills the process.
pub struct Session {
    shared: Arc<Shared>,
    harness: Harness,
    pid: u32,
}

struct Shared {
    core: Mutex<Core>,
    changed: Condvar,
    child: Mutex<Child>,
    stderr: Mutex<VecDeque<u8>>,
}

struct Core {
    proto: Box<dyn Protocol>,
    out: Out,
    events: Sender<Event>,
    stdin: Option<Sender<Vec<u8>>>,
    acks: HashMap<String, Ack>,
    native_id: Option<String>,
    exited: bool,
    stopping: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Core {
    /// Releases what the last protocol step produced, in order.
    fn pump(&mut self) {
        for line in self.out.writes.drain(..) {
            if let Some(stdin) = &self.stdin {
                let mut text = String::new();
                line.write(&mut text);
                text.push('\n');
                let _ = stdin.send(text.into_bytes());
            }
        }
        for event in self.out.events.drain(..) {
            match &event.entry {
                Entry::PromptAck {
                    request_id,
                    ack,
                    native_turn_id,
                    detail,
                } => {
                    let verdict = match ack {
                        AckKind::Confirmed => Ack::Confirmed {
                            native_turn_id: native_turn_id.clone(),
                        },
                        AckKind::Rejected => Ack::Rejected {
                            detail: detail.clone(),
                        },
                    };
                    self.acks.insert(request_id.clone(), verdict);
                }
                Entry::SessionInfo { native_id, .. } => self.native_id = Some(native_id.clone()),
                _ => {}
            }
            let _ = self.events.send(event);
        }
    }
}

/// Starts the harness. Events arrive on the receiver until `Entry::Exited`, which is last.
pub fn spawn(mut spec: SpawnSpec) -> Result<(Session, Receiver<Event>), DriverError> {
    let (args, mut proto) = plan(&spec)?;
    let mut cmd = match spec.prefix.take() {
        Some(mut prefix) => {
            prefix.arg(&spec.program);
            prefix
        }
        None => Command::new(&spec.program),
    };
    cmd.args(&args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for key in &spec.env_remove {
        cmd.env_remove(key);
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().map_err(|source| DriverError::Spawn {
        program: spec.program.clone(),
        source,
    })?;
    let pid = child.id();
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(DriverError::Spawn {
            program: spec.program,
            source: io::Error::other("child pipes missing"),
        });
    };

    let (events_tx, events_rx) = mpsc::channel();
    let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
    let mut out = Out::new(spec.g);
    proto.start(&mut out);
    let shared = Arc::new(Shared {
        core: Mutex::new(Core {
            proto,
            out,
            events: events_tx,
            stdin: Some(stdin_tx),
            acks: HashMap::new(),
            native_id: None,
            exited: false,
            stopping: false,
        }),
        changed: Condvar::new(),
        child: Mutex::new(child),
        stderr: Mutex::new(VecDeque::new()),
    });
    lock(&shared.core).pump();

    let harness = spec.harness;
    let name = harness.as_str();
    thread::Builder::new()
        .name(format!("pt-{name}-stdin"))
        .spawn(move || write_stdin(stdin, stdin_rx))
        .map_err(|source| spawn_thread_error(&spec.program, source))?;
    let (stdout_done_tx, stdout_done_rx) = mpsc::channel::<()>();
    let reader = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-stdout"))
        .spawn(move || {
            read_stdout(&reader, harness, stdout);
            drop(stdout_done_tx);
        })
        .map_err(|source| spawn_thread_error(&spec.program, source))?;
    let ring = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-stderr"))
        .spawn(move || read_stderr(&ring, stderr))
        .map_err(|source| spawn_thread_error(&spec.program, source))?;
    let waiter = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-wait"))
        .spawn(move || wait_exit(&waiter, &stdout_done_rx))
        .map_err(|source| spawn_thread_error(&spec.program, source))?;

    Ok((
        Session {
            shared,
            harness,
            pid,
        },
        events_rx,
    ))
}

fn spawn_thread_error(program: &std::path::Path, source: io::Error) -> DriverError {
    DriverError::Spawn {
        program: program.to_owned(),
        source,
    }
}

fn write_stdin(mut stdin: std::process::ChildStdin, lines: Receiver<Vec<u8>>) {
    for line in lines {
        if stdin.write_all(&line).and_then(|()| stdin.flush()).is_err() {
            break;
        }
    }
}

enum Line {
    Eof,
    Data,
    Oversized,
}

/// Reads one LF-terminated record into `buf`, without growing past `cap`: an oversized
/// record is skipped to its newline.
fn read_line(r: &mut impl BufRead, buf: &mut Vec<u8>, cap: usize) -> io::Result<Line> {
    buf.clear();
    let mut over = false;
    loop {
        let avail = match r.fill_buf() {
            Ok(avail) => avail,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if avail.is_empty() {
            return Ok(match (over, buf.is_empty()) {
                (true, _) => Line::Oversized,
                (false, true) => Line::Eof,
                (false, false) => Line::Data,
            });
        }
        let (take, done) = match avail.iter().position(|b| *b == b'\n') {
            Some(i) => (i, true),
            None => (avail.len(), false),
        };
        if !over {
            if buf.len() + take > cap {
                over = true;
                buf.clear();
            } else {
                buf.extend_from_slice(&avail[..take]);
            }
        }
        r.consume(if done { take + 1 } else { take });
        if done {
            return Ok(if over { Line::Oversized } else { Line::Data });
        }
    }
}

fn read_stdout(shared: &Shared, harness: Harness, stdout: impl Read) {
    let mut reader = BufReader::with_capacity(64 << 10, stdout);
    let mut buf = Vec::new();
    loop {
        let line = read_line(&mut reader, &mut buf, MAX_LINE);
        let mut core = lock(&shared.core);
        match line {
            Ok(Line::Eof) | Err(_) => break,
            Ok(Line::Oversized) => core.out.error(
                ErrorClass::Protocol,
                format!("{harness} wrote a stdout line over {MAX_LINE} bytes"),
            ),
            Ok(Line::Data) => {
                let core = &mut *core;
                handle_line(core.proto.as_mut(), &mut core.out, harness, &buf);
            }
        }
        core.pump();
        drop(core);
        shared.changed.notify_all();
    }
}

/// One stdout record: parsed and mapped, or reported as a protocol desync.
pub(crate) fn handle_line(proto: &mut dyn Protocol, out: &mut Out, harness: Harness, line: &[u8]) {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.iter().all(u8::is_ascii_whitespace) {
        return;
    }
    match pt_json::parse_bytes(line) {
        Ok(msg) if msg.as_object().is_some() => proto.on_message(msg, out),
        Ok(_) => out.error(
            ErrorClass::Protocol,
            format!("{harness} wrote a stdout record that is not a JSON object"),
        ),
        Err(e) => out.error(
            ErrorClass::Protocol,
            format!("{harness} wrote an unparseable stdout line: {e}"),
        ),
    }
}

fn read_stderr(shared: &Shared, mut stderr: impl Read) {
    let mut chunk = [0u8; 8192];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let mut ring = lock(&shared.stderr);
                ring.extend(&chunk[..n]);
                let excess = ring.len().saturating_sub(STDERR_RING);
                ring.drain(..excess);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn stderr_tail(shared: &Shared) -> String {
    let ring = lock(&shared.stderr);
    let start = ring.len().saturating_sub(STDERR_TAIL);
    let bytes: Vec<u8> = ring.range(start..).copied().collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn wait_exit(shared: &Shared, stdout_done: &Receiver<()>) {
    let status = loop {
        match lock(&shared.child).try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        thread::sleep(Duration::from_millis(50));
    };
    // Let the reader map what the harness wrote before it died. A grandchild holding the
    // pipe open must not hold the exit event back for long.
    let _ = stdout_done.recv_timeout(Duration::from_secs(2));
    let tail = stderr_tail(shared);
    let mut core = lock(&shared.core);
    let requested = core.stopping;
    let (code, signal) = match &status {
        Ok(status) => (status.code().map(i64::from), status.signal().map(i64::from)),
        Err(_) => (None, None),
    };
    if !requested {
        let class = match classify(&tail, None) {
            ErrorClass::Other => ErrorClass::Crash,
            class => class,
        };
        let how = match (&status, code, signal) {
            (Err(e), _, _) => format!("wait failed: {e}"),
            (_, Some(code), _) => format!("exit code {code}"),
            (_, _, Some(signal)) => format!("signal {signal}"),
            _ => "unknown status".to_owned(),
        };
        let last = tail
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        core.out
            .error(class, format!("harness exited unexpectedly ({how}) {last}"));
    }
    core.out.emit(Entry::Exited {
        code,
        signal,
        requested,
        stderr_tail: tail,
    });
    core.pump();
    core.exited = true;
    core.stdin = None;
    drop(core);
    shared.changed.notify_all();
}

/// Validates and writes a user message, then records it as an entry.
pub(crate) fn deliver(
    proto: &mut dyn Protocol,
    out: &mut Out,
    request_id: &str,
    from: &str,
    text: &str,
    steer: bool,
) -> Result<(), DriverError> {
    if request_id.is_empty() || request_id.len() > 256 {
        return Err(DriverError::Invalid("request id must have 1 to 256 bytes"));
    }
    if text.is_empty() || text.len() > MAX_MESSAGE {
        return Err(DriverError::Invalid("message must have 1 to 1 MiB"));
    }
    proto.send_user(request_id, text, steer, out)?;
    out.emit(Entry::UserMessage {
        request_id: request_id.to_owned(),
        from: from.to_owned(),
        text: text.to_owned(),
        steer,
    });
    Ok(())
}

impl Session {
    pub fn harness(&self) -> Harness {
        self.harness
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The native session id once the harness has reported it.
    pub fn native_id(&self) -> Option<String> {
        lock(&self.shared.core).native_id.clone()
    }

    pub fn is_exited(&self) -> bool {
        lock(&self.shared.core).exited
    }

    fn with_core<T>(
        &self,
        f: impl FnOnce(&mut Core) -> Result<T, DriverError>,
    ) -> Result<T, DriverError> {
        let mut core = lock(&self.shared.core);
        if core.exited || core.stopping || core.stdin.is_none() {
            return Err(DriverError::Ended);
        }
        let result = f(&mut core);
        core.pump();
        drop(core);
        self.shared.changed.notify_all();
        result
    }

    /// Starts a turn with `text`. `Err` means nothing was written. The verdict comes as
    /// `Entry::PromptAck` and through [`Session::wait_ack`].
    pub fn send_user(&self, request_id: &str, from: &str, text: &str) -> Result<(), DriverError> {
        self.with_core(|core| {
            deliver(
                core.proto.as_mut(),
                &mut core.out,
                request_id,
                from,
                text,
                false,
            )
        })
    }

    /// Adds `text` to the running turn where the harness steers natively (Codex, Pi).
    pub fn steer(&self, request_id: &str, from: &str, text: &str) -> Result<(), DriverError> {
        self.with_core(|core| {
            deliver(
                core.proto.as_mut(),
                &mut core.out,
                request_id,
                from,
                text,
                true,
            )
        })
    }

    /// Waits up to `timeout` for the protocol's verdict on `request_id`.
    pub fn wait_ack(&self, request_id: &str, timeout: Duration) -> Ack {
        let deadline = Instant::now() + timeout;
        let mut core = lock(&self.shared.core);
        loop {
            if let Some(ack) = core.acks.get(request_id) {
                return ack.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if core.exited || left.is_zero() {
                return Ack::Pending;
            }
            core = self
                .shared
                .changed
                .wait_timeout(core, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    pub fn respond_approval(&self, id: &str, decision: Decision) -> Result<(), DriverError> {
        self.with_core(|core| core.proto.respond_approval(id, decision, &mut core.out))
    }

    /// Asks the harness to stop the running turn; a no-op when idle.
    pub fn interrupt(&self) -> Result<(), DriverError> {
        self.with_core(|core| core.proto.interrupt(&mut core.out))
    }

    /// Closes stdin, waits up to `grace` for the harness to exit, then kills it, and returns
    /// once the exit is reported.
    pub fn shutdown(&self, grace: Duration) {
        {
            let mut core = lock(&self.shared.core);
            core.stopping = true;
            core.stdin = None;
        }
        if !self.wait_exited(grace) {
            let _ = lock(&self.shared.child).kill();
            self.wait_exited(Duration::from_secs(10));
        }
    }

    fn wait_exited(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut core = lock(&self.shared.core);
        while !core.exited {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            core = self
                .shared
                .changed
                .wait_timeout(core, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut core = lock(&self.shared.core);
        if !core.exited {
            core.stopping = true;
            core.stdin = None;
            drop(core);
            let _ = lock(&self.shared.child).kill();
        }
    }
}
