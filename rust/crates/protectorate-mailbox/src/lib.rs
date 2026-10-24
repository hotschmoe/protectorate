//! The hub's message store: an append-only JSONL log of everything that
//! happens to a message (sent, addressed, delivered, read, acked, reminded,
//! escalated), replayed on open.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex, MutexGuard,
};
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub type Time = DateTime<Utc>;

pub const CONCISE_GUIDANCE: &str = "Mail: aim for 150 words; lead with the result or blocker. Include only actionable findings and essential evidence; reference a file, commit or note for details. Do not restate the request or prior context. No acknowledgements unless asked or ack-required; use msg ack when only confirmation is needed. Finish every brief with protectorate report --status complete|partial|blocked --summary-file F; use partial honestly when work remains.";

pub const SUBAGENT_GUIDANCE: &str = "Use the harness's own subagents for parallel subtasks inside one workspace. Ask for another sleeve when the work needs its own branch, resource limits or isolation.";

#[derive(Debug)]
pub enum Error {
    NotFound {
        id: String,
    },
    NotRecipient {
        rcpt: String,
        id: String,
    },
    Corrupt {
        path: String,
        line: usize,
        message: String,
    },
    Io {
        context: String,
        source: std::io::Error,
    },
    Closed,
    Timeout {
        target: String,
    },
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound { id } => write!(f, "no such message: {}", id),
            Error::NotRecipient { rcpt, id } => {
                write!(
                    f,
                    "not a recipient of this message: {}, message {}",
                    rcpt, id
                )
            }
            Error::Corrupt {
                path,
                line,
                message,
            } => write!(f, "mailbox {}: line {}: {}", path, line, message),
            Error::Io { context, source } => write!(f, "{}: {}", context, source),
            Error::Closed => write!(f, "mailbox is closed"),
            Error::Timeout { target } => {
                write!(f, "wait for mail: timed out waiting for {}", target)
            }
            Error::Cancelled => write!(f, "wait for mail: cancelled"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Error {
    fn io(context: &str, source: std::io::Error) -> Error {
        Error::Io {
            context: context.to_string(),
            source,
        }
    }

    fn corrupt(message: impl Into<String>) -> Error {
        Error::Corrupt {
            path: String::new(),
            line: 0,
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Message kinds: free-form, with the three known values as constants.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kind(Cow<'static, str>);

impl Kind {
    pub const NOTE: Kind = Kind(Cow::Borrowed("note"));
    pub const REQUEST: Kind = Kind(Cow::Borrowed("request"));
    pub const NOTICE: Kind = Kind(Cow::Borrowed("notice"));

    pub fn new(s: impl Into<Cow<'static, str>>) -> Kind {
        Kind(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Kind {
    fn default() -> Kind {
        Kind::NOTE
    }
}

/// One recipient's view of a message. Since starts the ack timer: it is set
/// when the message is injected into the recipient's chat or, for a recipient
/// that cannot be injected, when it was addressed to them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Delivery {
    pub since: Option<Time>,
    pub delivered: Option<Time>,
    pub read: Option<Time>,
    pub acked: Option<Time>,
    pub reminders: i64,
}

/// One message with its delivery state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Message {
    pub id: String,
    pub thread: String,
    pub from: String,
    pub to: String,
    pub kind: Kind,
    pub ack_required: bool,
    pub body: String,
    pub request_id: String,
    pub reply_to: String,
    pub from_project: String,
    pub project: String,
    pub created: Time,
    pub recipients: Vec<String>,
    pub deliveries: BTreeMap<String, Delivery>,
    pub escalated: Option<Time>,
    pub note_id: String,
    pub run_id: String,
}

impl Message {
    /// Open reports whether no recipient has acked the message yet.
    pub fn open(&self) -> bool {
        !self.deliveries.values().any(|d| d.acked.is_some())
    }

    /// Unread reports whether rcpt has neither been shown, been injected nor
    /// acked the message.
    pub fn unread(&self, rcpt: &str) -> bool {
        match self.deliveries.get(rcpt) {
            Some(d) => d.delivered.is_none() && d.read.is_none() && d.acked.is_none(),
            None => false,
        }
    }
}

/// One log line.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    op: String,
    at: Time,
    id: String,
    rcpt: String,
    to: Vec<String>,
    item: String,
    msg: Option<Message>,
}

/// A message to send. Recipients may be left empty for an address that
/// resolves later (a project with no live sleeve yet).
pub struct NewMessage {
    pub from: String,
    pub to: String,
    pub kind: Kind,
    pub ack_required: bool,
    pub body: String,
    pub request_id: String,
    pub thread: String,
    pub reply_to: String,
    pub from_project: String,
    pub project: String,
    pub recipients: Vec<String>,
    /// Footer, if set, is appended to the body once the thread id is known.
    pub footer: Option<Footer>,
    pub run_id: String,
}

impl Default for NewMessage {
    fn default() -> NewMessage {
        NewMessage {
            from: String::new(),
            to: String::new(),
            kind: Kind::NOTE,
            ack_required: false,
            body: String::new(),
            request_id: String::new(),
            thread: String::new(),
            reply_to: String::new(),
            from_project: String::new(),
            project: String::new(),
            recipients: Vec::new(),
            footer: None,
            run_id: String::new(),
        }
    }
}

/// Appended to a body once the thread id is known.
pub type Footer = Box<dyn Fn(&str) -> String + Send + Sync>;

/// The result of a send: the message and whether it was newly created.
#[derive(Clone, Debug)]
pub struct SendOutcome {
    pub message: Message,
    pub created: bool,
}

/// What an overdue ack calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Remind,
    Escalate,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Remind => "remind",
            Action::Escalate => "escalate",
        }
    }
}

/// One overdue delivery.
#[derive(Clone, Debug)]
pub struct Due {
    pub id: String,
    pub rcpt: String,
    pub action: Action,
}

/// A summary of one thread.
#[derive(Clone, Debug)]
pub struct ThreadInfo {
    pub id: String,
    /// Senders and recipients in order of first appearance; a message with no
    /// recipients yet adds "project:<id>" for its project.
    pub participants: Vec<String>,
    pub messages: usize,
    pub last: Message,
    /// Messages unread by the reader, requiring an ack nobody gave, and
    /// escalated without an ack since.
    pub unread: usize,
    pub open_acks: usize,
    pub escalated: usize,
    pub projects: Vec<String>,
}

struct Inner {
    path: PathBuf,
    file: Option<File>,
    msgs: HashMap<String, Message>,
    order: Vec<String>,
    by_req: HashMap<String, String>,
    next_msg: i64,
    next_thr: i64,
}

fn req_key(from: &str, id: &str) -> String {
    format!("{}\u{0}", from) + id
}

fn id_number(id: &str) -> i64 {
    id.trim_start_matches(['m', 't'])
        .parse::<i64>()
        .unwrap_or(0)
}

fn delivery_pending(delivery: &Delivery) -> bool {
    delivery.delivered.is_none() && delivery.read.is_none() && delivery.acked.is_none()
}

/// The message store. All methods are safe for concurrent use.
pub struct Store {
    inner: Mutex<Inner>,
    changed: ChangeSignal,
}

struct ChangeSignal {
    sync: Condvar,
    asynchronous: watch::Sender<u64>,
}

impl ChangeSignal {
    fn notify_all(&self) {
        self.sync.notify_all();
        self.asynchronous
            .send_modify(|version| *version = version.wrapping_add(1));
    }
}

impl Store {
    /// Opens or creates the log at path and replays it. A partial last line,
    /// left by a crash mid-write, is cut off.
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                let mut db = DirBuilder::new();
                db.recursive(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    db.mode(0o700);
                }
                db.create(dir)
                    .map_err(|e| Error::io("create mailbox dir", e))?;
            }
        }
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(path).map_err(|e| Error::io("open mailbox", e))?;
        let mut inner = Inner {
            path: path.to_path_buf(),
            file: Some(file),
            msgs: HashMap::new(),
            order: Vec::new(),
            by_req: HashMap::new(),
            next_msg: 0,
            next_thr: 0,
        };
        replay(&mut inner)?;
        let (asynchronous, _) = watch::channel(0);
        Ok(Store {
            inner: Mutex::new(inner),
            changed: ChangeSignal {
                sync: Condvar::new(),
                asynchronous,
            },
        })
    }

    pub fn path(&self) -> PathBuf {
        self.lock().path.clone()
    }

    /// Closes the log. Later writes fail.
    pub fn close(&self) -> std::io::Result<()> {
        let mut inner = self.lock();
        match inner.file.take() {
            Some(f) => f.sync_all(),
            None => Ok(()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stores a message. A retry from the same sender with the same
    /// request_id returns the original and created=false. A message without a
    /// thread starts a new one.
    pub fn send(&self, n: NewMessage) -> Result<SendOutcome> {
        let mut s = self.lock();
        if !n.request_id.is_empty() {
            if let Some(id) = s.by_req.get(&req_key(&n.from, &n.request_id)) {
                return Ok(SendOutcome {
                    message: s.msgs[id].clone(),
                    created: false,
                });
            }
        }
        let mut msg = Message {
            id: format!("m{}", s.next_msg + 1),
            thread: n.thread,
            from: n.from,
            to: n.to,
            kind: n.kind,
            ack_required: n.ack_required,
            body: n.body,
            request_id: n.request_id,
            reply_to: n.reply_to,
            from_project: n.from_project,
            project: n.project,
            created: Utc::now(),
            recipients: n.recipients.clone(),
            run_id: n.run_id,
            ..Default::default()
        };
        if msg.thread.is_empty() {
            msg.thread = format!("t{}", s.next_thr + 1);
        }
        if let Some(f) = &n.footer {
            msg.body = format!("{}\n\n{}", msg.body.trim_end_matches('\n'), f(&msg.thread));
        }
        let id = msg.id.clone();
        commit(
            &mut s,
            &self.changed,
            Record {
                op: "send".to_string(),
                at: msg.created,
                msg: Some(msg),
                ..Default::default()
            },
        )?;
        Ok(SendOutcome {
            message: s.msgs[&id].clone(),
            created: true,
        })
    }

    /// Sets the recipients of a message that had none and starts the ack timer
    /// for each recipient that cannot be injected (armed).
    pub fn resolve(&self, id: &str, rcpts: &[String], armed: &[String]) -> Result<()> {
        let mut s = self.lock();
        if let Some(m) = s.msgs.get(id) {
            if !m.recipients.is_empty() {
                return Ok(());
            }
        }
        op(&mut s, &self.changed, "resolve", id, "", |r| {
            r.to = rcpts.to_vec()
        })?;
        for a in armed {
            op(&mut s, &self.changed, "arm", id, a, |_| {})?;
        }
        Ok(())
    }

    /// Starts the ack timer of a recipient that is not injected.
    pub fn arm(&self, id: &str, rcpt: &str) -> Result<()> {
        let mut s = self.lock();
        if let Some(m) = s.msgs.get(id) {
            if m.deliveries.get(rcpt).and_then(|d| d.since).is_some() {
                return Ok(());
            }
        }
        op(&mut s, &self.changed, "arm", id, rcpt, |_| {})
    }

    /// Records that the message was injected into rcpt's chat.
    pub fn deliver(&self, id: &str, rcpt: &str) -> Result<()> {
        let mut s = self.lock();
        op(&mut s, &self.changed, "deliver", id, rcpt, |_| {})
    }

    /// Records a second delivery of an unacked message.
    pub fn remind(&self, id: &str, rcpt: &str) -> Result<()> {
        let mut s = self.lock();
        op(&mut s, &self.changed, "remind", id, rcpt, |_| {})
    }

    /// Records that an unacked message was escalated. It happens once.
    pub fn escalate(&self, id: &str) -> Result<()> {
        let mut s = self.lock();
        if let Some(m) = s.msgs.get(id) {
            if m.escalated.is_some() {
                return Ok(());
            }
        }
        op(&mut s, &self.changed, "escalate", id, "", |_| {})
    }

    /// Records the note a request was filed as.
    pub fn set_note_id(&self, id: &str, note: &str) -> Result<()> {
        let mut s = self.lock();
        op(&mut s, &self.changed, "backlog", id, "", |r| {
            r.item = note.to_string()
        })
    }

    /// Marks the message acked by rcpt. changed is false if it already was.
    pub fn ack(&self, id: &str, rcpt: &str) -> Result<(Message, bool)> {
        let mut s = self.lock();
        if let Some(cur) = s.msgs.get(id) {
            if cur.deliveries.get(rcpt).and_then(|d| d.acked).is_some() {
                return Ok((cur.clone(), false));
            }
        }
        op(&mut s, &self.changed, "ack", id, rcpt, |_| {})?;
        Ok((s.msgs[id].clone(), true))
    }

    /// Returns one message.
    pub fn get(&self, id: &str) -> Option<Message> {
        let s = self.lock();
        s.msgs.get(id).cloned()
    }

    /// Returns the messages of a thread in the order they were sent.
    pub fn thread(&self, thread: &str) -> Vec<Message> {
        let s = self.lock();
        s.order
            .iter()
            .filter_map(|id| s.msgs.get(id))
            .filter(|m| m.thread == thread)
            .cloned()
            .collect()
    }

    /// Returns the messages addressed to rcpt, oldest first, only those of
    /// thread unless it is empty. With unread_only it leaves out those already
    /// injected, shown or acked. Unless peek is set, the returned messages are
    /// marked read; the returned copies still show the state from before that.
    pub fn inbox(
        &self,
        rcpt: &str,
        thread: &str,
        unread_only: bool,
        peek: bool,
    ) -> Result<Vec<Message>> {
        let mut s = self.lock();
        let mut out = Vec::new();
        let count = s.order.len();
        let mut i = 0;
        while i < count {
            let skip = match s.msgs.get(&s.order[i]) {
                None => true,
                Some(m) => {
                    !m.deliveries.contains_key(rcpt)
                        || (!thread.is_empty() && m.thread != thread)
                        || (unread_only && !m.unread(rcpt))
                }
            };
            if skip {
                i += 1;
                continue;
            }
            let c = s.msgs[&s.order[i]].clone();
            if !peek && s.msgs[&s.order[i]].deliveries[rcpt].read.is_none() {
                let id = s.order[i].clone();
                op(&mut s, &self.changed, "read", &id, rcpt, |_| {})?;
            }
            out.push(c);
            i += 1;
        }
        Ok(out)
    }

    /// Summarizes every thread, most recently active first, counting unread
    /// messages for reader.
    pub fn threads(&self, reader: &str) -> Vec<ThreadInfo> {
        let s = self.lock();
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut last: HashMap<String, String> = HashMap::new();
        let mut list: Vec<ThreadInfo> = Vec::new();
        let add = |xs: &mut Vec<String>, v: String| {
            if !xs.contains(&v) {
                xs.push(v);
            }
        };
        for id in &s.order {
            let m = match s.msgs.get(id) {
                Some(m) => m,
                None => continue,
            };
            let idx = match index.get(&m.thread) {
                Some(i) => *i,
                None => {
                    list.push(ThreadInfo {
                        id: m.thread.clone(),
                        participants: Vec::new(),
                        messages: 0,
                        last: Message::default(),
                        unread: 0,
                        open_acks: 0,
                        escalated: 0,
                        projects: Vec::new(),
                    });
                    index.insert(m.thread.clone(), list.len() - 1);
                    list.len() - 1
                }
            };
            let ti = &mut list[idx];
            ti.messages += 1;
            last.insert(m.thread.clone(), m.id.clone());
            add(&mut ti.participants, m.from.clone());
            for r in &m.recipients {
                add(&mut ti.participants, r.clone());
            }
            if m.recipients.is_empty() && !m.project.is_empty() {
                add(&mut ti.participants, format!("project:{}", m.project));
            }
            for p in [m.project.clone(), m.from_project.clone()] {
                if !p.is_empty() {
                    add(&mut ti.projects, p);
                }
            }
            if m.unread(reader) {
                ti.unread += 1;
            }
            if m.open() && m.ack_required {
                ti.open_acks += 1;
            }
            if m.open() && m.escalated.is_some() {
                ti.escalated += 1;
            }
        }
        for ti in list.iter_mut() {
            if let Some(m) = last.get(&ti.id).and_then(|id| s.msgs.get(id)) {
                ti.last = m.clone();
            }
        }
        list.sort_by_key(|ti| std::cmp::Reverse(id_number(&ti.last.id)));
        list
    }

    /// Returns the messages that have no recipients yet.
    pub fn unresolved(&self) -> Vec<Message> {
        let s = self.lock();
        s.order
            .iter()
            .filter_map(|id| s.msgs.get(id))
            .filter(|m| m.recipients.is_empty())
            .cloned()
            .collect()
    }

    /// Returns, per recipient, the messages still to be injected: not yet
    /// delivered, not read and not acked. Armed mail also remains pending so a
    /// terminal recipient switched to chat can receive it.
    pub fn pending(&self) -> BTreeMap<String, Vec<Message>> {
        let s = self.lock();
        let mut out: BTreeMap<String, Vec<Message>> = BTreeMap::new();
        for id in &s.order {
            let m = match s.msgs.get(id) {
                Some(m) => m,
                None => continue,
            };
            for (r, d) in &m.deliveries {
                if delivery_pending(d) {
                    out.entry(r.clone()).or_default().push(m.clone());
                }
            }
        }
        out
    }

    /// Returns pending mail for one recipient without cloning every other
    /// recipient's queue during a coordinator scan.
    pub fn pending_for(&self, recipient: &str) -> Vec<Message> {
        let s = self.lock();
        s.order
            .iter()
            .filter_map(|id| s.msgs.get(id))
            .filter(|message| {
                message
                    .deliveries
                    .get(recipient)
                    .is_some_and(delivery_pending)
            })
            .cloned()
            .collect()
    }

    /// Returns the unacked request messages addressed to project that were
    /// sent from from_project, oldest first.
    pub fn open_requests(&self, project: &str, from_project: &str) -> Vec<Message> {
        let s = self.lock();
        s.order
            .iter()
            .filter_map(|id| s.msgs.get(id))
            .filter(|m| {
                m.kind == Kind::REQUEST
                    && m.project == project
                    && m.from_project == from_project
                    && m.open()
            })
            .cloned()
            .collect()
    }

    /// Lists the unacked ack_required deliveries whose timer ran past timeout:
    /// the first time a reminder, the second time an escalation (once per
    /// message).
    pub fn overdue(&self, timeout: Duration) -> Vec<Due> {
        let s = self.lock();
        let now = Utc::now();
        let mut out = Vec::new();
        for id in &s.order {
            let m = match s.msgs.get(id) {
                Some(m) => m,
                None => continue,
            };
            if !m.ack_required || m.escalated.is_some() {
                continue;
            }
            let mut escalated = false;
            for (r, d) in &m.deliveries {
                let since = match d.since {
                    Some(t) => t,
                    None => continue,
                };
                if d.acked.is_some() {
                    continue;
                }
                if !now
                    .signed_duration_since(since)
                    .to_std()
                    .is_ok_and(|elapsed| elapsed >= timeout)
                {
                    continue;
                }
                if d.reminders == 0 {
                    out.push(Due {
                        id: id.clone(),
                        rcpt: r.clone(),
                        action: Action::Remind,
                    });
                } else if !escalated {
                    out.push(Due {
                        id: id.clone(),
                        rcpt: r.clone(),
                        action: Action::Escalate,
                    });
                    escalated = true;
                }
            }
        }
        out
    }

    /// Returns a reply or an acknowledged outgoing message, including outcomes
    /// already recorded before the call. Blocks until one appears or times out.
    pub fn wait(&self, target: &str, who: &str, timeout: Option<Duration>) -> Result<Message> {
        self.wait_with_cancel(target, who, timeout, &AtomicBool::new(false))
    }

    /// Waits without occupying a blocking worker. Dropping the future releases
    /// its notification subscription immediately.
    pub async fn wait_async(
        &self,
        target: &str,
        who: &str,
        timeout: Option<Duration>,
    ) -> Result<Message> {
        let mut updates = self.changed.asynchronous.subscribe();
        let deadline = timeout.map(|duration| tokio::time::Instant::now() + duration);
        loop {
            if let Some(outcome) = wait_scan(&self.lock(), target, who) {
                return outcome;
            }
            match deadline {
                Some(deadline) => {
                    if tokio::time::timeout_at(deadline, updates.changed())
                        .await
                        .is_err()
                    {
                        return Err(Error::Timeout {
                            target: target.to_string(),
                        });
                    }
                }
                None => {
                    let _ = updates.changed().await;
                }
            }
        }
    }

    /// Like `wait`, but also releases a blocked waiter when its caller goes away.
    /// The cancellation flag is checked at most 100 ms after it changes.
    pub fn wait_with_cancel(
        &self,
        target: &str,
        who: &str,
        timeout: Option<Duration>,
        cancelled: &AtomicBool,
    ) -> Result<Message> {
        let start = Instant::now();
        let mut guard = self.lock();
        loop {
            match wait_scan(&guard, target, who) {
                Some(Ok(m)) => return Ok(m),
                Some(Err(e)) => return Err(e),
                None => {}
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let left = timeout.map(|d| d.saturating_sub(start.elapsed()));
            if left.is_some_and(|left| left.is_zero()) {
                return Err(Error::Timeout {
                    target: target.to_string(),
                });
            }
            let interval = Duration::from_millis(100);
            let delay = left.map_or(interval, |left| left.min(interval));
            let (next, _) = self
                .changed
                .sync
                .wait_timeout(guard, delay)
                .unwrap_or_else(|e| e.into_inner());
            guard = next;
        }
    }
}

fn wait_scan(s: &Inner, target: &str, who: &str) -> Option<Result<Message>> {
    let mut thread = target.to_string();
    let original = s.msgs.get(target);
    if let Some(o) = original {
        thread = o.thread.clone();
    }
    let mut found = false;
    for id in &s.order {
        let m = match s.msgs.get(id) {
            Some(m) => m,
            None => continue,
        };
        if m.thread != thread {
            continue;
        }
        found = true;
        if original.is_some() && m.id != target && m.reply_to != target {
            continue;
        }
        if m.from != who && m.deliveries.contains_key(who) && (original.is_none() || m.id != target)
        {
            return Some(Ok(m.clone()));
        }
        if m.from == who && m.deliveries.values().any(|d| d.acked.is_some()) {
            return Some(Ok(m.clone()));
        }
    }
    if !found {
        return Some(Err(Error::NotFound {
            id: target.to_string(),
        }));
    }
    None
}

fn op(
    s: &mut Inner,
    changed: &ChangeSignal,
    op: &str,
    id: &str,
    rcpt: &str,
    extra: impl FnOnce(&mut Record),
) -> Result<()> {
    if !s.msgs.contains_key(id) {
        return Err(Error::NotFound { id: id.to_string() });
    }
    if !rcpt.is_empty() && !s.msgs[id].deliveries.contains_key(rcpt) {
        return Err(Error::NotRecipient {
            rcpt: rcpt.to_string(),
            id: id.to_string(),
        });
    }
    let mut rec = Record {
        op: op.to_string(),
        at: Utc::now(),
        id: id.to_string(),
        rcpt: rcpt.to_string(),
        ..Default::default()
    };
    extra(&mut rec);
    commit(s, changed, rec)
}

/// Appends rec durably and applies it. The state changes only if the write
/// succeeded.
fn commit(s: &mut Inner, changed: &ChangeSignal, rec: Record) -> Result<()> {
    let mut line = serde_json::to_vec(&rec).map_err(|e| Error::corrupt(e.to_string()))?;
    line.push(b'\n');
    let file = match s.file.as_mut() {
        Some(f) => f,
        None => return Err(Error::Closed),
    };
    file.write_all(&line)
        .map_err(|e| Error::io("write mailbox", e))?;
    file.sync_all().map_err(|e| Error::io("sync mailbox", e))?;
    apply(s, rec).map_err(|message| Error::Corrupt {
        path: s.path.display().to_string(),
        line: 0,
        message,
    })?;
    changed.notify_all();
    Ok(())
}

fn apply(s: &mut Inner, rec: Record) -> std::result::Result<(), String> {
    if rec.op == "send" {
        let mut m = match rec.msg {
            Some(m) => m,
            None => return Err("send record has no message".to_string()),
        };
        m.deliveries = BTreeMap::new();
        for r in m.recipients.clone() {
            m.deliveries.insert(r, Delivery::default());
        }
        s.next_msg = s.next_msg.max(id_number(&m.id));
        s.next_thr = s.next_thr.max(id_number(&m.thread));
        if !m.request_id.is_empty() {
            s.by_req
                .insert(req_key(&m.from, &m.request_id), m.id.clone());
        }
        s.order.push(m.id.clone());
        s.msgs.insert(m.id.clone(), m);
        return Ok(());
    }
    let m = match s.msgs.get_mut(&rec.id) {
        Some(m) => m,
        None => return Ok(()),
    };
    let at = rec.at;
    let delivery = if rec.rcpt.is_empty() {
        None
    } else {
        m.deliveries.get_mut(&rec.rcpt)
    };
    match rec.op.as_str() {
        "resolve" => {
            if !m.recipients.is_empty() {
                return Ok(());
            }
            for r in rec.to {
                m.recipients.push(r.clone());
                m.deliveries.insert(r, Delivery::default());
            }
        }
        "arm" => {
            if let Some(d) = delivery {
                d.since.get_or_insert(at);
            }
        }
        "deliver" => {
            if let Some(d) = delivery {
                d.delivered = Some(at);
                d.since = Some(at);
            }
        }
        "read" => {
            if let Some(d) = delivery {
                d.read.get_or_insert(at);
            }
        }
        "ack" => {
            if let Some(d) = delivery {
                d.acked.get_or_insert(at);
            }
        }
        "remind" => {
            if let Some(d) = delivery {
                d.reminders += 1;
                d.since = Some(at);
                d.delivered = Some(at);
            }
        }
        "escalate" => {
            if m.escalated.is_none() {
                m.escalated = Some(at);
            }
        }
        "backlog" => {
            m.note_id = rec.item;
        }
        _ => {}
    }
    Ok(())
}

fn replay(s: &mut Inner) -> Result<()> {
    let mut file = match s.file.take() {
        Some(f) => f,
        None => return Err(Error::Closed),
    };
    let res = replay_file(s, &mut file);
    s.file = Some(file);
    res
}

fn replay_file(s: &mut Inner, file: &mut File) -> Result<()> {
    file.rewind().map_err(|e| Error::io("read mailbox", e))?;
    let mut partial_at: Option<i64> = None;
    let mut off: i64 = 0;
    let mut line_no: usize = 0;
    {
        let mut reader = BufReader::with_capacity(1 << 20, &mut *file);
        loop {
            let mut line = Vec::new();
            let got = reader
                .read_until(b'\n', &mut line)
                .map_err(|e| Error::io("read mailbox", e))?;
            if got == 0 {
                break;
            }
            if !line.ends_with(b"\n") {
                partial_at = Some(off);
                break;
            }
            off += line.len() as i64;
            line_no += 1;
            let rec: Record = serde_json::from_slice(&line).map_err(|e| Error::Corrupt {
                path: s.path.display().to_string(),
                line: line_no,
                message: e.to_string(),
            })?;
            apply(s, rec).map_err(|message| Error::Corrupt {
                path: s.path.display().to_string(),
                line: line_no,
                message,
            })?;
        }
    }
    if let Some(at) = partial_at {
        file.set_len(at as u64)
            .map_err(|e| Error::io("cut partial mailbox line", e))?;
    }
    Ok(())
}
