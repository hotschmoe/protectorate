//! Subscription quota for dispatch (SPEC 8).
//!
//! - Readers (`claude`, `codex`, `grok`, `local`) are read-only: they use the owner's existing
//!   login files and the same read-only usage endpoints the provider CLIs use. They never refresh
//!   or write a token, and nothing secret is printed or returned.
//! - `Cache` keeps the last good reading per subscription, with a TTL and error backoff, and
//!   turns it into `pt_api::Quota` values.
//! - `select_target` and `spare_trigger` are pure functions over those values; `task_size`
//!   derives the size class (D27).

#![forbid(unsafe_code)]

mod cache;
pub mod claude;
pub mod codex;
mod error;
pub mod grok;
pub mod http;
pub mod local;
mod login;
mod reader;
mod select;
mod state;
pub mod task_size;
mod time;

pub use cache::{Cache, Options};
pub use error::{Error, Result};
pub use reader::{Reader, Snapshot};
pub use select::{
    Candidate, Health, Spare, Target, select_target, select_with_fallback, spare_quota,
    spare_trigger, window_length_ms,
};
pub use state::{State, state};
pub use task_size::{TaskSize, Thresholds, derive, median_of_last};
pub use time::{fmt_age, now_ms, parse_rfc3339};
