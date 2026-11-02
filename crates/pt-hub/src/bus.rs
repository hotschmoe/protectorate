//! The event stream behind `GET /api/events`: an in-memory ring of [`HubStreamMessage`] texts in
//! front of the stream log (`stream.rs`). Every kept message takes the next seq; a streaming
//! delta repeats the seq of the last kept message, reaches live subscribers only and is never
//! replayed on resume.
//!
//! Group commit: a push only queues the message. The writer thread appends what is queued with
//! one fdatasync and then makes it visible, so subscribers, `mark`, `pos` and `resume` see only
//! messages already on disk. Seqs therefore continue after a restart without a client ever
//! having seen one the log lacks.
//!
//! Subscribers walk the ring by position (one per message, deltas included), not by seq. The
//! condvar is also the hub's "something changed" signal for `Wait`: the core publishes the view
//! before it pushes the change, so a waiter that read position `n`, then found its condition
//! false, wakes for any change after that check.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pt_api::{EpochMs, EventKind, HubEvent, HubStreamMessage, Level};
use pt_json::{FromJson, ToJson};

use crate::home::now_ms;
use crate::lock::lock;
use crate::log::{error, info, warning};
use crate::stream::{self, Log, Rec};

const RING: usize = 8192;
/// Hub events a snapshot carries; `ListEvents` pages through every retained one.
pub const SNAPSHOT_EVENTS: usize = 500;

#[derive(Clone)]
pub struct Item {
    pub pos: u64,
    pub seq: u64,
    pub kept: bool,
    pub text: Arc<str>,
}

struct Ring {
    /// The seq the next kept message takes.
    next_seq: u64,
    /// The position the next message takes.
    next_pos: u64,
    /// Messages up to this position are on disk and visible.
    durable: u64,
    /// The seq of the last kept message on disk.
    durable_seq: u64,
    /// From this seq on, the log holds every kept message.
    window: u64,
    items: VecDeque<Item>,
    /// Kept messages pushed but not yet written.
    queued: Vec<Rec>,
    /// Retained hub events by their seq, for `GetState`, `ListEvents` and `MarkEventsRead`.
    events: BTreeMap<u64, HubEvent>,
}

pub struct Bus {
    path: PathBuf,
    ring: Mutex<Ring>,
    /// Subscribers and waiters: the durable position moved.
    cond: Condvar,
    /// The writer: something was pushed.
    work: Condvar,
}

/// What a subscriber gets: the messages after its position, or a gap when the ring moved past it.
pub enum Since {
    Items(Vec<Item>),
    Gap,
}

/// Where a resuming subscriber starts.
pub enum Resume {
    /// Stream from the ring after this position.
    Ring(u64),
    /// Send the logged messages `from <= seq < until` first, then stream after `cursor`.
    Log { from: u64, until: u64, cursor: u64 },
}

impl Bus {
    /// Opens the stream log at `path`. Seqs continue after the last logged message; a new log
    /// starts at the clock in ms, above any seq an earlier hub without a log handed out.
    pub fn open(path: PathBuf) -> io::Result<(Bus, Log)> {
        let o = stream::open(&path)?;
        if o.cut > 0 {
            warning!("stream log: cut {} bytes of a torn tail", o.cut);
        }
        let last = o.recs.last().map(|r| r.seq);
        let next_seq = match last {
            Some(s) => s + 1,
            None => o.window.unwrap_or(0).max(now_ms()).max(1),
        };
        let window = o
            .window
            .unwrap_or(o.recs.first().map_or(next_seq, |r| r.seq));
        let mut events = BTreeMap::new();
        for r in o.recs.iter().filter(|r| r.event.is_some()) {
            match pt_json::parse(&r.text)
                .map_err(|e| e.to_string())
                .and_then(|v| HubStreamMessage::from_json(&v).map_err(|e| e.to_string()))
            {
                Ok(HubStreamMessage::Event { event, .. }) => {
                    events.insert(event.seq, event);
                }
                Ok(_) => warning!("stream log: message {} is not an event", r.seq),
                Err(e) => warning!("stream log: message {}: {e}", r.seq),
            }
        }
        let live: Vec<&Rec> = o.recs.iter().filter(|r| r.seq >= window).collect();
        let mut items = VecDeque::with_capacity(RING);
        for (i, r) in live[live.len().saturating_sub(RING)..].iter().enumerate() {
            items.push_back(Item {
                pos: i as u64 + 1,
                seq: r.seq,
                kept: true,
                text: r.text.clone(),
            });
        }
        let n = items.len() as u64;
        info!(
            "stream log: {} messages from seq {window}, {} events, next seq {next_seq}",
            live.len(),
            events.len()
        );
        let bus = Bus {
            path,
            ring: Mutex::new(Ring {
                next_seq,
                next_pos: n + 1,
                durable: n,
                durable_seq: next_seq - 1,
                window,
                items,
                queued: Vec::new(),
                events,
            }),
            cond: Condvar::new(),
            work: Condvar::new(),
        };
        Ok((bus, o.log))
    }

    /// The writer thread: appends what was pushed with one fdatasync per batch, then makes it
    /// visible; compacts the log when it has grown to twice what retention keeps. Without the
    /// log no seq can be promised, so a failed append stops the hub.
    pub fn write(&self, mut log: Log) {
        loop {
            let (batch, upto) = {
                let mut r = lock(&self.ring);
                while r.queued.is_empty() && r.durable + 1 == r.next_pos {
                    r = self
                        .work
                        .wait(r)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                (std::mem::take(&mut r.queued), r.next_pos - 1)
            };
            if !batch.is_empty()
                && let Err(e) = log.append(&batch)
            {
                error!("stream log append failed, stopping: {e}");
                std::process::exit(70);
            }
            let next_seq = {
                let mut r = lock(&self.ring);
                r.durable = upto;
                if let Some(last) = batch.last() {
                    r.durable_seq = last.seq;
                }
                r.durable_seq + 1
            };
            self.cond.notify_all();
            if log.wants_compaction() {
                match log.compact(now_ms(), next_seq) {
                    Ok(c) => {
                        let mut r = lock(&self.ring);
                        r.window = c.window;
                        r.events.retain(|seq, _| !c.dropped_events.contains(seq));
                        info!("stream log compacted: window from seq {}", c.window);
                    }
                    Err(e) => warning!("stream log compaction failed: {e}"),
                }
            }
        }
    }

    fn add(r: &mut Ring, seq: u64, kept: bool, text: Arc<str>) {
        let pos = r.next_pos;
        r.next_pos += 1;
        if r.items.len() == RING {
            r.items.pop_front();
        }
        r.items.push_back(Item {
            pos,
            seq,
            kept,
            text,
        });
    }

    /// Queues the message `f` builds for the next seq.
    pub fn push(&self, f: impl FnOnce(u64) -> HubStreamMessage) -> u64 {
        let mut r = lock(&self.ring);
        let seq = r.next_seq;
        r.next_seq += 1;
        let msg = f(seq);
        let event = match &msg {
            HubStreamMessage::Event { event, .. } => {
                r.events.insert(event.seq, event.clone());
                Some(event.seq)
            }
            _ => None,
        };
        let text: Arc<str> = msg.to_json().to_string().into();
        r.queued.push(Rec {
            seq,
            at_ms: now_ms(),
            event,
            text: text.clone(),
        });
        Self::add(&mut r, seq, true, text);
        drop(r);
        self.work.notify_one();
        seq
    }

    /// Streams a line of the mail and events log.
    pub fn event(&self, kind: EventKind, level: Level, source: &str, target: &str, text: &str) {
        let at = EpochMs(now_ms());
        self.push(|seq| HubStreamMessage::Event {
            seq,
            event: HubEvent {
                seq,
                at_ms: at,
                kind,
                source: source.to_string(),
                target: Some(target.to_string()),
                text: text.to_string(),
                read: false,
                level,
            },
        });
    }

    /// Queues a streaming delta under the last kept seq; it is not logged or replayed.
    pub fn push_delta(&self, f: impl FnOnce(u64) -> HubStreamMessage) {
        let mut r = lock(&self.ring);
        let seq = r.next_seq - 1;
        let text = f(seq).to_json().to_string().into();
        Self::add(&mut r, seq, false, text);
        drop(r);
        self.work.notify_one();
    }

    /// The seq of the latest visible kept message, and the latest visible position.
    pub fn mark(&self) -> (u64, u64) {
        let r = lock(&self.ring);
        (r.durable_seq, r.durable)
    }

    /// The latest visible position.
    pub fn pos(&self) -> u64 {
        lock(&self.ring).durable
    }

    /// Where a subscriber resuming at kept message `from_seq` starts, when the ring or the log
    /// still holds every kept message from there on (or `from_seq` is the next seq).
    pub fn resume(&self, from_seq: u64) -> Option<Resume> {
        let r = lock(&self.ring);
        if from_seq == r.durable_seq + 1 {
            return Some(Resume::Ring(r.durable));
        }
        if from_seq > r.durable_seq || from_seq < r.window {
            return None;
        }
        let first = r.items.iter().find(|i| i.kept && i.pos <= r.durable);
        match first {
            Some(f) if f.seq <= from_seq => r
                .items
                .iter()
                .find(|i| i.kept && i.seq == from_seq && i.pos <= r.durable)
                .map(|i| Resume::Ring(i.pos - 1)),
            Some(f) => Some(Resume::Log {
                from: from_seq,
                until: f.seq,
                cursor: f.pos - 1,
            }),
            None => Some(Resume::Log {
                from: from_seq,
                until: r.durable_seq + 1,
                cursor: r.durable,
            }),
        }
    }

    /// The texts of the logged messages `from <= seq < until`, or `None` when any is gone.
    pub fn logged(&self, from: u64, until: u64) -> Option<Vec<Arc<str>>> {
        match stream::read_range(&self.path, from, until) {
            Ok(got) => got.map(|v| v.into_iter().map(|r| r.text).collect()),
            Err(e) => {
                warning!("stream log: {e}");
                None
            }
        }
    }

    /// The latest `n` retained hub events, oldest first.
    pub fn recent_events(&self, n: usize) -> Vec<HubEvent> {
        let r = lock(&self.ring);
        let skip = r.events.len().saturating_sub(n);
        r.events.values().skip(skip).cloned().collect()
    }

    /// Up to `limit` retained hub events from seq `from`, oldest first.
    pub fn events_from(&self, from: u64, limit: usize) -> Vec<HubEvent> {
        lock(&self.ring)
            .events
            .range(from..)
            .take(limit)
            .map(|(_, e)| e.clone())
            .collect()
    }

    /// The newest `limit` retained events, oldest first.
    pub fn events_latest(&self, limit: usize) -> Vec<HubEvent> {
        let r = lock(&self.ring);
        let skip = r.events.len().saturating_sub(limit);
        r.events.values().skip(skip).cloned().collect()
    }

    /// Marks every event up to `seq` read; returns the ones that changed.
    pub fn mark_read(&self, seq: u64) -> Vec<HubEvent> {
        let mut r = lock(&self.ring);
        let mut changed = Vec::new();
        for e in r.events.range_mut(..=seq).map(|(_, e)| e) {
            if !e.read {
                e.read = true;
                changed.push(e.clone());
            }
        }
        changed
    }

    /// Blocks until a visible message after position `after` exists or `deadline` passes.
    pub fn wait_after(&self, after: u64, deadline: Instant) {
        let mut r = lock(&self.ring);
        while r.durable <= after {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            r = self
                .cond
                .wait_timeout(r, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// Visible messages after position `after`, waiting up to `wait` for the first one.
    pub fn since(&self, after: u64, wait: Duration) -> Since {
        self.wait_after(after, Instant::now() + wait);
        let r = lock(&self.ring);
        if r.items.front().is_some_and(|i| i.pos > after + 1) {
            return Since::Gap;
        }
        Since::Items(
            r.items
                .iter()
                .filter(|i| i.pos > after && i.pos <= r.durable)
                .cloned()
                .collect(),
        )
    }
}
