//! Per-sleeve append-only chat events, with replay and gap-free live watches.
mod telemetry;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use telemetry::Telemetry;

const MAX_QUEUE: usize = 4096;

#[derive(Debug)]
pub enum Error {
    Closed,
    SlowConsumer,
    InvalidLog(String),
    Callback(String),
    Io(io::Error),
    Json(serde_json::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "chat log closed"),
            Self::SlowConsumer => {
                write!(f, "watcher fell too far behind; resume from the last seq")
            }
            Self::InvalidLog(s) => write!(f, "invalid chat log: {s}"),
            Self::Callback(s) => write!(f, "chat watcher callback: {s}"),
            Self::Io(e) => write!(f, "chat log IO: {e}"),
            Self::Json(e) => write!(f, "chat log JSON: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// The wire adapter maps the payload to a ChatEvent oneof. A text payload has
/// the shape `{"text":{"messageId":...,"delta":...,"thinking":...}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub time_ms: u64,
    pub payload: Value,
}
impl Event {
    pub fn new(payload: Value) -> Self {
        Self {
            seq: 0,
            time_ms: 0,
            payload,
        }
    }
}

#[derive(Clone)]
struct OpenText {
    seq: u64,
    id: String,
    auto: bool,
    thinking: bool,
    at: u64,
    text: String,
}
impl OpenText {
    fn whole(&self) -> Event {
        Event {
            seq: self.seq,
            time_ms: self.at,
            payload: text_payload(&self.id, self.thinking, &self.text),
        }
    }
}
fn text_payload(id: &str, thinking: bool, delta: &str) -> Value {
    json!({"text":{"messageId":id,"delta":delta,"thinking":thinking}})
}
/// The signal that ended a process. The sandbox helper reports a signal that
/// ends its workload as exit status 137, which stands for SIGKILL.
fn signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(status) {
        return Some(signal);
    }
    (status.code() == Some(137)).then_some(9)
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Default)]
struct WatchQueue {
    events: VecDeque<Event>,
    overflow: bool,
    closed: bool,
}
struct Watcher {
    queue: Mutex<WatchQueue>,
    notify: Condvar,
}
struct WatchSnapshot {
    start: Option<u64>,
    boundary: u64,
    partial: Option<Event>,
}
impl Watcher {
    fn push(&self, event: Event) {
        let mut q = lock(&self.queue);
        if q.events.len() >= MAX_QUEUE {
            q.overflow = true;
        } else {
            q.events.push_back(event);
        }
        self.notify.notify_one();
    }
    fn close(&self) {
        lock(&self.queue).closed = true;
        self.notify.notify_one();
    }
}

struct Inner {
    file: File,
    offsets: Vec<u64>,
    size: u64,
    last: u64,
    open: Option<OpenText>,
    watchers: HashMap<u64, Arc<Watcher>>,
    next_watcher: u64,
    closed: bool,
    cache: Telemetry,
}

pub struct Log {
    path: PathBuf,
    inner: Mutex<Inner>,
    stopping: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Log {
    /// Opens or creates a log. A torn final line is truncated; malformed
    /// complete lines and sequence gaps are errors so history is not lost.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        let mut inner = Inner {
            file,
            offsets: Vec::new(),
            size: 0,
            last: 0,
            open: None,
            watchers: HashMap::new(),
            next_watcher: 0,
            closed: false,
            cache: Telemetry::default(),
        };
        let mut reader = BufReader::new(inner.file.try_clone()?);
        let mut line = Vec::new();
        loop {
            line.clear();
            let count = reader.read_until(b'\n', &mut line)?;
            if count == 0 {
                break;
            }
            if line.last() != Some(&b'\n') {
                inner.file.set_len(inner.size)?;
                break;
            }
            let mut event: Event = serde_json::from_slice(&line).map_err(|e| {
                Error::InvalidLog(format!(
                    "{} line {}: {e}",
                    path.display(),
                    inner.offsets.len() + 1
                ))
            })?;
            if event.seq != inner.last + 1 {
                return Err(Error::InvalidLog(format!(
                    "{} line {} has seq {}",
                    path.display(),
                    inner.offsets.len() + 1,
                    event.seq
                )));
            }
            inner.cache.observe(&mut event.payload);
            inner.offsets.push(inner.size);
            inner.size += count as u64;
            inner.last = event.seq;
        }
        inner.cache.begin_run();
        inner.file.seek(SeekFrom::Start(inner.size))?;
        Ok(Self {
            path,
            inner: Mutex::new(inner),
            stopping: AtomicBool::new(false),
        })
    }

    pub fn last(&self) -> u64 {
        lock(&self.inner).last
    }

    /// Records that the running turn was asked to stop, so it settles as
    /// canceled.
    pub fn note_interrupt(&self) {
        lock(&self.inner).cache.interrupt();
    }

    /// Records that the harness is being stopped on purpose, so its exit is
    /// not reported as a kill.
    pub fn note_stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
    }

    /// Appends the error event for a harness process that ended. A signal the
    /// owner did not ask for is a kill, which is how a resource limit or the
    /// kernel's OOM killer ends a process; any other exit is a plain exit.
    pub fn append_exit<E: fmt::Display>(
        &self,
        harness: &str,
        result: &Result<std::process::ExitStatus, E>,
    ) -> Result<u64, Error> {
        let error = match result {
            Err(error) => json!({"text": format!("{harness} wait failed: {error}")}),
            Ok(status) => match signal(status).filter(|_| !self.stopping.load(Ordering::Relaxed)) {
                Some(signal) => json!({
                    "text": format!("{harness} killed by signal {signal}"),
                    "kind": "CHAT_ERROR_KIND_KILLED",
                }),
                None => json!({
                    "text": format!("{harness} exited: {status}"),
                    "kind": "CHAT_ERROR_KIND_EXITED",
                }),
            },
        };
        self.append(Event::new(json!({ "error": error })))
    }

    /// Prompt-cache telemetry in the ChatStatus `cache` JSON shape.
    pub fn cache(&self) -> Value {
        lock(&self.inner).cache.json()
    }

    /// Appends a non-text event, flushing an open assistant message first.
    pub fn append(&self, event: Event) -> Result<u64, Error> {
        self.append_with(|_| event).map(|event| event.seq)
    }

    /// Appends the report `build` makes from the event's own sequence number
    /// and returns it.
    pub fn append_report(&self, build: impl FnOnce(u64) -> Value) -> Result<Value, Error> {
        let event = self.append_with(|seq| Event::new(json!({"report": build(seq)})))?;
        Ok(event.payload["report"].clone())
    }

    fn append_with(&self, build: impl FnOnce(u64) -> Event) -> Result<Event, Error> {
        let mut inner = lock(&self.inner);
        if inner.closed {
            return Err(Error::Closed);
        }
        inner.flush()?;
        let mut event = build(inner.last + 1);
        event.seq = inner.last + 1;
        if event.time_ms == 0 {
            event.time_ms = now_ms();
        }
        inner.cache.observe(&mut event.payload);
        inner.persist(&event)?;
        inner.last = event.seq;
        inner.fanout(&event);
        Ok(event)
    }

    /// Streams a delta now and stores one complete message when it closes.
    pub fn append_text(&self, id: &str, thinking: bool, delta: &str) -> Result<(), Error> {
        if delta.is_empty() {
            return Ok(());
        }
        let mut inner = lock(&self.inner);
        if inner.closed {
            return Err(Error::Closed);
        }
        if inner
            .open
            .as_ref()
            .is_some_and(|o| o.thinking != thinking || (!id.is_empty() && (o.auto || o.id != id)))
        {
            inner.flush()?;
        }
        if inner.open.is_none() {
            inner.last += 1;
            inner.open = Some(OpenText {
                seq: inner.last,
                id: if id.is_empty() {
                    format!("m{}", inner.last)
                } else {
                    id.into()
                },
                auto: id.is_empty(),
                thinking,
                at: now_ms(),
                text: String::new(),
            });
        }
        let open = inner.open.as_mut().expect("opened above");
        open.text.push_str(delta);
        let event = Event {
            seq: open.seq,
            time_ms: now_ms(),
            payload: text_payload(&open.id, thinking, delta),
        };
        inner.fanout(&event);
        Ok(())
    }

    pub fn close(&self) -> Result<(), Error> {
        let mut inner = lock(&self.inner);
        if inner.closed {
            return Ok(());
        }
        let flush = inner.flush();
        inner.closed = true;
        for watcher in inner.watchers.values() {
            watcher.close();
        }
        let sync = inner.file.sync_all().map_err(Error::Io);
        flush.and(sync)
    }

    /// Replays seq > `after`, then follows live events. A caller resuming
    /// from a text delta should pass its seq minus one to receive the whole
    /// message on replay. Cancellation is checked at least every 100 ms.
    pub fn watch<F>(&self, after: u64, cancelled: &AtomicBool, mut send: F) -> Result<(), Error>
    where
        F: FnMut(&Event) -> Result<(), String>,
    {
        let watcher = Arc::new(Watcher {
            queue: Mutex::new(WatchQueue::default()),
            notify: Condvar::new(),
        });
        let (id, start, boundary, partial) = {
            let mut inner = lock(&self.inner);
            if inner.closed {
                return Err(Error::Closed);
            }
            let boundary = inner.open.as_ref().map_or(inner.last, |o| o.seq - 1);
            let start = if after < boundary {
                Some(inner.offsets[after as usize])
            } else {
                None
            };
            let partial = inner
                .open
                .as_ref()
                .map(OpenText::whole)
                .filter(|ev| ev.seq > after);
            inner.next_watcher += 1;
            let id = inner.next_watcher;
            inner.watchers.insert(id, Arc::clone(&watcher));
            (id, start, boundary, partial)
        };
        let snapshot = WatchSnapshot {
            start,
            boundary,
            partial,
        };
        let result = self.watch_inner(after, cancelled, &mut send, &watcher, snapshot);
        lock(&self.inner).watchers.remove(&id);
        result
    }

    fn watch_inner<F>(
        &self,
        after: u64,
        cancelled: &AtomicBool,
        send: &mut F,
        watcher: &Watcher,
        snapshot: WatchSnapshot,
    ) -> Result<(), Error>
    where
        F: FnMut(&Event) -> Result<(), String>,
    {
        if let Some(start) = snapshot.start {
            let mut file = File::open(&self.path)?;
            file.seek(SeekFrom::Start(start))?;
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(());
                }
                line.clear();
                if reader.read_until(b'\n', &mut line)? == 0 {
                    return Err(Error::InvalidLog("replay ended before boundary".into()));
                }
                let event: Event = serde_json::from_slice(&line)?;
                send(&event).map_err(Error::Callback)?;
                if event.seq >= snapshot.boundary {
                    break;
                }
            }
        }
        if let Some(partial) = snapshot.partial {
            send(&partial).map_err(Error::Callback)?;
        }
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let (batch, overflow, closed) = {
                let mut queue = lock(&watcher.queue);
                if queue.events.is_empty() && !queue.overflow && !queue.closed {
                    queue = watcher
                        .notify
                        .wait_timeout(queue, Duration::from_millis(100))
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                }
                (
                    std::mem::take(&mut queue.events),
                    queue.overflow,
                    queue.closed,
                )
            };
            let had_events = !batch.is_empty();
            for event in batch {
                if event.seq > after {
                    send(&event).map_err(Error::Callback)?;
                }
            }
            if overflow {
                return Err(Error::SlowConsumer);
            }
            if closed && !had_events {
                return Err(Error::Closed);
            }
        }
    }
}

impl Inner {
    fn persist(&mut self, event: &Event) -> Result<(), Error> {
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        self.file.seek(SeekFrom::Start(self.size))?;
        if let Err(e) = self.file.write_all(&line) {
            let _ = self.file.set_len(self.size);
            return Err(Error::Io(e));
        }
        self.offsets.push(self.size);
        self.size += line.len() as u64;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Error> {
        if let Some(open) = self.open.as_ref() {
            self.persist(&open.whole())?;
            self.open = None;
        }
        Ok(())
    }
    fn fanout(&self, event: &Event) {
        for watcher in self.watchers.values() {
            watcher.push(event.clone());
        }
    }
}

pub fn read_all(path: impl AsRef<Path>) -> Result<Vec<Event>, Error> {
    let file = File::open(path)?;
    let mut events = Vec::new();
    for line in BufReader::new(file).lines() {
        events.push(serde_json::from_str(&line?)?);
    }
    Ok(events)
}
