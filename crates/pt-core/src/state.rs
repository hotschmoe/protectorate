//! The orchestration state owned by the core thread. Everything is plain data, compared whole by
//! the replay check.

use std::collections::{BTreeMap, VecDeque};

use crate::event::{Ask, JobSpec};
use crate::intent::{Class, EffectKind};
use crate::model::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The latest event time seen.
    pub now: Millis,
    pub next_sleeve: u64,
    pub next_effect: u64,
    pub next_job: u64,
    pub next_approval: u64,
    pub sleeves: BTreeMap<SleeveId, Sleeve>,
    pub effects: BTreeMap<EffectId, Effect>,
    pub jobs: BTreeMap<JobId, Job>,
    /// Spawn request ids, for idempotent retries.
    pub spawn_requests: BTreeMap<String, SleeveId>,
    pub job_requests: BTreeMap<String, JobId>,
    /// Every approval, by id: `<sleeve>:<g>:<native id>` for a tool, `action:<n>` for D9.
    pub approvals: BTreeMap<String, Approval>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sleeve {
    pub id: SleeveId,
    pub name: String,
    pub parent: Option<SleeveId>,
    pub layout: Layout,
    pub claim: Claim,
    /// The current (or next) incarnation's target.
    pub target: Target,
    /// The effort the owner asked for, while the target's harness is the one it was asked of.
    pub effort: Option<String>,
    pub resleeve_to: Vec<Target>,
    pub budget: Option<u64>,
    /// Tokens used; a root's count includes its children's.
    pub used: u64,
    pub disk_limit: Option<u64>,

    pub life: Life,
    /// The current generation; 0 before the first incarnation.
    pub g: Gen,
    pub proc: Proc,
    /// The launch receipt of the current generation.
    pub receipt: Option<Receipt>,
    /// The native session of the latest incarnation, for native resume.
    pub session: Option<String>,
    /// That session has had a turn. Before one, a harness may have no session to reopen, and a
    /// fresh launch loses nothing.
    pub turned: bool,
    pub activity: Activity,
    /// The native turn id bound by the current turn's prompt ack.
    pub turn: Option<String>,
    pub desync_during_turn: bool,
    /// Owner turns begun here. Incremented when a prompt other than a continuation is dequeued.
    pub turn_seq: u64,
    /// Who opened the current owner turn. An agent steer replaces it. A continuation is sent
    /// as this party, so D9 sees the same origin the turn had.
    pub turn_from: Party,
    /// The agent name on that message's `ga:<name>:` request id, when `turn_from` is an agent.
    pub turn_agent: Option<String>,
    /// The owner turn that still owes one continuation, if any. One per turn, across generations.
    /// Set when a native resume follows a headless turn a crash ended, unless that turn was itself
    /// a continuation or an interrupt. Dropped at recovery once that incarnation is idle and armed,
    /// and when a settle of this owner turn is journaled, including while the incarnation is idle.
    /// Only a journaled `Continue` allocates the prompt.
    pub auto_continue: Option<u64>,

    /// Request ids of queued messages, FIFO.
    pub queue: VecDeque<String>,
    /// A steer waiting for the in-flight prompt's ack.
    pub steer_slot: Option<String>,
    /// Every message ever sent to this sleeve, by request id.
    pub messages: BTreeMap<String, Message>,

    pub guard: Guard,
    /// Pending Provision record while the workspace is being laid out.
    pub provision: Option<EffectId>,
    /// When StopIncarnation was sent for the current generation.
    pub stop_at: Option<Millis>,
    /// When KillScope was last sent for the current generation.
    pub kill_at: Option<Millis>,
    /// Why the core killed the current generation, which decides the path after its exit.
    pub kill_cause: Option<Cause>,
    /// A post-restart ProbeScope is outstanding for the current generation.
    pub probing: bool,

    pub boot_at: Millis,
    pub turn_at: Millis,
    pub progress_at: Millis,
    pub blocked_at: Millis,
    pub interrupt_at: Millis,
    pub loop_hash: Option<u64>,
    pub loop_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub scope: String,
    pub pid: u32,
}

/// Sleeve lifecycle (2.a). Generation lives in `Sleeve::g`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Life {
    Provisioning,
    Launching,
    Running,
    Recovering,
    Resleeving,
    Stopping { remove: bool },
    Stopped,
    Parked(ParkReason),
    Removed,
}

impl Life {
    /// Provisioning, Launching, Running, Recovering, Resleeving or Parked.
    pub fn live(self) -> bool {
        !matches!(self, Life::Stopping { .. } | Life::Stopped | Life::Removed)
    }
}

/// Whether the current generation's process tree may exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proc {
    /// Confirmed dead, or proven never started.
    Down,
    /// StartIncarnation is outstanding (no receipt yet).
    Starting(EffectId),
    Up,
}

/// Activity of the current incarnation (2.b, with Delivering from 12b item 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    Booting,
    Idle {
        seen: bool,
    },
    /// One prompt in flight, keyed by its effect (and so its message request id).
    Delivering {
        effect: EffectId,
    },
    Working,
    Blocked {
        approvals: Vec<String>,
    },
    Interrupting,
    Compacting,
    Unknown,
    /// No live incarnation.
    Ended,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub from: Party,
    pub text: String,
    pub mode: SendMode,
    pub state: MsgState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgState {
    Queued,
    /// Held in the steer slot until the in-flight prompt is acked.
    SteerHeld,
    InFlight(EffectId),
    Delivered,
    Rejected,
    /// Fenced or uncertain-then-abandoned: shown to the sender as "may have been applied".
    MayHaveApplied,
}

/// Guardrail and resleeve machine (2.d).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    pub state: GuardState,
    /// Resume starts in the last 10 minutes.
    pub resumes: Vec<Millis>,
    /// Automatic resleeve starts in the last hour.
    pub resleeves: Vec<Millis>,
    /// Interrupt strikes in the last hour.
    pub strikes: Vec<Millis>,
    /// OOM kills in the last hour.
    pub ooms: Vec<Millis>,
    /// Targets tried by the resleeve in progress.
    pub tried: Vec<Target>,
    pub cause: Option<Cause>,
    pub forced: bool,
    /// A target named by a manual resleeve, used instead of `select_target`.
    pub chosen: Option<Target>,
    pub chosen_effort: Option<String>,
    /// The generation the last resleeve landed on; the brief lists Fenced records from here on.
    pub since_gen: Gen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardState {
    Armed,
    Interrupting,
    Resuming,
    Quiescing { deadline: Millis, nudged: bool },
    Capturing { brief: EffectId },
    Selecting,
    Launching,
    Briefing { effect: EffectId },
    Cooldown { until: Millis },
    Parked,
}

/// Effect record (2.c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: EffectId,
    /// The idempotency key: a message request id, `resleeve:<name>:<g>`, or `<kind>:<sleeve>:..`.
    pub key: String,
    pub kind: EffectKind,
    pub class: Class,
    pub attempt: u32,
    pub state: EffectState,
    /// Probes answered Unknowable (or NotApplied for an Unsafe kind).
    pub unknowable: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectState {
    Intent,
    Sent,
    Confirmed,
    Rejected,
    Uncertain,
    Probing,
    NotApplied,
    Failed,
    Fenced,
    Abandoned,
}

impl EffectState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            EffectState::Confirmed
                | EffectState::Rejected
                | EffectState::Failed
                | EffectState::Fenced
                | EffectState::Abandoned
        )
    }
}

/// An approval the owner sees: a harness's tool request, or one a code rule requires (D9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub sleeve: SleeveId,
    pub source: Source,
    pub ask: Option<Ask>,
    pub requested_at: Millis,
    /// When code times it out (saturating).
    pub deadline: Millis,
    pub state: ApprovalState,
    /// Set at most once.
    pub decision: Option<Decision>,
    /// Set while pending once whoever waits on it is gone.
    pub orphan: Option<Orphan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Incarnation `g` asked, under the harness's own id `native`.
    Tool { g: Gen, native: String },
    /// An API call waits on it (D9).
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub allow: bool,
    pub by: Decider,
    pub at: Millis,
    /// The RespondApproval record that carries it to the harness.
    pub response: Option<EffectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decider {
    Owner,
    Rule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalState {
    Pending,
    /// Decided; a tool decision is on its way to the incarnation that asked.
    Decided,
    /// Timed out: denied by rule.
    Expired,
    /// The request went away first (withdrawn by the harness, turn settled, sleeve stopped).
    Cancelled,
    /// Decided, but the incarnation that asked could not be reached: nothing was written.
    Fenced,
    /// An action approval decided or timed out after the call waiting on it ended: nothing done.
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orphan {
    /// The incarnation that asked is dead.
    IncarnationEnded,
    /// The hub restarted, ending the API call that waited.
    HubRestarted,
}

/// Node job (2.e).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub request_id: String,
    pub spec: JobSpec,
    /// 0 until the first submit.
    pub attempt: u32,
    pub node: Option<NodeId>,
    /// The node's journal epoch recorded with the submit.
    pub epoch: u64,
    pub state: JobState,
    pub excluded: Vec<(NodeId, Millis)>,
    pub submit: Option<EffectId>,
    /// The node's log length at its last Running report (not the hub's read position).
    pub offset: u64,
    pub exit: Option<i32>,
    pub verify_failures: u32,
    pub artifact_refusals: Vec<crate::ArtifactRefusal>,
    pub artifact_incomplete: Option<String>,
    /// Last answer from the node about this attempt.
    pub seen_at: Millis,
    pub state_at: Millis,
    /// First time this attempt entered Lost. A later Collecting retry does not move it.
    /// Cleared once the job leaves Lost and Collecting.
    pub lost_at: Option<Millis>,
    /// The node did not know the job and its epoch changed: only an explicit requeue continues.
    pub needs_new_attempt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Submitting,
    SubmitUncertain,
    Running,
    Collecting,
    Succeeded,
    Failed(JobFailure),
    Interrupted,
    Lost,
    Cancelling,
    Cancelled,
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobFailure {
    Invalid,
    Exit(i32),
    Artifact,
    /// The node it was pinned to left the fleet before it ran.
    NodeRemoved(NodeId),
}

impl State {
    pub fn sleeve(&self, id: SleeveId) -> Option<&Sleeve> {
        self.sleeves.get(&id)
    }

    /// Children of `id` that are not Removed.
    pub fn children(&self, id: SleeveId) -> Vec<SleeveId> {
        self.sleeves
            .values()
            .filter(|s| s.parent == Some(id) && s.life != Life::Removed)
            .map(|s| s.id)
            .collect()
    }

    pub fn live_children(&self, id: SleeveId) -> usize {
        self.sleeves
            .values()
            .filter(|s| s.parent == Some(id) && s.life.live())
            .count()
    }
}
