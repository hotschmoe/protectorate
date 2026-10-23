//! Session monitor: publishes tmux session changes on the hub event bus.
use super::Hub;
use chrono::{DateTime, Utc};
use protectorate_session::{Error as SessionError, Session as TmuxSession, State as SessionState};
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
}

impl Monitor {
    pub fn new(dead_retention: Duration) -> Self {
        Self {
            dead_retention,
            known: Arc::new(Mutex::new(Known::default())),
            scan_lock: Arc::new(Mutex::new(())),
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use protectorate_session::{CreateOptions, Manager};
    use std::{process::Command, time::Instant};

    struct PrivateTmux(String);

    impl Drop for PrivateTmux {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["-L", &self.0, "kill-server"])
                .output();
        }
    }

    fn socket() -> PrivateTmux {
        PrivateTmux(format!(
            "p-rust-session-monitor-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ))
    }

    fn wait_exited(manager: &Manager, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if manager.get(name).unwrap().state == SessionState::Exited {
                return;
            }
            assert!(Instant::now() < deadline, "session did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn lifecycle_events_include_external_sessions_and_zero_retention_keeps_exit() {
        let temp = tempfile::tempdir().unwrap();
        let socket = socket();
        let manager = Arc::new(Manager::new(&socket.0));
        let hub = Hub::open(temp.path(), "owner-secret", "test")
            .unwrap()
            .with_sessions(Arc::clone(&manager))
            .with_dead_retention(Duration::ZERO);
        let mut events = hub.events.sender.subscribe();
        hub.session_monitor.scan(&hub);
        assert!(events.try_recv().is_err());

        manager
            .create(&CreateOptions {
                name: "external".into(),
                command: vec!["/bin/sh".into(), "-c".into(), "sleep 0.3; exit 7".into()],
                ..Default::default()
            })
            .unwrap();
        hub.session_monitor.scan(&hub);
        let created = events.try_recv().unwrap();
        assert_eq!(created["kind"], EVENT_CREATED);
        assert_eq!(created["subject"], "external");
        assert_eq!(created["payload"]["state"], "running");

        wait_exited(&manager, "external");
        hub.session_monitor.scan(&hub);
        let exited = events.try_recv().unwrap();
        assert_eq!(exited["kind"], EVENT_EXITED);
        assert_eq!(exited["payload"]["exit_code"], 7);
        assert!(exited["payload"]["exited_at"].is_string());
        assert_eq!(manager.get("external").unwrap().state, SessionState::Exited);

        manager.kill("external").unwrap();
        hub.session_monitor.scan(&hub);
        let removed = events.try_recv().unwrap();
        assert_eq!(removed["kind"], EVENT_REMOVED);
        assert_eq!(removed["subject"], "external");
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn positive_dead_retention_reaps_exited_session() {
        let temp = tempfile::tempdir().unwrap();
        let socket = socket();
        let manager = Arc::new(Manager::new(&socket.0));
        manager
            .create(&CreateOptions {
                name: "stale".into(),
                command: vec!["/usr/bin/true".into()],
                ..Default::default()
            })
            .unwrap();
        wait_exited(&manager, "stale");
        let hub = Hub::open(temp.path(), "owner-secret", "test")
            .unwrap()
            .with_sessions(Arc::clone(&manager))
            .with_dead_retention(Duration::from_nanos(1));
        hub.session_monitor.scan(&hub);
        assert!(matches!(manager.get("stale"), Err(SessionError::NotFound)));
    }
}
