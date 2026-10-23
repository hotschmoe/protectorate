use protectorate_mailbox::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Test mailboxes live under a namespaced temp dir and are left in place;
/// nothing in this crate deletes anything.
pub fn tmp_path(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("protectorate-mailbox-tests");
    std::fs::create_dir_all(&base).expect("temp dir");
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("{}-{}-{}-{}", name, std::process::id(), nanos, seq));
    std::fs::create_dir_all(&dir).expect("temp case dir");
    dir.join("mailbox.jsonl")
}

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn copy_fixture(name: &str, case: &str) -> PathBuf {
    let dst = tmp_path(case);
    std::fs::copy(fixture(name), &dst).expect("copy fixture");
    dst
}

/// A clock that hands out one second per call, like the Go fixture's clock.
pub struct Ticker {
    base: i64,
    n: Arc<AtomicI64>,
}

impl Ticker {
    pub fn new(base: i64) -> Ticker {
        Ticker {
            base,
            n: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn clock(&self) -> impl Fn() -> Time + Send + Sync + 'static {
        let n = self.n.clone();
        let base = self.base;
        move || Time::from_secs_nanos(base + 1 + n.fetch_add(1, Ordering::SeqCst), 0)
    }

    pub fn calls(&self) -> i64 {
        self.n.load(Ordering::SeqCst)
    }
}

pub fn fixed_clock(t: Time) -> impl Fn() -> Time + Send + Sync + 'static {
    move || t
}

/// A clock the test moves by hand.
pub struct Clock {
    secs: Arc<AtomicI64>,
}

impl Clock {
    pub fn new(sec: i64) -> Clock {
        Clock {
            secs: Arc::new(AtomicI64::new(T12 + sec)),
        }
    }

    pub fn clock(&self) -> impl Fn() -> Time + Send + Sync + 'static {
        let secs = self.secs.clone();
        move || Time::from_secs_nanos(secs.load(Ordering::SeqCst), 0)
    }

    pub fn set(&self, sec: i64) {
        self.secs.store(T12 + sec, Ordering::SeqCst);
    }
}

pub fn read_lines(path: &PathBuf) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("read mailbox");
    text.lines().map(|l| l.to_string()).collect()
}

pub const T12: i64 = 1_792_411_200; // 2026-10-19T12:00:00Z

pub fn at(sec: i64) -> Time {
    Time::from_secs_nanos(T12 + sec, 0)
}
