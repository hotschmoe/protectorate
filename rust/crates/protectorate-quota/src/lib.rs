//! Subscription usage windows and reset credits read from provider accounts
//! with read-only calls and locally stored credentials. A Rust port of
//! `internal/quota`; the wire format is this crate's own serde types, not the
//! Go structs or the protobuf messages byte for byte.
//!
//! Providers: `claude`, `codex`, `grok`, `antigravity` and `local`
//! (an OpenAI-compatible endpoint, which has no quota but reports reachability
//! and served models). `cache` keeps the last good snapshot per provider and
//! computes freshness; `suggest` ranks the sleeve harnesses by spare capacity
//! that would otherwise expire at a reset.

mod antigravity;
mod cache;
mod claude;
mod codex;
mod grok;
mod http;
mod local;
mod proc;
mod suggest;

pub use antigravity::Antigravity;
pub use cache::{
    window_length, Freshness, ProviderQuota, QuotaCache, QuotaOptions, QuotaWindow, ResetCredit,
};
pub use claude::Claude;
pub use codex::Codex;
pub use grok::Grok;
#[cfg(feature = "net")]
pub use http::UreqTransport;
pub use http::{do_raw, get_json, Request, Response, Transport};
pub use local::Local;
pub use suggest::{rank_harnesses, Harness, HarnessSuggestion, TaskSize};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

/// Bodies and command output are never read past this many bytes.
pub const MAX_BODY: usize = 1 << 20;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "used_percent", default)]
    pub used_percent: f64,
    #[serde(rename = "resets_at", default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reset {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub count: i32,
    #[serde(
        rename = "expires_at",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub expires_at: Option<DateTime<Utc>>,
}

/// One provider's usage. A failed read is a snapshot with `error` set, so a
/// batch of reads is always complete.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<Window>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resets: Vec<Reset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency: Option<u64>,
    #[serde(
        rename = "fetched_at",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fetched_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Credentials are expired and the provider CLI must re-login.
    Expired,
    /// The provider rejected the credentials.
    Unauthorized,
    /// The credentials lack the scope needed for usage queries.
    Scope,
    /// The source is not usable: not installed, not logged in, no such data.
    Unavailable(String),
    /// An unexpected HTTP status.
    Http(u16),
    /// The read deadline passed.
    Deadline,
    /// Anything else, already wrapped with context.
    Other(String),
}

impl Error {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Error::Unavailable(detail.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expired => f.write_str("credentials expired; re-login with the provider CLI"),
            Self::Unauthorized => f.write_str("credentials rejected (HTTP 401)"),
            Self::Scope => f.write_str("credentials lack the scope required for usage queries"),
            Self::Unavailable(d) => write!(f, "source unavailable: {d}"),
            Self::Http(s) => write!(f, "unexpected HTTP status {s}"),
            Self::Deadline => f.write_str("deadline exceeded"),
            Self::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A read's clock and cancellation bound, the port of Go's `context.Context`
/// plus the injected `Now` function.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub now: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    wall_deadline: Option<std::time::Instant>,
}

impl Ctx {
    pub fn at(now: DateTime<Utc>) -> Self {
        Self {
            now,
            deadline: None,
            wall_deadline: None,
        }
    }

    pub fn now() -> Self {
        Self::at(Utc::now())
    }

    /// A copy bounded by `timeout` from `self.now`, never past the current deadline.
    pub fn bounded(&self, timeout: Duration) -> Self {
        let until = self
            .now
            .checked_add_signed(
                chrono::Duration::from_std(timeout).unwrap_or(chrono::Duration::MAX),
            )
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        let deadline = match self.deadline {
            Some(d) if d < until => Some(d),
            _ => Some(until),
        };
        let wall_until = std::time::Instant::now().checked_add(timeout);
        let wall_deadline = match (self.wall_deadline, wall_until) {
            (Some(old), Some(new)) => Some(old.min(new)),
            (Some(old), None) => Some(old),
            (None, new) => new,
        };
        Self {
            now: self.now,
            deadline,
            wall_deadline,
        }
    }

    pub fn expired(&self) -> bool {
        self.deadline.is_some_and(|d| self.now >= d)
            || self
                .wall_deadline
                .is_some_and(|d| std::time::Instant::now() >= d)
    }

    pub fn remaining(&self) -> Option<Duration> {
        let logical = self.deadline.map(|d| {
            d.signed_duration_since(self.now)
                .to_std()
                .unwrap_or(Duration::ZERO)
        });
        let wall = self
            .wall_deadline
            .map(|d| d.saturating_duration_since(std::time::Instant::now()));
        match (logical, wall) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}

/// A read-only quota source for one provider.
pub trait Reader: Send + Sync {
    fn provider(&self) -> &str;
    fn read(&self, ctx: &Ctx, http: &dyn Transport) -> Result<Snapshot>;
}

/// Runs every reader concurrently, each bounded by `timeout`, and returns one
/// snapshot per reader in input order. Failures, including a reader that
/// panics, are folded into `Snapshot::error`.
pub fn read_all(
    ctx: &Ctx,
    readers: &[&dyn Reader],
    http: &dyn Transport,
    timeout: Duration,
) -> Vec<Snapshot> {
    std::thread::scope(|s| {
        let handles: Vec<_> = readers
            .iter()
            .map(|&r| {
                let sub = ctx.bounded(timeout);
                s.spawn(move || {
                    let mut snap = safe_read(r, &sub, http);
                    if snap.fetched_at.is_none() {
                        snap.fetched_at = Some(sub.now);
                    }
                    snap
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Snapshot::default()))
            .collect()
    })
}

pub(crate) fn safe_read(r: &dyn Reader, ctx: &Ctx, http: &dyn Transport) -> Snapshot {
    let provider = r.provider().to_string();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| r.read(ctx, http)));
    let mut snap = match res {
        Ok(Ok(snap)) => snap,
        Ok(Err(err)) => Snapshot {
            provider: provider.clone(),
            error: err.to_string(),
            ..Snapshot::default()
        },
        Err(_) => Snapshot {
            provider: provider.clone(),
            error: "reader panic".to_string(),
            ..Snapshot::default()
        },
    };
    snap.provider = provider;
    snap
}

pub(crate) fn home_dir(over: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = over {
        return Ok(p.to_path_buf());
    }
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var(key)
        .map(PathBuf::from)
        .map_err(|_| Error::Other(format!("home directory not found: {key} is unset")))
}

pub(crate) fn read_cred_file(path: &Path) -> Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(d) => Ok(d),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::unavailable("credentials file not found"))
        }
        Err(e) => Err(Error::Other(format!("read credentials: {e}"))),
    }
}

/// RFC 3339 timestamps; anything unparsable is "no time".
pub(crate) fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

pub(crate) fn clamp_percent(p: f64) -> f64 {
    if p < 0.0 {
        0.0
    } else {
        p
    }
}

/// "5h", "7d", "10080" -> "7d"; unknown lengths fall back to "window".
pub(crate) fn window_label(mins: i64) -> String {
    match mins {
        m if m <= 0 => "window".to_string(),
        300 => "5h".to_string(),
        10080 => "7d".to_string(),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{}m", m),
    }
}

/// A duration as "42m", "3h07m" or "9d04h".
pub(crate) fn fmt_age(d: Duration) -> String {
    let mins = (d.as_secs() as i64 + 30) / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    if mins < 48 * 60 {
        return format!("{}h{:02}m", mins / 60, mins % 60);
    }
    let hours = mins / 60;
    format!("{}d{:02}h", hours / 24, hours % 24)
}

pub(crate) fn sort_by_expiry<T, F>(v: &mut [T], expires_at: F)
where
    F: Fn(&T) -> Option<DateTime<Utc>>,
{
    v.sort_by(|a, b| match (expires_at(a), expires_at(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        _ => std::cmp::Ordering::Equal,
    });
}
