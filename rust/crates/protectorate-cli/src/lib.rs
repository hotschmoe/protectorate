//! Reusable dispatcher for the basic owner CLI commands: `status`,
//! `node ls`/`node show`, `backlog ls`/`show`/`add`/`done`/`reopen`, `lease ls`,
//! `job ls`/`job show`/`job cancel`/`job run`/`job logs`,
//! `chat send`/`chat watch`/`chat wait`/`chat respond`/`chat settings`,
//! `sleeve ls`/`sleeve workspaces`/`sleeve browse`/`sleeve show`/
//! `sleeve kill`/`sleeve spawn`/`sleeve envoy`/`sleeve resleeve`/
//! `sleeve prune`, the tmux sessions: `session ls`/`new`/`kill`/
//! `respawn`/`capture`/`attach`, the `msg` mailbox: `send`, `inbox`,
//! `reply`, `ack`, `thread`, `threads` and `wait`, and trusted-device
//! management: `devices ls`/`pending`/`approve`/`deny`/`revoke`/`rename` and
//! new-device `login`, plus the client's own `version` and a raw unary
//! `api call`.
//!
//! The dispatcher turns argv into authenticated Connect JSON calls through
//! `protectorate-connect-client` and renders the replies as aligned text, or
//! as compact proto-JSON one-liners when `--json` is set. It speaks only the
//! Connect API defined in `proto/protectorate/v1`; it never reads hub storage.
mod args;
mod attach;
mod format;
mod hub_update;
mod job;
mod login;
mod pair;
mod transfer;
#[cfg(windows)]
#[path = "login_windows.rs"]
pub mod windows_private;

pub use args::{parse, Invocation};
pub use job::RunError;
pub use protectorate_connect_client::Client;
use protectorate_connect_client::Credential;
use protectorate_wire::{proto_duration, v1, ReflectMessage};
pub use protectorate_wire::{Code, Result, RpcError};
use serde_json::{json, Value};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The hub URL used when neither `--hub` nor `PROTECTORATE_HUB` is set.
pub const DEFAULT_HUB: &str = "http://127.0.0.1:7470";
/// The request timeout used when `--timeout` is not set.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const HUB_URL_ENV: &str = "PROTECTORATE_HUB";
pub const TOKEN_ENV: &str = "PROTECTORATE_TOKEN";
pub const TOKEN_FILE_ENV: &str = "PROTECTORATE_TOKEN_FILE";
/// A saved device credential file the MCP bridge signs requests with.
pub const DEVICE_CREDENTIAL_ENV: &str = "PROTECTORATE_DEVICE_CREDENTIAL";
/// The state home holding this machine's saved device credential.
pub const HOME_ENV: &str = "PROTECTORATE_HOME";

const LIST_SESSIONS: &str = "/protectorate.v1.SessionService/ListSessions";
const CREATE_SESSION: &str = "/protectorate.v1.SessionService/CreateSession";
const KILL_SESSION: &str = "/protectorate.v1.SessionService/KillSession";
const RESPAWN_SESSION: &str = "/protectorate.v1.SessionService/RespawnSession";
const CAPTURE_PANE: &str = "/protectorate.v1.SessionService/CapturePane";
const MSG_SEND: &str = "/protectorate.v1.MessageService/Send";
const MSG_INBOX: &str = "/protectorate.v1.MessageService/Inbox";
const MSG_ACK: &str = "/protectorate.v1.MessageService/Ack";
const MSG_REPLY: &str = "/protectorate.v1.MessageService/Reply";
const MSG_THREAD: &str = "/protectorate.v1.MessageService/Thread";
const MSG_THREADS: &str = "/protectorate.v1.MessageService/ListThreads";
const MSG_WAIT: &str = "/protectorate.v1.MessageService/Wait";

/// The transport slack added to a `chat wait` deadline so the hub answers
/// with its own DeadlineExceeded instead of the HTTP client giving up.
const WAIT_SLACK: Duration = Duration::from_secs(5);
/// Sandbox startup can outlast the default 15s request timeout.
const SPAWN_TIMEOUT_FLOOR: Duration = Duration::from_secs(120);
/// The hub's resleeve wait when `--wait` is omitted.
const DEFAULT_RESLEEVE_WAIT: Duration = Duration::from_secs(5 * 60);
/// Time after the turn wait for the summary, the handoff and the new harness.
const RESLEEVE_AFTER_WAIT: Duration = Duration::from_secs(10 * 60);
/// The transport ceiling for `chat wait --timeout 0`, which has no deadline.
const WAIT_UNBOUNDED: Duration = Duration::from_secs(24 * 3600);
/// The longest `chat wait --timeout` the CLI accepts.
pub const MAX_WAIT: Duration = Duration::from_secs(24 * 3600);
/// `chat settings` with a selection re-reads the native catalog and waits for
/// the harness to confirm the new settings, so the transport has to outlast it.
const SELECTION_TIMEOUT_FLOOR: Duration = Duration::from_secs(120);
/// Release bundles can take longer than ten minutes to verify, transfer and
/// confirm on a slow node. Keep the public update stream open for that work.
const UPDATE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(3600);

/// Which notes `backlog ls` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NoteState {
    #[default]
    Open,
    Done,
    All,
}

impl NoteState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
            Self::All => "all",
        }
    }
}

/// The filters `backlog ls` accepts.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BacklogFilter {
    pub project: String,
    pub query: String,
    pub state: NoteState,
}

/// A direct resource or a selector for `lease acquire`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaseAcquire {
    pub resource: String,
    pub os: String,
    pub arch: String,
    pub min_free_memory_bytes: u64,
    pub gpu_vram_bytes: u64,
    pub gpu: String,
    pub npu: String,
    pub holder: String,
    pub ttl: Duration,
    pub wait: bool,
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditQuery {
    pub limit: i32,
    pub page_token: String,
    pub actor: String,
    pub procedure: String,
    pub subject: String,
    pub outcome: String,
    pub run_id: String,
    pub chronological: bool,
}

/// The note `backlog add` creates. `body` and `body_file` are mutually
/// exclusive; a `body_file` of `-` reads stdin.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NoteDraft {
    pub title: String,
    pub body: Option<String>,
    pub body_file: Option<String>,
    pub project: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteEdit {
    pub id: String,
    pub title: Option<String>,
    pub project: Option<String>,
    pub body: Option<String>,
    pub body_file: Option<String>,
    pub append: Option<String>,
}

/// Which jobs `job ls --state` keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Preparing,
    Running,
    Succeeded,
    Failed,
    Canceled,
    TimedOut,
    Killed,
}

impl JobState {
    /// The name `job ls --state` takes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::TimedOut => "timed-out",
            Self::Killed => "killed",
        }
    }

    pub fn as_proto(&self) -> v1::JobState {
        match self {
            Self::Preparing => v1::JobState::Preparing,
            Self::Running => v1::JobState::Running,
            Self::Succeeded => v1::JobState::Succeeded,
            Self::Failed => v1::JobState::Failed,
            Self::Canceled => v1::JobState::Canceled,
            Self::TimedOut => v1::JobState::TimedOut,
            Self::Killed => v1::JobState::Killed,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace('_', "-").as_str() {
            "preparing" => Some(Self::Preparing),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            "timed-out" => Some(Self::TimedOut),
            "killed" => Some(Self::Killed),
            _ => None,
        }
    }

    pub fn names() -> String {
        [
            Self::Preparing,
            Self::Running,
            Self::Succeeded,
            Self::Failed,
            Self::Canceled,
            Self::TimedOut,
            Self::Killed,
        ]
        .map(|state| state.as_str())
        .join("|")
    }
}

/// The filters `job ls` accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct JobFilter {
    pub state: Option<JobState>,
    /// Zero means the hub's own default.
    pub limit: i32,
}

/// How long a `job run` command may run when `--timeout` is omitted.
pub const DEFAULT_JOB_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// The hub's longest job timeout.
pub const MAX_JOB_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// `job run`. Paths stay as the user typed them until the command runs, so a
/// relative `--git` or `--artifact` is resolved in the working directory then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRun {
    pub on: String,
    pub git: String,
    pub artifact: String,
    pub workdir: String,
    pub artifact_globs: Vec<String>,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
    pub cpu_percent: u32,
    pub memory_bytes: u64,
    pub admin: bool,
    pub command: Vec<String>,
}

/// `job logs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobLogs {
    pub id: String,
    pub follow: bool,
}

/// The default `msg wait` deadline.
pub const DEFAULT_MSG_WAIT: Duration = Duration::from_secs(600);

/// Which condition `chat wait` blocks on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WaitUntil {
    /// The sleeve is idle with no pending mail.
    #[default]
    Idle,
    /// The sleeve has reported for its current brief.
    Report,
}

impl WaitUntil {
    /// The name `chat wait --until` takes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Report => "report",
        }
    }

    pub fn as_proto(&self) -> v1::WaitUntil {
        match self {
            Self::Idle => v1::WaitUntil::Idle,
            Self::Report => v1::WaitUntil::Report,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "idle" => Some(Self::Idle),
            "report" => Some(Self::Report),
            _ => None,
        }
    }
}

/// What `msg send --kind` sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MessageKind {
    /// Information; no one is asked to act.
    #[default]
    Note,
    /// Asks the recipient to do something, and asks for an ack by default.
    Request,
    /// States facts, typically sent by the hub.
    Notice,
}

impl MessageKind {
    /// The name `msg send --kind` takes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Request => "request",
            Self::Notice => "notice",
        }
    }

    /// The enum name MessageServiceSendRequest takes.
    pub fn as_proto(&self) -> &'static str {
        match self {
            Self::Note => "MESSAGE_KIND_NOTE",
            Self::Request => "MESSAGE_KIND_REQUEST",
            Self::Notice => "MESSAGE_KIND_NOTICE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "note" => Some(Self::Note),
            "request" => Some(Self::Request),
            "notice" => Some(Self::Notice),
            _ => None,
        }
    }

    pub fn names() -> String {
        [Self::Note, Self::Request, Self::Notice]
            .map(|kind| kind.as_str())
            .join("|")
    }
}

/// A harness name as the CLI spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessName {
    Claude,
    Codex,
    Grok,
    Pi,
    Shell,
}

impl HarnessName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
            Self::Shell => "shell",
        }
    }

    pub fn as_proto(self) -> v1::Harness {
        match self {
            Self::Claude => v1::Harness::Claude,
            Self::Codex => v1::Harness::Codex,
            Self::Grok => v1::Harness::Grok,
            Self::Pi => v1::Harness::Pi,
            Self::Shell => v1::Harness::Shell,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "grok" => Some(Self::Grok),
            "pi" => Some(Self::Pi),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }
}

/// `sleeve spawn --mode`. Empty means the hub's default for that harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SleeveModeName {
    #[default]
    Default,
    Chat,
    Terminal,
}

impl SleeveModeName {
    pub fn as_proto(self) -> v1::SleeveMode {
        match self {
            Self::Default => v1::SleeveMode::Unspecified,
            Self::Chat => v1::SleeveMode::Chat,
            Self::Terminal => v1::SleeveMode::Terminal,
        }
    }
}

/// `--model` and `--effort`. Both empty means the hub's native default.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ModelChoice {
    pub model: String,
    pub effort: String,
}

impl ModelChoice {
    pub fn is_set(&self) -> bool {
        !self.model.is_empty() || !self.effort.is_empty()
    }
}

/// `chat settings`. No `model`, no `effort` and no `native_defaults` reads the
/// current settings. A `model` or `effort` replaces the whole requested
/// selection; an explicit empty string asks the harness for that field's native
/// default. `native_defaults` restores both captured defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSettings {
    pub sleeve: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub native_defaults: bool,
}

impl ChatSettings {
    /// True when the invocation writes a selection instead of reading it.
    pub fn writes(&self) -> bool {
        self.native_defaults || self.model.is_some() || self.effort.is_some()
    }
}

/// One `sleeve spawn --ro host[:sandbox]` mount. An empty target uses the
/// host path inside the sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyBind {
    pub source: String,
    pub target: String,
}

/// `sleeve spawn`. `workspace` is still the user's text; a relative local
/// path is made absolute when the command runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SleeveSpawn {
    pub name: String,
    pub workspace: String,
    pub harness: HarnessName,
    pub args: Vec<String>,
    pub cpu_percent: u32,
    pub memory_bytes: u64,
    pub disk_bytes: u64,
    pub env: Vec<(String, String)>,
    pub mode: SleeveModeName,
    pub supervised: bool,
    pub selection: ModelChoice,
    pub read_only: Vec<ReadOnlyBind>,
    pub exit_after_idle: Duration,
    pub branch: String,
    pub new_branch: String,
    /// Path of the brief file, read by this CLI. Empty means no brief.
    pub brief_file: String,
    pub nested: bool,
    /// Weighted tokens the run may use; zero means no cap.
    pub token_budget: u64,
}

/// `sleeve envoy`, the owner-only SpawnEnvoy call. `harness` none is auto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SleeveEnvoy {
    pub harness: Option<HarnessName>,
    pub scopes: Vec<String>,
    pub add_scopes: Vec<String>,
    pub selection: ModelChoice,
    pub token_budget: u64,
}

/// `sleeve resleeve`. `to` none is auto. A zero `wait` leaves the hub's
/// five-minute default in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SleeveResleeve {
    pub name: String,
    pub to: Option<HarnessName>,
    pub interrupt: bool,
    pub wait: Duration,
    pub selection: ModelChoice,
    pub native_defaults: bool,
}

/// `session new`. `command` is the argv after `--`; empty starts the hub's
/// default shell. `cwd` empty leaves it to tmux.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SessionNew {
    pub name: String,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub command: Vec<String>,
}

/// `sleeve prune`. With no category set, the hub inventories caches.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SleevePrune {
    pub yes: bool,
    pub caches: bool,
    pub homes: bool,
    pub workspaces: bool,
    pub running: bool,
    pub ignored: bool,
}

/// What `sleeve checkpoint` does. A diff without `to` compares with the
/// working tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointAction {
    List,
    Create,
    Diff { from: u64, to: u64 },
    Restore { id: u64 },
}

/// The command an invocation asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    HelpPath(String),
    /// `version`: this client's own version and platform, without a hub call.
    Version,
    /// `api call <Service/Method> [json]`: one raw unary Connect JSON call.
    ApiCall {
        procedure: String,
        body: String,
    },
    Status,
    /// `hub update`: fetch a verified release and confirm the guardian trial.
    HubUpdate {
        channel: String,
        version: String,
    },
    Quota {
        refresh: bool,
    },
    QuotaSuggest {
        task_size: Option<String>,
        refresh: bool,
    },
    TokenCreate {
        name: String,
        scopes: Vec<String>,
        ttl: Duration,
    },
    TokenList,
    /// `orchestrator status`: Envoy's sleeve and most recently created token.
    OrchestratorStatus,
    TokenRevoke {
        id: String,
    },
    /// `token renew`: a fresh token for the capability this call presents.
    TokenRenew,
    /// `token attenuate`: narrow the presented capability offline.
    TokenAttenuate {
        scopes: Vec<String>,
        ttl: Option<Duration>,
    },
    GrantAdmin {
        node: String,
        task: String,
        ttl: Duration,
        out: Option<PathBuf>,
    },
    AuditList(AuditQuery),
    AuditVerify,
    ApprovalList {
        all: bool,
    },
    ApprovalDecide {
        id: String,
        approve: bool,
    },
    DeviceList,
    DevicePendingList,
    DeviceApprove {
        code: String,
    },
    DeviceDeny {
        code: String,
    },
    DeviceRevoke {
        id: String,
    },
    DeviceRename {
        id: String,
        name: String,
    },
    /// `login` with no harness: pair this machine with the hub. `name` and
    /// `platform` unset mean this machine's hostname and its OS/arch, `home`
    /// unset means the state home the credential is read from and saved to.
    DeviceLogin {
        name: Option<String>,
        platform: Option<String>,
        home: Option<PathBuf>,
    },
    /// Public links and a terminal QR for adding this hub on another device.
    Pair {
        url: Option<String>,
        app: bool,
    },
    Who {
        target: Option<String>,
    },
    Events,
    NodeList,
    /// `node metrics --local`: collect this machine's metrics without a hub.
    NodeMetricsLocal {
        watch: Vec<PathBuf>,
    },
    NodeShow {
        name: String,
    },
    NodeAdd {
        name: String,
        address: String,
        ssh_alias: String,
        tier: String,
        tags: Vec<String>,
        notes: String,
        update: bool,
    },
    NodeRemove {
        name: String,
        jobs: v1::RemoveJobs,
        uninstall: bool,
        wait_timeout: Option<Duration>,
    },
    NodeTier {
        name: String,
        tier: String,
    },
    NodeUpdate {
        name: Option<String>,
        all: bool,
        force: bool,
        no_wait: bool,
    },
    NodeInvite {
        name: String,
        tier: String,
        ttl: Duration,
        install_url: String,
    },
    NodePut {
        name: String,
        local: PathBuf,
        remote: String,
        force: bool,
        no_compress: bool,
    },
    NodeGet {
        name: String,
        remote: String,
        local: PathBuf,
        force: bool,
        no_compress: bool,
    },
    BacklogList(BacklogFilter),
    BacklogShow {
        id: String,
    },
    BacklogAdd(NoteDraft),
    BacklogEdit(NoteEdit),
    BacklogRemove {
        id: String,
    },
    BacklogPin {
        id: String,
        pinned: bool,
    },
    BacklogDone {
        id: String,
        resolution: Option<String>,
    },
    BacklogReopen {
        id: String,
    },
    LeaseList,
    LeaseAcquire(LeaseAcquire),
    LeaseRenew {
        id: String,
        ttl: Option<Duration>,
    },
    LeaseRelease {
        ids: Vec<String>,
    },
    JobList(JobFilter),
    JobShow {
        id: String,
    },
    JobCancel {
        id: String,
    },
    JobRun(JobRun),
    /// `job test-cross`: compile and execute Rust tests on a selected node.
    JobTestCross(JobRun),
    JobLogs(JobLogs),
    ChatSend {
        sleeve: String,
        text: String,
        request_id: String,
    },
    ChatWatch {
        sleeve: String,
        after: u64,
    },
    ChatRespond {
        sleeve: String,
        approval_id: String,
        allow: bool,
        note: String,
    },
    ChatWait {
        sleeves: Vec<String>,
        until: WaitUntil,
        timeout: Duration,
    },
    ChatInterrupt {
        sleeve: String,
    },
    ChatCompact {
        sleeve: String,
    },
    ChatStatus {
        sleeve: String,
    },
    ChatModels {
        sleeve: Option<String>,
        harness: String,
    },
    ChatSettings(ChatSettings),
    Report {
        status: String,
        summary_file: Option<String>,
    },
    SleeveList,
    SleeveWorkspaces,
    SleeveBrowse {
        path: String,
    },
    SleeveShow {
        name: String,
    },
    SleeveStatus {
        name: String,
        explain: bool,
    },
    SleeveKill {
        name: String,
        drop_caches: bool,
    },
    SleeveCheckpoint {
        name: String,
        action: CheckpointAction,
    },
    SleeveAttach {
        name: String,
        read_only: bool,
        escape: String,
    },
    SleeveSpawn(SleeveSpawn),
    SleeveEnvoy(SleeveEnvoy),
    SleeveResleeve(SleeveResleeve),
    SleevePrune(SleevePrune),
    SessionList,
    SessionNew(SessionNew),
    SessionKill {
        name: String,
    },
    SessionRespawn {
        name: String,
    },
    SessionCapture {
        name: String,
        lines: i32,
    },
    SessionAttach {
        name: String,
        read_only: bool,
        escape: String,
    },
    MsgSend {
        to: String,
        body: String,
        thread: String,
        kind: MessageKind,
        /// None leaves it to the hub: a request asks for an ack, other kinds
        /// do not.
        ack: Option<bool>,
        request_id: String,
    },
    MsgInbox {
        unread: bool,
        peek: bool,
        /// An owner-only mailbox to read instead of the caller's own.
        act_as: String,
        thread: String,
    },
    MsgReply {
        id: String,
        body: String,
        request_id: String,
    },
    MsgAck {
        id: String,
        act_as: String,
    },
    MsgThread {
        thread: String,
    },
    MsgThreads {
        sleeve: String,
        project: String,
        limit: i32,
    },
    MsgWait {
        target: String,
        timeout: Duration,
    },
}

/// argv plus the global options, ready to run.
fn usage() -> &'static str {
    "Usage:\n  \
     protectorate [--hub URL] [--token-file FILE] [--home DIR] [--json] [--timeout SECONDS] COMMAND\n\n\
     Commands:\n  \
     status                      hub version, host and uptime\n  \
     hub serve                   run the local Rust hub (--home, --listen, --tls-listen, --tls-name, and serve flags)\n  \
     hub backup                  snapshot local hub state (--home, --to DIR)\n  \
     hub restore FILE            restore into an empty state directory (--home DIR required)\n  \
     hub publish-flutter-cache   publish the Flutter SDK download cache (--home, --flutter-sdk-root, --max-bytes)\n  \
     hub publish-flutter-tools   publish Flutter tool binaries (--home, --flutter-sdk-root, --max-bytes)\n  \
     hub publish-pub-cache       publish the Pub package cache (--home, --pub-cache-root, --max-bytes)\n  \
     hub publish-go-mod-cache    publish the Go module cache (--home, --go-mod-cache-root, --max-bytes)\n  \
     hub update                 fetch a verified release and confirm restart (--channel, --version)\n  \
     service systemd hub|node    print a systemd unit (--binary, --home, --write, --install; serve flags after --)\n  \
     service windows-task       print Task Scheduler XML (--binary, --home, --trigger, --write)\n  \
     service install-windows    print Windows service registration commands (--binary, --home, --write)\n  \
     service termux             print Termux runit run and Termux:Boot scripts (--binary, --home, --write, --install; serve flags after --)\n  \
     quota [--refresh]           subscription usage, resets and projected unused capacity\n  \
     quota suggest [--for small|medium|large] [--refresh]  rank chat harness capacity\n  \
     token create <name>        mint a capability (--scope repeated, --ttl 1h, at most 24h; secret shown once)\n  \
     token ls | token list      capability lineages and their status, without secrets\n  \
     token revoke <id>          revoke a lineage immediately\n  \
     token renew                print a fresh token for the presented capability\n  \
     token attenuate            print the presented capability narrowed offline (--scope repeated, --ttl)\n  \
     approvals ls [--all]       agent calls waiting for the owner: id, actor, call, reason, expiry\n  \
     approvals approve <id>     run a gated agent call once\n  \
     approvals deny <id>        refuse a gated agent call\n  \
     devices ls | devices list  trusted devices: name, platform, last use and approver\n  \
     devices pending            pairing requests awaiting approval\n  \
     devices approve <code>     approve a request after matching its code on the new device\n  \
     devices deny <code>        deny a pairing request\n  \
     devices revoke <id>        revoke a device and disconnect its streams immediately\n  \
     devices rename <id> <name> rename a trusted device\n  \
     login [--name NAME] [--platform OS/ARCH] [--home DIR]  pair this machine with --hub through device approval\n  \
     pair [--url URL] [--app]  print a public pairing link and terminal QR; it carries the CA in PROTECTORATE_TLS_CA_CERT\n  \
     grant admin               mint a task grant (--node NAME|GLOB, --task TEXT, --ttl 4h, --out FILE)\n  \
     audit ls | audit tail     query owner audit history (-n/--lines, --actor, --procedure, --subject, --outcome, --run, --page-token)\n  \
     audit verify               check the audit hash chain against the hub's recorded head\n  \
     who [project|sleeve]       show projects, owning sleeves and dependency edges\n  \
     events                     stream hub events until interrupted\n  \
     node ls | node list        every node with its latest metrics summary\n  \
     node show <name>           one node with full metrics\n  \
     node metrics --local       collect this machine's metrics without a hub (--watch DIR repeated)\n  \
     node add <name> --addr HOST  add a node, or with --update change a joined node's address (--ssh, --tier, --tag, --notes)\n  \
     node rm <name>             drain, revoke and remove (--jobs wait|cancel, --wait-timeout, --uninstall)\n  \
     node tier <name> shared|care  retag and change a node's tier\n  \
     node update <name> | --all  push the hub release (--force, --no-wait)\n  \
     node serve                run this machine's Rust node agent over mutual TLS (--home DIR, --listen IP:PORT, --file-root DIR, --file-unconfined, --job-concurrency N, --sandbox-helper FILE, --allow-plain-jobs, --watch-dir DIR, --docker-host URL)\n  \
     node doctor               inspect this machine's Rust node install (--home DIR, --json)\n  \
     node join                 join a hub with an invite code (--hub URL, --ca-cert FILE, --code CODE or PROTECTORATE_JOIN_CODE, --name, --listen IP:PORT, --home DIR)\n  \
     node invite <name>         issue a single-use join code (--tier, --expires, --url)\n  \
     node put <node> <local> <remote>  copy file or directory to a node (-f, --no-compress)\n  \
     node get <node> <remote> <local>  copy file or directory from a node (-f, --no-compress)\n  \
     backlog ls | backlog list  notes, pinned first (--project, --query, --state open|done|all, --all)\n  \
     backlog show <id>          one note with its body\n  \
     backlog add [title]        add a note (--project, --pin, --body, --body-file FILE|-)\n  \
     backlog edit <id>          change fields (--title, --project, --body, --body-file, --append; otherwise $EDITOR)\n  \
     backlog rm <id>            delete a note permanently\n  \
     backlog pin <id>           pin a note (--off to unpin)\n  \
     backlog done <id>          mark a note done, with an optional --resolution line\n  \
     backlog reopen <id>        reopen a note and clear its completion\n  \
     lease ls | lease list      live leases, then queued tickets\n  \
     lease acquire             reserve a node or matching selector (--node, --os, --arch, --min-free-mem, --gpu-vram, --gpu, --npu, --holder, --ttl, --wait, --priority)\n  \
     lease renew <id>           claim or extend a lease (--ttl DURATION)\n  \
     lease release <id>...      free leases or withdraw queued tickets\n  \
     job ls | job list          jobs, newest first (--state preparing|running|succeeded|failed|canceled|timed-out|killed, --limit N)\n  \
     job show <id>              one job record\n  \
     job cancel <id>            stop a job and its process tree\n  \
     job run --on <target> -- <cmd...>\n                             \
       run a command on a node, lease or selector and stream its output\n                             \
       (--git repo[@commit] | --artifact file|dir, --workdir, --artifact-glob,\n                             \
       --env K=V, --timeout, --cpu, --mem, --admin on Linux nodes)\n  \
     job test-cross              run cargo test on a matching node (--os, --arch, --on, --git, --artifact, --artifact-glob, --package, --timeout; cargo args after --)\n  \
     job logs <id>              print a job's output (--follow, -f)\n  \
     chat send <sleeve> <text>  send a message (- reads stdin; --request-id makes a retry safe; -- before leading-dash text)\n  \
     chat watch <sleeve>        replay and follow chat events (--after SEQ; --json emits one event per line)\n  \
     chat respond <sleeve> <id> answer an approval (--allow or --deny; --note TEXT)\n  \
     chat wait <sleeve> [sleeve...]  block until every sleeve is idle or reports (--until idle|report, --timeout DURATION)\n  \
     chat interrupt <sleeve>    cancel the running turn\n  \
     chat compact <sleeve>      compact chat history while idle\n  \
     chat status <sleeve>       chat state, context and prompt-cache telemetry\n  \
     chat models [sleeve]       list native models and efforts (--harness, default codex)\n  \
     chat settings <sleeve>     show model and effort, or change them (--model, --effort, --native-defaults)\n  \
     report --status complete|partial|blocked  report this sleeve's result (--summary-file FILE|-; 32 KiB limit)\n  \
     sleeve ls | sleeve list    sleeves with live usage\n  \
     sleeve workspaces | ws     workspaces and read-only references on the hub\n  \
     sleeve browse [path]      browse configured workspace roots or a directory on the hub\n  \
     sleeve show <name>         one sleeve, its usage and the tail of an exited run\n  \
     sleeve status <name>       what a terminal sleeve is doing: working, blocked or idle (--explain lists the screen rules)\n  \
     sleeve kill <name>         stop a sleeve and its processes (--drop-caches)\n  \
     sleeve checkpoint <name>   list, create, diff <from> [to] or restore <id> workspace checkpoints\n  \
     sleeve attach <name>       attach a terminal (--readonly, --escape ^X|none; detach with Ctrl-])\n  \
     sleeve spawn <name>        start a harness (--workspace, --harness claude|codex|grok|pi|shell, --cpu, --mem, --disk, --mode chat|terminal, --supervised, --nested, --env K=V, --ro, --brief FILE, --branch, --new-branch, --exit-after-idle, --model, --effort, --budget TOKENS, -- args)\n  \
     sleeve envoy               create or respawn Envoy (--harness auto|claude|codex|grok|pi, --scopes, --add-scopes, --model, --effort, --budget TOKENS)\n  \
     orchestrator spawn         create or respawn Envoy (same options as sleeve envoy)\n  \
     orchestrator status        show Envoy sleeve and latest token\n  \
     sleeve resleeve <name>     move a chat sleeve (--to auto|claude|codex|grok|pi, --interrupt, --wait, --model, --effort, --native-defaults)\n  \
     sleeve prune               inventory unused sleeve disk; --yes removes (--caches, --homes, --workspaces, --ignored, --running)\n  \
     session ls | session list  tmux sessions, running and exited\n  \
     session new <name>         start a session (--cwd DIR, --env K=V, -- <cmd...>; default: the login shell)\n  \
     session kill <name>        stop a session and its processes, or drop an exited one\n  \
     session respawn <name>     restart an exited session with its original command\n  \
     session capture <name>     print the pane and its scrollback without attaching (--lines N)\n  \
     session attach <name>      attach a terminal to a session (--readonly, --escape ^X|none; detach with Ctrl-])\n  \
     msg send <to> <body>       mailbox note, request or notice (- reads stdin; --thread, --kind, --ack, --request-id; -- before leading-dash text)\n  \
     msg inbox                  your mail, oldest first (--unread, --peek, --as MAILBOX, --thread ID)\n  \
     msg reply <id> <body>      answer a message in its thread and ack it (--request-id; -- before leading-dash text)\n  \
     msg ack <id>               confirm a message (--as MAILBOX)\n  \
     msg thread <thread-id>     every message of a thread\n  \
     msg threads                threads, most recently active first (--sleeve NAME, --project ID, --limit N)\n  \
     msg wait <message-id|thread>  block on the hub for a reply or an ack (--timeout DURATION, default 10m)\n  \
     api call <Service/Method> [json]  one raw unary call, e.g. api call HubService/GetStatus '{}'\n  \
     completion bash|fish|zsh|powershell  print a shell completion script (--no-descriptions)\n  \
     version                      this client's version and platform\n\n\
     <to> is a sleeve name, project:<id-or-name>, orchestrator or owner; msg is\n  \
     also spelled message.\n\n\
     For chat wait and msg wait, --timeout is the hub-side deadline (5m, 90s,\n  \
     1h30m; for chat wait 0 waits until the condition holds), not the request\n  \
     timeout. sleeve spawn and sleeve envoy wait at least 120s. sleeve\n  \
     resleeve waits at least its --wait (default 5m) plus 10m.\n\n\
     job test-cross runs cargo test on a node with an installed Rust toolchain.\n  \
     --os and --arch select a matching node; --on accepts a node, lease or\n  \
     selector. The default --git . sends the hub's current commit and reuses\n  \
     the node's build cache. --artifact DIR stages a hub-side directory.\n  \
     --package and --artifact-glob may repeat; without a package Cargo tests\n  \
     the workspace. Collected artifacts are stored with the hub job. Cargo\n  \
     flags follow --.\n\n\
     session new, session respawn and session attach run or show unsandboxed
  \
     hub commands, so they need the owner token. session names are 1-64 of
  \
     A-Z a-z 0-9 _ -, and sleeve-* belongs to SleeveService.\n\n\
     Hub URL defaults to http://127.0.0.1:7470 or $PROTECTORATE_HUB. The\n  \
     credential is the capability in --token-file or $PROTECTORATE_TOKEN_FILE\n  \
     (renewed in place past half its life), then $PROTECTORATE_TOKEN, then the\n  \
     device key login saved for that hub under --home, $PROTECTORATE_HOME or\n  \
     $HOME/.protectorate. The owner token works only on the hub's loopback\n  \
     listener.\n"
}

fn command_usage(path: &str) -> Result<String> {
    let mut matches = Vec::new();
    for line in usage().lines() {
        let entry = line.trim_start();
        if entry.starts_with(path)
            && entry[path.len()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            matches.push(line);
        }
    }
    if matches.is_empty() {
        return Err(RpcError::invalid(format!("unknown command {path:?}")));
    }
    Ok(format!("Usage:\n{}\n", matches.join("\n")))
}

/// Runs an already parsed invocation against an existing client.
fn execute(client: &Client, invocation: &Invocation) -> Result<String> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    match render(client, invocation, &mut stdout, &mut stderr, None, false) {
        Ok(text) => {
            let mut output = String::from_utf8(stdout).map_err(|error| {
                RpcError::new(Code::Internal, format!("job output is not utf-8: {error}"))
            })?;
            output.push_str(&text);
            Ok(output)
        }
        Err(RunError::Rpc(error)) => Err(error),
        Err(RunError::Status { message, .. }) => Err(RpcError::new(Code::Unknown, message)),
    }
}

/// `signals` installs the process signal handler. The binary does; library
/// callers do not, so a test process is not rewired.
fn render(
    client: &Client,
    invocation: &Invocation,
    stdout: &mut dyn std::io::Write,
    stderr: &mut dyn std::io::Write,
    cancel: Option<&Client>,
    signals: bool,
) -> std::result::Result<String, RunError> {
    match &invocation.command {
        Command::JobRun(spec) | Command::JobTestCross(spec) => job::run_job(
            client,
            cancel,
            spec,
            invocation.json,
            signals,
            stdout,
            stderr,
        ),
        Command::JobLogs(spec) => job::logs(client, spec, signals, stdout, stderr),
        Command::ChatWatch { sleeve, after } => {
            let mut renderer = format::ChatRenderer::default();
            let request = v1::WatchRequest {
                sleeve: sleeve.clone(),
                after_seq: *after,
            };
            client.server_stream(v1::chat_service::WATCH, &request, |event| {
                if invocation.json {
                    stdout
                        .write_all(json_line(&event)?.as_bytes())
                        .map_err(|error| {
                            RpcError::new(Code::Internal, format!("write chat event: {error}"))
                        })?;
                } else {
                    renderer.render(stdout, &event)?;
                }
                stdout.flush().map_err(|error| {
                    RpcError::new(Code::Internal, format!("flush chat event: {error}"))
                })?;
                Ok(())
            })?;
            if !invocation.json {
                renderer.flush(stdout)?;
            }
            Ok(String::new())
        }
        Command::Events => {
            client.server_stream(
                v1::event_service::SUBSCRIBE,
                &v1::SubscribeRequest {},
                |event| {
                    let line = if invocation.json {
                        json_line(&event)?
                    } else {
                        format::event_text(&event)?
                    };
                    stdout.write_all(line.as_bytes()).map_err(|error| {
                        RpcError::new(Code::Internal, format!("write hub event: {error}"))
                    })?;
                    stdout.flush().map_err(|error| {
                        RpcError::new(Code::Internal, format!("flush hub event: {error}"))
                    })?;
                    Ok(())
                },
            )?;
            Ok(String::new())
        }
        Command::NodeRemove {
            name,
            jobs,
            uninstall,
            wait_timeout,
        } => {
            let request = v1::RemoveNodeRequest {
                name: name.clone(),
                jobs: *jobs as i32,
                uninstall: *uninstall,
                wait_timeout: wait_timeout.map(proto_duration),
            };
            client.server_stream(v1::node_service::REMOVE_NODE, &request, |progress| {
                let line = if invocation.json {
                    json_line(&progress)?
                } else {
                    format!(
                        "{}\t{}\n",
                        format::word(progress.phase().as_str_name(), "REMOVE_PHASE_"),
                        progress.message
                    )
                };
                stdout
                    .write_all(line.as_bytes())
                    .and_then(|()| stdout.flush())
                    .map_err(|e| RpcError::new(Code::Internal, format!("write node removal: {e}")))
            })?;
            Ok(String::new())
        }
        Command::NodeUpdate {
            name,
            all,
            force,
            no_wait,
        } => {
            update_nodes(
                client,
                name.as_deref(),
                *all,
                *force,
                *no_wait,
                invocation.json,
                stdout,
            )?;
            Ok(String::new())
        }
        Command::HubUpdate { channel, version } => {
            hub_update::run(
                client,
                channel,
                version,
                transport_timeout(invocation),
                invocation.json,
                stdout,
            )?;
            Ok(String::new())
        }
        Command::SleeveAttach {
            name,
            read_only,
            escape,
        } => attach::run(client, invocation, name, *read_only, escape, stdout, stderr)
            .map_err(RunError::from),
        Command::SessionAttach {
            name,
            read_only,
            escape,
        } => attach::run_session(invocation, name, *read_only, escape, stdout, stderr)
            .map_err(RunError::from),
        _ if matches!(invocation.command, Command::DeviceLogin { .. }) => {
            login::run(invocation, stdout).map_err(RunError::from)
        }
        Command::Pair { url, app } => {
            pair::run(invocation, url.as_deref(), *app).map_err(RunError::from)
        }
        Command::ChatWait {
            sleeves,
            until,
            timeout,
        } if sleeves.len() > 1 => {
            chat_wait_multi(client, sleeves, *until, *timeout, invocation.json, stdout)?;
            Ok(String::new())
        }
        _ => execute_commands(client, invocation, stderr).map_err(RunError::from),
    }
}

fn chat_wait_multi(
    client: &Client,
    sleeves: &[String],
    until: WaitUntil,
    timeout: Duration,
    json: bool,
    stdout: &mut dyn std::io::Write,
) -> Result<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    for sleeve in sleeves {
        let client = client.clone();
        let sender = sender.clone();
        let request = wait_request(sleeve, until, timeout);
        std::thread::Builder::new()
            .name("protectorate-chat-wait".into())
            .spawn(move || {
                let _ = sender.send(client.unary(v1::chat_service::WAIT, &request));
            })
            .map_err(|error| {
                RpcError::new(Code::ResourceExhausted, format!("start chat wait: {error}"))
            })?;
    }
    drop(sender);
    for _ in sleeves {
        let response = receiver
            .recv()
            .map_err(|_| RpcError::new(Code::Internal, "chat wait ended early"))??;
        let line = if json {
            json_line(&response)?
        } else {
            format::chat_wait_text(&response)
        };
        stdout
            .write_all(line.as_bytes())
            .map_err(|error| RpcError::new(Code::Internal, format!("write chat wait: {error}")))?;
        stdout
            .flush()
            .map_err(|error| RpcError::new(Code::Internal, format!("flush chat wait: {error}")))?;
    }
    Ok(())
}

fn wait_request(sleeve: &str, until: WaitUntil, timeout: Duration) -> v1::WaitRequest {
    v1::WaitRequest {
        sleeve: sleeve.to_owned(),
        until: until.as_proto() as i32,
        timeout: Some(proto_duration(timeout)),
    }
}

fn reserve_grant_file(path: &std::path::Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|error| {
        RpcError::new(
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Code::AlreadyExists
            } else {
                Code::Internal
            },
            format!("reserve grant secret {}: {error}", path.display()),
        )
    })
}

fn write_grant_secret(
    file: &mut std::fs::File,
    path: &std::path::Path,
    secret: &str,
) -> Result<()> {
    use std::io::Write;
    if secret.is_empty() {
        return Err(RpcError::new(
            Code::DataLoss,
            "hub returned no grant secret",
        ));
    }
    let result = file
        .write_all(secret.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all());
    if let Err(error) = result {
        return Err(RpcError::new(
            Code::Internal,
            format!("write grant secret {}: {error}", path.display()),
        ));
    }
    Ok(())
}

fn update_nodes(
    client: &Client,
    name: Option<&str>,
    all: bool,
    force: bool,
    no_wait: bool,
    json: bool,
    stdout: &mut dyn std::io::Write,
) -> Result<()> {
    let listing = client.unary(v1::node_service::LIST_NODES, &v1::ListNodesRequest {})?;
    let wanted = name.map(str::to_ascii_lowercase);
    let mut matched = false;
    let mut errors = Vec::new();
    for status in &listing.nodes {
        let Some(node) = &status.node else { continue };
        let current = node.name.as_str();
        if !all && Some(current) != wanted.as_deref() {
            continue;
        }
        matched = true;
        if node.local {
            if all {
                continue;
            }
            return Err(RpcError::new(
                Code::InvalidArgument,
                format!(
                    "{current} is the hub's own node: replace the hub's binary and restart the hub"
                ),
            ));
        }
        if status.freshness() != v1::NodeFreshness::Fresh || status.metrics.is_none() {
            let reason = format!(
                "node is not reachable ({})",
                status
                    .freshness()
                    .as_str_name()
                    .trim_start_matches("NODE_FRESHNESS_")
                    .to_ascii_lowercase()
            );
            if all {
                writeln!(stdout, "{current}\tskipped: {reason}")
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                continue;
            }
            return Err(RpcError::new(Code::Unavailable, reason));
        }
        let mut last_phase = v1::UpdatePhase::Unspecified;
        let request = v1::UpdateNodeRequest {
            name: current.to_owned(),
            force,
        };
        let result = client.server_stream(v1::node_service::UPDATE_NODE, &request, |progress| {
            last_phase = progress.phase();
            let line = if json {
                json_line(&progress)?
            } else {
                update_line(current, &progress)
            };
            if !line.is_empty() {
                stdout
                    .write_all(line.as_bytes())
                    .and_then(|()| stdout.flush())
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
            }
            if no_wait && last_phase == v1::UpdatePhase::Swapping {
                return Err(RpcError::new(Code::Canceled, "node update no-wait"));
            }
            Ok(())
        });
        if let Err(error) = result {
            if !(no_wait && error.code == Code::Canceled && error.message == "node update no-wait")
            {
                if !all {
                    return Err(error);
                }
                errors.push(format!("{current}: {}", error.message));
            }
        } else if last_phase == v1::UpdatePhase::RolledBack {
            if !all {
                return Err(RpcError::new(
                    Code::Aborted,
                    "the node came back on its previous version",
                ));
            }
            errors.push(format!(
                "{current}: the node came back on its previous version"
            ));
        }
    }
    if !matched && !all {
        return Err(RpcError::new(
            Code::NotFound,
            format!("no node {:?} in the inventory", name.unwrap_or_default()),
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(RpcError::new(Code::Unknown, errors.join("; ")))
    }
}

fn update_line(name: &str, progress: &v1::UpdateNodeProgress) -> String {
    use v1::UpdatePhase;
    let detail = match progress.phase() {
        UpdatePhase::Skipped => progress.message.clone(),
        UpdatePhase::Sending
            if progress.total_bytes > 0 && progress.sent_bytes >= progress.total_bytes =>
        {
            format!("sent {}", format::bytes(progress.sent_bytes))
        }
        UpdatePhase::Verifying => "verifying".into(),
        UpdatePhase::Swapping => format!(
            "swapped {} -> {}, restarting ({})",
            progress.previous_version, progress.version, progress.restart
        ),
        UpdatePhase::Restarting => "waiting for the node to come back".into(),
        UpdatePhase::Confirmed => format!("reports {}", progress.version),
        UpdatePhase::RolledBack => format!("rolled back: {}", progress.message),
        _ => String::new(),
    };
    if detail.is_empty() {
        String::new()
    } else {
        format!("{name}\t{detail}\n")
    }
}

/// The next report sequence: the clock in microseconds, kept above the last
/// one sent from this sleeve so a clock step back cannot reorder reports.
fn next_report_seq() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_micros() as u64);
    let state = std::env::var_os("PROTECTORATE_TOKEN_FILE")
        .map(PathBuf::from)
        .and_then(|token| Some(token.parent()?.join(".protectorate-report-seq")));
    let last = state
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let seq = now.max(last + 1);
    if let Some(path) = state {
        let _ = fs::write(path, seq.to_string());
    }
    seq
}

fn read_report_summary(path: Option<&str>) -> Result<String> {
    use std::io::Read;
    let Some(path) = path else {
        return Ok(String::new());
    };
    let source: Box<dyn Read> = if path == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(fs::File::open(path).map_err(|error| {
            RpcError::new(
                Code::InvalidArgument,
                format!("open report summary: {error}"),
            )
        })?)
    };
    let mut bytes = Vec::new();
    source
        .take(32 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            RpcError::new(
                Code::InvalidArgument,
                format!("read report summary: {error}"),
            )
        })?;
    if bytes.len() > 32 * 1024 {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "report summary exceeds 32768 bytes",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| RpcError::new(Code::InvalidArgument, "report summary must be UTF-8"))
}

fn execute_commands(
    client: &Client,
    invocation: &Invocation,
    stderr: &mut dyn std::io::Write,
) -> Result<String> {
    let json = invocation.json;
    match &invocation.command {
        Command::JobRun(_)
        | Command::JobTestCross(_)
        | Command::JobLogs(_)
        | Command::ChatWatch { .. }
        | Command::Events
        | Command::SleeveAttach { .. }
        | Command::SessionAttach { .. }
        | Command::NodeRemove { .. }
        | Command::NodeUpdate { .. }
        | Command::HubUpdate { .. }
        | Command::DeviceLogin { .. } => Err(RpcError::new(
            Code::Internal,
            "job run and job logs stream through a separate path",
        )),
        Command::Pair { url, app } => pair::run(invocation, url.as_deref(), *app),
        Command::Version => Ok(version_text(json)),
        Command::NodeMetricsLocal { watch } => local_metrics(watch, json),
        Command::ApiCall { procedure, body } => api_call(client, procedure, body, json),
        Command::Help => Ok(usage().to_string()),
        Command::HelpPath(path) => command_usage(path),
        Command::Who { target: None } => {
            let listing = client.unary(
                v1::directory_service::LIST_PROJECTS,
                &v1::ListProjectsRequest::default(),
            )?;
            if json {
                json_line(&listing)
            } else {
                Ok(format::projects_list_text(&listing))
            }
        }
        Command::Who {
            target: Some(target),
        } => {
            let mine = client.unary(
                v1::directory_service::LIST_PROJECTS,
                &v1::ListProjectsRequest {
                    sleeve: target.clone(),
                },
            )?;
            if !mine.projects.is_empty() {
                return if json {
                    json_line(&mine)
                } else {
                    Ok(format::projects_detail_text(&mine))
                };
            }
            let found = client.unary(
                v1::directory_service::GET_PROJECT,
                &v1::GetProjectRequest {
                    project: target.clone(),
                },
            )?;
            if json {
                json_line(&found)
            } else {
                Ok(found
                    .project
                    .as_ref()
                    .map(format::project_text)
                    .unwrap_or_default())
            }
        }
        Command::Status => {
            let status = client.unary(v1::hub_service::GET_STATUS, &v1::GetStatusRequest {})?;
            let runtime = match client.unary(
                v1::hub_service::GET_SLEEVE_RUNTIME,
                &v1::GetSleeveRuntimeRequest {},
            ) {
                Ok(runtime) => Some(runtime),
                Err(error) if error.code == Code::Unimplemented => None,
                Err(error) => return Err(error),
            };
            if json {
                json_line(&status)
            } else {
                Ok(format::status_text(&status, runtime.as_ref()))
            }
        }
        Command::Quota { refresh } => {
            let quota = client.unary(
                v1::quota_service::GET_QUOTA,
                &v1::GetQuotaRequest { refresh: *refresh },
            )?;
            if json {
                json_line(&quota)
            } else {
                Ok(format::quota_text(&quota))
            }
        }
        Command::QuotaSuggest { task_size, refresh } => {
            let mut request = v1::SuggestRequest {
                refresh: *refresh,
                ..Default::default()
            };
            if let Some(size) = task_size {
                let name = format!("TASK_SIZE_{}", size.to_ascii_uppercase());
                let size = v1::TaskSize::from_str_name(&name)
                    .ok_or_else(|| RpcError::invalid(format!("unknown task size {size:?}")))?;
                request.set_task_size(size);
            }
            let suggestions = client.unary(v1::quota_service::SUGGEST, &request)?;
            if json {
                json_line(&suggestions)
            } else {
                Ok(format::suggest_text(&suggestions))
            }
        }
        Command::TokenCreate { name, scopes, ttl } => {
            let issued = client.unary(
                v1::token_service::CREATE_TOKEN,
                &v1::CreateTokenRequest {
                    name: name.clone(),
                    scopes: scopes.clone(),
                    ttl: Some(proto_duration(*ttl)),
                },
            )?;
            if json {
                json_line(&issued)
            } else {
                Ok(format::token_created_text(&issued))
            }
        }
        Command::TokenList => {
            let listing =
                client.unary(v1::token_service::LIST_TOKENS, &v1::ListTokensRequest {})?;
            if json {
                json_line(&listing)
            } else {
                Ok(format::token_list_text(&listing))
            }
        }
        Command::OrchestratorStatus => orchestrator_status(client, json),
        Command::TokenRevoke { id } => {
            let revoked = client.unary(
                v1::token_service::REVOKE_TOKEN,
                &v1::RevokeTokenRequest { id: id.clone() },
            )?;
            if json {
                json_line(&revoked)
            } else {
                Ok(format::token_revoked_text(&revoked))
            }
        }
        Command::TokenRenew => {
            let renewed =
                client.unary(v1::token_service::RENEW_TOKEN, &v1::RenewTokenRequest {})?;
            if json {
                json_line(&renewed)
            } else {
                Ok(format::token_renewed_text(&renewed))
            }
        }
        Command::TokenAttenuate { .. } => Err(RpcError::invalid(
            "token attenuate runs without a hub client",
        )),
        Command::GrantAdmin {
            node,
            task,
            ttl,
            out,
        } => {
            let mut reserved = out
                .as_ref()
                .map(|path| reserve_grant_file(path))
                .transpose()?;
            let issued = client.unary(
                v1::grant_service::ADMIN,
                &v1::GrantServiceAdminRequest {
                    node: node.clone(),
                    ttl: Some(proto_duration(*ttl)),
                    task: task.clone(),
                },
            );
            let mut issued = match issued {
                Ok(issued) => issued,
                Err(error) => {
                    drop(reserved);
                    if let Some(path) = out {
                        let _ = std::fs::remove_file(path);
                    }
                    return Err(error);
                }
            };
            if let (Some(path), Some(mut file)) = (out, reserved.take()) {
                let saved = write_grant_secret(&mut file, path, &issued.secret);
                drop(file);
                if let Err(error) = saved {
                    let _ = std::fs::remove_file(path);
                    return Err(error);
                }
                issued.secret.clear();
            }
            if json {
                json_line(&issued)
            } else {
                Ok(format::grant_admin_text(&issued, node, out.as_deref()))
            }
        }
        Command::AuditList(query) => {
            let page = client.unary(
                v1::audit_service::LIST_AUDIT_ENTRIES,
                &v1::ListAuditEntriesRequest {
                    limit: query.limit,
                    page_token: query.page_token.clone(),
                    actor: query.actor.clone(),
                    procedure: query.procedure.clone(),
                    subject: query.subject.clone(),
                    outcome: query.outcome.clone(),
                    run_id: query.run_id.clone(),
                },
            )?;
            if json {
                if query.chronological {
                    json_lines(page.entries.iter().rev())
                } else {
                    json_line(&page)
                }
            } else {
                Ok(format::audit_entries_text(&page, query.chronological))
            }
        }
        Command::AuditVerify => {
            let report = client.unary_with_timeout(
                v1::audit_service::VERIFY_AUDIT,
                &v1::VerifyAuditRequest {},
                MAX_WAIT,
            )?;
            if !report.ok {
                return Err(RpcError::new(
                    Code::DataLoss,
                    format!("audit chain broken: {}", report.error),
                ));
            }
            if json {
                json_line(&report)
            } else {
                Ok(format::audit_verify_text(&report))
            }
        }
        Command::ApprovalList { all } => {
            let approvals = client.unary(
                v1::approval_service::LIST_APPROVALS,
                &v1::ListApprovalsRequest { all: *all },
            )?;
            if json {
                json_line(&approvals)
            } else {
                Ok(format::approvals_text(&approvals))
            }
        }
        Command::ApprovalDecide { id, approve } => {
            let response = client.unary(
                v1::approval_service::DECIDE_APPROVAL,
                &v1::DecideApprovalRequest {
                    id: id.clone(),
                    approve: *approve,
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!(
                    "{} {id}\n",
                    if *approve { "approved" } else { "denied" }
                ))
            }
        }
        Command::DeviceList => {
            let devices =
                client.unary(v1::device_service::LIST_DEVICES, &v1::ListDevicesRequest {})?;
            if json {
                json_line(&devices)
            } else {
                Ok(format::device_list_text(&devices))
            }
        }
        Command::DevicePendingList => {
            let requests = client.unary(
                v1::device_service::LIST_DEVICE_AUTH,
                &v1::ListDeviceAuthRequest {},
            )?;
            if json {
                json_line(&requests)
            } else {
                Ok(format::device_pending_text(&requests))
            }
        }
        Command::DeviceApprove { code } => {
            let response = client.unary(
                v1::device_service::APPROVE_DEVICE_AUTH,
                &v1::ApproveDeviceAuthRequest {
                    user_code: code.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("approved {code}\n"))
            }
        }
        Command::DeviceDeny { code } => {
            let response = client.unary(
                v1::device_service::DENY_DEVICE_AUTH,
                &v1::DenyDeviceAuthRequest {
                    user_code: code.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("denied {code}\n"))
            }
        }
        Command::DeviceRevoke { id } => {
            let response = client.unary(
                v1::device_service::REVOKE_DEVICE,
                &v1::RevokeDeviceRequest { id: id.clone() },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("revoked {id}\n"))
            }
        }
        Command::DeviceRename { id, name } => {
            let response = client.unary(
                v1::device_service::RENAME_DEVICE,
                &v1::RenameDeviceRequest {
                    id: id.clone(),
                    name: name.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("renamed {id} to {name}\n"))
            }
        }
        Command::NodeList => {
            let nodes = client.unary(v1::node_service::LIST_NODES, &v1::ListNodesRequest {})?;
            if json {
                json_line(&nodes)
            } else {
                Ok(format::node_list_text(&nodes))
            }
        }
        Command::NodeShow { name } => {
            let node = client.unary(
                v1::node_service::GET_NODE,
                &v1::GetNodeRequest { name: name.clone() },
            )?;
            if json {
                json_line(&node)
            } else {
                Ok(format::node_show_text(&node))
            }
        }
        Command::NodeAdd {
            name,
            address,
            ssh_alias,
            tier,
            tags,
            notes,
            update,
        } => {
            let request = v1::AddNodeRequest {
                node: Some(v1::NodeInfo {
                    name: name.clone(),
                    address: address.clone(),
                    ssh_alias: ssh_alias.clone(),
                    tier: tier.clone(),
                    tags: tags.clone(),
                    notes: notes.clone(),
                    ..Default::default()
                }),
                update: *update,
            };
            let node = client.unary(v1::node_service::ADD_NODE, &request)?;
            if json {
                json_line(&node)
            } else {
                Ok(format!(
                    "added {} ({}, tier {})\n",
                    or(&node.name, name),
                    or(&node.address, address),
                    or(&node.tier, tier)
                ))
            }
        }
        Command::NodeTier { name, tier } => {
            let node = client.unary(
                v1::node_service::SET_NODE_TIER,
                &v1::SetNodeTierRequest {
                    name: name.clone(),
                    tier: tier.clone(),
                },
            )?;
            if json {
                json_line(&node)
            } else {
                Ok(format!(
                    "{} is now tier {}\n",
                    or(&node.name, name),
                    or(&node.tier, tier)
                ))
            }
        }
        Command::NodeInvite {
            name,
            tier,
            ttl,
            install_url,
        } => {
            let invite = client.unary(
                v1::node_service::INVITE_NODE,
                &v1::InviteNodeRequest {
                    name: name.clone(),
                    tier: tier.clone(),
                    ttl: Some(proto_duration(*ttl)),
                    install_url: install_url.clone(),
                },
            )?;
            if json {
                json_line(&invite)
            } else {
                let invited_name = invite
                    .node
                    .as_ref()
                    .map_or(name.as_str(), |node| or(&node.name, name));
                let expiry = invite
                    .expires_at
                    .map_or_else(|| "unknown time".to_owned(), |at| at.to_string());
                writeln!(stderr, "{invited_name} is in the inventory as invited; the key is single-use and expires {expiry}")
                    .map_err(|error| RpcError::new(Code::Internal, format!("write invite status: {error}")))?;
                Ok(format::node_invite_text(&invite))
            }
        }
        Command::NodePut {
            name,
            local,
            remote,
            force,
            no_compress,
        } => {
            let (response, total, wire, elapsed) =
                transfer::put(client, name, local, remote, *force, *no_compress)?;
            if json {
                json_line(&response)
            } else {
                Ok(format!(
                    "put\t{name}:{}\nbytes\t{total} ({wire} bytes sent, {elapsed:?})\n",
                    or(&response.path, remote)
                ))
            }
        }
        Command::NodeGet {
            name,
            remote,
            local,
            force,
            no_compress,
        } => {
            let (trailer, wire, elapsed) =
                transfer::get(client, name, remote, local, *force, *no_compress)?;
            if json {
                json_line(&v1::FileTrailer {
                    size: trailer.size,
                    sha256: trailer.sha256,
                    skipped: trailer.skipped,
                })
            } else {
                let mut output = format!(
                    "get\t{name}:{remote}\nbytes\t{} ({wire} bytes received, {elapsed:?})\n",
                    trailer.size
                );
                if trailer.skipped > 0 {
                    output.push_str(&format!(
                        "skipped\t{} entries (symlinks or special files)\n",
                        trailer.skipped
                    ));
                }
                Ok(output)
            }
        }
        Command::BacklogList(filter) => {
            let notes = client.unary(
                v1::backlog_service::LIST_NOTES,
                &v1::ListNotesRequest {
                    project: filter.project.clone(),
                    query: filter.query.clone(),
                    state: filter.state.as_str().to_owned(),
                },
            )?;
            if json {
                json_lines(&notes.notes)
            } else {
                Ok(format::note_list_text(&notes))
            }
        }
        Command::BacklogShow { id } => {
            let response = client.unary(
                v1::backlog_service::GET_NOTE,
                &v1::GetNoteRequest { id: id.clone() },
            )?;
            let note = returned_note(response.note)?;
            if json {
                json_line(&note)
            } else {
                Ok(format::note_text(&note))
            }
        }
        Command::BacklogAdd(draft) => {
            let body = match (&draft.body_file, &draft.body) {
                (Some(path), _) => read_body(path)?,
                (None, Some(body)) => body.clone(),
                (None, None) => String::new(),
            };
            let response = client.unary(
                v1::backlog_service::CREATE_NOTE,
                &v1::CreateNoteRequest {
                    title: draft.title.clone(),
                    body,
                    pinned: draft.pinned,
                    project: draft.project.clone(),
                },
            )?;
            note_output(response.note, json)
        }
        Command::BacklogEdit(edit) => {
            let interactive = edit.title.is_none()
                && edit.project.is_none()
                && edit.body.is_none()
                && edit.body_file.is_none()
                && edit.append.is_none();
            if json && interactive {
                return Err(RpcError::invalid(
                    "edit --json needs --title, --body, --body-file, --append or --project",
                ));
            }
            let mut request = v1::UpdateNoteRequest {
                id: edit.id.clone(),
                title: edit.title.clone(),
                project: edit.project.clone(),
                body: edit.body.clone(),
                ..Default::default()
            };
            if let Some(path) = &edit.body_file {
                request.body = Some(read_body(path)?);
            }
            if let Some(append) = &edit.append {
                let current = current_note(client, &edit.id)?;
                let body = current.body.trim_end_matches('\n');
                request.body = Some(format!(
                    "{body}{}{append}\n",
                    if body.is_empty() { "" } else { "\n" },
                ));
            }
            if interactive {
                let (title, body) = edit_in_editor(&current_note(client, &edit.id)?)?;
                request.title = Some(title);
                request.body = Some(body);
            }
            let response = client.unary(v1::backlog_service::UPDATE_NOTE, &request)?;
            note_output(response.note, json)
        }
        Command::BacklogRemove { id } => {
            let response = client.unary(
                v1::backlog_service::DELETE_NOTE,
                &v1::DeleteNoteRequest { id: id.clone() },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("deleted\t{id}\n"))
            }
        }
        Command::BacklogPin { id, pinned } => update_note(
            client,
            v1::UpdateNoteRequest {
                id: id.clone(),
                pinned: Some(*pinned),
                ..Default::default()
            },
            json,
        ),
        Command::BacklogDone { id, resolution } => update_note(
            client,
            v1::UpdateNoteRequest {
                id: id.clone(),
                state: Some("done".into()),
                resolution: resolution.clone(),
                ..Default::default()
            },
            json,
        ),
        Command::BacklogReopen { id } => update_note(
            client,
            v1::UpdateNoteRequest {
                id: id.clone(),
                state: Some("open".into()),
                ..Default::default()
            },
            json,
        ),
        Command::LeaseList => {
            let leases = client.unary(v1::lease_service::LIST_LEASES, &v1::ListLeasesRequest {})?;
            if json {
                json_line(&leases)
            } else {
                Ok(format::lease_list_text(&leases, now_millis()))
            }
        }
        Command::LeaseAcquire(spec) => {
            use v1::acquire_lease_request::Target;
            let target = if spec.resource.is_empty() {
                Target::Selector(v1::NodeSelector {
                    os: spec.os.clone(),
                    arch: spec.arch.clone(),
                    min_free_memory_bytes: spec.min_free_memory_bytes,
                    gpu_vram_bytes: spec.gpu_vram_bytes,
                    gpu: spec.gpu.clone(),
                    npu: spec.npu.clone(),
                })
            } else {
                Target::Resource(spec.resource.clone())
            };
            let lease = client.unary(
                v1::lease_service::ACQUIRE_LEASE,
                &v1::AcquireLeaseRequest {
                    target: Some(target),
                    holder: spec.holder.clone(),
                    ttl: Some(proto_duration(spec.ttl)),
                    wait: spec.wait,
                    priority: spec.priority,
                },
            )?;
            if json {
                json_line(&lease)
            } else {
                Ok(format::lease_text(&lease))
            }
        }
        Command::LeaseRenew { id, ttl } => {
            let lease = client.unary(
                v1::lease_service::RENEW_LEASE,
                &v1::RenewLeaseRequest {
                    id: id.clone(),
                    ttl: ttl.map(proto_duration),
                },
            )?;
            if json {
                json_line(&lease)
            } else {
                Ok(format::lease_text(&lease))
            }
        }
        Command::LeaseRelease { ids } => {
            let mut output = String::new();
            let mut failures = Vec::new();
            for id in ids {
                let request = v1::ReleaseLeaseRequest { id: id.clone() };
                match client.unary(v1::lease_service::RELEASE_LEASE, &request) {
                    Ok(reply) => {
                        if json {
                            output.push_str(&json_line(&reply)?);
                        } else {
                            output.push_str(&format!("released {id}\n"));
                        }
                    }
                    Err(error) => failures.push((id, error)),
                }
            }
            if failures.is_empty() {
                Ok(output)
            } else {
                let code = failures[0].1.code;
                let message = failures
                    .into_iter()
                    .map(|(id, error)| format!("{id}: {}", error.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(RpcError::new(code, message))
            }
        }
        Command::JobList(filter) => {
            let request = v1::ListJobsRequest {
                state: filter.state.map_or(0, |state| state.as_proto() as i32),
                limit: filter.limit,
            };
            let jobs = client.unary(v1::job_service::LIST_JOBS, &request)?;
            if json {
                json_lines(&jobs.jobs)
            } else {
                Ok(format::job_list_text(&jobs, now_millis()))
            }
        }
        Command::JobShow { id } => {
            let job = client.unary(
                v1::job_service::GET_JOB,
                &v1::GetJobRequest { id: id.clone() },
            )?;
            if json {
                json_line(&job)
            } else {
                Ok(format::job_text(&job))
            }
        }
        Command::ChatSend {
            sleeve,
            text,
            request_id,
        } => {
            let text = match text.as_str() {
                "-" => read_stdin()?,
                _ => text.clone(),
            };
            let response = client.unary(
                v1::chat_service::SEND,
                &v1::SendRequest {
                    sleeve: sleeve.clone(),
                    text,
                    request_id: request_id.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format::chat_send_text(&response))
            }
        }
        Command::ChatWait {
            sleeves,
            until,
            timeout,
        } => {
            let sleeve = sleeves.first().ok_or_else(|| {
                RpcError::new(Code::InvalidArgument, "chat wait requires a sleeve")
            })?;
            let response = client.unary(
                v1::chat_service::WAIT,
                &wait_request(sleeve, *until, *timeout),
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format::chat_wait_text(&response))
            }
        }
        Command::ChatRespond {
            sleeve,
            approval_id,
            allow,
            note,
        } => {
            let response = client.unary(
                v1::chat_service::RESPOND,
                &v1::RespondRequest {
                    sleeve: sleeve.clone(),
                    approval_id: approval_id.clone(),
                    allow: *allow,
                    note: note.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok("answered\n".into())
            }
        }
        Command::ChatInterrupt { sleeve } => {
            let response = client.unary(
                v1::chat_service::INTERRUPT,
                &v1::InterruptRequest {
                    sleeve: sleeve.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok("interrupted\n".into())
            }
        }
        Command::ChatCompact { sleeve } => {
            let response = client.unary(
                v1::chat_service::COMPACT,
                &v1::CompactRequest {
                    sleeve: sleeve.clone(),
                },
            )?;
            if json {
                json_line(&response)
            } else {
                let compaction = response.compaction.unwrap_or_default();
                Ok(format!(
                    "compacted\t{} -> {} tokens\n",
                    compaction.tokens_before, compaction.tokens_after
                ))
            }
        }
        Command::ChatStatus { sleeve } => {
            let chat = require_sleeve(&get_sleeve(client, sleeve)?.sleeve)?
                .chat
                .clone()
                .ok_or_else(|| RpcError::new(Code::FailedPrecondition, "sleeve has no chat"))?;
            if json {
                json_line(&chat)
            } else {
                Ok(format::chat_status_text(&chat))
            }
        }
        Command::ChatModels { sleeve, harness } => {
            let models = chat_models(client, sleeve.as_deref(), harness)?;
            if json {
                json_line(&models)
            } else if !models.supported {
                Ok(format!("{}\n", models.reason))
            } else {
                Ok(models
                    .models
                    .iter()
                    .map(|model| {
                        format!(
                            "{}\t{}\tefforts=[{}]\tdefault effort={}\n",
                            model.model,
                            model.name,
                            model.efforts.join(" "),
                            model.default_effort
                        )
                    })
                    .collect())
            }
        }
        Command::ChatSettings(settings) => chat_settings(client, settings, json),
        Command::Report {
            status,
            summary_file,
        } => {
            let summary = read_report_summary(summary_file.as_deref())?;
            let report_status = v1::ReportStatus::from_str_name(&format!(
                "REPORT_STATUS_{}",
                status.to_ascii_uppercase()
            ))
            .ok_or_else(|| RpcError::invalid("--status must be complete, partial or blocked"))?;
            let response = client.unary(
                v1::chat_service::REPORT,
                &v1::ReportRequest {
                    status: report_status as i32,
                    summary,
                    source_seq: next_report_seq(),
                    ..Default::default()
                },
            )?;
            if json {
                json_line(&response)
            } else if response.dropped {
                Ok("dropped: a newer report is already recorded\n".into())
            } else {
                Ok(format!(
                    "reported {status} at chat seq {}\n",
                    response.result.map_or(0, |result| result.chat_seq)
                ))
            }
        }
        Command::JobCancel { id } => {
            let job = client.unary(
                v1::job_service::CANCEL_JOB,
                &v1::CancelJobRequest {
                    id: id.clone(),
                    ..Default::default()
                },
            )?;
            if json {
                json_line(&job)
            } else {
                Ok(format::job_cancel_text(&job))
            }
        }
        Command::SleeveList => {
            let sleeves =
                client.unary(v1::sleeve_service::LIST_SLEEVES, &v1::ListSleevesRequest {})?;
            if json {
                json_lines(&sleeves.sleeves)
            } else {
                Ok(format::sleeve_list_text(&sleeves))
            }
        }
        Command::SleeveWorkspaces => {
            let response = client.unary(
                v1::sleeve_service::LIST_WORKSPACES,
                &v1::ListWorkspacesRequest {},
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format::sleeve_workspaces_text(&response))
            }
        }
        Command::SleeveBrowse { path } => {
            let response = client.unary(
                v1::sleeve_service::BROWSE_WORKSPACES,
                &v1::BrowseWorkspacesRequest { path: path.clone() },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format::sleeve_browse_text(&response))
            }
        }
        Command::SleeveShow { name } => {
            let response = get_sleeve(client, name)?;
            let sleeve = require_sleeve(&response.sleeve)?;
            if json {
                json_line(&response)
            } else {
                let tail = exited_pane_tail(client, sleeve);
                Ok(format::sleeve_text(sleeve, tail.as_deref()))
            }
        }
        Command::SleeveStatus { name, explain } => {
            let response = client.unary(
                v1::sleeve_service::GET_TERMINAL_STATUS,
                &v1::GetTerminalStatusRequest { name: name.clone() },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format::terminal_status_text(&response, *explain))
            }
        }
        Command::SleeveKill { name, drop_caches } => {
            let response = client.unary(
                v1::sleeve_service::KILL_SLEEVE,
                &v1::KillSleeveRequest {
                    name: name.clone(),
                    drop_caches: *drop_caches,
                },
            )?;
            if json {
                json_line(&response)
            } else {
                Ok(format!("killed\t{name}\n"))
            }
        }
        Command::SleeveCheckpoint { name, action } => checkpoint(client, name, *action, json),
        Command::SleeveSpawn(spawn) => spawn_sleeve(client, spawn, json),
        Command::SleeveEnvoy(envoy) => spawn_envoy(client, envoy, json),
        Command::SleeveResleeve(resleeve) => resleeve_sleeve(client, resleeve, json),
        Command::SleevePrune(prune) => prune_sleeves(client, prune, json),
        Command::SessionList => {
            let sessions = client.call(LIST_SESSIONS, &json!({}))?;
            if json {
                value_line(&sessions)
            } else {
                Ok(format::session_list_text(&sessions))
            }
        }
        Command::SessionNew(spec) => {
            let response = client.call(CREATE_SESSION, &session_new_body(spec))?;
            if json {
                value_line(&response)
            } else {
                Ok(format!(
                    "created\t{}\n",
                    response["session"]["name"].as_str().unwrap_or(&spec.name)
                ))
            }
        }
        Command::SessionKill { name } => {
            let response = client.call(KILL_SESSION, &json!({"name": name}))?;
            if json {
                value_line(&response)
            } else {
                Ok(format!("killed\t{name}\n"))
            }
        }
        Command::SessionRespawn { name } => {
            let response = client.call(RESPAWN_SESSION, &json!({"name": name}))?;
            if json {
                value_line(&response)
            } else {
                Ok(format!(
                    "respawned\t{}\n",
                    response["session"]["name"].as_str().unwrap_or(name)
                ))
            }
        }
        Command::SessionCapture { name, lines } => {
            let pane = client.call(CAPTURE_PANE, &json!({"name": name, "lines": lines}))?;
            if json {
                value_line(&pane)
            } else {
                Ok(pane
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned())
            }
        }
        Command::MsgSend {
            to,
            body,
            thread,
            kind,
            ack,
            request_id,
        } => {
            let body = match body.as_str() {
                "-" => read_stdin()?,
                _ => body.clone(),
            };
            let mut request = json!({
                "to": to,
                "body": body,
                "threadId": thread,
                "kind": kind.as_proto(),
                "requestId": request_id,
            });
            if let Some(ack) = ack {
                request["ackRequired"] = json!(ack);
            }
            let response = client.call(MSG_SEND, &request)?;
            if json {
                value_line(&response)
            } else {
                Ok(format::msg_send_text(&response))
            }
        }
        Command::MsgInbox {
            unread,
            peek,
            act_as,
            thread,
        } => {
            let response = client.call(
                MSG_INBOX,
                &json!({
                    "unreadOnly": unread,
                    "peek": peek,
                    "as": act_as,
                    "threadId": thread,
                }),
            )?;
            mail_list_output(&response, json)
        }
        Command::MsgReply {
            id,
            body,
            request_id,
        } => {
            let body = match body.as_str() {
                "-" => read_stdin()?,
                _ => body.clone(),
            };
            let response = client.call(
                MSG_REPLY,
                &json!({"id": id, "body": body, "requestId": request_id}),
            )?;
            sent_message_output(&response, json, format::msg_reply_text)
        }
        Command::MsgAck { id, act_as } => {
            let response = client.call(MSG_ACK, &json!({"id": id, "as": act_as}))?;
            sent_message_output(&response, json, format::msg_ack_text)
        }
        Command::MsgThread { thread } => {
            let response = client.call(MSG_THREAD, &json!({"threadId": thread}))?;
            mail_list_output(&response, json)
        }
        Command::MsgThreads {
            sleeve,
            project,
            limit,
        } => {
            let response = client.call(
                MSG_THREADS,
                &json!({"sleeve": sleeve, "project": project, "limit": limit}),
            )?;
            if json {
                Ok(format::items(&response, "threads")
                    .iter()
                    .map(value_line)
                    .collect::<Result<Vec<_>>>()?
                    .concat())
            } else {
                Ok(format::thread_list_text(&response))
            }
        }
        Command::MsgWait { target, timeout } => {
            let response = client.call(
                MSG_WAIT,
                &json!({"target": target, "timeout": format_duration(*timeout)}),
            )?;
            if json {
                value_line(&response)
            } else {
                Ok(format::msg_wait_text(&response))
            }
        }
    }
}

/// What `msg inbox` and `msg thread` print.
fn mail_list_output(response: &Value, json: bool) -> Result<String> {
    if json {
        value_line(response)
    } else {
        Ok(format::mail_text(&format::items(response, "messages")))
    }
}

/// What `msg reply` and `msg ack` print: the message the hub just sent or
/// changed.
fn sent_message_output(
    response: &Value,
    json: bool,
    human: impl Fn(&Value) -> String,
) -> Result<String> {
    let message = response
        .get("message")
        .filter(|value| !value.is_null())
        .ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no message"))?;
    if json {
        value_line(message)
    } else {
        Ok(human(message))
    }
}

/// The tail of an exited sleeve's kept session, read through
/// SessionService.CapturePane. A session that is gone, or a hub that
/// refuses the capture, just means there is no tail to show.
fn exited_pane_tail(client: &Client, sleeve: &v1::Sleeve) -> Option<String> {
    if sleeve.state() != v1::SleeveState::Exited || sleeve.session.is_empty() {
        return None;
    }
    let request = v1::CapturePaneRequest {
        name: sleeve.session.clone(),
        lines: 100,
    };
    let pane = client
        .unary(v1::session_service::CAPTURE_PANE, &request)
        .ok()?;
    let tail = format::pane_tail(&pane.text, format::SHOW_TAIL_LINES);
    (!tail.is_empty()).then_some(tail)
}

/// Builds the client for an invocation: the hub URL from `--hub` or
/// `PROTECTORATE_HUB`, the bearer token from `--token-file`,
/// `PROTECTORATE_TOKEN_FILE` or `PROTECTORATE_TOKEN`, in that order.
pub fn client_for(invocation: &Invocation) -> Result<Client> {
    Ok(
        Client::new(&hub_url(invocation), "", transport_timeout(invocation))?
            .with_credential(credential(invocation)?),
    )
}

/// The HTTP timeout for an invocation. A `chat wait` blocks on hub events for
/// as long as its own deadline, so the transport has to outlast it.
fn transport_timeout(invocation: &Invocation) -> Duration {
    match &invocation.command {
        Command::ChatWait { timeout, .. } => {
            let wait = if timeout.is_zero() {
                WAIT_UNBOUNDED
            } else {
                timeout.saturating_add(WAIT_SLACK)
            };
            wait.max(invocation.timeout)
        }
        Command::ChatWatch { .. } | Command::Events => invocation.timeout.max(WAIT_UNBOUNDED),
        Command::ChatCompact { .. } => invocation.timeout.max(Duration::from_secs(600)),
        Command::ChatSettings(settings) if settings.writes() => {
            invocation.timeout.max(SELECTION_TIMEOUT_FLOOR)
        }
        Command::NodeGet { .. } | Command::NodePut { .. } => {
            invocation.timeout.max(Duration::from_secs(3600))
        }
        Command::NodeRemove {
            jobs, wait_timeout, ..
        } => {
            let wait = if *jobs == v1::RemoveJobs::Wait {
                wait_timeout.unwrap_or(Duration::from_secs(3600))
            } else {
                Duration::from_secs(120)
            };
            invocation.timeout.max(wait.saturating_add(WAIT_SLACK))
        }
        Command::NodeUpdate { .. } => invocation.timeout.max(UPDATE_TRANSFER_TIMEOUT),
        Command::HubUpdate { .. } => invocation.timeout.max(UPDATE_TRANSFER_TIMEOUT),
        Command::SleeveSpawn(_) | Command::SleeveEnvoy(_) => {
            invocation.timeout.max(SPAWN_TIMEOUT_FLOOR)
        }
        Command::SleeveResleeve(resleeve) => {
            let hub_wait = if resleeve.wait.is_zero() {
                DEFAULT_RESLEEVE_WAIT
            } else {
                resleeve.wait
            };
            invocation
                .timeout
                .max(hub_wait.saturating_add(RESLEEVE_AFTER_WAIT))
        }
        Command::MsgWait { timeout, .. } => {
            let wait = if timeout.is_zero() {
                DEFAULT_MSG_WAIT
            } else {
                *timeout
            };
            wait.saturating_add(WAIT_SLACK).max(invocation.timeout)
        }
        // WatchJob stays open for the whole run. The transport has to outlast
        // the job plus the follow window used when the stream drops.
        Command::JobRun(spec) | Command::JobTestCross(spec) => (if spec.timeout.is_zero() {
            DEFAULT_JOB_TIMEOUT
        } else {
            spec.timeout
        })
        .saturating_add(job::FOLLOW_WINDOW)
        .saturating_add(Duration::from_secs(60))
        .max(invocation.timeout),
        Command::JobLogs(JobLogs { follow: true, .. }) => MAX_JOB_TIMEOUT
            .saturating_add(job::FOLLOW_WINDOW)
            .max(invocation.timeout),
        _ => invocation.timeout,
    }
}

/// A google.protobuf.Duration in protobuf JSON. The hub reads it as seconds.
pub(crate) fn format_duration(timeout: Duration) -> String {
    let micros = timeout.as_micros();
    let seconds = micros / 1_000_000;
    let fraction = micros % 1_000_000;
    if fraction == 0 {
        return format!("{seconds}s");
    }
    let fraction = fraction.to_string().trim_end_matches('0').to_owned();
    format!("{seconds}.{fraction}s")
}

fn read_stdin() -> Result<String> {
    use std::io::Read;
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| RpcError::new(Code::Internal, format!("read stdin: {error}")))?;
    Ok(text)
}

fn edit_in_editor(note: &v1::Note) -> Result<(String, String)> {
    use std::io::Write;
    let mut nonce = [0_u8; 8];
    getrandom::getrandom(&mut nonce)
        .map_err(|error| RpcError::new(Code::Internal, format!("editor temp name: {error}")))?;
    let suffix = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let path = env::temp_dir().join(format!(
        "protectorate-note-{}-{suffix}.md",
        std::process::id()
    ));
    struct RemoveTemp(PathBuf);
    impl Drop for RemoveTemp {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| RpcError::new(Code::Internal, format!("create editor file: {error}")))?;
    let _cleanup = RemoveTemp(path.clone());
    file.write_all(format!("{}\n\n{}", note.title, note.body).as_bytes())
        .map_err(|error| RpcError::new(Code::Internal, format!("write editor file: {error}")))?;
    drop(file);
    let editor = env::var("EDITOR")
        .ok()
        .filter(|editor| !editor.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".into()
            } else {
                "vi".into()
            }
        });
    let mut argv = editor.split_whitespace();
    let program = argv
        .next()
        .ok_or_else(|| RpcError::invalid("EDITOR is empty"))?;
    let status = std::process::Command::new(program)
        .args(argv)
        .arg(&path)
        .status()
        .map_err(|error| {
            RpcError::new(Code::Internal, format!("run editor {program:?}: {error}"))
        })?;
    if !status.success() {
        return Err(RpcError::new(
            Code::Aborted,
            format!("editor {program:?} exited with {status}"),
        ));
    }
    let edited = fs::read_to_string(&path)
        .map_err(|error| RpcError::new(Code::Internal, format!("read edited note: {error}")))?;
    let edited = edited.replace("\r\n", "\n");
    let (title, body) = edited.split_once('\n').unwrap_or((&edited, ""));
    Ok((
        title.trim().to_owned(),
        body.strip_prefix('\n').unwrap_or(body).to_owned(),
    ))
}

/// Parses `args`, resolves the hub endpoint and token from the flags and the
/// environment, and runs the command. `job run` writes the log as it arrives
/// and returns [`RunError::Status`] with the command's exit status.
pub fn run_with_status(
    args: impl IntoIterator<Item = impl AsRef<str>>,
) -> std::result::Result<String, RunError> {
    let invocation = parse(args)?;
    match &invocation.command {
        Command::Help => return Ok(usage().to_string()),
        Command::HelpPath(path) => return command_usage(path).map_err(RunError::from),
        _ => {}
    }
    if invocation.command == Command::Version {
        return Ok(version_text(invocation.json));
    }
    if let Command::NodeMetricsLocal { watch } = &invocation.command {
        return local_metrics(watch, invocation.json).map_err(RunError::from);
    }
    // Device login has no bearer token yet: it runs on its own public client
    // and writes its progress straight out so the code is visible while waiting.
    if matches!(invocation.command, Command::DeviceLogin { .. }) {
        let mut stdout = std::io::stdout();
        return login::run(&invocation, &mut stdout).map_err(RunError::from);
    }
    if let Command::Pair { url, app } = &invocation.command {
        return pair::run(&invocation, url.as_deref(), *app).map_err(RunError::from);
    }
    if let Command::TokenAttenuate { scopes, ttl } = &invocation.command {
        return attenuate(&invocation, scopes, *ttl).map_err(RunError::from);
    }
    let client = client_for(&invocation)?;
    if streams(&invocation.command) {
        let cancel = cancel_client(&invocation)?;
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        return render(
            &client,
            &invocation,
            &mut stdout,
            &mut stderr,
            cancel.as_ref(),
            true,
        );
    }
    execute(&client, &invocation).map_err(RunError::from)
}

fn local_metrics(watch: &[PathBuf], json: bool) -> Result<String> {
    use protectorate_node_metrics::{metrics_json, Collector, Options};
    let options = Options {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        watch_paths: watch.to_vec(),
        ..Options::default()
    };
    #[cfg(windows)]
    let snapshot = Collector::windows(options).collect();
    #[cfg(not(windows))]
    let snapshot = Collector::linux(options).collect();
    let snapshot = snapshot.map_err(|error| {
        RpcError::new(Code::Internal, format!("collect local metrics: {error}"))
    })?;
    let encoded = serde_json::to_vec(&metrics_json(&snapshot))
        .map_err(|error| RpcError::new(Code::Internal, format!("encode local metrics: {error}")))?;
    let metrics: v1::NodeMetrics = protectorate_wire::decode(&encoded)?;
    if json {
        json_line(&metrics)
    } else {
        Ok(format::local_metrics_text(&metrics))
    }
}

fn streams(command: &Command) -> bool {
    matches!(command, Command::ChatWait { sleeves, .. } if sleeves.len() > 1)
        || matches!(
            command,
            Command::JobRun(_)
                | Command::JobTestCross(_)
                | Command::JobLogs(_)
                | Command::ChatWatch { .. }
                | Command::Events
                | Command::SleeveAttach { .. }
                | Command::SessionAttach { .. }
                | Command::NodeUpdate { .. }
                | Command::HubUpdate { .. }
        )
}

/// A short-timeout client for CancelJob, so a dropped watch does not hold the
/// only connection the cancel needs.
fn cancel_client(invocation: &Invocation) -> Result<Option<Client>> {
    if !matches!(
        invocation.command,
        Command::JobRun(_) | Command::JobTestCross(_)
    ) {
        return Ok(None);
    }
    Ok(Some(
        Client::new(&hub_url(invocation), "", Duration::from_secs(20))?
            .with_credential(credential(invocation)?),
    ))
}

/// What `api call` prints: the hub's reply as one compact proto-JSON line
/// with `--json`, or as two-space indented JSON without it.
fn api_call(client: &Client, procedure: &str, body: &str, json: bool) -> Result<String> {
    let input = serde_json::from_str(body)
        .map_err(|error| RpcError::invalid(format!("api call request JSON: {error}")))?;
    let response = client.call(procedure, &input)?;
    let text = if json {
        serde_json::to_string(&response)
    } else {
        serde_json::to_string_pretty(&response)
    }
    .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))?;
    Ok(format!("{text}\n"))
}

/// The client build version: the release tag stamped at build time, or this
/// crate's version for a local build.
fn client_version() -> &'static str {
    option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// This machine's platform as releases name it (`linux/amd64`).
fn client_platform() -> String {
    format!("{}/{}", std::env::consts::OS, host_arch())
}

/// What `version` prints: `protectorate 0.1.0 linux/amd64`, or one JSON
/// object holding the same three fields.
fn version_text(json: bool) -> String {
    let version = client_version();
    if json {
        return format!(
            "{}\n",
            json!({"version": version, "os": std::env::consts::OS, "arch": host_arch()})
        );
    }
    format!("protectorate {version} {}\n", client_platform())
}

pub(crate) fn hub_url(invocation: &Invocation) -> String {
    invocation
        .hub
        .clone()
        .or_else(|| {
            env::var(HUB_URL_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_HUB.to_owned())
}

/// The credential for an invocation: the capability file from --token-file
/// or the environment, a token in the environment, then the device key saved
/// by `login` for this hub.
fn credential(invocation: &Invocation) -> Result<Credential> {
    let file = invocation.token_file.clone().or_else(|| {
        env::var(TOKEN_FILE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    });
    if let Some(file) = file {
        let raw = fs::read_to_string(&file)
            .map_err(|error| RpcError::new(Code::Internal, format!("read token file: {error}")))?;
        if raw.trim().is_empty() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                format!("token file {} is empty", file.display()),
            ));
        }
        return Ok(Credential::TokenFile(file));
    }
    if let Some(token) = env::var(TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        return Ok(Credential::Bearer(token.into()));
    }
    match login::saved_device(invocation)? {
        Some(key) => Ok(Credential::Device(key)),
        None => Err(RpcError::new(
            Code::Unauthenticated,
            format!(
                "no credential: set --token-file, {TOKEN_FILE_ENV} or {TOKEN_ENV}, or run login --hub {}",
                hub_url(invocation)
            ),
        )),
    }
}

/// The capability or token an offline command such as `token attenuate`
/// works on, from the same places as `credential`.
fn presented_token(invocation: &Invocation) -> Result<zeroize::Zeroizing<String>> {
    match credential(invocation) {
        Ok(Credential::TokenFile(file)) => fs::read_to_string(&file)
            .map(|raw| raw.trim().to_owned().into())
            .map_err(|error| RpcError::new(Code::Internal, format!("read token file: {error}"))),
        Ok(Credential::Bearer(token)) => Ok(token),
        Ok(_) => Err(RpcError::invalid(
            "a device key is not a capability; pass --token-file",
        )),
        Err(error) => Err(error),
    }
}

/// `token attenuate`: append caveats to the presented capability. Only the
/// hub can verify the result; it never grants more than the original.
fn attenuate(invocation: &Invocation, scopes: &[String], ttl: Option<Duration>) -> Result<String> {
    use protectorate_auth::capability::{self, Caveat};
    let mut token = presented_token(invocation)?.to_string();
    let bad = |error: capability::Error| RpcError::invalid(error.to_string());
    if !scopes.is_empty() {
        token = capability::attenuate(&token, &Caveat::Scopes(scopes.to_vec())).map_err(bad)?;
    }
    if let Some(ttl) = ttl {
        let at = chrono::Utc::now()
            .timestamp()
            .saturating_add(ttl.as_secs() as i64);
        token = capability::attenuate(&token, &Caveat::Expires(at)).map_err(bad)?;
    }
    Ok(format!("{token}\n"))
}

/// How the MCP bridge child authenticates, as the environment variable and
/// value the CLI sets for it: the same credential as other commands, then the
/// private owner token created by the Rust hub installer. The bridge never
/// writes credentials to the MCP protocol stream.
pub fn mcp_credential(
    hub: Option<&str>,
    token_file: Option<&Path>,
    home: Option<&Path>,
) -> Result<(&'static str, String)> {
    let invocation = Invocation {
        command: Command::Help,
        json: false,
        hub: hub.map(str::to_owned),
        token_file: token_file.map(Path::to_path_buf),
        home: home.map(Path::to_path_buf),
        timeout: DEFAULT_TIMEOUT,
    };
    match credential(&invocation) {
        Ok(Credential::TokenFile(file)) => Ok((TOKEN_FILE_ENV, file.display().to_string())),
        Ok(Credential::Bearer(token)) => Ok((TOKEN_ENV, token.to_string())),
        Ok(_) => Ok((
            DEVICE_CREDENTIAL_ENV,
            login::credential_file(&invocation)?.display().to_string(),
        )),
        Err(error) if error.code == Code::Unauthenticated => {
            let home = login::state_home(&invocation)?;
            private_owner_token(&home.join("token"))?
                .map(|token| (TOKEN_ENV, token))
                .ok_or(error)
        }
        Err(error) => Err(error),
    }
}

/// Read the installer's local owner token only when it is a bounded private
/// regular file. On Unix the opened descriptor itself is checked and symlink
/// traversal of the final path component is refused.
fn private_owner_token(path: &Path) -> Result<Option<String>> {
    #[cfg(unix)]
    let raw = {
        use std::io::Read;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut options = fs::OpenOptions::new();
        let file = match options
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    format!("read local hub token {}: {error}", path.display()),
                ))
            }
        };
        let metadata = file.metadata().map_err(|error| {
            RpcError::new(
                Code::Internal,
                format!("inspect local hub token {}: {error}", path.display()),
            )
        })?;
        if !metadata.is_file()
            || metadata.len() > 64 * 1024
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(RpcError::new(
                Code::PermissionDenied,
                format!(
                    "local hub token {} must be a private regular file under 64 KiB",
                    path.display()
                ),
            ));
        }
        let mut raw = String::new();
        file.take(64 * 1024 + 1)
            .read_to_string(&mut raw)
            .map_err(|error| {
                RpcError::new(
                    Code::Internal,
                    format!("read local hub token {}: {error}", path.display()),
                )
            })?;
        raw
    };
    #[cfg(windows)]
    let raw = match login::read_private_token_file(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RpcError::new(
                Code::PermissionDenied,
                format!("read local hub token {}: {error}", path.display()),
            ))
        }
    };
    #[cfg(not(any(unix, windows)))]
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RpcError::new(
                Code::Internal,
                format!("read local hub token {}: {error}", path.display()),
            ))
        }
    };
    if raw.len() > 64 * 1024 {
        return Err(RpcError::new(
            Code::PermissionDenied,
            format!("local hub token {} exceeds 64 KiB", path.display()),
        ));
    }
    let token = raw.trim();
    if token.is_empty() {
        return Err(RpcError::invalid(format!(
            "local hub token {} is empty",
            path.display()
        )));
    }
    Ok(Some(token.to_owned()))
}

/// What `backlog add`, `done` and `reopen` print: the note the hub just
/// created or changed, as one text row or one proto-JSON line.
fn note_output(note: Option<v1::Note>, json: bool) -> Result<String> {
    let note = returned_note(note)?;
    if json {
        json_line(&note)
    } else {
        Ok(format::note_record_text(&note))
    }
}

fn returned_note(note: Option<v1::Note>) -> Result<v1::Note> {
    note.ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no note"))
}

fn update_note(client: &Client, request: v1::UpdateNoteRequest, json: bool) -> Result<String> {
    note_output(
        client
            .unary(v1::backlog_service::UPDATE_NOTE, &request)?
            .note,
        json,
    )
}

fn current_note(client: &Client, id: &str) -> Result<v1::Note> {
    let response = client.unary(
        v1::backlog_service::GET_NOTE,
        &v1::GetNoteRequest { id: id.to_owned() },
    )?;
    returned_note(response.note)
}

/// A note body from a file, or stdin for `-`.
fn read_body(path: &str) -> Result<String> {
    if path == "-" {
        return read_stdin();
    }
    fs::read_to_string(path)
        .map_err(|error| RpcError::new(Code::Internal, format!("read body: {error}")))
}

fn spawn_sleeve(client: &Client, spawn: &SleeveSpawn, json: bool) -> Result<String> {
    let brief = if spawn.brief_file.is_empty() {
        String::new()
    } else {
        let raw = fs::read_to_string(&spawn.brief_file)
            .map_err(|error| RpcError::new(Code::Internal, format!("read brief: {error}")))?;
        if raw.trim().is_empty() {
            return Err(RpcError::invalid("brief is empty"));
        }
        raw
    };
    let request = v1::SpawnSleeveRequest {
        name: spawn.name.clone(),
        workspace: absolute_workspace(&spawn.workspace)?,
        harness: spawn.harness.as_proto() as i32,
        args: spawn.args.clone(),
        limits: (spawn.cpu_percent > 0 || spawn.memory_bytes > 0 || spawn.disk_bytes > 0).then(
            || v1::SleeveLimits {
                cpu_percent: spawn.cpu_percent,
                memory_bytes: spawn.memory_bytes,
                disk_bytes: spawn.disk_bytes,
            },
        ),
        env: spawn.env.iter().cloned().collect(),
        mode: spawn.mode.as_proto() as i32,
        supervised: spawn.supervised,
        selection: selection(&spawn.selection),
        read_only: spawn
            .read_only
            .iter()
            .map(|mount| v1::ReadOnlyMount {
                source: mount.source.clone(),
                target: mount.target.clone(),
            })
            .collect(),
        exit_after_idle: (!spawn.exit_after_idle.is_zero())
            .then(|| proto_duration(spawn.exit_after_idle)),
        branch: spawn.branch.clone(),
        new_branch: spawn.new_branch.clone(),
        brief,
        nested: spawn.nested,
        token_budget: spawn.token_budget,
    };
    let response = client.unary(v1::sleeve_service::SPAWN_SLEEVE, &request)?;
    if json {
        json_line(&response)
    } else {
        Ok(format::sleeve_spawn_text(require_sleeve(&response.sleeve)?))
    }
}

/// `orchestrator status`: Envoy's sleeve and its newest token, which the
/// JSON form returns as one object with either field absent.
fn orchestrator_status(client: &Client, json: bool) -> Result<String> {
    let sleeve = match get_sleeve(client, "envoy") {
        Ok(response) => response.sleeve,
        Err(error) if error.code == Code::NotFound => None,
        Err(error) => return Err(error),
    };
    let listing = client.unary(v1::token_service::LIST_TOKENS, &v1::ListTokensRequest {})?;
    let token = listing
        .tokens
        .into_iter()
        .filter(|token| token.name == "orchestrator-envoy")
        .max_by_key(|token| format::unix_millis(token.created_at.as_ref()).unwrap_or_default());
    if !json {
        return Ok(format::orchestrator_status_text(
            sleeve.as_ref(),
            token.as_ref(),
            now_millis(),
        ));
    }
    let mut status = serde_json::Map::new();
    if let Some(sleeve) = &sleeve {
        status.insert("sleeve".into(), message_value(sleeve)?);
    }
    if let Some(token) = &token {
        status.insert("token".into(), message_value(token)?);
    }
    value_line(&Value::Object(status))
}

fn spawn_envoy(client: &Client, envoy: &SleeveEnvoy, json: bool) -> Result<String> {
    let request = v1::SpawnEnvoyRequest {
        harness: envoy.harness.map_or(0, |harness| harness.as_proto() as i32),
        scopes: envoy.scopes.clone(),
        add_scopes: envoy.add_scopes.clone(),
        selection: selection(&envoy.selection),
        token_budget: envoy.token_budget,
    };
    let response = client.unary(v1::sleeve_service::SPAWN_ENVOY, &request)?;
    if json {
        json_line(&response)
    } else {
        require_sleeve(&response.sleeve)?;
        Ok(format::sleeve_envoy_text(&response))
    }
}

/// `chat models`. A harness without model listing is an answer, not a
/// failure: the hub says so with `supported` false, and a hub that does not
/// list models for a harness this CLI knows gets the same answer.
fn chat_models(
    client: &Client,
    sleeve: Option<&str>,
    harness: &str,
) -> Result<v1::ListModelsResponse> {
    let request = v1::ListModelsRequest {
        sleeve: sleeve.unwrap_or_default().to_owned(),
        harness: harness.to_owned(),
    };
    let unsupported = || v1::ListModelsResponse {
        reason: format!("model listing is not supported for {harness}"),
        ..Default::default()
    };
    match client.unary(v1::chat_service::LIST_MODELS, &request) {
        Ok(models) if !models.supported && models.reason.is_empty() => Ok(v1::ListModelsResponse {
            reason: unsupported().reason,
            ..models
        }),
        Ok(models) => Ok(models),
        Err(error)
            if sleeve.is_none()
                && HarnessName::parse(harness).is_some()
                && matches!(error.code, Code::Unimplemented | Code::InvalidArgument) =>
        {
            Ok(unsupported())
        }
        Err(error) => Err(error),
    }
}

/// `chat settings`: GetSleeve for the current settings, SetSelection when the
/// invocation carries a selection or --native-defaults.
fn chat_settings(client: &Client, settings: &ChatSettings, json: bool) -> Result<String> {
    let chat = if settings.writes() {
        let request = v1::SetSelectionRequest {
            sleeve: settings.sleeve.clone(),
            selection: Some(v1::ModelSelection {
                model: settings.model.clone().unwrap_or_default(),
                effort: settings.effort.clone().unwrap_or_default(),
            }),
        };
        client
            .unary(v1::chat_service::SET_SELECTION, &request)?
            .chat
            .ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no chat status"))?
    } else {
        let response = get_sleeve(client, &settings.sleeve)?;
        require_sleeve(&response.sleeve)?
            .chat
            .clone()
            .unwrap_or_default()
    };
    if json {
        json_line(&chat)
    } else {
        Ok(format::chat_settings_text(&chat))
    }
}

fn resleeve_sleeve(client: &Client, resleeve: &SleeveResleeve, json: bool) -> Result<String> {
    let request = v1::ResleeveSleeveRequest {
        name: resleeve.name.clone(),
        to: resleeve.to.map_or(0, |harness| harness.as_proto() as i32),
        interrupt: resleeve.interrupt,
        wait: (!resleeve.wait.is_zero()).then(|| proto_duration(resleeve.wait)),
        selection: selection(&resleeve.selection),
        native_defaults: resleeve.native_defaults,
    };
    let response = client.unary(v1::sleeve_service::RESLEEVE_SLEEVE, &request)?;
    if json {
        json_line(&response)
    } else {
        Ok(format::sleeve_resleeve_text(require_sleeve(
            &response.sleeve,
        )?))
    }
}

fn prune_sleeves(client: &Client, prune: &SleevePrune, json: bool) -> Result<String> {
    let request = v1::PruneSleevesRequest {
        yes: prune.yes,
        caches: prune.caches,
        homes: prune.homes,
        workspaces: prune.workspaces,
        running: prune.running,
        ignored: prune.ignored,
    };
    let response = client.unary(v1::sleeve_service::PRUNE_SLEEVES, &request)?;
    if json {
        json_line(&response)
    } else {
        Ok(format::sleeve_prune_text(&response, prune.yes))
    }
}

/// `sleeve checkpoint`: list, create, diff or restore workspace checkpoints.
fn checkpoint(client: &Client, name: &str, action: CheckpointAction, json: bool) -> Result<String> {
    use v1::sleeve_service::{
        CREATE_CHECKPOINT, DIFF_CHECKPOINT, LIST_CHECKPOINTS, RESTORE_CHECKPOINT,
    };
    let name = name.to_owned();
    match action {
        CheckpointAction::List => {
            let response = client.unary(LIST_CHECKPOINTS, &v1::ListCheckpointsRequest { name })?;
            if json {
                return json_line(&response);
            }
            Ok(response
                .checkpoints
                .iter()
                .map(format::checkpoint_line)
                .collect())
        }
        CheckpointAction::Create => {
            let response =
                client.unary(CREATE_CHECKPOINT, &v1::CreateCheckpointRequest { name })?;
            if json {
                return json_line(&response);
            }
            Ok(format::checkpoint_line(
                &response.checkpoint.unwrap_or_default(),
            ))
        }
        CheckpointAction::Diff { from, to } => {
            let request = v1::DiffCheckpointRequest {
                name,
                from_id: from,
                to_id: to,
            };
            let mut response = client.unary(DIFF_CHECKPOINT, &request)?;
            if json {
                return json_line(&response);
            }
            if response.truncated {
                response.diff.push_str("\n(diff truncated)\n");
            }
            Ok(response.diff)
        }
        CheckpointAction::Restore { id } => {
            let request = v1::RestoreCheckpointRequest {
                name: name.clone(),
                id,
            };
            let response = client.unary(RESTORE_CHECKPOINT, &request)?;
            if json {
                return json_line(&response);
            }
            Ok(format!(
                "restored\t{name}\tbefore\t{}",
                format::checkpoint_line(&response.before.unwrap_or_default())
            ))
        }
    }
}

pub(crate) fn get_sleeve(client: &Client, name: &str) -> Result<v1::GetSleeveResponse> {
    client.unary(
        v1::sleeve_service::GET_SLEEVE,
        &v1::GetSleeveRequest {
            name: name.to_owned(),
        },
    )
}

fn require_sleeve(sleeve: &Option<v1::Sleeve>) -> Result<&v1::Sleeve> {
    sleeve
        .as_ref()
        .ok_or_else(|| RpcError::new(Code::DataLoss, "hub returned no sleeve"))
}

/// CreateSessionRequest. Omitted fields stay unset, so the hub uses its own
/// defaults for cwd and the login shell.
fn session_new_body(spec: &SessionNew) -> Value {
    let mut request = json!({ "name": spec.name });
    if !spec.cwd.is_empty() {
        request["cwd"] = json!(spec.cwd);
    }
    if !spec.command.is_empty() {
        request["command"] = json!(spec.command);
    }
    if !spec.env.is_empty() {
        let mut env = serde_json::Map::new();
        for (key, value) in &spec.env {
            env.insert(key.clone(), json!(value));
        }
        request["env"] = Value::Object(env);
    }
    request
}

fn selection(selection: &ModelChoice) -> Option<v1::ModelSelection> {
    selection.is_set().then(|| v1::ModelSelection {
        model: selection.model.clone(),
        effort: selection.effort.clone(),
    })
}

/// A message as a JSON value, for output the CLI composes from several.
fn message_value<T: ReflectMessage>(message: &T) -> Result<Value> {
    serde_json::from_slice(&protectorate_wire::encode(message)?)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))
}

/// A disk path becomes absolute and cleaned. Unlike [`absolute_workspace`],
/// `@` is not treated as a URL: job sources are files on the hub.
pub(crate) fn absolute_disk_path(path: &str) -> Result<String> {
    let path = if path.is_empty() { "." } else { path };
    let path = std::path::Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| RpcError::new(Code::Internal, format!("current directory: {error}")))?
            .join(path)
    };
    Ok(normalize_path(&absolute))
}

/// A local path becomes absolute and cleaned. A git URL, recognized by
/// `://` or `@`, is left unchanged.
fn absolute_workspace(workspace: &str) -> Result<String> {
    if workspace.contains("://") || workspace.contains('@') {
        return Ok(workspace.to_owned());
    }
    let path = std::path::Path::new(workspace);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| RpcError::new(Code::Internal, format!("current directory: {error}")))?
            .join(path)
    };
    Ok(normalize_path(&absolute))
}

fn normalize_path(path: &std::path::Path) -> String {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// One message as a compact protobuf JSON line.
pub(crate) fn json_line<T: ReflectMessage>(message: &T) -> Result<String> {
    let mut line = protectorate_wire::encode(message)?;
    line.push(b'\n');
    String::from_utf8(line)
        .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))
}

/// One compact protobuf JSON line per message.
fn json_lines<'a, T: ReflectMessage + 'a>(
    messages: impl IntoIterator<Item = &'a T>,
) -> Result<String> {
    messages
        .into_iter()
        .map(json_line)
        .collect::<Result<Vec<_>>>()
        .map(|lines| lines.concat())
}

/// A JSON value the CLI composed itself, as one compact line.
pub(crate) fn value_line(value: &Value) -> Result<String> {
    serde_json::to_string(value)
        .map(|line| format!("{line}\n"))
        .map_err(|error| RpcError::new(Code::Internal, format!("encode JSON: {error}")))
}

/// `value`, or `fallback` when the hub left it empty.
fn or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
