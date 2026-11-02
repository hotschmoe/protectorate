//! The effect record machine (2.c with 12b item 1).

use crate::Core;
use crate::intent::{Class, EffectKind, Intent, Notice};
use crate::model::*;
use crate::state::*;

/// Attempts per record before NotApplied becomes Failed.
pub const MAX_ATTEMPTS: u32 = 3;
/// Unknowable probe answers before the owner is told.
pub const UNKNOWABLE_NOTIFY: u32 = 3;

pub(crate) enum Report {
    Dispatched,
    Acked(Option<String>),
    Nacked,
    Lost,
    Probed(ProbeResult),
}

impl Core<'_> {
    /// Records a new effect in Intent and emits it for execution after the fsync.
    pub(crate) fn new_effect(&mut self, kind: EffectKind, key: String) -> EffectId {
        let id = EffectId(self.st.next_effect);
        self.st.next_effect += 1;
        let class = kind.class();
        self.st.effects.insert(
            id,
            Effect {
                id,
                key,
                kind: kind.clone(),
                class,
                attempt: 1,
                state: EffectState::Intent,
                unknowable: 0,
            },
        );
        self.out.push(Intent::Execute {
            effect: id,
            attempt: 1,
            kind,
        });
        id
    }

    pub(crate) fn effect_state(&self, id: EffectId) -> Option<EffectState> {
        self.st.effects.get(&id).map(|e| e.state)
    }

    fn set_effect(&mut self, id: EffectId, state: EffectState) {
        self.st.effects.get_mut(&id).expect("effect exists").state = state;
    }

    pub(crate) fn effect_report(&mut self, id: EffectId, attempt: u32, report: Report) {
        let Some(e) = self.st.effects.get(&id) else {
            return;
        };
        if e.attempt != attempt {
            return;
        }
        use EffectState as S;
        match (e.state, report) {
            (S::Intent, Report::Dispatched) => self.set_effect(id, S::Sent),
            (S::Intent | S::Sent | S::Uncertain | S::Probing, Report::Acked(native)) => {
                self.set_effect(id, S::Confirmed);
                self.on_effect(id, native);
            }
            (S::Sent, Report::Nacked) => {
                self.set_effect(id, S::Rejected);
                self.on_effect(id, None);
            }
            (S::Sent, Report::Lost) => {
                self.set_effect(id, S::Uncertain);
                self.probe(id);
                self.on_effect(id, None);
            }
            (S::Probing, Report::Probed(ProbeResult::Applied)) => {
                self.set_effect(id, S::Confirmed);
                self.on_effect(id, None);
            }
            (S::Probing, Report::Probed(ProbeResult::NotApplied)) => self.not_applied(id),
            (S::Probing, Report::Probed(ProbeResult::Unknowable)) => self.unknowable(id),
            _ => {}
        }
    }

    /// Confirms a non-terminal effect from evidence other than its own ack (a harness that
    /// started working on the prompt, a scope that has tasks).
    pub(crate) fn evidence(&mut self, id: EffectId) {
        if self.effect_state(id).is_some_and(|s| !s.terminal()) {
            self.set_effect(id, EffectState::Confirmed);
            self.on_effect(id, None);
        }
    }

    fn probe(&mut self, id: EffectId) {
        self.set_effect(id, EffectState::Probing);
        let e = &self.st.effects[&id];
        let intent = match e.kind {
            EffectKind::StartIncarnation { sleeve, g, .. } => Intent::ProbeScope { sleeve, g },
            _ => Intent::Probe {
                effect: id,
                attempt: e.attempt,
                kind: e.kind.clone(),
            },
        };
        self.out.push(intent);
    }

    /// A probe answer for a record whose kind cannot tell "not applied" apart: Unsafe kinds.
    fn unknowable(&mut self, id: EffectId) {
        let e = self.st.effects.get_mut(&id).expect("effect exists");
        e.state = EffectState::Uncertain;
        e.unknowable += 1;
        if e.unknowable == UNKNOWABLE_NOTIFY {
            let sleeve = e.kind.sleeve();
            self.notify(
                Party::Owner,
                sleeve,
                Notice::EffectUnknowable { effect: id },
            );
        }
        self.on_effect(id, None);
    }

    pub(crate) fn not_applied(&mut self, id: EffectId) {
        let e = &self.st.effects[&id];
        if e.class == Class::Unsafe {
            return self.unknowable(id);
        }
        let attempt = e.attempt;
        self.set_effect(id, EffectState::NotApplied);
        if attempt < MAX_ATTEMPTS && self.wanted(id) {
            let e = self.st.effects.get_mut(&id).expect("effect exists");
            e.attempt += 1;
            e.state = EffectState::Intent;
            let intent = Intent::Execute {
                effect: id,
                attempt: e.attempt,
                kind: e.kind.clone(),
            };
            self.out.push(intent);
        } else {
            self.set_effect(id, EffectState::Failed);
        }
        self.on_effect(id, None);
    }

    /// Sends a record that may not have landed again, as its next attempt, up to `limit`
    /// attempts; then it fails and the owner is told. Never for an Unsafe kind.
    pub(crate) fn resend_or_give_up(&mut self, id: EffectId, limit: u32) {
        let e = &self.st.effects[&id];
        if e.class == Class::Unsafe
            || !matches!(e.state, EffectState::Uncertain | EffectState::Probing)
        {
            return;
        }
        if e.attempt >= limit {
            let sleeve = e.kind.sleeve();
            self.set_effect(id, EffectState::Failed);
            self.notify(
                Party::Owner,
                sleeve,
                Notice::EffectUnknowable { effect: id },
            );
            return self.on_effect(id, None);
        }
        let e = self.st.effects.get_mut(&id).expect("effect exists");
        e.attempt += 1;
        e.state = EffectState::Intent;
        let intent = Intent::Execute {
            effect: id,
            attempt: e.attempt,
            kind: e.kind.clone(),
        };
        self.out.push(intent);
        self.on_effect(id, None);
    }

    /// Tick: a record a probe could not settle (Uncertain) is sent again when that is safe and
    /// still wanted, else probed again until the owner has been told (12a item 7, 12b item 1).
    /// Node writes are the job machine's.
    pub(crate) fn tick_effects(&mut self) {
        let ids: Vec<EffectId> = self
            .st
            .effects
            .values()
            .filter(|e| e.state == EffectState::Uncertain && e.kind.sleeve().is_some())
            .map(|e| e.id)
            .collect();
        for id in ids {
            let e = &self.st.effects[&id];
            if e.class != Class::Unsafe && e.attempt < MAX_ATTEMPTS && self.wanted(id) {
                self.resend_or_give_up(id, MAX_ATTEMPTS);
            } else if e.unknowable < UNKNOWABLE_NOTIFY {
                self.probe(id);
                self.on_effect(id, None);
            }
        }
    }

    /// Records a non-terminal Idempotent record as applied: its goal is reached another way.
    pub(crate) fn goal_reached(&mut self, id: EffectId) {
        let e = &self.st.effects[&id];
        if e.class == Class::Idempotent && !e.state.terminal() {
            self.set_effect(id, EffectState::Confirmed);
            self.on_effect(id, None);
        }
    }

    /// Whether the owner of a NotApplied record still wants it performed.
    fn wanted(&self, id: EffectId) -> bool {
        let st = &*self.st;
        match &st.effects[&id].kind {
            EffectKind::Provision { sleeve, .. } => st.sleeves[sleeve].provision == Some(id),
            EffectKind::StartIncarnation { sleeve, g, .. } => {
                let s = &st.sleeves[sleeve];
                s.g == *g
                    && s.proc == Proc::Starting(id)
                    && matches!(s.life, Life::Launching | Life::Resleeving)
            }
            EffectKind::StopIncarnation { sleeve, g }
            | EffectKind::KillScope { sleeve, g }
            | EffectKind::Interrupt { sleeve, g } => {
                let s = &st.sleeves[sleeve];
                s.g == *g && s.proc != Proc::Down
            }
            EffectKind::MailDeliver {
                sleeve, request_id, ..
            } => st.sleeves[sleeve]
                .messages
                .get(request_id)
                .is_some_and(|m| m.state == MsgState::InFlight(id)),
            EffectKind::ContinuationBrief { sleeve, .. } => {
                st.sleeves[sleeve].guard.state == GuardState::Capturing { brief: id }
            }
            EffectKind::NodeSubmit { job, .. } => {
                let j = &st.jobs[job];
                j.submit == Some(id)
                    && matches!(
                        j.state,
                        JobState::Submitting | JobState::SubmitUncertain { .. }
                    )
            }
            EffectKind::NodeCancel { job, .. } => st.jobs[job].state == JobState::Cancelling,
            EffectKind::NodeAckCollected { .. } => true,
            EffectKind::SendPrompt { .. }
            | EffectKind::Steer { .. }
            | EffectKind::RespondApproval { .. }
            | EffectKind::Compact { .. } => false,
        }
    }

    /// The incarnation `(sleeve, g)` is confirmed dead: its outstanding writes are fenced.
    /// Idempotent stop-like kinds count as applied, since their goal is reached.
    pub(crate) fn fence_incarnation(&mut self, sleeve: SleeveId, g: Gen) {
        let ids: Vec<EffectId> = self
            .st
            .effects
            .values()
            .filter(|e| !e.state.terminal() && e.kind.incarnation() == Some((sleeve, g)))
            .map(|e| e.id)
            .collect();
        for id in ids {
            // A message whose prompt never left goes back to the head of the queue.
            let unsent = self.st.effects[&id].state == EffectState::Intent;
            let to = if unsent && self.return_unsent(id) {
                EffectState::Abandoned
            } else if self.st.effects[&id].class == Class::Idempotent {
                EffectState::Confirmed
            } else {
                EffectState::Fenced
            };
            self.set_effect(id, to);
            self.on_effect(id, None);
        }
    }

    /// Takes back a released record before any executor dispatched it: a later `Dispatched`
    /// is refused, so it is never written.
    pub(crate) fn withdraw(&mut self, id: EffectId) {
        if self.effect_state(id) == Some(EffectState::Intent) {
            self.set_effect(id, EffectState::Abandoned);
            self.on_effect(id, None);
        }
    }

    pub(crate) fn abandon_cmd(&mut self, id: EffectId) {
        if self
            .st
            .effects
            .get(&id)
            .is_some_and(|e| e.kind.sleeve().is_none())
        {
            return self.refuse(
                crate::Code::FailedPrecondition,
                "a node write is the job's: abandon or requeue the job",
            );
        }
        // Probing too: a Tick re-probes Uncertain records, and a late answer to an abandoned
        // record is dropped.
        if !matches!(
            self.effect_state(id),
            Some(EffectState::Uncertain | EffectState::Probing)
        ) {
            return self.refuse(
                crate::Code::FailedPrecondition,
                "only an Uncertain effect can be abandoned",
            );
        }
        self.set_effect(id, EffectState::Abandoned);
        self.reply(crate::Reply::Accepted);
        self.on_effect(id, None);
    }

    /// HubRecovered: anything that may have been in flight becomes Uncertain, and every
    /// Uncertain or Probing record is probed (again).
    pub(crate) fn recover_effects(&mut self) {
        let ids: Vec<EffectId> = self
            .st
            .effects
            .values()
            .filter(|e| !e.state.terminal())
            .map(|e| e.id)
            .collect();
        for id in ids {
            if matches!(
                self.effect_state(id),
                Some(EffectState::Intent | EffectState::Sent)
            ) {
                self.set_effect(id, EffectState::Uncertain);
            }
            self.probe(id);
            self.on_effect(id, None);
        }
    }

    /// Lets the machine that owns the record react to its new state.
    fn on_effect(&mut self, id: EffectId, native: Option<String>) {
        let kind = self.st.effects[&id].kind.clone();
        match kind {
            EffectKind::Provision { sleeve, .. } => self.on_provision(sleeve, id),
            EffectKind::StartIncarnation { sleeve, .. } => self.on_start(sleeve, id),
            EffectKind::SendPrompt { sleeve, .. } => self.on_prompt(sleeve, id, native),
            EffectKind::Steer {
                sleeve, request_id, ..
            }
            | EffectKind::MailDeliver {
                sleeve, request_id, ..
            } => self.on_message_effect(sleeve, &request_id, id),
            EffectKind::Compact { sleeve, .. } => self.on_compact(sleeve, id),
            EffectKind::RespondApproval { .. } => self.on_response(id),
            EffectKind::StopIncarnation { .. }
            | EffectKind::KillScope { .. }
            | EffectKind::Interrupt { .. } => {}
            EffectKind::ContinuationBrief { sleeve, .. } => self.on_brief(sleeve, id),
            EffectKind::NodeSubmit { job, .. } => self.on_submit(job, id),
            EffectKind::NodeCancel { .. } | EffectKind::NodeAckCollected { .. } => {}
        }
    }
}
