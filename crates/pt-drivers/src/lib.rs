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

#![forbid(unsafe_code)]

mod claude;
mod codex;
mod emit;
mod event;
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
    fn send_user(
        &mut self,
        request_id: &str,
        text: &str,
        steer: bool,
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

/// The harness argv after the program, and the protocol state for `spec`.
fn plan(spec: &SpawnSpec) -> Result<(Vec<String>, Box<dyn Protocol>), DriverError> {
    let cwd = spec
        .cwd
        .to_str()
        .ok_or(DriverError::Invalid("cwd must be UTF-8"))?
        .to_owned();
    Ok(match spec.harness {
        Harness::Claude => {
            let id = match &spec.resume {
                Some(id) => id.clone(),
                None => emit::new_uuid().map_err(|source| DriverError::Spawn {
                    program: "/dev/urandom".into(),
                    source,
                })?,
            };
            let p = claude::Claude::new(id, spec, cwd);
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
            let new_id = match &spec.resume {
                Some(_) => None,
                None => Some(emit::new_uuid().map_err(|source| DriverError::Spawn {
                    program: "/dev/urandom".into(),
                    source,
                })?),
            };
            let p = pi::Pi::new(spec);
            (p.argv(spec, new_id.as_deref()), Box::new(p))
        }
    })
}

/// Drives a protocol mapper with arbitrary input for the fuzz targets. An optional first
/// line `#` with `resume=<id>` and `supervised` sets the spawn options. Lines starting with
/// `!` are calls (`!send`, `!steer`, `!approve <id>`, `!session <id>`, `!deny <id>`,
/// `!cancel <id>`, `!interrupt`); every other line goes through the same path as the
/// harness's stdout. Every entry must survive a JSON round trip. Not a stable API.
#[doc(hidden)]
pub fn fuzz_feed(harness: Harness, input: &[u8]) -> usize {
    let mut spec = SpawnSpec::new(harness, "harness", "/work");
    let mut lines = input.split(|b| *b == b'\n').peekable();
    if let Some(header) = lines.next_if(|l| l.first() == Some(&b'#')) {
        for word in String::from_utf8_lossy(&header[1..]).split_whitespace() {
            if let Some(id) = word.strip_prefix("resume=") {
                spec.resume = Some(id.to_owned());
            } else if word == "supervised" && harness != Harness::Pi {
                spec.permission = Permission::Supervised;
            }
        }
    }
    let Ok((_, mut proto)) = plan(&spec) else {
        return 0;
    };
    let mut out = Out::new(1);
    proto.start(&mut out);
    let mut n = 0;
    for (i, line) in lines.enumerate() {
        if let Some(call) = line.strip_prefix(b"!") {
            let call = String::from_utf8_lossy(call);
            let (verb, arg) = call.split_once(' ').unwrap_or((&call, ""));
            let req = format!("r{i}");
            let p = proto.as_mut();
            let _ = match verb {
                "send" => session::deliver(p, &mut out, &req, "fuzz", "hi", false),
                "steer" => session::deliver(p, &mut out, &req, "fuzz", "hi", true),
                "approve" => p.respond_approval(arg, Decision::AllowOnce, &mut out),
                "session" => p.respond_approval(arg, Decision::AllowSession, &mut out),
                "deny" => p.respond_approval(arg, Decision::Deny, &mut out),
                "cancel" => p.respond_approval(arg, Decision::Cancel, &mut out),
                "interrupt" => p.interrupt(&mut out),
                _ => Ok(()),
            };
        } else {
            session::handle_line(proto.as_mut(), &mut out, harness, line);
        }
        for event in out.events.drain(..) {
            let text = pt_json::ToJson::to_json(&event).to_string();
            let back: Event = pt_json::from_str(&text).expect("an event parses back");
            assert_eq!(back, event, "an event survives a JSON round trip");
            n += text.len();
        }
        for line in out.writes.drain(..) {
            n += line.to_string().len();
        }
    }
    n
}
