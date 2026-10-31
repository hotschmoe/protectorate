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
    ApprovalRequested {
        id: String,
    },
    ApprovalResolved {
        id: String,
    },
    TurnSettled {
        end: TurnEnd,
    },
    CompactDone,
    /// Oversize or unparseable frame, or session id mismatch (G14).
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
    RespondApproval {
        sleeve: SleeveId,
        approval: String,
        allow: bool,
        note: String,
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
    },
    Resleeve {
        sleeve: SleeveId,
        target: Option<Target>,
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
    JobStatus {
        job: JobId,
        attempt: u32,
        report: JobReport,
    },
    ArtifactsChecked {
        job: JobId,
        attempt: u32,
        ok: bool,
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
