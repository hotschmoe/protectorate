//! Invariants I1-I10, the auto-continue rule, an owed-continuations reference kept by the
//! sim, and the table-driven reference transitions,
//! checked after every apply.
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
        // Parking a root freezes its children (12b item 10): no new message goes to them.
        if *attempt == 1
            && let EffectKind::SendPrompt {
                sleeve, request_id, ..
            }
            | EffectKind::Steer {
                sleeve, request_id, ..
            }
            | EffectKind::MailDeliver {
                sleeve, request_id, ..
            } = kind
        {
            let s = &sim.st.sleeves[sleeve];
            let frozen = s
                .parent
                .is_some_and(|p| matches!(sim.st.sleeves[&p].life, Life::Parked(_)));
            assert!(
                !(frozen && s.messages.contains_key(request_id)),
                "message {request_id} sent to {sleeve:?}, a child of a parked root"
            );
        }
    }
}

/// Records each automatic resleeve as the core decides it: the step that puts a sleeve into
/// Resleeving (I10). Its brief can come much later, after a long quiesce.
pub fn resleeves(prev: &State, sim: &mut Sim, idx: usize) {
    for (&sid, s) in &sim.st.sleeves {
        let entered = s.life == Life::Resleeving
            && prev.sleeves.get(&sid).map(|p| p.life) != Some(Life::Resleeving);
        if entered && s.guard.cause.is_some_and(|c| c != Cause::Manual) {
            sim.resleeves.push((idx, sid, sim.now));
        }
    }
}

/// Checks after a Tick or HubRecovered: every waiting state has re-issued its I/O (12a item 7).
pub fn timers(st: &State, tick: bool) {
    for j in st.jobs.values() {
        if matches!(
            j.state,
            JobState::Submitting | JobState::SubmitUncertain { .. }
        ) {
            let submit = j.submit.map(|e| st.effects[&e].state);
            assert!(
                submit.is_some_and(|s| !s.terminal()),
                "job {:?} is {:?} with its submit {submit:?}",
                j.id,
                j.state
            );
        }
        // No state waits on a node past its lease (12a item 6), counting time the hub was down.
        let expired = match j.state {
            JobState::Cancelling => st.now.saturating_sub(j.state_at) >= LEASE,
            JobState::Collecting { .. } => tick && st.now.saturating_sub(j.seen_at) >= LEASE,
            _ => false,
        };
        assert!(!expired, "job {:?} is {:?} past its lease", j.id, j.state);
        let cancel_wanted = matches!(
            j.state,
            JobState::Cancelling | JobState::Lost { .. } | JobState::Abandoned
        );
        let stuck = st.effects.values().find(|e| {
            let ours = match e.kind {
                EffectKind::NodeCancel { job, attempt, .. } => {
                    cancel_wanted && job == j.id && attempt == j.attempt
                }
                EffectKind::NodeAckCollected { job, attempt, .. } => {
                    job == j.id && attempt == j.attempt
                }
                _ => false,
            };
            ours && matches!(e.state, EffectState::Uncertain | EffectState::Probing)
        });
        if let Some(e) = stuck {
            panic!(
                "job {:?} is {:?} and its node write {} is {:?}, not re-sent",
                j.id, j.state, e.key, e.state
            );
        }
    }
    // An approval past its journaled deadline has timed out, unless it waits on a live
    // incarnation that G9 does not cover (a pane, or a request outside a blocked turn).
    for a in st.approvals.values() {
        if a.state != ApprovalState::Pending {
            continue;
        }
        let s = &st.sleeves[&a.sleeve];
        assert!(
            s.life.live(),
            "approval {} pending on a {:?} sleeve",
            a.id,
            s.life
        );
        if !a.due(st.now) {
            continue;
        }
        let Source::Tool { g, native } = &a.source else {
            panic!("action approval {} pending past its deadline", a.id);
        };
        assert!(
            a.orphan.is_none(),
            "orphaned approval {} pending past its deadline",
            a.id
        );
        let g9 = s.g == *g
            && s.life == Life::Running
            && s.target.mode == Mode::Headless
            && matches!(&s.activity, Activity::Blocked { approvals } if approvals.contains(native));
        assert!(!(tick && g9), "approval {} pending past G9", a.id);
    }
    // A sleeve write a probe could not settle is sent again or probed again until the owner
    // has been told.
    if tick {
        for e in st.effects.values() {
            if e.state == EffectState::Uncertain && e.kind.sleeve().is_some() {
                assert!(
                    e.unknowable >= UNKNOWABLE_NOTIFY,
                    "effect {} left Uncertain by a Tick after {} unknowable probes",
                    e.key,
                    e.unknowable
                );
            }
        }
    }
}

/// Checks after one apply: transitions against the reference tables, then I1-I10.
pub fn step(prev: &State, st: &State, sim: &Sim) {
    for j in st.jobs.values().filter(|j| j.state == JobState::Abandoned) {
        if prev.jobs.get(&j.id).is_some_and(|p| p.state != j.state) {
            for e in prev.effects.values() {
                if matches!(
                    e.kind,
                    EffectKind::NodeCancel { job, attempt, .. }
                        if job == j.id && attempt == j.attempt
                ) {
                    assert!(st.effects[&e.id].state.terminal());
                }
            }
        }
    }
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
    unsent_messages(prev, st);
    approval_steps(prev, st);
    continue_prompts(prev, st, sim);
    owed_continuations(st, sim);
    invariants(st, sim);
}

/// At most one live continuation prompt per owner turn. An unsent one may be sent
/// again, under the same key, after its generation dies. A new one is owed by the previous state
/// or is that re-delivery.
fn continue_prompts(prev: &State, st: &State, sim: &Sim) {
    for s in st.sleeves.values() {
        let mut live = BTreeSet::new();
        for e in st.effects.values() {
            let EffectKind::SendPrompt {
                sleeve,
                g,
                request_id,
                kind: MsgKind::Continuation { turn: seq },
                ..
            } = &e.kind
            else {
                continue;
            };
            if *sleeve != s.id {
                continue;
            }
            let seq = *seq;
            if e.state != EffectState::Abandoned {
                assert!(
                    live.insert(seq),
                    "two continue effects for {:?} turn {seq}",
                    s.id
                );
            }
            if prev.effects.contains_key(&e.id) {
                continue;
            }
            let ps = &prev.sleeves[&s.id];
            let redelivery = ps
                .messages
                .get(request_id)
                .is_some_and(|m| m.state == MsgState::Queued);
            assert!(
                g.0 >= 2 && !sim.alive.contains(&(s.id, Gen(g.0 - 1))),
                "continue for {:?} turn {seq} but generation {} did not die",
                s.id,
                g.0 - 1
            );
            if redelivery {
                continue;
            }
            assert_eq!(
                ps.auto_continue,
                Some(seq),
                "continue for {:?} turn {seq} without a debt from an interrupted turn",
                s.id
            );
            assert_eq!(
                sim.owed_before.get(&s.id).copied(),
                Some(seq),
                "continue for {:?} turn {seq} was not owed by the reference ({:?})",
                s.id,
                sim.last_kind
            );
            assert_eq!(ps.target.mode, Mode::Headless);
            assert_eq!(
                ps.life,
                Life::Running,
                "continue for {:?} turn {seq} while {:?}",
                s.id,
                ps.life
            );
            assert_eq!(
                st.sleeves[&s.id].messages[request_id].from, ps.turn_from,
                "continue for {:?} turn {seq} is sent as {:?}, the turn was opened by {:?}",
                s.id, st.sleeves[&s.id].messages[request_id].from, ps.turn_from
            );
        }
    }
}

/// The core's `auto_continue` matches the reference kept on `Sim`.
fn owed_continuations(st: &State, sim: &Sim) {
    for (id, s) in &st.sleeves {
        assert_eq!(
            s.auto_continue,
            sim.owed.get(id).copied(),
            "owed continuation for {id:?} after {}: core {:?} reference {:?}",
            sim.last_kind,
            s.auto_continue,
            sim.owed.get(id).copied()
        );
    }
    for id in sim.owed.keys() {
        assert!(
            st.sleeves.contains_key(id),
            "reference owes a continuation for missing sleeve {id:?}"
        );
    }
}

/// A message whose prompt or steer record ended while still Intent (never dispatched) is back
/// in its queue: neither lost nor shown as "may have applied".
fn unsent_messages(prev: &State, st: &State) {
    for (sid, ps) in &prev.sleeves {
        let Some(s) = st.sleeves.get(sid) else {
            continue;
        };
        for (r, pm) in &ps.messages {
            let MsgState::InFlight(e) = pm.state else {
                continue;
            };
            let unsent = prev.effects.get(&e).map(|x| x.state) == Some(EffectState::Intent);
            let ended = st
                .effects
                .get(&e)
                .is_some_and(|x| x.state.terminal() && x.state != EffectState::Confirmed);
            if !unsent || !ended {
                continue;
            }
            let m = s.messages.get(r).map(|m| m.state);
            // A stop, park, or resleeve drops an unsent continuation instead of sending it
            // on the next start. It is Rejected, with no "may have applied" notice.
            let dropped_continue = matches!(pm.kind, MsgKind::Continuation { .. })
                && matches!(m, Some(MsgState::Rejected))
                && matches!(
                    s.life,
                    Life::Parked(_)
                        | Life::Stopping { .. }
                        | Life::Stopped
                        | Life::Removed
                        | Life::Resleeving
                );
            if dropped_continue {
                continue;
            }
            assert!(
                matches!(
                    m,
                    Some(MsgState::Queued | MsgState::SteerHeld | MsgState::InFlight(_))
                ),
                "message {r} of {sid:?} was never dispatched, yet is {m:?}"
            );
        }
    }
}

/// The ledger only grows; an approval leaves Pending once, its decision is set at most once and
/// never changes, and only a definitively refused response turns Decided into Fenced.
fn approval_steps(prev: &State, st: &State) {
    for (id, p) in &prev.approvals {
        let a = st
            .approvals
            .get(id)
            .unwrap_or_else(|| panic!("approval {id} vanished"));
        assert_eq!(
            (a.sleeve, &a.source, a.requested_at, a.deadline),
            (p.sleeve, &p.source, p.requested_at, p.deadline),
            "approval {id} changed what it asks"
        );
        if p.state == ApprovalState::Pending {
            assert!(
                p.orphan.is_none() || a.orphan == p.orphan,
                "approval {id}: orphan changed"
            );
            continue;
        }
        assert_eq!(a.decision, p.decision, "approval {id}: decision changed");
        assert!(
            a.state == p.state
                || (p.state == ApprovalState::Decided && a.state == ApprovalState::Fenced),
            "approval {id}: {:?} -> {:?}",
            p.state,
            a.state
        );
    }
    for (id, a) in &st.approvals {
        if !prev.approvals.contains_key(id) {
            assert_eq!(
                a.state,
                ApprovalState::Pending,
                "approval {id} opened as {:?}",
                a.state
            );
        }
    }
}

/// Approvals against the rest of the state.
fn approvals(st: &State, sim: &Sim) {
    for (id, a) in &st.approvals {
        assert_eq!(&a.id, id);
        // A tool approval times out with its sleeve's blocked episode (G9), which may have
        // begun well before this request.
        assert!(
            a.source != Source::Action || a.deadline >= a.requested_at,
            "approval {id}: deadline before its request"
        );
        match a.state {
            ApprovalState::Pending | ApprovalState::Cancelled => {
                assert!(
                    a.decision.is_none(),
                    "approval {id}: {:?} with a decision",
                    a.state
                )
            }
            ApprovalState::Decided | ApprovalState::Fenced => {
                assert!(
                    a.decision.is_some(),
                    "approval {id}: {:?} without a decision",
                    a.state
                )
            }
            ApprovalState::Expired => assert!(
                a.decision
                    .is_some_and(|d| !d.allow && d.by == Decider::Rule),
                "approval {id} expired without a rule's denial"
            ),
            ApprovalState::Abandoned => assert!(
                a.decision.is_none_or(|d| d.by == Decider::Owner),
                "approval {id} abandoned by a rule"
            ),
        }
        let response = a.decision.and_then(|d| d.response.map(|e| (d, e)));
        match (&a.source, response) {
            (Source::Action, r) => {
                assert!(r.is_none(), "action approval {id} wrote to a harness");
                assert!(id.starts_with(ACTION_PREFIX));
            }
            (Source::Tool { g, native }, Some((d, eff))) => {
                // A tool decision goes only to the incarnation that asked.
                let e = &st.effects[&eff];
                assert!(
                    matches!(&e.kind, EffectKind::RespondApproval { sleeve, g: eg, approval, allow, .. }
                        if *sleeve == a.sleeve && eg == g && approval == native && *allow == d.allow),
                    "approval {id}: response {} is not its answer",
                    e.key
                );
                if a.state == ApprovalState::Fenced {
                    assert_eq!(
                        e.state,
                        EffectState::Rejected,
                        "approval {id} fenced with a response that may apply"
                    );
                }
            }
            (Source::Tool { g, native }, None) => {
                assert_eq!(id, &tool_approval_id(a.sleeve, *g, native));
                assert!(
                    a.state != ApprovalState::Decided,
                    "tool approval {id} decided without a response"
                );
            }
        }
        // Whoever waits on a pending approval is marked once it is gone.
        if a.state == ApprovalState::Pending
            && let Source::Tool { g, .. } = a.source
        {
            let s = &st.sleeves[&a.sleeve];
            if s.g != g || s.proc == Proc::Down {
                assert!(
                    a.orphan.is_some(),
                    "approval {id}: g{} is gone but the approval is not orphaned",
                    g.0
                );
            }
        }
    }
    // Every RespondApproval record is a ledger decision's response.
    for e in st.effects.values() {
        if let EffectKind::RespondApproval {
            sleeve,
            g,
            approval,
            ..
        } = &e.kind
        {
            let id = tool_approval_id(*sleeve, *g, approval);
            assert!(
                st.approvals
                    .get(&id)
                    .and_then(|a| a.decision)
                    .is_some_and(|d| d.response == Some(e.id)),
                "response {} is not an approval's decision",
                e.key
            );
        }
    }
    // The current incarnation's blocked requests are in the ledger.
    for s in st.sleeves.values() {
        if let Activity::Blocked { approvals } = &s.activity {
            for native in approvals {
                let id = tool_approval_id(s.id, s.g, native);
                assert!(
                    st.approvals.contains_key(&id),
                    "blocked on {id}, not in the ledger"
                );
            }
        }
    }
    // A response that left the hub went to the incarnation that asked, while it was current.
    for (eff, at) in &sim.responses {
        let e = &st.effects[eff];
        let EffectKind::RespondApproval { g, .. } = e.kind else {
            unreachable!()
        };
        assert_eq!(*at, g, "response {} reached g{}", e.key, at.0);
    }
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
            if p.d9 {
                assert!(s.d9, "a child of a D9-marked sleeve is unmarked");
            }
        }
        assert!(
            s.d9_turn.is_none() || (s.name == ENVOY_NAME && s.parent.is_none()),
            "{:?} has a D9 turn and is not Envoy",
            s.id
        );
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

    // A write the harness or a node acted on reached it: a record confirmed by evidence (harness
    // output, a node's report) was dispatched. Idempotent records count as applied once their
    // target is gone, and a start is confirmed by its scope, so neither is checked here.
    for e in st.effects.values() {
        let checked = !matches!(
            e.kind,
            EffectKind::StartIncarnation { .. } | EffectKind::NodeSubmit { .. }
        ) && e.class != Class::Idempotent;
        if checked && e.state == EffectState::Confirmed {
            assert!(
                sim.dispatched.contains(&e.id),
                "effect {:?} ({}) confirmed but never dispatched",
                e.id,
                e.key
            );
        }
    }
    // A job runs on the node that was sent its submit.
    for j in st.jobs.values() {
        if matches!(j.state, JobState::Running | JobState::Collecting { .. }) {
            let node = j.node.expect("a running job has a node");
            assert!(
                sim.submitted.contains(&(j.id, j.attempt, node)),
                "job {:?} attempt {} is {:?} on {node:?}, which was never sent it",
                j.id,
                j.attempt,
                j.state
            );
        }
        if matches!(
            j.state,
            JobState::Collecting { .. }
                | JobState::Succeeded
                | JobState::Failed(_)
                | JobState::Interrupted
                | JobState::Cancelled
        ) {
            for e in st.effects.values() {
                if matches!(
                    e.kind,
                    EffectKind::NodeCancel { job, attempt, .. }
                        if job == j.id && attempt == j.attempt
                ) {
                    assert!(
                        e.state.terminal(),
                        "job {:?} is {:?} with open cancel {} ({:?})",
                        j.id,
                        j.state,
                        e.key,
                        e.state
                    );
                }
            }
        }
    }
    // An Unsafe write is issued once: no second approval response while one may apply (12b
    // item 1). An abandoned record is the owner's explicit release.
    let mut answers = BTreeSet::new();
    for e in st.effects.values() {
        if let EffectKind::RespondApproval {
            sleeve,
            g,
            approval,
            ..
        } = &e.kind
            && matches!(
                e.state,
                EffectState::Intent
                    | EffectState::Sent
                    | EffectState::Uncertain
                    | EffectState::Probing
                    | EffectState::Confirmed
            )
        {
            assert!(
                answers.insert((*sleeve, *g, approval.clone())),
                "two live responses to approval {approval} of {sleeve:?} g{}",
                g.0
            );
        }
    }

    approvals(st, sim);

    // I10: automatic resleeves per sleeve in any hour.
    let mut by_sleeve: BTreeMap<SleeveId, Vec<Millis>> = BTreeMap::new();
    for &(_, s, t) in &sim.resleeves {
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
        // A record abandoned unsent (its message went back to the queue) leaves room for one more.
        if msg && e.state != EffectState::Abandoned {
            assert!(
                keys.insert(e.key.clone()),
                "I9: {} has two delivery records",
                e.key
            );
        }
    }
}
