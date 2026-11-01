//! The simulated hub and world.
//!
//! Hub: every event is encoded, decoded, applied and appended to the journal. Intents are held
//! until `commit` (the group-commit fsync), then released to the executors. `crash` drops the
//! uncommitted tail, checks `replay(journal) == state at the last commit`, and applies
//! `HubRecovered`. Executors re-check attempt and generation before acting (12b item 4).
//!
//! World: the set of incarnations whose scope has tasks, the writes whose first byte left, and
//! which node got which submit. Processes survive a hub crash unless the crash says otherwise,
//! but lose their pipes to the hub. Scope probes answer from this set, effect probes never call
//! an undispatched write applied, and a node never claims a submit it never got.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use pt_core::codec;
use pt_core::*;

use crate::check;

#[derive(Clone, Copy, Debug, Default)]
pub struct Policy {
    /// StartIncarnation: 0 spawned, 1 spawn failed, 2 spawned but receipt lost, 3 lost, not spawned.
    pub start: u8,
    /// SendPrompt, Steer, RespondApproval, Compact: 0 ack, 1 nack, 2 lost, 3 no answer.
    pub prompt: u8,
    /// Stop and Kill: 0 the scope empties, 1 acked but lingering.
    pub stop: u8,
    /// Every other effect: 0 ack, 1 nack, 2 lost.
    pub other: u8,
    /// Probe answers: 0 applied, 1 not applied, 2 unknowable.
    pub probe: u8,
    /// select_target: 0 first candidate, 1 none, 2 second candidate.
    pub select: u8,
    /// Node answers (see `job_report`).
    pub poll: u8,
    /// Artifact checks: 0 ok, 1 bad.
    pub fetch: u8,
}

pub struct Sim {
    pub st: State,
    pub now: Millis,
    log: Vec<Vec<u8>>,
    committed: usize,
    committed_state: State,
    held: Vec<Intent>,
    pub pending: VecDeque<Intent>,
    pub alive: BTreeSet<(SleeveId, Gen)>,
    /// Incarnations this hub process started and so holds pipes to; a crash detaches them all.
    attached: BTreeSet<(SleeveId, Gen)>,
    /// Submits whose first byte reached a node: (job, attempt, node).
    pub submitted: BTreeSet<(JobId, u32, NodeId)>,
    /// Effects whose first byte left the hub.
    pub dispatched: BTreeSet<EffectId>,
    /// Journal index of the event whose apply first emitted each effect (I7).
    pub emitted: BTreeMap<EffectId, usize>,
    /// Automatic resleeve briefs: (journal index, sleeve, time) (I10).
    pub briefs: Vec<(usize, SleeveId, Millis)>,
    /// Intents of the most recent apply.
    pub last: Vec<Intent>,
    pub steps: usize,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    pub fn new() -> Sim {
        Sim {
            st: State::default(),
            now: 1_700_000_000_000,
            log: Vec::new(),
            committed: 0,
            committed_state: State::default(),
            held: Vec::new(),
            pending: VecDeque::new(),
            alive: BTreeSet::new(),
            attached: BTreeSet::new(),
            submitted: BTreeSet::new(),
            dispatched: BTreeSet::new(),
            emitted: BTreeMap::new(),
            briefs: Vec::new(),
            last: Vec::new(),
            steps: 0,
        }
    }

    pub fn apply(&mut self, kind: EventKind) -> Vec<Intent> {
        let e = Event { at: self.now, kind };
        let bytes = codec::encode(&e);
        assert_eq!(
            codec::decode(&bytes).as_ref(),
            Ok(&e),
            "event codec round trip"
        );
        let prev = self.st.clone();
        let out = apply(&mut self.st, &e);
        if std::env::var_os("PT_TRACE").is_some() {
            eprintln!("{:>4} {:?}", self.log.len(), e.kind);
            for i in &out {
                eprintln!("       -> {i:?}");
            }
        }
        let idx = self.log.len();
        self.log.push(bytes);
        self.steps += 1;
        check::intents(self, idx, &out);
        check::step(&prev, &self.st, self);
        if matches!(e.kind, EventKind::Tick | EventKind::HubRecovered) {
            check::timers(&self.st);
        }
        self.held.extend(out.iter().cloned());
        self.last = out.clone();
        out
    }

    pub fn tick(&mut self, by: Millis) {
        self.now = self.now.saturating_add(by);
        self.apply(EventKind::Tick);
    }

    pub fn commit(&mut self) {
        self.committed = self.log.len();
        self.committed_state = self.st.clone();
        self.pending.extend(self.held.drain(..));
    }

    /// The hub process dies before its next fsync and restarts.
    pub fn crash(&mut self, scopes_emptied: bool) {
        self.log.truncate(self.committed);
        let events: Vec<Event> = self
            .log
            .iter()
            .map(|b| codec::decode(b).expect("journaled event decodes"))
            .collect();
        let replayed = replay(&events);
        assert!(
            replayed == self.committed_state,
            "replay(journal) differs from the live state at the last commit"
        );
        self.st = replayed;
        self.held.clear();
        self.pending.clear();
        let committed = self.committed;
        self.emitted.retain(|_, &mut i| i < committed);
        self.briefs.retain(|&(i, _, _)| i < committed);
        if scopes_emptied {
            self.alive.clear();
        }
        self.attached.clear();
        self.now = self.now.saturating_add(2 * SECOND);
        self.apply(EventKind::HubRecovered);
    }

    /// End of run: the whole journal replays to the live state.
    pub fn finish(&mut self) {
        self.commit();
        let events: Vec<Event> = self
            .log
            .iter()
            .map(|b| codec::decode(b).expect("journaled event decodes"))
            .collect();
        assert!(replay(&events) == self.st, "replay(journal) != live state");
    }

    /// Commits and executes released intents until none are left (bounded).
    pub fn run(&mut self, p: &Policy) {
        for _ in 0..2000 {
            self.commit();
            let Some(i) = self.pending.pop_front() else {
                return;
            };
            self.exec(i, p);
        }
    }

    pub fn exec_at(&mut self, idx: usize, p: &Policy) {
        if let Some(i) = self.pending.remove(idx) {
            self.exec(i, p);
        }
    }

    /// One executor acting on one released intent.
    pub fn exec(&mut self, intent: Intent, p: &Policy) {
        match intent {
            Intent::Execute {
                effect,
                attempt,
                kind,
            } => {
                let Some(e) = self.st.effects.get(&effect) else {
                    return;
                };
                if e.attempt != attempt || e.state != EffectState::Intent {
                    return;
                }
                if let Some((s, g)) = kind.incarnation()
                    && self.st.sleeves[&s].g != g
                {
                    return;
                }
                self.apply(EventKind::Dispatched { effect, attempt });
                self.dispatched.insert(effect);
                if let EffectKind::NodeSubmit {
                    job, attempt, node, ..
                } = kind
                {
                    self.submitted.insert((job, attempt, node));
                }
                self.perform(effect, attempt, &kind, p);
            }
            Intent::Probe {
                effect,
                attempt,
                kind,
            } => {
                let Some(e) = self.st.effects.get(&effect) else {
                    return;
                };
                if e.attempt != attempt || e.state != EffectState::Probing {
                    return;
                }
                match kind {
                    EffectKind::NodeSubmit {
                        job, attempt, node, ..
                    }
                    | EffectKind::NodeCancel { job, attempt, node }
                    | EffectKind::NodeAckCollected { job, attempt, node } => {
                        let report = self.node_report(job, attempt, node, p.poll);
                        self.apply(EventKind::JobStatus {
                            job,
                            attempt,
                            node,
                            report,
                        });
                    }
                    _ => {
                        // Nothing applied a write whose first byte never left.
                        let result = match p.probe % 3 {
                            0 if self.dispatched.contains(&effect) => ProbeResult::Applied,
                            0 | 1 => ProbeResult::NotApplied,
                            _ => ProbeResult::Unknowable,
                        };
                        self.apply(EventKind::Probed {
                            effect,
                            attempt,
                            result,
                        });
                    }
                }
            }
            Intent::ProbeScope { sleeve, g } => {
                let live = self.alive.contains(&(sleeve, g));
                self.apply(EventKind::ScopeProbed { sleeve, g, live });
            }
            Intent::SelectTarget {
                sleeve,
                g,
                candidates,
                exclude,
                ..
            } => {
                let ok: Vec<&Target> = candidates.iter().filter(|t| !exclude.contains(t)).collect();
                let target = match p.select % 3 {
                    0 => ok.first().map(|t| (*t).clone()),
                    1 => None,
                    _ => ok.get(1).or(ok.first()).map(|t| (*t).clone()),
                };
                self.apply(EventKind::TargetChosen { sleeve, g, target });
            }
            Intent::ScheduleJob { job, exclude } => {
                let Some(j) = self.st.jobs.get(&job) else {
                    return;
                };
                let attempt = j.attempt;
                let node = [NodeId(1), NodeId(2), NodeId(3)]
                    .into_iter()
                    .find(|n| !exclude.contains(n))
                    .unwrap_or(NodeId(1));
                self.apply(EventKind::NodeChosen {
                    job,
                    attempt,
                    node,
                    epoch: 7,
                });
            }
            Intent::PollJob { job, attempt, node } => {
                let report = self.node_report(job, attempt, node, p.poll);
                self.apply(EventKind::JobStatus {
                    job,
                    attempt,
                    node,
                    report,
                });
            }
            Intent::FetchArtifacts { job, attempt, .. } => {
                self.apply(EventKind::ArtifactsChecked {
                    job,
                    attempt,
                    ok: p.fetch % 2 == 0,
                });
            }
            Intent::ReadLog { .. }
            | Intent::Checkpoint { .. }
            | Intent::Notify { .. }
            | Intent::Reply(_) => {}
        }
    }

    fn perform(&mut self, effect: EffectId, attempt: u32, kind: &EffectKind, p: &Policy) {
        let ack = |native: Option<String>| EventKind::Acked {
            effect,
            attempt,
            native,
        };
        let nack = EventKind::Nacked {
            effect,
            attempt,
            reason: "refused".into(),
        };
        let lost = EventKind::Lost { effect, attempt };
        match kind {
            EffectKind::StartIncarnation { sleeve, g, .. } => match p.start % 4 {
                0 => {
                    self.alive.insert((*sleeve, *g));
                    self.attached.insert((*sleeve, *g));
                    self.apply(EventKind::Launched {
                        sleeve: *sleeve,
                        g: *g,
                        scope: format!("pt-{}-g{}.scope", sleeve.0, g.0),
                        pid: 4000 + g.0 as u32,
                    });
                }
                1 => {
                    self.apply(nack);
                }
                2 => {
                    self.alive.insert((*sleeve, *g));
                    self.attached.insert((*sleeve, *g));
                    self.apply(lost);
                }
                _ => {
                    self.apply(lost);
                }
            },
            EffectKind::StopIncarnation { sleeve, g } | EffectKind::KillScope { sleeve, g } => {
                self.apply(ack(None));
                if p.stop % 2 == 0 && self.alive.remove(&(*sleeve, *g)) {
                    self.apply(EventKind::Exited {
                        sleeve: *sleeve,
                        g: *g,
                        cause: ExitCause::Killed,
                    });
                }
            }
            EffectKind::SendPrompt { .. }
            | EffectKind::Steer { .. }
            | EffectKind::RespondApproval { .. }
            | EffectKind::Compact { .. } => match p.prompt % 4 {
                0 => {
                    self.apply(ack(Some(format!("turn-{}", effect.0))));
                }
                1 => {
                    self.apply(nack);
                }
                2 => {
                    self.apply(lost);
                }
                _ => {}
            },
            _ => match p.other % 3 {
                0 => {
                    self.apply(ack(None));
                }
                1 => {
                    self.apply(nack);
                }
                _ => {
                    self.apply(lost);
                }
            },
        }
    }

    /// A node's answer about `(job, attempt)`, chosen by `b`. A node never claims to run, or to
    /// have run, a submit that never reached it.
    fn node_report(&self, job: JobId, attempt: u32, node: NodeId, b: u8) -> JobReport {
        let report = job_report(b);
        let claims_run = matches!(
            report,
            JobReport::Running { .. }
                | JobReport::Exited { .. }
                | JobReport::Interrupted
                | JobReport::Cancelled
        );
        if claims_run && !self.submitted.contains(&(job, attempt, node)) {
            return JobReport::Unknown { epoch: 7 };
        }
        report
    }

    /// A harness observation. The current generation speaks only through pipes this hub
    /// process holds; a stale generation's late output is delivered, to exercise fencing.
    pub fn observe(&mut self, sleeve: SleeveId, g: Gen, obs: Obs) {
        let current = self.st.sleeves.get(&sleeve).is_some_and(|s| s.g == g);
        if current && !self.attached.contains(&(sleeve, g)) {
            return;
        }
        self.apply(EventKind::Observed { sleeve, g, obs });
    }

    pub fn crash_process(&mut self, sleeve: SleeveId, g: Gen, cause: ExitCause) {
        if self.alive.remove(&(sleeve, g)) {
            self.apply(EventKind::Exited { sleeve, g, cause });
        }
    }
}

pub fn job_report(b: u8) -> JobReport {
    match b % 9 {
        0 => JobReport::Running { offset: 64 },
        1 => JobReport::Exited { code: 0 },
        2 => JobReport::Exited { code: 2 },
        3 => JobReport::Interrupted,
        4 => JobReport::Unknown { epoch: 7 },
        5 => JobReport::Unknown { epoch: 8 },
        6 => JobReport::Refused { invalid: false },
        7 => JobReport::Refused { invalid: true },
        _ => JobReport::Cancelled,
    }
}
