use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::emit::Out;
use crate::error::DriverError;
use crate::event::{AckKind, Decision, Entry, ErrorClass, Event, Harness, PromptKind, classify};
use crate::plan::{plan, vet_effort};
use crate::protocol::Protocol;
use crate::spec::SpawnSpec;

const MAX_LINE: usize = 8 << 20;
const MAX_MESSAGE: usize = 1 << 20;
const STDERR_RING: usize = 64 << 10;
const STDERR_TAIL: usize = 4 << 10;
/// Bytes queued for a harness that is not reading its stdin before the session fails.
const MAX_QUEUED: usize = 32 << 20;

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
    stdin: Option<Sender<Queued>>,
    /// Bytes handed to the writer and not yet written.
    queued: usize,
    acks: HashMap<String, Ack>,
    native_id: Option<String>,
    exited: bool,
    stopping: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A stdin line for the writer thread.
struct Queued {
    bytes: Vec<u8>,
    request: Option<String>,
}

impl Core {
    /// Releases what the last protocol step produced, in order. False when the session must
    /// fail: the harness has stopped reading its stdin (the queue is over its bound), or it
    /// refused its handshake.
    fn pump(&mut self) -> bool {
        let mut ok = true;
        for w in std::mem::take(&mut self.out.writes) {
            let Some(stdin) = &self.stdin else {
                break;
            };
            let mut text = String::new();
            w.line.write(&mut text);
            text.push('\n');
            let len = text.len();
            if self.queued.saturating_add(len) > MAX_QUEUED {
                ok = false;
                let queued = self.queued;
                self.stdin = None;
                self.out.error(
                    ErrorClass::Protocol,
                    format!("harness stopped reading its stdin with {queued} bytes queued"),
                );
                break;
            }
            let line = Queued {
                bytes: text.into_bytes(),
                request: w.request,
            };
            if stdin.send(line).is_ok() {
                self.queued += len;
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
        ok && !self.out.refused
    }
}

impl Shared {
    /// Pumps `core`, and kills the harness when the session must fail.
    fn release(&self, core: &mut Core) -> bool {
        let ok = core.pump();
        if !ok {
            let _ = lock(&self.child).kill();
        }
        ok
    }
}

/// Starts the harness. Events arrive on the receiver until `Entry::Exited`, which is last.
pub fn spawn(mut spec: SpawnSpec) -> Result<(Session, Receiver<Event>), DriverError> {
    let effort_note = vet_effort(&mut spec);
    let (args, mut proto) = plan(&spec, &mut crate::emit::new_uuid)?;
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
    let (stdin_tx, stdin_rx) = mpsc::channel::<Queued>();
    let mut out = Out::new(spec.g);
    if let Some(note) = effort_note {
        out.notice("effort", note);
    }
    proto.start(&mut out);
    let shared = Arc::new(Shared {
        core: Mutex::new(Core {
            proto,
            out,
            events: events_tx,
            stdin: Some(stdin_tx),
            queued: 0,
            acks: HashMap::new(),
            native_id: None,
            exited: false,
            stopping: false,
        }),
        changed: Condvar::new(),
        child: Mutex::new(child),
        stderr: Mutex::new(VecDeque::new()),
    });
    shared.release(&mut lock(&shared.core));

    let harness = spec.harness;
    let name = harness.as_str();
    let fail = |source: io::Error| abort_spawn(&shared, &spec.program, source);
    let writer = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-stdin"))
        .spawn(move || write_stdin(&writer, stdin, stdin_rx))
        .map_err(fail)?;
    let (stdout_done_tx, stdout_done_rx) = mpsc::channel::<()>();
    let reader = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-stdout"))
        .spawn(move || {
            read_stdout(&reader, harness, stdout);
            drop(stdout_done_tx);
        })
        .map_err(fail)?;
    let ring = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-stderr"))
        .spawn(move || read_stderr(&ring, stderr))
        .map_err(fail)?;
    let waiter = Arc::clone(&shared);
    thread::Builder::new()
        .name(format!("pt-{name}-wait"))
        .spawn(move || wait_exit(&waiter, &stdout_done_rx))
        .map_err(fail)?;

    Ok((
        Session {
            shared,
            harness,
            pid,
        },
        events_rx,
    ))
}

/// A thread failed to start after the child did: kill and reap the child, and close the
/// writer's queue so its thread (if it started) ends too.
fn abort_spawn(shared: &Shared, program: &Path, source: io::Error) -> DriverError {
    {
        let mut core = lock(&shared.core);
        core.stopping = true;
        core.stdin = None;
    }
    let mut child = lock(&shared.child);
    let _ = child.kill();
    let _ = child.wait();
    DriverError::Spawn {
        program: program.to_owned(),
        source,
    }
}

/// Writes queued lines in order. Once a user message's line is written, its `user_message`
/// entry enters the event stream, so everything read before it predates the write.
fn write_stdin(shared: &Shared, mut stdin: ChildStdin, lines: Receiver<Queued>) {
    for line in lines {
        let result = stdin.write_all(&line.bytes).and_then(|()| stdin.flush());
        let mut core = lock(&shared.core);
        core.queued = core.queued.saturating_sub(line.bytes.len());
        match &result {
            Ok(()) => {
                if let Some(request_id) = &line.request {
                    core.out.written(request_id);
                }
            }
            Err(e) => {
                if !core.exited && !core.stopping {
                    core.out.notice(
                        "stdin",
                        format!("writing to the harness's stdin failed: {e}"),
                    );
                }
                core.stdin = None;
            }
        }
        shared.release(&mut core);
        drop(core);
        shared.changed.notify_all();
        if result.is_err() {
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
        shared.release(&mut core);
        drop(core);
        shared.changed.notify_all();
    }
}

/// One stdout record: parsed and mapped, or counted as stray output.
pub(crate) fn handle_line(proto: &mut dyn Protocol, out: &mut Out, harness: Harness, line: &[u8]) {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.iter().all(u8::is_ascii_whitespace) {
        return;
    }
    match pt_json::parse_bytes(line) {
        Ok(msg) if msg.as_object().is_some() => proto.on_message(msg, out),
        _ => out.stray_line(harness.as_str(), &String::from_utf8_lossy(line)),
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
    shared.release(&mut core);
    core.exited = true;
    core.stdin = None;
    drop(core);
    shared.changed.notify_all();
}

/// Validates and writes a user message; `steer` names the turn it is meant for. Its entry
/// follows once the line is written.
pub(crate) fn deliver(
    proto: &mut dyn Protocol,
    out: &mut Out,
    request_id: &str,
    from: &str,
    kind: PromptKind,
    text: &str,
    steer: Option<&str>,
) -> Result<(), DriverError> {
    if request_id.is_empty() || request_id.len() > 256 {
        return Err(DriverError::Invalid("request id must have 1 to 256 bytes"));
    }
    if text.is_empty() || text.len() > MAX_MESSAGE {
        return Err(DriverError::Invalid("message must have 1 to 1 MiB"));
    }
    proto.send_user(request_id, text, steer, out)?;
    out.after_write(
        request_id,
        Entry::UserMessage {
            request_id: request_id.to_owned(),
            from: from.to_owned(),
            text: text.to_owned(),
            steer: steer.is_some(),
            kind,
        },
    );
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
        let queued = self.shared.release(&mut core);
        drop(core);
        self.shared.changed.notify_all();
        if !queued {
            return Err(DriverError::Ended);
        }
        result
    }

    /// Starts a turn with `text`. `Err` means nothing was written. `Entry::UserMessage`
    /// enters the event stream once the line is written to the harness, never after the
    /// harness's answer to it: harness output before it predates the prompt and is no
    /// evidence for it. The verdict comes as `Entry::PromptAck` and through
    /// [`Session::wait_ack`].
    pub fn send_user(
        &self,
        request_id: &str,
        from: &str,
        kind: PromptKind,
        text: &str,
    ) -> Result<(), DriverError> {
        self.with_core(|core| {
            deliver(
                core.proto.as_mut(),
                &mut core.out,
                request_id,
                from,
                kind,
                text,
                None,
            )
        })
    }

    /// Adds `text` to the running turn `expected_turn` (a `native_turn_id` from its ack)
    /// where the harness steers natively (Codex, Pi). [`DriverError::TurnEnded`] when that
    /// turn is no longer the running one; nothing was written then.
    pub fn steer(
        &self,
        request_id: &str,
        from: &str,
        text: &str,
        expected_turn: &str,
    ) -> Result<(), DriverError> {
        self.with_core(|core| {
            deliver(
                core.proto.as_mut(),
                &mut core.out,
                request_id,
                from,
                PromptKind::Prompt,
                text,
                Some(expected_turn),
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
