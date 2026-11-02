//! Node jobs on the hub (SPEC 3.5, 6, 12a, 12b): the request a job carries through the core,
//! node scheduling, one serial executor per node, and the hub's copy of each job's log and
//! artifacts.
//!
//! A node's executor thread is the one ordered stream for that node's reports: it reports
//! `Dispatched` before it writes, then the effect's outcome, then the node's status, in the
//! order they happened. `Dispatched` waits for the core's answer, and a refused one is not
//! written: a job cancelled before its submit left has taken the submit back.
//!
//! Each submit carries a token naming it (SPEC 12a fencing); a status whose token is not the
//! current attempt's is ignored, and cancels, acks and fetches carry it so the node refuses them
//! for any other submit. Artifacts come back through the node's `FetchArtifact`.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use pt_api::{self as api, EpochMs, HubStreamMessage};
use pt_core::{
    EffectKind, EffectState, EventKind, Intent, Job, JobId, JobReport, JobSpec, JobState, NodeId,
    Reply,
};
use pt_fleet::NodeClient;
use pt_frame::msg::{
    self, ErrorCode as NodeCode, ErrorReply, FetchArtifact, FetchLog, JobRef, JobReply, OnPresent,
    SubmitJob,
};
use pt_json::ToJson;

use crate::hub::{Hub, View};
use crate::log::{debug, error, info, warning};
use crate::transcript::lock;

/// Log bytes asked for per FetchLog.
pub const LOG_FETCH: u64 = 1 << 20;
/// How long an executor waits for a fleet that is still starting (after a hub restart).
const FLEET_WAIT: Duration = Duration::from_secs(120);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a node executor waits for the core to take a `Dispatched`.
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(30);
const SCHEDULE_EVERY: Duration = Duration::from_secs(2);
const ARTIFACT_CAP: u64 = 64 << 20;
/// Artifact bytes asked for per FetchArtifact.
const ARTIFACT_FETCH: u64 = 4 << 20;

pt_json::record! {
    /// What `RunJob` asked for. It rides through the core as the job spec's `command`, so the
    /// journal holds it, with its SHA-256 as the spec's `digest`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct JobRequest {
        pub argv: Vec<String>,
        pub target: api::JobTarget,
        pub cwd: Option<String>,
        pub artifacts: Vec<String> = Vec::new(),
        pub on_present: api::OnPresent = api::OnPresent::Idle,
        pub timeout_secs: u64 = msg::DEFAULT_TIMEOUT_SECS,
        pub maintenance: bool = false,
        pub requested_by: String,
        /// The requesting sleeve; none for the owner.
        pub sleeve: Option<u64>,
        pub approval_id: Option<String>,
        pub queued_at_ms: u64,
    }

    /// One artifact as the hub fetched it, in `artifacts.json`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Fetched {
        pub path: String,
        /// The file under the attempt's `artifacts/` dir.
        pub file: String,
        pub size_bytes: u64,
        pub sha256: String,
    }
}

impl JobRequest {
    pub fn spec(&self, even_if_present: bool) -> JobSpec {
        let command = self.to_json().to_string();
        JobSpec {
            digest: pt_hash::to_hex(&pt_hash::sha256(command.as_bytes())),
            arch: match &self.target {
                api::JobTarget::Arch { arch } => arch.clone(),
                api::JobTarget::Node { .. } => String::new(),
            },
            command,
            even_if_present,
        }
    }

    pub fn of(spec: &JobSpec) -> Option<JobRequest> {
        pt_json::from_str(&spec.command).ok()
    }
}

/// The node's job id for a hub job: the hub tag keeps two hubs' jobs apart on one node.
pub fn node_job_id(hub: &Hub, job: JobId) -> String {
    format!("{}-j{}", hub.tokens.hub_tag, job.0)
}

/// The submit's token: it names this hub, job, attempt and submit record, so a status about any
/// other submit under the same node job id never passes for this one.
fn submit_token(hub: &Hub, j: &Job) -> Option<String> {
    let eff = j.submit?;
    let text = format!(
        "{}\n{}\n{}\n{}\n{}",
        hub.tokens.hub_tag, j.id.0, j.attempt, eff.0, j.spec.digest
    );
    Some(pt_hash::to_hex(&pt_hash::sha256(text.as_bytes()))[..32].to_string())
}

/// The core's epoch for a node's journal epoch string.
fn epoch(s: &str) -> u64 {
    let d = pt_hash::sha256(s.as_bytes());
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    u64::from_be_bytes(b)
}

fn arch(a: &str) -> &str {
    match a {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "amd64" | "x64" => "amd64",
        other => other,
    }
}

pub fn api_state(s: JobState) -> api::JobState {
    use api::JobState as A;
    match s {
        JobState::Queued => A::Queued,
        JobState::Submitting => A::Submitting,
        JobState::SubmitUncertain => A::SubmitUncertain,
        JobState::Running => A::Running,
        JobState::Collecting => A::Collecting,
        JobState::Succeeded => A::Succeeded,
        JobState::Failed(_) => A::Failed,
        JobState::Interrupted => A::Interrupted,
        JobState::Lost => A::Lost,
        JobState::Cancelling => A::Cancelling,
        JobState::Cancelled => A::Cancelled,
        JobState::Abandoned => A::Abandoned,
    }
}

pub fn ended(s: JobState) -> bool {
    matches!(
        s,
        JobState::Succeeded
            | JobState::Failed(_)
            | JobState::Interrupted
            | JobState::Cancelled
            | JobState::Abandoned
    )
}

// ---- the hub's copy ------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Meta {
    log_bytes: Option<u64>,
    started_at_ms: Option<u64>,
    fetched: Option<Vec<Fetched>>,
}

/// `<home>/jobs/<job>/<attempt>/`: `log`, `artifacts.json` and `artifacts/<n>`.
pub struct JobStore {
    root: PathBuf,
    meta: Mutex<BTreeMap<(JobId, u32), Meta>>,
}

impl JobStore {
    pub fn new(root: PathBuf) -> JobStore {
        JobStore {
            root,
            meta: Mutex::new(BTreeMap::new()),
        }
    }

    fn dir(&self, job: JobId, attempt: u32) -> PathBuf {
        self.root.join(job.0.to_string()).join(attempt.to_string())
    }

    fn log_path(&self, job: JobId, attempt: u32) -> PathBuf {
        self.dir(job, attempt).join("log")
    }

    pub fn log_len(&self, job: JobId, attempt: u32) -> u64 {
        let mut m = lock(&self.meta);
        let e = m.entry((job, attempt)).or_default();
        *e.log_bytes
            .get_or_insert_with(|| fs::metadata(self.log_path(job, attempt)).map_or(0, |m| m.len()))
    }

    /// Appends bytes that start at `at`, which must be the hub copy's length.
    fn append(&self, job: JobId, attempt: u32, at: u64, bytes: &[u8]) -> io::Result<u64> {
        let path = self.log_path(job, attempt);
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        fs::create_dir_all(self.dir(job, attempt)).map_err(ctx)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(ctx)?;
        let len = f.metadata().map_err(ctx)?.len();
        if len != at {
            return Err(ctx(io::Error::other(format!(
                "the hub copy is {len} bytes, not {at}"
            ))));
        }
        f.write_all(bytes)
            .and_then(|()| f.sync_data())
            .map_err(ctx)?;
        let n = at + bytes.len() as u64;
        lock(&self.meta)
            .entry((job, attempt))
            .or_default()
            .log_bytes = Some(n);
        Ok(n)
    }

    /// Up to `limit` bytes of the hub copy from `offset`, and the copy's length.
    pub fn read_log(
        &self,
        job: JobId,
        attempt: u32,
        offset: u64,
        limit: u64,
    ) -> io::Result<(Vec<u8>, u64)> {
        read_slice(&self.log_path(job, attempt), offset, limit)
    }

    fn set_started(&self, job: JobId, attempt: u32, ms: u64) {
        lock(&self.meta)
            .entry((job, attempt))
            .or_default()
            .started_at_ms = Some(ms);
    }

    fn started(&self, job: JobId, attempt: u32) -> Option<u64> {
        lock(&self.meta)
            .get(&(job, attempt))
            .and_then(|m| m.started_at_ms)
    }

    pub fn fetched(&self, job: JobId, attempt: u32) -> Vec<Fetched> {
        let mut m = lock(&self.meta);
        let e = m.entry((job, attempt)).or_default();
        e.fetched
            .get_or_insert_with(|| {
                fs::read(self.dir(job, attempt).join("artifacts.json"))
                    .ok()
                    .and_then(|b| pt_json::from_bytes(&b).ok())
                    .unwrap_or_default()
            })
            .clone()
    }

    fn save_fetched(&self, job: JobId, attempt: u32, list: Vec<Fetched>) -> io::Result<()> {
        let path = self.dir(job, attempt).join("artifacts.json");
        crate::home::write_private(&path, list.to_json().to_string().as_bytes())?;
        lock(&self.meta).entry((job, attempt)).or_default().fetched = Some(list);
        Ok(())
    }

    fn artifact_path(&self, job: JobId, attempt: u32, file: &str) -> PathBuf {
        self.dir(job, attempt).join("artifacts").join(file)
    }

    /// A slice of fetched artifact `path`, with its record.
    pub fn read_artifact(
        &self,
        job: JobId,
        attempt: u32,
        path: &str,
        offset: u64,
        limit: u64,
    ) -> io::Result<Option<(Fetched, Vec<u8>)>> {
        let Some(f) = self
            .fetched(job, attempt)
            .into_iter()
            .find(|f| f.path == path)
        else {
            return Ok(None);
        };
        let (bytes, _) = read_slice(&self.artifact_path(job, attempt, &f.file), offset, limit)?;
        Ok(Some((f, bytes)))
    }
}

fn read_slice(path: &std::path::Path, offset: u64, limit: u64) -> io::Result<(Vec<u8>, u64)> {
    let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(ctx(e)),
    };
    let len = f.metadata().map_err(ctx)?.len();
    if offset >= len {
        return Ok((Vec::new(), len));
    }
    f.seek(SeekFrom::Start(offset)).map_err(ctx)?;
    let mut buf = Vec::new();
    f.take(limit.min(len - offset))
        .read_to_end(&mut buf)
        .map_err(ctx)?;
    Ok((buf, len))
}

/// A core job as the API shows it.
pub fn view(hub: &Hub, j: &Job) -> api::Job {
    let req = JobRequest::of(&j.spec);
    let fetched = hub.jobs.fetched(j.id, j.attempt);
    let artifacts = req
        .as_ref()
        .map(|r| r.artifacts.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|p| {
            let f = fetched.iter().find(|f| f.path == *p);
            api::JobArtifact {
                path: p.clone(),
                size_bytes: f.map(|f| f.size_bytes),
                sha256: f.map(|f| f.sha256.clone()),
            }
        })
        .collect();
    api::Job {
        id: j.id.0.to_string(),
        node_id: j.node.map(|n| n.0.to_string()),
        target: req.as_ref().map_or_else(
            || api::JobTarget::Arch {
                arch: j.spec.arch.clone(),
            },
            |r| r.target.clone(),
        ),
        argv: req.as_ref().map(|r| r.argv.clone()).unwrap_or_default(),
        state: api_state(j.state),
        requested_by: req
            .as_ref()
            .map_or_else(|| "owner".into(), |r| r.requested_by.clone()),
        attempt: u64::from(j.attempt),
        even_if_present: j.spec.even_if_present,
        on_present: req.as_ref().map_or(api::OnPresent::Idle, |r| r.on_present),
        approval_id: req.as_ref().and_then(|r| r.approval_id.clone()),
        queued_at_ms: EpochMs(req.as_ref().map_or(0, |r| r.queued_at_ms)),
        started_at_ms: hub.jobs.started(j.id, j.attempt).map(EpochMs),
        ended_at_ms: ended(j.state).then_some(EpochMs(j.state_at)),
        exit_code: j.exit.map(i64::from),
        error: match j.state {
            JobState::Failed(pt_core::JobFailure::NodeRemoved(n)) => {
                Some(format!("node {} was removed", n.0))
            }
            _ => None,
        },
        log_bytes: hub.jobs.log_len(j.id, j.attempt),
        cwd: req.as_ref().and_then(|r| r.cwd.clone()),
        artifacts,
    }
}

fn stream(hub: &Hub, job: JobId) {
    let v = hub.view();
    if let Some(j) = v.state.jobs.get(&job) {
        let job = view(hub, j);
        hub.bus.push(|seq| HubStreamMessage::Job { seq, job });
    }
}

// ---- routing -------------------------------------------------------------------------------

pub struct JobExec {
    hub: Arc<Hub>,
    sched: Sender<Intent>,
    nodes: HashMap<NodeId, Sender<Intent>>,
}

fn node_of(i: &Intent) -> Option<NodeId> {
    match i {
        Intent::Execute { kind, .. } | Intent::Probe { kind, .. } => match kind {
            EffectKind::NodeSubmit { node, .. }
            | EffectKind::NodeCancel { node, .. }
            | EffectKind::NodeAckCollected { node, .. } => Some(*node),
            _ => None,
        },
        Intent::PollJob { node, .. }
        | Intent::ReadLog { node, .. }
        | Intent::FetchArtifacts { node, .. } => Some(*node),
        _ => None,
    }
}

impl JobExec {
    pub fn new(hub: Arc<Hub>) -> JobExec {
        let (sched, rx) = mpsc::channel();
        let h = hub.clone();
        thread::Builder::new()
            .name("pt-sched".into())
            .spawn(move || scheduler(&h, &rx))
            .expect("spawn the job scheduler");
        JobExec {
            hub,
            sched,
            nodes: HashMap::new(),
        }
    }

    pub fn route(&mut self, intent: Intent) {
        let Some(node) = node_of(&intent) else {
            let _ = self.sched.send(intent);
            return;
        };
        let intent = if let Some(tx) = self.nodes.get(&node) {
            match tx.send(intent) {
                Ok(()) => return,
                Err(e) => {
                    warning!("node {}: restarting its executor", node.0);
                    self.nodes.remove(&node);
                    e.0
                }
            }
        } else {
            intent
        };
        let (tx, rx) = mpsc::channel();
        let exec = NodeExec {
            hub: self.hub.clone(),
            node,
            client: None,
            epoch: String::new(),
            pending_status: None,
        };
        match thread::Builder::new()
            .name(format!("pt-node-{}", node.0))
            .spawn(move || exec.run(rx))
        {
            Ok(_) => {
                let _ = tx.send(intent);
                self.nodes.insert(node, tx);
            }
            Err(e) => {
                error!("node {}: spawn executor: {e}", node.0);
                crate::exec::refuse(&self.hub, intent, &format!("spawn executor: {e}"));
            }
        }
    }
}

// ---- scheduling ----------------------------------------------------------------------------

/// Pure node choice: an online node with an agent that answered, matching the target, AWAY
/// unless the job may run while someone is present, not excluded; the least loaded wins.
fn choose(
    nodes: &[pt_fleet::NodeView],
    req: &JobRequest,
    even_if_present: bool,
    exclude: &[NodeId],
) -> Option<(NodeId, u64)> {
    nodes
        .iter()
        .filter(|v| v.node.online && v.error.is_none())
        .filter_map(|v| Some((v, v.node.id.parse::<u64>().ok()?, v.hello.as_ref()?)))
        .filter(|(_, id, _)| !exclude.contains(&NodeId(*id)))
        .filter(|(v, _, h)| match &req.target {
            api::JobTarget::Node { node_id } => v.node.id == *node_id,
            api::JobTarget::Arch { arch: want } => arch(&h.arch) == arch(want),
        })
        .filter(|(v, ..)| even_if_present || v.node.presence.state == api::PresenceState::Away)
        .min_by(|a, b| {
            let load = |v: &pt_fleet::NodeView| {
                v.node
                    .sample
                    .as_ref()
                    .and_then(|s| s.load1)
                    .unwrap_or(f64::MAX)
            };
            load(a.0)
                .total_cmp(&load(b.0))
                .then_with(|| a.0.node.name.cmp(&b.0.node.name))
        })
        .map(|(_, id, h)| (NodeId(id), epoch(&h.journal_epoch)))
}

/// Holds every `ScheduleJob` until a node fits, re-trying every couple of seconds while the
/// job is still Queued.
fn scheduler(hub: &Hub, rx: &Receiver<Intent>) {
    let mut pending: BTreeMap<JobId, Vec<NodeId>> = BTreeMap::new();
    loop {
        match rx.recv_timeout(SCHEDULE_EVERY) {
            Ok(Intent::ScheduleJob { job, exclude }) => {
                pending.insert(job, exclude);
            }
            Ok(other) => debug!("scheduler: ignored {other:?}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(i) = rx.try_recv() {
            if let Intent::ScheduleJob { job, exclude } = i {
                pending.insert(job, exclude);
            }
        }
        if pending.is_empty() {
            continue;
        }
        let crate::fleet::Status::Up(up) = hub.fleet.status() else {
            continue;
        };
        let nodes = up.poller.nodes();
        let view = hub.view();
        let mut unseen: Vec<(JobId, NodeId)> = Vec::new();
        pending.retain(|job, exclude| {
            let Some(j) = view.state.jobs.get(job) else {
                return false;
            };
            if j.state != JobState::Queued {
                return false;
            }
            let Some(req) = JobRequest::of(&j.spec) else {
                warning!("job {}: its request does not parse", job.0);
                return false;
            };
            if let Some(node) = pinned(&req)
                && !nodes.iter().any(|v| v.node.id == node.0.to_string())
            {
                unseen.push((*job, node));
                return true;
            }
            match choose(&nodes, &req, j.spec.even_if_present, exclude) {
                Some((node, epoch)) => {
                    info!("job {}: attempt {} on node {}", job.0, j.attempt, node.0);
                    hub.submit(EventKind::NodeChosen {
                        job: *job,
                        attempt: j.attempt,
                        node,
                        epoch,
                    });
                    false
                }
                None => true,
            }
        });
        if !unseen.is_empty() {
            gone_nodes(hub, &up, &unseen, &mut pending);
        }
    }
}

/// The node id a job may run only on.
fn pinned(req: &JobRequest) -> Option<NodeId> {
    match &req.target {
        api::JobTarget::Node { node_id } => node_id.parse().ok().map(NodeId),
        api::JobTarget::Arch { .. } => None,
    }
}

/// Fails the queued jobs pinned to a node headscale itself no longer lists. The poller's views
/// lag behind (a node approved a moment ago, a hub that just started), so only headscale's
/// answer counts.
fn gone_nodes(
    hub: &Hub,
    up: &crate::fleet::Up,
    unseen: &[(JobId, NodeId)],
    pending: &mut BTreeMap<JobId, Vec<NodeId>>,
) {
    let listed = match up.fleet.admin().nodes() {
        Ok(l) => l,
        Err(e) => return warning!("scheduler: listing headscale's nodes: {e}"),
    };
    for &(job, node) in unseen {
        if listed
            .iter()
            .any(|n| n.id == node.0 && n.has_tag(pt_fleet::TAG_NODE))
        {
            continue;
        }
        info!("job {}: node {} was removed", job.0, node.0);
        hub.submit(EventKind::JobNodeRemoved { job, node });
        pending.remove(&job);
    }
}

// ---- one node's executor -------------------------------------------------------------------

enum Fail {
    /// Nothing was sent: the node could not be reached.
    Reach(String),
    /// The node answered with an error frame.
    Node(ErrorReply),
    /// The bytes came, but are not the artifact the node described.
    Check(String),
    /// Sent, then the connection failed: the call may have applied.
    Io(String),
    /// The node's refusal already went in as the job's status, which resolves the record.
    Answered,
}

impl Fail {
    fn text(&self) -> String {
        match self {
            Fail::Reach(e) | Fail::Io(e) | Fail::Check(e) => e.clone(),
            Fail::Node(r) => format!("node refused ({}): {}", r.code, r.message),
            Fail::Answered => "answered".into(),
        }
    }
}

struct NodeExec {
    hub: Arc<Hub>,
    node: NodeId,
    client: Option<NodeClient>,
    /// The agent's journal epoch from its last Hello.
    epoch: String,
    /// A status the node returned with a write, submitted after the write's outcome.
    pending_status: Option<(JobId, u32, JobReply)>,
}

fn report(reply: &JobReply) -> JobReport {
    use msg::JobState as N;
    match &reply.job {
        None => JobReport::Unknown {
            epoch: epoch(&reply.journal_epoch),
        },
        Some(info) => match info.state {
            N::Running | N::Cancelling => JobReport::Running {
                offset: info.log_bytes,
            },
            N::Succeeded | N::Failed => JobReport::Exited {
                code: info
                    .exit_code
                    .and_then(|c| i32::try_from(c).ok())
                    .unwrap_or(-1),
            },
            N::Cancelled => JobReport::Cancelled,
            N::Interrupted | N::Lost => JobReport::Interrupted,
        },
    }
}

impl NodeExec {
    fn run(mut self, rx: Receiver<Intent>) {
        for intent in rx {
            self.handle(intent);
        }
    }

    fn connect(&mut self) -> Result<&mut NodeClient, String> {
        if self.client.is_none() {
            let up = self.hub.fleet.wait_up(FLEET_WAIT)?;
            let id = self.node.0;
            let ip = match up.poller.node(&id.to_string()).and_then(|v| v.ip) {
                Some(ip) => ip,
                None => up
                    .fleet
                    .admin()
                    .node(id)
                    .map_err(|e| e.to_string())?
                    .ips
                    .first()
                    .cloned()
                    .ok_or_else(|| format!("headscale lists no address for node {id}"))?,
            };
            let mut c = up.fleet.connect(&ip).map_err(|e| e.to_string())?;
            c.set_timeout(CALL_TIMEOUT).map_err(|e| e.to_string())?;
            let hello = c.hello().map_err(|e| e.to_string())?;
            c.require_compatible().map_err(|e| e.to_string())?;
            self.epoch = hello.journal_epoch;
            self.client = Some(c);
        }
        self.client.as_mut().ok_or_else(|| "no connection".into())
    }

    fn call<T>(
        &mut self,
        what: &str,
        mut f: impl FnMut(&mut NodeClient) -> pt_fleet::Result<T>,
    ) -> Result<T, Fail> {
        let node = self.node.0;
        let c = self
            .connect()
            .map_err(|e| Fail::Reach(format!("node {node}: {e}")))?;
        let result = f(c);
        let disconnected = c.disconnected();
        match result {
            Ok(v) => Ok(v),
            Err(pt_fleet::Error::Node(r)) => Err(Fail::Node(r)),
            Err(e)
                if disconnected
                    && matches!(
                        what,
                        "SubmitJob"
                            | "CancelJob"
                            | "AckCollected"
                            | "JobStatus"
                            | "FetchLog"
                            | "FetchArtifact"
                    ) =>
            {
                // The node retains each submitted job until this executor acknowledges collection.
                self.client = None;
                let before = self.epoch.clone();
                self.connect().map_err(|retry| {
                    Fail::Io(format!("node {node} {what}: {e}; reconnecting: {retry}"))
                })?;
                let after = self.epoch.clone();
                if before != after {
                    self.client = None;
                    return Err(Fail::Io(format!(
                        "node {node} {what}: {e}; journal epoch changed while reconnecting"
                    )));
                }
                let c = self.client.as_mut().ok_or_else(|| {
                    Fail::Io(format!(
                        "node {node} {what}: no connection after reconnecting"
                    ))
                })?;
                match f(c) {
                    Ok(v) => Ok(v),
                    Err(pt_fleet::Error::Node(r)) => Err(Fail::Node(r)),
                    Err(retry) => {
                        self.client = None;
                        Err(Fail::Io(format!("node {node} {what}: {e}; retry: {retry}")))
                    }
                }
            }
            Err(e) => {
                self.client = None;
                Err(Fail::Io(format!("node {node} {what}: {e}")))
            }
        }
    }

    fn submit(&self, kind: EventKind) {
        self.hub.submit(kind);
    }

    fn token(&self, view: &View, job: JobId) -> Option<String> {
        view.state
            .jobs
            .get(&job)
            .and_then(|j| submit_token(&self.hub, j))
    }

    /// Why a status is about another submit than the job's current one, if it is.
    fn stale(&self, view: &View, job: JobId, reply: &JobReply) -> Option<String> {
        let info = reply.job.as_ref()?;
        let want = self.token(view, job);
        (info.token != want).then(|| {
            format!(
                "token {:?}, not this attempt's {:?}",
                info.token.as_deref().unwrap_or("none"),
                want.as_deref().unwrap_or("none")
            )
        })
    }

    fn status(&self, view: &View, job: JobId, attempt: u32, reply: &JobReply) {
        if let Some(why) = self.stale(view, job, reply) {
            return warning!(
                "job {}: ignored a status of attempt {attempt} from node {}: {why}",
                job.0,
                self.node.0
            );
        }
        if let Some(t) = reply.job.as_ref().and_then(|i| i.started_at) {
            self.hub
                .jobs
                .set_started(job, attempt, t.saturating_mul(1000));
        }
        self.submit(EventKind::JobStatus {
            job,
            attempt,
            node: self.node,
            report: report(reply),
        });
    }

    /// Whether the intent still names the job's current attempt on this node.
    fn current(&self, view: &View, job: JobId, attempt: u32) -> bool {
        view.state
            .jobs
            .get(&job)
            .is_some_and(|j| j.attempt == attempt && j.node == Some(self.node))
    }

    fn handle(&mut self, intent: Intent) {
        let view = self.hub.view();
        match intent {
            Intent::Execute {
                effect,
                attempt,
                kind,
            } => {
                if !crate::exec::fresh(&view, effect, attempt, &kind) {
                    return debug!("node {}: dropped stale effect {}", self.node.0, effect.0);
                }
                // Asked, not told: a job cancelled before its submit left takes the submit back.
                let dispatched = EventKind::Dispatched { effect, attempt };
                match self.hub.command(dispatched, DISPATCH_TIMEOUT) {
                    Ok(Reply::Accepted) => {}
                    other => {
                        return debug!(
                            "node {}: effect {} not dispatched: {other:?}",
                            self.node.0, effect.0
                        );
                    }
                }
                let outcome = match kind {
                    EffectKind::NodeSubmit {
                        job, attempt: a, ..
                    } => self.run_job(&view, job, a),
                    EffectKind::NodeCancel {
                        job, attempt: a, ..
                    } => {
                        let r = self.job_ref(&view, job, a);
                        self.call("CancelJob", |c| c.cancel_job(r.clone()))
                            .map(|r| self.pending_status = Some((job, a, r)))
                    }
                    EffectKind::NodeAckCollected {
                        job, attempt: a, ..
                    } => {
                        // Nothing of ours to drop: gone, or another submit's.
                        let r = self.job_ref(&view, job, a);
                        match self.call("AckCollected", |c| c.ack_collected(r.clone())) {
                            Err(Fail::Node(r))
                                if matches!(
                                    r.code,
                                    NodeCode::NotFound | NodeCode::StaleAttempt
                                ) =>
                            {
                                Ok(())
                            }
                            other => other.map(drop),
                        }
                    }
                    other => Err(Fail::Reach(format!("{other:?} is not a node effect"))),
                };
                self.submit(match outcome {
                    Err(Fail::Answered) => return,
                    Ok(()) => EventKind::Acked {
                        effect,
                        attempt,
                        native: None,
                    },
                    Err(Fail::Reach(reason) | Fail::Check(reason)) => EventKind::Nacked {
                        effect,
                        attempt,
                        reason,
                    },
                    Err(Fail::Node(r)) => {
                        let reason = format!("{}: {}", r.code, r.message);
                        if matches!(r.code, NodeCode::Internal) {
                            EventKind::Lost { effect, attempt }
                        } else {
                            EventKind::Nacked {
                                effect,
                                attempt,
                                reason,
                            }
                        }
                    }
                    Err(Fail::Io(e)) => {
                        info!("effect {}: {e}", effect.0);
                        EventKind::Lost { effect, attempt }
                    }
                });
                if let Some((job, a, reply)) = self.pending_status.take() {
                    self.status(&view, job, a, &reply);
                }
            }
            Intent::Probe {
                effect,
                attempt,
                kind,
            } => {
                let probing = view
                    .state
                    .effects
                    .get(&effect)
                    .is_some_and(|e| e.attempt == attempt && e.state == EffectState::Probing);
                let job = match kind {
                    EffectKind::NodeSubmit { job, attempt, .. }
                    | EffectKind::NodeCancel { job, attempt, .. }
                    | EffectKind::NodeAckCollected { job, attempt, .. } => Some((job, attempt)),
                    _ => None,
                };
                if let (true, Some((job, a))) = (probing, job) {
                    self.poll(&view, job, a);
                }
            }
            Intent::PollJob { job, attempt, .. } => {
                if self.current(&view, job, attempt) {
                    self.poll(&view, job, attempt);
                }
            }
            Intent::ReadLog {
                job, attempt, upto, ..
            } => {
                if self.current(&view, job, attempt)
                    && let Err(e) = self.pull_log(&view, job, attempt, Some(upto))
                {
                    debug!("job {}: reading the log: {}", job.0, e.text());
                }
            }
            Intent::FetchArtifacts { job, attempt, .. } => {
                let collecting = view
                    .state
                    .jobs
                    .get(&job)
                    .is_some_and(|j| j.state == JobState::Collecting);
                if collecting && self.current(&view, job, attempt) {
                    self.collect(&view, job, attempt);
                }
            }
            other => debug!("node {}: ignored {other:?}", self.node.0),
        }
    }

    /// The submit. A refusal (someone present, busy, a bad spec) is the node's answer about
    /// the job and goes in as its status; the record's outcome is only for writes that may
    /// have applied or never left.
    fn run_job(&mut self, view: &View, job: JobId, attempt: u32) -> Result<(), Fail> {
        let Some(j) = view.state.jobs.get(&job) else {
            return Err(Fail::Reach(format!("job {} is gone", job.0)));
        };
        let Some(req) = JobRequest::of(&j.spec) else {
            return Err(Fail::Reach(format!(
                "job {}: its request does not parse",
                job.0
            )));
        };
        let spec = SubmitJob {
            job_id: node_job_id(&self.hub, job),
            attempt: u64::from(attempt),
            argv: Some(req.argv.clone()),
            script: None,
            cwd: req.cwd.clone(),
            env: Default::default(),
            timeout_secs: req.timeout_secs,
            log_cap: msg::DEFAULT_LOG_CAP,
            even_if_present: j.spec.even_if_present,
            on_present: match req.on_present {
                api::OnPresent::Idle => OnPresent::Lower,
                api::OnPresent::Kill => OnPresent::Kill,
            },
            token: submit_token(&self.hub, j),
            maintenance: req.maintenance,
        };
        match self.call("SubmitJob", |c| c.submit_job(spec.clone())) {
            Ok(reply) => {
                if let Some(why) = self.stale(view, job, &reply) {
                    // The node already holds another submit under this id; ours never ran.
                    warning!(
                        "job {}: node {} holds another submit of attempt {attempt} ({why}); its status is ignored",
                        job.0,
                        self.node.0
                    );
                    self.submit(EventKind::JobStatus {
                        job,
                        attempt,
                        node: self.node,
                        report: JobReport::Refused { invalid: true },
                    });
                    return Err(Fail::Answered);
                }
                // The node journals before it answers: the submit is confirmed.
                self.pending_status = Some((job, attempt, reply));
                Ok(())
            }
            Err(Fail::Node(r)) => {
                let invalid = match r.code {
                    NodeCode::Present | NodeCode::Busy => false,
                    NodeCode::Internal => return Err(Fail::Node(r)),
                    _ => true,
                };
                info!(
                    "job {}: node {} refused it ({}): {}",
                    job.0, self.node.0, r.code, r.message
                );
                self.submit(EventKind::JobStatus {
                    job,
                    attempt,
                    node: self.node,
                    report: JobReport::Refused { invalid },
                });
                Err(Fail::Answered)
            }
            Err(e) => Err(e),
        }
    }

    fn job_ref(&self, view: &View, job: JobId, attempt: u32) -> JobRef {
        JobRef {
            job_id: node_job_id(&self.hub, job),
            attempt: u64::from(attempt),
            token: self.token(view, job),
        }
    }

    fn poll(&mut self, view: &View, job: JobId, attempt: u32) {
        let nid = node_job_id(&self.hub, job);
        match self.call("JobStatus", |c| {
            c.job_status(&nid, Some(u64::from(attempt)))
        }) {
            Ok(reply) => self.status(view, job, attempt, &reply),
            Err(Fail::Node(r)) if r.code == NodeCode::NotFound => {
                let e = r
                    .journal_epoch
                    .clone()
                    .unwrap_or_else(|| self.epoch.clone());
                self.submit(EventKind::JobStatus {
                    job,
                    attempt,
                    node: self.node,
                    report: JobReport::Unknown { epoch: epoch(&e) },
                });
            }
            Err(e) => debug!("job {}: status: {}", job.0, e.text()),
        }
    }

    /// Appends the node's log to the hub copy, up to `upto` or to its end. True once the node
    /// says every byte has been sent. A node that sends more than it was asked for, or past the
    /// log cap the job was submitted with, fails the check.
    fn pull_log(
        &mut self,
        view: &View,
        job: JobId,
        attempt: u32,
        upto: Option<u64>,
    ) -> Result<bool, Fail> {
        let nid = node_job_id(&self.hub, job);
        let token = self.token(view, job);
        let start = self.hub.jobs.log_len(job, attempt);
        let mut at = start;
        let mut done = false;
        let result = loop {
            if upto.is_some_and(|u| at >= u) {
                break Ok(());
            }
            let max = upto.map_or(LOG_FETCH, |u| (u - at).min(LOG_FETCH));
            let q = FetchLog {
                job_id: nid.clone(),
                attempt: u64::from(attempt),
                offset: at,
                max,
                token: token.clone(),
            };
            let (bytes, end) = match self.call("FetchLog", |c| c.fetch_log(q.clone())) {
                Ok(v) => v,
                Err(e) => break Err(e),
            };
            let len = bytes.len() as u64;
            if len > max || at.saturating_add(len) > msg::DEFAULT_LOG_CAP {
                break Err(Fail::Check(format!(
                    "the node sent {len} log bytes at {at}, asked for {max} of at most {}",
                    msg::DEFAULT_LOG_CAP
                )));
            }
            if !bytes.is_empty() {
                match self.hub.jobs.append(job, attempt, at, &bytes) {
                    Ok(n) => at = n,
                    Err(e) => break Err(Fail::Reach(e.to_string())),
                }
            }
            if end.done {
                done = true;
                break Ok(());
            }
            if bytes.is_empty() {
                break Ok(());
            }
        };
        if at != start {
            stream(&self.hub, job);
        }
        result.map(|()| done)
    }

    /// Exited: the rest of the log, then each artifact, then `ArtifactsChecked`. A connection
    /// failure reports nothing: the lease makes the job Lost and its next status collects again.
    fn collect(&mut self, view: &View, job: JobId, attempt: u32) {
        match self.pull_log(view, job, attempt, None) {
            Ok(_) => {}
            Err(Fail::Check(e)) => {
                warning!("job {}: the log: {e}", job.0);
                return self.submit(EventKind::ArtifactsChecked {
                    job,
                    attempt,
                    ok: false,
                });
            }
            Err(e) => return warning!("job {}: collecting the log: {}", job.0, e.text()),
        }
        let req = view
            .state
            .jobs
            .get(&job)
            .and_then(|j| JobRequest::of(&j.spec));
        let wanted = req
            .as_ref()
            .map(|r| r.artifacts.clone())
            .unwrap_or_default();
        let mut fetched = Vec::new();
        let mut ok = true;
        let token = self.token(view, job);
        for (n, path) in wanted.iter().enumerate() {
            let got = self.fetch_artifact(job, attempt, n, path, token.clone());
            match got {
                Ok(f) => {
                    info!(
                        "job {}: artifact {path}: {} bytes, sha256 {}",
                        job.0, f.size_bytes, f.sha256
                    );
                    fetched.push(f);
                }
                Err(Fail::Io(e) | Fail::Reach(e)) => {
                    return warning!("job {}: fetching {path}: {e}", job.0);
                }
                Err(e) => {
                    warning!("job {}: artifact {path}: {}", job.0, e.text());
                    ok = false;
                }
            }
        }
        if !wanted.is_empty() {
            if let Err(e) = self.hub.jobs.save_fetched(job, attempt, fetched) {
                warning!("job {}: {e}", job.0);
                ok = false;
            }
            stream(&self.hub, job);
        }
        self.submit(EventKind::ArtifactsChecked { job, attempt, ok });
    }

    /// The file from the node's `FetchArtifact`, in slices, checked against the size and
    /// SHA-256 the node read at the end.
    fn fetch_artifact(
        &mut self,
        job: JobId,
        attempt: u32,
        n: usize,
        path: &str,
        token: Option<String>,
    ) -> Result<Fetched, Fail> {
        let nid = node_job_id(&self.hub, job);
        let file = n.to_string();
        let dest = self.hub.jobs.artifact_path(job, attempt, &file);
        let io = |e: io::Error| Fail::Reach(format!("{}: {e}", dest.display()));
        if let Some(d) = dest.parent() {
            fs::create_dir_all(d).map_err(io)?;
        }
        let mut out = File::create(&dest).map_err(io)?;
        let mut sha = pt_hash::Sha256::new();
        let mut at = 0u64;
        // The size the first answer gives: every later slice must agree and fit in it.
        let mut size = None;
        let end = loop {
            let q = FetchArtifact {
                job_id: nid.clone(),
                attempt: u64::from(attempt),
                path: path.to_string(),
                offset: at,
                max: ARTIFACT_FETCH,
                token: token.clone(),
            };
            let (bytes, end) = self.call("FetchArtifact", |c| c.fetch_artifact(q.clone()))?;
            if end.size > ARTIFACT_CAP {
                return Err(Fail::Check(format!(
                    "{} bytes, over the {ARTIFACT_CAP} byte cap",
                    end.size
                )));
            }
            let declared = *size.get_or_insert(end.size);
            let len = bytes.len() as u64;
            if end.size != declared || len > ARTIFACT_FETCH || at.saturating_add(len) > declared {
                return Err(Fail::Check(format!(
                    "the node sent {len} bytes at {at} of {} (first said {declared})",
                    end.size
                )));
            }
            if end.offset != at + len {
                return Err(Fail::Check(format!(
                    "the node sent {} bytes from {at} but says the next fetch starts at {}",
                    bytes.len(),
                    end.offset
                )));
            }
            out.write_all(&bytes).map_err(io)?;
            sha.update(&bytes);
            at = end.offset;
            if end.done {
                break end;
            }
            if bytes.is_empty() {
                return Err(Fail::Check(format!(
                    "no bytes at {at} of {}, and not done",
                    end.size
                )));
            }
        };
        out.sync_all().map_err(io)?;
        let sha = pt_hash::to_hex(&sha.finalize());
        if at != end.size || end.sha256.as_deref() != Some(sha.as_str()) {
            return Err(Fail::Check(format!(
                "got {at} bytes with SHA-256 {sha}; the node read {} bytes with SHA-256 {}",
                end.size,
                end.sha256.as_deref().unwrap_or("none")
            )));
        }
        Ok(Fetched {
            path: path.to_string(),
            file,
            size_bytes: at,
            sha256: sha,
        })
    }
}
