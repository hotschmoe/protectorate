//! Subscription quota for dispatch (SPEC 8).
//!
//! - Readers (`claude`, `codex`, `grok`, `local`) are read-only: they use the owner's existing
//!   login files and the same read-only usage endpoints the provider CLIs use. They never refresh
//!   or write a token, and nothing secret is printed or returned.
//! - `Cache` keeps the last good reading per subscription, with a TTL and error backoff, and
//!   turns it into `pt_api::Quota` values.
//! - `select_target` and `spare_trigger` are pure functions over those values; `task_size`
//!   derives the size class (D27).
//!
//! A `pt_api::Quota` has three reading values (`ok`, `stale`, `error`). The fourth state here,
//! unknown (nothing to read: not logged in, not configured), is an `error` reading whose note
//! starts with [`UNKNOWN_PREFIX`]; `state` tells the four apart.

#![forbid(unsafe_code)]

mod cache;
pub mod claude;
pub mod codex;
mod error;
pub mod grok;
pub mod http;
pub mod local;
mod select;
pub mod task_size;
mod time;

use std::time::Duration;

use pt_api::{Quota, QuotaReading, QuotaWindow, Subscription};

pub use cache::{Cache, Options};
pub use error::{Error, Result};
pub use select::{
    Health, Spare, Target, select_target, spare_quota, spare_trigger, window_length_ms,
};
pub use task_size::{TaskSize, Thresholds, derive, median_of_last};
pub use time::{fmt_age, now_ms, parse_rfc3339};

pub const UNKNOWN_PREFIX: &str = "unknown: ";

/// The four reading states of a quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Fresh,
    Stale,
    Unknown,
    Error,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Fresh => "fresh",
            State::Stale => "stale",
            State::Unknown => "unknown",
            State::Error => "error",
        }
    }
}

pub fn state(q: &Quota) -> State {
    match q.reading {
        QuotaReading::Ok => State::Fresh,
        QuotaReading::Stale => State::Stale,
        QuotaReading::Error if q.note.starts_with(UNKNOWN_PREFIX) => State::Unknown,
        QuotaReading::Error => State::Error,
    }
}

/// What a reader found in one successful read.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub plan: String,
    pub windows: Vec<QuotaWindow>,
    pub note: String,
}

/// A read-only quota source for one subscription.
pub trait Reader: Send + Sync {
    fn subscription(&self) -> Subscription;
    /// `now_ms` is the clock for expiry checks; `timeout` bounds the whole read.
    fn read(&self, now_ms: u64, timeout: Duration) -> Result<Snapshot>;
}

/// The login-file home: `$HOME`, or an override.
pub(crate) fn home_dir(over: Option<&std::path::Path>) -> Result<std::path::PathBuf> {
    if let Some(p) = over {
        return Ok(p.to_path_buf());
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| Error::other("HOME is not set"))
}

pub(crate) fn read_login(path: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::unavailable("not logged in (no login file)"),
        _ => Error::other(format!("open login file: {e}")),
    })?;
    let mut buf = Vec::new();
    file.take(1 << 20)
        .read_to_end(&mut buf)
        .map_err(|e| Error::other(format!("read login file: {e}")))?;
    Ok(buf)
}

pub(crate) fn clamp_pct(p: f64) -> Option<f64> {
    p.is_finite().then(|| p.max(0.0))
}

/// `300` -> `5h`, `10080` -> `wk`, `1440` -> `day`; other lengths are `Nd`, `Nh` or `Nm`.
pub(crate) fn window_label(mins: i64) -> String {
    match mins {
        m if m <= 0 => "window".to_string(),
        10080 => "wk".to_string(),
        1440 => "day".to_string(),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

pub(crate) fn window(
    label: impl Into<String>,
    used_pct: f64,
    resets_at_ms: Option<u64>,
) -> QuotaWindow {
    QuotaWindow {
        label: label.into(),
        used_pct: clamp_pct(used_pct),
        resets_at_ms: resets_at_ms.map(pt_api::EpochMs),
    }
}
