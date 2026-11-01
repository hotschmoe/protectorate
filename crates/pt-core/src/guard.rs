//! Guardrails G1-G14 as pure predicates, and the resume and resleeve machine (2.d, 12a item 1,
//! 12b items 2 and 7).

use crate::Core;
use crate::intent::{Code, EffectKind, Intent, Notice, Reply};
use crate::model::*;
use crate::state::*;

/// Thresholds (SPEC D16 defaults).
pub mod limits {
    use crate::model::*;

    pub const MAX_CHILDREN: usize = 8;
    /// G5: not Ready within this long.
    pub const START_TIMEOUT: Millis = 60 * SECOND;
    /// G6: Working with no progress for this long.
    pub const SILENT: Millis = 15 * MINUTE;
    /// G7: consecutive identical tool calls.
    pub const LOOP_REPEATS: u32 = 6;
    /// G8: one turn Working for this long.
    pub const TURN_WALLCLOCK: Millis = 90 * MINUTE;
    /// G9: Blocked on an approval for this long.
    pub const APPROVAL_TIMEOUT: Millis = 30 * MINUTE;
    /// G13: Interrupting for this long.
    pub const INTERRUPT_TIMEOUT: Millis = 30 * SECOND;
    /// StopIncarnation to KillScope, and between KillScope resends.
    pub const STOP_GRACE: Millis = 30 * SECOND;
    /// G3: at most `RESUME_LIMIT` resumes in `RESUME_WINDOW`, then a forced resleeve.
    pub const RESUME_WINDOW: Millis = 10 * MINUTE;
    pub const RESUME_LIMIT: usize = 2;
    /// At most `RESLEEVE_LIMIT` automatic resleeves per hour: the next trip parks (I10).
    pub const RESLEEVE_WINDOW: Millis = HOUR;
    pub const RESLEEVE_LIMIT: usize = 2;
    /// Interrupt-class strikes in an hour before the next one resleeves.
    pub const STRIKE_WINDOW: Millis = HOUR;
    pub const STRIKE_LIMIT: usize = 2;
    /// G4: a second OOM in this window parks.
    pub const OOM_WINDOW: Millis = HOUR;
    /// Quiescing waits this long for a turn boundary, then interrupts; then stops after grace.
    pub const QUIESCE_WAIT: Millis = 5 * MINUTE;
    pub const QUIESCE_GRACE: Millis = 30 * SECOND;
    pub const COOLDOWN: Millis = 10 * MINUTE;
    /// Resleeve targets tried before parking.
    pub const MAX_TARGETS: usize = 3;
}

use limits::*;

/// What a tripped guardrail does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Interrupt,
    Resume,
    Resleeve,
    Park,
    Kill,
    DenyApproval,
}

pub fn action(g: Guardrail) -> Action {
    use Guardrail::*;
    match g {
        QuotaExhausted | AuthFailed | StartFailed => Action::Resleeve,
        Crash | Oom => Action::Resume,
        Silent | Loop | TurnWallclock => Action::Interrupt,
        ApprovalTimeout => Action::DenyApproval,
        Budget | ForbiddenPath | DiskLimit => Action::Park,
        InterruptUnresponsive | ProtocolDesync => Action::Kill,
    }
}

/// Time since `t`, which is never after `now`. Event times may come close to `Millis::MAX`, so
/// every timeout compares elapsed time rather than adding to a stamp.
pub(crate) fn since(now: Millis, t: Millis) -> Millis {
    now.saturating_sub(t)
}

fn recent(v: &mut Vec<Millis>, now: Millis, window: Millis) -> usize {
    v.retain(|&t| since(now, t) < window);
    v.len()
}

/// G5: not Ready within 60 s.
pub fn start_failed(s: &Sleeve, now: Millis) -> bool {
    s.activity == Activity::Booting && since(now, s.boot_at) >= START_TIMEOUT
}

/// G6: Working with no progress for 15 min.
pub fn silent(s: &Sleeve, now: Millis) -> bool {
    s.activity == Activity::Working && since(now, s.progress_at) >= SILENT
}

/// G8: one turn Working (or Blocked) for more than 90 min.
pub fn turn_wallclock(s: &Sleeve, now: Millis) -> bool {
    matches!(s.activity, Activity::Working | Activity::Blocked { .. })
        && since(now, s.turn_at) >= TURN_WALLCLOCK
}

/// G9: Blocked on an approval for more than 30 min.
pub fn approval_timeout(s: &Sleeve, now: Millis) -> bool {
    matches!(&s.activity, Activity::Blocked { approvals } if !approvals.is_empty())
        && since(now, s.blocked_at) >= APPROVAL_TIMEOUT
}

/// G13: Interrupting for more than 30 s.
pub fn interrupt_unresponsive(s: &Sleeve, now: Millis) -> bool {
    s.activity == Activity::Interrupting && since(now, s.interrupt_at) >= INTERRUPT_TIMEOUT
}

impl Core<'_> {
    /// Runs a guardrail's action if the sleeve is in a state where it applies.
    pub(crate) fn trip(&mut self, sid: SleeveId, g: Guardrail) {
        let s = &self.st.sleeves[&sid];
        let running = matches!(
            s.life,
            Life::Launching | Life::Running | Life::Resleeving | Life::Recovering
        );
        if !running {
            return;
        }
        let armed = matches!(
            s.guard.state,
            GuardState::Armed | GuardState::Cooldown { .. }
        );
        let pane = s.target.mode == Mode::Pane;
        match action(g) {
            Action::Park => self.park(sid, ParkReason::Guard(g)),
            Action::Kill => {
                if s.proc != Proc::Down && s.kill_at.is_none() {
                    self.kill(sid, Some(Cause::Guard(g)));
                    self.notify(Party::Owner, Some(sid), Notice::Tripped { guardrail: g });
                }
            }
            Action::Resleeve => {
                let active = armed
                    || matches!(
                        s.guard.state,
                        GuardState::Interrupting | GuardState::Resuming
                    );
                if matches!(s.life, Life::Launching | Life::Running) && active {
                    self.notify(Party::Owner, Some(sid), Notice::Tripped { guardrail: g });
                    self.begin_resleeve(sid, Cause::Guard(g), None);
                }
            }
            Action::Interrupt => {
                let busy = matches!(s.activity, Activity::Working | Activity::Blocked { .. });
                if s.life != Life::Running || !armed || !busy {
                    return;
                }
                self.notify(Party::Owner, Some(sid), Notice::Tripped { guardrail: g });
                let now = self.now;
                if pane {
                    // The hub never types into a pane: notify only, and restart the clocks.
                    let s = self.sl(sid);
                    s.progress_at = now;
                    s.turn_at = now;
                    return;
                }
                let gd = &mut self.sl(sid).guard;
                if recent(&mut gd.strikes, now, STRIKE_WINDOW) < STRIKE_LIMIT {
                    gd.strikes.push(now);
                    gd.state = GuardState::Interrupting;
                    self.interrupt(sid);
                } else {
                    self.begin_resleeve(sid, Cause::Guard(g), None);
                }
            }
            Action::DenyApproval => {
                let Activity::Blocked { approvals } = &s.activity else {
                    return;
                };
                if pane || s.life != Life::Running {
                    return;
                }
                let approvals = approvals.clone();
                self.sl(sid).blocked_at = self.now;
                for a in approvals {
                    if !self.respond(sid, &a, false, "approval timed out after 30 minutes") {
                        continue;
                    }
                    self.notify(
                        Party::Owner,
                        Some(sid),
                        Notice::ApprovalTimedOut { approval: a },
                    );
                }
            }
            Action::Resume => self.after_death(sid, Cause::Guard(g)),
        }
    }

    /// The incarnation is dead and the sleeve is Recovering: resume, resleeve or park.
    pub(crate) fn after_death(&mut self, sid: SleeveId, cause: Cause) {
        let now = self.now;
        let s = self.sl(sid);
        if s.life != Life::Recovering || s.proc != Proc::Down {
            return;
        }
        if s.guard.state == GuardState::Resuming && cause != Cause::HubRestart {
            // The resumed incarnation failed before Ready.
            return self.begin_resleeve(sid, cause, None);
        }
        match cause {
            Cause::Guard(Guardrail::StartFailed) => return self.begin_resleeve(sid, cause, None),
            Cause::Guard(Guardrail::Oom) => {
                if recent(&mut s.guard.ooms, now, OOM_WINDOW) > 0 {
                    return self.park(sid, ParkReason::Guard(Guardrail::Oom));
                }
                s.guard.ooms.push(now);
            }
            _ => {}
        }
        let s = self.sl(sid);
        if recent(&mut s.guard.resumes, now, RESUME_WINDOW) >= RESUME_LIMIT {
            return self.begin_resleeve(sid, cause, None);
        }
        s.guard.resumes.push(now);
        s.guard.state = GuardState::Resuming;
        let target = s.target.clone();
        self.start_incarnation(sid, target, true);
    }

    /// Enters the resleeve path: Quiescing while the incarnation lives, else a forced capture.
    pub(crate) fn begin_resleeve(&mut self, sid: SleeveId, cause: Cause, target: Option<Target>) {
        let now = self.now;
        let s = self.sl(sid);
        if cause != Cause::Manual {
            if recent(&mut s.guard.resleeves, now, RESLEEVE_WINDOW) >= RESLEEVE_LIMIT {
                return self.park(sid, ParkReason::Flapping);
            }
            s.guard.resleeves.push(now);
        }
        s.life = Life::Resleeving;
        s.guard.cause = Some(cause);
        s.guard.tried.clear();
        s.guard.chosen = target;
        if s.proc == Proc::Down {
            return self.capture(sid, true);
        }
        s.guard.state = GuardState::Quiescing {
            deadline: now.saturating_add(QUIESCE_WAIT),
            nudged: false,
        };
        if matches!(
            s.activity,
            Activity::Idle { .. } | Activity::Booting | Activity::Unknown
        ) {
            self.stop_incarnation(sid);
        }
    }

    /// The old scope is confirmed empty (12a item 1): checkpoint and ask for the brief.
    pub(crate) fn capture(&mut self, sid: SleeveId, forced: bool) {
        let s = &self.st.sleeves[&sid];
        let from_gen = s.g;
        let since = s.guard.since_gen;
        let cause = s.guard.cause.unwrap_or(Cause::Manual);
        let effects = self
            .st
            .effects
            .values()
            .filter(|e| e.kind.sleeve() == Some(sid))
            .filter(|e| match e.state {
                EffectState::Uncertain | EffectState::Probing => true,
                EffectState::Fenced => e.kind.incarnation().is_some_and(|(_, g)| g >= since),
                _ => false,
            })
            .map(|e| e.id)
            .collect();
        let roster = self
            .st
            .sleeves
            .values()
            .filter(|c| c.parent == Some(sid) && c.life.live())
            .map(|c| c.id)
            .collect();
        let s = self.sl(sid);
        s.life = Life::Resleeving;
        s.guard.forced = forced;
        self.out.push(Intent::Checkpoint {
            sleeve: sid,
            g: from_gen,
        });
        let to_gen = Gen(from_gen.0 + 1);
        let brief = self.new_effect(
            EffectKind::ContinuationBrief {
                sleeve: sid,
                from_gen,
                to_gen,
                forced,
                cause,
                effects,
                roster,
            },
            format!("brief:{}:{}", sid.0, to_gen.0),
        );
        self.sl(sid).guard.state = GuardState::Capturing { brief };
    }

    pub(crate) fn on_brief(&mut self, sid: SleeveId, id: EffectId) {
        let done = self.effect_state(id).is_some_and(EffectState::terminal);
        if done && self.st.sleeves[&sid].guard.state == (GuardState::Capturing { brief: id }) {
            // A failed brief still continues: the new incarnation gets a facts-only brief.
            self.select(sid);
        }
    }

    /// Selecting: use a manually named target, or ask the pure `select_target` outside the core.
    pub(crate) fn select(&mut self, sid: SleeveId) {
        let s = self.sl(sid);
        if let Some(t) = s.guard.chosen.take()
            && !s.guard.tried.contains(&t)
        {
            return self.launch_target(sid, t);
        }
        s.guard.state = GuardState::Selecting;
        let cause = s.guard.cause.unwrap_or(Cause::Manual);
        let mut exclude = s.guard.tried.clone();
        if cause != Cause::Manual && !exclude.contains(&s.target) {
            exclude.push(s.target.clone());
        }
        let intent = Intent::SelectTarget {
            sleeve: sid,
            g: s.g,
            cause,
            candidates: s.resleeve_to.clone(),
            exclude,
        };
        self.out.push(intent);
    }

    pub(crate) fn target_chosen(&mut self, sid: SleeveId, g: Gen, target: Option<&Target>) {
        let Some(s) = self.st.sleeves.get(&sid) else {
            return;
        };
        if s.g != g || s.guard.state != GuardState::Selecting {
            return;
        }
        let auto = s.guard.cause != Some(Cause::Manual);
        match target {
            Some(t)
                if t.valid() && !s.guard.tried.contains(t) && !(auto && t.mode == Mode::Pane) =>
            {
                self.launch_target(sid, t.clone())
            }
            _ => self.park(sid, ParkReason::NoTarget),
        }
    }

    fn launch_target(&mut self, sid: SleeveId, t: Target) {
        let g = &mut self.sl(sid).guard;
        g.tried.push(t.clone());
        g.state = GuardState::Launching;
        self.start_incarnation(sid, t, false);
    }

    /// The new incarnation died, or never started, before its brief landed.
    pub(crate) fn launch_failed(&mut self, sid: SleeveId) {
        let g = &mut self.sl(sid).guard;
        if g.tried.len() < MAX_TARGETS {
            g.chosen = None;
            self.select(sid);
        } else {
            self.park(sid, ParkReason::Launch);
        }
    }

    pub(crate) fn resleeve_cmd(&mut self, sid: SleeveId, target: Option<&Target>) {
        let Some(s) = self.st.sleeve(sid).filter(|s| s.life != Life::Removed) else {
            return self.refuse(Code::NotFound, "no such sleeve");
        };
        if target.is_some_and(|t| !t.valid()) {
            return self.refuse(Code::InvalidArgument, "that harness has no headless mode");
        }
        let armed = matches!(
            s.guard.state,
            GuardState::Armed | GuardState::Cooldown { .. }
        );
        if s.life != Life::Running || !armed {
            return self.refuse(
                Code::FailedPrecondition,
                "only a running, settled sleeve resleeves",
            );
        }
        self.begin_resleeve(sid, Cause::Manual, target.cloned());
        self.reply(Reply::Accepted);
    }

    pub(crate) fn tick_sleeves(&mut self) {
        let now = self.now;
        let ids: Vec<SleeveId> = self
            .st
            .sleeves
            .values()
            .filter(|s| {
                s.proc != Proc::Down || matches!(s.guard.state, GuardState::Cooldown { .. })
            })
            .map(|s| s.id)
            .collect();
        for sid in ids {
            let s = &self.st.sleeves[&sid];
            if let GuardState::Cooldown { until } = s.guard.state
                && now >= until
            {
                self.sl(sid).guard.state = GuardState::Armed;
            }
            let s = &self.st.sleeves[&sid];
            if s.proc == Proc::Down {
                continue;
            }
            let stop_due =
                s.stop_at.is_some_and(|t| since(now, t) >= STOP_GRACE) && s.kill_at.is_none();
            let kill_due = s.kill_at.is_some_and(|t| since(now, t) >= STOP_GRACE);
            if stop_due || kill_due {
                self.kill(sid, None);
                continue;
            }
            if let GuardState::Quiescing { deadline, nudged } = s.guard.state {
                if s.stop_at.is_none() && now >= deadline {
                    let busy = !matches!(
                        s.activity,
                        Activity::Idle { .. } | Activity::Booting | Activity::Unknown
                    );
                    if busy && !nudged && s.proc == Proc::Up && s.target.mode == Mode::Headless {
                        self.sl(sid).guard.state = GuardState::Quiescing {
                            deadline: now.saturating_add(QUIESCE_GRACE),
                            nudged: true,
                        };
                        self.interrupt(sid);
                    } else {
                        self.stop_incarnation(sid);
                    }
                }
                continue;
            }
            let s = &self.st.sleeves[&sid];
            if s.kill_at.is_none() && s.stop_at.is_none() {
                if interrupt_unresponsive(s, now) {
                    self.trip(sid, Guardrail::InterruptUnresponsive);
                    continue;
                }
                if start_failed(s, now) {
                    match s.guard.state {
                        GuardState::Armed if s.life == Life::Launching => {
                            self.trip(sid, Guardrail::StartFailed)
                        }
                        GuardState::Resuming | GuardState::Launching => {
                            self.kill(sid, Some(Cause::Guard(Guardrail::StartFailed)))
                        }
                        _ => {}
                    }
                    continue;
                }
            }
            let s = &self.st.sleeves[&sid];
            if approval_timeout(s, now) {
                self.trip(sid, Guardrail::ApprovalTimeout);
            } else if turn_wallclock(s, now) {
                self.trip(sid, Guardrail::TurnWallclock);
            } else if silent(s, now) {
                self.trip(sid, Guardrail::Silent);
            }
        }
    }
}
