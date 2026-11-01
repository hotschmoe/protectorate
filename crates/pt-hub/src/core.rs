//! The core thread: the one owner of `pt_core::State` (SPEC principle 3). It applies a batch of
//! inputs, journals the batch with one fsync, publishes the view, and only then releases the
//! batch's intents (pt-core README, SPEC 12b item 4).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use pt_core::{Event, Intent, Notice, Party, SleeveId, State, codec};
use pt_journal::Journal;
use pt_json::obj;

use crate::exec::Executors;
use crate::home::now_ms;
use crate::hub::{Hub, Input, View};
use crate::log::{debug, error, info};
use crate::wire;

const MAX_BATCH: usize = 512;

pub fn run(hub: Arc<Hub>, mut state: State, mut journal: Journal, rx: Receiver<Input>) {
    let mut exec = Executors::new(hub.clone());
    let mut batch_n = hub.view().batch;
    let mut since: BTreeMap<SleeveId, (&'static str, u64)> = BTreeMap::new();
    while let Ok(first) = rx.recv() {
        let mut inputs = vec![first];
        while inputs.len() < MAX_BATCH {
            match rx.try_recv() {
                Ok(i) => inputs.push(i),
                Err(_) => break,
            }
        }
        let before = hub.view();
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
            let kind = wire::activity_wire(&s.activity);
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
    }
    info!("core stopped");
}

/// Pushes one `sleeve` message per sleeve that changed in this batch.
fn announce_changes(hub: &Hub, before: &View, after: &View) {
    for s in after.state.sleeves.values() {
        if before.state.sleeves.get(&s.id) != Some(s) {
            hub.bus.push(obj! {
                "kind" => "sleeve",
                "at_ms" => now_ms(),
                "sleeve" => wire::sleeve(hub, after, s),
            });
        }
    }
}

fn notice_text(view: &View, n: &Notice) -> (&'static str, &'static str, String) {
    let name = |id: SleeveId| {
        view.state
            .sleeve(id)
            .map_or_else(|| id.0.to_string(), |s| s.name.clone())
    };
    match n {
        Notice::MessageRejected { request_id } => (
            "mail",
            "warn",
            format!("message {request_id} was rejected by the harness"),
        ),
        Notice::MessageMayHaveApplied { request_id } => (
            "mail",
            "warn",
            format!("message {request_id} may or may not have reached the harness"),
        ),
        Notice::EffectUnknowable { effect } => (
            "hub",
            "alarm",
            format!("effect {} cannot be confirmed", effect.0),
        ),
        Notice::ApprovalRequested { approval } => {
            ("approval", "warn", format!("approval {approval} requested"))
        }
        Notice::ApprovalTimedOut { approval } => (
            "approval",
            "warn",
            format!("approval {approval} timed out and was denied"),
        ),
        Notice::Tripped { guardrail } => (
            "guardrail",
            "alarm",
            format!("guardrail G{} tripped ({guardrail:?})", guardrail.number()),
        ),
        Notice::Resumed { g } => ("sleeve", "info", format!("resumed natively as g{}", g.0)),
        Notice::Resleeved { g } => ("sleeve", "warn", format!("resleeved as g{}", g.0)),
        Notice::Parked { reason } => (
            "sleeve",
            "alarm",
            format!("parked ({})", wire::park_wire(*reason)),
        ),
        Notice::Stopped => ("sleeve", "info", "stopped".into()),
        Notice::Removed => ("sleeve", "info", "removed".into()),
        Notice::ChildCrashed { child } => {
            ("sleeve", "warn", format!("child {} crashed", name(*child)))
        }
        Notice::ChildParked { child } => {
            ("sleeve", "warn", format!("child {} parked", name(*child)))
        }
        Notice::ChildStopped { child } => {
            ("sleeve", "info", format!("child {} stopped", name(*child)))
        }
        Notice::JobInterrupted { job } => ("job", "warn", format!("job {} interrupted", job.0)),
        Notice::JobLost { job } => ("job", "alarm", format!("job {} lost", job.0)),
        Notice::JobFinished { job, ok } => (
            "job",
            if *ok { "info" } else { "warn" },
            format!("job {} {}", job.0, if *ok { "succeeded" } else { "failed" }),
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
    let at = now_ms();
    hub.bus.push(obj! {
        "kind" => "event",
        "at_ms" => at,
        "event" => obj! {
            "seq" => hub.bus.seq() + 1,
            "at_ms" => at,
            "kind" => kind,
            "source" => source,
            "target" => target,
            "text" => text.as_str(),
            "read" => false,
            "level" => level,
        },
    });
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
        );
    }
}
