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

/// Sent once, after a native resume of a headless turn a crash ended. Not after an interrupt,
/// and not for a turn this prompt itself started.
const CONTINUE_PROMPT: &str = "The previous turn was interrupted. Continue from where it stopped.";

/// A sleeve name: 1-64 bytes of `[A-Za-z0-9._-]`.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Refusal for the outermost root or writable claim that overlaps `path`. `kind` is
/// `directory` or `worktree`. The caller drops removed sleeves.
pub fn overlap_refusal<'a>(
    sleeves: impl Iterator<Item = &'a Sleeve>,
    path: &str,
    kind: &str,
) -> Option<String> {
    let hit = sleeves
        .filter(|s| (s.claim.writable || s.parent.is_none()) && paths_overlap(&s.claim.path, path))
        .min_by(|a, b| {
            (a.claim.path.len(), a.claim.path.as_str(), a.name.as_str()).cmp(&(
                b.claim.path.len(),
                b.claim.path.as_str(),
                b.name.as_str(),
            ))
        })?;
    Some(format!(
        "the {kind} overlaps sleeve {} ({})",
        hit.name, hit.claim.path
    ))
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
            chosen_effort: None,
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
                effort: spec.effort.clone(),
                resleeve_to: spec.resleeve_to.clone(),
                budget: spec.budget,
                used: 0,
                disk_limit: spec.disk_limit,
                life: Life::Provisioning,
                g: Gen(0),
                proc: Proc::Down,
                receipt: None,
                session: None,
                turned: false,
                activity: Activity::Ended,
                turn: None,
                desync_during_turn: false,
                desync_while_idle: false,
                turn_seq: 0,
                turn_from: Party::Owner,
                turn_agent: None,
                auto_continue: None,
                d9_turn: None,
                foreign: None,
                delivery: None,
                retry_unwritten: false,
                unwritten_prompts: Default::default(),
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
                envoy_mails: Vec::new(),
                parent_mails: Vec::new(),
                d9: false,
            },
        );
        if spec.d9 || spec.parent.is_some_and(|p| self.st.sleeves[&p].d9) {
            self.mark_d9(id);
        }
        if spec.by_envoy
            && let Some(envoy) = self.st.sleeves.keys().copied().find(|&e| self.is_envoy(e))
            && let Some(t) = &mut self.sl(envoy).d9_turn
        {
            t.spawns = t.spawns.saturating_add(1);
        }
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

    fn spawn_check(&self, spec: &SpawnSpec) -> Result<(), (Code, String)> {
        use Code::*;
        let no = |code, why: &str| Err((code, why.to_string()));
        if !name_ok(&spec.name) {
            return no(InvalidArgument, "name must be 1-64 of [A-Za-z0-9._-]");
        }
        if !path_ok(&spec.claim.path) {
            return no(
                InvalidArgument,
                "claim path must be absolute and normalized",
            );
        }
        if let Some(why) = std::iter::once(&spec.target)
            .chain(&spec.resleeve_to)
            .find_map(Target::invalid_reason)
        {
            return Err((InvalidArgument, why));
        }
        let others = || self.st.sleeves.values().filter(|s| s.life != Life::Removed);
        if others().any(|s| s.name == spec.name) {
            return no(AlreadyExists, "a sleeve has that name");
        }
        let path = &spec.claim.path;
        let Some(pid) = spec.parent else {
            if spec.layout != Layout::Root || !spec.claim.writable {
                return no(InvalidArgument, "a root has a writable Root claim");
            }
            if let Some(why) = overlap_refusal(others(), path, "directory") {
                return Err((AlreadyExists, why));
            }
            return Ok(());
        };
        let Some(p) = self.st.sleeve(pid).filter(|p| p.life != Life::Removed) else {
            return no(NotFound, "no such parent");
        };
        if p.parent.is_some() {
            return no(FailedPrecondition, "children are depth 1");
        }
        if !p.life.live() {
            return no(FailedPrecondition, "the parent is stopping or stopped");
        }
        if self.st.live_children(pid) >= MAX_CHILDREN {
            return no(FailedPrecondition, "the root has 8 live children");
        }
        match spec.layout {
            Layout::Root => return no(InvalidArgument, "a child has a Worktree or View layout"),
            Layout::Worktree => {
                if !spec.claim.writable {
                    return no(InvalidArgument, "a worktree claim is writable");
                }
                if let Some(why) = overlap_refusal(others(), path, "worktree") {
                    return Err((AlreadyExists, why));
                }
            }
            Layout::View => {
                if spec.claim.writable || !path_within(path, &p.claim.path) {
                    return no(InvalidArgument, "a view is read-only inside the root");
                }
            }
        }
        if let Some(b) = p.budget {
            let left = b.saturating_sub(p.used);
            if left == 0 || spec.budget.is_some_and(|c| c > left) {
                return no(
                    FailedPrecondition,
                    "the budget exceeds the root's remainder",
                );
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
        let resume = if resume && s.turned && s.target.harness == target.harness {
            s.session.clone()
        } else {
            None
        };
        s.g = g;
        if s.target.harness != target.harness {
            s.effort = None;
        }
        if let Some(effort) = s.guard.chosen_effort.take() {
            s.effort = Some(effort);
        }
        s.target = target.clone();
        s.receipt = None;
        s.turn = None;
        s.foreign = None;
        s.delivery = None;
        s.retry_unwritten = false;
        s.desync_during_turn = false;
        s.desync_while_idle = false;
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
        let Some(s) = self.st.sleeves.get(&sid) else {
            return;
        };
        if s.g != g || s.proc == Proc::Down {
            return;
        }
        if let Some(foreign) = s.foreign.clone() {
            let pending = s.delivery.as_ref().filter(|d| !d.applied).map(|d| d.effect);
            if let Some(effect) = pending
                && self.effect_state(effect) != Some(EffectState::Intent)
            {
                self.account_prompt(sid, effect);
            }
            let s = self.sl(sid);
            s.foreign = None;
            s.activity = foreign.previous;
            s.turn = foreign.native;
        }
        let s = &self.st.sleeves[&sid];
        let delivery_sent = matches!(s.activity, Activity::Delivering { effect } if self.effect_state(effect) != Some(EffectState::Intent));
        // The ack can lose the race with Exited: the watcher reports the scope empty before
        // KillScope is confirmed. A guardrail kill is not HubRestart, so the parent still hears.
        let killed_by_recovery = s.kill_cause == Some(Cause::HubRestart)
            && self
                .st
                .effects
                .values()
                .any(|e| e.kind == EffectKind::KillScope { sleeve: sid, g });
        let s = self.st.sleeves.get_mut(&sid).expect("sleeve exists");
        let was = std::mem::replace(&mut s.activity, Activity::Ended);
        // An interrupt is the owner or a guardrail stopping the turn. `turn` stays set, so it
        // must not count on its own: resuming would continue the turn they stopped.
        let interrupted_turn = was != Activity::Interrupting
            && (s.turn.is_some()
                || s.desync_during_turn
                || delivery_sent
                || matches!(was, Activity::Working | Activity::Blocked { .. }));
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
        // A brief prompt whose first byte left may have landed: once fenced it cannot be
        // confirmed, so the sleeve parks instead of trying another target (12b item 7).
        let brief_sent = match self.st.sleeves[&sid].guard.state {
            GuardState::Briefing { effect } => {
                self.effect_state(effect) != Some(EffectState::Intent)
            }
            _ => false,
        };
        self.fence_incarnation(sid, g);
        self.orphan_tool_approvals(sid, g);

        let s = &self.st.sleeves[&sid];
        match s.life {
            Life::Stopping { .. } => {
                // Fencing just put an unsent continuation back. Stop already dropped the debt.
                self.drop_queued_continuation(sid);
                self.try_finalize(sid);
            }
            Life::Resleeving => {
                self.drop_queued_continuation(sid);
                match self.st.sleeves[&sid].guard.state {
                    GuardState::Quiescing { .. } => self.capture(sid, !stop_requested),
                    GuardState::Briefing { .. } if brief_sent => {
                        self.park(sid, ParkReason::BriefUnconfirmed)
                    }
                    GuardState::Launching | GuardState::Briefing { .. } => self.launch_failed(sid),
                    _ => {}
                }
            }
            Life::Launching | Life::Running | Life::Recovering => {
                let parent = s.parent;
                self.sl(sid).life = Life::Recovering;
                if let Some(p) = parent
                    && !killed_by_recovery
                {
                    self.notify(
                        Party::Sleeve(p),
                        Some(sid),
                        if cause == ExitCause::Oom && kill_cause.is_none() {
                            Notice::ChildOom { child: sid }
                        } else {
                            Notice::ChildCrashed { child: sid }
                        },
                    );
                }
                let c = match (kill_cause, cause) {
                    (Some(c), _) => c,
                    (None, ExitCause::Oom) => Cause::Guard(Guardrail::Oom),
                    (None, ExitCause::Auth) => Cause::Guard(Guardrail::AuthFailed),
                    (None, ExitCause::Quota) => Cause::Guard(Guardrail::QuotaExhausted),
                    (None, _) if was == Activity::Booting => Cause::Guard(Guardrail::StartFailed),
                    (None, _) => Cause::Guard(Guardrail::Crash),
                };
                // Read before after_death: a new incarnation clears both desync flags.
                let was_desync = self.st.sleeves[&sid].desync_during_turn;
                let desync_while_idle = self.st.sleeves[&sid].desync_while_idle;
                // An idle death that parks or resleeves drops a debt with no warning. A
                // working turn still warns. Unknown without a mid-turn desync is that idle
                // death, including a desync that started while idle.
                let idle_death = matches!(was, Activity::Idle { .. })
                    || (was == Activity::Unknown && !was_desync);
                self.after_death(sid, c, idle_death);
                // Idle (and anything past boot) does not carry a continuation into the next
                // incarnation. Booting still does. A working crash may set a fresh debt below.
                // Unknown from an idle desync drops the debt; it does not count as interrupted,
                // so the resume below does not set a fresh one.
                let reached_idle = matches!(
                    was,
                    Activity::Idle { .. }
                        | Activity::Delivering { .. }
                        | Activity::Working
                        | Activity::Blocked { .. }
                        | Activity::Interrupting
                        | Activity::Compacting
                ) || (was == Activity::Unknown
                    && (was_desync || desync_while_idle));
                let had = self.st.sleeves[&sid].auto_continue;
                if reached_idle && had.is_some() {
                    self.sl(sid).auto_continue = None;
                }
                let s = &self.st.sleeves[&sid];
                let resuming = interrupted_turn && s.guard.state == GuardState::Resuming;
                let headless = s.target.mode == Mode::Headless;
                let seq = s.turn_seq;
                if resuming {
                    // One continuation per owner turn, whichever generation sends it.
                    // A fresh session has no history of the turn: the dead incarnation never
                    // acked a prompt, so `StartIncarnation` carried no resume id.
                    let state = (seq > 0).then(|| self.continue_state(sid, seq)).flatten();
                    let native = self.native_resume(sid);
                    match (headless, native, state) {
                        (true, true, None) if seq > 0 => self.sl(sid).auto_continue = Some(seq),
                        (true, true, Some(MsgState::Queued)) => {
                            // Fencing put the unsent prompt back. The next generation sends it.
                        }
                        _ => self.needs_prompt(sid),
                    }
                }
                // Warn only for a debt this crash did not replace. An interrupt already ended
                // the turn, and a working crash warns from the match above when it must.
                if reached_idle
                    && had.is_some()
                    && self.st.sleeves[&sid].auto_continue.is_none()
                    && was != Activity::Interrupting
                    && !interrupted_turn
                {
                    self.needs_prompt(sid);
                }
            }
            // Park already cleared the debt. The unsent continuation fencing just requeued
            // is dropped here, once, with the owner warning.
            Life::Parked(_) => self.drop_queued_continuation(sid),
            Life::Provisioning | Life::Stopped | Life::Removed => {}
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
        let dropped = self.discard_continuation(sid);
        self.stop_incarnation(sid);
        if dropped {
            self.needs_prompt(sid);
        }
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
        self.park_quiet(sid, reason, false);
    }

    /// `quiet` is an idle death that parks: the debt goes, and nobody is told the turn
    /// will not continue. A queued continuation is still dropped.
    pub(crate) fn park_quiet(&mut self, sid: SleeveId, reason: ParkReason, quiet: bool) {
        let s = self.sl(sid);
        s.life = Life::Parked(reason);
        s.guard.state = GuardState::Parked;
        s.guard.chosen = None;
        let parent = s.parent;
        let dropped = self.discard_continuation(sid);
        self.stop_incarnation(sid);
        self.notify(Party::Owner, Some(sid), Notice::Parked { reason });
        if let Some(p) = parent {
            self.notify(
                Party::Sleeve(p),
                Some(sid),
                Notice::ChildParked { child: sid },
            );
        }
        if dropped && !quiet {
            self.needs_prompt(sid);
        }
    }

    pub(crate) fn recover_sleeves(&mut self) {
        let ids: Vec<SleeveId> = self.st.sleeves.keys().copied().collect();
        for sid in ids {
            let s = self.sl(sid);
            // Idle means this incarnation reached Ready. A debt still held means no prompt
            // was sent after that, so the continuation is dropped with no warning. Keeping
            // it would send the prompt on a later start, long after the turn. A resume that
            // is still booting keeps the debt.
            if s.auto_continue.is_some()
                && matches!(s.activity, Activity::Idle { .. })
                && s.guard.state == GuardState::Armed
            {
                s.auto_continue = None;
            }
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

    pub(crate) fn existing(&mut self, sid: SleeveId) -> bool {
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

    pub(crate) fn start_cmd(
        &mut self,
        sid: SleeveId,
        target: Option<&Target>,
        effort: Option<&str>,
    ) {
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
        if let Some(why) = target.and_then(Target::invalid_reason) {
            return self.refuse(Code::InvalidArgument, why);
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
        g.chosen_effort = effort.map(str::to_owned);
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

    pub(crate) fn viewed_cmd(&mut self, sid: SleeveId) {
        if self.existing(sid) {
            if let Activity::Idle { seen } = &mut self.sl(sid).activity {
                *seen = true;
            }
            self.reply(Reply::Accepted);
        }
    }

    // ----- messages -----

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send(
        &mut self,
        to: SleeveId,
        request_id: &str,
        from: Party,
        agent: Option<&str>,
        text: &str,
        mode: SendMode,
        d9: bool,
    ) {
        if !self.existing(to) {
            return;
        }
        // Only an agent has a name.
        let agent = agent.filter(|_| from == Party::Agent);
        let s = &self.st.sleeves[&to];
        if let Some(m) = s.messages.get(request_id) {
            if m.from == from && m.agent.as_deref() == agent && m.text == text && m.mode == mode {
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
        let insert = |core: &mut Self, state| {
            let seq = core.st.next_msg;
            core.st.next_msg += 1;
            core.sl(to).messages.insert(
                request_id.to_string(),
                Message {
                    from,
                    agent: agent.map(str::to_string),
                    kind: MsgKind::Prompt,
                    text: text.to_string(),
                    mode,
                    state,
                    seq,
                },
            );
        };

        if pane {
            if mode == SendMode::Steer {
                return self.refuse(Code::FailedPrecondition, "a pane cannot be steered");
            }
            if !self.mail_ok(to, from, d9) {
                return;
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
                if !self.mail_ok(to, from, d9) {
                    return;
                }
                insert(self, MsgState::Queued);
                self.steer(to, request_id);
                self.reply(Reply::Steering);
            }
            SendMode::Steer if steers && delivering => {
                if !self.mail_ok(to, from, d9) {
                    return;
                }
                insert(self, MsgState::SteerHeld);
                self.sl(to).steer_slot = Some(request_id.to_string());
                self.reply(Reply::Steering);
            }
            SendMode::Steer if !s.target.harness.steers() && !(self.deliverable(to) && queue_empty) => self.refuse(
                Code::FailedPrecondition,
                "this harness cannot steer a running turn; omit --steer to queue the message for its next turn",
            ),
            SendMode::Steer if !(self.deliverable(to) && queue_empty) => self.refuse(
                Code::FailedPrecondition,
                "the sleeve has no steerable turn ready; omit --steer to queue the message for its next turn",
            ),
            _ => {
                if !self.mail_ok(to, from, d9) {
                    return;
                }
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
        let m = &self.st.sleeves[&sid].messages[request_id];
        let (from, agent) = (m.from, m.agent.clone());
        self.taint_d9(sid, from, agent.clone());
        if from == Party::Agent {
            let s = self.sl(sid);
            s.turn_from = Party::Agent;
            s.turn_agent = agent;
        } else if let Party::Sleeve(id) = from
            && self.st.sleeve(id).is_some_and(|s| s.d9)
        {
            let s = self.sl(sid);
            s.turn_from = from;
            s.turn_agent = None;
        }
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

    /// Accepts one bounded mail, or refuses it and says when the window reopens.
    /// Mail to Envoy spends the sender's tree budget, kept on the root. Mail to a parent
    /// who is not Envoy spends that sender's own budget. A `d9` send marks the recipient,
    /// and so does a send from a sleeve that is already marked, so a relay cannot wash the mark.
    fn mail_ok(&mut self, to: SleeveId, from: Party, d9: bool) -> bool {
        if !self.within_mail_bound(to, from) {
            return false;
        }
        let from_marked =
            matches!(from, Party::Sleeve(id) if self.st.sleeve(id).is_some_and(|s| s.d9));
        if d9 || from_marked {
            self.mark_d9(to);
        }
        true
    }

    fn within_mail_bound(&mut self, to: SleeveId, from: Party) -> bool {
        let Party::Sleeve(sender) = from else {
            return true;
        };
        if sender == to {
            return true;
        }
        if self.is_envoy(to) {
            return self.stamp_mail(self.tree_root(sender), true);
        }
        if self.st.sleeve(sender).and_then(|s| s.parent) == Some(to) {
            return self.stamp_mail(sender, false);
        }
        true
    }

    /// `envoy` stamps the tree root's Envoy budget; otherwise the sender's parent budget.
    fn stamp_mail(&mut self, id: SleeveId, envoy: bool) -> bool {
        let now = self.now;
        let stamps = if envoy {
            &self.st.sleeves[&id].envoy_mails
        } else {
            &self.st.sleeves[&id].parent_mails
        };
        let mut n = 0;
        let mut oldest = None;
        for &t in stamps {
            if since(now, t) < ENVOY_MAIL_WINDOW {
                n += 1;
                oldest = Some(oldest.map_or(t, |o: Millis| o.min(t)));
            }
        }
        if n >= ENVOY_MAIL_LIMIT {
            let oldest = oldest.expect("a full window has a stamp");
            let left = ENVOY_MAIL_WINDOW - since(now, oldest);
            let secs = left.div_ceil(SECOND).max(1);
            let why = if envoy {
                format!("too many messages to Envoy from this tree; the window reopens in {secs}s")
            } else {
                format!(
                    "too many messages to the parent from this sleeve; the window reopens in {secs}s"
                )
            };
            self.refuse(Code::FailedPrecondition, why);
            return false;
        }
        let v = if envoy {
            &mut self.sl(id).envoy_mails
        } else {
            &mut self.sl(id).parent_mails
        };
        v.retain(|&t| since(now, t) < ENVOY_MAIL_WINDOW);
        v.push(now);
        true
    }

    fn tree_root(&self, mut id: SleeveId) -> SleeveId {
        while let Some(p) = self.st.sleeves[&id].parent {
            id = p;
        }
        id
    }

    /// Marks `id` and its children. Envoy is never marked.
    fn mark_d9(&mut self, id: SleeveId) {
        if self.is_envoy(id) {
            return;
        }
        self.sl(id).d9 = true;
        let kids = self.st.children(id);
        for k in kids {
            if !self.is_envoy(k) {
                self.sl(k).d9 = true;
            }
        }
    }

    fn is_envoy(&self, id: SleeveId) -> bool {
        self.st
            .sleeve(id)
            .is_some_and(|s| s.name == ENVOY_NAME && s.parent.is_none() && s.life != Life::Removed)
    }

    /// A Send can go out as the next prompt now.
    pub(crate) fn deliverable(&self, sid: SleeveId) -> bool {
        let s = &self.st.sleeves[&sid];
        // After a restart, nothing goes to an incarnation whose scope is not yet probed.
        s.life == Life::Running
            && s.proc == Proc::Up
            && !s.probing
            && !s.retry_unwritten
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
        let m = &s.messages[&r];
        let msg_kind = m.kind;
        let kind = EffectKind::SendPrompt {
            sleeve: sid,
            g: s.g,
            request_id: r.clone(),
            kind: msg_kind,
            text: m.text.clone(),
        };
        let eff = self.new_effect(kind, r.clone());
        let s = self.sl(sid);
        let m = s.messages.get_mut(&r).expect("queued message");
        m.state = MsgState::InFlight(eff);
        s.activity = Activity::Delivering { effect: eff };
        // A continuation belongs to the turn already begun. Any other prompt starts the next one
        // and supersedes a continuation that turn still owed.
        match msg_kind {
            MsgKind::Prompt => self.account_prompt(sid, eff),
            MsgKind::Continuation { .. } => {
                let m = &self.st.sleeves[&sid].messages[&r];
                self.taint_d9(sid, m.from, m.agent.clone());
            }
            MsgKind::Brief => {}
        }
    }

    fn account_prompt(&mut self, sid: SleeveId, effect: EffectId) {
        let s = &self.st.sleeves[&sid];
        let m = &s.messages[&self.st.effects[&effect].key];
        let (from, agent) = (m.from, m.agent.clone());
        let saved = Delivery {
            effect,
            applied: true,
            turn_seq: s.turn_seq,
            auto_continue: s.auto_continue,
            turn_from: s.turn_from,
            turn_agent: s.turn_agent.clone(),
            d9_turn: s.d9_turn.clone(),
        };
        let d9 = self.d9_origin(sid, from, agent.clone());
        let s = self.sl(sid);
        s.delivery = Some(saved);
        s.turn_seq += 1;
        s.auto_continue = None;
        s.turn_from = from;
        s.turn_agent = agent;
        s.d9_turn = d9;
    }

    pub(crate) fn unaccount_prompt(&mut self, sid: SleeveId, effect: EffectId) {
        let s = self.sl(sid);
        let Some(saved) = s
            .delivery
            .as_mut()
            .filter(|d| d.effect == effect && d.applied)
        else {
            return;
        };
        s.turn_seq = saved.turn_seq;
        s.auto_continue = saved.auto_continue;
        s.turn_from = saved.turn_from;
        s.turn_agent.clone_from(&saved.turn_agent);
        s.d9_turn.clone_from(&saved.d9_turn);
        saved.applied = false;
    }

    pub(crate) fn retry_unwritten(&mut self, sid: SleeveId, effect: EffectId) {
        self.unaccount_prompt(sid, effect);
        let s = self.sl(sid);
        if s.delivery.as_ref().is_some_and(|d| d.effect == effect) {
            s.delivery = None;
        }
        if let Some(foreign) = &mut s.foreign {
            if foreign.previous == (Activity::Delivering { effect }) {
                foreign.previous = Activity::Idle { seen: true };
            }
        } else if s.activity == (Activity::Delivering { effect }) {
            s.activity = Activity::Idle { seen: true };
        }
        s.retry_unwritten = true;
    }

    pub(crate) fn tick_unwritten(&mut self) {
        let ids: Vec<_> = self
            .st
            .sleeves
            .values()
            .filter(|s| s.retry_unwritten && s.foreign.is_none())
            .map(|s| s.id)
            .collect();
        for sid in ids {
            self.sl(sid).retry_unwritten = false;
            let s = &self.st.sleeves[&sid];
            if let GuardState::Briefing { effect } = s.guard.state
                && s.activity == (Activity::Delivering { effect })
                && let Some(e) = self.st.effects.get(&effect)
                && e.state == EffectState::Intent
            {
                self.out.push(Intent::Execute {
                    effect,
                    attempt: e.attempt,
                    kind: e.kind.clone(),
                });
            } else {
                self.deliver_next(sid);
            }
        }
    }

    /// Envoy's turn has D9 origin when a vendor agent or a D9-marked sleeve gave it a prompt.
    fn d9_origin(&self, sid: SleeveId, from: Party, agent: Option<String>) -> Option<D9Turn> {
        let d9 = match from {
            Party::Agent => true,
            Party::Sleeve(x) => self.st.sleeve(x).is_some_and(|s| s.d9),
            Party::Owner | Party::Envoy => false,
        };
        (d9 && self.is_envoy(sid)).then_some(D9Turn {
            from,
            agent,
            spawns: 0,
        })
    }

    /// A steer, or a continuation, from such a party replaces the origin and keeps the count.
    fn taint_d9(&mut self, sid: SleeveId, from: Party, agent: Option<String>) {
        let Some(mut t) = self.d9_origin(sid, from, agent) else {
            return;
        };
        let s = self.sl(sid);
        t.spawns = s.d9_turn.as_ref().map_or(0, |old| old.spawns);
        s.d9_turn = Some(t);
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

    /// The message of a prompt or steer record that was never dispatched goes back to the head
    /// of its queue, for the next incarnation; true when there was one.
    pub(crate) fn return_unsent(&mut self, id: EffectId) -> bool {
        let (EffectKind::SendPrompt {
            sleeve, request_id, ..
        }
        | EffectKind::Steer {
            sleeve, request_id, ..
        }) = self.st.effects[&id].kind.clone()
        else {
            return false;
        };
        if self.st.sleeves[&sleeve]
            .messages
            .get(&request_id)
            .map(|m| m.state)
            != Some(MsgState::InFlight(id))
        {
            return false;
        }
        if matches!(
            self.st.effects[&id].kind,
            EffectKind::SendPrompt {
                kind: MsgKind::Prompt,
                ..
            }
        ) && self.st.sleeves[&sleeve]
            .unwritten_prompts
            .contains(&request_id)
        {
            self.unaccount_prompt(sleeve, id);
        }
        // A steer held for this prompt's turn keeps its place behind it.
        self.return_steer(sleeve);
        let s = self.sl(sleeve);
        s.messages.get_mut(&request_id).expect("message").state = MsgState::Queued;
        s.queue.push_front(request_id);
        true
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
        let continued = matches!(
            self.st.effects[&id].kind,
            EffectKind::SendPrompt {
                kind: MsgKind::Continuation { .. },
                ..
            }
        );
        let state = self.effect_state(id).expect("effect exists");
        if state.terminal()
            && (state != EffectState::Abandoned
                || self.st.sleeves[&sid]
                    .messages
                    .get(&request_id)
                    .is_none_or(|m| m.state != MsgState::Queued))
        {
            self.sl(sid).unwritten_prompts.remove(&request_id);
        }
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
        if continued && matches!(state, EffectState::Rejected | EffectState::Failed) {
            self.needs_prompt(sid);
        }
        if let Some(foreign) = &mut self.sl(sid).foreign
            && foreign.previous == (Activity::Delivering { effect: id })
        {
            match state {
                EffectState::Confirmed => {
                    foreign.previous = Activity::Working;
                    foreign.native = native;
                }
                s if s.terminal() => foreign.previous = Activity::Idle { seen: true },
                _ => {}
            }
            if state.terminal() && state != EffectState::Confirmed {
                self.sl(sid).delivery = None;
            }
            return;
        }
        if state.terminal()
            && self.st.sleeves[&sid]
                .delivery
                .as_ref()
                .is_some_and(|d| d.effect == id)
        {
            self.sl(sid).delivery = None;
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
        s.turned = true;
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
                if let Some(foreign) = &mut s.foreign
                    && foreign.previous == (Activity::Delivering { effect: id })
                {
                    foreign.previous = Activity::Working;
                    foreign.native = native.clone();
                }
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
            Obs::TurnBegan => self.foreign_began(sid),
            Obs::ForeignTurnSettled { end } => self.foreign_settled(sid, g, *end),
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
            Obs::ApprovalRequested { id, ask } => {
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
                self.tool_requested(sid, g, id, ask);
            }
            Obs::ApprovalResolved { id } => {
                let s = self.sl(sid);
                if let Activity::Blocked { approvals } = &mut s.activity {
                    approvals.retain(|a| a != id);
                    if approvals.is_empty() {
                        s.activity = Activity::Working;
                    }
                }
                self.tool_withdrawn(sid, g, id);
            }
            Obs::TurnSettled { end } => {
                // A usage limit or a rejected login resleeves, and the brief is sent as the
                // owner, so the D9 origin and the spawn count must outlive it.
                if !matches!(end, TurnEnd::UsageLimit | TurnEnd::AuthFailed) {
                    self.sl(sid).d9_turn = None;
                }
                // Not evidence for an in-flight prompt: a settle with no turn begun is the
                // previous turn's. It still retires a continuation owed for this owner turn,
                // including one recorded after resume while this incarnation is already idle,
                // so a later hub start does not warn about a turn the journal already settled.
                let s = self.sl(sid);
                if !matches!(
                    s.activity,
                    Activity::Working | Activity::Blocked { .. } | Activity::Interrupting
                ) {
                    if s.auto_continue == Some(s.turn_seq) {
                        s.auto_continue = None;
                    }
                    return;
                }
                s.activity = Activity::Idle { seen: false };
                s.turn = None;
                if s.auto_continue == Some(s.turn_seq) {
                    s.auto_continue = None;
                }
                // Whatever the harness still asked in this turn went away with it.
                self.withdraw_tool_approvals(sid, g);
                let s = self.sl(sid);
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
                let s = &self.st.sleeves[&sid];
                // An interrupt already stopped this turn. Counting it as active would set
                // desync_during_turn, the kill would leave activity Unknown, and resume
                // would continue the turn the owner stopped. `turn` is cleared for the
                // same reason: it stays set across the interrupt.
                let interrupting = s.activity == Activity::Interrupting;
                let owner_activity = s.foreign.as_ref().map_or(&s.activity, |f| &f.previous);
                let idle = matches!(owner_activity, Activity::Idle { .. });
                let active = !interrupting
                    && (matches!(owner_activity, Activity::Working | Activity::Blocked { .. })
                        || matches!(owner_activity, Activity::Delivering { effect } if self.effect_state(*effect) != Some(EffectState::Intent)));
                let s = self.sl(sid);
                s.desync_during_turn |= active;
                // Separate from the mid-turn flag: resume must not treat this as interrupted
                // and set a fresh debt. The exit drops the one already held.
                if idle {
                    s.desync_while_idle = true;
                }
                if interrupting {
                    s.turn = None;
                }
                s.activity = Activity::Unknown;
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

    fn foreign_began(&mut self, sid: SleeveId) {
        if self.st.sleeves[&sid].foreign.is_some() {
            return;
        }
        if let Activity::Delivering { effect } = self.st.sleeves[&sid].activity {
            self.unaccount_prompt(sid, effect);
        }
        let now = self.now;
        let s = self.sl(sid);
        let previous = std::mem::replace(&mut s.activity, Activity::Working);
        let native = s.turn.take();
        s.foreign = Some(ForeignTurn { previous, native });
        s.turned = true;
        s.turn_at = now;
        s.progress_at = now;
    }

    fn foreign_settled(&mut self, sid: SleeveId, g: Gen, end: TurnEnd) {
        let Some(foreign) = self.sl(sid).foreign.take() else {
            return;
        };
        self.withdraw_tool_approvals(sid, g);
        let delivery = self.st.sleeves[&sid]
            .delivery
            .as_ref()
            .filter(|d| !d.applied)
            .map(|d| d.effect);
        if let Some(effect) = delivery {
            self.account_prompt(sid, effect);
        }
        let working = foreign.previous == Activity::Working && delivery.is_some();
        let s = self.sl(sid);
        s.activity = foreign.previous;
        s.turn = foreign.native.clone();
        if s.guard.state == GuardState::Interrupting {
            s.guard.state = GuardState::Armed;
        }
        self.out.push(Intent::Checkpoint { sleeve: sid, g });
        if working {
            self.sl(sid).delivery = None;
            self.begin_turn(sid, foreign.native);
        }
        match end {
            TurnEnd::UsageLimit => self.trip(sid, Guardrail::QuotaExhausted),
            TurnEnd::AuthFailed => self.trip(sid, Guardrail::AuthFailed),
            _ => {}
        }
        if matches!(self.st.sleeves[&sid].activity, Activity::Idle { .. }) {
            self.settled(sid);
            self.offer_auto_continue(sid);
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

    /// The generation `after_death` just started is a native resume of the session
    /// whose turn was acked. A fresh session leaves `resume` empty.
    fn native_resume(&self, sid: SleeveId) -> bool {
        let g = self.st.sleeves[&sid].g;
        self.st.effects.values().any(|e| {
            matches!(
                &e.kind,
                EffectKind::StartIncarnation {
                    sleeve,
                    g: eg,
                    resume,
                    ..
                } if *sleeve == sid && *eg == g && resume.is_some()
            )
        })
    }

    fn continue_key(sid: SleeveId, seq: u64) -> String {
        format!("continue:{}:{}", sid.0, seq)
    }

    /// The continuation already issued for this owner turn, if any.
    fn continue_state(&self, sid: SleeveId, seq: u64) -> Option<MsgState> {
        self.st.sleeves[&sid]
            .messages
            .get(&Self::continue_key(sid, seq))
            .map(|m| m.state)
    }

    /// Drops the continuation debt and any continuation still queued. An in-flight
    /// unsent prompt is not queued yet; fencing puts it back, and the exit then drops it.
    /// Rejected, not left queued: a later start must not send it. No `MessageRejected` —
    /// the caller warns with `TurnNeedsPrompt` when something was actually dropped.
    pub(crate) fn discard_continuation(&mut self, sid: SleeveId) -> bool {
        let had = self.sl(sid).auto_continue.take().is_some();
        had || self.reject_queued_continues(sid)
    }

    fn drop_queued_continuation(&mut self, sid: SleeveId) {
        if self.reject_queued_continues(sid) {
            self.needs_prompt(sid);
        }
    }

    fn reject_queued_continues(&mut self, sid: SleeveId) -> bool {
        let s = self.sl(sid);
        let continuation = |id: &String| {
            s.messages
                .get(id)
                .is_some_and(|m| matches!(m.kind, MsgKind::Continuation { .. }))
        };
        let keys: Vec<String> = s
            .queue
            .iter()
            .filter(|id| continuation(id))
            .cloned()
            .collect();
        if keys.is_empty() {
            return false;
        }
        s.queue.retain(|id| !keys.contains(id));
        for id in keys {
            if let Some(m) = s.messages.get_mut(&id) {
                m.state = MsgState::Rejected;
            }
        }
        true
    }

    /// The owner, and the parent when there is one, hear that nobody continued the turn.
    pub(crate) fn needs_prompt(&mut self, sid: SleeveId) {
        let parent = self.st.sleeves[&sid].parent;
        self.notify(Party::Owner, Some(sid), Notice::TurnNeedsPrompt);
        if let Some(p) = parent {
            self.notify(Party::Sleeve(p), Some(sid), Notice::TurnNeedsPrompt);
        }
    }

    /// After the resumed incarnation is ready: ask the hub to journal `Continue` when a prompt
    /// can still go out. Allocates nothing; the effect belongs to that later event.
    fn offer_auto_continue(&mut self, sid: SleeveId) {
        let s = &self.st.sleeves[&sid];
        if s.foreign.is_some() || s.retry_unwritten {
            return;
        }
        if s.auto_continue != Some(s.turn_seq) {
            return;
        }
        if !s.queue.is_empty() || matches!(s.activity, Activity::Delivering { .. }) {
            self.sl(sid).auto_continue = None;
            return;
        }
        if self.deliverable(sid) {
            self.out.push(Intent::AutoContinue {
                sleeve: sid,
                text: CONTINUE_PROMPT.to_string(),
            });
            return;
        }
        let s = &self.st.sleeves[&sid];
        if self.frozen(sid) || s.target.mode != Mode::Headless || !s.life.live() {
            self.sl(sid).auto_continue = None;
            self.needs_prompt(sid);
        }
    }

    /// Applies a journaled `Continue`. One prompt for the owed owner turn. If it cannot be
    /// delivered, the debt is dropped and, unless the sleeve is being torn down, the owner is
    /// warned. A journal without this event allocates nothing.
    pub(crate) fn continue_turn(&mut self, sid: SleeveId, text: &str) {
        let Some(s) = self.st.sleeves.get(&sid) else {
            return;
        };
        if s.foreign.is_some() || s.retry_unwritten {
            return;
        }
        let Some(seq) = s.auto_continue else {
            return;
        };
        if seq != s.turn_seq || text.is_empty() {
            return;
        }
        let (from, agent) = (s.turn_from, s.turn_agent.clone());
        let key = Self::continue_key(sid, seq);
        if s.messages.contains_key(&key)
            || !s.queue.is_empty()
            || matches!(s.activity, Activity::Delivering { .. })
        {
            self.sl(sid).auto_continue = None;
            return;
        }
        if !self.deliverable(sid) {
            let s = &self.st.sleeves[&sid];
            // Compacting, probing, or booting can race the prompt. Keeping the debt would send
            // it the next time this incarnation is idle, which may be long after the turn.
            let abandoned = !s.life.live() || matches!(s.life, Life::Parked(_) | Life::Resleeving);
            self.sl(sid).auto_continue = None;
            if !abandoned {
                self.needs_prompt(sid);
            }
            return;
        }
        let msg_seq = self.st.next_msg;
        self.st.next_msg += 1;
        self.sl(sid).messages.insert(
            key.clone(),
            Message {
                // The party that opened this owner turn (an agent steer, or mail from a marked
                // sleeve, replaces it), so the continued turn keeps that origin and a rejection
                // is reported to that party. A rejected continuation still warns the owner.
                from,
                agent,
                kind: MsgKind::Continuation { turn: seq },
                text: text.to_string(),
                mode: SendMode::Queue,
                state: MsgState::Queued,
                seq: msg_seq,
            },
        );
        self.sl(sid).queue.push_back(key);
        self.sl(sid).auto_continue = None;
        self.deliver_next(sid);
    }

    fn ready(&mut self, sid: SleeveId, session: &str) {
        let s = self.sl(sid);
        if s.activity != Activity::Booting {
            return;
        }
        if s.session.as_deref() != Some(session) {
            s.session = Some(session.to_string());
            s.turned = false;
        }
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
                self.offer_auto_continue(sid);
            }
            (Life::Resleeving, GuardState::Launching) => {
                let g = s.g;
                let key = format!("resleeve:{}:{}", s.name, g.0);
                let kind = EffectKind::SendPrompt {
                    sleeve: sid,
                    g,
                    request_id: key.clone(),
                    kind: MsgKind::Brief,
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
