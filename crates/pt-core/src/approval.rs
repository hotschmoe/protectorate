//! Approvals as core state (SPEC 3.3): a harness's tool request or a code rule's action
//! approval (D9), requested, decided at most once, timed out from its journaled deadline,
//! cancelled when the request goes away, and orphaned when whoever waits on it is gone.
//!
//! A tool decision goes out as one RespondApproval record for the incarnation that asked, and
//! only while that incarnation is the sleeve's current, live one; otherwise the decision is
//! recorded as Fenced and nothing is written (12a item 4, 12b item 1).

use crate::Core;
use crate::event::Ask;
use crate::guard::limits::APPROVAL_TIMEOUT;
use crate::intent::{Code, EffectKind, Notice, Reply};
use crate::model::*;
use crate::state::*;

/// The id of incarnation `g`'s tool approval `native` (the harness's own id).
pub fn tool_approval_id(sleeve: SleeveId, g: Gen, native: &str) -> String {
    format!("{}:{}:{native}", sleeve.0, g.0)
}

pub const ACTION_PREFIX: &str = "action:";

impl Core<'_> {
    fn approval(&mut self, id: &str) -> &mut Approval {
        self.st.approvals.get_mut(id).expect("approval exists")
    }

    fn decide(&mut self, id: &str, state: ApprovalState, decision: Option<Decision>) {
        let a = self.approval(id);
        debug_assert!(a.state == ApprovalState::Pending && a.decision.is_none());
        a.state = state;
        a.decision = decision;
    }

    /// Incarnation `g` asked (it is current and alive): a new pending tool approval, timing out
    /// with the sleeve's blocked episode (G9).
    pub(crate) fn tool_requested(
        &mut self,
        sid: SleeveId,
        g: Gen,
        native: &str,
        ask: &Option<Ask>,
    ) {
        let id = tool_approval_id(sid, g, native);
        if self.st.approvals.contains_key(&id) {
            return;
        }
        let now = self.now;
        let deadline = self.st.sleeves[&sid]
            .blocked_at
            .saturating_add(APPROVAL_TIMEOUT);
        self.st.approvals.insert(
            id.clone(),
            Approval {
                id: id.clone(),
                sleeve: sid,
                source: Source::Tool {
                    g,
                    native: native.to_string(),
                },
                ask: ask.clone(),
                requested_at: now,
                deadline,
                state: ApprovalState::Pending,
                decision: None,
                orphan: None,
            },
        );
        self.notify(
            Party::Owner,
            Some(sid),
            Notice::ApprovalRequested { approval: id },
        );
    }

    /// RequestApproval: a code rule asks the owner on the sleeve's behalf.
    pub(crate) fn request_action(&mut self, sid: SleeveId, ask: &Ask, timeout: Millis) {
        if !self.existing(sid) {
            return;
        }
        if !self.st.sleeves[&sid].life.live() {
            return self.refuse(Code::FailedPrecondition, "the sleeve is not live");
        }
        let id = format!("{ACTION_PREFIX}{}", self.st.next_approval);
        self.st.next_approval += 1;
        let now = self.now;
        self.st.approvals.insert(
            id.clone(),
            Approval {
                id: id.clone(),
                sleeve: sid,
                source: Source::Action,
                ask: Some(ask.clone()),
                requested_at: now,
                deadline: now.saturating_add(timeout),
                state: ApprovalState::Pending,
                decision: None,
                orphan: None,
            },
        );
        self.notify(
            Party::Owner,
            Some(sid),
            Notice::ApprovalRequested {
                approval: id.clone(),
            },
        );
        self.reply(Reply::ApprovalOpened(id));
    }

    /// RespondApproval: the owner's decision, recorded at most once.
    pub(crate) fn respond_cmd(&mut self, sid: SleeveId, id: &str, allow: bool, note: &str) {
        if !self.existing(sid) {
            return;
        }
        let Some(a) = self.st.approvals.get(id).filter(|a| a.sleeve == sid) else {
            return self.refuse(Code::NotFound, "no such approval");
        };
        if a.state != ApprovalState::Pending {
            let same = a
                .decision
                .is_some_and(|d| d.by == Decider::Owner && d.allow == allow);
            if same {
                return self.reply(Reply::Duplicate);
            }
            return self.refuse(
                Code::FailedPrecondition,
                "that approval is already decided or ended",
            );
        }
        let now = self.now;
        let mut decision = Decision {
            allow,
            by: Decider::Owner,
            at: now,
            response: None,
        };
        let state = match a.source.clone() {
            Source::Action if a.orphan.is_some() => ApprovalState::Abandoned,
            Source::Action => ApprovalState::Decided,
            Source::Tool { g, native } => {
                let s = &self.st.sleeves[&sid];
                let reachable = s.g == g && s.proc == Proc::Up && s.target.mode == Mode::Headless;
                if !reachable {
                    ApprovalState::Fenced
                } else {
                    match self.respond(sid, &native, allow, note) {
                        Some(eff) => {
                            decision.response = Some(eff);
                            ApprovalState::Decided
                        }
                        None => {
                            return self.refuse(
                                Code::FailedPrecondition,
                                "a response to that approval is already sent or may have applied",
                            );
                        }
                    }
                }
            }
        };
        self.decide(id, state, Some(decision));
        self.reply(Reply::Accepted);
    }

    /// The decision of a response to `native` that was, or may yet be, written to the current
    /// incarnation. Another write would answer twice (12b item 1). A Rejected record wrote
    /// nothing, and an Abandoned one is the owner's explicit release.
    fn answered(&self, sid: SleeveId, native: &str) -> Option<bool> {
        let g = self.st.sleeves[&sid].g;
        self.st.effects.values().find_map(|e| match &e.kind {
            EffectKind::RespondApproval {
                sleeve,
                g: eg,
                approval: a,
                allow,
                ..
            } if *sleeve == sid
                && *eg == g
                && a == native
                && !matches!(
                    e.state,
                    EffectState::Rejected | EffectState::Abandoned | EffectState::Failed
                ) =>
            {
                Some(*allow)
            }
            _ => None,
        })
    }

    /// Writes a response to the current incarnation unless one is already outstanding.
    fn respond(
        &mut self,
        sid: SleeveId,
        native: &str,
        allow: bool,
        note: &str,
    ) -> Option<EffectId> {
        if self.answered(sid, native).is_some() {
            return None;
        }
        let g = self.st.sleeves[&sid].g;
        let n = self.st.next_effect;
        Some(self.new_effect(
            EffectKind::RespondApproval {
                sleeve: sid,
                g,
                approval: native.to_string(),
                allow,
                note: note.to_string(),
            },
            format!("approval:{}:{}:{native}:{n}", sid.0, g.0),
        ))
    }

    /// G9: the current incarnation's pending approvals are denied by rule.
    pub(crate) fn deny_timed_out(&mut self, sid: SleeveId, natives: &[String]) {
        let now = self.now;
        let g = self.st.sleeves[&sid].g;
        for native in natives {
            let id = tool_approval_id(sid, g, native);
            if self
                .st
                .approvals
                .get(&id)
                .is_none_or(|a| a.state != ApprovalState::Pending)
            {
                continue;
            }
            let Some(eff) = self.respond(sid, native, false, "approval timed out after 30 minutes")
            else {
                continue;
            };
            let decision = Decision {
                allow: false,
                by: Decider::Rule,
                at: now,
                response: Some(eff),
            };
            self.decide(&id, ApprovalState::Expired, Some(decision));
            self.notify(
                Party::Owner,
                Some(sid),
                Notice::ApprovalTimedOut { approval: id },
            );
        }
    }

    /// The harness withdrew or settled request `native` of incarnation `g`.
    pub(crate) fn tool_withdrawn(&mut self, sid: SleeveId, g: Gen, native: &str) {
        let id = tool_approval_id(sid, g, native);
        if self
            .st
            .approvals
            .get(&id)
            .is_some_and(|a| a.state == ApprovalState::Pending)
        {
            self.decide(&id, ApprovalState::Cancelled, None);
        }
    }

    /// Incarnation `g` settled its turn: every request it still had pending went away.
    pub(crate) fn withdraw_tool_approvals(&mut self, sid: SleeveId, g: Gen) {
        for a in self.st.approvals.values_mut() {
            if a.sleeve == sid && a.state == ApprovalState::Pending && a.source_gen() == Some(g) {
                a.state = ApprovalState::Cancelled;
            }
        }
    }

    /// Incarnation `g` is dead: its pending requests can no longer be answered.
    pub(crate) fn orphan_tool_approvals(&mut self, sid: SleeveId, g: Gen) {
        for a in self.st.approvals.values_mut() {
            if a.sleeve == sid
                && a.state == ApprovalState::Pending
                && a.source_gen() == Some(g)
                && a.orphan.is_none()
            {
                a.orphan = Some(Orphan::IncarnationEnded);
            }
        }
    }

    /// The RespondApproval record `eff` changed: a definitive Nack wrote nothing.
    pub(crate) fn on_response(&mut self, eff: EffectId) {
        let e = &self.st.effects[&eff];
        let EffectKind::RespondApproval {
            sleeve,
            g,
            approval,
            ..
        } = &e.kind
        else {
            return;
        };
        let rejected = e.state == EffectState::Rejected;
        let id = tool_approval_id(*sleeve, *g, approval);
        if let Some(a) = self.st.approvals.get_mut(&id)
            && rejected
            && a.state == ApprovalState::Decided
            && a.decision.is_some_and(|d| d.response == Some(eff))
        {
            a.state = ApprovalState::Fenced;
        }
    }

    /// Tick (and HubRecovered): pending approvals of a sleeve that is no longer live are
    /// cancelled, and those past their journaled deadline time out. A live incarnation's tool
    /// approvals are G9's, which also answers the harness.
    pub(crate) fn tick_approvals(&mut self) {
        let now = self.now;
        let ids: Vec<String> = self
            .st
            .approvals
            .values()
            .filter(|a| a.state == ApprovalState::Pending)
            .map(|a| a.id.clone())
            .collect();
        for id in ids {
            let a = &self.st.approvals[&id];
            let live = self.st.sleeve(a.sleeve).is_some_and(|s| s.life.live());
            if !live {
                self.decide(&id, ApprovalState::Cancelled, None);
                continue;
            }
            if !a.due(now) {
                continue;
            }
            let rule_deny = Some(Decision {
                allow: false,
                by: Decider::Rule,
                at: now,
                response: None,
            });
            match (&a.source, a.orphan) {
                (Source::Action, Some(_)) => self.decide(&id, ApprovalState::Abandoned, None),
                (Source::Action, None) | (Source::Tool { .. }, Some(_)) => {
                    self.decide(&id, ApprovalState::Expired, rule_deny)
                }
                (Source::Tool { .. }, None) => {}
            }
        }
    }

    /// HubRecovered: every API call that waited on an action approval ended with the old
    /// process. Approvals already past their deadline time out now.
    pub(crate) fn recover_approvals(&mut self) {
        for a in self.st.approvals.values_mut() {
            if a.state == ApprovalState::Pending && a.source == Source::Action {
                a.orphan.get_or_insert(Orphan::HubRestarted);
            }
        }
        self.tick_approvals();
    }
}

impl Approval {
    /// Past its deadline. A deadline that saturated at the end of time never comes.
    pub fn due(&self, now: Millis) -> bool {
        self.deadline != Millis::MAX && now >= self.deadline
    }

    /// The incarnation that asked, for a tool approval.
    pub fn source_gen(&self) -> Option<Gen> {
        match self.source {
            Source::Tool { g, .. } => Some(g),
            Source::Action => None,
        }
    }
}
