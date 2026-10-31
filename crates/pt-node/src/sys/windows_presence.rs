//! Windows presence from a service: WTSEnumerateSessions to find the physical console session,
//! then WTSINFOEXW for its LastInputTime and lock flag. Remote (RDP) sessions are not the console
//! and never count.

use super::win32;
use crate::presence::{Config, Observation, Session};

const WTS_ACTIVE: i32 = 0;
const WTS_CONNECTED: i32 = 1;
const FILETIME_PER_SEC: i64 = 10_000_000;

pub struct Platform;

impl Platform {
    pub fn new() -> Platform {
        Platform
    }

    pub fn observe(&mut self, _cfg: &Config) -> Observation {
        let fail = |detail: String| Observation {
            session: Session::Failed,
            idle_secs: None,
            locked: None,
            source: "wts",
            detail,
            alt_idle_secs: None,
        };
        let Some(console) = win32::console_session_id() else {
            return Observation {
                session: Session::Absent,
                idle_secs: None,
                locked: None,
                source: "wts",
                detail: "no session attached to the physical console".into(),
                alt_idle_secs: None,
            };
        };
        let sessions = match win32::wts_sessions() {
            Ok(s) => s,
            Err(e) => return fail(format!("WTSEnumerateSessions: {e}")),
        };
        let Some(entry) = sessions.iter().find(|s| s.id == console) else {
            return fail(format!("console session {console} not in the session list"));
        };
        if entry.state != WTS_ACTIVE && entry.state != WTS_CONNECTED {
            return Observation {
                session: Session::Absent,
                idle_secs: None,
                locked: None,
                source: "wts",
                detail: format!(
                    "console session {console} ({}) is in state {}",
                    entry.name, entry.state
                ),
                alt_idle_secs: None,
            };
        }
        let info = match win32::session_info_ex(console) {
            Ok(i) => i,
            Err(e) => {
                return fail(format!(
                    "WTSQuerySessionInformation(session {console}): {e}"
                ));
            }
        };
        let locked = match info.flags {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        };
        let idle = if info.last_input <= 0 {
            // The console session's LastInputTime stays 0 for ordinary callers (quser shows
            // "none"); without a locked screen to go on, presence cannot be told.
            if locked != Some(true) {
                return fail(format!(
                    "console session {console} reports no LastInputTime (state {}, flags {})",
                    info.state, info.flags
                ));
            }
            None
        } else if info.last_input > info.current {
            return fail("LastInputTime is in the future".into());
        } else {
            Some(((info.current - info.last_input) / FILETIME_PER_SEC) as u64)
        };
        // GetLastInputInfo sees only the caller's own session, so it is a cross-check when this
        // process runs inside the console session and meaningless from a service.
        let alt = (win32::own_session_id() == Some(console))
            .then(win32::own_session_idle_ms)
            .flatten()
            .map(|ms| u64::from(ms) / 1000);
        Observation {
            session: Session::Present,
            idle_secs: idle,
            locked,
            source: "wts",
            detail: format!(
                "console session {console} state {} user '{}' flags {}",
                info.state, info.user, info.flags
            ),
            alt_idle_secs: alt,
        }
    }
}
