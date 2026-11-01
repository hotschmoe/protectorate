//! Invariants I1-I10 and the table-driven reference transitions, checked after every apply.
//!
//! The reference tables are the edges of the design doc's lifecycle and effect diagrams (with the
//! SPEC 12a/12b corrections). One apply may take several documented edges (a crash that resumes at
//! once goes Running, Recovering, Launching), so a step must be a path of at most `MAX_HOPS`
//! documented edges.

use std::collections::{BTreeMap, BTreeSet};

use pt_core::limits::*;
use pt_core::*;

use crate::sim::Sim;

const MAX_HOPS: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum L {
    None,
    Provisioning,
    Launching,
    Running,
    Recovering,
    Resleeving,
    Stopping,
    Stopped,
    Parked,
    Removed,
}

fn life(l: Option<Life>) -> L {
    match l {
        None => L::None,
        Some(Life::Provisioning) => L::Provisioning,
        Some(Life::Launching) => L::Launching,
        Some(Life::Running) => L::Running,
        Some(Life::Recovering) => L::Recovering,
        Some(Life::Resleeving) => L::Resleeving,
        Some(Life::Stopping { .. }) => L::Stopping,
        Some(Life::Stopped) => L::Stopped,
        Some(Life::Parked(_)) => L::Parked,
        Some(Life::Removed) => L::Removed,
    }
}

/// Sleeve lifecycle edges (2.a). Rows added for the corrections: park-class trips from Launching,
/// Running and Recovering; a start failure while Launching resleeves (G5).
const LIFE_EDGES: &[(L, L)] = &[
    (L::None, L::Provisioning),
    (L::Provisioning, L::Launching),
    (L::Provisioning, L::Parked),
    (L::Provisioning, L::Stopping),
    (L::Launching, L::Running),
    (L::Launching, L::Recovering),
    (L::Launching, L::Resleeving),
    (L::Launching, L::Parked),
    (L::Launching, L::Stopping),
    (L::Running, L::Recovering),
    (L::Running, L::Resleeving),
    (L::Running, L::Parked),
    (L::Running, L::Stopping),
    (L::Recovering, L::Launching),
    (L::Recovering, L::Resleeving),
    (L::Recovering, L::Parked),
    (L::Recovering, L::Stopping),
    (L::Resleeving, L::Running),
    (L::Resleeving, L::Parked),
    (L::Resleeving, L::Stopping),
    (L::Parked, L::Launching),
    (L::Parked, L::Stopping),
    (L::Stopping, L::Stopped),
    (L::Stopped, L::Launching),
    (L::Stopped, L::Removed),
];

/// Effect record edges (2.c). "Evidence" edges (a late ack or harness output) confirm from
/// Intent, Uncertain or Probing; Idempotent stop-like records count as applied once their
/// target is dead.
const EFFECT_EDGES: &[(EffectState, EffectState)] = {
    use EffectState::*;
    &[
        (Intent, Sent),
        (Intent, Uncertain),
        (Intent, Fenced),
        (Intent, Confirmed),
        (Sent, Confirmed),
        (Sent, Rejected),
        (Sent, Uncertain),
        (Sent, Fenced),
        (Uncertain, Probing),
        (Uncertain, Intent),
        (Uncertain, Abandoned),
        (Uncertain, Fenced),
        (Uncertain, Confirmed),
        (Probing, Probing),
        (Probing, Confirmed),
        (Probing, NotApplied),
        (Probing, Uncertain),
        (Probing, Fenced),
        (NotApplied, Intent),
        (NotApplied, Failed),
    ]
};

fn reachable<T: Copy + Eq>(
    edges: &[(T, T)],
    from: T,
    to: T,
    allowed: impl Fn(T, T) -> bool,
) -> bool {
    if from == to {
        return true;
    }
    let mut frontier = vec![from];
    for _ in 0..MAX_HOPS {
        let mut next = Vec::new();
        for &f in &frontier {
            for &(a, b) in edges {
                if a == f && allowed(a, b) {
                    if b == to {
                        return true;
                    }
                    next.push(b);
                }
            }
        }
        frontier = next;
    }
    false
}

/// Checks on the intents of one apply.
pub fn intents(sim: &mut Sim, idx: usize, out: &[Intent]) {
    for i in out {
        let Intent::Execute {
            effect,
            attempt,
            kind,
        } = i
        else {
            continue;
        };
        if *attempt == 1 {
            sim.emitted.entry(*effect).or_insert(idx);
        }
        if let EffectKind::StartIncarnation { sleeve, g, .. } = kind {
            // I5: a new start only when nothing of this sleeve is alive.
            assert!(
                !sim.alive.iter().any(|(s, _)| s == sleeve),
                "I5: StartIncarnation({sleeve:?}, {g:?}) while an incarnation is alive"
            );
        }
        if let Some((s, g)) = kind.incarnation() {
            // I6: no effect for a generation that is no longer current.
            assert_eq!(sim.st.sleeves[&s].g, g, "I6: effect for a stale generation");
        }
        if let EffectKind::ContinuationBrief { sleeve, cause, .. } = kind
            && *cause != Cause::Manual
            && *attempt == 1
        {
            sim.briefs.push((idx, *sleeve, sim.now));
        }
    }
}

/// Checks after one apply: transitions against the reference tables, then I1-I10.
pub fn step(prev: &State, st: &State, sim: &Sim) {
    // Reference transitions, and I6 (generation only grows).
    for (id, s) in &st.sleeves {
        let before = prev.sleeves.get(id);
        let (a, b) = (life(before.map(|p| p.life)), life(Some(s.life)));
        assert!(
            reachable(LIFE_EDGES, a, b, |_, _| true),
            "sleeve {id:?}: {a:?} -> {b:?} is not a path in the lifecycle table"
        );
        if let Some(p) = before {
            assert!(p.g <= s.g, "I6: generation went down");
        }
    }
    for (id, e) in &st.effects {
        assert!(
            sim.emitted.contains_key(id),
            "I7: effect {id:?} was never emitted as an Intent"
        );
        let unsafe_ = e.class == Class::Unsafe;
        if unsafe_ {
            assert_eq!(e.attempt, 1, "I8: Unsafe effect {id:?} retried");
        }
        let Some(p) = prev.effects.get(id) else {
            assert!(
                matches!(e.state, EffectState::Intent)
                    || e.state.terminal()
                    || matches!(
                        e.state,
                        EffectState::Probing | EffectState::Uncertain | EffectState::Sent
                    ),
                "new effect {id:?} in {:?}",
                e.state
            );
            continue;
        };
        assert!(
            !(p.state.terminal() && p.state != e.state),
            "effect {id:?} left terminal {:?} for {:?}",
            p.state,
            e.state
        );
        let ok = reachable(EFFECT_EDGES, p.state, e.state, |a, b| {
            !(unsafe_
                && b == EffectState::Intent
                && matches!(
                    a,
                    EffectState::Uncertain | EffectState::Probing | EffectState::NotApplied
                ))
        });
        assert!(
            ok,
            "effect {id:?} ({:?}): {:?} -> {:?} is not a path in the effect table",
            e.class, p.state, e.state
        );
        if p.state != EffectState::Intent || e.state != EffectState::Intent {
            assert!(e.attempt >= p.attempt, "effect attempt went down");
        }
    }
    invariants(st, sim);
}

pub fn invariants(st: &State, sim: &Sim) {
    let not_removed: Vec<&Sleeve> = st
        .sleeves
        .values()
        .filter(|s| s.life != Life::Removed)
        .collect();
    for s in &not_removed {
        if let Some(pid) = s.parent {
            let p = &st.sleeves[&pid];
            // I1
            assert!(
                p.life != Life::Removed,
                "I1: {:?} has a Removed parent",
                s.id
            );
            // I2
            if !p.life.live() {
                assert!(
                    !s.life.live(),
                    "I2: parent {pid:?} is {:?} but child {:?} is {:?}",
                    p.life,
                    s.id,
                    s.life
                );
            }
            // I3
            assert!(p.parent.is_none(), "I3: depth above 1");
        }
        if s.parent.is_none() {
            assert!(
                st.live_children(s.id) <= MAX_CHILDREN,
                "I3: more than 8 live children"
            );
        }
    }
    // I4: writable claims (and root directories) never overlap.
    for (i, a) in not_removed.iter().enumerate() {
        for b in &not_removed[i + 1..] {
            let wa = a.claim.writable || a.parent.is_none();
            let wb = b.claim.writable || b.parent.is_none();
            if wa && wb {
                assert!(
                    !paths_overlap(&a.claim.path, &b.claim.path),
                    "I4: {} and {} overlap",
                    a.claim.path,
                    b.claim.path
                );
            }
        }
    }
    // I5: alive incarnations are the current generation, and the core knows they may live.
    let mut per_sleeve = BTreeMap::new();
    for &(s, g) in &sim.alive {
        *per_sleeve.entry(s).or_insert(0) += 1;
        let sl = &st.sleeves[&s];
        assert_eq!(sl.g, g, "I5: a stale generation is alive");
        assert!(
            sl.proc != Proc::Down,
            "I5: core thinks {s:?} g{} is dead",
            g.0
        );
    }
    assert!(
        per_sleeve.values().all(|&n| n <= 1),
        "I5: two incarnations alive"
    );

    for s in st.sleeves.values() {
        if matches!(s.life, Life::Running | Life::Launching) {
            assert!(
                s.proc != Proc::Down,
                "{:?} is {:?} with no incarnation",
                s.id,
                s.life
            );
        }
        // One prompt in flight per sleeve (12b item 3).
        let in_flight: Vec<&Effect> = st
            .effects
            .values()
            .filter(|e| {
                !e.state.terminal()
                    && matches!(&e.kind, EffectKind::SendPrompt { sleeve, .. } if *sleeve == s.id)
            })
            .collect();
        assert!(in_flight.len() <= 1, "{:?}: two prompts in flight", s.id);
        if let Activity::Delivering { effect } = s.activity {
            let e = &st.effects[&effect];
            assert!(
                !e.state.terminal(),
                "Delivering on a finished effect {:?}",
                e.state
            );
            assert!(
                matches!(&e.kind, EffectKind::SendPrompt { sleeve, g, .. } if *sleeve == s.id && *g == s.g)
            );
        }
        if s.proc == Proc::Down {
            assert_eq!(
                s.activity,
                Activity::Ended,
                "{:?}: activity without an incarnation",
                s.id
            );
        }
        i9(st, s);
    }

    // I10: automatic resleeves per sleeve in any hour.
    let mut by_sleeve: BTreeMap<SleeveId, Vec<Millis>> = BTreeMap::new();
    for &(_, s, t) in &sim.briefs {
        by_sleeve.entry(s).or_default().push(t);
    }
    for times in by_sleeve.values() {
        for (i, &t) in times.iter().enumerate() {
            let n = times[i..]
                .iter()
                .take_while(|&&u| u.saturating_sub(t) < RESLEEVE_WINDOW)
                .count();
            assert!(
                n <= RESLEEVE_LIMIT,
                "I10: {n} automatic resleeves within an hour"
            );
        }
    }
}

/// I9: every message is in exactly one place, and no message has two delivery records.
fn i9(st: &State, s: &Sleeve) {
    let mut queued = BTreeSet::new();
    for r in &s.queue {
        assert!(queued.insert(r.clone()), "I9: {r} queued twice");
        assert_eq!(
            s.messages[r].state,
            MsgState::Queued,
            "I9: queued {r} not Queued"
        );
    }
    if let Some(r) = &s.steer_slot {
        assert!(!queued.contains(r), "I9: {r} both queued and held");
        assert_eq!(s.messages[r].state, MsgState::SteerHeld);
    }
    for (r, m) in &s.messages {
        match m.state {
            MsgState::Queued => assert!(queued.contains(r), "I9: {r} Queued but not in the queue"),
            MsgState::SteerHeld => assert_eq!(s.steer_slot.as_ref(), Some(r)),
            MsgState::InFlight(e) => {
                let e = &st.effects[&e];
                assert!(
                    !e.state.terminal(),
                    "I9: {r} in flight on a finished record"
                );
                assert_eq!(&e.key, r);
            }
            _ => {}
        }
    }
    let mut keys = BTreeSet::new();
    for e in st.effects.values() {
        let msg = match &e.kind {
            EffectKind::SendPrompt { sleeve, .. }
            | EffectKind::Steer { sleeve, .. }
            | EffectKind::MailDeliver { sleeve, .. } => *sleeve == s.id,
            _ => false,
        };
        if msg {
            assert!(
                keys.insert(e.key.clone()),
                "I9: {} has two delivery records",
                e.key
            );
        }
    }
}
