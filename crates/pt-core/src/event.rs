//! Everything that can happen to the core. The hub journals each event, encoded by `codec`.

use crate::model::*;

/// One input to [`crate::apply`]. `at` is the hub's clock when the event was taken in; the core
/// clamps it so time never runs backwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub at: Millis,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnSpec {
    pub request_id: String,
    pub name: String,
    pub parent: Option<SleeveId>,
    pub layout: Layout,
    pub claim: Claim,
    pub target: Target,
    /// Reasoning effort asked of the harness; it lasts until a resleeve changes the harness.
    pub effort: Option<String>,
    /// Ordered resleeve targets handed to `select_target` (default `[pi_local]`, chosen by the hub).
    pub resleeve_to: Vec<Target>,
    pub budget: Option<u64>,
    pub disk_limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSpec {
    pub digest: String,
    pub arch: String,
    pub command: String,
    pub even_if_present: bool,
}

/// What a node said about `(job, attempt)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobReport {
    /// Accepted and running, with the log length so far.
    Running {
        offset: u64,
    },
    Exited {
        code: i32,
    },
    /// The node restarted and the process is gone.
    Interrupted,
    Cancelled,
    /// The submit was refused: `invalid` for a bad spec, otherwise busy or unfit.
    Refused {
        invalid: bool,
    },
    /// The node has no record of the job; `epoch` is its journal epoch.
    Unknown {
        epoch: u64,
    },
}

/// What an approval asks, as the owner is shown it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub title: String,
    pub summary: String,
    pub detail: String,
    /// The answers offered; empty means allow once or deny.
    pub options: Vec<Choice>,
}

/// A harness observation, from a headless driver or a pane's screen rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Obs {
    Ready {
        session: String,
    },
    /// Text, a tool call (with its (name, input) hash) or a tool result.
    Progress {
        tool: Option<u64>,
    },
    /// `id` is the harness's own; `ask` is `None` when the request carried no details.
    ApprovalRequested {
        id: String,
        ask: Option<Ask>,
    },
    /// The harness no longer waits on `id`: answered, or withdrawn by the harness.
    ApprovalResolved {
        id: String,
    },
    TurnSettled {
        end: TurnEnd,
    },
    CompactDone,
    /// An oversize frame, or valid JSON that contradicts the protocol state: a session id
    /// mismatch, a reply nobody asked for (G14).
    Desync {
        detail: String,
    },
    Screen(Verdict),
    /// Tokens used since the last report.
    Usage {
        tokens: u64,
    },
    Disk {
        bytes: u64,
    },
    /// The settle-time diff touched a deny glob (G11).
    ForbiddenPath {
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    // Commands from the API (owner, Envoy, agents, sleeves).
    Spawn(SpawnSpec),
    Send {
        to: SleeveId,
        request_id: String,
        from: Party,
        text: String,
        mode: SendMode,
    },
    Interrupt {
        sleeve: SleeveId,
    },
    Compact {
        sleeve: SleeveId,
    },
    /// Decides approval `approval` (its id in `State::approvals`) of `sleeve`.
    RespondApproval {
        sleeve: SleeveId,
        approval: String,
        allow: bool,
        note: String,
    },
    /// A code rule (D9) asks the owner on `sleeve`'s behalf; it times out `timeout` after `at`.
    RequestApproval {
        sleeve: SleeveId,
        ask: Ask,
        timeout: Millis,
    },
    Stop {
        sleeve: SleeveId,
    },
    Remove {
        sleeve: SleeveId,
    },
    Start {
        sleeve: SleeveId,
        target: Option<Target>,
        effort: Option<String>,
    },
    Resleeve {
        sleeve: SleeveId,
        target: Option<Target>,
        effort: Option<String>,
    },
    Abandon {
        effect: EffectId,
    },
    Viewed {
        sleeve: SleeveId,
    },
    CreateJob {
        request_id: String,
        spec: JobSpec,
    },
    CancelJob {
        job: JobId,
    },
    AbandonJob {
        job: JobId,
    },
    RequeueJob {
        job: JobId,
    },

    // Executor reports on effect records.
    Dispatched {
        effect: EffectId,
        attempt: u32,
    },
    Acked {
        effect: EffectId,
        attempt: u32,
        /// The native turn id bound by a prompt ack, when the harness has one.
        native: Option<String>,
    },
    Nacked {
        effect: EffectId,
        attempt: u32,
        reason: String,
    },
    /// Ack timeout or channel lost.
    Lost {
        effect: EffectId,
        attempt: u32,
    },
    Probed {
        effect: EffectId,
        attempt: u32,
        result: ProbeResult,
    },
    /// The launch receipt: spawn returned with this scope and pid.
    Launched {
        sleeve: SleeveId,
        g: Gen,
        scope: String,
        pid: u32,
    },
    ScopeProbed {
        sleeve: SleeveId,
        g: Gen,
        live: bool,
    },
    /// The answer of the pure `select_target` (outside the core) to a `SelectTarget` intent.
    TargetChosen {
        sleeve: SleeveId,
        g: Gen,
        target: Option<Target>,
    },
    NodeChosen {
        job: JobId,
        attempt: u32,
        node: NodeId,
        epoch: u64,
    },
    /// The node a queued job is pinned to is gone from the fleet; a job in any other state is
    /// left alone.
    JobNodeRemoved {
        job: JobId,
        node: NodeId,
    },
    /// `node` is the node that answered: a report from any node but the job's current one is
    /// stale and dropped.
    JobStatus {
        job: JobId,
        attempt: u32,
        node: NodeId,
        report: JobReport,
    },
    ArtifactsChecked {
        job: JobId,
        attempt: u32,
        result: ArtifactCheck,
    },
    ArtifactCollection {
        job: JobId,
        attempt: u32,
        refusals: Vec<ArtifactRefusal>,
        incomplete: Option<String>,
    },

    // Harness and scope observations, fenced by generation.
    Observed {
        sleeve: SleeveId,
        g: Gen,
        obs: Obs,
    },
    /// The incarnation's scope is confirmed empty.
    Exited {
        sleeve: SleeveId,
        g: Gen,
        cause: ExitCause,
    },

    /// The 10 s timer.
    Tick,
    /// The hub restarted and replayed the journal; issued once before any new input.
    HubRecovered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactCheck {
    Verified,
    Retryable,
    Permanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRefusal {
    pub path: String,
    pub reason: String,
}
