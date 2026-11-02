//! Human presence at the machine (SPEC 3.5). Only local seat input and lock state count; remote
//! logins never do.

use crate::clock::unix_now;
pub use crate::probe::Config;
use crate::probe::Session;
use crate::sys;
use pt_frame::msg::{Presence, PresenceState as State};
use std::path::PathBuf;

/// Test override, read when the Detector is made: the variable names a file holding PRESENT,
/// AWAY or UNKNOWN, re-read at every detection, which then replaces the platform probe (source
/// `test-override`). A missing or unreadable file reads as UNKNOWN. Only the e2e tests set it.
pub const OVERRIDE_ENV: &str = "PROTECTORATE_NODE_TEST_PRESENCE";

/// One JSON line, `{"session":N,"idle_ms":M}`, for the caller's own session: what the
/// Windows service reads from `idle-probe`, which it starts inside the console session.
pub fn idle_probe() -> Result<String, String> {
    #[cfg(windows)]
    {
        sys::idle_probe()
    }
    #[cfg(not(windows))]
    {
        Err("idle-probe is Windows only".into())
    }
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

pub struct Detector {
    cfg: Config,
    platform: sys::Platform,
    override_file: Option<PathBuf>,
}

impl Detector {
    pub fn new(cfg: Config) -> Detector {
        Detector {
            cfg,
            platform: sys::Platform::new(),
            override_file: std::env::var_os(OVERRIDE_ENV).map(PathBuf::from),
        }
    }

    /// Long-running callers keep the Detector so input devices stay open and watched.
    pub fn detect(&mut self) -> Presence {
        if let Some(path) = &self.override_file {
            return self.forced(path);
        }
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
            source: o.source.to_string(),
            detail: o.detail,
            alt_idle_secs: o.alt_idle_secs,
            away_after_secs: cfg.away_after_secs,
            sampled_at: unix_now(),
        }
    }

    fn forced(&self, path: &std::path::Path) -> Presence {
        let text = std::fs::read_to_string(path);
        let state = match text.as_deref().map(str::trim) {
            Ok("PRESENT") => State::Present,
            Ok("AWAY") => State::Away,
            _ => State::Unknown,
        };
        Presence {
            state,
            idle_secs: None,
            locked: None,
            source: "test-override".to_string(),
            detail: format!("{OVERRIDE_ENV}={}", path.display()),
            alt_idle_secs: None,
            away_after_secs: self.cfg.away_after_secs,
            sampled_at: unix_now(),
        }
    }
}
