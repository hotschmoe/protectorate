//! The event stream behind `GET /api/events`: a bounded in-memory ring of [`HubStreamMessage`]
//! texts. Every kept message takes the next seq; a streaming delta repeats the seq of the last
//! kept message, reaches live subscribers only and is never replayed on resume.
//!
//! Subscribers walk the ring by position (one per message, deltas included), not by seq. The
//! condvar is also the hub's "something changed" signal for `Wait`: the core publishes the view
//! before it pushes the change, so a waiter that read position `n`, then found its condition
//! false, wakes for any change after that check.
//!
//! Seqs start at the hub's start time in ms, so a client resuming with a seq from an earlier hub
//! process asks for one that is too old or too new and gets a fresh snapshot.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pt_api::{HubEvent, HubStreamMessage};
use pt_json::ToJson;

use crate::home::now_ms;
use crate::transcript::lock;

const RING: usize = 8192;
const EVENTS: usize = 500;

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
    items: VecDeque<Item>,
    /// Recent hub events, for `GetState`, `ListEvents` and `MarkEventsRead`.
    events: VecDeque<HubEvent>,
}

pub struct Bus {
    ring: Mutex<Ring>,
    cond: Condvar,
}

/// What a subscriber gets: the messages after its position, or a gap when the ring moved past it.
pub enum Since {
    Items(Vec<Item>),
    Gap,
}

impl Default for Bus {
    fn default() -> Bus {
        Bus {
            ring: Mutex::new(Ring {
                next_seq: now_ms().max(1),
                next_pos: 1,
                items: VecDeque::new(),
                events: VecDeque::new(),
            }),
            cond: Condvar::new(),
        }
    }
}

impl Ring {
    fn add(&mut self, seq: u64, kept: bool, msg: &HubStreamMessage) -> u64 {
        let pos = self.next_pos;
        self.next_pos += 1;
        if self.items.len() == RING {
            self.items.pop_front();
        }
        self.items.push_back(Item {
            pos,
            seq,
            kept,
            text: msg.to_json().to_string().into(),
        });
        pos
    }
}

impl Bus {
    /// Pushes the message `f` builds for the next seq and wakes every waiter.
    pub fn push(&self, f: impl FnOnce(u64) -> HubStreamMessage) -> u64 {
        let mut r = lock(&self.ring);
        let seq = r.next_seq;
        r.next_seq += 1;
        let msg = f(seq);
        if let HubStreamMessage::Event { event, .. } = &msg {
            if let Some(e) = r.events.iter_mut().find(|e| e.seq == event.seq) {
                *e = event.clone();
            } else {
                if r.events.len() == EVENTS {
                    r.events.pop_front();
                }
                r.events.push_back(event.clone());
            }
        }
        r.add(seq, true, &msg);
        drop(r);
        self.cond.notify_all();
        seq
    }

    /// Pushes a streaming delta under the last kept seq; it is not replayed on resume.
    pub fn push_delta(&self, f: impl FnOnce(u64) -> HubStreamMessage) {
        let mut r = lock(&self.ring);
        let seq = r.next_seq - 1;
        let msg = f(seq);
        r.add(seq, false, &msg);
        drop(r);
        self.cond.notify_all();
    }

    /// The seq of the latest kept message, and the position of the latest message.
    pub fn mark(&self) -> (u64, u64) {
        let r = lock(&self.ring);
        (r.next_seq - 1, r.next_pos - 1)
    }

    /// The position of the latest message.
    pub fn pos(&self) -> u64 {
        lock(&self.ring).next_pos - 1
    }

    /// The position just before the kept message `from_seq`, when the ring still holds every
    /// kept message from there on (or `from_seq` is the next seq).
    pub fn resume(&self, from_seq: u64) -> Option<u64> {
        let r = lock(&self.ring);
        if from_seq == r.next_seq {
            return Some(r.next_pos - 1);
        }
        let first = r.items.iter().find(|i| i.kept)?;
        if from_seq < first.seq || from_seq > r.next_seq {
            return None;
        }
        r.items
            .iter()
            .find(|i| i.kept && i.seq == from_seq)
            .map(|i| i.pos - 1)
    }

    pub fn events(&self) -> Vec<HubEvent> {
        lock(&self.ring).events.iter().cloned().collect()
    }

    /// Marks every event up to `seq` read; returns the ones that changed.
    pub fn mark_read(&self, seq: u64) -> Vec<HubEvent> {
        let mut r = lock(&self.ring);
        let mut changed = Vec::new();
        for e in r.events.iter_mut().filter(|e| e.seq <= seq && !e.read) {
            e.read = true;
            changed.push(e.clone());
        }
        changed
    }

    /// Blocks until a message after position `after` exists or `deadline` passes.
    pub fn wait_after(&self, after: u64, deadline: Instant) {
        let mut r = lock(&self.ring);
        while r.next_pos - 1 <= after {
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

    /// Messages after position `after`, waiting up to `wait` for the first one.
    pub fn since(&self, after: u64, wait: Duration) -> Since {
        self.wait_after(after, Instant::now() + wait);
        let r = lock(&self.ring);
        if r.items.front().is_some_and(|i| i.pos > after + 1) {
            return Since::Gap;
        }
        Since::Items(r.items.iter().filter(|i| i.pos > after).cloned().collect())
    }
}
