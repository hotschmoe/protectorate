//! Durable sleeve lifecycle records. A launcher owns process creation and
//! calls `record_spawned` only after the sandbox is live. This store never
//! infers that a process exists from a saved row.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Exists(String),
    NotFound(String),
    State(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "invalid sleeve: {s}"),
            Self::Exists(s) => write!(f, "sleeve already running: {s}"),
            Self::NotFound(s) => write!(f, "sleeve not found: {s}"),
            Self::State(s) => write!(f, "invalid sleeve state: {s}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Grok,
    Pi,
    Shell,
}
impl Harness {
    pub fn proto(self) -> &'static str {
        match self {
            Self::Claude => "HARNESS_CLAUDE",
            Self::Codex => "HARNESS_CODEX",
            Self::Grok => "HARNESS_GROK",
            Self::Pi => "HARNESS_PI",
            Self::Shell => "HARNESS_SHELL",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Chat,
    Terminal,
}
impl Mode {
    pub fn proto(self) -> &'static str {
        match self {
            Self::Chat => "SLEEVE_MODE_CHAT",
            Self::Terminal => "SLEEVE_MODE_TERMINAL",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Running,
    Exited,
}
impl State {
    pub fn proto(self) -> &'static str {
        match self {
            Self::Running => "SLEEVE_STATE_RUNNING",
            Self::Exited => "SLEEVE_STATE_EXITED",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub cpu_percent: u32,
    pub memory_bytes: u64,
    pub disk_bytes: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Usage {
    pub cpu_percent: f64,
    pub cpu_seconds: f64,
    pub memory_current_bytes: u64,
    pub memory_peak_bytes: u64,
    pub pids: u32,
    pub oom_kills: u64,
    pub workspace_disk_bytes: u64,
    pub home_disk_bytes: u64,
    pub cache_disk_bytes: u64,
    pub go_build_cache_disk_bytes: u64,
    pub sampled_at: Option<String>,
    pub disk_sampled_at: Option<String>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskUsage {
    pub workspace_bytes: Option<u64>,
    pub home_bytes: Option<u64>,
    pub cache_bytes: Option<u64>,
    pub go_build_cache_bytes: Option<u64>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadOnlyMount {
    pub source: String,
    pub target: String,
}

/// What a launch of one harness ran: the executable's bytes and the profile
/// (sandbox plan inputs and tool scopes), each as SHA-256.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub executable_sha256: String,
    pub profile_sha256: String,
}

/// The nested protobuf-shaped summaries are retained without coupling this
/// persistence crate to generated messages or a specific harness driver.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub name: String,
    pub harness: Harness,
    pub mode: Mode,
    pub workspace: String,
    pub session: String,
    pub created_at: String,
    pub state: State,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub exited_at: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub exit_signal: i32,
    #[serde(default)]
    pub oom_killed: bool,
    #[serde(default)]
    pub chat: Option<serde_json::Value>,
    #[serde(default)]
    pub supervised: bool,
    #[serde(default)]
    pub handoff: Option<serde_json::Value>,
    #[serde(default)]
    pub parent: String,
    #[serde(default)]
    pub parent_token_id: String,
    #[serde(default)]
    pub read_only: Vec<ReadOnlyMount>,
    #[serde(default)]
    pub exit_after_idle: Option<String>,
    #[serde(default)]
    pub git_branch: String,
    #[serde(default)]
    pub base_repo: String,
    #[serde(default)]
    pub base_branch: String,
    #[serde(default)]
    pub base_tip: String,
    #[serde(default)]
    pub managed_clone: bool,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub killed: bool,
    #[serde(default)]
    pub nested: bool,
    #[serde(default)]
    pub helper: bool,
    /// Minted at spawn, kept across resleeves and restarts; correlates the
    /// sleeve's audit entries, jobs, mail, leases, checkpoints and egress.
    #[serde(default)]
    pub run_id: String,
    /// The last approved launch of each harness this sleeve ran, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pins: BTreeMap<String, Pin>,
}

impl Record {
    pub fn new(
        name: &str,
        harness: Harness,
        mode: Mode,
        workspace: &Path,
        created_at: &str,
    ) -> Result<Self> {
        let row = Self {
            name: name.into(),
            harness,
            mode,
            workspace: workspace.to_string_lossy().into_owned(),
            session: format!("sleeve-{name}"),
            created_at: created_at.into(),
            state: State::Running,
            limits: Limits::default(),
            usage: Usage::default(),
            args: Vec::new(),
            env: BTreeMap::new(),
            exited_at: None,
            exit_code: None,
            exit_signal: 0,
            oom_killed: false,
            chat: None,
            supervised: false,
            handoff: None,
            parent: String::new(),
            parent_token_id: String::new(),
            read_only: Vec::new(),
            exit_after_idle: None,
            git_branch: String::new(),
            base_repo: String::new(),
            base_branch: String::new(),
            base_tip: String::new(),
            managed_clone: false,
            result: None,
            killed: false,
            nested: false,
            helper: false,
            run_id: String::new(),
            pins: BTreeMap::new(),
        };
        row.validate()?;
        Ok(row)
    }
    pub fn validate(&self) -> Result<()> {
        valid_name(&self.name)?;
        validate_env(&self.env)?;
        if self.session != format!("sleeve-{}", self.name) {
            return Err(Error::Invalid("session must match sleeve name".into()));
        }
        if !Path::new(&self.workspace).is_absolute() || self.workspace.is_empty() {
            return Err(Error::Invalid("workspace must be an absolute path".into()));
        }
        if self.created_at.is_empty() {
            return Err(Error::Invalid("created_at is required".into()));
        }
        if self.mode == Mode::Chat && self.harness == Harness::Shell {
            return Err(Error::Invalid("shell harness needs terminal mode".into()));
        }
        if self.supervised && self.mode != Mode::Chat {
            return Err(Error::Invalid("supervised requires chat mode".into()));
        }
        if self.nested && (self.limits.cpu_percent == 0 || self.limits.memory_bytes == 0) {
            return Err(Error::Invalid(
                "nested sleeve needs CPU and memory limits".into(),
            ));
        }
        if self.state == State::Running
            && (self.exited_at.is_some() || self.exit_code.is_some() || self.killed)
        {
            return Err(Error::Invalid("running sleeve has exit metadata".into()));
        }
        if self.state == State::Exited && self.exited_at.is_none() {
            return Err(Error::Invalid("exited sleeve needs exited_at".into()));
        }
        for mount in &self.read_only {
            if !Path::new(&mount.source).is_absolute()
                || (!mount.target.is_empty() && !Path::new(&mount.target).is_absolute())
            {
                return Err(Error::Invalid(
                    "read-only mount paths must be absolute".into(),
                ));
            }
        }
        for value in [&self.chat, &self.handoff, &self.result]
            .into_iter()
            .flatten()
        {
            if !value.is_object() {
                return Err(Error::Invalid("nested summary must be an object".into()));
            }
        }
        Ok(())
    }
}
pub fn valid_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 40
        || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(Error::Invalid(format!(
            "name {name:?} must use 1-40 lowercase letters, digits or '-'"
        )));
    }
    Ok(())
}

pub fn validate_env(env: &BTreeMap<String, String>) -> Result<()> {
    for (key, value) in env {
        let mut bytes = key.bytes();
        let valid = bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !valid {
            return Err(Error::Invalid(format!("invalid env name {key:?}")));
        }
        if matches!(
            key.as_str(),
            "HOME"
                | "PATH"
                | "TERM"
                | "USER"
                | "LOGNAME"
                | "SHELL"
                | "LANG"
                | "COLORTERM"
                | "CLAUDE_CONFIG_DIR"
                | "CLAUDE_SECURESTORAGE_CONFIG_DIR"
                | "CODEX_HOME"
                | "GROK_HOME"
                | "PI_CODING_AGENT_DIR"
                | "PI_CODING_AGENT_SESSION_DIR"
                | "PROTECTORATE_HUB"
                | "PROTECTORATE_HOME"
                | "PROTECTORATE_TOKEN"
                | "PROTECTORATE_TOKEN_FILE"
                | "PROTECTORATE_TLS_CA_PEM"
                | "PROTECTORATE_SLEEVE"
                | "PROTECTORATE_SLEEVE_PATH"
        ) {
            return Err(Error::Invalid(format!(
                "env {key} is managed by the sleeve"
            )));
        }
        if value.contains('\0') {
            return Err(Error::Invalid(format!("env {key} contains a NUL byte")));
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    sleeves: Vec<Record>,
}
pub struct Registry {
    path: PathBuf,
    records: Mutex<BTreeMap<String, Record>>,
}
impl Registry {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<File>(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => File {
                version: 1,
                sleeves: Vec::new(),
            },
            Err(e) => return Err(e.into()),
        };
        if file.version != 1 {
            return Err(Error::Invalid(format!(
                "unsupported registry version {}",
                file.version
            )));
        }
        let mut records = BTreeMap::new();
        for row in file.sleeves {
            row.validate()?;
            if records.insert(row.name.clone(), row).is_some() {
                return Err(Error::Invalid("duplicate sleeve name".into()));
            }
        }
        Ok(Self {
            path,
            records: Mutex::new(records),
        })
    }
    pub fn list(&self) -> Vec<Record> {
        self.records
            .lock()
            .unwrap()
            .values()
            .filter(|r| !r.helper)
            .cloned()
            .collect()
    }
    pub fn get(&self, name: &str) -> Result<Record> {
        self.records
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| Error::NotFound(name.into()))
    }
    /// Called only after a launcher has a live process. An exited name may be
    /// respawned; its workspace/home may be reused by the launcher.
    pub fn record_spawned(&self, record: Record) -> Result<Record> {
        record.validate()?;
        if record.state != State::Running {
            return Err(Error::State("record_spawned needs a running row".into()));
        }
        self.update(|rows| {
            if rows
                .get(&record.name)
                .is_some_and(|old| old.state == State::Running)
            {
                return Err(Error::Exists(record.name.clone()));
            }
            rows.insert(record.name.clone(), record.clone());
            Ok(record)
        })
    }
    pub fn record_exit(
        &self,
        name: &str,
        at: String,
        code: Option<i32>,
        signal: i32,
        oom: bool,
        killed: bool,
    ) -> Result<Record> {
        if at.is_empty() {
            return Err(Error::Invalid("exit time is required".into()));
        }
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.state != State::Running {
                return Err(Error::State(format!("{name} already exited")));
            }
            row.state = State::Exited;
            row.exited_at = Some(at);
            row.exit_code = code;
            row.exit_signal = signal;
            row.oom_killed = oom;
            row.killed = killed;
            Ok(row.clone())
        })
    }
    /// A kill may race the background exit observer after the process has
    /// already stopped. Preserve that the caller requested termination.
    pub fn mark_killed_after_exit(&self, name: &str) -> Result<Record> {
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.state != State::Exited {
                return Err(Error::State(format!("{name} has not exited")));
            }
            row.killed = true;
            Ok(row.clone())
        })
    }
    /// Chat drivers have no recoverable tmux process yet. Terminal rows are
    /// reconciled against the configured tmux server after hub startup.
    pub fn mark_unmanaged_chat_exited(&self, at: &str) -> Result<usize> {
        if at.is_empty() {
            return Err(Error::Invalid("exit time is required".into()));
        }
        self.update(|rows| {
            let mut count = 0;
            for row in rows
                .values_mut()
                .filter(|r| r.state == State::Running && r.mode == Mode::Chat)
            {
                row.state = State::Exited;
                row.exited_at = Some(at.into());
                row.exit_code = None;
                row.exit_signal = 0;
                row.oom_killed = false;
                count += 1;
            }
            Ok(count)
        })
    }
    /// Merge a slow disk sample into the same sleeve incarnation. A respawn
    /// under the same name must never inherit an old scan's byte counts.
    pub fn set_disk_usage(
        &self,
        name: &str,
        created_at: &str,
        sample: DiskUsage,
        sampled_at: String,
    ) -> Result<Option<Record>> {
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.created_at != created_at {
                return Ok(None);
            }
            if let Some(bytes) = sample.workspace_bytes {
                row.usage.workspace_disk_bytes = bytes;
            }
            if let Some(bytes) = sample.home_bytes {
                row.usage.home_disk_bytes = bytes;
            }
            if let Some(bytes) = sample.cache_bytes {
                row.usage.cache_disk_bytes = bytes;
            }
            if let Some(bytes) = sample.go_build_cache_bytes {
                row.usage.go_build_cache_disk_bytes = bytes;
            }
            row.usage.disk_sampled_at = Some(sampled_at);
            Ok(Some(row.clone()))
        })
    }
    pub fn set_chat(&self, name: &str, chat: serde_json::Value) -> Result<Record> {
        if !chat.is_object() {
            return Err(Error::Invalid("chat status must be an object".into()));
        }
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.mode != Mode::Chat {
                return Err(Error::State(format!("{name} is a terminal sleeve")));
            }
            row.chat = Some(chat);
            Ok(row.clone())
        })
    }
    /// Advances the observed source tip only when the sleeve still tracks the
    /// expected tip. A replacement sleeve cannot inherit a stale notice.
    pub fn set_base_tip(&self, name: &str, expected: &str, tip: &str) -> Result<bool> {
        if !matches!(tip.len(), 40 | 64) || !tip.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Invalid("invalid base tip".into()));
        }
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.state != State::Running || row.base_tip != expected {
                return Ok(false);
            }
            row.base_tip = tip.into();
            Ok(true)
        })
    }
    pub fn set_handoff(&self, name: &str, handoff: serde_json::Value) -> Result<Record> {
        if !handoff.is_object() {
            return Err(Error::Invalid("handoff must be an object".into()));
        }
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.mode != Mode::Chat || row.state != State::Running {
                return Err(Error::State(format!("{name} is not a running chat sleeve")));
            }
            row.handoff = Some(handoff);
            Ok(row.clone())
        })
    }
    pub fn set_pin(&self, name: &str, harness: &str, pin: Pin) -> Result<Record> {
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            row.pins.insert(harness.into(), pin);
            Ok(row.clone())
        })
    }
    pub fn set_result(&self, name: &str, result: serde_json::Value) -> Result<Record> {
        if !result.is_object() {
            return Err(Error::Invalid("result must be an object".into()));
        }
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            row.result = Some(result);
            Ok(row.clone())
        })
    }
    /// A new non-mail user brief supersedes only an older reported result.
    /// Retrying an old request cannot erase a report for that same or a later brief.
    pub fn clear_result_before(&self, name: &str, brief_seq: u64) -> Result<Record> {
        self.update(|rows| {
            let row = rows
                .get_mut(name)
                .ok_or_else(|| Error::NotFound(name.into()))?;
            if row.result.as_ref().is_some_and(|result| {
                result["briefSeq"]
                    .as_str()
                    .and_then(|seq| seq.parse::<u64>().ok())
                    .is_none_or(|old| old < brief_seq)
            }) {
                row.result = None;
            }
            Ok(row.clone())
        })
    }
    fn update<T>(
        &self,
        apply: impl FnOnce(&mut BTreeMap<String, Record>) -> Result<T>,
    ) -> Result<T> {
        let mut guard = self.records.lock().unwrap();
        let mut next = guard.clone();
        let value = apply(&mut next)?;
        if next != *guard {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let data = serde_json::to_vec_pretty(&File {
                version: 1,
                sleeves: next.values().cloned().collect(),
            })?;
            protectorate_fsutil::write_file(&self.path, &data, 0o600)?;
            *guard = next;
        }
        Ok(value)
    }
}
