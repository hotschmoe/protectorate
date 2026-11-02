//! The views the hub serves: one snapshot ([`HubState`]), the per-sleeve transcript and the
//! event-stream messages. Keys are snake_case, times are `*_ms` epoch milliseconds, sizes are
//! `*_bytes`, enums are lowercase strings.

use pt_json::Value;

use crate::schema::EpochMs;

api_enums! {
    pub enum Level {
        Info = "info",
        Warn = "warn",
        Alarm = "alarm",
    }

    pub enum SleeveRole {
        /// The orchestrator, an ordinary root sleeve in its own directory.
        Envoy = "envoy",
        Root = "root",
        Child = "child",
    }

    /// SPEC 4, sleeve lifecycle.
    pub enum Lifecycle {
        Provisioning = "provisioning",
        Launching = "launching",
        Running = "running",
        Recovering = "recovering",
        Resleeving = "resleeving",
        Stopping = "stopping",
        Stopped = "stopped",
        Parked = "parked",
        Removed = "removed",
    }

    /// SPEC 4, activity per incarnation.
    pub enum ActivityState {
        Booting = "booting",
        Idle = "idle",
        Delivering = "delivering",
        Working = "working",
        Blocked = "blocked",
        Interrupting = "interrupting",
        Compacting = "compacting",
        Unknown = "unknown",
        Ended = "ended",
    }

    /// The sleeve kind an incarnation runs (SPEC 3.4).
    pub enum Harness {
        Claude = "claude",
        Codex = "codex",
        Grok = "grok",
        /// Pi with a vendor login or API key from Pi's own catalog.
        PiVendor = "pi_vendor",
        /// Pi against a local OpenAI-compatible endpoint.
        PiLocal = "pi_local",
        /// Pane only (D13).
        Antigravity = "antigravity",
    }

    pub enum Mode {
        Headless = "headless",
        Pane = "pane",
    }

    /// What a child sleeve works in.
    pub enum WorkspaceView {
        /// Its own git worktree and branch (the default, D8).
        Worktree = "worktree",
        /// A read-only view of the root's directory.
        Readonly = "readonly",
    }

    /// Whose quota an incarnation draws on.
    pub enum Subscription {
        Claude = "claude",
        Codex = "codex",
        Grok = "grok",
        /// The local model: unmetered.
        Local = "local",
        /// A metered API key with no subscription window.
        Api = "api",
    }

    /// What a tool does, for the collapsed work-group summary.
    pub enum ToolKind {
        Command = "command",
        Read = "read",
        Edit = "edit",
        Search = "search",
        Web = "web",
        Mcp = "mcp",
        Subagent = "subagent",
        Other = "other",
    }

    pub enum ToolStatus {
        Completed = "completed",
        Failed = "failed",
        Declined = "declined",
        Cancelled = "cancelled",
    }

    /// An answer to an approval.
    pub enum Decision {
        AllowOnce = "allow_once",
        /// Tool approvals only: allow this tool for the rest of the session.
        AllowSession = "allow_session",
        Deny = "deny",
    }

    pub enum DecidedBy {
        Owner = "owner",
        Envoy = "envoy",
        /// A code rule.
        Rule = "rule",
        /// The sleeve runs unsupervised and the harness allowed it itself.
        Autonomous = "autonomous",
    }

    /// Why a turn settled. `other` keeps the harness's own word in `native_stop`.
    pub enum Stop {
        EndTurn = "end_turn",
        Cancelled = "cancelled",
        Error = "error",
        MaxTurns = "max_turns",
        Other = "other",
    }

    /// Read by guardrails: G1 quota and rate limits, G2 auth, G3 crash, G14 protocol.
    pub enum ErrorClass {
        Auth = "auth",
        Quota = "quota",
        RateLimit = "rate_limit",
        Protocol = "protocol",
        Crash = "crash",
        Other = "other",
    }

    pub enum AckKind {
        Confirmed = "confirmed",
        Rejected = "rejected",
    }

    pub enum ApprovalKind {
        /// A harness permission request.
        Tool = "tool",
        /// One a code rule requires (D9): Envoy removing sleeves, running node jobs, pushing or
        /// spawning a burst of sleeves during a turn an agent message started.
        Action = "action",
    }

    pub enum ApprovalState {
        Pending = "pending",
        Approved = "approved",
        Denied = "denied",
        Expired = "expired",
        /// The request went away first (turn interrupted, sleeve stopped).
        Cancelled = "cancelled",
        /// Decided, but the incarnation that asked was gone: nothing reached the harness.
        Fenced = "fenced",
        /// An action approval decided or timed out after the call waiting on it ended (a hub
        /// restart): nothing was done.
        Abandoned = "abandoned",
    }

    /// UNKNOWN counts as present: the node takes no jobs.
    pub enum PresenceState {
        Present = "present",
        Away = "away",
        Unknown = "unknown",
    }

    /// SPEC 4, node job.
    pub enum JobState {
        Queued = "queued",
        Submitting = "submitting",
        SubmitUncertain = "submit_uncertain",
        Running = "running",
        Collecting = "collecting",
        Succeeded = "succeeded",
        Failed = "failed",
        Interrupted = "interrupted",
        Lost = "lost",
        Cancelling = "cancelling",
        Cancelled = "cancelled",
        Abandoned = "abandoned",
    }

    /// What a running job does when a person arrives at the node.
    pub enum OnPresent {
        /// Drop to idle CPU and I/O priority and keep running.
        Idle = "idle",
        Kill = "kill",
    }

    pub enum QuotaReading {
        /// No reader for this subscription yet.
        Unknown = "unknown",
        Ok = "ok",
        /// The reader failed; the windows are the last good value.
        Stale = "stale",
        Error = "error",
    }

    pub enum EventKind {
        Mail = "mail",
        Sleeve = "sleeve",
        Approval = "approval",
        Job = "job",
        Node = "node",
        Guardrail = "guardrail",
        Hub = "hub",
    }
}

api_records! {
    pub struct HubInfo {
        pub name: String,
        pub version: String,
        pub started_at_ms: EpochMs,
        pub inventory: Option<Inventory>,
        pub sample: Option<Sample>,
        /// Set when a rollback restore failed and the trial release was kept.
        pub restore_error: Option<String>,
    }

    /// One full snapshot. `GET /api/events` starts with it and then streams changes.
    pub struct HubState {
        /// The event-stream seq this snapshot includes; resume with `from_seq = seq + 1`.
        pub seq: u64,
        pub hub: HubInfo,
        pub workspaces: Vec<Workspace> = Vec::new(),
        pub sleeves: Vec<Sleeve> = Vec::new(),
        pub approvals: Vec<Approval> = Vec::new(),
        pub nodes: Vec<Node> = Vec::new(),
        pub jobs: Vec<Job> = Vec::new(),
        pub quotas: Vec<Quota> = Vec::new(),
        pub events: Vec<HubEvent> = Vec::new(),
    }

    pub struct Workspace {
        pub id: String,
        pub name: String,
        pub path: String,
    }

    /// `Idle{seen}` and `Delivering{request_id}` carry their payload beside `state`.
    pub struct Activity {
        pub state: ActivityState,
        pub since_ms: EpochMs,
        /// Idle only: false until the owner has looked at the settled turn.
        pub seen: bool = true,
        /// Delivering only: the message being written to the harness.
        pub request_id: Option<String>,
    }

    /// One harness process tree with generation `g`.
    pub struct Incarnation {
        pub g: u64,
        pub mode: Mode,
        pub harness: Harness,
        pub subscription: Subscription,
        pub provider: String,
        pub model: String,
        pub effort: Option<String>,
        /// The harness's own session id.
        pub native_id: Option<String>,
        pub activity: Activity,
        pub started_at_ms: EpochMs,
        pub turn: u64 = 0,
        /// 0 when unknown.
        pub context_tokens: u64 = 0,
        /// 0 when unknown.
        pub context_window: u64 = 0,
        pub cache_ratio: Option<f64>,
    }

    /// The durable identity: name, workspace claim, parent link, resleeve policy, budget and
    /// queue.
    pub struct Sleeve {
        pub id: String,
        pub name: String,
        pub workspace_id: String,
        pub parent_id: Option<String>,
        pub role: SleeveRole,
        pub lifecycle: Lifecycle,
        /// Parked only: provision, no_target, launch, flapping, brief_unconfirmed or `g<N>`.
        /// A flap after a rejected login is `flapping (<harness> login rejected)`.
        pub park_reason: Option<String>,
        /// The directory the incarnation works in (a child's worktree, a root's claim).
        pub path: String = String::new(),
        /// The ordered automatic resleeve targets (default `[pi_local]`).
        pub resleeve_to: Vec<Harness> = Vec::new(),
        pub branch: Option<String>,
        /// Children only.
        pub view: Option<WorkspaceView>,
        pub incarnation: Option<Incarnation>,
        /// Messages waiting for the turn to settle.
        pub queued: u64 = 0,
        /// Tokens.
        pub budget_used: Option<u64>,
        pub budget_limit: Option<u64>,
    }

    /// Token counts the harness reported for one turn.
    pub struct Usage {
        pub input: u64 = 0,
        pub output: u64 = 0,
        pub cache_read: u64 = 0,
        pub cache_write: u64 = 0,
        pub cost_usd: Option<f64>,
        pub context_tokens: Option<u64>,
        pub context_window: Option<u64>,
    }

    pub struct FileDelta {
        pub path: String,
        pub op: Option<String>,
        pub added: Option<u64>,
        pub removed: Option<u64>,
    }

    /// One entry of a sleeve's append-only transcript. A status change is a new entry naming
    /// the item's `id`; the client folds them. Streaming deltas carry the seq their block's
    /// whole entry will have, and are not kept.
    pub struct ChatEntry {
        /// Monotonic per sleeve.
        pub seq: u64,
        pub g: u64,
        /// The hub's turn number.
        pub turn: u64 = 0,
        pub at_ms: EpochMs,
        pub entry: Entry,
    }

    pub struct Transcript {
        pub sleeve_id: String,
        pub entries: Vec<ChatEntry> = Vec::new(),
        /// More entries exist after the last one returned.
        pub more: bool = false,
    }

    pub struct Approval {
        pub id: String,
        pub sleeve_id: String,
        pub kind: ApprovalKind,
        pub title: String,
        pub summary: String,
        pub detail: String = String::new(),
        /// The answers offered; empty means allow_once and deny.
        pub options: Vec<Decision> = Vec::new(),
        pub requested_at_ms: EpochMs,
        pub timeout_at_ms: Option<EpochMs>,
        pub state: ApprovalState,
        pub decision: Option<Decision>,
        pub decided_by: Option<DecidedBy>,
        pub decided_at_ms: Option<EpochMs>,
        /// Why a decision did or will do nothing (the asker is gone), when that is so.
        pub note: String = String::new(),
    }

    pub struct Presence {
        pub state: PresenceState,
        pub idle_secs: Option<u64>,
        pub locked: Option<bool>,
        /// The detector used: logind, evdev, wts, hid.
        pub source: String = String::new(),
        pub sampled_at_ms: Option<EpochMs>,
    }

    pub struct Gpu {
        pub name: String,
        pub driver: String = String::new(),
        pub vram_bytes: Option<u64>,
        pub util_pct: Option<f64>,
    }

    pub struct Disk {
        pub mount: String,
        pub size_bytes: u64,
        pub free_bytes: u64,
    }

    pub struct Toolchain {
        pub name: String,
        pub version: String,
    }

    pub struct Inventory {
        pub hostname: String,
        pub os: String,
        pub os_version: String,
        pub arch: String,
        pub cpu_model: String,
        pub cpu_count: u64,
        pub ram_bytes: u64,
        pub gpus: Vec<Gpu> = Vec::new(),
        pub disks: Vec<Disk> = Vec::new(),
        pub toolchains: Vec<Toolchain> = Vec::new(),
        pub collected_at_ms: EpochMs,
    }

    /// Unknown numbers are absent and never count as spare capacity.
    pub struct Sample {
        pub sampled_at_ms: EpochMs,
        pub cpu_pct: Option<f64>,
        pub load1: Option<f64>,
        pub mem_avail_bytes: Option<u64>,
        pub swap_used_bytes: Option<u64>,
    }

    pub struct Node {
        pub id: String,
        pub name: String,
        pub online: bool,
        pub last_seen_ms: EpochMs,
        pub node_version: String,
        pub error: Option<String>,
        /// Absent until the first inventory arrives.
        pub inventory: Option<Inventory>,
        pub sample: Option<Sample>,
        pub presence: Presence,
    }

    pub struct Job {
        pub id: String,
        /// The node it runs on, once scheduled.
        pub node_id: Option<String>,
        pub target: JobTarget,
        pub argv: Vec<String>,
        pub state: JobState,
        /// `owner` or the requesting sleeve's name.
        pub requested_by: String,
        pub attempt: u64 = 1,
        pub even_if_present: bool = false,
        pub on_present: OnPresent = OnPresent::Idle,
        /// Set while the job waits for the owner (node maintenance jobs).
        pub approval_id: Option<String>,
        pub queued_at_ms: EpochMs,
        pub started_at_ms: Option<EpochMs>,
        pub ended_at_ms: Option<EpochMs>,
        pub exit_code: Option<i64>,
        /// Why the hub ended it when no node did, such as `node 4 was removed`.
        pub error: Option<String>,
        /// Bytes of log the hub holds.
        pub log_bytes: u64 = 0,
        /// The node's directory the job ran in; absent means the node's own work dir.
        pub cwd: Option<String>,
        /// The files the job was asked to leave, fetched once it exits.
        pub artifacts: Vec<JobArtifact> = Vec::new(),
    }

    /// A file a job leaves on its node. `sha256` and `size_bytes` are set once the hub has
    /// fetched it.
    pub struct JobArtifact {
        pub path: String,
        pub size_bytes: Option<u64>,
        pub sha256: Option<String>,
    }

    pub struct QuotaWindow {
        /// `5h`, `wk`, `day` as the vendor names it.
        pub label: String,
        pub used_pct: Option<f64>,
        pub resets_at_ms: Option<EpochMs>,
        /// Vendor window length in milliseconds, when the reading named one.
        pub length_ms: Option<u64>,
    }

    pub struct Quota {
        pub subscription: Subscription,
        pub plan: String,
        pub reading: QuotaReading,
        pub fetched_at_ms: EpochMs,
        pub windows: Vec<QuotaWindow> = Vec::new(),
        pub note: String = String::new(),
    }

    /// A line of the mail and events log.
    pub struct HubEvent {
        pub seq: u64,
        pub at_ms: EpochMs,
        pub kind: EventKind,
        pub source: String,
        pub target: Option<String>,
        pub text: String,
        pub read: bool = false,
        pub level: Level = Level::Info,
    }

    pub struct Device {
        pub id: String,
        pub name: String,
        pub created_at_ms: EpochMs,
        pub last_seen_ms: Option<EpochMs>,
    }
}

api_unions! {
    /// The normalized transcript entry (docs/design/sessions-chat-first.md section 3).
    pub enum Entry {
        /// A new incarnation's session: a divider line.
        SessionInfo = "session_info" {
            harness: Harness,
            mode: Mode,
            native_id: String,
            model: Option<String>,
            provider: Option<String>,
            effort: Option<String>,
        },
        /// Written to the harness; the verdict follows as `prompt_ack`.
        UserMessage = "user_message" {
            request_id: String,
            /// `owner`, `envoy`, `mail:<sleeve>`, or `agent:<name>` for a vendor general agent.
            from: String,
            text: String,
            steer: bool = false,
        },
        PromptAck = "prompt_ack" {
            request_id: String,
            ack: AckKind,
            detail: Option<String>,
        },
        /// `delta` true: `text` is the next piece of block `id`, live only. False: the whole
        /// block.
        AssistantText = "assistant_text" {
            id: String,
            text: String,
            delta: bool = false,
        },
        Reasoning = "reasoning" {
            id: String,
            text: String,
            delta: bool = false,
        },
        ToolCall = "tool_call" {
            id: String,
            tool: ToolKind,
            name: String,
            /// At most 300 characters.
            summary: String,
            /// Bounded to 16 KiB.
            input: Option<Value>,
            truncated: bool = false,
        },
        /// Folds into the `tool_call` with the same `id`.
        ToolResult = "tool_result" {
            id: String,
            status: ToolStatus,
            output: String,
            truncated: bool = false,
            duration_ms: Option<u64>,
        },
        /// Computed by the hub from git at the turn boundary.
        FileChange = "file_change" {
            tool_id: Option<String>,
            files: Vec<FileDelta> = Vec::new(),
            diff_ref: Option<String>,
        },
        /// The card for an [`Approval`]; its state lives there.
        ApprovalRequest = "approval_request" {
            approval_id: String,
            title: String,
            summary: String,
        },
        ApprovalDecision = "approval_decision" {
            approval_id: String,
            decision: Decision,
            by: DecidedBy,
        },
        TurnStarted = "turn_started" {
            /// The user message that caused it; absent for mail and resume.
            request_id: Option<String>,
        },
        TurnSettled = "turn_settled" {
            stop: Stop,
            native_stop: Option<String>,
            usage: Usage,
            duration_ms: Option<u64>,
        },
        Error = "error" {
            kind: ErrorClass,
            text: String,
        },
        Compaction = "compaction" {
            trigger: Option<String>,
            tokens_before: Option<u64>,
            tokens_after: Option<u64>,
        },
        /// Hub and harness notices: retries, guardrails, resleeves, takeover.
        Notice = "notice" {
            level: Level = Level::Info,
            /// The source, such as `guardrail:G1` or `unmapped:<type>`.
            code: Option<String>,
            text: String,
        },
        /// The last entry of an incarnation.
        Exited = "exited" {
            code: Option<i64>,
            signal: Option<i64>,
            requested: bool,
            stderr_tail: String = String::new(),
        },
    }

    /// Where a job may run.
    pub enum JobTarget {
        /// Asked for by headscale id or name; the hub keeps the id.
        Node = "node" {
            node_id: String,
        },
        /// Any AWAY node of this arch, picked by the scheduler.
        Arch = "arch" {
            arch: String,
        },
    }

    /// One message of `GET /api/events`. `seq` grows by one per kept message; a streaming
    /// delta repeats the seq of the last kept message and is not replayed on resume. The stream
    /// starts with `snapshot` unless `from_seq` is given and the hub still holds every message
    /// from it.
    pub enum HubStreamMessage {
        Snapshot = "snapshot" {
            seq: u64,
            state: HubState,
        },
        Workspace = "workspace" {
            seq: u64,
            workspace: Workspace,
        },
        Sleeve = "sleeve" {
            seq: u64,
            sleeve: Sleeve,
        },
        SleeveRemoved = "sleeve_removed" {
            seq: u64,
            id: String,
        },
        Entry = "entry" {
            seq: u64,
            sleeve_id: String,
            entry: ChatEntry,
        },
        Approval = "approval" {
            seq: u64,
            approval: Approval,
        },
        Hub = "hub" {
            seq: u64,
            hub: HubInfo,
        },
        Node = "node" {
            seq: u64,
            node: Node,
        },
        /// The node left the tailnet (deleted, or no longer tagged as a node).
        NodeRemoved = "node_removed" {
            seq: u64,
            id: String,
        },
        Job = "job" {
            seq: u64,
            job: Job,
        },
        Quota = "quota" {
            seq: u64,
            quota: Quota,
        },
        Event = "event" {
            seq: u64,
            event: HubEvent,
        },
    }
}
