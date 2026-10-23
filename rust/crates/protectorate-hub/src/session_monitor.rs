//! Session monitor: publishes tmux session changes on the hub event bus.
use super::Hub;
use chrono::{DateTime, Utc};
use protectorate_session::{
    classify, Activity, Error as SessionError, Session as TmuxSession, State as SessionState,
};
use protectorate_sleeve_registry::{Mode, State};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
pub const EVENT_CREATED: &str = "session.created";
pub const EVENT_EXITED: &str = "session.exited";
pub const EVENT_REMOVED: &str = "session.removed";

type Verdict = Option<(Activity, &'static str)>;

/// A screen verdict is announced once two scans in a row agree on it.
#[derive(Default)]
struct Seen {
    shown: Option<Verdict>,
    pending: Option<Verdict>,
}

#[derive(Default)]
struct Known {
    sessions: BTreeMap<String, TmuxSession>,
    seeded: bool,
}

/// Watches the tmux sessions of one hub. The first scan only records what is
/// already there; later scans publish created, exited and removed transitions,
/// including sessions started outside the hub, and kill exited sessions once
/// their dead retention has elapsed (retention zero disables the sweep).
#[derive(Clone)]
pub struct Monitor {
    dead_retention: Duration,
    known: Arc<Mutex<Known>>,
    scan_lock: Arc<Mutex<()>>,
    activity: Arc<Mutex<BTreeMap<String, Seen>>>,
}

impl Monitor {
    pub fn new(dead_retention: Duration) -> Self {
        Self {
            dead_retention,
            known: Arc::new(Mutex::new(Known::default())),
            scan_lock: Arc::new(Mutex::new(())),
            activity: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// What the screen of a terminal sleeve showed at the last announcement.
    pub fn activity(&self, sleeve: &str) -> Verdict {
        let seen = self
            .activity
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        seen.get(sleeve).and_then(|seen| seen.shown).flatten()
    }

    fn scan_activity(&self, hub: &Hub) {
        let rows = hub.sleeves.list();
        let mut seen = self
            .activity
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        seen.retain(|name, _| {
            rows.iter().any(|row| {
                row.name == *name && row.state == State::Running && row.mode == Mode::Terminal
            })
        });
        for row in rows
            .iter()
            .filter(|row| row.state == State::Running && row.mode == Mode::Terminal)
        {
            let harness = super::sleeves::harness_name(row.harness);
            let verdict = hub
                .sessions
                .screen(&row.session)
                .ok()
                .and_then(|pane| classify(harness, &pane));
            let entry = seen.entry(row.name.clone()).or_default();
            if entry.shown == Some(verdict) || entry.pending != Some(verdict) {
                entry.pending = Some(verdict);
                continue;
            }
            entry.shown = Some(verdict);
            hub.emit(
                "sleeve.activity",
                &row.name,
                json!({
                    "harness": harness,
                    "activity": verdict.map_or("unknown", |(activity, _)| activity.as_str()),
                    "rule": verdict.map_or("", |(_, rule)| rule),
                }),
            );
        }
    }

    /// One tmux pass. Listing blocks on tmux, so callers run it with
    /// `tokio::task::spawn_blocking`. A failed pass keeps the previous state.
    pub fn scan(&self, hub: &Hub) {
        let _scan = self
            .scan_lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let sessions = match hub.sessions.list() {
            Ok(sessions) => sessions,
            Err(error) => {
                eprintln!("session monitor: list failed: {error}");
                return;
            }
        };
        let mut events: Vec<(&'static str, String, Value)> = Vec::new();
        let mut current: BTreeMap<String, TmuxSession> = BTreeMap::new();
        let known = self
            .known
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for session in sessions {
            if self.expired(&session) {
                match hub.sessions.kill(&session.name) {
                    Ok(()) | Err(SessionError::NotFound) => continue,
                    Err(_) => {}
                }
            }
            let previous = known.sessions.get(&session.name);
            if known.seeded && previous.is_none() {
                events.push((EVENT_CREATED, session.name.clone(), payload(&session)));
            }
            if session.state == SessionState::Exited
                && (previous.is_some() || known.seeded)
                && previous.map(|s| s.state) != Some(SessionState::Exited)
            {
                events.push((EVENT_EXITED, session.name.clone(), payload(&session)));
            }
            current.insert(session.name.clone(), session);
        }
        for (name, session) in &known.sessions {
            if !current.contains_key(name) {
                events.push((EVENT_REMOVED, name.clone(), payload(session)));
            }
        }
        drop(known);
        *self
            .known
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Known {
            sessions: current,
            seeded: true,
        };
        for (kind, subject, payload) in events {
            hub.emit(kind, &subject, payload);
        }
        self.scan_activity(hub);
    }

    /// Scans immediately and then every second until the hub is dropped.
    /// Without a Tokio runtime the monitor stays idle.
    pub fn start(&self, hub: Arc<Hub>) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let hub = Arc::downgrade(&hub);
        let monitor = self.clone();
        runtime.spawn(async move {
            let mut interval = tokio::time::interval_at(tokio::time::Instant::now(), POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let Some(hub) = hub.upgrade() else { break };
                let monitor = monitor.clone();
                match tokio::task::spawn_blocking(move || monitor.scan(&hub)).await {
                    Ok(()) => {}
                    Err(error) => eprintln!("session monitor: scan failed: {error}"),
                }
            }
        });
    }

    fn expired(&self, session: &TmuxSession) -> bool {
        self.dead_retention > Duration::ZERO
            && session.state == SessionState::Exited
            && session.exited.is_some_and(|at| {
                SystemTime::now()
                    .duration_since(at)
                    .is_ok_and(|age| age > self.dead_retention)
            })
    }
}

fn payload(session: &TmuxSession) -> Value {
    let mut out = json!({
        "name": session.name,
        "cwd": session.cwd.to_string_lossy(),
        "state": match session.state {
            SessionState::Running => "running",
            SessionState::Exited => "exited",
        },
    });
    if session.state == SessionState::Exited {
        out["exit_code"] = json!(session.exit_code);
        out["exit_signal"] = json!(session.exit_signal);
        if let Some(at) = session.exited {
            out["exited_at"] = json!(super::timestamp(DateTime::<Utc>::from(at)));
        }
    }
    out
}
