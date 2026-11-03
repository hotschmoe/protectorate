//! What every thread shares: the published view, the core's input channel, the transcripts, the
//! event bus and devices.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pt_api::HubStreamMessage;
use pt_core::{EventKind, Gen, Intent, ProbeResult, Reply, SleeveId};
use pt_drivers::Entry;

use crate::bus::Bus;
use crate::devices::Devices;
use crate::fleet::FleetSlot;
use crate::home::{Config, Home, Tokens, now_ms};
use crate::jobs::JobStore;
use crate::lock::lock;
use crate::log::{debug, warning};
use crate::transcript::{Line, Note, Transcripts};
use crate::view::View;
use crate::wire;

/// One input to the core thread. Commands carry the channel their `Intent::Reply` goes to.
#[allow(clippy::large_enum_variant)]
pub enum Input {
    Event {
        kind: EventKind,
        reply: Option<Sender<Reply>>,
    },
    Snapshot {
        dir: PathBuf,
        reply: Sender<Result<JournalSnapshot, String>>,
    },
}

/// A journal copied at a batch boundary, and the sleeve id the core would hand out next.
pub struct JournalSnapshot {
    pub at_ms: u64,
    pub next_sleeve: u64,
}

/// The test-only knob reports a usage limit at the first turn's settle.
pub struct Fault {
    pub sleeve: String,
    pub fired: AtomicBool,
}

impl Fault {
    pub fn from_env() -> Option<Fault> {
        let v = std::env::var("PT_TEST_FAULT").ok()?;
        let sleeve = v.strip_prefix("usage_limit:")?;
        Some(Fault {
            sleeve: sleeve.to_string(),
            fired: AtomicBool::new(false),
        })
    }

    pub fn fire(&self, name: &str) -> bool {
        name == self.sleeve && !self.fired.swap(true, Ordering::SeqCst)
    }
}

/// Why `serve` returns.
pub enum Stop {
    Signal(i32),
    /// Exit for the service manager to start `releases/current`.
    Restart(String),
}

pub struct Hub {
    pub home: Home,
    pub config: Config,
    pub tokens: Tokens,
    pub devices: Devices,
    /// Held by one Envoy spawn from the D9 gate until the core answers it, so two concurrent
    /// spawns cannot both pass the spawn limit on the same count.
    pub d9_spawn: Mutex<()>,
    pub uid: u32,
    pub started_ms: u64,
    pub info: Mutex<pt_api::HubInfo>,
    pub clients: crate::clients::Clients,
    pub owner_home: PathBuf,
    pub fault: Option<Fault>,
    pub bus: Bus,
    pub transcripts: Transcripts,
    pub fleet: FleetSlot,
    pub jobs: JobStore,
    pub quotas: crate::quota::Quotas,
    pub history: crate::history::History,
    pub doctor: crate::doctor::Doctor,
    pub harness_updates: crate::harness_update::Updater,
    /// Hub and node updates, one at a time.
    pub update_lock: Mutex<()>,
    /// Where `serve` waits; an update sends `Stop::Restart` here.
    pub stop: Mutex<Option<Sender<Stop>>>,
    input: Sender<Input>,
    /// Inputs sent to the core and not yet taken into a batch.
    pub(crate) input_depth: AtomicU64,
    view: Mutex<Arc<View>>,
}

#[derive(Debug)]
pub enum CommandError {
    /// The core did not answer in time; the command may still apply.
    Timeout,
    /// The core thread is gone.
    Down,
}

impl Hub {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        home: Home,
        config: Config,
        tokens: Tokens,
        devices: Devices,
        uid: u32,
        owner_home: PathBuf,
        input: Sender<Input>,
        view: View,
        bus: Bus,
        clients: crate::clients::Clients,
    ) -> Hub {
        let transcripts = Transcripts::new(home.clone());
        let jobs = JobStore::new(home.jobs());
        let history = crate::history::History::open(
            &if config.history_dir.is_empty() {
                home.history()
            } else {
                PathBuf::from(&config.history_dir)
            },
            config.history_budget_bytes,
        );
        let started_ms = now_ms();
        Hub {
            home,
            config,
            tokens,
            devices,
            d9_spawn: Mutex::new(()),
            uid,
            started_ms,
            info: Mutex::new(pt_api::HubInfo {
                name: "protectorate".into(),
                version: pt_version::VERSION.into(),
                started_at_ms: pt_api::EpochMs(started_ms),
                inventory: None,
                sample: None,
                history_capped: Vec::new(),
                restore_error: None,
                clients: clients.names(),
            }),
            clients,
            owner_home,
            fault: Fault::from_env(),
            bus,
            transcripts,
            fleet: FleetSlot::default(),
            jobs,
            quotas: Default::default(),
            history,
            doctor: Default::default(),
            harness_updates: Default::default(),
            update_lock: Mutex::new(()),
            stop: Mutex::new(None),
            input,
            input_depth: AtomicU64::new(0),
            view: Mutex::new(Arc::new(view)),
        }
    }

    pub fn view(&self) -> Arc<View> {
        lock(&self.view).clone()
    }

    pub(crate) fn publish(&self, v: Arc<View>) {
        *lock(&self.view) = v;
    }

    /// Counted before the send, so the core's count of what it took never passes it.
    fn send(&self, input: Input) -> Result<(), ()> {
        self.input_depth.fetch_add(1, Ordering::Relaxed);
        self.input.send(input).map_err(|_| {
            self.input_depth.fetch_sub(1, Ordering::Relaxed);
        })
    }

    /// Feeds an event to the core without waiting for its effect.
    pub fn submit(&self, kind: EventKind) {
        if self.send(Input::Event { kind, reply: None }).is_err() {
            debug!("core is gone; dropped an event");
        }
    }

    /// Answers an intent no executor will run: an effect is refused, a probe cannot tell.
    pub fn refuse(&self, intent: Intent, why: &str) {
        match intent {
            Intent::Execute {
                effect, attempt, ..
            } => {
                self.submit(EventKind::Dispatched { effect, attempt });
                self.submit(EventKind::Nacked {
                    effect,
                    attempt,
                    reason: why.to_string(),
                });
            }
            Intent::Probe {
                effect, attempt, ..
            } => self.submit(EventKind::Probed {
                effect,
                attempt,
                result: ProbeResult::Unknowable,
            }),
            other => debug!("dropped {other:?}: {why}"),
        }
    }

    /// Feeds a command to the core and waits for its reply, which is released only after the
    /// command is journaled.
    pub fn command(&self, kind: EventKind, timeout: Duration) -> Result<Reply, CommandError> {
        let (tx, rx) = mpsc::channel();
        self.send(Input::Event {
            kind,
            reply: Some(tx),
        })
        .map_err(|_| CommandError::Down)?;
        rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => CommandError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => CommandError::Down,
        })
    }

    pub fn snapshot_journal(&self, dir: PathBuf) -> Result<JournalSnapshot, String> {
        let (tx, rx) = mpsc::channel();
        self.send(Input::Snapshot { dir, reply: tx })
            .map_err(|_| "core is down".to_string())?;
        rx.recv()
            .map_err(|_| "core stopped while snapshotting the journal".to_string())?
    }

    /// The scope unit name of incarnation `g` of a sleeve on this hub.
    pub fn scope_name(&self, id: SleeveId, g: Gen) -> String {
        pt_sandbox::scope_name(&self.scope_sleeve(id), g.0)
    }

    pub fn scope_sleeve(&self, id: SleeveId) -> String {
        format!("{}-{}", self.tokens.hub_tag, id.0)
    }

    /// Stores a transcript entry and streams it. The stream message is pushed while the store
    /// is held, so stream order is transcript order.
    pub fn record(&self, id: SleeveId, g: u64, time_ms: u64, entry: Entry, note: Note) {
        let store = match self.transcripts.get(id) {
            Ok(s) => s,
            Err(e) => return warning!("sleeve {}: {e}", id.0),
        };
        let mut store = lock(&store);
        match store.append(g, time_ms, entry, note) {
            Ok(line) => {
                let entry = wire::chat_entry(id, &line, false);
                self.bus.push(|seq| HubStreamMessage::Entry {
                    seq,
                    sleeve_id: id.0.to_string(),
                    entry,
                });
            }
            Err(e) => warning!("sleeve {}: {e}", id.0),
        }
    }

    /// Streams a live text delta under the seq its final entry will take; nothing is stored.
    pub fn delta(&self, id: SleeveId, g: u64, time_ms: u64, entry: Entry) {
        let Ok(store) = self.transcripts.get(id) else {
            return;
        };
        let store = lock(&store);
        let line = Line {
            seq: store.peek_seq(),
            g,
            time_ms,
            turn: store.peek_turn(&entry),
            harness: None,
            level: None,
            entry,
        };
        let entry = wire::chat_entry(id, &line, true);
        self.bus.push_delta(|seq| HubStreamMessage::Entry {
            seq,
            sleeve_id: id.0.to_string(),
            entry,
        });
    }
}
