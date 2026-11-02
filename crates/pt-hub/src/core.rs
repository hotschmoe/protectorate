//! The core thread: the one owner of `pt_core::State` (SPEC principle 3). It applies a batch of
//! inputs, journals the batch with one fsync, publishes the view, and only then releases the
//! batch's intents (pt-core README, SPEC 12b item 4).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use pt_api::EventKind as Kind;
use pt_api::{ActivityState, HubStreamMessage, Level};
use pt_core::{Event, Intent, Life, Notice, Party, SleeveId, State, codec};
use pt_journal::Journal;

use crate::exec::Executors;
use crate::home::now_ms;
use crate::hub::{Hub, Input, View};
use crate::log::{debug, error, info};
use crate::transcript::Note;
use crate::wire;

const MAX_BATCH: usize = 512;

pub fn run(hub: Arc<Hub>, mut state: State, mut journal: Journal, rx: Receiver<Input>) {
    let mut exec = Executors::new(hub.clone());
    let mut batch_n = hub.view().batch;
    let mut since: BTreeMap<SleeveId, (ActivityState, u64)> = BTreeMap::new();
    while let Ok(first) = rx.recv() {
        let mut inputs = vec![first];
        while inputs.len() < MAX_BATCH {
            match rx.try_recv() {
                Ok(i) => inputs.push(i),
                Err(_) => break,
            }
        }
        // The first batch announces everything recent: a message the crash kept from the stream
        // log would otherwise leave a resuming client behind the replayed state.
        let before = if batch_n == 0 {
            Arc::new(recent_baseline(&state))
        } else {
            hub.view()
        };
        let mut frames = Vec::with_capacity(inputs.len());
        let mut out = Vec::with_capacity(inputs.len());
        for input in inputs {
            let ev = Event {
                at: now_ms(),
                kind: input.kind,
            };
            let intents = pt_core::apply(&mut state, &ev);
            if !matches!(ev.kind, pt_core::EventKind::Tick) || !intents.is_empty() {
                debug!("apply {:?} -> {} intents", ev.kind, intents.len());
            }
            frames.push(codec::encode(&ev));
            out.push((input.reply, intents));
        }
        let refs: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
        if let Err(e) = journal.append_batch(&refs) {
            // Without durability no intent may leave: stop the hub (SPEC principle 5).
            error!("journal append failed, stopping: {e}");
            std::process::exit(70);
        }
        batch_n += 1;
        let now = now_ms();
        for s in state.sleeves.values() {
            let kind = wire::activity(&s.activity);
            match since.get(&s.id) {
                Some((k, _)) if *k == kind => {}
                _ => {
                    since.insert(s.id, (kind, now));
                }
            }
        }
        let view = Arc::new(View {
            state: state.clone(),
            batch: batch_n,
            since: since.iter().map(|(k, (_, t))| (*k, *t)).collect(),
        });
        hub.publish(view.clone());
        announce_changes(&hub, &before, &view);
        for (reply, intents) in out {
            for intent in intents {
                match intent {
                    Intent::Reply(r) => {
                        if let Some(tx) = &reply {
                            let _ = tx.send(r);
                        }
                    }
                    Intent::Notify { to, sleeve, notice } => {
                        notify(&hub, &view, to, sleeve, &notice)
                    }
                    other => exec.route(other),
                }
            }
        }
        let removed = view.state.sleeves.values().filter(|s| {
            s.life == Life::Removed
                && before
                    .state
                    .sleeve(s.id)
                    .is_some_and(|o| o.life != Life::Removed)
        });
        for s in removed {
            exec.release(s.id);
            hub.transcripts.close(s.id);
        }
    }
    info!("core stopped");
}

/// How far back the first batch after a start re-announces jobs and approvals.
const RECENT: u64 = 10 * 60 * 1000;

/// What the first batch compares against: no sleeves, and only the jobs and approvals that are
/// settled and older than `RECENT` before the last journaled event.
fn recent_baseline(st: &State) -> View {
    let old = |t: u64| st.now.saturating_sub(t) >= RECENT;
    let base = State {
        jobs: st
            .jobs
            .iter()
            .filter(|(_, j)| old(j.state_at) && old(j.seen_at))
            .map(|(k, j)| (*k, j.clone()))
            .collect(),
        approvals: st
            .approvals
            .iter()
            .filter(|(_, a)| {
                a.state != pt_core::ApprovalState::Pending
                    && old(a.requested_at)
                    && a.decision.is_none_or(|d| old(d.at))
            })
            .map(|(k, a)| (k.clone(), a.clone()))
            .collect(),
        ..State::default()
    };
    View {
        state: base,
        batch: 0,
        since: BTreeMap::new(),
    }
}

/// Pushes the stream messages for every sleeve that changed in this batch: `workspace` for a new
/// root, `sleeve`, or `sleeve_removed`; `job` for every job and `approval` for every approval
/// that changed.
fn announce_changes(hub: &Hub, before: &View, after: &View) {
    for s in after.state.sleeves.values() {
        let old = before.state.sleeves.get(&s.id);
        if old == Some(s) {
            continue;
        }
        if s.life == Life::Removed {
            if old.is_some_and(|o| o.life != Life::Removed) {
                let id = s.id.0.to_string();
                hub.bus
                    .push(|seq| HubStreamMessage::SleeveRemoved { seq, id });
            }
            continue;
        }
        if s.parent.is_none() && old.is_none() {
            let workspace = wire::workspace(s);
            hub.bus
                .push(|seq| HubStreamMessage::Workspace { seq, workspace });
        }
        let sleeve = wire::sleeve(hub, after, s);
        hub.bus.push(|seq| HubStreamMessage::Sleeve { seq, sleeve });
    }
    for j in after.state.jobs.values() {
        if before.state.jobs.get(&j.id) != Some(j) {
            let job = crate::jobs::view(hub, j);
            hub.bus.push(|seq| HubStreamMessage::Job { seq, job });
        }
    }
    for a in after.state.approvals.values() {
        if before.state.approvals.get(&a.id) != Some(a) {
            let approval = crate::approvals::view(a);
            hub.bus
                .push(|seq| HubStreamMessage::Approval { seq, approval });
        }
    }
}

fn notice_text(view: &View, n: &Notice) -> (Kind, Level, String) {
    let name = |id: SleeveId| {
        view.state
            .sleeve(id)
            .map_or_else(|| id.0.to_string(), |s| s.name.clone())
    };
    match n {
        Notice::MessageRejected { request_id } => (
            Kind::Mail,
            Level::Warn,
            format!("message {request_id} was rejected by the harness"),
        ),
        Notice::MessageMayHaveApplied { request_id } => (
            Kind::Mail,
            Level::Warn,
            format!("message {request_id} may or may not have reached the harness"),
        ),
        Notice::EffectUnknowable { effect } => (
            Kind::Hub,
            Level::Alarm,
            format!("effect {} cannot be confirmed", effect.0),
        ),
        Notice::ApprovalRequested { approval } => (
            Kind::Approval,
            Level::Warn,
            match view
                .state
                .approvals
                .get(approval)
                .and_then(|a| a.ask.as_ref())
            {
                Some(ask) => format!("approval {approval} requested: {}", ask.title),
                None => format!("approval {approval} requested"),
            },
        ),
        Notice::ApprovalTimedOut { approval } => (
            Kind::Approval,
            Level::Warn,
            format!("approval {approval} timed out and was denied"),
        ),
        Notice::Tripped { guardrail } => (
            Kind::Guardrail,
            Level::Alarm,
            format!("guardrail G{} tripped ({guardrail:?})", guardrail.number()),
        ),
        Notice::Resumed { g } => (
            Kind::Sleeve,
            Level::Info,
            format!("resumed natively as g{}", g.0),
        ),
        Notice::Resleeved { g } => (Kind::Sleeve, Level::Warn, format!("resleeved as g{}", g.0)),
        Notice::Parked { reason } => (
            Kind::Sleeve,
            Level::Alarm,
            format!("parked ({})", wire::park_wire(*reason)),
        ),
        Notice::Stopped => (Kind::Sleeve, Level::Info, "stopped".into()),
        Notice::Removed => (Kind::Sleeve, Level::Info, "removed".into()),
        Notice::ChildCrashed { child } => (
            Kind::Sleeve,
            Level::Warn,
            format!("child {} crashed", name(*child)),
        ),
        Notice::ChildParked { child } => (
            Kind::Sleeve,
            Level::Warn,
            format!("child {} parked", name(*child)),
        ),
        Notice::ChildStopped { child } => (
            Kind::Sleeve,
            Level::Info,
            format!("child {} stopped", name(*child)),
        ),
        Notice::JobInterrupted { job } => {
            (Kind::Job, Level::Warn, format!("job {} interrupted", job.0))
        }
        Notice::JobLost { job } => (Kind::Job, Level::Alarm, format!("job {} lost", job.0)),
        Notice::JobFinished { job, ok } => (
            Kind::Job,
            if *ok { Level::Info } else { Level::Warn },
            match view.state.jobs.get(job).map(|j| j.state) {
                Some(pt_core::JobState::Failed(pt_core::JobFailure::NodeRemoved(n))) => {
                    format!("job {} failed: node {} was removed", job.0, n.0)
                }
                _ => format!("job {} {}", job.0, if *ok { "succeeded" } else { "failed" }),
            },
        ),
    }
}

/// A notice becomes a hub event on the stream and, for a sleeve, a line in its transcript.
fn notify(hub: &Hub, view: &View, to: Party, sleeve: Option<SleeveId>, n: &Notice) {
    let (kind, level, text) = notice_text(view, n);
    let target = match to {
        Party::Owner => "owner".to_string(),
        Party::Envoy => "envoy".to_string(),
        Party::Agent => "agent".to_string(),
        Party::Sleeve(x) => view
            .state
            .sleeve(x)
            .map_or_else(|| x.0.to_string(), |s| s.name.clone()),
    };
    let source = sleeve
        .and_then(|id| view.state.sleeve(id))
        .map_or_else(|| "hub".to_string(), |s| s.name.clone());
    info!("notice for {target} from {source}: {text}");
    hub.event(kind, level, &source, &target, &text);
    let at = now_ms();
    // Notices addressed to a parent about its child stay on the stream only.
    if let Some(id) = sleeve
        && !matches!(to, Party::Sleeve(_))
    {
        let g = view.state.sleeve(id).map_or(0, |s| s.g.0);
        hub.record(
            id,
            g,
            at,
            pt_drivers::Entry::Notice {
                code: Some(format!("hub:{kind}")),
                text,
            },
            Note {
                harness: None,
                level: Some(level),
            },
        );
    }
}
