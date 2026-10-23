//! Node-side desktop policy and action dispatch. Windows owns the OS driver.
#[cfg(any(windows, test))]
mod typing;
#[cfg(windows)]
mod windows;

use chrono::{DateTime, SecondsFormat, Utc};
use protectorate_desk_policy::{evaluate, parse_chord, Inputs, Policy, Ready, Reason, ScreenState};
use serde_json::{json, Value};
use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_IDLE: Duration = Duration::from_secs(24 * 60 * 60);
const TYPE_CHUNK: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Invalid,
    Unavailable,
    Unsupported,
    Internal,
}
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub reason: Option<Reason>,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
impl Error {
    fn invalid(s: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Invalid,
            message: s.into(),
            reason: None,
        }
    }
    fn unavailable(s: impl Into<String>, reason: Option<Reason>) -> Self {
        Self {
            kind: ErrorKind::Unavailable,
            message: s.into(),
            reason,
        }
    }
    fn internal(s: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: s.into(),
            reason: None,
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub trait Desktop: Send + Sync + 'static {
    fn ready(&self) -> Ready;
    fn screen(&self) -> ScreenState;
    /// None means physical-input monitoring is unavailable; policy inputs fail closed.
    fn last_physical_input(&self) -> Option<DateTime<Utc>>;
    fn screenshot(&self, max_width: i32) -> Result<Value>;
    fn windows(&self) -> Result<Value>;
    fn foreground(&self) -> Result<Value>;
    fn move_pointer(&self, x: i32, y: i32) -> Result<()>;
    fn click(&self, x: i32, y: i32, button: &str, double: bool) -> Result<()>;
    fn type_text(&self, text: &str) -> Result<()>;
    fn key(&self, chord: protectorate_desk_policy::Chord) -> Result<()>;
    fn run(&self, program: &str, args: &[String]) -> Result<u32>;
    fn lock(&self) -> Result<()>;
}

struct Guard {
    raw: Option<Value>,
    policy: Option<Policy>,
    was_in: bool,
    last_command: Option<DateTime<Utc>>,
    lease_end: Option<DateTime<Utc>>,
}
pub struct Controller {
    desktop: Arc<dyn Desktop>,
    path: PathBuf,
    guard: Mutex<Guard>,
}
impl Controller {
    #[cfg(windows)]
    pub fn open(home: impl Into<PathBuf>) -> io::Result<Self> {
        Self::with_desktop(home.into(), Arc::new(windows::WindowsDesktop::new()))
    }

    pub fn with_desktop(home: PathBuf, desktop: Arc<dyn Desktop>) -> io::Result<Self> {
        let path = home.join("desk-policy.json");
        let raw = match fs::read(&path) {
            Ok(bytes) => Some(serde_json::from_slice::<Value>(&bytes).map_err(io::Error::other)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let raw =
            raw.filter(|value| value.get("window") != Some(&json!("")) && value != &json!({}));
        let (raw, policy) = parse_policy(raw.as_ref()).map_err(io::Error::other)?;
        let was_in = policy.as_ref().is_some_and(|p| p.in_window(Utc::now()));
        Ok(Self {
            desktop,
            path,
            guard: Mutex::new(Guard {
                raw,
                policy,
                was_in,
                last_command: None,
                lease_end: None,
            }),
        })
    }

    /// Runs lease and window-end locking even when no RPC is in flight.
    pub fn start_background(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let Some(controller) = weak.upgrade() else {
                break;
            };
            if let Ok(mut guard) = controller.guard.lock() {
                controller.step_locked(&mut guard, Utc::now());
            };
        });
    }

    /// Reports the current local decision without adopting or clearing policy.
    pub fn inspect_state(&self) -> Result<Value> {
        let now = Utc::now();
        let guard = self
            .guard
            .lock()
            .map_err(|_| Error::internal("desk guard poisoned"))?;
        let mut state = self.state_locked(&guard, now);
        state["sessionReady"] = json!(self.desktop.ready() == Ready::Interactive);
        state["screenState"] = json!(match self.desktop.screen() {
            ScreenState::Locked => "locked",
            ScreenState::Unlocked => "unlocked",
        });
        Ok(state)
    }

    /// Adopts the hub's policy, then reports the current node-side decision.
    pub fn get_state(&self, request: &Value) -> Result<Value> {
        let requested = request.get("policy");
        let (raw, policy) = parse_policy(requested)?;
        let now = Utc::now();
        let mut guard = self
            .guard
            .lock()
            .map_err(|_| Error::internal("desk guard poisoned"))?;
        if guard.raw != raw {
            let bytes = serde_json::to_vec(raw.as_ref().unwrap_or(&json!({})))
                .map_err(|e| Error::internal(e.to_string()))?;
            protectorate_fsutil::write_file(&self.path, &bytes, 0o600)
                .map_err(|e| Error::internal(format!("persist desk policy: {e}")))?;
            guard.was_in = policy.as_ref().is_some_and(|p| p.in_window(now));
            guard.raw = raw;
            guard.policy = policy;
            guard.lease_end = None;
        }
        if request.get("leaseEnded").and_then(Value::as_bool) == Some(true)
            && guard.policy.as_ref().is_some_and(|p| p.lock_after)
        {
            guard.lease_end = Some(now);
        }
        self.step_locked(&mut guard, now);
        Ok(self.state_locked(&guard, now))
    }

    fn state_locked(&self, guard: &Guard, now: DateTime<Utc>) -> Value {
        let last = self.desktop.last_physical_input();
        let decision = evaluate(
            guard.policy.as_ref(),
            Inputs {
                now,
                last_input: last.unwrap_or(now),
                ready: self.desktop.ready(),
                screen: self.desktop.screen(),
            },
        );
        let decision = if guard.policy.is_some() && last.is_none() && decision.available {
            protectorate_desk_policy::Verdict {
                available: false,
                reason: Reason::UserActive,
                detail: "physical input monitor unavailable".into(),
            }
        } else {
            decision
        };
        let mut out = json!({
            "available": decision.available,
            "reason": reason_name(decision.reason),
            "detail": decision.detail,
            "evaluatedAt": now.to_rfc3339_opts(SecondsFormat::Millis, true),
        });
        if let Some(last) = last {
            out["lastInput"] = json!(last.to_rfc3339_opts(SecondsFormat::Millis, true));
        }
        if let Some(policy) = &guard.raw {
            out["policy"] = policy.clone();
        }
        out
    }

    fn admit(&self) -> Result<()> {
        let now = Utc::now();
        let mut guard = self
            .guard
            .lock()
            .map_err(|_| Error::internal("desk guard poisoned"))?;
        self.step_locked(&mut guard, now);
        let state = self.state_locked(&guard, now);
        if state["available"] != true {
            let reason = parse_reason(state["reason"].as_str().unwrap_or_default());
            return Err(Error::unavailable(
                format!(
                    "desktop unavailable: {}",
                    state["detail"].as_str().unwrap_or_default()
                ),
                reason,
            ));
        }
        guard.last_command = Some(now);
        Ok(())
    }

    fn step_locked(&self, guard: &mut Guard, now: DateTime<Utc>) {
        let Some(policy) = guard.policy.as_ref() else {
            return;
        };
        if !policy.lock_after {
            return;
        }
        let in_window = policy.in_window(now);
        let mut lock = guard.was_in && !in_window;
        guard.was_in = in_window;
        if guard
            .lease_end
            .is_some_and(|end| now.signed_duration_since(end).num_seconds() >= 60)
        {
            let end = guard.lease_end.take().expect("checked");
            lock |= in_window && guard.last_command.is_none_or(|last| last <= end);
        }
        if lock
            && self.desktop.ready() == Ready::Interactive
            && self.desktop.screen() != ScreenState::Locked
            && self.desktop.last_physical_input().is_some_and(|last| {
                now.signed_duration_since(last)
                    .to_std()
                    .is_ok_and(|idle| idle >= policy.idle)
            })
        {
            let _ = self.desktop.lock();
        }
    }

    pub fn action(&self, request: &Value) -> Result<Value> {
        let obj = request
            .as_object()
            .ok_or_else(|| Error::invalid("desktop action is not an object"))?;
        if obj.len() != 1 {
            return Err(Error::invalid("exactly one desktop action required"));
        }
        let (kind, args) = obj.iter().next().expect("one action");
        let field = |name: &str| {
            args.get(name)
                .map_or(Some(0), Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| Error::invalid(format!("{name} is out of range")))
        };
        match kind.as_str() {
            "screenshot" => {
                self.require_ready()?;
                let max_width = args.get("maxWidth").and_then(Value::as_i64).unwrap_or(0);
                let max_width = i32::try_from(max_width)
                    .map_err(|_| Error::invalid("maxWidth out of range"))?;
                if max_width < 0 {
                    return Err(Error::invalid("maxWidth must be nonnegative"));
                }
                Ok(json!({"screenshot": self.desktop.screenshot(max_width)?}))
            }
            "listWindows" => {
                self.require_ready()?;
                Ok(json!({"listWindows": self.desktop.windows()?}))
            }
            "foreground" => {
                self.require_ready()?;
                Ok(json!({"foreground": self.desktop.foreground()?}))
            }
            "move" => {
                let (x, y) = (field("x")?, field("y")?);
                self.admit()?;
                self.desktop.move_pointer(x, y)?;
                Ok(json!({"input": {}}))
            }
            "click" => {
                let (x, y) = (field("x")?, field("y")?);
                let button = args.get("button").and_then(Value::as_str).unwrap_or("");
                if !matches!(button, "" | "left" | "right" | "middle") {
                    return Err(Error::invalid("invalid mouse button"));
                }
                let double = args.get("double").and_then(Value::as_bool).unwrap_or(false);
                self.admit()?;
                self.desktop.click(x, y, button, double)?;
                Ok(json!({"input": {}}))
            }
            "type" => {
                let text = args.get("text").and_then(Value::as_str).unwrap_or("");
                self.admit()?;
                let chars: Vec<char> = text.chars().collect();
                for (index, chunk) in chars.chunks(TYPE_CHUNK).enumerate() {
                    self.desktop.type_text(&chunk.iter().collect::<String>())?;
                    if (index + 1) * TYPE_CHUNK < chars.len() {
                        self.admit()?;
                    }
                }
                Ok(json!({"input": {}}))
            }
            "key" => {
                let chord = args.get("chord").and_then(Value::as_str).unwrap_or("");
                let parsed = parse_chord(chord).map_err(|e| Error::invalid(e.to_string()))?;
                self.admit()?;
                self.desktop.key(parsed)?;
                Ok(json!({"input": {}}))
            }
            "run" => {
                let program = args
                    .get("program")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| Error::invalid("program is empty"))?;
                let arguments = args
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.admit()?;
                Ok(json!({"run": {"pid": self.desktop.run(program, &arguments)?}}))
            }
            _ => Err(Error::invalid("unknown desktop action")),
        }
    }

    fn require_ready(&self) -> Result<()> {
        match self.desktop.ready() {
            Ready::Interactive => Ok(()),
            Ready::NotInteractive => Err(Error::unavailable(
                "not an interactive session",
                Some(Reason::NotInteractive),
            )),
            Ready::Unsupported => Err(Error {
                kind: ErrorKind::Unsupported,
                message: "desktop control unsupported".into(),
                reason: Some(Reason::Unsupported),
            }),
        }
    }
}

fn parse_policy(raw: Option<&Value>) -> Result<(Option<Value>, Option<Policy>)> {
    let Some(raw) = raw.filter(|v| !v.is_null()) else {
        return Ok((None, None));
    };
    let window = raw
        .get("window")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::invalid("policy window is missing"))?;
    let idle = raw
        .get("idle")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid("idle duration missing"))?;
    let idle = parse_duration(idle)?;
    if idle > MAX_IDLE {
        return Err(Error::invalid("idle exceeds 24 hours"));
    }
    let timezone = raw.get("timezone").and_then(Value::as_str).unwrap_or("");
    let lock_after = raw
        .get("lockAfter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let policy = Policy::new(window, idle, timezone, lock_after)
        .map_err(|e| Error::invalid(e.to_string()))?;
    Ok((Some(raw.clone()), Some(policy)))
}

fn parse_duration(raw: &str) -> Result<Duration> {
    let seconds = raw
        .strip_suffix('s')
        .ok_or_else(|| Error::invalid("idle duration must end in s"))?;
    let seconds: f64 = seconds
        .parse()
        .map_err(|_| Error::invalid("invalid idle duration"))?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > MAX_IDLE.as_secs_f64() {
        return Err(Error::invalid("idle duration out of range"));
    }
    Ok(Duration::from_secs_f64(seconds))
}
pub fn reason_name(reason: Reason) -> &'static str {
    match reason {
        Reason::Available => "DESK_REASON_AVAILABLE",
        Reason::OutsideWindow => "DESK_REASON_OUTSIDE_WINDOW",
        Reason::UserActive => "DESK_REASON_USER_ACTIVE",
        Reason::NotInteractive => "DESK_REASON_NOT_INTERACTIVE",
        Reason::Locked => "DESK_REASON_LOCKED",
        Reason::Unsupported => "DESK_REASON_UNSUPPORTED",
    }
}
fn parse_reason(raw: &str) -> Option<Reason> {
    match raw {
        "DESK_REASON_OUTSIDE_WINDOW" => Some(Reason::OutsideWindow),
        "DESK_REASON_USER_ACTIVE" => Some(Reason::UserActive),
        "DESK_REASON_NOT_INTERACTIVE" => Some(Reason::NotInteractive),
        "DESK_REASON_LOCKED" => Some(Reason::Locked),
        "DESK_REASON_UNSUPPORTED" => Some(Reason::Unsupported),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        input: Mutex<Option<DateTime<Utc>>>,
        screen: Mutex<ScreenState>,
        moves: AtomicUsize,
        typed: AtomicUsize,
        locks: AtomicUsize,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                input: Mutex::new(Some(Utc::now() - chrono::Duration::minutes(30))),
                screen: Mutex::new(ScreenState::Unlocked),
                moves: AtomicUsize::new(0),
                typed: AtomicUsize::new(0),
                locks: AtomicUsize::new(0),
            }
        }
    }
    impl Desktop for Fake {
        fn ready(&self) -> Ready {
            Ready::Interactive
        }
        fn screen(&self) -> ScreenState {
            *self.screen.lock().unwrap()
        }
        fn last_physical_input(&self) -> Option<DateTime<Utc>> {
            *self.input.lock().unwrap()
        }
        fn screenshot(&self, _: i32) -> Result<Value> {
            Ok(json!({"png":"AQ==","width":1,"height":1}))
        }
        fn windows(&self) -> Result<Value> {
            Ok(json!({"windows":[]}))
        }
        fn foreground(&self) -> Result<Value> {
            Ok(json!({"title":"test","pid":1,"blocks":false}))
        }
        fn move_pointer(&self, _: i32, _: i32) -> Result<()> {
            self.moves.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn click(&self, _: i32, _: i32, _: &str, _: bool) -> Result<()> {
            Ok(())
        }
        fn type_text(&self, _: &str) -> Result<()> {
            self.typed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn key(&self, _: protectorate_desk_policy::Chord) -> Result<()> {
            Ok(())
        }
        fn run(&self, _: &str, _: &[String]) -> Result<u32> {
            Ok(42)
        }
        fn lock(&self) -> Result<()> {
            self.locks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    fn current_window() -> String {
        let now = Utc::now();
        let start = (now - chrono::Duration::hours(1)).time();
        let end = (now + chrono::Duration::hours(1)).time();
        format!(
            "{:02}:{:02}-{:02}:{:02}",
            start.hour(),
            start.minute(),
            end.hour(),
            end.minute()
        )
    }
    #[test]
    fn policy_persists_and_each_input_rechecks_physical_idle() {
        let home = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::new());
        let desk = Controller::with_desktop(home.path().to_owned(), fake.clone()).unwrap();
        assert_eq!(desk.get_state(&json!({})).unwrap()["available"], true);
        desk.action(&json!({"move":{}})).unwrap();
        assert_eq!(fake.moves.load(Ordering::SeqCst), 1);
        let policy =
            json!({"window":current_window(),"idle":"900s","timezone":"UTC","lockAfter":true});
        let state = desk.get_state(&json!({"policy":policy})).unwrap();
        assert_eq!(state["available"], true);
        desk.action(&json!({"move":{"x":2,"y":3}})).unwrap();
        assert_eq!(fake.moves.load(Ordering::SeqCst), 2);
        *fake.input.lock().unwrap() = Some(Utc::now());
        let denied = desk.action(&json!({"move":{"x":2,"y":3}})).unwrap_err();
        assert_eq!(denied.kind, ErrorKind::Unavailable);
        assert_eq!(denied.reason, Some(Reason::UserActive));
        assert_eq!(fake.moves.load(Ordering::SeqCst), 2);
        let reopened = Controller::with_desktop(home.path().to_owned(), fake.clone()).unwrap();
        assert!(reopened.guard.lock().unwrap().policy.is_some());
        assert!(fs::read_to_string(home.path().join("desk-policy.json"))
            .unwrap()
            .contains("900s"));
        assert_eq!(desk.inspect_state().unwrap()["policy"], policy);
    }
    #[test]
    fn invalid_actions_precede_session_check_and_type_chunks_recheck() {
        let home = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::new());
        let desk = Controller::with_desktop(home.path().to_owned(), fake.clone()).unwrap();
        assert_eq!(
            desk.action(&json!({"key":{"chord":"ctrl+bogus"}}))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        assert_eq!(
            desk.action(&json!({"run":{"program":""}}))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        desk.action(&json!({"type":{"text":"a".repeat(129)}}))
            .unwrap();
        assert_eq!(fake.typed.load(Ordering::SeqCst), 3);
        assert_eq!(
            desk.get_state(&json!({"policy":{"window":"bad","idle":"10s"}}))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
    }
    #[test]
    fn lease_lock_waits_for_quiet_and_monitor_failure_denies_policy_input() {
        let home = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::new());
        let desk = Controller::with_desktop(home.path().to_owned(), fake.clone()).unwrap();
        let policy =
            json!({"window":current_window(),"idle":"1s","timezone":"UTC","lockAfter":true});
        desk.get_state(&json!({"policy":policy,"leaseEnded":true}))
            .unwrap();
        {
            let mut guard = desk.guard.lock().unwrap();
            guard.lease_end = Some(Utc::now() - chrono::Duration::seconds(61));
            drop(guard);
            let snapshot = desk.inspect_state().unwrap();
            assert_eq!(snapshot["sessionReady"], true);
            assert_eq!(snapshot["screenState"], "unlocked");
            assert_eq!(fake.locks.load(Ordering::SeqCst), 0);
            let mut guard = desk.guard.lock().unwrap();
            desk.step_locked(&mut guard, Utc::now());
        }
        assert_eq!(fake.locks.load(Ordering::SeqCst), 1);
        *fake.input.lock().unwrap() = None;
        let state = desk.get_state(&json!({"policy":policy})).unwrap();
        assert_eq!(state["available"], false);
        assert_eq!(state["reason"], "DESK_REASON_USER_ACTIVE");
    }
    #[test]
    fn locked_desktop_refuses_input_without_a_policy() {
        let home = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::new());
        *fake.screen.lock().unwrap() = ScreenState::Locked;
        let desk = Controller::with_desktop(home.path().to_owned(), fake.clone()).unwrap();
        let state = desk.inspect_state().unwrap();
        assert_eq!(state["available"], false);
        assert_eq!(state["reason"], "DESK_REASON_LOCKED");
        for action in [
            json!({"move":{"x":1,"y":1}}),
            json!({"run":{"program":"demo"}}),
        ] {
            let error = desk.action(&action).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Unavailable);
            assert_eq!(error.reason, Some(Reason::Locked));
        }
        assert_eq!(fake.moves.load(Ordering::SeqCst), 0);
    }
}
