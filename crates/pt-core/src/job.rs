//! Node jobs (2.e with 12a items 5 and 6, 12b item 9). Nodes never initiate: every report here
//! answers a hub request, and a job never re-runs by itself.

use crate::Core;
use crate::effect::Report;
use crate::event::{JobReport, JobSpec};
use crate::guard::since;
use crate::intent::{Code, EffectKind, Intent, Notice, Reply};
use crate::model::*;
use crate::state::*;

/// No answer for this long: Lost. Also the Cancelling timeout (12a item 6).
pub const LEASE: Millis = 5 * MINUTE;
/// Lost this long: Abandoned.
pub const LOST_ABANDON: Millis = 24 * HOUR;
/// A node that refused a submit as busy or unfit is skipped this long.
pub const EXCLUDE_FOR: Millis = 10 * MINUTE;
pub const MAX_VERIFY: u32 = 3;
/// Attempts of a cancel or collected ack before the record fails and the owner is told: about
/// one lease of Ticks.
pub const NODE_RESENDS: u32 = 30;

impl Core<'_> {
    fn job(&mut self, id: JobId) -> &mut Job {
        self.st.jobs.get_mut(&id).expect("job exists")
    }

    fn schedule(&mut self, id: JobId) {
        let now = self.now;
        let j = self.job(id);
        j.excluded.retain(|&(_, until)| until > now);
        let exclude = j.excluded.iter().map(|&(n, _)| n).collect();
        self.out.push(Intent::ScheduleJob { job: id, exclude });
    }

    fn set_job(&mut self, id: JobId, state: JobState) {
        let now = self.now;
        let j = self.job(id);
        j.state = state;
        j.state_at = now;
    }

    fn poll(&mut self, id: JobId) {
        let j = &self.st.jobs[&id];
        if let Some(node) = j.node {
            self.out.push(Intent::PollJob {
                job: id,
                attempt: j.attempt,
                node,
            });
        }
    }

    fn node_effect(&mut self, id: JobId, ack: bool) {
        let j = &self.st.jobs[&id];
        let Some(node) = j.node else { return };
        let (job, attempt) = (id, j.attempt);
        let (kind, key) = if ack {
            (
                EffectKind::NodeAckCollected { job, attempt, node },
                format!("collected:{}:{attempt}", id.0),
            )
        } else {
            (
                EffectKind::NodeCancel { job, attempt, node },
                format!("cancel:{}:{attempt}:{}", id.0, self.st.next_effect),
            )
        };
        self.new_effect(kind, key);
    }

    /// The cancel (`ack` false) or collected-ack records of the job's current attempt.
    fn node_writes(&self, id: JobId, ack: bool) -> Vec<EffectId> {
        let attempt = self.st.jobs[&id].attempt;
        self.st
            .effects
            .values()
            .filter(|e| match e.kind {
                EffectKind::NodeCancel {
                    job, attempt: a, ..
                } => !ack && job == id && a == attempt,
                EffectKind::NodeAckCollected {
                    job, attempt: a, ..
                } => ack && job == id && a == attempt,
                _ => false,
            })
            .map(|e| e.id)
            .collect()
    }

    /// On Tick and recovery, re-sends a cancel while the job may still run and the collected
    /// ack until the node takes it (12a items 5 and 7). A node's status cannot tell whether
    /// either landed, so they are re-sent rather than probed, up to `NODE_RESENDS` attempts.
    fn resend_node_writes(&mut self, id: JobId) {
        let cancel_wanted = matches!(
            self.st.jobs[&id].state,
            JobState::Cancelling | JobState::Lost | JobState::Abandoned
        );
        let mut writes = self.node_writes(id, true);
        if cancel_wanted {
            writes.extend(self.node_writes(id, false));
        }
        for eff in writes {
            self.resend_or_give_up(eff, NODE_RESENDS);
        }
    }

    pub(crate) fn create_job(&mut self, request_id: &str, spec: &JobSpec) {
        if let Some(&id) = self.st.job_requests.get(request_id) {
            if self.st.jobs[&id].spec == *spec {
                return self.reply(Reply::JobCreated(id));
            }
            return self.refuse(
                Code::InvalidArgument,
                "request id reused for a different job",
            );
        }
        if request_id.is_empty() || spec.command.is_empty() {
            return self.refuse(Code::InvalidArgument, "request id and command are required");
        }
        let id = JobId(self.st.next_job);
        self.st.next_job += 1;
        let now = self.now;
        self.st.jobs.insert(
            id,
            Job {
                id,
                request_id: request_id.to_string(),
                spec: spec.clone(),
                attempt: 1,
                node: None,
                epoch: 0,
                state: JobState::Queued,
                excluded: Vec::new(),
                submit: None,
                offset: 0,
                exit: None,
                verify_failures: 0,
                seen_at: now,
                state_at: now,
                needs_new_attempt: false,
            },
        );
        self.st.job_requests.insert(request_id.to_string(), id);
        self.schedule(id);
        self.reply(Reply::JobCreated(id));
    }

    pub(crate) fn node_chosen(&mut self, id: JobId, attempt: u32, node: NodeId, epoch: u64) {
        let Some(j) = self.st.jobs.get(&id) else {
            return;
        };
        if j.attempt != attempt || j.state != JobState::Queued {
            return;
        }
        let now = self.now;
        let j = self.job(id);
        j.node = Some(node);
        j.epoch = epoch;
        j.seen_at = now;
        self.set_job(id, JobState::Submitting);
        let eff = self.new_effect(
            EffectKind::NodeSubmit {
                job: id,
                attempt,
                node,
                epoch,
            },
            format!("job:{}:{attempt}", id.0),
        );
        self.job(id).submit = Some(eff);
    }

    pub(crate) fn on_submit(&mut self, id: JobId, eff: EffectId) {
        let j = &self.st.jobs[&id];
        if j.submit != Some(eff) {
            return;
        }
        match (self.effect_state(eff).expect("effect exists"), j.state) {
            (EffectState::Probing, JobState::Submitting) => {
                self.set_job(id, JobState::SubmitUncertain)
            }
            (EffectState::Confirmed, JobState::Submitting | JobState::SubmitUncertain) => {
                // The node acknowledged the submit: it journaled and started the job.
                self.set_job(id, JobState::Running)
            }
            (EffectState::Intent, JobState::SubmitUncertain) => {
                self.set_job(id, JobState::Submitting)
            }
            (
                EffectState::Failed | EffectState::Rejected,
                JobState::Submitting | JobState::SubmitUncertain,
            ) => {
                // The node provably never took it (it refused, or every attempt was not
                // applied): pick a node again, same attempt.
                let node = j.node;
                let now = self.now;
                let j = self.job(id);
                if let Some(n) = node {
                    j.excluded.push((n, now.saturating_add(EXCLUDE_FOR)));
                }
                j.node = None;
                j.submit = None;
                self.set_job(id, JobState::Queued);
                self.schedule(id);
            }
            _ => {}
        }
    }

    /// Resolves the submit record from a node's answer about the job.
    fn resolve_submit(&mut self, id: JobId, report: &JobReport) {
        let j = &self.st.jobs[&id];
        let Some(eff) = j.submit else { return };
        let Some(state) = self.effect_state(eff) else {
            return;
        };
        if state.terminal() {
            return;
        }
        let attempt = self.st.effects[&eff].attempt;
        match report {
            JobReport::Running { .. }
            | JobReport::Exited { .. }
            | JobReport::Interrupted
            | JobReport::Cancelled => self.evidence(eff),
            JobReport::Refused { .. } => {
                if state == EffectState::Sent {
                    self.effect_report(eff, attempt, Report::Nacked);
                } else if state == EffectState::Probing {
                    self.effect_report(eff, attempt, Report::Probed(ProbeResult::NotApplied));
                }
            }
            JobReport::Unknown { epoch } if state == EffectState::Probing => {
                // Never-seen only when the node's journal epoch is the one we submitted against.
                if *epoch == j.epoch {
                    self.effect_report(eff, attempt, Report::Probed(ProbeResult::NotApplied));
                } else {
                    self.job(id).needs_new_attempt = true;
                    self.effect_report(eff, attempt, Report::Probed(ProbeResult::Unknowable));
                }
            }
            JobReport::Unknown { .. } => {}
        }
    }

    pub(crate) fn job_status(&mut self, id: JobId, attempt: u32, node: NodeId, report: &JobReport) {
        let Some(j) = self.st.jobs.get(&id) else {
            return;
        };
        // A refused attempt is scheduled again on the same attempt number, so the node fences
        // the report: a late answer from the node that refused is not about this submit.
        if j.attempt != attempt || j.node != Some(node) {
            return;
        }
        let state = j.state;
        let now = self.now;
        self.job(id).seen_at = now;
        use JobState as J;
        if matches!(state, J::Submitting | J::SubmitUncertain)
            && let JobReport::Refused { invalid } = report
        {
            // Leave the submitting states first so the record is not retried.
            if *invalid {
                self.set_job(id, J::Failed(JobFailure::Invalid));
                self.notify(
                    Party::Owner,
                    None,
                    Notice::JobFinished { job: id, ok: false },
                );
            } else {
                let node = self.st.jobs[&id].node.expect("node");
                let j = self.job(id);
                j.excluded.push((node, now.saturating_add(EXCLUDE_FOR)));
                j.node = None;
                self.set_job(id, J::Queued);
            }
            self.resolve_submit(id, report);
            if !*invalid {
                self.job(id).submit = None;
                self.schedule(id);
            }
            return;
        }
        self.resolve_submit(id, report);
        let state = self.st.jobs[&id].state;
        match (state, report) {
            (
                J::Submitting | J::SubmitUncertain | J::Running | J::Lost,
                JobReport::Running { offset },
            ) => {
                self.job(id).offset = *offset;
                // A lost job the owner cancelled is still being cancelled once it answers.
                let cancelled = state == J::Lost && !self.node_writes(id, false).is_empty();
                if cancelled {
                    self.set_job(id, J::Cancelling);
                } else if state != J::Running {
                    self.set_job(id, J::Running);
                }
                let j = &self.st.jobs[&id];
                self.out.push(Intent::ReadLog {
                    job: id,
                    attempt,
                    node: j.node.expect("node"),
                    upto: *offset,
                });
            }
            (
                J::Submitting | J::SubmitUncertain | J::Running | J::Lost,
                JobReport::Exited { code },
            ) => {
                self.job(id).exit = Some(*code);
                self.set_job(id, J::Collecting);
                self.fetch(id);
            }
            (J::Submitting | J::SubmitUncertain | J::Running | J::Lost, JobReport::Interrupted) => {
                self.set_job(id, J::Interrupted);
                self.notify(Party::Owner, None, Notice::JobInterrupted { job: id });
            }
            (J::Submitting | J::SubmitUncertain | J::Running | J::Lost, JobReport::Cancelled) => {
                self.set_job(id, J::Cancelled);
                self.node_effect(id, true);
            }
            (J::Running, JobReport::Unknown { .. }) => {
                self.set_job(id, J::Lost);
                self.notify(Party::Owner, None, Notice::JobLost { job: id });
            }
            (
                J::Cancelling,
                JobReport::Cancelled | JobReport::Exited { .. } | JobReport::Interrupted,
            ) => {
                self.set_job(id, J::Cancelled);
                for eff in self.node_writes(id, false) {
                    self.goal_reached(eff);
                }
                self.node_effect(id, true);
            }
            _ => {}
        }
    }

    fn fetch(&mut self, id: JobId) {
        let j = &self.st.jobs[&id];
        self.out.push(Intent::FetchArtifacts {
            job: id,
            attempt: j.attempt,
            node: j.node.expect("node"),
        });
    }

    pub(crate) fn artifacts_checked(&mut self, id: JobId, attempt: u32, ok: bool) {
        let Some(j) = self.st.jobs.get(&id) else {
            return;
        };
        if j.attempt != attempt || j.state != JobState::Collecting {
            return;
        }
        let now = self.now;
        self.job(id).seen_at = now;
        let j = &self.st.jobs[&id];
        let code = j.exit.unwrap_or(-1);
        let done = if ok {
            Some(if code == 0 {
                JobState::Succeeded
            } else {
                JobState::Failed(JobFailure::Exit(code))
            })
        } else {
            let j = self.job(id);
            j.verify_failures += 1;
            (j.verify_failures >= MAX_VERIFY).then_some(JobState::Failed(JobFailure::Artifact))
        };
        match done {
            Some(state) => {
                self.set_job(id, state);
                self.node_effect(id, true);
                let ok = state == JobState::Succeeded;
                self.notify(Party::Owner, None, Notice::JobFinished { job: id, ok });
            }
            None => self.fetch(id),
        }
    }

    pub(crate) fn cancel_job(&mut self, id: JobId) {
        let Some(j) = self.st.jobs.get(&id) else {
            return self.refuse(Code::NotFound, "no such job");
        };
        let unsent = j
            .submit
            .is_some_and(|e| self.effect_state(e) == Some(EffectState::Intent));
        match j.state {
            JobState::Queued => self.set_job(id, JobState::Cancelled),
            // Its submit has not been dispatched, and now never is: nothing reached the node.
            JobState::Submitting if unsent => {
                let eff = j.submit.expect("submit");
                self.set_job(id, JobState::Cancelled);
                self.withdraw(eff);
            }
            JobState::Submitting
            | JobState::SubmitUncertain
            | JobState::Running
            | JobState::Lost => {
                self.set_job(id, JobState::Cancelling);
                self.node_effect(id, false);
            }
            _ => {
                return self.refuse(
                    Code::FailedPrecondition,
                    "the job can no longer be cancelled",
                );
            }
        }
        self.reply(Reply::Accepted);
    }

    pub(crate) fn abandon_job(&mut self, id: JobId) {
        let Some(j) = self.st.jobs.get(&id) else {
            return self.refuse(Code::NotFound, "no such job");
        };
        let stuck = j.state == JobState::SubmitUncertain && j.needs_new_attempt;
        if j.state != JobState::Lost && !stuck {
            return self.refuse(Code::FailedPrecondition, "only a lost job can be abandoned");
        }
        self.set_job(id, JobState::Abandoned);
        self.node_effect(id, false);
        self.reply(Reply::Accepted);
    }

    /// An explicit new attempt: the only way a job runs again.
    pub(crate) fn requeue_job(&mut self, id: JobId) {
        let Some(j) = self.st.jobs.get(&id) else {
            return self.refuse(Code::NotFound, "no such job");
        };
        let stuck = j.state == JobState::SubmitUncertain && j.needs_new_attempt;
        if !matches!(j.state, JobState::Interrupted | JobState::Lost) && !stuck {
            return self.refuse(
                Code::FailedPrecondition,
                "only an interrupted, lost or unknowable job is requeued",
            );
        }
        let j = self.job(id);
        j.attempt += 1;
        j.node = None;
        j.submit = None;
        j.offset = 0;
        j.exit = None;
        j.verify_failures = 0;
        j.needs_new_attempt = false;
        self.set_job(id, JobState::Queued);
        self.schedule(id);
        self.reply(Reply::Accepted);
    }

    pub(crate) fn tick_jobs(&mut self) {
        let now = self.now;
        let ids: Vec<JobId> = self.st.jobs.keys().copied().collect();
        for id in ids {
            let j = &self.st.jobs[&id];
            match j.state {
                JobState::SubmitUncertain | JobState::Running if since(now, j.seen_at) >= LEASE => {
                    self.set_job(id, JobState::Lost);
                    self.notify(Party::Owner, None, Notice::JobLost { job: id });
                }
                JobState::Cancelling if since(now, j.state_at) >= LEASE => {
                    self.set_job(id, JobState::Lost);
                    self.notify(Party::Owner, None, Notice::JobLost { job: id });
                }
                // No artifact check answered within the lease: Lost, and a later Exited
                // report collects again.
                JobState::Collecting if since(now, j.seen_at) >= LEASE => {
                    self.set_job(id, JobState::Lost);
                    self.notify(Party::Owner, None, Notice::JobLost { job: id });
                }
                JobState::Lost if since(now, j.state_at) >= LOST_ABANDON => {
                    self.set_job(id, JobState::Abandoned);
                    self.node_effect(id, false);
                }
                JobState::SubmitUncertain
                | JobState::Running
                | JobState::Cancelling
                | JobState::Lost => self.poll(id),
                _ => {}
            }
            self.resend_node_writes(id);
        }
    }

    /// HubRecovered: every non-terminal job re-issues its I/O. Leases keep their journaled
    /// stamps, so time spent down counts (12b item 9); the polls issued here answer before the
    /// next Tick for a node that is up.
    pub(crate) fn recover_jobs(&mut self) {
        let ids: Vec<JobId> = self.st.jobs.keys().copied().collect();
        for id in ids {
            match self.st.jobs[&id].state {
                JobState::Queued => self.schedule(id),
                JobState::Submitting => self.set_job(id, JobState::SubmitUncertain),
                JobState::Cancelling if since(self.now, self.st.jobs[&id].state_at) >= LEASE => {
                    self.set_job(id, JobState::Lost);
                    self.notify(Party::Owner, None, Notice::JobLost { job: id });
                    self.poll(id);
                }
                JobState::SubmitUncertain
                | JobState::Running
                | JobState::Cancelling
                | JobState::Lost => self.poll(id),
                JobState::Collecting => self.fetch(id),
                _ => {}
            }
            self.resend_node_writes(id);
        }
    }
}
