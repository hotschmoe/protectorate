//! Headless harness drivers: Claude Code stream-json, Codex app-server, Grok ACP and Pi rpc,
//! each mapped into one normalized event stream ([`Event`], [`Entry`]).
//!
//! A driver only speaks the harness's official headless protocol with its own flags, config
//! and auth. It never adds a sandbox: the caller passes a [`SpawnSpec::prefix`] command (bwrap,
//! systemd-run) that the harness argv is appended to.
//!
//! ```text
//! let (session, events) = pt_drivers::spawn(spec)?;
//! // wait for Entry::SessionInfo, then:
//! session.send_user("req-1", "owner", PromptKind::Prompt, "hello")?;
//! match session.wait_ack("req-1", timeout) { Ack::Confirmed { .. } => .., .. }
//! for event in events { .. }                    // ends after Entry::Exited
//! ```
//!
//! Ordering: `Entry::UserMessage` for a request enters the stream once its line is written
//! to the harness's stdin, and always before that request's `PromptAck` and `TurnStarted`.
//! Harness output that comes before it in the stream was read before the prompt was written,
//! so it is never evidence that the prompt landed.

#![forbid(unsafe_code)]

mod claude;
mod codex;
mod emit;
mod error;
mod event;
mod fuzz;
mod grok;
mod pi;
mod plan;
mod protocol;
mod session;
mod spec;

pub use error::DriverError;
pub use event::{
    AckKind, Decision, Entry, ErrorClass, Event, Harness, PromptKind, Stop, ToolKind, ToolStatus,
    TouchedPath, Usage, classify,
};
pub use session::{Ack, Session, spawn};
pub use spec::{Permission, SpawnSpec};

#[doc(hidden)]
pub use fuzz::fuzz_feed;
