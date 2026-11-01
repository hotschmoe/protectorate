//! The hub-node RPC set, carried as pt-json payloads. A request frame holds a `Request`
//! (`{"type": "<Method>", ...fields}`); its answer is one response frame with the method's reply,
//! or one error frame with an `ErrorReply`. `FetchLog` and `FetchArtifact` answer with raw chunk
//! frames of bytes and then an end frame holding a `LogEnd` or an `ArtifactEnd`. All frames of one
//! exchange carry the request's id.
//!
//! Protocol 2 adds the submit token (SPEC 12a fencing): the hub sends one with each submit, the
//! node keeps it with the job and echoes it in every `JobInfo`, and refuses a cancel, ack or fetch
//! whose token names another submit. It also adds `FetchArtifact`. A protocol 1 node ignores the
//! token fields and does not know `FetchArtifact`.

use std::collections::BTreeMap;

use pt_json::{Error, FromJson, Map, ToJson, Value, record, string_enum};

pub const PROTOCOL: u32 = 2;
/// Longest submit token.
pub const MAX_TOKEN: usize = 128;
/// Longest artifact path.
pub const MAX_PATH: usize = 4096;
/// The tailnet tag the member must report for the peer before anything is served.
pub const HUB_TAG: &str = "tag:hub";
/// Longest header line from the member, newline included.
pub const MAX_HEADER_LINE: usize = 4096;

pub const DEFAULT_TIMEOUT_SECS: u64 = 3600;
pub const MAX_TIMEOUT_SECS: u64 = 24 * 3600;
pub const DEFAULT_LOG_CAP: u64 = 64 << 20;
pub const MAX_LOG_CAP: u64 = 1 << 30;
pub const DEFAULT_FETCH: u64 = 1 << 20;
pub const MAX_FETCH: u64 = 16 << 20;
/// Largest chunk frame payload the node sends.
pub const CHUNK: usize = 64 << 10;

/// Exit codes the node reports for jobs it ended itself.
pub const EXIT_TIMEOUT: i64 = 124;
pub const EXIT_CANCELLED: i64 = 130;
pub const EXIT_KILLED: i64 = 137;

string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Method {
        Hello = "Hello",
        Inventory = "Inventory",
        Sample = "Sample",
        Presence = "Presence",
        SubmitJob = "SubmitJob",
        JobStatus = "JobStatus",
        FetchLog = "FetchLog",
        FetchArtifact = "FetchArtifact",
        CancelJob = "CancelJob",
        AckCollected = "AckCollected",
        ListJobs = "ListJobs",
        Update = "Update",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PresenceState {
        Present = "PRESENT",
        Away = "AWAY",
        Unknown = "UNKNOWN",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OnPresent {
        Lower = "lower",
        Kill = "kill",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum JobState {
        Running = "running",
        Cancelling = "cancelling",
        Succeeded = "succeeded",
        Failed = "failed",
        Cancelled = "cancelled",
        /// The node stopped while the job ran, and the job was ended because of it.
        Interrupted = "interrupted",
        /// The node stopped while the job ran, and how the job ended is unknown.
        Lost = "lost",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Isolation {
        Scope = "systemd-scope",
        Group = "process-group",
        JobObject = "job-object",
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorCode {
        BadRequest = "bad_request",
        UnknownMethod = "unknown_method",
        Present = "present",
        Busy = "busy",
        StaleAttempt = "stale_attempt",
        NotFound = "not_found",
        NotFinished = "not_finished",
        Unsupported = "unsupported",
        Internal = "internal",
    }
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        !matches!(self, JobState::Running | JobState::Cancelling)
    }
}

record! {
    /// The line the member writes before splicing a tailnet connection to the agent.
    #[derive(Debug, Clone, PartialEq)]
    pub struct PeerHeader {
        pub peer_name: Option<String>,
        pub tags: Vec<String> = Vec::new(),
        pub ip: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Hello {
        pub hub_version: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct HelloReply {
        pub protocol: u32,
        pub node_version: String,
        pub node_id: String,
        /// Changes only when the node's job journal is recreated; an unknown job counts as
        /// never seen only when this matches the epoch recorded at submit (SPEC 12a item 5).
        pub journal_epoch: String,
        pub hostname: String,
        pub os: String,
        pub arch: String,
        /// The release this agent was started from, when it runs from `<state>/releases/`.
        pub release: Option<String>,
    }

    /// Exactly one of `argv` and `script` (run by `sh -c` or `cmd /c`).
    #[derive(Debug, Clone, PartialEq)]
    pub struct SubmitJob {
        pub job_id: String,
        pub attempt: u64,
        pub argv: Option<Vec<String>>,
        pub script: Option<String>,
        pub cwd: Option<String>,
        pub env: BTreeMap<String, String> = BTreeMap::new(),
        pub timeout_secs: u64 = DEFAULT_TIMEOUT_SECS,
        pub log_cap: u64 = DEFAULT_LOG_CAP,
        pub even_if_present: bool = false,
        pub on_present: OnPresent = OnPresent::Lower,
        /// Names this submit; echoed in every status of the job.
        pub token: Option<String>,
    }

    /// Without `attempt`, the highest attempt the node holds.
    #[derive(Debug, Clone, PartialEq)]
    pub struct JobStatus {
        pub job_id: String,
        pub attempt: Option<u64>,
    }

    /// With `token`, refused unless the job was submitted with it.
    #[derive(Debug, Clone, PartialEq)]
    pub struct JobRef {
        pub job_id: String,
        pub attempt: u64,
        pub token: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct FetchLog {
        pub job_id: String,
        pub attempt: u64,
        pub offset: u64 = 0,
        pub max: u64 = DEFAULT_FETCH,
        pub token: Option<String>,
    }

    /// A regular file inside the finished job's directory (its `cwd`, else the node's own
    /// directory for that attempt), from `offset`, at most `max` bytes. `path` is relative and
    /// without `..`; one that still resolves outside the directory, through a symlink, is
    /// refused.
    #[derive(Debug, Clone, PartialEq)]
    pub struct FetchArtifact {
        pub job_id: String,
        pub attempt: u64,
        pub path: String,
        pub offset: u64 = 0,
        pub max: u64 = DEFAULT_FETCH,
        pub token: Option<String>,
    }

    /// Fetch release `version` from the GitHub releases API at `api` (https, or http to a
    /// loopback host) for `repo`, check its asset against the release's SHA256SUMS, stage it,
    /// switch `releases/current` on probation and restart through the service manager. Unless
    /// the hub makes contact within `probation_secs` of the restart, the node switches back.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Update {
        pub version: String,
        pub api: String,
        pub repo: String,
        pub probation_secs: u64 = 600,
    }

    /// The node staged and switched, and restarts once this is sent.
    #[derive(Debug, Clone, PartialEq)]
    pub struct UpdateReply {
        pub from: Option<String>,
        pub to: String,
        /// The release was on disk already and matched SHA256SUMS.
        pub reused: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct JobInfo {
        pub job_id: String,
        pub attempt: u64,
        pub state: JobState,
        pub submitted_at: u64,
        pub started_at: Option<u64>,
        pub ended_at: Option<u64>,
        pub pid: Option<u32>,
        pub isolation: Option<Isolation>,
        pub scope: Option<String>,
        pub exit_code: Option<i64>,
        pub reason: Option<String>,
        pub log_bytes: u64 = 0,
        pub log_truncated: bool = false,
        pub lowered: bool = false,
        pub on_present: OnPresent = OnPresent::Lower,
        pub timeout_secs: u64 = DEFAULT_TIMEOUT_SECS,
        /// The submit's token, as it came.
        pub token: Option<String>,
    }

    /// The answer to SubmitJob, JobStatus, CancelJob and AckCollected; `job` is absent when the
    /// node holds no such job.
    #[derive(Debug, Clone, PartialEq)]
    pub struct JobReply {
        pub journal_epoch: String,
        pub job: Option<JobInfo>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct JobList {
        pub journal_epoch: String,
        pub jobs: Vec<JobInfo> = Vec::new(),
    }

    /// The end frame of a FetchLog stream. `offset` is where the next fetch starts; `done` means
    /// the job has ended and every log byte has been sent.
    #[derive(Debug, Clone, PartialEq)]
    pub struct LogEnd {
        pub offset: u64,
        pub size: u64,
        pub done: bool,
        pub truncated: bool,
    }

    /// The end frame of a FetchArtifact stream. `offset` is where the next fetch starts; `done`
    /// means it reached `size`, and then `sha256` is the whole file's, read again at the end.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ArtifactEnd {
        pub offset: u64,
        pub size: u64,
        pub done: bool,
        pub sha256: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct ErrorReply {
        pub code: ErrorCode,
        pub message: String,
        pub journal_epoch: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Gpu {
        pub vendor: String,
        pub name: String,
        pub vendor_id: Option<u32>,
        pub device_id: Option<u32>,
        pub vram_bytes: Option<u64>,
        pub driver: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Disk {
        pub mount: String,
        pub device: String,
        pub fs: String,
        pub total_bytes: u64,
        pub free_bytes: u64,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Toolchain {
        pub name: String,
        pub version: String,
        pub path: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Inventory {
        pub collected_at: u64,
        pub hostname: String,
        pub os_name: String,
        pub os_version: String,
        pub kernel: String,
        pub arch: String,
        pub cpu_model: String,
        pub cores: u32,
        pub threads: u32,
        pub ram_total_bytes: u64,
        pub gpus: Vec<Gpu> = Vec::new(),
        pub disks: Vec<Disk> = Vec::new(),
        pub toolchains: Vec<Toolchain> = Vec::new(),
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Sample {
        pub taken_at: u64,
        pub cpu_percent: Option<f64>,
        pub load: Option<Vec<f64>>,
        pub mem_total_bytes: Option<u64>,
        pub mem_available_bytes: Option<u64>,
        pub mem_used_bytes: Option<u64>,
        pub swap_total_bytes: Option<u64>,
        pub swap_free_bytes: Option<u64>,
        pub disks: Vec<Disk> = Vec::new(),
        pub uptime_secs: Option<u64>,
    }

    /// Human presence at the machine (SPEC 3.5): facts, with the threshold that was applied.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Presence {
        pub state: PresenceState,
        pub idle_secs: Option<u64>,
        pub locked: Option<bool>,
        pub source: String,
        pub detail: String,
        /// A second, independent idle reading when one exists, for comparison only.
        pub alt_idle_secs: Option<u64>,
        pub away_after_secs: u64,
        pub sampled_at: u64,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Hello(Hello),
    Inventory,
    Sample,
    Presence,
    SubmitJob(SubmitJob),
    JobStatus(JobStatus),
    FetchLog(FetchLog),
    FetchArtifact(FetchArtifact),
    CancelJob(JobRef),
    AckCollected(JobRef),
    ListJobs,
    Update(Update),
}

impl Request {
    pub fn method(&self) -> Method {
        match self {
            Request::Hello(_) => Method::Hello,
            Request::Inventory => Method::Inventory,
            Request::Sample => Method::Sample,
            Request::Presence => Method::Presence,
            Request::SubmitJob(_) => Method::SubmitJob,
            Request::JobStatus(_) => Method::JobStatus,
            Request::FetchLog(_) => Method::FetchLog,
            Request::FetchArtifact(_) => Method::FetchArtifact,
            Request::CancelJob(_) => Method::CancelJob,
            Request::AckCollected(_) => Method::AckCollected,
            Request::ListJobs => Method::ListJobs,
            Request::Update(_) => Method::Update,
        }
    }

    /// Builds the request for `method` from its fields (an object without `type`).
    pub fn from_fields(method: Method, fields: &Value) -> Result<Request, Error> {
        Ok(match method {
            Method::Hello => Request::Hello(Hello::from_json(fields)?),
            Method::Inventory => Request::Inventory,
            Method::Sample => Request::Sample,
            Method::Presence => Request::Presence,
            Method::SubmitJob => Request::SubmitJob(SubmitJob::from_json(fields)?),
            Method::JobStatus => Request::JobStatus(JobStatus::from_json(fields)?),
            Method::FetchLog => Request::FetchLog(FetchLog::from_json(fields)?),
            Method::FetchArtifact => Request::FetchArtifact(FetchArtifact::from_json(fields)?),
            Method::CancelJob => Request::CancelJob(JobRef::from_json(fields)?),
            Method::AckCollected => Request::AckCollected(JobRef::from_json(fields)?),
            Method::ListJobs => Request::ListJobs,
            Method::Update => Request::Update(Update::from_json(fields)?),
        })
    }

    fn fields(&self) -> Value {
        match self {
            Request::Hello(r) => r.to_json(),
            Request::SubmitJob(r) => r.to_json(),
            Request::JobStatus(r) => r.to_json(),
            Request::FetchLog(r) => r.to_json(),
            Request::FetchArtifact(r) => r.to_json(),
            Request::CancelJob(r) | Request::AckCollected(r) => r.to_json(),
            Request::Update(r) => r.to_json(),
            Request::Inventory | Request::Sample | Request::Presence | Request::ListJobs => {
                Value::Object(Map::new())
            }
        }
    }
}

impl ToJson for Request {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("type", self.method().as_str());
        if let Value::Object(fields) = self.fields() {
            for (k, v) in fields {
                m.insert(k, v);
            }
        }
        Value::Object(m)
    }
}

impl FromJson for Request {
    fn from_json(v: &Value) -> Result<Request, Error> {
        if v.as_object().is_none() {
            return Err(Error::new("expected object"));
        }
        let method = match v.get("type") {
            Some(t) => Method::from_json(t).map_err(|e| e.in_field("type"))?,
            None => return Err(Error::new("missing").in_field("type")),
        };
        Request::from_fields(method, v)
    }
}

/// Compact JSON bytes for a frame payload.
pub fn payload<T: ToJson + ?Sized>(v: &T) -> Vec<u8> {
    v.to_json().to_string().into_bytes()
}
