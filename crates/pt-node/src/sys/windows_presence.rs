//! Windows presence from a service: WTSEnumerateSessions to find the physical console session,
//! then WTSINFOEXW for its LastInputTime and lock flag. Remote (RDP) sessions are not the console
//! and never count.

use super::win32::{self, Job};
use super::win32_proc::{self, Spawn, Token};
use crate::probe::{Config, Observation, Session};
use std::fs::File;
use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

const WTS_ACTIVE: i32 = 0;
const WTS_CONNECTED: i32 = 1;
const FILETIME_PER_SEC: i64 = 10_000_000;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_MAX: u64 = 4096;

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
        if info.user.is_empty() {
            return Observation {
                session: Session::Absent,
                idle_secs: None,
                locked: None,
                source: "wts",
                detail: format!("console session {console} has no signed-in user"),
                alt_idle_secs: None,
            };
        }
        let in_console = win32::own_session_id() == Some(console);
        let mut source = "wts";
        let mut note = String::new();
        let idle = if info.last_input > info.current {
            return fail("LastInputTime is in the future".into());
        } else if info.last_input > 0 {
            Some(((info.current - info.last_input) / FILETIME_PER_SEC) as u64)
        } else if in_console {
            // GetLastInputInfo sees only the caller's own session, here the console's.
            source = "wts+getlastinputinfo";
            win32::own_session_idle_ms().map(|ms| u64::from(ms) / 1000)
        } else {
            // WTS leaves LastInputTime 0 even for LocalSystem, so ask inside the session.
            source = "wts+idle-probe";
            match probe_console(console) {
                Ok(ms) => Some(ms / 1000),
                Err(e) => {
                    note = format!("; idle-probe: {e}");
                    None
                }
            }
        };
        if idle.is_none() && locked != Some(true) {
            return Observation {
                source,
                ..fail(format!(
                    "console session {console} reports no idle time (state {}, flags {}){note}",
                    info.state, info.flags
                ))
            };
        }
        let alt = (in_console && source == "wts")
            .then(win32::own_session_idle_ms)
            .flatten()
            .map(|ms| u64::from(ms) / 1000);
        Observation {
            session: Session::Present,
            idle_secs: idle,
            locked,
            source,
            detail: format!(
                "console session {console} state {} user '{}' flags {}{note}",
                info.state, info.user, info.flags
            ),
            alt_idle_secs: alt,
        }
    }
}

/// The `idle-probe` answer for the calling process's own session.
pub fn idle_probe() -> Result<String, String> {
    let session = win32::own_session_id().ok_or("ProcessIdToSessionId failed")?;
    let ms = win32::own_session_idle_ms().ok_or("GetLastInputInfo failed")?;
    Ok(pt_json::obj! { "session" => session, "idle_ms" => ms }.to_string())
}

/// Runs this binary's `idle-probe` as the console session's user on its interactive desktop
/// (WTSQueryUserToken needs LocalSystem), in a kill-on-close job, and returns its idle ms.
fn probe_console(session: u32) -> Result<u64, String> {
    let token = Token::session_user(session).map_err(|e| format!("WTSQueryUserToken: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("finding this binary: {e}"))?;
    let mut cmd = String::new();
    win32_proc::quote_arg(&exe.to_string_lossy(), &mut cmd);
    win32_proc::quote_arg("idle-probe", &mut cmd);
    let nul_in = File::open("NUL").map_err(|e| format!("opening NUL: {e}"))?;
    let nul_out = File::options()
        .write(true)
        .open("NUL")
        .map_err(|e| format!("opening NUL: {e}"))?;
    let (mut rx, tx) = std::io::pipe().map_err(|e| format!("pipe: {e}"))?;
    let job = Job::new().map_err(|e| format!("job object: {e}"))?;
    let mut proc = win32_proc::spawn(&Spawn {
        token: Some(&token),
        command_line: cmd,
        desktop: Some("winsta0\\default"),
        stdin: &nul_in,
        stdout: &tx,
        stderr: &nul_out,
    })
    .map_err(|e| format!("CreateProcessAsUserW: {e}"))?;
    drop(tx);
    if let Err(e) = job.assign(&proc).and_then(|()| proc.resume()) {
        let _ = proc.terminate(1);
        return Err(format!("starting the probe: {e}"));
    }
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let r = (&mut rx).take(PROBE_MAX).read_to_end(&mut buf);
        let _ = done.send(r.map(|_| buf));
    });
    let out = match wait.recv_timeout(PROBE_TIMEOUT) {
        Ok(r) => r.map_err(|e| format!("reading its output: {e}"))?,
        Err(_) => {
            let _ = job.terminate(1);
            return Err(format!("no answer within {} s", PROBE_TIMEOUT.as_secs()));
        }
    };
    let code = proc.wait_ms(1000).ok().flatten();
    let text = String::from_utf8_lossy(&out);
    let v = pt_json::parse(text.trim())
        .map_err(|e| format!("answer {:?} (exit {code:?}): {e}", text.trim()))?;
    let got = v.get("session").and_then(|s| s.as_u64());
    if got != Some(u64::from(session)) {
        return Err(format!("answered for session {got:?}, not {session}"));
    }
    v.get("idle_ms")
        .and_then(|m| m.as_u64())
        .ok_or_else(|| format!("answer {:?} has no idle_ms", text.trim()))
}
