//! What `apply` asks the hub to do. Intents are released only after the event that produced them
//! is fsynced (group commit). Each executor re-checks `g` or `attempt` against the state right
//! before its syscall and drops a stale intent.

use crate::model::*;

/// An external write the hub performs, tracked by an effect record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Provision {
        sleeve: SleeveId,
        layout: Layout,
        claim: Claim,
    },
    StartIncarnation {
        sleeve: SleeveId,
        g: Gen,
        target: Target,
        /// Native session to resume, or `None` for a fresh session.
        resume: Option<String>,
    },
    StopIncarnation {
        sleeve: SleeveId,
        g: Gen,
    },
    KillScope {
        sleeve: SleeveId,
        g: Gen,
    },
    Interrupt {
        sleeve: SleeveId,
        g: Gen,
    },
    SendPrompt {
        sleeve: SleeveId,
        g: Gen,
        request_id: String,
        text: String,
    },
    Steer {
        sleeve: SleeveId,
        g: Gen,
        request_id: String,
        text: String,
        expected_turn: String,
    },
    RespondApproval {
        sleeve: SleeveId,
        g: Gen,
        approval: String,
        allow: bool,
        note: String,
    },
    Compact {
        sleeve: SleeveId,
        g: Gen,
    },
    /// Writes a message into a sleeve's inbox (pane sleeves read it with `protectorate mail inbox`).
    MailDeliver {
        sleeve: SleeveId,
        request_id: String,
        from: Party,
        text: String,
    },
    /// Writes `.protectorate/BRIEF.md` for `to_gen`, keyed by `(sleeve, to_gen)`.
    ContinuationBrief {
        sleeve: SleeveId,
        from_gen: Gen,
        to_gen: Gen,
        forced: bool,
        cause: Cause,
        /// Effects that may have been applied (Uncertain and Fenced records).
        effects: Vec<EffectId>,
        /// Live children.
        roster: Vec<SleeveId>,
    },
    NodeSubmit {
        job: JobId,
        attempt: u32,
        node: NodeId,
        epoch: u64,
    },
    NodeCancel {
        job: JobId,
        attempt: u32,
        node: NodeId,
    },
    /// Lets the node drop its tombstone for `(job, attempt)` (12a item 5).
    NodeAckCollected {
        job: JobId,
        attempt: u32,
        node: NodeId,
    },
}

/// Retry class of an effect kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Repeating it is harmless.
    Idempotent,
    /// The target dedupes by key, so a probe can tell whether it landed and a retry is safe.
    Keyed,
    /// Never retried by any path (12b item 1).
    Unsafe,
}

impl EffectKind {
    pub fn class(&self) -> Class {
        match self {
            EffectKind::Provision { .. }
            | EffectKind::StopIncarnation { .. }
            | EffectKind::KillScope { .. }
            | EffectKind::Interrupt { .. }
            | EffectKind::NodeCancel { .. }
            | EffectKind::NodeAckCollected { .. } => Class::Idempotent,
            EffectKind::StartIncarnation { .. }
            | EffectKind::MailDeliver { .. }
            | EffectKind::ContinuationBrief { .. }
            | EffectKind::NodeSubmit { .. } => Class::Keyed,
            EffectKind::SendPrompt { .. }
            | EffectKind::Steer { .. }
            | EffectKind::RespondApproval { .. }
            | EffectKind::Compact { .. } => Class::Unsafe,
        }
    }

    pub fn sleeve(&self) -> Option<SleeveId> {
        match self {
            EffectKind::Provision { sleeve, .. }
            | EffectKind::StartIncarnation { sleeve, .. }
            | EffectKind::StopIncarnation { sleeve, .. }
            | EffectKind::KillScope { sleeve, .. }
            | EffectKind::Interrupt { sleeve, .. }
            | EffectKind::SendPrompt { sleeve, .. }
            | EffectKind::Steer { sleeve, .. }
            | EffectKind::RespondApproval { sleeve, .. }
            | EffectKind::Compact { sleeve, .. }
            | EffectKind::MailDeliver { sleeve, .. }
            | EffectKind::ContinuationBrief { sleeve, .. } => Some(*sleeve),
            EffectKind::NodeSubmit { .. }
            | EffectKind::NodeCancel { .. }
            | EffectKind::NodeAckCollected { .. } => None,
        }
    }

    /// The incarnation this effect writes to. It is fenced once that incarnation is dead.
    pub fn incarnation(&self) -> Option<(SleeveId, Gen)> {
        match self {
            EffectKind::StopIncarnation { sleeve, g }
            | EffectKind::KillScope { sleeve, g }
            | EffectKind::Interrupt { sleeve, g }
            | EffectKind::SendPrompt { sleeve, g, .. }
            | EffectKind::Steer { sleeve, g, .. }
            | EffectKind::RespondApproval { sleeve, g, .. }
            | EffectKind::Compact { sleeve, g } => Some((*sleeve, *g)),
            _ => None,
        }
    }
}

/// Answer to a command event, released with the event's journal batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Spawned(SleeveId),
    JobCreated(JobId),
    /// The id of a new pending approval.
    ApprovalOpened(String),
    /// Accepted; the result arrives as state changes and notices.
    Accepted,
    Queued,
    /// Sent to the harness as the next prompt.
    Delivering,
    /// Bound to the running turn as a steer.
    Steering,
    /// Written to the sleeve's inbox (pane sleeves).
    Inboxed,
    /// The same request id was seen before with the same content.
    Duplicate,
    Refused(Code, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    InvalidArgument,
    NotFound,
    AlreadyExists,
    FailedPrecondition,
}

/// Something a person or a sleeve should hear about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    MessageRejected {
        request_id: String,
    },
    /// The message's send was fenced or abandoned: it may or may not have reached the harness.
    MessageMayHaveApplied {
        request_id: String,
    },
    EffectUnknowable {
        effect: EffectId,
    },
    /// `approval` is the id in `State::approvals`.
    ApprovalRequested {
        approval: String,
    },
    ApprovalTimedOut {
        approval: String,
    },
    Tripped {
        guardrail: Guardrail,
    },
    Resumed {
        g: Gen,
    },
    Resleeved {
        g: Gen,
    },
    Parked {
        reason: ParkReason,
    },
    Stopped,
    Removed,
    ChildCrashed {
        child: SleeveId,
    },
    ChildParked {
        child: SleeveId,
    },
    ChildStopped {
        child: SleeveId,
    },
    JobInterrupted {
        job: JobId,
    },
    JobLost {
        job: JobId,
    },
    JobFinished {
        job: JobId,
        ok: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Perform the effect record's write (attempt `attempt`), reporting Dispatched first.
    Execute {
        effect: EffectId,
        attempt: u32,
        kind: EffectKind,
    },
    /// Find out whether the effect landed; answer with `Probed` (or `JobStatus` for node kinds).
    Probe {
        effect: EffectId,
        attempt: u32,
        kind: EffectKind,
    },
    /// Report whether scope `pt-<sleeve>-g<g>` has tasks; answer with `ScopeProbed`. Also the
    /// probe of a `StartIncarnation` record.
    ProbeScope {
        sleeve: SleeveId,
        g: Gen,
    },
    /// Run the pure `select_target` and answer with `TargetChosen`.
    SelectTarget {
        sleeve: SleeveId,
        g: Gen,
        cause: Cause,
        candidates: Vec<Target>,
        exclude: Vec<Target>,
    },
    /// Commit a workspace checkpoint (git, git-excluded ref) for the incarnation.
    Checkpoint {
        sleeve: SleeveId,
        g: Gen,
    },
    Notify {
        to: Party,
        sleeve: Option<SleeveId>,
        notice: Notice,
    },
    Reply(Reply),
    /// Run node scheduling (pure, outside the core) and answer with `NodeChosen`.
    ScheduleJob {
        job: JobId,
        exclude: Vec<NodeId>,
    },
    /// Ask the node for `Status(job, attempt)`; answer with `JobStatus`.
    PollJob {
        job: JobId,
        attempt: u32,
        node: NodeId,
    },
    /// The node's log is `upto` bytes long: append to the hub's copy from the length it already
    /// holds, up to `upto`.
    ReadLog {
        job: JobId,
        attempt: u32,
        node: NodeId,
        upto: u64,
    },
    FetchArtifacts {
        job: JobId,
        attempt: u32,
        node: NodeId,
    },
}
