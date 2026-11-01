//! The event stream behind `GET /api/events`: a bounded in-memory ring of JSON messages with a
//! monotonic seq. Its condvar is also the hub's "something changed" signal for `Wait`: the core
//! publishes the view before it pushes the change, so a waiter that read seq `n`, then found its
//! condition false, wakes for any change after that check.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pt_json::Value;

use crate::transcript::lock;

const RING: usize = 8192;
const EVENTS: usize = 200;

struct Ring {
    next: u64,
    items: VecDeque<(u64, Arc<str>)>,
    /// Recent hub events (notices), for the `events` list of `GetState`.
    events: VecDeque<Value>,
}

pub struct Bus {
    ring: Mutex<Ring>,
    cond: Condvar,
}

/// What a subscriber gets: the messages after its cursor, or a gap when the ring moved past it.
pub enum Since {
    Items(Vec<(u64, Arc<str>)>),
    Gap,
}

impl Default for Bus {
    fn default() -> Bus {
        Bus {
            ring: Mutex::new(Ring {
                next: 1,
                items: VecDeque::new(),
                events: VecDeque::new(),
            }),
            cond: Condvar::new(),
        }
    }
}

impl Bus {
    /// Pushes `{"seq": n, ...fields of msg}` and wakes every waiter.
    pub fn push(&self, mut msg: Value) -> u64 {
        let mut r = lock(&self.ring);
        let seq = r.next;
        r.next += 1;
        if let Some(m) = msg.as_object_mut() {
            m.insert("seq", seq);
        }
        if msg["kind"].as_str() == Some("event") {
            if r.events.len() == EVENTS {
                r.events.pop_front();
            }
            r.events.push_back(msg["event"].clone());
        }
        if r.items.len() == RING {
            r.items.pop_front();
        }
        r.items.push_back((seq, msg.to_string().into()));
        drop(r);
        self.cond.notify_all();
        seq
    }

    /// The seq of the latest message (0 before the first).
    pub fn seq(&self) -> u64 {
        lock(&self.ring).next - 1
    }

    pub fn events(&self) -> Vec<Value> {
        lock(&self.ring).events.iter().cloned().collect()
    }

    /// Blocks until a message after `after` exists or `deadline` passes; returns the latest seq.
    pub fn wait_after(&self, after: u64, deadline: Instant) -> u64 {
        let mut r = lock(&self.ring);
        while r.next - 1 <= after {
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
        r.next - 1
    }

    /// Messages after `after`, waiting up to `wait` for the first one.
    pub fn since(&self, after: u64, wait: Duration) -> Since {
        self.wait_after(after, Instant::now() + wait);
        let r = lock(&self.ring);
        if r.items.front().is_some_and(|(s, _)| *s > after + 1) {
            return Since::Gap;
        }
        Since::Items(
            r.items
                .iter()
                .filter(|(s, _)| *s > after)
                .cloned()
                .collect(),
        )
    }
}
