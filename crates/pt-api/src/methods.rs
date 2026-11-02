//! Request and response bodies of `POST /api/<Method>`, and API errors.

use crate::schema::EpochMs;
use crate::types::*;

api_enums! {
    /// How a message reaches a sleeve that is mid-turn.
    pub enum Delivery {
        /// Wait for the turn to settle.
        Queue = "queue",
        /// Into the running turn: native on Codex and Pi; Claude and Grok queue.
        Steer = "steer",
    }

    /// SPEC 3.3: what `Wait` waits for.
    pub enum WaitUntil {
        /// Idle, seen or not.
        Idle = "idle",
        /// A turn settled that the waiter has not seen.
        Done = "done",
        /// Waiting on an approval.
        Blocked = "blocked",
        /// The sleeve mailed its parent or Envoy.
        Report = "report",
        /// The incarnation ended.
        Ended = "ended",
        Parked = "parked",
    }

    /// How `SearchHistory` combines the query's words.
    pub enum HistoryMode {
        /// Every word (or quoted phrase) must match.
        All = "all",
        /// Any word may match; BM25 ranks.
        Any = "any",
        /// The whole query is one phrase.
        Phrase = "phrase",
    }

    /// What an indexed history doc is.
    pub enum HistoryRole {
        User = "user",
        Assistant = "assistant",
        /// Tool name, paths and command line.
        ToolCall = "tool_call",
        /// A failed tool result.
        ToolError = "tool_error",
        /// A compaction summary.
        Compaction = "compaction",
    }

    /// Stable takes `vX.Y.Z` tags only; dev takes every `vX.Y.Z[-PRE]` tag.
    pub enum ReleaseChannel {
        Stable = "stable",
        Dev = "dev",
    }

    pub enum HarnessUpdateState {
        /// Running now, in the background; its outcome is a hub event.
        Started = "started",
        /// A sleeve on it is mid-turn: it runs once they are all idle.
        Deferred = "deferred",
        /// Not installed, without an update command, or already running.
        Skipped = "skipped",
    }

    pub enum ErrorCode {
        /// No credential, or one the hub does not know.
        Unauthenticated = "unauthenticated",
        NotFound = "not_found",
        Invalid = "invalid",
        Forbidden = "forbidden",
        Conflict = "conflict",
        Unavailable = "unavailable",
        /// The hub gave up waiting: on its core, or on an owner approval a code rule asked for.
        Timeout = "timeout",
        Internal = "internal",
    }
}

impl ErrorCode {
    pub fn http_status(self) -> u16 {
        match self {
            ErrorCode::Unauthenticated => 401,
            ErrorCode::NotFound => 404,
            ErrorCode::Invalid => 400,
            ErrorCode::Forbidden => 403,
            ErrorCode::Conflict => 409,
            ErrorCode::Unavailable => 503,
            ErrorCode::Timeout => 504,
            ErrorCode::Internal => 500,
        }
    }
}

api_records! {
    /// No fields.
    pub struct Empty {}

    /// The body of every non-2xx response.
    pub struct ErrorBody {
        pub error: ApiError,
    }

    pub struct ApiError {
        pub code: ErrorCode,
        pub message: String,
    }

    pub struct SleeveList {
        pub sleeves: Vec<Sleeve> = Vec::new(),
    }

    pub struct GetTranscriptRequest {
        pub sleeve_id: String,
        /// The first seq wanted; absent means the last `limit` entries.
        pub from_seq: Option<u64>,
        /// The hub caps it.
        pub limit: Option<u64>,
    }

    pub struct SpawnSleeveRequest {
        pub name: String,
        pub harness: Harness,
        /// The directory a root claims; a child works in its parent's.
        pub path: Option<String>,
        /// Makes this a child of that root.
        pub parent_id: Option<String>,
        /// Children only; default worktree.
        pub view: Option<WorkspaceView>,
        pub mode: Mode = Mode::Headless,
        pub model: Option<String>,
        /// Pi only: a provider from Pi's catalog.
        pub provider: Option<String>,
        pub effort: Option<String>,
        /// Empty means the hub default, `[pi_local]`.
        pub resleeve_to: Vec<Harness> = Vec::new(),
        /// The first message, delivered once the incarnation is up.
        pub message: Option<String>,
        /// Tokens; a root's count includes its children's (G10).
        pub budget: Option<u64>,
    }

    pub struct SendMessageRequest {
        pub sleeve_id: String,
        pub text: String,
        pub delivery: Delivery = Delivery::Queue,
    }

    /// A message the hub took; its `user_message` and `prompt_ack` entries carry this id.
    pub struct Sent {
        pub request_id: String,
    }

    pub struct SleeveRef {
        pub sleeve_id: String,
    }

    /// The hub took the action. `approval_id` is set when a code rule made it wait for the
    /// owner first.
    pub struct Accepted {
        pub approval_id: Option<String>,
    }

    /// Starts a stopped or parked sleeve again, on its own target unless one is named.
    pub struct StartSleeveRequest {
        pub sleeve_id: String,
        pub target: Option<Harness>,
        pub model: Option<String>,
        pub provider: Option<String>,
        pub effort: Option<String>,
    }

    pub struct ResleeveRequest {
        pub sleeve_id: String,
        pub target: Harness,
        pub model: Option<String>,
        pub provider: Option<String>,
        pub effort: Option<String>,
    }

    pub struct ListApprovalsRequest {
        pub pending_only: bool = false,
    }

    pub struct ApprovalList {
        pub approvals: Vec<Approval> = Vec::new(),
    }

    pub struct DecideRequest {
        pub approval_id: String,
        pub decision: Decision,
    }

    pub struct NodeList {
        pub nodes: Vec<Node> = Vec::new(),
    }

    pub struct NodeRef {
        pub node_id: String,
    }

    pub struct RunJobRequest {
        pub target: JobTarget,
        pub argv: Vec<String>,
        /// An absolute directory on the node; default the node's own work dir.
        pub cwd: Option<String>,
        /// Files (relative to the job's directory, or absolute) the hub fetches once it exits.
        pub artifacts: Vec<String> = Vec::new(),
        /// Run although someone is at the node (PROBLEMS P2).
        pub even_if_present: bool = false,
        pub on_present: OnPresent = OnPresent::Idle,
        /// A maintenance job waits for the owner's approval.
        pub maintenance: bool = false,
        /// Default 1 h.
        pub timeout_secs: Option<u64>,
    }

    pub struct JobRef {
        pub job_id: String,
    }

    pub struct ListJobsRequest {
        /// Only the jobs on this node (id or name).
        pub node_id: Option<String>,
        /// The newest `limit` jobs; the hub caps it.
        pub limit: Option<u64>,
    }

    pub struct JobList {
        pub jobs: Vec<Job> = Vec::new(),
        /// Only the caller's and its children's jobs are visible.
        pub scoped_to_caller: bool,
    }

    pub struct GetJobArtifactRequest {
        pub job_id: String,
        /// As the job's `artifacts` list names it.
        pub path: String,
        pub offset: u64 = 0,
        /// Bytes; the hub caps it.
        pub limit: Option<u64>,
    }

    /// A slice of a fetched artifact, as lowercase hex.
    pub struct JobArtifactData {
        pub job_id: String,
        pub path: String,
        pub offset: u64,
        pub next_offset: u64,
        pub size_bytes: u64,
        pub sha256: String,
        pub hex: String,
        pub eof: bool,
    }

    /// How a machine joins this hub's tailnet: run `join_command` on it, then hand its QR text
    /// to `AddNode` or approve it with the line it prints.
    pub struct Enrollment {
        pub server_url: String,
        /// `sha256:<hex>` of headscale's control key; the node shows the one it saw.
        pub control_key: String,
        pub join_command: String,
        pub pending: Vec<PendingNode> = Vec::new(),
    }

    /// A registration the owner scanned or pasted but has not approved.
    pub struct PendingNode {
        pub auth_id: String,
        pub name: String,
        pub control_key: String,
        pub seen_at_ms: EpochMs,
        /// headscale forgets the registration then; the node must run `join` again.
        pub expires_at_ms: EpochMs,
        /// True when the node saw this hub's control key.
        pub key_matches: bool,
    }

    pub struct AddNodeRequest {
        /// The QR payload `protectorate-node join` shows, or its pasted text.
        pub registration: String,
    }

    /// `name` and `fingerprint` default to the pending registration's.
    pub struct ApproveNodeRequest {
        pub auth_id: String,
        pub name: Option<String>,
        pub fingerprint: Option<String>,
    }

    pub struct ApprovedNode {
        pub node_id: String,
        pub name: String,
    }

    pub struct RemoveNodeRequest {
        /// By headscale id or name.
        pub node_id: String,
        /// Remove it although it is online or has jobs in flight; those jobs go lost.
        pub force: bool = false,
    }

    pub struct RemovedNode {
        pub node_id: String,
        pub name: String,
        /// Queued jobs that could run only on this node: failed, `node N was removed`. One
        /// queued later (an approval was pending) fails the same way when the hub schedules it.
        pub failed_jobs: Vec<String> = Vec::new(),
        /// Jobs it had in flight (`force`), left to the lease: they go lost, then abandoned.
        pub orphaned_jobs: Vec<String> = Vec::new(),
        /// Headscale had already forgotten it; only the hub's enrollment record was left.
        pub record_only: bool = false,
    }

    pub struct RenameNodeRequest {
        /// By headscale id or name.
        pub node_id: String,
        /// A tailnet DNS label no other node has.
        pub name: String,
    }

    pub struct RenamedNode {
        pub node_id: String,
        pub from: String,
        pub to: String,
    }

    pub struct GetJobLogRequest {
        pub job_id: String,
        /// A byte offset into the job's log.
        pub offset: u64 = 0,
        /// Bytes; the hub caps it.
        pub limit: Option<u64>,
    }

    /// Log bytes as UTF-8 (invalid sequences replaced).
    pub struct JobLog {
        pub job_id: String,
        pub offset: u64,
        pub next_offset: u64,
        pub text: String,
        /// The job has ended and `next_offset` is the end of its log.
        pub eof: bool,
    }

    pub struct QuotaList {
        pub quotas: Vec<Quota> = Vec::new(),
    }

    pub struct ListEventsRequest {
        pub from_seq: Option<u64>,
        pub limit: Option<u64>,
    }

    pub struct EventList {
        pub events: Vec<HubEvent> = Vec::new(),
    }

    pub struct MarkEventsReadRequest {
        pub up_to_seq: u64,
    }

    pub struct EnvoySendRequest {
        pub text: String,
        /// The vendor agent's name for the `agent:<name>` tag, `[A-Za-z0-9_-]`; default `agent`.
        pub agent: Option<String>,
    }

    pub struct EnvoyWaitRequest {
        /// Absent: the caller's latest message to Envoy.
        pub request_id: Option<String>,
        /// The hub caps it.
        pub timeout_ms: u64 = 60_000,
    }

    /// Envoy's answer to one message.
    pub struct EnvoyReply {
        pub request_id: String,
        /// False when the timeout came first; `text` is what Envoy has said so far.
        pub settled: bool,
        /// Envoy's assistant text of the turns that answered, joined.
        pub text: String,
        /// The last transcript seq read; continue with `EnvoyRead` from `seq + 1`.
        pub seq: u64,
    }

    pub struct EnvoyReadRequest {
        pub from_seq: u64 = 0,
        pub limit: Option<u64>,
    }

    pub struct WaitRequest {
        pub sleeve_id: String,
        pub until: WaitUntil,
        /// The hub caps it.
        pub timeout_ms: u64 = 60_000,
    }

    pub struct WaitResult {
        /// False when the timeout came first.
        pub reached: bool,
        pub sleeve: Sleeve,
    }

    /// The envoy-only capability for vendor general agents (D9).
    pub struct EnvoyToken {
        pub token: String,
    }

    pub struct DeviceList {
        pub devices: Vec<Device> = Vec::new(),
    }

    pub struct AddDeviceRequest {
        pub name: String,
    }

    /// A one-time code the device trades for its token with `PairDevice` before
    /// `expires_at_ms`.
    pub struct PairingCode {
        pub name: String,
        pub code: String,
        pub expires_at_ms: EpochMs,
    }

    pub struct PairDeviceRequest {
        pub code: String,
    }

    /// `token` is the `pd1.` device token, shown once.
    pub struct PairedDevice {
        pub device: Device,
        pub token: String,
        pub expires_at_ms: Option<EpochMs>,
    }

    pub struct DeviceRef {
        pub device_id: String,
    }

    pub struct SearchHistoryRequest {
        pub query: String,
        pub mode: HistoryMode = HistoryMode::All,
        /// Sleeves by id or name, removed ones too. Empty means every sleeve the caller reaches.
        pub sleeves: Vec<String> = Vec::new(),
        /// `pi_local` and `pi_vendor` both match every Pi session.
        pub harness: Option<Harness>,
        /// A native session id.
        pub session: Option<String>,
        pub since_ms: Option<EpochMs>,
        pub until_ms: Option<EpochMs>,
        /// Empty means any role.
        pub roles: Vec<HistoryRole> = Vec::new(),
        /// Default 10; the hub caps it.
        pub limit: Option<u64>,
    }

    /// One indexed unit of a session the hub launched, with its provenance.
    pub struct HistoryDoc {
        pub doc_id: u64,
        pub sleeve_id: String,
        pub sleeve: String,
        pub harness: Option<Harness>,
        /// The native session id.
        pub session: String,
        /// User prompts in the session up to and including this doc.
        pub turn: u64,
        pub role: HistoryRole,
        pub time_ms: EpochMs,
        pub tool: Option<String>,
        pub files: Vec<String> = Vec::new(),
        /// The log the doc came from, and the byte offset of its line there.
        pub path: String,
        pub offset: u64,
    }

    /// A byte range of a snippet.
    pub struct HistorySpan {
        pub start: u64,
        pub end: u64,
    }

    pub struct HistoryHit {
        pub doc: HistoryDoc,
        pub score: f64,
        /// About 240 bytes around the best cluster of matches, whitespace flattened.
        pub snippet: String,
        pub highlights: Vec<HistorySpan> = Vec::new(),
    }

    pub struct HistoryHits {
        pub hits: Vec<HistoryHit> = Vec::new(),
        /// Docs in the index, all sleeves.
        pub indexed_docs: u64,
    }

    /// One doc by id, or a session's docs whose turn is in `from_turn..=to_turn`.
    pub struct ReadHistoryRequest {
        pub doc_id: Option<u64>,
        pub session: Option<String>,
        pub from_turn: u64 = 0,
        /// Absent: the session's last turn.
        pub to_turn: Option<u64>,
    }

    pub struct HistoryText {
        pub doc: HistoryDoc,
        pub text: String,
    }

    pub struct HistoryRead {
        pub docs: Vec<HistoryText> = Vec::new(),
        /// The hub cut the range short; read on from the last doc's turn.
        pub more: bool = false,
    }

    pub struct GetDoctorRequest {
        /// Run the checks again (a few seconds) before answering.
        pub refresh: bool = false,
    }

    /// A harness as `doctor` found it (SPEC 7, D33).
    pub struct DoctorHarness {
        /// The hub's name for it, `pi_durable` included.
        pub name: String,
        pub present: bool,
        pub path: String,
        pub version: String,
        /// `native`, `npm`, `script`, `other` or `missing`.
        pub install: String,
        /// A login file or key was found (never read or shown).
        pub login: bool,
        /// Whether the hub spawns, starts or resleeves onto it.
        pub eligible: bool,
        /// Why it is not eligible: each failed check.
        pub reasons: Vec<String> = Vec::new(),
        /// Its own update command; empty when it has none.
        pub update: Vec<String> = Vec::new(),
        /// The last run of that command by the hub, and how it went.
        pub updated_at_ms: Option<EpochMs>,
        pub update_note: String = String::new(),
    }

    /// The hub machine's `doctor.json`, compacted.
    pub struct DoctorReport {
        /// 0 until the first run finishes.
        pub checked_at_ms: EpochMs,
        pub os: String,
        pub arch: String,
        pub harnesses: Vec<DoctorHarness> = Vec::new(),
        pub gh_logged_in: bool,
        pub gh_account: String,
        pub sandbox_ok: bool,
        /// Everything that needs the owner, one line each.
        pub problems: Vec<String> = Vec::new(),
    }

    pub struct UpdateHarnessesRequest {
        /// Programs (`claude`, `codex`, `grok`, `pi`); empty means every installed one.
        pub programs: Vec<String> = Vec::new(),
    }

    pub struct HarnessUpdate {
        pub program: String,
        pub state: HarnessUpdateState,
        pub detail: String = String::new(),
    }

    pub struct HarnessUpdates {
        pub programs: Vec<HarnessUpdate> = Vec::new(),
    }

    pub struct UpdateHubRequest {
        /// Only say what would be installed.
        pub check: bool = false,
        /// Default: the hub's `release_channel`.
        pub channel: Option<ReleaseChannel>,
        /// A release tag, in place of the channel's newest.
        pub version: Option<String>,
        /// Swap `releases/current` and `releases/previous`, then restart.
        pub rollback: bool = false,
    }

    pub struct HubUpdate {
        /// What `releases/current` named before.
        pub current: Option<String>,
        /// The release picked, or rolled back to.
        pub target: String,
        /// The target differs from `current`.
        pub available: bool,
        /// The hub exits so its service manager starts the target, which keeps it only if its
        /// own health check passes within 2 minutes.
        pub restarting: bool,
        pub note: String = String::new(),
    }

    pub struct UpdateNodesRequest {
        /// By id or name; leave empty with `all`.
        pub nodes: Vec<String> = Vec::new(),
        /// Every online node, one canary per OS and arch first.
        pub all: bool = false,
        /// A release tag, in place of the channel's newest.
        pub version: Option<String>,
        pub channel: Option<ReleaseChannel>,
        /// How long a node waits for the hub after its restart before it rolls back (600).
        pub probation_secs: Option<u64>,
        /// Answer once each node has switched, without waiting for it to come back.
        pub no_wait: bool = false,
    }

    pub struct NodeUpdate {
        pub node_id: String,
        pub name: String,
        /// The release the node ran before, as it reported it.
        pub from: String,
        pub to: String,
        pub ok: bool,
        pub detail: String = String::new(),
    }

    pub struct NodeUpdates {
        pub version: String,
        pub nodes: Vec<NodeUpdate> = Vec::new(),
    }
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ApiError {
        ApiError {
            code,
            message: message.into(),
        }
    }

    pub fn body(self) -> ErrorBody {
        ErrorBody { error: self }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

api_methods! {
    // State
    /// The full snapshot.
    GetState(Empty) -> HubState: Owner,
    ListSleeves(Empty) -> SleeveList: Sleeve,
    GetTranscript(GetTranscriptRequest) -> Transcript: Sleeve,

    // Sleeves
    SpawnSleeve(SpawnSleeveRequest) -> Sleeve: Sleeve,
    SendMessage(SendMessageRequest) -> Sent: Sleeve,
    Interrupt(SleeveRef) -> Empty: Sleeve,
    /// Stops the sleeve and, for a root, every child in the same step.
    StopSleeve(SleeveRef) -> Accepted: Sleeve,
    /// Needs the owner's approval when Envoy asks during a turn an agent message started (D9).
    RemoveSleeve(SleeveRef) -> Accepted: Sleeve,
    StartSleeve(StartSleeveRequest) -> Accepted: Sleeve,
    Resleeve(ResleeveRequest) -> Accepted: Sleeve,
    /// Not built yet: answers `unavailable`.
    TakeoverTerminal(SleeveRef) -> Empty: Owner,

    // Approvals
    ListApprovals(ListApprovalsRequest) -> ApprovalList: Owner,
    Decide(DecideRequest) -> Approval: Owner,

    // Nodes and jobs, over the tailnet the hub's fleet runs. Without a fleet they answer
    // `unavailable` (ListNodes: no nodes).
    ListNodes(Empty) -> NodeList: Sleeve,
    /// By headscale id or name.
    GetNode(NodeRef) -> Node: Sleeve,
    GetEnrollment(Empty) -> Enrollment: Owner,
    /// Records a registration the owner scanned or pasted, for `ApproveNode`.
    AddNode(AddNodeRequest) -> PendingNode: Owner,
    /// Checks the control-key fingerprint, then registers, tags and names the node.
    ApproveNode(ApproveNodeRequest) -> ApprovedNode: Owner,
    /// Deletes the node from headscale, which revokes its tailnet key, and drops its
    /// enrollment record. Queued jobs pinned to it fail. An online node, or one with jobs in
    /// flight, needs `force`. After a partial removal, retry by id.
    RemoveNode(RemoveNodeRequest) -> RemovedNode: Owner,
    /// Renames the node in headscale and in its enrollment record; jobs follow its id.
    RenameNode(RenameNodeRequest) -> RenamedNode: Owner,
    /// A maintenance job from a sleeve, or a job Envoy asks for in an agent's turn (D9), waits
    /// for the owner's approval.
    RunJob(RunJobRequest) -> Job: Sleeve,
    ListJobs(ListJobsRequest) -> JobList: Sleeve,
    GetJob(JobRef) -> Job: Sleeve,
    GetJobLog(GetJobLogRequest) -> JobLog: Sleeve,
    GetJobArtifact(GetJobArtifactRequest) -> JobArtifactData: Sleeve,
    CancelJob(JobRef) -> Job: Sleeve,

    // Quota and events
    ListQuotas(Empty) -> QuotaList: Sleeve,
    ListEvents(ListEventsRequest) -> EventList: Sleeve,
    MarkEventsRead(MarkEventsReadRequest) -> Empty: Owner,

    // Envoy, for vendor general agents (D9)
    /// A message to Envoy from the caller.
    EnvoySend(EnvoySendRequest) -> Sent: EnvoyOnly,
    /// Blocks until Envoy's reply to `request_id` settles or the timeout passes.
    EnvoyWait(EnvoyWaitRequest) -> EnvoyReply: EnvoyOnly,
    /// Envoy's transcript from `from_seq`.
    EnvoyRead(EnvoyReadRequest) -> Transcript: EnvoyOnly,
    GetEnvoyToken(Empty) -> EnvoyToken: Owner,

    // Wait (SPEC 3.3)
    /// Blocks until the sleeve reaches `until` or the timeout passes.
    Wait(WaitRequest) -> WaitResult: Sleeve,

    // History (SPEC 3.7): the sessions this hub launched. A sleeve reaches its own and its
    // children's; Envoy and the owner reach every sleeve's.
    /// Keyword search with BM25 ranking, newest first on ties.
    SearchHistory(SearchHistoryRequest) -> HistoryHits: Sleeve,
    /// Whole docs, by id or by session and turn range.
    ReadHistory(ReadHistoryRequest) -> HistoryRead: Sleeve,

    // Hub
    GetHubInfo(Empty) -> HubInfo: Sleeve,

    // Doctor and updates (SPEC 6a, 7; D33)
    /// The doctor report; Envoy and the owner only.
    GetDoctor(GetDoctorRequest) -> DoctorReport: Sleeve,
    /// Runs each harness's own update command now; one with a sleeve mid-turn waits for it.
    UpdateHarnesses(UpdateHarnessesRequest) -> HarnessUpdates: Owner,
    /// Stages a release from GitHub, checks it against SHA256SUMS, switches `releases/current`
    /// and restarts the hub on probation.
    UpdateHub(UpdateHubRequest) -> HubUpdate: Owner,
    /// Has each node fetch, check and switch to a release, then waits for it to come back.
    UpdateNodes(UpdateNodesRequest) -> NodeUpdates: Owner,

    // Devices: `pd1.` tokens for the apps
    ListDevices(Empty) -> DeviceList: Owner,
    /// A one-time pairing code for a new device named `name`.
    AddDevice(AddDeviceRequest) -> PairingCode: Owner,
    /// Trades a pairing code for the device's token; the code is the credential.
    PairDevice(PairDeviceRequest) -> PairedDevice: Public,
    RevokeDevice(DeviceRef) -> Empty: Owner,
}
