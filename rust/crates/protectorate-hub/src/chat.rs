//! Headless chat sleeves. Pi is the first live driver; unsupported harnesses
//! remain explicit failures rather than shell sessions pretending to be chat.
use super::{input, respond, string, EventBus, Hub};
use chrono::{SecondsFormat, TimeZone, Utc};
use protectorate_chatlog::{Error as LogError, Event, Log};
use protectorate_claude_driver::Session as ClaudeSession;
use protectorate_codex_driver::Session as CodexSession;
use protectorate_grok_driver::Session as GrokSession;
use protectorate_inventory::Inventory;
use protectorate_pi_driver::Session as PiSession;
use protectorate_sandbox::{
    runtime::LaunchOptions, Access, BindGrant, Limits, Policy, Provider, Request, Workload,
};
use protectorate_sleeve_registry::{
    Harness, Mode, ReadOnlyMount, Record, Registry as SleeveRegistry, State,
};
use protectorate_wire::{self as wire, Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

/// Summarize a handoff in a separate Pi sandbox. The workspace contains no
/// task files and the helper receives no hub identity or token path.
pub(super) fn summarize_handoff(
    hub: &Hub,
    owner: &str,
    prompt: &str,
) -> protectorate_handoff::Summary {
    match run_handoff_summary(hub, owner, prompt) {
        Ok(summary) => summary,
        Err(err) => protectorate_handoff::Summary {
            err,
            ..Default::default()
        },
    }
}

fn run_handoff_summary(
    hub: &Hub,
    owner: &str,
    prompt: &str,
) -> std::result::Result<protectorate_handoff::Summary, String> {
    const DEADLINE: Duration = Duration::from_secs(75);
    let launcher = &hub
        .chat_runtime
        .as_ref()
        .ok_or("chat runtime is not configured")?
        .launcher;
    let executable = launcher
        .pi_executable
        .as_ref()
        .ok_or("Pi executable is not configured")?;
    if !executable.is_file() {
        return Err("Pi executable is missing".into());
    }
    let auth = launcher.state_root.join("auth/pi");
    if !auth.is_dir() {
        return Err("Pi credential directory is missing".into());
    }
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let workspace = scratch.path().join("workspace");
    fs::create_dir(&workspace).map_err(|e| e.to_string())?;
    let mut random = [0u8; 8];
    getrandom::getrandom(&mut random).map_err(|e| e.to_string())?;
    let name = format!(
        "summary-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let root = launcher.state_root.join("sleeves").join(&name);
    let home = root.join("home");
    fs::create_dir_all(home.join(".pi/sessions")).map_err(|e| e.to_string())?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let result = (|| {
        let cache_environment =
            protectorate_sleeve_runner::prepare_private_caches(&home, &launcher.user_home)
                .map_err(|e| e.to_string())?;
        let policy = Policy {
            state_root: launcher.state_root.clone(),
            user_home: launcher.user_home.clone(),
            protected: Vec::new(),
            provider_credentials: BTreeMap::from([(Provider::Pi, auth)]),
            trusted_tool_roots: launcher.tool_roots.clone(),
            immutable_cache_roots: Vec::new(),
            nested_cgroups_available: false,
        };
        let plan = policy
            .plan(Request {
                workload: Workload::ChatSleeve,
                command: vec![
                    executable.to_string_lossy().into_owned(),
                    "--mode".into(),
                    "rpc".into(),
                    "--no-tools".into(),
                    "--no-extensions".into(),
                    "--no-mcp".into(),
                    "--no-skills".into(),
                    "--no-context-files".into(),
                    "--no-session".into(),
                ],
                workspace: Some(workspace.clone()),
                managed_workspace_name: protectorate_sandbox::managed_workspace_name(
                    &launcher.state_root,
                    &workspace,
                ),
                private_home: Some(home.clone()),
                grants: Vec::new(),
                tool_roots: launcher.tool_roots.clone(),
                selected_executable: None,
                credential: Some(Provider::Pi),
                overlays: Vec::new(),
                masks: Vec::new(),
                egress: Some(crate::sleeves::sleeve_egress(hub, true)),
                limits: Limits {
                    cpu_percent: 0,
                    memory_bytes: 0,
                    tasks_max: launcher.tasks_max,
                },
            })
            .map_err(|e| e.to_string())?;
        let mut options = LaunchOptions::new(
            &launcher.helper,
            &launcher.runtime_dir,
            &launcher.user_home,
            &workspace,
        );
        options.environment = launcher.environment.clone();
        let run_id = hub
            .sleeves
            .get(owner)
            .map(|row| row.run_id)
            .unwrap_or_default();
        options.egress_log = Some(crate::sleeves::egress_log(hub, owner, &run_id));
        options.environment.extend(cache_environment);
        options.environment.remove("PROTECTORATE_HUB");
        options.environment.remove("PROTECTORATE_TOKEN_FILE");
        options.environment.insert(
            "HOME".into(),
            launcher.user_home.to_string_lossy().into_owned(),
        );
        options.environment.insert(
            "PI_CODING_AGENT_DIR".into(),
            "/run/protectorate/auth/pi".into(),
        );
        options.environment.insert(
            "PI_CODING_AGENT_SESSION_DIR".into(),
            launcher
                .user_home
                .join(".pi/sessions")
                .to_string_lossy()
                .into_owned(),
        );
        let log = Arc::new(Log::open(root.join("chat.jsonl")).map_err(|e| e.to_string())?);
        let session = PiSession::start(&plan, options, Arc::clone(&log), None)?;
        let outcome = (|| {
            let after = session.send(prompt, "hub", "")?;
            let cancelled = Arc::new(AtomicBool::new(false));
            let (sender, receiver) = mpsc::sync_channel(1);
            let watch_log = Arc::clone(&log);
            let watch_cancelled = Arc::clone(&cancelled);
            let watcher = std::thread::spawn(move || {
                let mut answer = String::new();
                let mut error = String::new();
                let mut settled = false;
                let _ = watch_log.watch(after, &watch_cancelled, |event| {
                    if let Some(text) = event.payload["text"]["delta"].as_str() {
                        if event.payload["text"]["thinking"] != true {
                            answer.push_str(text);
                            if answer.len() > protectorate_handoff::MAX_SUMMARY * 2 {
                                error = "summary exceeds output limit".into();
                                return Err("summary exceeds output limit".into());
                            }
                        }
                    }
                    if let Some(text) = event.payload["error"]["text"].as_str() {
                        error = text.into();
                    }
                    if !event.payload["settled"].is_null() {
                        settled = true;
                        return Err("settled".into());
                    }
                    Ok(())
                });
                let _ = sender.send((answer, error, settled));
            });
            let result = receiver.recv_timeout(DEADLINE);
            cancelled.store(true, Ordering::Relaxed);
            session.stop();
            let _ = watcher.join();
            let (answer, error, settled) = result
                .map_err(|_| format!("no summary answer within {} seconds", DEADLINE.as_secs()))?;
            if !settled {
                return Err(if error.is_empty() {
                    "Pi summary session exited before settling".into()
                } else {
                    error
                });
            }
            let answer = answer.trim();
            if answer.is_empty() {
                return Err(if error.is_empty() {
                    "Pi returned an empty summary".into()
                } else {
                    error
                });
            }
            let status = session.status();
            Ok(protectorate_handoff::Summary {
                text: answer.into(),
                harness: "pi".into(),
                model: status["model"].as_str().unwrap_or_default().into(),
                provider: status["provider"].as_str().unwrap_or_default().into(),
                local: crate::local_model::served_locally(
                    &session.model_base_url(),
                    &hub.local_model_hosts,
                    &hub.inventory,
                ),
                err: String::new(),
            })
        })();
        session.stop();
        let _ = session.wait_exited(Duration::from_secs(2));
        outcome
    })();
    if let Err(error) = fs::remove_dir_all(&root) {
        eprintln!("resleeve {owner}: remove summary helper: {error}");
    }
    result
}

#[derive(Clone, Debug)]
pub struct ChatLauncher {
    pub state_root: PathBuf,
    pub user_home: PathBuf,
    pub runtime_dir: PathBuf,
    pub helper: PathBuf,
    /// An absolute host path to the official Pi executable.
    pub pi_executable: Option<PathBuf>,
    pub claude_executable: Option<PathBuf>,
    pub claude_tool_roots: Vec<PathBuf>,
    pub codex_executable: Option<PathBuf>,
    pub codex_tool_roots: Vec<PathBuf>,
    pub grok_executable: Option<PathBuf>,
    pub grok_tool_roots: Vec<PathBuf>,
    /// Explicit SDK roots exposed read-only to the chat sleeve.
    pub tool_roots: Vec<PathBuf>,
    pub flutter_sdk_root: Option<PathBuf>,
    pub flutter_cache_lower: Option<PathBuf>,
    pub flutter_tools_lower: Option<PathBuf>,
    pub pub_cache_lower: Option<PathBuf>,
    pub go_mod_cache_lower: Option<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub tasks_max: u32,
}

pub struct ChatRuntime {
    launcher: ChatLauncher,
    sessions: Mutex<HashMap<String, Arc<dyn ChatSession>>>,
    logs: Mutex<HashMap<String, Arc<Log>>>,
    requests: Mutex<HashMap<String, RequestKeys>>,
}

const REQUEST_WINDOW_MS: u64 = 60 * 60 * 1000;
const MAX_REQUEST_KEYS: usize = 10_000;

#[derive(Clone, Copy)]
struct RequestReceipt {
    seq: u64,
    expires_ms: u64,
}

#[derive(Default)]
struct RequestKeys {
    by_id: HashMap<String, RequestReceipt>,
    by_expiry: BTreeSet<(u64, u64, String)>,
}

impl RequestKeys {
    fn restore_event(&mut self, event: &Event, now_ms: u64) {
        if let Some(id) = event.payload["user"]["requestId"]
            .as_str()
            .filter(|id| valid_request_id(id))
        {
            let expires_ms = event.time_ms.saturating_add(REQUEST_WINDOW_MS);
            if expires_ms > now_ms {
                self.insert(id.to_owned(), event.seq, expires_ms);
                if self.len() > MAX_REQUEST_KEYS {
                    self.discard_oldest();
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.by_id.len()
    }

    fn get(&self, id: &str) -> Option<u64> {
        self.by_id.get(id).map(|entry| entry.seq)
    }

    fn insert(&mut self, id: String, seq: u64, expires_ms: u64) {
        if let Some(old) = self
            .by_id
            .insert(id.clone(), RequestReceipt { seq, expires_ms })
        {
            self.by_expiry
                .remove(&(old.expires_ms, old.seq, id.clone()));
        }
        self.by_expiry.insert((expires_ms, seq, id));
    }

    fn discard_oldest(&mut self) {
        if let Some((_, _, id)) = self.by_expiry.pop_first() {
            self.by_id.remove(&id);
        }
    }

    fn sweep(&mut self, now_ms: u64) {
        while self
            .by_expiry
            .first()
            .is_some_and(|(expires_ms, _, _)| *expires_ms <= now_ms)
        {
            self.discard_oldest();
        }
    }
}

fn request_now_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

fn valid_request_id(id: &str) -> bool {
    (8..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

impl ChatRuntime {
    pub(crate) fn launcher(&self) -> &ChatLauncher {
        &self.launcher
    }

    pub(crate) fn helper_path(&self) -> &Path {
        &self.launcher.helper
    }
}

pub(super) fn hub_url(hub: &Hub) -> Option<String> {
    hub.chat_runtime
        .as_ref()?
        .launcher
        .environment
        .get("PROTECTORATE_HUB")
        .cloned()
}

trait ChatSession: Send + Sync {
    fn status(&self) -> Value;
    fn model_base_url(&self) -> String;
    fn is_exited(&self) -> bool;
    fn is_idle(&self) -> bool;
    fn stop(&self);
    fn wait_exited(&self, timeout: std::time::Duration) -> bool;
    fn send(&self, text: &str, from: &str, request_id: &str) -> std::result::Result<u64, String>;
    fn interrupt(&self) -> std::result::Result<(), String>;
    fn respond(
        &self,
        _id: &str,
        _allow: bool,
        _note: &str,
        _by: &str,
    ) -> std::result::Result<(), String> {
        Err("precondition: approvals are not supported by this harness".into())
    }
    fn list_models(&self) -> std::result::Result<Value, String>;
    fn set_selection(&self, model: &str, effort: &str) -> std::result::Result<Value, String>;
    fn compact(&self) -> std::result::Result<Value, String>;
    fn append_report(
        &self,
        status: &str,
        summary: &str,
        brief_seq: u64,
    ) -> std::result::Result<Value, String>;
}
macro_rules! impl_session {
    ($driver:ty, $respond:expr, $base_url:expr) => {
        impl ChatSession for $driver {
            fn status(&self) -> Value {
                self.status()
            }
            fn model_base_url(&self) -> String {
                $base_url(self)
            }
            fn is_exited(&self) -> bool {
                self.is_exited()
            }
            fn is_idle(&self) -> bool {
                self.is_idle()
            }
            fn stop(&self) {
                self.stop()
            }
            fn wait_exited(&self, timeout: std::time::Duration) -> bool {
                self.wait_exited(timeout)
            }
            fn send(
                &self,
                text: &str,
                from: &str,
                request_id: &str,
            ) -> std::result::Result<u64, String> {
                self.send(text, from, request_id)
            }
            fn interrupt(&self) -> std::result::Result<(), String> {
                self.interrupt()
            }
            fn respond(
                &self,
                id: &str,
                allow: bool,
                note: &str,
                by: &str,
            ) -> std::result::Result<(), String> {
                $respond(self, id, allow, note, by)
            }
            fn list_models(&self) -> std::result::Result<Value, String> {
                self.list_models()
            }
            fn set_selection(
                &self,
                model: &str,
                effort: &str,
            ) -> std::result::Result<Value, String> {
                self.set_selection(model, effort)
            }
            fn compact(&self) -> std::result::Result<Value, String> {
                self.compact()
            }
            fn append_report(
                &self,
                status: &str,
                summary: &str,
                brief_seq: u64,
            ) -> std::result::Result<Value, String> {
                self.append_report(status, summary, brief_seq)
            }
        }
    };
}
impl_session!(
    PiSession,
    |_s: &PiSession, _id: &str, _allow: bool, _note: &str, _by: &str| Err(
        "precondition: Pi has no approval gate".into()
    ),
    |s: &PiSession| s.model_base_url()
);
impl_session!(
    ClaudeSession,
    |s: &ClaudeSession, id: &str, allow: bool, note: &str, by: &str| s.respond(id, allow, note, by),
    |_s: &ClaudeSession| String::new()
);
impl_session!(
    CodexSession,
    |s: &CodexSession, id: &str, allow: bool, note: &str, by: &str| s.respond(id, allow, note, by),
    |_s: &CodexSession| String::new()
);
impl_session!(
    GrokSession,
    |s: &GrokSession, id: &str, allow: bool, note: &str, by: &str| s.respond(id, allow, note, by),
    |_s: &GrokSession| String::new()
);

fn chat_status(hub: &Hub, session: &dyn ChatSession) -> Value {
    chat_status_with(session, &hub.local_model_hosts, &hub.inventory)
}

fn chat_status_with(session: &dyn ChatSession, hosts: &[String], inventory: &Inventory) -> Value {
    let mut status = session.status();
    if let Some(object) = status.as_object_mut() {
        object.insert(
            "localModel".into(),
            Value::Bool(crate::local_model::served_locally(
                &session.model_base_url(),
                hosts,
                inventory,
            )),
        );
    }
    status
}

fn chat_state_payload(status: &Value) -> Value {
    let number = |key: &str| {
        status[key]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| status[key].as_u64())
            .unwrap_or_default()
    };
    json!({
        "state": status["state"],
        "model": status["model"],
        "provider": status["provider"],
        "local_model": status["localModel"] == true,
        "context_tokens": number("contextTokens"),
        "reasoning_effort": status["reasoningEffort"],
        "context_window": number("contextWindow"),
        "last_seq": number("lastSeq"),
    })
}

fn publish_chat_state(
    events: &EventBus,
    name: &str,
    status: &Value,
    previous: &mut Value,
    last_publish: &mut Instant,
) {
    let payload = chat_state_payload(status);
    if payload == *previous {
        return;
    }
    let important = previous.is_null()
        || [
            "state",
            "model",
            "provider",
            "local_model",
            "reasoning_effort",
        ]
        .iter()
        .any(|key| payload[*key] != previous[*key]);
    if !important && last_publish.elapsed() < Duration::from_secs(1) {
        return;
    }
    *previous = payload.clone();
    *last_publish = Instant::now();
    events.publish("chat.state", name, payload);
}

struct ChatStateWatcher {
    runtime: Weak<ChatRuntime>,
    session: Arc<dyn ChatSession>,
    log: Arc<Log>,
    after: u64,
    events: Arc<EventBus>,
    inventory: Arc<Inventory>,
    hosts: Vec<String>,
    name: String,
    checkpoints: crate::checkpoints::Store,
    sleeves: Arc<SleeveRegistry>,
}

impl ChatStateWatcher {
    fn start(self) {
        std::thread::spawn(move || self.run());
    }

    fn run(self) {
        let Self {
            runtime,
            session,
            log,
            after,
            events,
            inventory,
            hosts,
            name,
            checkpoints,
            sleeves,
        } = self;
        let mut previous = Value::Null;
        let mut last_publish = Instant::now();
        publish_chat_state(
            &events,
            &name,
            &chat_status_with(session.as_ref(), &hosts, &inventory),
            &mut previous,
            &mut last_publish,
        );
        let cancelled = AtomicBool::new(false);
        let _ = log.watch(after, &cancelled, |event| {
            let Some(current) = runtime.upgrade() else {
                return Err("chat runtime stopped".into());
            };
            if !current
                .sessions
                .lock()
                .unwrap()
                .get(&name)
                .is_some_and(|live| Arc::ptr_eq(live, &session))
            {
                return Err("chat session replaced".into());
            }
            let state = event.payload["state"]["state"]
                .as_str()
                .or_else(|| event.payload.get("user").map(|_| "CHAT_STATE_WORKING"))
                .or_else(|| event.payload.get("settled").map(|_| "CHAT_STATE_IDLE"));
            if state.is_none()
                && !["usage", "settings"]
                    .iter()
                    .any(|key| event.payload.get(*key).is_some())
            {
                return Ok(());
            }
            if event.payload.get("settled").is_some() {
                if let Ok(row) = sleeves.get(&name) {
                    crate::checkpoints::after_turn(checkpoints.clone(), row);
                }
            }
            let mut status = chat_status_with(session.as_ref(), &hosts, &inventory);
            if let Some(state) = state {
                status["state"] = json!(state);
            }
            publish_chat_state(&events, &name, &status, &mut previous, &mut last_publish);
            Ok(())
        });
    }
}

impl ChatRuntime {
    pub fn new(launcher: ChatLauncher) -> Self {
        Self {
            launcher,
            sessions: Mutex::new(HashMap::new()),
            logs: Mutex::new(HashMap::new()),
            requests: Mutex::new(HashMap::new()),
        }
    }
    fn log(&self, name: &str) -> Result<Arc<Log>> {
        let mut logs = self.logs.lock().unwrap();
        if let Some(log) = logs.get(name).cloned() {
            return Ok(log);
        }
        protectorate_sleeve_registry::valid_name(name)
            .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))?;
        let path = self
            .launcher
            .state_root
            .join("sleeves")
            .join(name)
            .join("chat.jsonl");
        let log = Arc::new(Log::open(&path).map_err(log_error)?);
        // The log has no time index, so startup scans it once. Keep only the
        // latest live request keys in memory; do not materialize every event.
        let mut keys = RequestKeys::default();
        let reader = BufReader::new(fs::File::open(&path).map_err(|e| log_error(LogError::Io(e)))?);
        let now_ms = request_now_ms();
        for line in reader.lines() {
            let line = line.map_err(|e| log_error(LogError::Io(e)))?;
            let event: Event =
                serde_json::from_str(&line).map_err(|e| log_error(LogError::Json(e)))?;
            keys.restore_event(&event, now_ms);
        }
        self.requests.lock().unwrap().insert(name.into(), keys);
        logs.insert(name.into(), Arc::clone(&log));
        Ok(log)
    }
    pub fn status(&self, hub: &Hub, name: &str) -> Option<Value> {
        self.sessions
            .lock()
            .unwrap()
            .get(name)
            .map(|session| chat_status(hub, session.as_ref()))
    }
    pub fn exited(&self, name: &str) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(name)
            .is_some_and(|s| s.is_exited())
    }
    pub fn stop(&self, name: &str) -> Result<()> {
        let session = self.sessions.lock().unwrap().get(name).cloned();
        if let Some(session) = session {
            session.stop();
            if !session.wait_exited(std::time::Duration::from_secs(5)) {
                return Err(RpcError::new(
                    Code::Unavailable,
                    "chat process did not exit after cancellation",
                ));
            }
            self.sessions.lock().unwrap().remove(name);
            self.logs.lock().unwrap().remove(name);
            self.requests.lock().unwrap().remove(name);
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        let sessions = std::mem::take(&mut *self.sessions.lock().unwrap());
        let logs = std::mem::take(&mut *self.logs.lock().unwrap());
        for session in sessions.values() {
            session.stop();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        for session in sessions.values() {
            let _ = session.wait_exited(deadline.saturating_duration_since(Instant::now()));
        }
        for log in logs.values() {
            let _ = log.close();
        }
    }
}

impl Drop for ChatRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let send_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Send", move |request| {
        let hub = Arc::clone(&send_hub);
        async move {
            let value = input(&request)?;
            let name = string(&value, "sleeve");
            let text = string(&value, "text");
            if text.trim().is_empty() {
                return Err(RpcError::new(Code::InvalidArgument, "text is required"));
            }
            let request_id = string(&value, "requestId");
            if !request_id.is_empty() && !valid_request_id(&request_id) {
                return Err(RpcError::new(
                    Code::InvalidArgument,
                    "request_id must be 8 to 128 characters of letters, digits, '.', '_' and '-'",
                ));
            }
            let actor = request.auth.actor.clone();
            let seq =
                tokio::task::spawn_blocking(move || send(&hub, &name, &text, &request_id, &actor))
                    .await
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, json!({"seq":seq.to_string()}))
        }
    })?;
    let interrupt_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Interrupt", move |request| {
        let hub = Arc::clone(&interrupt_hub);
        async move {
            let name = string(&input(&request)?, "sleeve");
            tokio::task::spawn_blocking(move || interrupt(&hub, &name))
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, json!({}))
        }
    })?;
    let models_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/ListModels", move |request| {
        let hub = Arc::clone(&models_hub);
        async move {
            let value = input(&request)?;
            let name = string(&value, "sleeve");
            let harness = string(&value, "harness");
            if !name.is_empty() {
                own_sleeve(&request.auth.actor, &name)?;
            }
            let result = tokio::task::spawn_blocking(move || list_models(&hub, &name, &harness))
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, result)
        }
    })?;
    let selection_hub = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.ChatService/SetSelection",
        move |request| {
            let hub = Arc::clone(&selection_hub);
            async move {
                let value = input(&request)?;
                let name = string(&value, "sleeve");
                own_sleeve(&request.auth.actor, &name)?;
                let model = string(&value["selection"], "model");
                let effort = string(&value["selection"], "effort");
                let chat = tokio::task::spawn_blocking(move || {
                    set_selection(&hub, &name, &model, &effort)
                })
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                respond(&request, json!({"chat":chat}))
            }
        },
    )?;
    let compact_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Compact", move |request| {
        let hub = Arc::clone(&compact_hub);
        async move {
            let name = string(&input(&request)?, "sleeve");
            own_sleeve(&request.auth.actor, &name)?;
            let compaction = tokio::task::spawn_blocking(move || compact(&hub, &name))
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, json!({"compaction":compaction}))
        }
    })?;
    let respond_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Respond", move |request| {
        let hub = Arc::clone(&respond_hub);
        async move {
            let value = input(&request)?;
            let name = string(&value, "sleeve");
            own_sleeve(&request.auth.actor, &name)?;
            let approval = string(&value, "approvalId");
            if approval.is_empty() {
                return Err(RpcError::new(
                    Code::InvalidArgument,
                    "approval_id is required",
                ));
            }
            let row = hub.sleeves.get(&name).map_err(registry_error)?;
            if row.harness == Harness::Pi {
                return Err(RpcError::new(
                    Code::Unimplemented,
                    "Pi has no approval gate",
                ));
            }
            let allow = value["allow"] == true;
            let note = string(&value, "note");
            let actor = request.auth.actor.clone();
            tokio::task::spawn_blocking(move || {
                let session = live_session(&hub, &name)?;
                session
                    .respond(&approval, allow, &note, &actor)
                    .map_err(driver_error)?;
                hub.sleeves
                    .set_chat(&name, chat_status(&hub, session.as_ref()))
                    .map_err(registry_error)?;
                Ok::<(), RpcError>(())
            })
            .await
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, json!({}))
        }
    })?;
    let report_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Report", move |request| {
        let hub = Arc::clone(&report_hub);
        async move {
            let value = input(&request)?;
            let actor = request.auth.actor.clone();
            let result = tokio::task::spawn_blocking(move || report(&hub, &actor, &value))
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
            respond(&request, json!({"result":result}))
        }
    })?;
    let wait_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.ChatService/Wait", move |request| {
        let hub = Arc::clone(&wait_hub);
        async move {
            let value = input(&request)?;
            let name = string(&value, "sleeve");
            own_sleeve(&request.auth.actor, &name)?;
            let result = wait(&hub, &name, &value).await?;
            respond(&request, result)
        }
    })?;
    server.register_streaming(
        "/protectorate.v1.ChatService/Watch",
        move |request, sender| {
            let hub = Arc::clone(&hub);
            async move {
                let value = input(&request)?;
                let name = string(&value, "sleeve");
                own_sleeve(&request.auth.actor, &name)?;
                let after = number(&value, "afterSeq")?;
                let row = hub.sleeves.get(&name).map_err(registry_error)?;
                if row.helper {
                    return Err(RpcError::new(Code::NotFound, "sleeve not found"));
                }
                if row.mode != Mode::Chat && row.result.is_none() {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        format!("sleeve {name} runs in terminal mode, which has no chat"),
                    ));
                }
                let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
                let descriptor = request.method.output();
                if row.mode == Mode::Terminal || row.state == State::Exited || runtime.exited(&name)
                {
                    let path = runtime
                        .launcher
                        .state_root
                        .join("sleeves")
                        .join(&name)
                        .join("chat.jsonl");
                    for event in protectorate_chatlog::read_all(path).map_err(log_error)? {
                        if event.seq <= after {
                            continue;
                        }
                        let bytes = serde_json::to_vec(&event_json(&event))
                            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                        sender
                            .send(wire::decode_json(descriptor.clone(), &bytes)?)
                            .await?;
                    }
                    return Ok(Default::default());
                }
                let log = runtime.log(&name)?;
                let cancelled = Arc::new(AtomicBool::new(false));
                let cancel_flag = Arc::clone(&cancelled);
                let cancel_sender = sender.clone();
                let cancel_task = tokio::spawn(async move {
                    cancel_sender.cancelled().await;
                    cancel_flag.store(true, Ordering::Release);
                });
                let result = tokio::task::spawn_blocking(move || {
                    let handle = tokio::runtime::Handle::current();
                    log.watch(after, &cancelled, |event| {
                        let value = event_json(event);
                        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
                        let message = wire::decode_json(descriptor.clone(), &bytes)
                            .map_err(|e| e.to_string())?;
                        handle
                            .block_on(sender.send(message))
                            .map_err(|e| e.to_string())
                    })
                })
                .await
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                cancel_task.abort();
                match result {
                    Ok(()) | Err(LogError::Closed | LogError::Callback(_)) => {
                        Ok(Default::default())
                    }
                    Err(error) => Err(log_error(error)),
                }
            }
        },
    )?;
    Ok(())
}

pub(super) fn spawn_pi(
    hub: &Hub,
    value: &Value,
    parent: &crate::sleeves::SpawnParent,
) -> Result<Record> {
    spawn_chat(hub, value, Harness::Pi, false, false, parent)
}

pub(super) fn spawn_claude(
    hub: &Hub,
    value: &Value,
    parent: &crate::sleeves::SpawnParent,
) -> Result<Record> {
    spawn_chat(hub, value, Harness::Claude, false, false, parent)
}

pub(super) fn spawn_codex(
    hub: &Hub,
    value: &Value,
    parent: &crate::sleeves::SpawnParent,
) -> Result<Record> {
    spawn_chat(hub, value, Harness::Codex, false, false, parent)
}

pub(super) fn spawn_grok(
    hub: &Hub,
    value: &Value,
    parent: &crate::sleeves::SpawnParent,
) -> Result<Record> {
    spawn_chat(hub, value, Harness::Grok, false, false, parent)
}

pub(super) fn spawn_envoy(hub: &Hub, value: &Value, harness: Harness) -> Result<Record> {
    spawn_chat(
        hub,
        value,
        harness,
        true,
        false,
        &crate::sleeves::SpawnParent::owner(),
    )
}

pub(super) fn spawn_resleeve(hub: &Hub, value: &Value, harness: Harness) -> Result<Record> {
    spawn_chat(
        hub,
        value,
        harness,
        string(value, "name") == "envoy",
        true,
        &crate::sleeves::SpawnParent::owner(),
    )
}

pub(super) fn recover_chat_sleeves(hub: &Hub) -> Vec<(String, String)> {
    let mut failures = Vec::new();
    for row in &hub.chat_recovery {
        let request = recovery_request(hub, row);
        if let Err(error) = spawn_resleeve(hub, &request, row.harness) {
            hub.audit.write(protectorate_audit::Entry {
                actor: "hub".into(),
                procedure: "chat_recovery".into(),
                subject: row.name.clone(),
                outcome: "error".into(),
                code: format!("{:?}", error.code),
                detail: error.to_string(),
                ..Default::default()
            });
            failures.push((row.name.clone(), error.to_string()));
        }
    }
    failures
}

fn recovery_request(hub: &Hub, row: &Record) -> Value {
    let generated =
        crate::sleeves::branch::base_grants(hub.workspaces.state_root(), &row.name, &row.base_repo)
            .unwrap_or_default();
    let read_only = row
        .read_only
        .iter()
        .filter(|mount| {
            !generated.iter().any(|grant| {
                mount.source == grant.source.to_string_lossy()
                    && mount.target == grant.target.to_string_lossy()
            })
        })
        .collect::<Vec<_>>();
    json!({
        "name": row.name,
        "workspace": row.workspace,
        "mode": "SLEEVE_MODE_CHAT",
        "recovery": true,
        "selection": row.chat.as_ref().and_then(|chat| chat.pointer("/settings/requested")),
        "supervised": row.supervised,
        "limits": row.limits,
        "readOnly": read_only,
        "nested": row.nested,
        "env": row.env,
        "exitAfterIdle": row.exit_after_idle,
    })
}

/// Resume the owner-facing Envoy after an unplanned process exit. An explicit
/// owner kill leaves `killed` set and must not be undone by queued mail.
pub(super) fn resume_envoy_for_mail(hub: &Hub) -> Result<bool> {
    let _guard = hub.envoy_spawn_lock.lock().unwrap();
    let Ok(row) = hub.sleeves.get("envoy") else {
        return Ok(false);
    };
    if row.mode != Mode::Chat || row.killed {
        return Ok(false);
    }
    let row = crate::sleeves::refresh_record(hub, &row);
    if row.state != State::Exited || row.killed {
        return Ok(false);
    }
    spawn_resleeve(hub, &recovery_request(hub, &row), row.harness)?;
    Ok(true)
}

fn spawn_chat(
    hub: &Hub,
    value: &Value,
    harness: Harness,
    envoy: bool,
    resleeve: bool,
    parent: &crate::sleeves::SpawnParent,
) -> Result<Record> {
    let exit_after_idle = parse_exit_after_idle(value)?;
    let _guard = hub.sleeve_launch_lock.lock().unwrap();
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    let launcher = &runtime.launcher;
    let name = string(value, "name");
    protectorate_sleeve_registry::valid_name(&name).map_err(registry_error)?;
    if (name == "envoy") != envoy {
        return Err(RpcError::new(
            Code::PermissionDenied,
            "envoy is a reserved sleeve name",
        ));
    }
    if !resleeve && hub.resleeving.lock().unwrap().contains(&name) {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "sleeve is being resleeved",
        ));
    }
    let mode = string(value, "mode");
    if !mode.is_empty() && mode != "SLEEVE_MODE_UNSPECIFIED" && mode != "SLEEVE_MODE_CHAT" {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "chat harness needs chat mode",
        ));
    }
    if value
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
    {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "args apply to terminal mode; a chat sleeve runs the harness's headless protocol",
        ));
    }
    let caller_env = crate::sleeves::request_env(value)?;
    let workspace = fs::canonicalize(PathBuf::from(string(value, "workspace")))
        .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))?;
    if !workspace.is_dir() || !workspace.is_absolute() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "workspace must be an absolute directory",
        ));
    }
    let previous = hub.sleeves.get(&name).ok();
    if previous.as_ref().is_some_and(|r| r.state == State::Running) {
        return Err(RpcError::new(Code::AlreadyExists, "sleeve already running"));
    }
    runtime.stop(&name)?;
    runtime.logs.lock().unwrap().remove(&name);
    runtime.requests.lock().unwrap().remove(&name);
    let root = launcher.state_root.join("sleeves").join(&name);
    let home = root.join("home");
    fs::create_dir_all(&home).map_err(internal)?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    let cache_environment =
        protectorate_sleeve_runner::prepare_private_caches(&home, &launcher.user_home)
            .map_err(internal)?;
    let (provider, executable, tool_roots, harness_name) = match harness {
        Harness::Pi => (
            Provider::Pi,
            launcher.pi_executable.as_ref(),
            &launcher.tool_roots,
            "Pi",
        ),
        Harness::Claude => (
            Provider::Claude,
            launcher.claude_executable.as_ref(),
            &launcher.claude_tool_roots,
            "Claude",
        ),
        Harness::Codex => (
            Provider::Codex,
            launcher.codex_executable.as_ref(),
            &launcher.codex_tool_roots,
            "Codex",
        ),
        Harness::Grok => (
            Provider::Grok,
            launcher.grok_executable.as_ref(),
            &launcher.grok_tool_roots,
            "Grok",
        ),
        _ => {
            return Err(RpcError::new(
                Code::Unimplemented,
                "chat harness is not implemented",
            ))
        }
    };
    let mut tool_roots = tool_roots.clone();
    if let Some(sdk) = &launcher.flutter_sdk_root {
        tool_roots.push(sdk.clone());
    }
    tool_roots.sort();
    tool_roots.dedup();
    let executable = executable.ok_or_else(|| {
        RpcError::new(
            Code::Unavailable,
            format!("{harness_name} executable is not configured"),
        )
    })?;
    if !executable.is_file() {
        return Err(RpcError::new(
            Code::Unavailable,
            format!("{harness_name} executable is missing"),
        ));
    }
    if harness == Harness::Pi {
        fs::create_dir_all(home.join(".pi/sessions")).map_err(internal)?;
    }
    let auth = launcher.state_root.join("auth").join(provider.as_str());
    if !auth.is_dir() {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            format!(
                "{harness_name} credential directory is missing: {}",
                auth.display()
            ),
        ));
    }
    let mut readonly = value
        .get("readOnly")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|item| {
                    let source = PathBuf::from(string(item, "source"));
                    let target = PathBuf::from(
                        item["target"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(item["source"].as_str().unwrap_or("")),
                    );
                    BindGrant {
                        source,
                        target,
                        access: Access::ReadOnly,
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let base_repo = previous
        .as_ref()
        .map_or_else(|| string(value, "baseRepo"), |row| row.base_repo.clone());
    let base_grants = crate::sleeves::branch::base_grants(&launcher.state_root, &name, &base_repo)?;
    readonly.extend(base_grants);
    let limits = &value["limits"];
    let cpu_percent = u32::try_from(number(limits, "cpuPercent")?)
        .map_err(|_| RpcError::new(Code::InvalidArgument, "CPU limit exceeds u32"))?;
    let memory_bytes = number(limits, "memoryBytes")?;
    let disk_bytes = number(limits, "diskBytes")?;
    let nested = value.get("nested").and_then(Value::as_bool) == Some(true);
    let mut command = if harness == Harness::Pi {
        vec![
            executable.to_string_lossy().into_owned(),
            "--mode".into(),
            "rpc".into(),
        ]
    } else if harness == Harness::Codex {
        vec![
            executable.to_string_lossy().into_owned(),
            "app-server".into(),
        ]
    } else if harness == Harness::Grok {
        let mut command = vec![executable.to_string_lossy().into_owned()];
        command.extend(protectorate_grok_driver::stdio_args().map(str::to_owned));
        command
    } else {
        vec![
            executable.to_string_lossy().into_owned(),
            "-p".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--permission-prompts".into(),
            "host".into(),
            "--permission-prompt-tool".into(),
            "stdio".into(),
            "--permission-mode".into(),
            "manual".into(),
        ]
    };
    let old_session = hub
        .sleeves
        .get(&name)
        .ok()
        .filter(|r| r.harness == harness)
        .and_then(|r| r.chat)
        .and_then(|c| c["harnessSessionId"].as_str().map(str::to_owned));
    let session_id = if harness == Harness::Claude {
        Some(match old_session.as_deref() {
            Some(id) => id.to_owned(),
            None => protectorate_claude_driver::new_session_id()
                .map_err(|e| RpcError::new(Code::Internal, e))?,
        })
    } else {
        None
    };
    if harness == Harness::Pi {
        if let Some(id) = old_session.as_deref() {
            command.extend(["--session".into(), id.into()]);
        }
    } else if harness == Harness::Codex || harness == Harness::Grok {
        // Codex and Grok resume by their stdio RPC after initialize.
    } else if let Some(id) = old_session.as_deref() {
        command.extend(["--resume".into(), id.into()]);
    } else if harness == Harness::Claude {
        command.extend(["--session-id".into(), session_id.clone().unwrap()]);
    }
    let policy = Policy {
        state_root: launcher.state_root.clone(),
        user_home: launcher.user_home.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::from([(provider, auth.clone())]),
        trusted_tool_roots: tool_roots.clone(),
        immutable_cache_roots: launcher
            .flutter_cache_lower
            .iter()
            .chain(launcher.flutter_tools_lower.iter())
            .chain(launcher.pub_cache_lower.iter())
            .chain(launcher.go_mod_cache_lower.iter())
            .cloned()
            .collect(),
        nested_cgroups_available: nested,
    };
    let plan = policy
        .plan(Request {
            workload: if nested {
                Workload::NestedRuntimeTestSleeve
            } else {
                Workload::ChatSleeve
            },
            command,
            workspace: Some(workspace.clone()),
            managed_workspace_name: protectorate_sandbox::managed_workspace_name(
                &launcher.state_root,
                &workspace,
            )
            .or_else(|| envoy.then(|| "envoy".to_owned())),
            private_home: Some(home),
            grants: readonly.clone(),
            tool_roots: tool_roots.clone(),
            selected_executable: None,
            credential: Some(provider),
            overlays: protectorate_sleeve_runner::flutter_overlay(
                &launcher.state_root,
                &name,
                launcher.flutter_sdk_root.as_deref(),
                launcher.flutter_cache_lower.as_deref(),
            )
            .map_err(internal)?
            .into_iter()
            .chain(
                protectorate_sleeve_runner::flutter_tools_overlay(
                    &launcher.state_root,
                    &name,
                    launcher.flutter_sdk_root.as_deref(),
                    launcher.flutter_tools_lower.as_deref(),
                )
                .map_err(internal)?,
            )
            .chain(
                protectorate_sleeve_runner::pub_cache_overlay(
                    &launcher.state_root,
                    &name,
                    &launcher.user_home,
                    launcher.pub_cache_lower.as_deref(),
                )
                .map_err(internal)?,
            )
            .chain(
                protectorate_sleeve_runner::go_mod_cache_overlay(
                    &launcher.state_root,
                    &name,
                    &launcher.user_home,
                    launcher.go_mod_cache_lower.as_deref(),
                )
                .map_err(internal)?,
            )
            .collect(),
            masks: Vec::new(),
            egress: Some(crate::sleeves::sleeve_egress(hub, harness == Harness::Pi)),
            limits: Limits {
                cpu_percent,
                memory_bytes,
                tasks_max: launcher.tasks_max,
            },
        })
        .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))?;
    let run_id = match previous.as_ref() {
        Some(old) if resleeve && !old.run_id.is_empty() => old.run_id.clone(),
        _ => crate::sleeves::new_run_id()?,
    };
    let mut options = LaunchOptions::new(
        &launcher.helper,
        &launcher.runtime_dir,
        &launcher.user_home,
        &workspace,
    );
    options.environment = launcher.environment.clone();
    options.egress_log = Some(crate::sleeves::egress_log(hub, &name, &run_id));
    options.environment.extend(cache_environment);
    options.environment.extend(caller_env.clone());
    if launcher.pub_cache_lower.is_some() {
        options.environment.insert(
            "PUB_CACHE".into(),
            launcher
                .user_home
                .join(".pub-cache")
                .to_string_lossy()
                .into_owned(),
        );
    }
    if let Some(toolchain) =
        protectorate_sleeve_runner::rustup_default_toolchain(&launcher.user_home, &tool_roots)
    {
        options
            .environment
            .entry("RUSTUP_TOOLCHAIN".into())
            .or_insert(toolchain);
    }
    options.environment.insert(
        "PROTECTORATE_TOKEN_FILE".into(),
        if envoy {
            workspace.join(".protectorate-token")
        } else {
            launcher.user_home.join(".protectorate-token")
        }
        .to_string_lossy()
        .into_owned(),
    );
    options.environment.insert(
        "HOME".into(),
        launcher.user_home.to_string_lossy().into_owned(),
    );
    options
        .environment
        .insert("PROTECTORATE_SLEEVE".into(), name.clone());
    if harness == Harness::Pi {
        options.environment.insert(
            "PI_CODING_AGENT_DIR".into(),
            "/run/protectorate/auth/pi".into(),
        );
        options.environment.insert(
            "PI_CODING_AGENT_SESSION_DIR".into(),
            launcher
                .user_home
                .join(".pi/sessions")
                .to_string_lossy()
                .into_owned(),
        );
    } else if harness == Harness::Claude {
        // Claude's transcript and settings live in the sleeve's private home.
        // Only its credential file resolves to the shared provider authority.
        let config = root.join("home/.claude");
        fs::create_dir_all(&config).map_err(internal)?;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).map_err(internal)?;
        let credential = config.join(".credentials.json");
        if auth.join(".credentials.json").exists() && fs::symlink_metadata(&credential).is_err() {
            std::os::unix::fs::symlink(
                "/run/protectorate/auth/claude/.credentials.json",
                &credential,
            )
            .map_err(internal)?;
        }
        options.environment.insert(
            "CLAUDE_CONFIG_DIR".into(),
            launcher
                .user_home
                .join(".claude")
                .to_string_lossy()
                .into_owned(),
        );
    } else if harness == Harness::Codex {
        let config = root.join("home/.codex");
        fs::create_dir_all(&config).map_err(internal)?;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).map_err(internal)?;
        let credential = config.join("auth.json");
        if auth.join("auth.json").exists() && fs::symlink_metadata(&credential).is_err() {
            std::os::unix::fs::symlink("/run/protectorate/auth/codex/auth.json", &credential)
                .map_err(internal)?;
        }
        let config_file = config.join("config.toml");
        if fs::symlink_metadata(&config_file).is_err() {
            fs::write(
                &config_file,
                "approval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n",
            )
            .map_err(internal)?;
            fs::set_permissions(&config_file, fs::Permissions::from_mode(0o600))
                .map_err(internal)?;
        }
        options.environment.insert(
            "CODEX_HOME".into(),
            launcher
                .user_home
                .join(".codex")
                .to_string_lossy()
                .into_owned(),
        );
    } else {
        let config = root.join("home/.grok");
        fs::create_dir_all(&config).map_err(internal)?;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).map_err(internal)?;
        for filename in ["auth.json", "auth.json.lock"] {
            let credential = config.join(filename);
            if fs::symlink_metadata(&credential).is_err() {
                std::os::unix::fs::symlink(
                    format!("/run/protectorate/auth/grok/{filename}"),
                    &credential,
                )
                .map_err(internal)?;
            }
        }
        options.environment.insert(
            "GROK_HOME".into(),
            launcher
                .user_home
                .join(".grok")
                .to_string_lossy()
                .into_owned(),
        );
    }
    let log = runtime.log(&name)?;
    if envoy {
        revoke_identity(hub, &name)?;
    } else {
        issue_identity(hub, &name)?;
    }
    let session: Arc<dyn ChatSession> = if harness == Harness::Pi {
        PiSession::start(&plan, options, Arc::clone(&log), old_session.as_deref())
            .map(|s| s as Arc<dyn ChatSession>)
    } else if harness == Harness::Claude {
        ClaudeSession::start(
            &plan,
            options,
            Arc::clone(&log),
            session_id.unwrap(),
            !value["supervised"].as_bool().unwrap_or(false),
        )
        .map(|s| s as Arc<dyn ChatSession>)
    } else if harness == Harness::Codex {
        CodexSession::start(
            &plan,
            options,
            Arc::clone(&log),
            old_session.as_deref(),
            !value["supervised"].as_bool().unwrap_or(false),
        )
        .map(|s| s as Arc<dyn ChatSession>)
    } else {
        GrokSession::start(
            &plan,
            options,
            Arc::clone(&log),
            old_session.as_deref(),
            !value["supervised"].as_bool().unwrap_or(false),
        )
        .map(|s| s as Arc<dyn ChatSession>)
    }
    .map_err(|e| {
        let _ = revoke_identity(hub, &name);
        RpcError::new(Code::Unavailable, e)
    })?;
    let model = string(&value["selection"], "model");
    let mut effort = string(&value["selection"], "effort");
    let defaulted_effort = effort.is_empty()
        && value["recovery"] != true
        && matches!(harness, Harness::Pi | Harness::Codex)
        && !hub.sleeve_effort.is_empty();
    if defaulted_effort {
        effort = hub.sleeve_effort.clone();
    }
    if !model.is_empty() || !effort.is_empty() {
        if let Err(error) = session.set_selection(&model, &effort) {
            let unsupported_effort = error.contains("does not advertise effort")
                || error.contains("requested effort is not available");
            if defaulted_effort && unsupported_effort {
                eprintln!("chat sleeve {name}: hub default effort {effort:?} is unavailable; using native default: {error}");
                if let Err(fallback) = session.set_selection(&model, "") {
                    session.stop();
                    let _ = revoke_identity(hub, &name);
                    return Err(driver_error(fallback));
                }
            } else {
                session.stop();
                let _ = revoke_identity(hub, &name);
                return Err(driver_error(error));
            }
        }
    }
    let mut row = Record::new(&name, harness, Mode::Chat, &workspace, &now()).map_err(|e| {
        session.stop();
        let _ = revoke_identity(hub, &name);
        registry_error(e)
    })?;
    if resleeve {
        if let Some(old) = previous.as_ref() {
            row.parent = old.parent.clone();
            row.parent_token_id = old.parent_token_id.clone();
            row.result = old.result.clone();
            row.handoff = old.handoff.clone();
            row.exit_after_idle = old.exit_after_idle.clone();
            row.git_branch = old.git_branch.clone();
            row.managed_clone = old.managed_clone;
        }
    }
    if let Some(old) = previous {
        row.base_repo = old.base_repo;
        row.base_branch = old.base_branch;
        row.base_tip = old.base_tip;
        row.managed_clone = old.managed_clone;
    } else {
        row.base_repo = string(value, "baseRepo");
        row.base_branch = string(value, "baseBranch");
        row.base_tip = string(value, "baseTip");
        row.managed_clone = value["managedClone"] == true;
    }
    row.chat = Some(chat_status(hub, session.as_ref()));
    row.env = caller_env;
    row.run_id = run_id;
    let state_event_cursor = log.last();
    row.supervised = value["supervised"] == true;
    if !resleeve && !exit_after_idle.is_zero() {
        row.exit_after_idle = value["exitAfterIdle"].as_str().map(str::to_owned);
    }
    if !resleeve {
        row.parent = parent.label.clone();
        row.parent_token_id = parent.token_id.clone();
    }
    row.limits.cpu_percent = cpu_percent;
    row.limits.memory_bytes = memory_bytes;
    row.limits.disk_bytes = disk_bytes;
    row.nested = nested;
    row.read_only = readonly
        .iter()
        .map(|g| ReadOnlyMount {
            source: g.source.to_string_lossy().into_owned(),
            target: g.target.to_string_lossy().into_owned(),
        })
        .collect();
    if let Some(brief) = value
        .get("brief")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        if let Err(error) = session.send(brief, "hub", "") {
            session.stop();
            let _ = revoke_identity(hub, &name);
            return Err(RpcError::new(Code::Unavailable, error));
        }
    }
    let row = hub.sleeves.record_spawned(row).map_err(|e| {
        session.stop();
        let _ = revoke_identity(hub, &name);
        registry_error(e)
    })?;
    runtime
        .sessions
        .lock()
        .unwrap()
        .insert(name.clone(), Arc::clone(&session));
    hub.emit(
        "sleeve.spawned",
        &name,
        json!({"harness":provider.as_str(),"mode":"chat","workspace":workspace}),
    );
    ChatStateWatcher {
        runtime: Arc::downgrade(runtime),
        session,
        log,
        after: state_event_cursor,
        events: Arc::clone(&hub.events),
        inventory: Arc::clone(&hub.inventory),
        hosts: hub.local_model_hosts.clone(),
        name,
        checkpoints: crate::checkpoints::Store::for_chat(launcher),
        sleeves: Arc::clone(&hub.sleeves),
    }
    .start();
    Ok(row)
}

pub(super) fn kill_pi(hub: &Hub, name: &str) -> Result<()> {
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    if row.mode != Mode::Chat {
        return Err(RpcError::new(Code::FailedPrecondition, "not a chat sleeve"));
    }
    if let Some(runtime) = &hub.chat_runtime {
        runtime.stop(name)?;
        revoke_identity(hub, name)?;
    }
    if row.state == State::Running {
        hub.sleeves
            .record_exit(name, now(), None, 0, false, true)
            .map_err(registry_error)?;
        hub.emit("sleeve.exited", name, json!({"reason":"killed"}));
    }
    Ok(())
}

pub(super) fn parse_exit_after_idle(value: &Value) -> Result<Duration> {
    super::parse_duration(value.get("exitAfterIdle").filter(|raw| !raw.is_null()))
        .map_err(|_| RpcError::new(Code::InvalidArgument, "invalid exit-after-idle duration"))
}

pub(super) fn start_idle_reaper(runtime: &tokio::runtime::Handle, hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    runtime.spawn(async move {
        let mut idle_since = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(hub) = weak.upgrade() else { break };
            idle_since =
                match tokio::task::spawn_blocking(move || reap_idle(&hub, idle_since)).await {
                    Ok(next) => next,
                    Err(error) => {
                        eprintln!("idle sleeve reaper stopped: {error}");
                        break;
                    }
                };
        }
    });
}

fn reap_idle(
    hub: &Hub,
    mut previous: HashMap<String, (String, Instant)>,
) -> HashMap<String, (String, Instant)> {
    let mut next = HashMap::new();
    let Some(runtime) = &hub.chat_runtime else {
        return next;
    };
    let mut pending = None;
    for row in hub.sleeves.list() {
        if row.state != State::Running || row.mode != Mode::Chat || row.name == "envoy" {
            continue;
        }
        let Some(limit) = row
            .exit_after_idle
            .as_ref()
            .and_then(|raw| super::parse_duration(Some(&Value::String(raw.clone()))).ok())
        else {
            continue;
        };
        if limit.is_zero() || hub.resleeving.lock().unwrap().contains(&row.name) {
            continue;
        }
        let session = runtime.sessions.lock().unwrap().get(&row.name).cloned();
        let Some(session) = session else { continue };
        if !session.is_idle()
            || pending
                .get_or_insert_with(|| hub.mail.pending())
                .get(&row.name)
                .is_some_and(|mail| !mail.is_empty())
        {
            continue;
        }
        let since = previous
            .remove(&row.name)
            .filter(|(created, _)| created == &row.created_at)
            .map(|(_, since)| since)
            .unwrap_or_else(Instant::now);
        if since.elapsed() < limit {
            next.insert(row.name.clone(), (row.created_at, since));
            continue;
        }
        let _launch = hub.sleeve_launch_lock.lock().unwrap();
        let busy = hub.resleeving.lock().unwrap();
        let still_idle = !busy.contains(&row.name)
            && hub.sleeves.get(&row.name).is_ok_and(|current| {
                current.state == State::Running && current.created_at == row.created_at
            })
            && runtime
                .sessions
                .lock()
                .unwrap()
                .get(&row.name)
                .is_some_and(|current| current.is_idle())
            && hub.mail.pending().get(&row.name).is_none_or(Vec::is_empty);
        if still_idle {
            if let Err(error) = kill_pi(hub, &row.name) {
                eprintln!("reap idle sleeve {}: {error}", row.name);
            } else {
                hub.directory_kick.notify_one();
            }
        }
    }
    next
}

fn send(hub: &Hub, name: &str, text: &str, request_id: &str, actor: &str) -> Result<u64> {
    send_inner(hub, name, text, request_id, actor, false)
}

fn send_inner(
    hub: &Hub,
    name: &str,
    text: &str,
    request_id: &str,
    actor: &str,
    require_idle: bool,
) -> Result<u64> {
    let resleeving = hub.resleeving.lock().unwrap();
    if resleeving.contains(name) {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "sleeve is being resleeved",
        ));
    }
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    if row.mode != Mode::Chat || row.state != State::Running {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "chat sleeve is not running",
        ));
    }
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    let _ = runtime.log(name)?;
    let mut requests = runtime.requests.lock().unwrap();
    let keys = requests.entry(name.to_owned()).or_default();
    keys.sweep(request_now_ms());
    if !request_id.is_empty() {
        if let Some(seq) = keys.get(request_id) {
            if !require_idle {
                hub.sleeves
                    .clear_result_before(name, seq)
                    .map_err(registry_error)?;
            }
            return Ok(seq);
        }
        if keys.len() >= MAX_REQUEST_KEYS {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "too many request ids in use; retry later",
            ));
        }
    }
    let session = runtime
        .sessions
        .lock()
        .unwrap()
        .get(name)
        .cloned()
        .ok_or_else(|| RpcError::new(Code::Unavailable, "chat process is unavailable"))?;
    if require_idle && !session.is_idle() {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "chat sleeve is busy; mailbox delivery waits for idle",
        ));
    }
    let seq = session
        .send(text, actor, request_id)
        .map_err(|e| RpcError::new(Code::Unavailable, e))?;
    drop(resleeving);
    if !request_id.is_empty() {
        keys.insert(
            request_id.to_owned(),
            seq,
            request_now_ms().saturating_add(REQUEST_WINDOW_MS),
        );
    }
    if !require_idle {
        hub.sleeves
            .clear_result_before(name, seq)
            .map_err(registry_error)?;
    }
    let _ = hub
        .sleeves
        .set_chat(name, chat_status(hub, session.as_ref()));
    Ok(seq)
}

/// Delivers queued mailbox text without replacing the latest user brief result.
pub(super) fn send_mailbox(
    hub: &Hub,
    name: &str,
    text: &str,
    request_id: &str,
    from: &str,
) -> Result<u64> {
    send_inner(hub, name, text, request_id, from, true)
}

/// A missing or exited process is not ready for mailbox delivery.
pub(super) fn chat_idle(hub: &Hub, name: &str) -> bool {
    if !hub
        .sleeves
        .get(name)
        .is_ok_and(|row| row.mode == Mode::Chat && row.state == State::Running)
    {
        return false;
    }
    hub.chat_runtime
        .as_ref()
        .and_then(|runtime| runtime.sessions.lock().unwrap().get(name).cloned())
        .is_some_and(|session| session.is_idle())
}

pub(super) fn quiesce_for_resleeve(
    hub: &Hub,
    name: &str,
    interrupt: bool,
    wait: std::time::Duration,
) -> Result<Value> {
    let session = live_session(hub, name)?;
    let deadline = std::time::Instant::now()
        .checked_add(wait)
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "resleeve wait is too large"))?;
    let mut interrupted = false;
    let mut stop_deadline = deadline;
    loop {
        if session.is_idle() || session.is_exited() {
            return Ok(chat_status(hub, session.as_ref()));
        }
        if !interrupted && (interrupt || std::time::Instant::now() >= deadline) {
            let _ = session.interrupt();
            interrupted = true;
            stop_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        }
        if interrupted && std::time::Instant::now() >= stop_deadline {
            return Ok(chat_status(hub, session.as_ref()));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub(super) fn append_handoff(hub: &Hub, name: &str, handoff: &Value) -> Result<()> {
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    runtime
        .log(name)?
        .append(Event::new(json!({"handoff":handoff})))
        .map_err(log_error)?;
    Ok(())
}

pub(super) fn append_error(hub: &Hub, name: &str, text: &str) -> Result<()> {
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    runtime
        .log(name)?
        .append(Event::new(json!({"error":{"text":text}})))
        .map_err(log_error)?;
    Ok(())
}

pub(super) fn send_resleeve_prompt(hub: &Hub, name: &str, text: &str) -> Result<()> {
    let session = live_session(hub, name)?;
    session
        .send(text, "hub", "")
        .map_err(|error| RpcError::new(Code::Unavailable, error))?;
    let _ = hub
        .sleeves
        .set_chat(name, chat_status(hub, session.as_ref()));
    Ok(())
}
fn interrupt(hub: &Hub, name: &str) -> Result<()> {
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    if row.mode != Mode::Chat {
        return Err(RpcError::new(Code::FailedPrecondition, "not a chat sleeve"));
    }
    if let Some(session) = runtime.sessions.lock().unwrap().get(name) {
        session
            .interrupt()
            .map_err(|e| RpcError::new(Code::Unavailable, e))?;
    }
    Ok(())
}
fn own_sleeve(actor: &str, target: &str) -> Result<()> {
    if let Some(self_name) = actor.strip_prefix("token:sleeve:") {
        if self_name != target {
            return Err(RpcError::new(
                Code::PermissionDenied,
                format!("sleeve {self_name} may only access its own chat"),
            ));
        }
    }
    Ok(())
}
fn live_session(hub: &Hub, name: &str) -> Result<Arc<dyn ChatSession>> {
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    if row.mode != Mode::Chat {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "sleeve is not in chat mode",
        ));
    }
    if row.state != State::Running {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "chat sleeve has exited",
        ));
    }
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    let session = runtime.sessions.lock().unwrap().get(name).cloned();
    session.ok_or_else(|| RpcError::new(Code::Unavailable, "chat process is unavailable"))
}
fn driver_error(error: String) -> RpcError {
    if let Some(message) = error.strip_prefix("invalid: ") {
        RpcError::new(Code::InvalidArgument, message)
    } else if let Some(message) = error.strip_prefix("precondition: ") {
        RpcError::new(Code::FailedPrecondition, message)
    } else if let Some(message) = error.strip_prefix("unimplemented: ") {
        RpcError::new(Code::Unimplemented, message)
    } else {
        RpcError::new(Code::Unavailable, error)
    }
}
fn identity_path(hub: &Hub, name: &str) -> PathBuf {
    hub.chat_runtime
        .as_ref()
        .expect("chat runtime configured")
        .launcher
        .state_root
        .join("sleeves")
        .join(name)
        .join("home/.protectorate-token")
}
pub(super) fn revoke_identity(hub: &Hub, name: &str) -> Result<()> {
    let now = super::now_unix();
    for token in hub.identity.list() {
        if token.name == format!("sleeve:{name}") && token.is_active(now) {
            hub.identity
                .revoke(&token.id, now)
                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
        }
    }
    let path = identity_path(hub, name);
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(internal(error)),
    }
}
fn issue_identity(hub: &Hub, name: &str) -> Result<()> {
    revoke_identity(hub, name)?;
    let catalog = wire::Catalog::embedded()?;
    let methods = catalog.methods().cloned().collect::<Vec<_>>();
    let scope_methods = super::scope_methods(&methods);
    let issued = hub
        .identity
        .create(
            &format!("sleeve:{name}"),
            &[
                "read",
                "backlog_*",
                "directory_*",
                "lease_*",
                "message_*",
                "chat_report",
            ],
            std::time::Duration::ZERO,
            super::now_unix(),
            &scope_methods,
        )
        .map_err(|e| RpcError::new(Code::Internal, format!("issue sleeve token: {e}")))?;
    let path = identity_path(hub, name);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("token-{}-{nonce}", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(issued.secret.expose().as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
    };
    if let Err(error) = write() {
        let _ = fs::remove_file(&temporary);
        let _ = hub.identity.revoke(&issued.token.id, super::now_unix());
        return Err(internal(error));
    }
    Ok(())
}
fn list_models(hub: &Hub, name: &str, harness: &str) -> Result<Value> {
    if name.is_empty() {
        if harness.is_empty() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "sleeve or harness is required",
            ));
        }
        return match harness.to_ascii_lowercase().as_str() {
            "codex" => catalog_codex(hub),
            "pi" | "claude" | "grok" => Ok(
                json!({"supported":false,"reason":format!("model/effort controls are not implemented for {harness}")}),
            ),
            _ => Err(RpcError::new(
                Code::InvalidArgument,
                format!("unknown chat harness {harness:?}"),
            )),
        };
    }
    let row = hub.sleeves.get(name).map_err(registry_error)?;
    if !matches!(row.harness, Harness::Pi | Harness::Codex | Harness::Grok) {
        return Ok(
            json!({"supported":false,"reason":"model controls are not implemented for this harness"}),
        );
    }
    live_session(hub, name)?.list_models().map_err(driver_error)
}

/// Query Codex's native catalog in a short lived sandbox. The app server is
/// initialized, but no thread is created and no sleeve is registered.
fn catalog_codex(hub: &Hub) -> Result<Value> {
    let launcher = &hub.chat_runtime.as_ref().ok_or_else(unavailable)?.launcher;
    let executable = launcher
        .codex_executable
        .as_ref()
        .ok_or_else(|| RpcError::new(Code::Unavailable, "Codex executable is not configured"))?;
    if !executable.is_file() {
        return Err(RpcError::new(
            Code::Unavailable,
            "Codex executable is missing",
        ));
    }
    let auth = launcher.state_root.join("auth/codex");
    if !auth.is_dir() {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "Codex credential directory is missing",
        ));
    }
    let sleeves_root = launcher.state_root.join("sleeves");
    fs::create_dir_all(&sleeves_root).map_err(internal)?;
    let scratch = tempfile::Builder::new()
        .prefix("catalog-")
        .tempdir_in(&sleeves_root)
        .map_err(internal)?;
    let home = scratch.path().join("home");
    let config = home.join(".codex");
    let workspace_root = tempfile::tempdir().map_err(internal)?;
    let workspace = workspace_root.path().join("workspace");
    fs::create_dir_all(&config).map_err(internal)?;
    fs::create_dir_all(&workspace).map_err(internal)?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    if auth.join("auth.json").is_file() {
        std::os::unix::fs::symlink(
            "/run/protectorate/auth/codex/auth.json",
            config.join("auth.json"),
        )
        .map_err(internal)?;
    }
    fs::write(
        config.join("config.toml"),
        "approval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n",
    )
    .map_err(internal)?;
    let mut tool_roots = launcher.codex_tool_roots.clone();
    tool_roots.sort();
    tool_roots.dedup();
    let policy = Policy {
        state_root: launcher.state_root.clone(),
        user_home: launcher.user_home.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::from([(Provider::Codex, auth)]),
        trusted_tool_roots: tool_roots.clone(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let plan = policy
        .plan(Request {
            workload: Workload::ChatSleeve,
            command: vec![
                executable.to_string_lossy().into_owned(),
                "app-server".into(),
            ],
            workspace: Some(workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: tool_roots.clone(),
            selected_executable: None,
            credential: Some(Provider::Codex),
            overlays: Vec::new(),
            masks: Vec::new(),
            egress: Some(crate::sleeves::sleeve_egress(hub, false)),
            limits: Limits {
                cpu_percent: 0,
                memory_bytes: 0,
                tasks_max: launcher.tasks_max,
            },
        })
        .map_err(|error| RpcError::new(Code::Unavailable, error.to_string()))?;
    let mut options = LaunchOptions::new(
        &launcher.helper,
        &launcher.runtime_dir,
        &launcher.user_home,
        &workspace,
    );
    options.stdio = protectorate_sandbox::runtime::StdioMode::Duplex;
    options.environment = launcher.environment.clone();
    options.egress_log = Some(crate::sleeves::egress_log(hub, "codex-catalog", ""));
    options.environment.extend(
        protectorate_sleeve_runner::prepare_private_caches(
            scratch.path().join("home").as_path(),
            &launcher.user_home,
        )
        .map_err(internal)?,
    );
    options.environment.insert(
        "HOME".into(),
        launcher.user_home.to_string_lossy().into_owned(),
    );
    options.environment.insert(
        "CODEX_HOME".into(),
        launcher
            .user_home
            .join(".codex")
            .to_string_lossy()
            .into_owned(),
    );
    let mut child = protectorate_sandbox::runtime::spawn(&plan, &options)
        .map_err(|error| RpcError::new(Code::Unavailable, error.to_string()))?;
    let result = query_codex_catalog(&mut child);
    let _ = child.cancel();
    let _ = child.wait();
    result
        .map_err(|error| RpcError::new(Code::Unavailable, format!("Codex model catalog: {error}")))
}

fn query_codex_catalog(
    child: &mut protectorate_sandbox::runtime::ChildHandle,
) -> std::result::Result<Value, String> {
    let mut stdin = child.take_stdin().ok_or("Codex catalog has no stdin")?;
    let stdout = child.take_stdout().ok_or("Codex catalog has no stdout")?;
    let stderr = child.take_stderr().ok_or("Codex catalog has no stderr")?;
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
    });
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if value.get("id").is_some() && tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let mut rpc = |id: u64, method: &str, params: Value| -> std::result::Result<Value, String> {
        writeln!(
            stdin,
            "{}",
            json!({"id":id,"method":method,"params":params})
        )
        .map_err(|error| error.to_string())?;
        stdin.flush().map_err(|error| error.to_string())?;
        loop {
            let reply = rx
                .recv_timeout(Duration::from_secs(15))
                .map_err(|error| error.to_string())?;
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if !reply["error"].is_null() {
                return Err(reply["error"]["message"]
                    .as_str()
                    .unwrap_or("Codex RPC failed")
                    .into());
            }
            if id == 1 {
                writeln!(stdin, "{}", json!({"method":"initialized","params":{}}))
                    .map_err(|error| error.to_string())?;
                stdin.flush().map_err(|error| error.to_string())?;
            }
            return Ok(reply["result"].clone());
        }
    };
    rpc(
        1,
        "initialize",
        json!({"clientInfo":{"name":"protectorate","version":"0"},"capabilities":{"experimentalApi":true}}),
    )?;
    let mut cursor = String::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut models = Vec::new();
    for id in 2..=102 {
        let params = if cursor.is_empty() {
            json!({"limit":100})
        } else {
            json!({"limit":100,"cursor":cursor})
        };
        let page = rpc(id, "model/list", params)?;
        let entries = page["data"]
            .as_array()
            .ok_or("Codex model/list returned no data")?;
        models.extend(entries.iter().map(|model| json!({
            "model":model["model"], "name":model["displayName"], "isDefault":model["isDefault"],
            "defaultEffort":model["defaultReasoningEffort"],
            "efforts":model["supportedReasoningEfforts"].as_array().map(|efforts| efforts.iter().map(|effort| effort["reasoningEffort"].clone()).collect::<Vec<_>>()).unwrap_or_default()
        })));
        cursor = page["nextCursor"].as_str().unwrap_or_default().to_owned();
        if cursor.is_empty() {
            return Ok(json!({"supported":true,"liveUpdate":true,"models":models}));
        }
        if !seen.insert(cursor.clone()) || models.len() > 10_000 {
            break;
        }
    }
    Err("Codex model catalog pagination did not finish".into())
}
fn set_selection(hub: &Hub, name: &str, model: &str, effort: &str) -> Result<Value> {
    if model.len() > 256 || effort.len() > 64 {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "selection is too long",
        ));
    }
    let session = live_session(hub, name)?;
    session.set_selection(model, effort).map_err(driver_error)?;
    let chat = chat_status(hub, session.as_ref());
    hub.sleeves
        .set_chat(name, chat.clone())
        .map_err(registry_error)?;
    hub.emit(
        "sleeve.selection",
        name,
        json!({"model":model,"effort":effort}),
    );
    Ok(chat)
}
fn compact(hub: &Hub, name: &str) -> Result<Value> {
    let session = live_session(hub, name)?;
    let compaction = session.compact().map_err(driver_error)?;
    hub.sleeves
        .set_chat(name, chat_status(hub, session.as_ref()))
        .map_err(registry_error)?;
    hub.emit("sleeve.compacted", name, compaction.clone());
    Ok(compaction)
}
fn report(hub: &Hub, actor: &str, value: &Value) -> Result<Value> {
    let self_name = if actor == "token:orchestrator-envoy" {
        "envoy"
    } else {
        actor.strip_prefix("token:sleeve:").ok_or_else(|| {
            RpcError::new(
                Code::PermissionDenied,
                "only a sleeve identity token may report",
            )
        })?
    };
    let target = string(value, "sleeve");
    if !target.is_empty() && target != self_name {
        return Err(RpcError::new(
            Code::PermissionDenied,
            "a sleeve may only report for itself",
        ));
    }
    let status = string(value, "status");
    if !matches!(
        status.as_str(),
        "REPORT_STATUS_COMPLETE" | "REPORT_STATUS_PARTIAL" | "REPORT_STATUS_BLOCKED"
    ) {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "status must be complete, partial or blocked",
        ));
    }
    let summary = string(value, "summary");
    if summary.len() > 32 * 1024 {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "summary exceeds 32 KiB",
        ));
    }
    let runtime = hub.chat_runtime.as_ref().ok_or_else(unavailable)?;
    let _request_guard = runtime.requests.lock().unwrap();
    let row = hub.sleeves.get(self_name).map_err(registry_error)?;
    let path = runtime
        .launcher
        .state_root
        .join("sleeves")
        .join(self_name)
        .join("chat.jsonl");
    let events = match protectorate_chatlog::read_all(&path) {
        Ok(events) => events,
        Err(LogError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(log_error(error)),
    };
    let brief_seq = events
        .into_iter()
        .rev()
        .find(|event| {
            event.payload["user"]["text"].is_string() && event.payload["user"]["from"] != "hub"
        })
        .map_or(0, |event| event.seq);
    let result = if row.state == State::Running && row.mode == Mode::Chat {
        let session = live_session(hub, self_name)?;
        session
            .append_report(&status, &summary, brief_seq)
            .map_err(driver_error)?
    } else {
        let log = protectorate_chatlog::Log::open(&path).map_err(log_error)?;
        let result = json!({"status":status,"summary":summary,
            "time":Utc::now().to_rfc3339_opts(SecondsFormat::Millis,true),
            "chatSeq":(log.last()+1).to_string(),"briefSeq":brief_seq.to_string()});
        log.append(Event::new(json!({"report":result})))
            .map_err(log_error)?;
        result
    };
    hub.sleeves
        .set_result(self_name, result.clone())
        .map_err(registry_error)?;
    if let Some(mut chat) = row.chat {
        if let Some(fields) = chat.as_object_mut() {
            fields.insert("lastSeq".into(), result["chatSeq"].clone());
        }
        hub.sleeves
            .set_chat(self_name, chat)
            .map_err(registry_error)?;
    }
    hub.emit(
        "sleeve.report",
        self_name,
        json!({"status":status.trim_start_matches("REPORT_STATUS_").to_ascii_lowercase(),
            "summary":summary,"time":result["time"],"chat_seq":result["chatSeq"],
            "brief_seq":result["briefSeq"]}),
    );
    Ok(result)
}
async fn wait(hub: &Hub, name: &str, value: &Value) -> Result<Value> {
    if name.is_empty() {
        return Err(RpcError::new(Code::InvalidArgument, "sleeve is required"));
    }
    let until = string(value, "until");
    let until = if until.is_empty() || until == "WAIT_UNTIL_UNSPECIFIED" {
        "WAIT_UNTIL_IDLE"
    } else {
        until.as_str()
    };
    if until != "WAIT_UNTIL_IDLE" && until != "WAIT_UNTIL_REPORT" {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "until must be idle or report",
        ));
    }
    let timeout = wait_timeout(value.get("timeout"))?;
    let deadline = if timeout.is_zero() {
        None
    } else {
        Some(tokio::time::Instant::now() + timeout)
    };
    loop {
        let row = hub.sleeves.get(name).map_err(registry_error)?;
        if until == "WAIT_UNTIL_REPORT" {
            if let Some(result) = row.result {
                return Ok(json!({"sleeve":name,"result":result}));
            }
        }
        if row.state == State::Exited {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "sleeve exited before wait condition",
            ));
        }
        if until == "WAIT_UNTIL_IDLE" {
            if row.mode != Mode::Chat {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "idle requires chat mode",
                ));
            }
            let session = live_session(hub, name)?;
            if session.is_idle() && hub.mail.pending().get(name).is_none_or(Vec::is_empty) {
                return Ok(json!({"sleeve":name}));
            }
        }
        if let Some(at) = deadline {
            if tokio::time::Instant::now() >= at {
                return Err(RpcError::new(Code::DeadlineExceeded, "wait timed out"));
            }
            tokio::time::sleep_until(std::cmp::min(
                at,
                tokio::time::Instant::now() + std::time::Duration::from_millis(50),
            ))
            .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}
fn wait_timeout(value: Option<&Value>) -> Result<std::time::Duration> {
    let Some(value) = value else {
        return Ok(std::time::Duration::from_secs(600));
    };
    let text = value
        .as_str()
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "timeout must be a duration"))?;
    let seconds = text
        .strip_suffix('s')
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "timeout must end in s"))?;
    let seconds: f64 = seconds
        .parse()
        .map_err(|_| RpcError::new(Code::InvalidArgument, "invalid timeout"))?;
    if !seconds.is_finite() || !(0.0..=86_400.0 * 365.0).contains(&seconds) {
        return Err(RpcError::new(Code::InvalidArgument, "invalid timeout"));
    }
    Ok(std::time::Duration::from_secs_f64(seconds))
}
fn event_json(event: &Event) -> Value {
    let time = Utc
        .timestamp_millis_opt(event.time_ms as i64)
        .single()
        .unwrap_or_else(Utc::now);
    let event_time = event.payload["report"]["time"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| time.to_rfc3339_opts(SecondsFormat::Millis, true));
    let mut value = json!({"seq":event.seq.to_string(),"time":event_time});
    if let (Some(to), Some(from)) = (value.as_object_mut(), event.payload.as_object()) {
        for (key, item) in from {
            let mut item = item.clone();
            if key == "user" {
                if let Some(user) = item.as_object_mut() {
                    user.remove("requestId");
                }
            }
            to.insert(key.clone(), item);
        }
    }
    value
}
fn number(value: &Value, key: &str) -> Result<u64> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| RpcError::new(Code::InvalidArgument, format!("invalid {key}"))),
        Some(Value::String(s)) => s
            .parse()
            .map_err(|_| RpcError::new(Code::InvalidArgument, format!("invalid {key}"))),
        _ => Err(RpcError::new(
            Code::InvalidArgument,
            format!("invalid {key}"),
        )),
    }
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}
fn unavailable() -> RpcError {
    RpcError::new(Code::Unavailable, "chat launcher is not configured")
}
fn internal(error: std::io::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn log_error(error: LogError) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn registry_error(error: protectorate_sleeve_registry::Error) -> RpcError {
    use protectorate_sleeve_registry::Error;
    let code = match error {
        Error::NotFound(_) => Code::NotFound,
        Error::Exists(_) => Code::AlreadyExists,
        Error::Invalid(_) => Code::InvalidArgument,
        Error::State(_) => Code::FailedPrecondition,
        Error::Io(_) | Error::Json(_) => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}
