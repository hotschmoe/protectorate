//! Per-connection session state: the initialize handshake and the set of
//! requests in progress, which `notifications/cancelled` can mark cancelled.
use crate::jsonrpc::{Failure, INVALID_REQUEST, SERVER_ERROR};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Requests a session may have in flight at once.
pub const MAX_PENDING_REQUESTS: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct Session {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    initialized: bool,
    max_pending: usize,
    pending: BTreeMap<String, Arc<AtomicBool>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            initialized: false,
            max_pending: MAX_PENDING_REQUESTS,
            pending: BTreeMap::new(),
        }
    }
}

/// Holds one slot in the session until the request is answered. Dropping it
/// frees the slot, which is what lets a worker thread own its own lifetime.
#[derive(Debug)]
pub struct Guard {
    session: Session,
    key: String,
    pub cancelled: Arc<AtomicBool>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let Ok(mut state) = self.session.state.lock() else {
            return;
        };
        state.pending.remove(&self.key);
    }
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// A session with a smaller in-progress budget, which is how the HTTP
    /// transport keeps one client from holding every worker.
    pub fn with_max_pending(max_pending: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                max_pending: max_pending.max(1),
                ..State::default()
            })),
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.initialized)
            .unwrap_or(false)
    }

    pub fn mark_initialized(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.initialized = true;
        }
    }

    /// Reserves a slot for `id`. The `Err` carries the JSON-RPC reply for a
    /// duplicate id or a full session.
    pub fn begin(&self, id: &Value) -> Result<Guard, Failure> {
        let key = request_key(id);
        let mut state = self
            .state
            .lock()
            .map_err(|_| Failure::new(Some(id.clone()), SERVER_ERROR, "session lock poisoned"))?;
        if state.pending.contains_key(&key) {
            return Err(Failure::new(
                Some(id.clone()),
                INVALID_REQUEST,
                "duplicate request id",
            ));
        }
        if state.pending.len() >= state.max_pending {
            return Err(Failure::new(
                Some(id.clone()),
                SERVER_ERROR,
                "too many requests in progress",
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        state.pending.insert(key.clone(), cancelled.clone());
        Ok(Guard {
            session: self.clone(),
            key,
            cancelled,
        })
    }

    /// Marks the request with this id cancelled. Unknown and finished ids are
    /// ignored, so a late notification is harmless.
    pub fn cancel(&self, id: &Value) {
        let key = request_key(id);
        if let Ok(state) = self.state.lock() {
            if let Some(cancelled) = state.pending.get(&key) {
                cancelled.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(state) = self.state.lock() {
            for cancelled in state.pending.values() {
                cancelled.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// String and numeric ids must not collide, so each kind gets its own prefix.
pub fn request_key(id: &Value) -> String {
    match id {
        Value::String(text) => format!("string:{text}"),
        Value::Null => "null".to_owned(),
        other => format!("number:{other}"),
    }
}
