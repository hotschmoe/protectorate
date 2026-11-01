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
    }

    pub struct ResleeveRequest {
        pub sleeve_id: String,
        pub target: Harness,
        pub model: Option<String>,
        pub provider: Option<String>,
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

    // Nodes and jobs: no node transport yet, so the job methods answer `unavailable`.
    ListNodes(Empty) -> NodeList: Sleeve,
    GetNode(NodeRef) -> Node: Sleeve,
    RunJob(RunJobRequest) -> Job: Sleeve,
    GetJob(JobRef) -> Job: Sleeve,
    GetJobLog(GetJobLogRequest) -> JobLog: Sleeve,
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

    // Hub
    GetHubInfo(Empty) -> HubInfo: Sleeve,

    // Devices: `pd1.` tokens for the apps
    ListDevices(Empty) -> DeviceList: Owner,
    /// A one-time pairing code for a new device named `name`.
    AddDevice(AddDeviceRequest) -> PairingCode: Owner,
    /// Trades a pairing code for the device's token; the code is the credential.
    PairDevice(PairDeviceRequest) -> PairedDevice: Public,
    RevokeDevice(DeviceRef) -> Empty: Owner,
}
