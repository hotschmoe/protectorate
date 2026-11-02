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
//! session.send_user("req-1", "owner", "hello")?;
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
mod event;
mod fuzz;
mod grok;
mod pi;
mod session;

use std::fmt;
use std::path::PathBuf;
use std::process::Command;

pub use event::{
    AckKind, Decision, Entry, ErrorClass, Event, Harness, Stop, ToolKind, ToolStatus, TouchedPath,
    Usage, classify,
};
pub use session::{Ack, Session, spawn};

#[doc(hidden)]
pub use fuzz::fuzz_feed;

use emit::Out;
use pt_json::Value;

pt_json::string_enum! {
    /// Unsupervised runs with the harness's own approvals off (the sandbox is the boundary);
    /// supervised routes each approval to the hub as `approval_request`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Permission {
        Unsupervised = "unsupervised",
        Supervised = "supervised",
    }
}

/// How to launch one incarnation.
pub struct SpawnSpec {
    pub harness: Harness,
    /// The harness executable, as the prefix (or this process) resolves it.
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// The incarnation's generation, stamped on every event.
    pub g: u64,
    pub model: Option<String>,
    /// Pi only: the provider from Pi's own catalog or models.json.
    pub provider: Option<String>,
    pub effort: Option<String>,
    /// A native session id to resume.
    pub resume: Option<String>,
    pub permission: Permission,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    /// Further official flags, appended where the harness's argv takes options.
    pub extra_args: Vec<String>,
    /// The sandbox or scope wrapper; the harness program and its args are appended to it.
    pub prefix: Option<Command>,
}

impl SpawnSpec {
    pub fn new(harness: Harness, program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        SpawnSpec {
            harness,
            program: program.into(),
            cwd: cwd.into(),
            g: 0,
            model: None,
            provider: None,
            effort: None,
            resume: None,
            permission: Permission::Unsupervised,
            env: Vec::new(),
            env_remove: Vec::new(),
            extra_args: Vec::new(),
            prefix: None,
        }
    }
}

#[derive(Debug)]
pub enum DriverError {
    /// The harness has not reported its session yet.
    NotReady,
    /// A turn is running and this harness cannot take another message now.
    Busy,
    /// The process has exited or is shutting down; nothing was written.
    Ended,
    /// The turn a steer was meant for is not the running one; nothing was written.
    TurnEnded,
    Unsupported(&'static str),
    UnknownApproval(String),
    Invalid(&'static str),
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NotReady => f.write_str("harness session is not ready"),
            DriverError::Busy => f.write_str("harness is in a turn and cannot take a message"),
            DriverError::Ended => f.write_str("harness session has ended"),
            DriverError::TurnEnded => f.write_str("the turn to steer has ended"),
            DriverError::Unsupported(what) => write!(f, "unsupported: {what}"),
            DriverError::UnknownApproval(id) => write!(f, "approval {id:?} is not pending"),
            DriverError::Invalid(what) => write!(f, "invalid: {what}"),
            DriverError::Spawn { program, source } => {
                write!(f, "spawn {}: {source}", program.display())
            }
        }
    }
}

impl std::error::Error for DriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DriverError::Spawn { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One harness protocol as a pure state machine: messages and calls in, entries and stdin
/// lines out (through [`Out`]). No I/O, so the fuzz targets drive it directly.
trait Protocol: Send {
    fn start(&mut self, out: &mut Out);
    fn on_message(&mut self, msg: Value, out: &mut Out);
    /// A prompt, or with `steer` the native id of the turn it is meant for.
    fn send_user(
        &mut self,
        request_id: &str,
        text: &str,
        steer: Option<&str>,
        out: &mut Out,
    ) -> Result<(), DriverError>;
    fn respond_approval(
        &mut self,
        id: &str,
        decision: Decision,
        out: &mut Out,
    ) -> Result<(), DriverError>;
    fn interrupt(&mut self, out: &mut Out) -> Result<(), DriverError>;
}

type UuidSource<'a> = &'a mut dyn FnMut() -> std::io::Result<String>;

/// The effort levels a harness's own CLI accepts. Codex's depend on the model, so its driver
/// checks them against Codex's catalog instead.
fn effort_levels(harness: Harness) -> Option<&'static [&'static str]> {
    match harness {
        Harness::Claude => Some(&["low", "medium", "high", "xhigh", "max"]),
        Harness::Grok => Some(&["low", "medium", "high", "xhigh"]),
        Harness::Pi => Some(&["off", "minimal", "low", "medium", "high", "xhigh", "max"]),
        Harness::Codex => None,
    }
}

/// Drops an effort the harness would refuse and says so, so the owner is told rather than the
/// launch failing or the CLI quietly choosing for itself.
fn vet_effort(spec: &mut SpawnSpec) -> Option<String> {
    let effort = spec.effort.as_deref()?;
    let levels = effort_levels(spec.harness)?;
    if levels.contains(&effort) {
        return None;
    }
    let note = format!(
        "{}: effort {effort:?} is not one of {}; using the default",
        spec.harness,
        levels.join(", ")
    );
    spec.effort = None;
    Some(note)
}

/// The harness argv after the program, and the protocol state for `spec`; `uuid` yields
/// fresh random UUIDs.
fn plan(
    spec: &SpawnSpec,
    uuid: UuidSource<'_>,
) -> Result<(Vec<String>, Box<dyn Protocol>), DriverError> {
    let cwd = spec
        .cwd
        .to_str()
        .ok_or(DriverError::Invalid("cwd must be UTF-8"))?
        .to_owned();
    Ok(match spec.harness {
        Harness::Claude => {
            let id = match &spec.resume {
                Some(id) => id.clone(),
                None => random(uuid)?,
            };
            let p = claude::Claude::new(id, random(uuid)?, spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Codex => {
            let p = codex::Codex::new(spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Grok => {
            let p = grok::Grok::new(spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Pi => {
            if spec.permission == Permission::Supervised {
                return Err(DriverError::Unsupported("Pi has no approval gate"));
            }
            let session_id = match &spec.resume {
                Some(id) => id.clone(),
                None => random(uuid)?,
            };
            let p = pi::Pi::new(spec, session_id);
            (p.argv(spec), Box::new(p))
        }
    })
}

fn random(uuid: UuidSource<'_>) -> Result<String, DriverError> {
    uuid().map_err(|source| DriverError::Spawn {
        program: "/dev/urandom".into(),
        source,
    })
}
