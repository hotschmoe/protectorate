//! The pure orchestration core: `apply(&mut State, &Event) -> Vec<Intent>`.
//!
//! No I/O, no clocks, no randomness: time arrives in `Event::at`, so replaying the journal of
//! encoded events rebuilds the live state exactly. See README.md for the hub's contract.

#![forbid(unsafe_code)]

pub mod codec;
pub mod event;
pub mod intent;
pub mod model;
pub mod state;

mod approval;
mod effect;
mod guard;
mod job;
mod machine;
mod sleeve;

pub use approval::{ACTION_PREFIX, tool_approval_id};
pub use event::{
    ArtifactCheck, ArtifactRefusal, Ask, Chosen, Event, EventKind, JobReport, JobSpec, Obs,
    SpawnSpec,
};
pub use guard::limits;
pub use intent::{Class, Code, EffectKind, Intent, Notice, Reply};
pub(crate) use machine::Core;
pub use model::*;
pub use sleeve::{name_ok, overlap_refusal};
pub use state::*;

/// Applies one event and returns the intents to execute once the event is durable.
pub fn apply(state: &mut State, event: &Event) -> Vec<Intent> {
    let now = event.at.max(state.now);
    state.now = now;
    let mut core = Core {
        st: state,
        out: Vec::new(),
        now,
    };
    core.handle(&event.kind);
    core.out
}

/// Rebuilds the state from journaled events. The intents are dropped: they were released (or
/// lost with the crash) the first time, and `HubRecovered` re-derives what is still needed.
pub fn replay<'a>(events: impl IntoIterator<Item = &'a Event>) -> State {
    let mut state = State::default();
    for e in events {
        apply(&mut state, e);
    }
    state
}

impl Core<'_> {
    fn handle(&mut self, kind: &EventKind) {
        use EventKind as E;
        match kind {
            E::Spawn(spec) => self.spawn(spec),
            E::Send {
                to,
                request_id,
                from,
                text,
                mode,
                d9,
            } => self.send(*to, request_id, *from, text, *mode, *d9),
            E::Interrupt { sleeve } => self.interrupt_cmd(*sleeve),
            E::Compact { sleeve } => self.compact_cmd(*sleeve),
            E::RespondApproval {
                sleeve,
                approval,
                allow,
                note,
            } => self.respond_cmd(*sleeve, approval, *allow, note),
            E::RequestApproval {
                sleeve,
                ask,
                timeout,
            } => self.request_action(*sleeve, ask, *timeout),
            E::Stop { sleeve } => self.stop_cmd(*sleeve, false),
            E::Remove { sleeve } => self.stop_cmd(*sleeve, true),
            E::Start {
                sleeve,
                target,
                effort,
            } => self.start_cmd(*sleeve, target.as_ref(), effort.as_deref()),
            E::Resleeve {
                sleeve,
                target,
                effort,
            } => self.resleeve_cmd(*sleeve, target.as_ref(), effort.as_deref()),
            E::Abandon { effect } => self.abandon_cmd(*effect),
            E::Viewed { sleeve } => self.viewed_cmd(*sleeve),
            E::CreateJob { request_id, spec } => self.create_job(request_id, spec),
            E::CancelJob { job } => self.cancel_job(*job),
            E::AbandonJob { job } => self.abandon_job(*job),
            E::RequeueJob { job } => self.requeue_job(*job),
            E::JobNodeRemoved { job, node } => self.job_node_removed(*job, *node),

            E::Dispatched { effect, attempt } => {
                // The answer lets an executor that asks write only what is still released.
                let released = self
                    .st
                    .effects
                    .get(effect)
                    .is_some_and(|e| e.attempt == *attempt && e.state == EffectState::Intent);
                self.effect_report(*effect, *attempt, effect::Report::Dispatched);
                if released {
                    self.reply(Reply::Accepted);
                } else {
                    self.refuse(Code::FailedPrecondition, "the effect is no longer released");
                }
            }
            E::Acked {
                effect,
                attempt,
                native,
            } => self.effect_report(*effect, *attempt, effect::Report::Acked(native.clone())),
            E::Nacked {
                effect, attempt, ..
            } => self.effect_report(*effect, *attempt, effect::Report::Nacked),
            E::Lost { effect, attempt } => {
                self.effect_report(*effect, *attempt, effect::Report::Lost)
            }
            E::Probed {
                effect,
                attempt,
                result,
            } => self.effect_report(*effect, *attempt, effect::Report::Probed(*result)),
            E::Launched {
                sleeve,
                g,
                scope,
                pid,
            } => self.launched(*sleeve, *g, scope, *pid),
            E::ScopeProbed { sleeve, g, live } => self.scope_probed(*sleeve, *g, *live),
            E::TargetChosen { sleeve, g, chosen } => self.target_chosen(*sleeve, *g, chosen),
            E::NodeChosen {
                job,
                attempt,
                node,
                epoch,
            } => self.node_chosen(*job, *attempt, *node, *epoch),
            E::JobStatus {
                job,
                attempt,
                node,
                report,
            } => self.job_status(*job, *attempt, *node, report),
            E::ArtifactsChecked {
                job,
                attempt,
                result,
            } => self.artifacts_checked(*job, *attempt, *result),
            E::ArtifactCollection {
                job,
                attempt,
                refusals,
                incomplete,
            } => self.artifact_collection(*job, *attempt, refusals, incomplete.as_deref()),

            E::Observed { sleeve, g, obs } => self.observed(*sleeve, *g, obs),
            E::Exited { sleeve, g, cause } => self.exited(*sleeve, *g, *cause),
            E::Tick => {
                self.tick_effects();
                self.tick_sleeves();
                self.tick_jobs();
                self.tick_approvals();
            }
            E::Continue { sleeve, text } => self.continue_turn(*sleeve, text),
            E::HubRecovered => {
                self.recover_effects();
                self.recover_sleeves();
                self.recover_jobs();
                self.recover_approvals();
            }
        }
    }
}
