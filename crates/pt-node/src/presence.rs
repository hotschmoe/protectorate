//! Human presence at the machine (SPEC 3.5). Only local seat input and lock state count; remote
//! logins never do.

use crate::json::Obj;
use crate::sys;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Present,
    Away,
    Unknown,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Present => "PRESENT",
            State::Away => "AWAY",
            State::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Config {
    /// Idle seconds at or above which a person counts as away.
    pub away_after_secs: u64,
    /// A locked screen counts as away once idle this long (a person who just locked is still there).
    pub locked_grace_secs: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            away_after_secs: 600,
            locked_grace_secs: 60,
        }
    }
}

pub struct Presence {
    pub state: State,
    pub idle_secs: Option<u64>,
    pub locked: Option<bool>,
    pub source: &'static str,
    pub detail: String,
    /// A second, independent idle reading when one exists (Windows GetLastInputInfo from the console
    /// session), for comparison only.
    pub alt_idle_secs: Option<u64>,
    pub away_after_secs: u64,
    pub sampled_at: u64,
}

pub enum Gate {
    Allow,
    Refuse(String),
}

/// UNKNOWN counts as present: a node whose detector fails takes no jobs.
pub fn job_gate(p: &Presence) -> Gate {
    match p.state {
        State::Away => Gate::Allow,
        State::Present => Gate::Refuse(format!("a person is present (source {})", p.source)),
        State::Unknown => Gate::Refuse(format!(
            "presence is unknown, treated as present (source {}: {})",
            p.source, p.detail
        )),
    }
}

pub(crate) enum Session {
    Failed,
    Absent,
    Present,
}

/// What a platform probe saw, before the threshold policy is applied.
pub(crate) struct Observation {
    pub session: Session,
    /// Seconds since the last local input; a lower bound when the probe only watched for a while.
    pub idle_secs: Option<u64>,
    pub locked: Option<bool>,
    pub source: &'static str,
    pub detail: String,
    pub alt_idle_secs: Option<u64>,
}

pub struct Detector {
    cfg: Config,
    platform: sys::Platform,
}

impl Detector {
    pub fn new(cfg: Config) -> Detector {
        Detector {
            cfg,
            platform: sys::Platform::new(),
        }
    }

    /// Long-running callers keep the Detector so input devices stay open and watched.
    pub fn detect(&mut self) -> Presence {
        let o = self.platform.observe(&self.cfg);
        let cfg = &self.cfg;
        let state = match o.session {
            Session::Failed => State::Unknown,
            Session::Absent => State::Away,
            Session::Present => match (o.idle_secs, o.locked) {
                (Some(i), _) if i >= cfg.away_after_secs => State::Away,
                (Some(i), Some(true)) if i >= cfg.locked_grace_secs => State::Away,
                (None, Some(true)) => State::Away,
                _ => State::Present,
            },
        };
        Presence {
            state,
            idle_secs: o.idle_secs,
            locked: o.locked,
            source: o.source,
            detail: o.detail,
            alt_idle_secs: o.alt_idle_secs,
            away_after_secs: cfg.away_after_secs,
            sampled_at: crate::unix_now(),
        }
    }
}

impl Presence {
    pub fn to_json(&self) -> String {
        Obj::new()
            .str("state", self.state.as_str())
            .opt_uint("idle_secs", self.idle_secs)
            .opt_bool("locked", self.locked)
            .str("source", self.source)
            .str("detail", &self.detail)
            .opt_uint("alt_idle_secs", self.alt_idle_secs)
            .uint("away_after_secs", self.away_after_secs)
            .uint("sampled_at", self.sampled_at)
            .finish()
    }
}
