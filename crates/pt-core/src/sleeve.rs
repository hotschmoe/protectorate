//! Sleeve lifecycle and tree (2.a), activity (2.b) and the per-sleeve message queue.

use std::collections::VecDeque;

use crate::Core;
use crate::event::{Obs, SpawnSpec};
use crate::guard::limits::*;
use crate::intent::{Code, EffectKind, Intent, Notice, Reply};
use crate::model::*;
use crate::state::*;

const BRIEF_PROMPT: &str = "Continue from .protectorate/BRIEF.md: read it first, then carry on \
                            with the task it describes.";

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

impl Guard {
    pub(crate) fn new() -> Guard {
        Guard {
            state: GuardState::Armed,
            resumes: Vec::new(),
            resleeves: Vec::new(),
            strikes: Vec::new(),
            ooms: Vec::new(),
            tried: Vec::new(),
            cause: None,
            forced: false,
            chosen: None,
            since_gen: Gen(0),
        }
    }
}

impl Core<'_> {
    // ----- tree and lifecycle -----

    pub(crate) fn spawn(&mut self, spec: &SpawnSpec) {
        if let Some(&id) = self.st.spawn_requests.get(&spec.request_id) {
            let s = &self.st.sleeves[&id];
            if s.name == spec.name && s.parent == spec.parent && s.claim == spec.claim {
                return self.reply(Reply::Spawned(id));
            }
            return self.refuse(
                Code::InvalidArgument,
                "request id reused for a different spawn",
            );
        }
        if let Err((code, why)) = self.spawn_check(spec) {
            return self.refuse(code, why);
        }
        let id = SleeveId(self.st.next_sleeve);
        self.st.next_sleeve += 1;
        let now = self.now;
        self.st.sleeves.insert(
            id,
            Sleeve {
                id,
                name: spec.name.clone(),
                parent: spec.parent,
                layout: spec.layout,
                claim: spec.claim.clone(),
                target: spec.target.clone(),
                resleeve_to: spec.resleeve_to.clone(),
                budget: spec.budget,
                used: 0,
                disk_limit: spec.disk_limit,
                life: Life::Provisioning,
                g: Gen(0),
                proc: Proc::Down,
                receipt: None,
                session: None,
                activity: Activity::Ended,
                turn: None,
                queue: VecDeque::new(),
                steer_slot: None,
                messages: Default::default(),
                guard: Guard::new(),
                provision: None,
                stop_at: None,
                kill_at: None,
                kill_cause: None,
                probing: false,
                boot_at: now,
                turn_at: now,
                progress_at: now,
                blocked_at: now,
                interrupt_at: now,
                loop_hash: None,
                loop_count: 0,
            },
        );
        self.st.spawn_requests.insert(spec.request_id.clone(), id);
        let eff = self.new_effect(
            EffectKind::Provision {
                sleeve: id,
                layout: spec.layout,
                claim: spec.claim.clone(),
            },
            format!("provision:{}", id.0),
        );
        self.sl(id).provision = Some(eff);
        self.reply(Reply::Spawned(id));
    }

    fn spawn_check(&self, spec: &SpawnSpec) -> Result<(), (Code, &'static str)> {
        use Code::*;
        if !name_ok(&spec.name) {
            return Err((InvalidArgument, "name must be 1-64 of [A-Za-z0-9._-]"));
        }
        if !path_ok(&spec.claim.path) {
            return Err((
                InvalidArgument,
                "claim path must be absolute and normalized",
            ));
        }
        if !spec.target.valid() || !spec.resleeve_to.iter().all(Target::valid) {
            return Err((InvalidArgument, "that harness has no headless mode"));
        }
        let others = || self.st.sleeves.values().filter(|s| s.life != Life::Removed);
        if others().any(|s| s.name == spec.name) {
            return Err((AlreadyExists, "a sleeve has that name"));
        }
        let path = &spec.claim.path;
        let clashes_writable = || {
            others().any(|s| {
                (s.claim.writable || s.parent.is_none()) && paths_overlap(&s.claim.path, path)
            })
        };
        let Some(pid) = spec.parent else {
            if spec.layout != Layout::Root || !spec.claim.writable {
                return Err((InvalidArgument, "a root has a writable Root claim"));
            }
            if clashes_writable() {
                return Err((
                    AlreadyExists,
                    "the directory overlaps a root or writable claim",
                ));
            }
            return Ok(());
        };
        let Some(p) = self.st.sleeve(pid).filter(|p| p.life != Life::Removed) else {
            return Err((NotFound, "no such parent"));
        };
        if p.parent.is_some() {
            return Err((FailedPrecondition, "children are depth 1"));
        }
        if !p.life.live() {
            return Err((FailedPrecondition, "the parent is stopping or stopped"));
        }
        if self.st.live_children(pid) >= MAX_CHILDREN {
            return Err((FailedPrecondition, "the root has 8 live children"));
        }
        match spec.layout {
            Layout::Root => return Err((InvalidArgument, "a child has a Worktree or View layout")),
            Layout::Worktree => {
                if !spec.claim.writable {
                    return Err((InvalidArgument, "a worktree claim is writable"));
                }
                if clashes_writable() {
                    return Err((
                        AlreadyExists,
                        "the worktree overlaps a root or writable claim",
                    ));
                }
            }
            Layout::View => {
                if spec.claim.writable || !path_within(path, &p.claim.path) {
                    return Err((InvalidArgument, "a view is read-only inside the root"));
                }
            }
        }
        if let Some(b) = p.budget {
            let left = b.saturating_sub(p.used);
            if left == 0 || spec.budget.is_some_and(|c| c > left) {
                return Err((
                    FailedPrecondition,
                    "the budget exceeds the root's remainder",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn on_provision(&mut self, sid: SleeveId, id: EffectId) {
        let state = self.effect_state(id).expect("effect exists");
        if !state.terminal() || self.st.sleeves[&sid].provision != Some(id) {
            return;
        }
        self.sl(sid).provision = None;
        match self.st.sleeves[&sid].life {
            Life::Provisioning if state == EffectState::Confirmed => {
                let target = self.st.sleeves[&sid].target.clone();
                self.start_incarnation(sid, target, false);
            }
            Life::Provisioning => self.park(sid, ParkReason::Provision),
            Life::Stopping { .. } => self.try_finalize(sid),
            _ => {}
        }
    }

    /// The only place that emits StartIncarnation (12b item 2). Refuses unless the current
    /// generation is known dead (I5).
    pub(crate) fn start_incarnation(
        &mut self,
        sid: SleeveId,
        target: Target,
        resume: bool,
    ) -> bool {
        let now = self.now;
        let s = self.sl(sid);
        if s.proc != Proc::Down {
            return false;
        }
        let g = Gen(s.g.0 + 1);
        let resume = if resume && s.target.harness == target.harness {
            s.session.clone()
        } else {
            None
        };
        s.g = g;
        s.target = target.clone();
        s.receipt = None;
        s.turn = None;
        s.activity = Activity::Booting;
        s.boot_at = now;
        s.progress_at = now;
        s.stop_at = None;
        s.kill_at = None;
        s.kill_cause = None;
        s.probing = false;
        s.loop_hash = None;
        s.loop_count = 0;
        if s.guard.state != GuardState::Launching {
            s.life = Life::Launching;
        }
        let eff = self.new_effect(
            EffectKind::StartIncarnation {
                sleeve: sid,
                g,
                target,
                resume,
            },
            format!("start:{}:{}", sid.0, g.0),
        );
        self.sl(sid).proc = Proc::Starting(eff);
        true
    }

    /// The current incarnation should go away: the sleeve is stopping, parked or quiescing.
    fn wants_down(s: &Sleeve) -> bool {
        matches!(s.life, Life::Stopping { .. } | Life::Parked(_))
            || matches!(s.guard.state, GuardState::Quiescing { .. })
    }

    pub(crate) fn on_start(&mut self, sid: SleeveId, id: EffectId) {
        if self.st.sleeves[&sid].proc != Proc::Starting(id) {
            return;
        }
        match self.effect_state(id).expect("effect exists") {
            EffectState::Confirmed => {
                self.sl(sid).proc = Proc::Up;
                if Self::wants_down(&self.st.sleeves[&sid]) {
                    self.stop_incarnation(sid);
                }
            }
            EffectState::Rejected | EffectState::Failed => {
                let s = self.sl(sid);
                s.proc = Proc::Down;
                s.activity = Activity::Ended;
                self.start_failed(sid);
            }
            _ => {}
        }
    }

    /// StartIncarnation definitively did not run.
    fn start_failed(&mut self, sid: SleeveId) {
        let s = &self.st.sleeves[&sid];
        match (s.life, s.guard.state) {
            (Life::Stopping { .. }, _) => self.try_finalize(sid),
            (Life::Resleeving, GuardState::Launching) => self.launch_failed(sid),
            (Life::Launching, GuardState::Resuming) => {
                self.sl(sid).life = Life::Recovering;
                self.begin_resleeve(sid, Cause::Guard(Guardrail::StartFailed), None);
            }
            (Life::Launching, _) => {
                self.sl(sid).life = Life::Recovering;
                self.begin_resleeve(sid, Cause::Guard(Guardrail::StartFailed), None);
            }
            _ => {}
        }
    }

    pub(crate) fn launched(&mut self, sid: SleeveId, g: Gen, scope: &str, pid: u32) {
        let Some(s) = self.st.sleeves.get_mut(&sid) else {
            return;
        };
        if s.g != g || s.proc == Proc::Down {
            return;
        }
        s.receipt = Some(Receipt {
            scope: scope.to_string(),
            pid,
        });
        if let Proc::Starting(eff) = s.proc {
            self.evidence(eff);
        }
    }

    pub(crate) fn scope_probed(&mut self, sid: SleeveId, g: Gen, live: bool) {
        let Some(s) = self.st.sleeves.get(&sid) else {
            return;
        };
        if s.g != g {
            return;
        }
        match s.proc {
            Proc::Starting(eff) if self.effect_state(eff) == Some(EffectState::Probing) => {
                if live {
                    // Started without a receipt and we hold no pipes to it: kill, then resume.
                    self.evidence(eff);
                    self.kill(sid, Some(Cause::HubRestart));
                } else {
                    let attempt = self.st.effects[&eff].attempt;
                    self.effect_report(
                        eff,
                        attempt,
                        crate::effect::Report::Probed(ProbeResult::NotApplied),
                    );
                }
            }
            Proc::Up if s.probing => {
                self.sl(sid).probing = false;
                if live {
                    self.kill(sid, Some(Cause::HubRestart));
                } else {
                    // A receipt and no scope: started and gone (12a item 2).
                    let s = self.sl(sid);
                    s.kill_cause.get_or_insert(Cause::HubRestart);
                    self.exited(sid, g, ExitCause::Killed);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn stop_incarnation(&mut self, sid: SleeveId) {
        let now = self.now;
        let s = self.sl(sid);
        if s.proc != Proc::Up || s.stop_at.is_some() {
            return;
        }
        s.stop_at = Some(now);
        let g = s.g;
        self.new_effect(
            EffectKind::StopIncarnation { sleeve: sid, g },
            format!("stop:{}:{}", sid.0, g.0),
        );
    }

    pub(crate) fn kill(&mut self, sid: SleeveId, cause: Option<Cause>) {
        let now = self.now;
        let n = self.st.next_effect;
        let s = self.sl(sid);
        if s.proc == Proc::Down {
            return;
        }
        if s.kill_cause.is_none() {
            s.kill_cause = cause;
        }
        s.kill_at = Some(now);
        let g = s.g;
        self.new_effect(
            EffectKind::KillScope { sleeve: sid, g },
            format!("kill:{}:{}:{n}", sid.0, g.0),
        );
    }

    pub(crate) fn exited(&mut self, sid: SleeveId, g: Gen, cause: ExitCause) {
        let Some(s) = self.st.sleeves.get_mut(&sid) else {
            return;
        };
        if s.g != g || s.proc == Proc::Down {
            return;
        }
        let was = std::mem::replace(&mut s.activity, Activity::Ended);
        let old = std::mem::replace(&mut s.proc, Proc::Down);
        let stop_requested = s.stop_at.is_some();
        let kill_cause = s.kill_cause.take();
        s.turn = None;
        s.stop_at = None;
        s.kill_at = None;
        s.probing = false;
        if let Some(r) = s.steer_slot.take() {
            s.queue.push_front(r.clone());
            s.messages.get_mut(&r).expect("held message").state = MsgState::Queued;
        }
        if let Proc::Starting(eff) = old {
            self.evidence(eff);
        }
        self.fence_incarnation(sid, g);

        let s = &self.st.sleeves[&sid];
        match s.life {
            Life::Stopping { .. } => self.try_finalize(sid),
            Life::Resleeving => match s.guard.state {
                GuardState::Quiescing { .. } => self.capture(sid, !stop_requested),
                GuardState::Launching | GuardState::Briefing { .. } => self.launch_failed(sid),
                _ => {}
            },
            Life::Launching | Life::Running | Life::Recovering => {
                let parent = s.parent;
                self.sl(sid).life = Life::Recovering;
                if let Some(p) = parent {
                    self.notify(
                        Party::Sleeve(p),
                        Some(sid),
                        Notice::ChildCrashed { child: sid },
                    );
                }
                let c = match (kill_cause, cause) {
                    (Some(c), _) => c,
                    (None, ExitCause::Oom) => Cause::Guard(Guardrail::Oom),
                    (None, _) if was == Activity::Booting => Cause::Guard(Guardrail::StartFailed),
                    (None, _) => Cause::Guard(Guardrail::Crash),
                };
                self.after_death(sid, c);
            }
            Life::Provisioning | Life::Parked(_) | Life::Stopped | Life::Removed => {}
        }
    }

    /// Stop (and with `remove`, then Remove) the sleeve and, in the same step, its children.
    pub(crate) fn stop(&mut self, sid: SleeveId, remove: bool) {
        match self.st.sleeves[&sid].life {
            Life::Removed => return,
            Life::Stopped => {
                if remove {
                    self.remove(sid);
                }
                return;
            }
            Life::Stopping { remove: r } => {
                if remove && !r {
                    self.sl(sid).life = Life::Stopping { remove: true };
                    for c in self.st.children(sid) {
                        self.stop(c, true);
                    }
                }
                return;
            }
            _ => {}
        }
        let s = self.sl(sid);
        s.life = Life::Stopping { remove };
        s.guard.state = GuardState::Armed;
        s.guard.chosen = None;
        s.guard.tried.clear();
        self.stop_incarnation(sid);
        for c in self.st.children(sid) {
            self.stop(c, remove);
        }
        self.try_finalize(sid);
    }

    pub(crate) fn try_finalize(&mut self, sid: SleeveId) {
        let s = &self.st.sleeves[&sid];
        let Life::Stopping { remove } = s.life else {
            return;
        };
        if s.proc != Proc::Down || s.provision.is_some() {
            return;
        }
        let children = self.st.children(sid);
        if children
            .iter()
            .any(|c| self.st.sleeves[c].life != Life::Stopped)
        {
            return;
        }
        let parent = s.parent;
        self.sl(sid).life = Life::Stopped;
        self.notify(Party::Owner, Some(sid), Notice::Stopped);
        if let Some(p) = parent {
            self.notify(
                Party::Sleeve(p),
                Some(sid),
                Notice::ChildStopped { child: sid },
            );
        }
        if remove {
            self.remove(sid);
        }
        if let Some(p) = parent {
            self.try_finalize(p);
        }
    }

    /// Stopped to Removed, children first. The claim is released; worktrees stay on disk.
    fn remove(&mut self, sid: SleeveId) {
        for c in self.st.children(sid) {
            self.remove(c);
        }
        self.sl(sid).life = Life::Removed;
        self.notify(Party::Owner, Some(sid), Notice::Removed);
    }

    pub(crate) fn park(&mut self, sid: SleeveId, reason: ParkReason) {
        let s = self.sl(sid);
        s.life = Life::Parked(reason);
        s.guard.state = GuardState::Parked;
        s.guard.chosen = None;
        let parent = s.parent;
        self.stop_incarnation(sid);
        self.notify(Party::Owner, Some(sid), Notice::Parked { reason });
        if let Some(p) = parent {
            self.notify(
                Party::Sleeve(p),
                Some(sid),
                Notice::ChildParked { child: sid },
            );
        }
    }

    pub(crate) fn recover_sleeves(&mut self) {
        let ids: Vec<SleeveId> = self.st.sleeves.keys().copied().collect();
        for sid in ids {
            let s = self.sl(sid);
            if s.proc == Proc::Up {
                s.probing = true;
                let g = s.g;
                self.out.push(Intent::ProbeScope { sleeve: sid, g });
            }
            if self.st.sleeves[&sid].guard.state == GuardState::Selecting {
                self.select(sid);
            }
        }
    }

    // ----- commands -----

    fn existing(&mut self, sid: SleeveId) -> bool {
        let ok = self.st.sleeve(sid).is_some_and(|s| s.life != Life::Removed);
        if !ok {
            self.refuse(Code::NotFound, "no such sleeve");
        }
        ok
    }

    pub(crate) fn stop_cmd(&mut self, sid: SleeveId, remove: bool) {
        if self.existing(sid) {
            self.stop(sid, remove);
            self.reply(Reply::Accepted);
        }
    }

    pub(crate) fn start_cmd(&mut self, sid: SleeveId, target: Option<&Target>) {
        if !self.existing(sid) {
            return;
        }
        let s = &self.st.sleeves[&sid];
        if !matches!(s.life, Life::Parked(_) | Life::Stopped) {
            return self.refuse(
                Code::FailedPrecondition,
                "only a parked or stopped sleeve starts",
            );
        }
        if target.is_some_and(|t| !t.valid()) {
            return self.refuse(Code::InvalidArgument, "that harness has no headless mode");
        }
        if let Some(p) = s.parent {
            if !self.st.sleeves[&p].life.live() {
                return self.refuse(
                    Code::FailedPrecondition,
                    "the parent is stopping or stopped",
                );
            }
            if s.life == Life::Stopped && self.st.live_children(p) >= MAX_CHILDREN {
                return self.refuse(Code::FailedPrecondition, "the root has 8 live children");
            }
        }
        if s.proc != Proc::Down {
            return self.refuse(
                Code::FailedPrecondition,
                "the incarnation is still stopping",
            );
        }
        let target = target.cloned().unwrap_or_else(|| s.target.clone());
        let g = &mut self.sl(sid).guard;
        g.state = GuardState::Armed;
        g.tried.clear();
        g.chosen = None;
        g.cause = None;
        self.start_incarnation(sid, target, true);
        self.reply(Reply::Accepted);
        for c in self.st.children(sid) {
            self.deliver_next(c);
        }
    }

    pub(crate) fn interrupt_cmd(&mut self, sid: SleeveId) {
        if !self.existing(sid) {
            return;
        }
        let s = &self.st.sleeves[&sid];
        let busy = matches!(s.activity, Activity::Working | Activity::Blocked { .. });
        if s.proc != Proc::Up || s.target.mode != Mode::Headless || !busy {
            return self.refuse(Code::FailedPrecondition, "no headless turn to interrupt");
        }
        self.interrupt(sid);
        self.reply(Reply::Accepted);
    }

    pub(crate) fn interrupt(&mut self, sid: SleeveId) {
        let now = self.now;
        let s = self.sl(sid);
        s.activity = Activity::Interrupting;
        s.interrupt_at = now;
        let g = s.g;
        let n = self.st.next_effect;
        self.new_effect(
            EffectKind::Interrupt { sleeve: sid, g },
            format!("interrupt:{}:{}:{n}", sid.0, g.0),
        );
    }

    pub(crate) fn compact_cmd(&mut self, sid: SleeveId) {
        if !self.existing(sid) {
            return;
        }
        if !self.deliverable(sid) {
            return self.refuse(
                Code::FailedPrecondition,
                "compaction needs an idle headless sleeve",
            );
        }
        let s = self.sl(sid);
        s.activity = Activity::Compacting;
        let g = s.g;
        let n = self.st.next_effect;
        self.new_effect(
            EffectKind::Compact { sleeve: sid, g },
            format!("compact:{}:{}:{n}", sid.0, g.0),
        );
        self.reply(Reply::Accepted);
    }

    pub(crate) fn on_compact(&mut self, sid: SleeveId, id: EffectId) {
        let state = self.effect_state(id).expect("effect exists");
        let failed = matches!(
            state,
            EffectState::Rejected | EffectState::Failed | EffectState::Abandoned
        );
        if failed && self.st.sleeves[&sid].activity == Activity::Compacting {
            self.sl(sid).activity = Activity::Idle { seen: true };
            self.settled(sid);
        }
    }

    pub(crate) fn respond_cmd(&mut self, sid: SleeveId, approval: &str, allow: bool, note: &str) {
        if !self.existing(sid) {
            return;
        }
        let s = &self.st.sleeves[&sid];
        let pending = matches!(&s.activity, Activity::Blocked { approvals } if approvals.iter().any(|a| a == approval));
        if s.proc != Proc::Up || s.target.mode != Mode::Headless || !pending {
            return self.refuse(Code::FailedPrecondition, "no such pending approval");
        }
        if let Some(sent) = self.answered(sid, approval) {
            if sent == allow {
                return self.reply(Reply::Duplicate);
            }
            return self.refuse(
                Code::FailedPrecondition,
                "a response to that approval is already sent or may have applied",
            );
        }
        self.respond(sid, approval, allow, note);
        self.reply(Reply::Accepted);
    }

    /// The decision of a response to `approval` that was, or may yet be, written to the current
    /// incarnation. Another write would answer twice (12b item 1). A Rejected record wrote
    /// nothing, and an Abandoned one is the owner's explicit release.
    fn answered(&self, sid: SleeveId, approval: &str) -> Option<bool> {
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
                && a == approval
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

    /// Writes a response unless one is already outstanding; returns whether it did.
    pub(crate) fn respond(
        &mut self,
        sid: SleeveId,
        approval: &str,
        allow: bool,
        note: &str,
    ) -> bool {
        if self.answered(sid, approval).is_some() {
            return false;
        }
        let g = self.st.sleeves[&sid].g;
        let n = self.st.next_effect;
        self.new_effect(
            EffectKind::RespondApproval {
                sleeve: sid,
                g,
                approval: approval.to_string(),
                allow,
                note: note.to_string(),
            },
            format!("approval:{}:{}:{approval}:{n}", sid.0, g.0),
        );
        true
    }

    pub(crate) fn viewed_cmd(&mut self, sid: SleeveId) {
        if self.existing(sid) {
            if let Activity::Idle { seen } = &mut self.sl(sid).activity {
                *seen = true;
            }
            self.reply(Reply::Accepted);
        }
    }

    // ----- messages -----

    pub(crate) fn send(
        &mut self,
        to: SleeveId,
        request_id: &str,
        from: Party,
        text: &str,
        mode: SendMode,
    ) {
        if !self.existing(to) {
            return;
        }
        let s = &self.st.sleeves[&to];
        if let Some(m) = s.messages.get(request_id) {
            if m.from == from && m.text == text && m.mode == mode {
                return self.reply(Reply::Duplicate);
            }
            return self.refuse(
                Code::InvalidArgument,
                "request id reused for a different message",
            );
        }
        if let Party::Sleeve(x) = from
            && self.st.sleeve(x).is_none_or(|x| x.life == Life::Removed)
        {
            return self.refuse(Code::NotFound, "no such sending sleeve");
        }
        if request_id.is_empty() || text.is_empty() {
            return self.refuse(Code::InvalidArgument, "request id and text are required");
        }
        if !s.life.live() {
            return self.refuse(
                Code::FailedPrecondition,
                "the sleeve is stopping or stopped",
            );
        }
        let pane = s.target.mode == Mode::Pane;
        let steers = s.target.harness.steers() && s.life == Life::Running;
        let working = s.proc == Proc::Up
            && s.activity == Activity::Working
            && s.turn.is_some()
            && !self.frozen(to);
        let delivering =
            matches!(s.activity, Activity::Delivering { .. }) && s.steer_slot.is_none();
        let queue_empty = s.queue.is_empty();
        let msg = |state| Message {
            from,
            text: text.to_string(),
            mode,
            state,
        };
        let insert = |core: &mut Self, state| {
            core.sl(to)
                .messages
                .insert(request_id.to_string(), msg(state));
        };

        if pane {
            if mode == SendMode::Steer {
                return self.refuse(Code::FailedPrecondition, "a pane cannot be steered");
            }
            insert(self, MsgState::Queued);
            if self.frozen(to) {
                self.sl(to).queue.push_back(request_id.to_string());
                return self.reply(Reply::Queued);
            }
            self.inbox(to, request_id);
            return self.reply(Reply::Inboxed);
        }
        match mode {
            SendMode::Steer if steers && working => {
                insert(self, MsgState::Queued);
                self.steer(to, request_id);
                self.reply(Reply::Steering);
            }
            SendMode::Steer if steers && delivering => {
                insert(self, MsgState::SteerHeld);
                self.sl(to).steer_slot = Some(request_id.to_string());
                self.reply(Reply::Steering);
            }
            SendMode::Steer if !(self.deliverable(to) && queue_empty) => self.refuse(
                Code::FailedPrecondition,
                "cannot steer now; queue it instead",
            ),
            _ => {
                insert(self, MsgState::Queued);
                self.sl(to).queue.push_back(request_id.to_string());
                self.deliver_next(to);
                let sent = matches!(
                    self.st.sleeves[&to].messages[request_id].state,
                    MsgState::InFlight(_)
                );
                self.reply(if sent {
                    Reply::Delivering
                } else {
                    Reply::Queued
                });
            }
        }
    }

    fn inbox(&mut self, sid: SleeveId, request_id: &str) {
        let m = &self.st.sleeves[&sid].messages[request_id];
        let kind = EffectKind::MailDeliver {
            sleeve: sid,
            request_id: request_id.to_string(),
            from: m.from,
            text: m.text.clone(),
        };
        let eff = self.new_effect(kind, request_id.to_string());
        self.sl(sid)
            .messages
            .get_mut(request_id)
            .expect("message")
            .state = MsgState::InFlight(eff);
    }

    fn steer(&mut self, sid: SleeveId, request_id: &str) {
        let s = &self.st.sleeves[&sid];
        let kind = EffectKind::Steer {
            sleeve: sid,
            g: s.g,
            request_id: request_id.to_string(),
            text: s.messages[request_id].text.clone(),
            expected_turn: s.turn.clone().expect("bound turn"),
        };
        let eff = self.new_effect(kind, request_id.to_string());
        self.sl(sid)
            .messages
            .get_mut(request_id)
            .expect("message")
            .state = MsgState::InFlight(eff);
    }

    /// A Send can go out as the next prompt now.
    pub(crate) fn deliverable(&self, sid: SleeveId) -> bool {
        let s = &self.st.sleeves[&sid];
        s.life == Life::Running
            && s.proc == Proc::Up
            && matches!(s.activity, Activity::Idle { .. })
            && s.target.mode == Mode::Headless
            && matches!(
                s.guard.state,
                GuardState::Armed | GuardState::Cooldown { .. }
            )
            && !self.frozen(sid)
    }

    /// The sleeve's root is parked, which stops every send to its children (12b item 10).
    fn frozen(&self, sid: SleeveId) -> bool {
        self.st.sleeves[&sid]
            .parent
            .is_some_and(|p| matches!(self.st.sleeves[&p].life, Life::Parked(_)))
    }

    /// Dequeues the head message as the one prompt in flight (Delivering, 12b item 3). A pane
    /// sleeve's queue drains into its inbox instead.
    pub(crate) fn deliver_next(&mut self, sid: SleeveId) {
        let s = &self.st.sleeves[&sid];
        if s.target.mode == Mode::Pane && s.life.live() {
            if self.frozen(sid) {
                return;
            }
            while let Some(r) = self.sl(sid).queue.pop_front() {
                self.inbox(sid, &r);
            }
            return;
        }
        if !self.deliverable(sid) {
            return;
        }
        let Some(r) = self.sl(sid).queue.pop_front() else {
            return;
        };
        let s = &self.st.sleeves[&sid];
        let kind = EffectKind::SendPrompt {
            sleeve: sid,
            g: s.g,
            request_id: r.clone(),
            text: s.messages[&r].text.clone(),
        };
        let eff = self.new_effect(kind, r.clone());
        let s = self.sl(sid);
        s.messages.get_mut(&r).expect("queued message").state = MsgState::InFlight(eff);
        s.activity = Activity::Delivering { effect: eff };
    }

    /// Activity just became Idle: let a quiescing resleeve stop the incarnation, else deliver.
    pub(crate) fn settled(&mut self, sid: SleeveId) {
        if matches!(
            self.st.sleeves[&sid].guard.state,
            GuardState::Quiescing { .. }
        ) {
            self.stop_incarnation(sid);
        }
        self.deliver_next(sid);
    }

    /// The held steer goes to the head of the queue when its turn ends first.
    fn return_steer(&mut self, sid: SleeveId) {
        let s = self.sl(sid);
        if let Some(r) = s.steer_slot.take() {
            s.messages.get_mut(&r).expect("held message").state = MsgState::Queued;
            s.queue.push_front(r);
        }
    }

    pub(crate) fn on_prompt(&mut self, sid: SleeveId, id: EffectId, native: Option<String>) {
        if self.st.sleeves[&sid].guard.state == (GuardState::Briefing { effect: id }) {
            return self.on_brief_prompt(sid, id, native);
        }
        let request_id = self.st.effects[&id].key.clone();
        let state = self.effect_state(id).expect("effect exists");
        if let Some(m) = self.sl(sid).messages.get_mut(&request_id)
            && m.state == MsgState::InFlight(id)
        {
            let from = m.from;
            let notice = match state {
                EffectState::Confirmed => {
                    m.state = MsgState::Delivered;
                    None
                }
                EffectState::Rejected | EffectState::Failed => {
                    m.state = MsgState::Rejected;
                    Some(Notice::MessageRejected { request_id })
                }
                EffectState::Fenced | EffectState::Abandoned => {
                    m.state = MsgState::MayHaveApplied;
                    Some(Notice::MessageMayHaveApplied { request_id })
                }
                _ => None,
            };
            if let Some(n) = notice {
                self.notify(from, Some(sid), n);
            }
        }
        // Activity follows the effect's resolution (12a item 3), message or not.
        if self.st.sleeves[&sid].activity != (Activity::Delivering { effect: id }) {
            return;
        }
        match state {
            EffectState::Confirmed => self.begin_turn(sid, native),
            s if s.terminal() => {
                self.sl(sid).activity = Activity::Idle { seen: true };
                self.return_steer(sid);
                self.settled(sid);
            }
            _ => {}
        }
    }

    fn begin_turn(&mut self, sid: SleeveId, native: Option<String>) {
        let now = self.now;
        let s = self.sl(sid);
        s.activity = Activity::Working;
        s.turn = native;
        s.turn_at = now;
        s.progress_at = now;
        let ready = s.turn.is_some() && s.proc == Proc::Up;
        if !ready {
            return;
        }
        if self.frozen(sid) {
            return self.return_steer(sid);
        }
        if let Some(r) = self.sl(sid).steer_slot.take() {
            self.steer(sid, &r);
        }
    }

    fn on_brief_prompt(&mut self, sid: SleeveId, id: EffectId, native: Option<String>) {
        let state = self.effect_state(id).expect("effect exists");
        match state {
            EffectState::Confirmed => {
                let now = self.now;
                let s = self.sl(sid);
                s.life = Life::Running;
                s.guard.state = GuardState::Cooldown {
                    until: now.saturating_add(COOLDOWN),
                };
                s.guard.since_gen = s.g;
                s.guard.tried.clear();
                s.guard.chosen = None;
                s.guard.cause = None;
                let g = s.g;
                let delivering = s.activity == Activity::Delivering { effect: id };
                if delivering {
                    self.begin_turn(sid, native);
                }
                self.notify(Party::Owner, Some(sid), Notice::Resleeved { g });
            }
            EffectState::Rejected
            | EffectState::Failed
            | EffectState::Abandoned
            | EffectState::Uncertain => {
                let s = self.sl(sid);
                if s.activity == (Activity::Delivering { effect: id }) && state.terminal() {
                    s.activity = Activity::Idle { seen: true };
                }
                self.park(sid, ParkReason::BriefUnconfirmed)
            }
            _ => {}
        }
    }

    pub(crate) fn on_message_effect(&mut self, sid: SleeveId, request_id: &str, id: EffectId) {
        let state = self.effect_state(id).expect("effect exists");
        let Some(m) = self.sl(sid).messages.get_mut(request_id) else {
            return;
        };
        if m.state != MsgState::InFlight(id) {
            return;
        }
        let from = m.from;
        let request_id = request_id.to_string();
        match state {
            EffectState::Confirmed => m.state = MsgState::Delivered,
            EffectState::Rejected | EffectState::Failed => {
                m.state = MsgState::Rejected;
                self.notify(from, Some(sid), Notice::MessageRejected { request_id });
            }
            EffectState::Fenced | EffectState::Abandoned => {
                m.state = MsgState::MayHaveApplied;
                self.notify(
                    from,
                    Some(sid),
                    Notice::MessageMayHaveApplied { request_id },
                );
            }
            _ => {}
        }
    }

    // ----- observations -----

    pub(crate) fn observed(&mut self, sid: SleeveId, g: Gen, obs: &Obs) {
        let Some(s) = self.st.sleeves.get(&sid) else {
            return;
        };
        if s.g != g || s.proc == Proc::Down {
            return;
        }
        if let Proc::Starting(eff) = s.proc {
            self.evidence(eff);
        }
        let now = self.now;
        match obs {
            Obs::Ready { session } => self.ready(sid, session),
            Obs::Progress { tool } => {
                self.prompt_evidence(sid);
                let s = self.sl(sid);
                if !matches!(s.activity, Activity::Working | Activity::Blocked { .. }) {
                    return;
                }
                s.progress_at = now;
                if let Some(h) = *tool {
                    if s.loop_hash == Some(h) {
                        s.loop_count += 1;
                    } else {
                        s.loop_hash = Some(h);
                        s.loop_count = 1;
                    }
                    if s.loop_count >= LOOP_REPEATS {
                        s.loop_hash = None;
                        s.loop_count = 0;
                        self.trip(sid, Guardrail::Loop);
                    }
                }
            }
            Obs::ApprovalRequested { id } => {
                self.prompt_evidence(sid);
                let s = self.sl(sid);
                match &mut s.activity {
                    Activity::Working => {
                        s.activity = Activity::Blocked {
                            approvals: vec![id.clone()],
                        };
                        s.blocked_at = now;
                    }
                    Activity::Blocked { approvals } if !approvals.contains(id) => {
                        approvals.push(id.clone())
                    }
                    _ => return,
                }
                self.notify(
                    Party::Owner,
                    Some(sid),
                    Notice::ApprovalRequested {
                        approval: id.clone(),
                    },
                );
            }
            Obs::ApprovalResolved { id } => {
                let s = self.sl(sid);
                if let Activity::Blocked { approvals } = &mut s.activity {
                    approvals.retain(|a| a != id);
                    if approvals.is_empty() {
                        s.activity = Activity::Working;
                    }
                }
            }
            Obs::TurnSettled { end } => {
                // Not evidence for an in-flight prompt: a settle with no turn begun is the
                // previous turn's, and is dropped below.
                let s = self.sl(sid);
                if !matches!(
                    s.activity,
                    Activity::Working | Activity::Blocked { .. } | Activity::Interrupting
                ) {
                    return;
                }
                s.activity = Activity::Idle { seen: false };
                s.turn = None;
                if s.guard.state == GuardState::Interrupting {
                    s.guard.state = GuardState::Armed;
                }
                self.out.push(Intent::Checkpoint { sleeve: sid, g });
                self.return_steer(sid);
                match end {
                    TurnEnd::UsageLimit => self.trip(sid, Guardrail::QuotaExhausted),
                    TurnEnd::AuthFailed => self.trip(sid, Guardrail::AuthFailed),
                    _ => {}
                }
                self.settled(sid);
            }
            Obs::CompactDone => {
                if self.st.sleeves[&sid].activity != Activity::Compacting {
                    return;
                }
                let pending: Vec<EffectId> = self
                    .st
                    .effects
                    .values()
                    .filter(|e| {
                        !e.state.terminal() && e.kind == EffectKind::Compact { sleeve: sid, g }
                    })
                    .map(|e| e.id)
                    .collect();
                // Not this compaction's: its first byte has not left yet.
                if pending
                    .iter()
                    .any(|&e| self.effect_state(e) == Some(EffectState::Intent))
                {
                    return;
                }
                self.sl(sid).activity = Activity::Idle { seen: true };
                for e in pending {
                    self.evidence(e);
                }
                self.settled(sid);
            }
            Obs::Desync { .. } => {
                self.sl(sid).activity = Activity::Unknown;
                self.trip(sid, Guardrail::ProtocolDesync);
            }
            Obs::Screen(v) => self.screen(sid, *v),
            Obs::Usage { tokens } => self.usage(sid, *tokens),
            Obs::Disk { bytes } => {
                if self.st.sleeves[&sid]
                    .disk_limit
                    .is_some_and(|l| *bytes >= l)
                {
                    self.trip(sid, Guardrail::DiskLimit);
                }
            }
            Obs::ForbiddenPath { .. } => self.trip(sid, Guardrail::ForbiddenPath),
        }
    }

    /// Harness output while a prompt is in flight shows the prompt landed, but only once its
    /// first byte has left: output before that is the previous turn's (README, ordering).
    fn prompt_evidence(&mut self, sid: SleeveId) {
        if let Activity::Delivering { effect } = self.st.sleeves[&sid].activity
            && self.effect_state(effect) != Some(EffectState::Intent)
        {
            self.evidence(effect);
        }
    }

    fn ready(&mut self, sid: SleeveId, session: &str) {
        let s = self.sl(sid);
        if s.activity != Activity::Booting {
            return;
        }
        s.session = Some(session.to_string());
        s.activity = Activity::Idle { seen: true };
        match (s.life, s.guard.state) {
            (Life::Launching, g) => {
                s.life = Life::Running;
                if g == GuardState::Resuming {
                    s.guard.state = GuardState::Armed;
                    let g = s.g;
                    self.notify(Party::Owner, Some(sid), Notice::Resumed { g });
                }
                self.settled(sid);
            }
            (Life::Resleeving, GuardState::Launching) => {
                let g = s.g;
                let key = format!("resleeve:{}:{}", s.name, g.0);
                let kind = EffectKind::SendPrompt {
                    sleeve: sid,
                    g,
                    request_id: key.clone(),
                    text: BRIEF_PROMPT.to_string(),
                };
                let eff = self.new_effect(kind, key);
                let s = self.sl(sid);
                s.activity = Activity::Delivering { effect: eff };
                s.guard.state = GuardState::Briefing { effect: eff };
            }
            _ => self.settled(sid),
        }
    }

    fn screen(&mut self, sid: SleeveId, v: Verdict) {
        let now = self.now;
        let s = self.sl(sid);
        if s.target.mode != Mode::Pane || s.activity == Activity::Booting {
            return;
        }
        match v {
            Verdict::Idle => {
                let seen = !matches!(s.activity, Activity::Working | Activity::Blocked { .. });
                if !matches!(s.activity, Activity::Idle { .. }) {
                    s.activity = Activity::Idle { seen };
                    self.settled(sid);
                }
            }
            Verdict::Working => {
                if s.activity != Activity::Working {
                    s.turn_at = now;
                }
                s.activity = Activity::Working;
                s.progress_at = now;
            }
            Verdict::Blocked => {
                if !matches!(s.activity, Activity::Blocked { .. }) {
                    s.activity = Activity::Blocked {
                        approvals: Vec::new(),
                    };
                    s.blocked_at = now;
                }
            }
            Verdict::NoMatch => s.activity = Activity::Unknown,
        }
    }

    /// Usage counts against the sleeve and its root; a child's budget is checked live against
    /// the root's remainder (12b item 10).
    fn usage(&mut self, sid: SleeveId, tokens: u64) {
        let s = self.sl(sid);
        s.used = s.used.saturating_add(tokens);
        let parent = s.parent;
        let mut over = s.budget.is_some_and(|b| s.used >= b);
        if let Some(p) = parent {
            let root = self.sl(p);
            root.used = root.used.saturating_add(tokens);
            let root_over = root.budget.is_some_and(|b| root.used >= b);
            over |= root_over;
            if root_over {
                self.trip(p, Guardrail::Budget);
            }
        }
        if over {
            self.trip(sid, Guardrail::Budget);
        }
    }
}
