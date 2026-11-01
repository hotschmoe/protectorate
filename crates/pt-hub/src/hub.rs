//! What every thread shares: the published view, the core's input channel, the transcripts and
//! the event bus.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pt_core::{EventKind, Gen, Millis, Party, Reply, SleeveId, State};
use pt_drivers::Entry;
use pt_json::obj;

use crate::bus::Bus;
use crate::home::{Config, Home, Tokens, now_ms};
use crate::log::{debug, warning};
use crate::transcript::{Transcripts, lock};
use crate::wire;

pub const ENVOY: &str = "envoy";

/// One input to the core thread. Commands carry the channel their `Intent::Reply` goes to.
pub struct Input {
    pub kind: EventKind,
    pub reply: Option<Sender<Reply>>,
}

/// The read-only snapshot published after each journaled batch.
pub struct View {
    pub state: State,
    pub batch: u64,
    /// When each sleeve's activity last changed kind.
    pub since: BTreeMap<SleeveId, Millis>,
}

/// The test-only knob (`PT_TEST_FAULT=usage_limit:<sleeve name>`): the named sleeve's first
/// settled turn is reported as hitting its usage limit, which trips G1 and forces a resleeve.
pub struct Fault {
    pub sleeve: String,
    pub fired: AtomicBool,
}

impl Fault {
    pub fn from_env() -> Option<Fault> {
        let v = std::env::var("PT_TEST_FAULT").ok()?;
        let sleeve = v.strip_prefix("usage_limit:")?.to_string();
        Some(Fault {
            sleeve,
            fired: AtomicBool::new(false),
        })
    }

    pub fn fire(&self, name: &str) -> bool {
        name == self.sleeve && !self.fired.swap(true, Ordering::SeqCst)
    }
}

pub struct Hub {
    pub home: Home,
    pub config: Config,
    pub tokens: Tokens,
    pub uid: u32,
    pub started_ms: u64,
    pub owner_home: PathBuf,
    pub fault: Option<Fault>,
    pub bus: Bus,
    pub transcripts: Transcripts,
    input: Sender<Input>,
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
        uid: u32,
        owner_home: PathBuf,
        input: Sender<Input>,
        view: View,
    ) -> Hub {
        let transcripts = Transcripts::new(home.clone());
        Hub {
            home,
            config,
            tokens,
            uid,
            started_ms: now_ms(),
            owner_home,
            fault: Fault::from_env(),
            bus: Bus::default(),
            transcripts,
            input,
            view: Mutex::new(Arc::new(view)),
        }
    }

    pub fn view(&self) -> Arc<View> {
        lock(&self.view).clone()
    }

    pub(crate) fn publish(&self, v: Arc<View>) {
        *lock(&self.view) = v;
    }

    /// Feeds an event to the core without waiting for its effect.
    pub fn submit(&self, kind: EventKind) {
        if self.input.send(Input { kind, reply: None }).is_err() {
            debug!("core is gone; dropped an event");
        }
    }

    /// Feeds a command to the core and waits for its reply, which is released only after the
    /// command is journaled.
    pub fn command(&self, kind: EventKind, timeout: Duration) -> Result<Reply, CommandError> {
        let (tx, rx) = mpsc::channel();
        self.input
            .send(Input {
                kind,
                reply: Some(tx),
            })
            .map_err(|_| CommandError::Down)?;
        rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => CommandError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => CommandError::Down,
        })
    }

    /// The scope unit name of incarnation `g` of a sleeve on this hub.
    pub fn scope_name(&self, id: SleeveId, g: Gen) -> String {
        pt_sandbox::scope_name(&self.scope_sleeve(id), g.0)
    }

    pub fn scope_sleeve(&self, id: SleeveId) -> String {
        format!("{}-{}", self.tokens.hub_tag, id.0)
    }

    /// The live (not removed) sleeve named `envoy` with no parent.
    pub fn envoy_id(state: &State) -> Option<SleeveId> {
        state
            .sleeves
            .values()
            .find(|s| s.name == ENVOY && s.parent.is_none() && s.life != pt_core::Life::Removed)
            .map(|s| s.id)
    }

    /// Stores a transcript entry and streams it.
    pub fn record(&self, id: SleeveId, g: u64, time_ms: u64, entry: Entry) {
        let store = match self.transcripts.get(id) {
            Ok(s) => s,
            Err(e) => return warning!("sleeve {}: {e}", id.0),
        };
        let stored = lock(&store).append(g, time_ms, entry);
        match stored {
            Ok(ev) => {
                self.bus.push(obj! {
                    "kind" => "transcript",
                    "sleeve_id" => id.0.to_string(),
                    "entry" => wire::chat_entry(&ev, false),
                });
            }
            Err(e) => warning!("sleeve {}: {e}", id.0),
        }
    }

    /// Streams a live text delta under the seq its final entry will take; nothing is stored.
    pub fn delta(&self, id: SleeveId, g: u64, time_ms: u64, entry: Entry) {
        let seq = match self.transcripts.get(id) {
            Ok(s) => lock(&s).peek_seq(),
            Err(_) => return,
        };
        let ev = pt_drivers::Event {
            seq,
            g,
            time_ms,
            entry,
        };
        self.bus.push(obj! {
            "kind" => "transcript",
            "sleeve_id" => id.0.to_string(),
            "entry" => wire::chat_entry(&ev, true),
        });
    }

    /// The `from` label a harness sees on a user message: owner, envoy, agent:<name>, or the
    /// sending sleeve's name.
    pub fn from_label(state: &State, to: SleeveId, request_id: &str) -> String {
        let party = state
            .sleeve(to)
            .and_then(|s| s.messages.get(request_id))
            .map(|m| m.from);
        match party {
            Some(Party::Owner) | None => "owner".into(),
            Some(Party::Envoy) => "envoy".into(),
            Some(Party::Agent) => match request_id.strip_prefix("ga:") {
                Some(rest) => format!("agent:{}", rest.split(':').next().unwrap_or("")),
                None => "agent".into(),
            },
            Some(Party::Sleeve(x)) => state
                .sleeve(x)
                .map_or_else(|| format!("sleeve:{}", x.0), |s| s.name.clone()),
        }
    }
}
