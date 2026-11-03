//! Which owner turn still owes a continuation, per sleeve.
//!
//! Updated from the event and the pre-state (activity, turn, messages, life, guard). It never
//! reads `Sleeve::auto_continue`, so a core that stays internally consistent while breaking the
//! continuation rules still fails the check.

use std::collections::BTreeMap;

use pt_core::limits::*;
use pt_core::*;

pub fn note(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, kind: &EventKind, now: Millis) {
    use EventKind as E;
    match kind {
        E::Send {
            to,
            request_id,
            from,
            text,
            mode,
            ..
        } => on_send(owed, prev, *to, request_id, *from, text, *mode),
        E::Report {
            sleeve,
            to,
            turn,
            text,
        } => on_report(owed, prev, *sleeve, *to, *turn, text),
        E::Stop { sleeve } => on_stop(owed, prev, *sleeve, false),
        E::Remove { sleeve } => on_stop(owed, prev, *sleeve, true),
        E::Start { sleeve, target, .. } => on_start_cmd(owed, prev, *sleeve, target.as_ref()),
        E::Resleeve { sleeve, target, .. } => on_resleeve(owed, prev, *sleeve, target.as_ref()),
        E::Abandon { effect } => on_abandon(owed, prev, *effect),
        E::Acked {
            effect, attempt, ..
        } => {
            if let Some(seen) = report_seen(prev, *effect, *attempt, Report::Acked) {
                on_effect(owed, prev, *effect, seen);
            }
        }
        E::Nacked {
            effect, attempt, ..
        } => {
            if let Some(seen) = report_seen(prev, *effect, *attempt, Report::Nacked) {
                on_effect(owed, prev, *effect, seen);
            }
        }
        E::Lost { effect, attempt } => {
            if let Some(seen) = report_seen(prev, *effect, *attempt, Report::Lost) {
                on_effect(owed, prev, *effect, seen);
            }
        }
        E::Probed {
            effect,
            attempt,
            result,
        } => {
            if let Some(seen) = report_seen(prev, *effect, *attempt, Report::Probed(*result)) {
                on_effect(owed, prev, *effect, seen);
            }
        }
        E::ScopeProbed { sleeve, g, live } => on_scope(owed, prev, *sleeve, *g, *live, now),
        E::TargetChosen { sleeve, g, chosen } => {
            let target = match chosen {
                Chosen::Target(t) => Some(t),
                Chosen::NoTarget | Chosen::Ineligible(_) => None,
            };
            on_target(owed, prev, *sleeve, *g, target)
        }
        E::Observed { sleeve, g, obs } => on_obs(owed, prev, *sleeve, *g, obs, now),
        E::Exited { sleeve, g, cause } => on_exit(owed, prev, *sleeve, *g, *cause, None, now),
        E::Tick => on_tick(owed, prev, now),
        E::HubRecovered => on_recovered(owed, prev),
        E::Continue { sleeve, text } => on_continue(owed, prev, *sleeve, text),
        E::Spawn(_)
        | E::Interrupt { .. }
        | E::Compact { .. }
        | E::RespondApproval { .. }
        | E::RequestApproval { .. }
        | E::Viewed { .. }
        | E::CreateJob { .. }
        | E::CancelJob { .. }
        | E::AbandonJob { .. }
        | E::RequeueJob { .. }
        | E::Dispatched { .. }
        | E::Unwritten { .. }
        | E::Launched { .. }
        | E::NodeChosen { .. }
        | E::JobNodeRemoved { .. }
        | E::JobStatus { .. }
        | E::ArtifactsChecked { .. }
        | E::ArtifactCollection { .. } => {}
    }
}

fn clear(owed: &mut BTreeMap<SleeveId, u64>, sid: SleeveId) {
    owed.remove(&sid);
}

fn frozen(st: &State, sid: SleeveId) -> bool {
    st.sleeves[&sid]
        .parent
        .is_some_and(|p| matches!(st.sleeves[&p].life, Life::Parked(_)))
}

fn deliverable(st: &State, s: &Sleeve) -> bool {
    s.life == Life::Running
        && s.proc == Proc::Up
        && !s.probing
        && matches!(s.activity, Activity::Idle { .. })
        && s.target.mode == Mode::Headless
        && matches!(
            s.guard.state,
            GuardState::Armed | GuardState::Cooldown { .. }
        )
        && !frozen(st, s.id)
}

/// A dequeued prompt other than a continuation supersedes the debt.
fn note_dequeue(owed: &mut BTreeMap<SleeveId, u64>, st: &State, s: &Sleeve) {
    if s.target.mode == Mode::Pane || !deliverable(st, s) {
        return;
    }
    let prompt = |k: &String| s.messages.get(k).is_some_and(|m| m.kind == MsgKind::Prompt);
    if s.queue.front().is_some_and(prompt) {
        clear(owed, s.id);
    }
}

fn proc_up_after_evidence(st: &State, s: &Sleeve) -> bool {
    match s.proc {
        Proc::Up => true,
        Proc::Starting(id) => st.effects.get(&id).is_some_and(|e| !e.state.terminal()),
        Proc::Down => false,
    }
}

fn recent_count(v: &[Millis], now: Millis, window: Millis) -> usize {
    v.iter()
        .filter(|&&t| now.saturating_sub(t) < window)
        .count()
}

fn running(s: &Sleeve) -> bool {
    matches!(
        s.life,
        Life::Launching | Life::Running | Life::Resleeving | Life::Recovering
    )
}

fn armed(s: &Sleeve) -> bool {
    matches!(
        s.guard.state,
        GuardState::Armed | GuardState::Cooldown { .. }
    )
}

/// `begin_resleeve` clears the debt. A resleeve trip does that only when it fires.
fn resleeve_trip_clears(s: &Sleeve) -> bool {
    if !running(s) {
        return false;
    }
    let active = armed(s)
        || matches!(
            s.guard.state,
            GuardState::Interrupting | GuardState::Resuming
        );
    matches!(s.life, Life::Launching | Life::Running) && active
}

fn park_trip_clears(s: &Sleeve) -> bool {
    running(s)
}

/// An interrupt trip resleeves once the strike window is full, and that clears the debt.
fn interrupt_resleeves(s: &Sleeve, now: Millis) -> bool {
    let busy = matches!(s.activity, Activity::Working | Activity::Blocked { .. });
    if s.life != Life::Running || !armed(s) || !busy || s.target.mode == Mode::Pane {
        return false;
    }
    recent_count(&s.guard.strikes, now, STRIKE_WINDOW) >= STRIKE_LIMIT
}

fn continuation(s: &Sleeve, seq: u64) -> Option<&Message> {
    s.messages
        .values()
        .find(|m| m.kind == MsgKind::Continuation { turn: seq })
}

fn continue_state(s: &Sleeve, seq: u64) -> Option<MsgState> {
    continuation(s, seq).map(|m| m.state)
}

fn death_cause(s: &Sleeve, cause: ExitCause, was: &Activity, kill: Option<Cause>) -> Cause {
    if let Some(c) = kill.or(s.kill_cause) {
        return c;
    }
    match cause {
        ExitCause::Oom => Cause::Guard(Guardrail::Oom),
        ExitCause::Auth => Cause::Guard(Guardrail::AuthFailed),
        ExitCause::Quota => Cause::Guard(Guardrail::QuotaExhausted),
        _ if *was == Activity::Booting => Cause::Guard(Guardrail::StartFailed),
        _ => Cause::Guard(Guardrail::Crash),
    }
}

/// `after_death` took a path that clears the debt (resleeve or park) instead of resuming.
fn death_clears(s: &Sleeve, cause: Cause, now: Millis) -> bool {
    if matches!(
        cause,
        Cause::Guard(Guardrail::AuthFailed | Guardrail::QuotaExhausted)
    ) {
        return true;
    }
    if s.guard.state == GuardState::Resuming && cause != Cause::HubRestart {
        return true;
    }
    if matches!(cause, Cause::Guard(Guardrail::StartFailed)) {
        return true;
    }
    if matches!(cause, Cause::Guard(Guardrail::Oom))
        && recent_count(&s.guard.ooms, now, OOM_WINDOW) > 0
    {
        return true;
    }
    recent_count(&s.guard.resumes, now, RESUME_WINDOW) >= RESUME_LIMIT
}

fn on_exit(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    g: Gen,
    cause: ExitCause,
    kill: Option<Cause>,
    now: Millis,
) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.g != g || s.proc == Proc::Down {
        return;
    }
    let was = s.foreign.as_ref().map_or(&s.activity, |f| &f.previous);
    let turn = s
        .foreign
        .as_ref()
        .map_or(s.turn.as_ref(), |f| f.native.as_ref());
    let delivery_sent = matches!(was, Activity::Delivering { effect } if prev.effects.get(effect).map(|e| e.state) != Some(EffectState::Intent));
    let interrupted = *was != Activity::Interrupting
        && (turn.is_some()
            || s.desync_during_turn
            || delivery_sent
            || matches!(was, Activity::Working | Activity::Blocked { .. }));
    let reached_idle = matches!(
        was,
        Activity::Idle { .. }
            | Activity::Delivering { .. }
            | Activity::Working
            | Activity::Blocked { .. }
            | Activity::Interrupting
            | Activity::Compacting
    ) || (*was == Activity::Unknown
        && (s.desync_during_turn || s.desync_while_idle));
    match s.life {
        Life::Launching | Life::Running | Life::Recovering => {}
        Life::Resleeving => {
            on_resleeving_exit(owed, prev, s);
            return;
        }
        _ => return,
    }
    let cause = death_cause(s, cause, was, kill);
    let resumes = !death_clears(s, cause, now);
    // A resleeve or park drops whatever was owed. So does a crash of an incarnation that
    // already reached idle. A resume of an interrupted turn may set a fresh debt below.
    if !resumes || reached_idle {
        clear(owed, sid);
    }
    if resumes && interrupted {
        // A prompt sent under a foreign turn is counted at the exit, still Delivering or
        // already acked (the ack turns `previous` to Working).
        let sent = |effect: &EffectId| {
            prev.effects.get(effect).is_some_and(|e| {
                e.state != EffectState::Intent
                    && matches!(e.kind, EffectKind::SendPrompt { kind: MsgKind::Prompt, .. })
            })
        };
        let seq = if s.foreign.is_some()
            && (matches!(was, Activity::Delivering { effect } if sent(effect))
                || s.delivery.as_ref().is_some_and(|d| !d.applied && sent(&d.effect)))
        {
            s.turn_seq + 1
        } else {
            s.turn_seq
        };
        let state = continue_state(s, seq);
        // Same test as `start_incarnation`: a resume id is kept only after a turn was
        // acked (`turned`) and the session is known. A fresh session is not continued.
        let native = s.turned && s.session.is_some();
        if s.target.mode == Mode::Headless && state.is_none() && seq > 0 && native {
            owed.insert(sid, seq);
        }
    }
}

fn on_resleeving_exit(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve) {
    match s.guard.state {
        GuardState::Briefing { effect } => {
            let sent = prev.effects.get(&effect).map(|e| e.state) != Some(EffectState::Intent);
            if sent || s.guard.tried.len() >= MAX_TARGETS {
                clear(owed, s.id);
            }
        }
        GuardState::Launching if s.guard.tried.len() >= MAX_TARGETS => clear(owed, s.id),
        _ => {}
    }
}

fn on_scope(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    g: Gen,
    live: bool,
    now: Millis,
) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.g != g || live || s.probing && !matches!(s.proc, Proc::Up) {
        return;
    }
    if s.probing && s.proc == Proc::Up {
        let kill = s.kill_cause.unwrap_or(Cause::HubRestart);
        on_exit(owed, prev, sid, g, ExitCause::Killed, Some(kill), now);
    }
}

fn on_recovered(owed: &mut BTreeMap<SleeveId, u64>, prev: &State) {
    let ids: Vec<SleeveId> = owed.keys().copied().collect();
    for sid in ids {
        let Some(s) = prev.sleeves.get(&sid) else {
            clear(owed, sid);
            continue;
        };
        if matches!(s.activity, Activity::Idle { .. }) && s.guard.state == GuardState::Armed {
            clear(owed, sid);
        }
    }
}

fn on_continue(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, sid: SleeveId, text: &str) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.foreign.is_some() || s.retry_unwritten {
        return;
    }
    let Some(&seq) = owed.get(&sid) else {
        return;
    };
    if seq != s.turn_seq || text.is_empty() {
        return;
    }
    if continuation(s, seq).is_some()
        || !s.queue.is_empty()
        || matches!(s.activity, Activity::Delivering { .. })
        || !deliverable(prev, s)
    {
        clear(owed, sid);
        return;
    }
    clear(owed, sid);
}

fn on_send(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    to: SleeveId,
    request_id: &str,
    from: Party,
    text: &str,
    mode: SendMode,
) {
    let Some(s) = prev.sleeves.get(&to) else {
        return;
    };
    if s.life == Life::Removed || request_id.is_empty() || text.is_empty() || !s.life.live() {
        return;
    }
    if s.messages.contains_key(request_id) {
        return;
    }
    if let Party::Sleeve(x) = from
        && prev.sleeve(x).is_none_or(|x| x.life == Life::Removed)
    {
        return;
    }
    if s.target.mode == Mode::Pane {
        return;
    }
    let steers = s.target.harness.steers() && s.life == Life::Running;
    let working = s.proc == Proc::Up
        && s.activity == Activity::Working
        && s.turn.is_some()
        && !frozen(prev, to);
    let delivering = matches!(s.activity, Activity::Delivering { .. }) && s.steer_slot.is_none();
    let queue_empty = s.queue.is_empty();
    let can_prompt = deliverable(prev, s) && queue_empty;
    match mode {
        SendMode::Steer if steers && (working || delivering) => return,
        SendMode::Steer if !can_prompt => return,
        _ => {}
    }
    let mut queued = s.clone();
    queued.queue.push_back(request_id.to_string());
    queued.messages.insert(
        request_id.to_string(),
        Message {
            from,
            agent: None,
            kind: MsgKind::Prompt,
            text: text.to_string(),
            mode,
            state: MsgState::Queued,
            seq: prev.next_msg,
        },
    );
    note_dequeue(owed, prev, &queued);
}

/// A turn report is a queued send from the reporting sleeve to Envoy or its parent, once per turn.
fn on_report(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    to: SleeveId,
    turn: u64,
    text: &str,
) {
    let Some(s) = prev.sleeve(sid) else {
        return;
    };
    let envoy = |id: SleeveId| {
        prev.sleeve(id)
            .is_some_and(|t| t.name == ENVOY_NAME && t.parent.is_none() && t.life != Life::Removed)
    };
    if s.life == Life::Removed || envoy(sid) || !(envoy(to) || s.parent == Some(to)) {
        return;
    }
    let request_id = format!("report:{}:{turn}", sid.0);
    on_send(
        owed,
        prev,
        to,
        &request_id,
        Party::Sleeve(sid),
        text,
        SendMode::Queue,
    );
}

fn on_stop(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, sid: SleeveId, remove: bool) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    match s.life {
        Life::Removed | Life::Stopped => {}
        Life::Stopping { remove: already } => {
            if remove && !already {
                for c in prev.children(sid) {
                    on_stop(owed, prev, c, true);
                }
            }
        }
        _ => {
            clear(owed, sid);
            for c in prev.children(sid) {
                on_stop(owed, prev, c, remove);
            }
        }
    }
}

fn on_start_cmd(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    target: Option<&Target>,
) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.life == Life::Removed || !matches!(s.life, Life::Parked(_) | Life::Stopped) {
        return;
    }
    if target.is_some_and(|t| !t.valid()) || s.proc != Proc::Down {
        return;
    }
    if let Some(p) = s.parent {
        if !prev.sleeves[&p].life.live() {
            return;
        }
        if s.life == Life::Stopped && prev.live_children(p) >= MAX_CHILDREN {
            return;
        }
    }
    for c in prev.children(sid) {
        if let Some(child) = prev.sleeves.get(&c) {
            note_dequeue(owed, prev, child);
        }
    }
}

fn on_resleeve(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    target: Option<&Target>,
) {
    let Some(s) = prev.sleeves.get(&sid).filter(|s| s.life != Life::Removed) else {
        return;
    };
    if target.is_some_and(|t| !t.valid()) || s.life != Life::Running || !armed(s) {
        return;
    }
    clear(owed, sid);
}

fn on_target(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    g: Gen,
    target: Option<&Target>,
) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.g != g || s.guard.state != GuardState::Selecting {
        return;
    }
    let auto = s.guard.cause != Some(Cause::Manual);
    let ok = target.is_some_and(|t| {
        t.valid() && !s.guard.tried.contains(t) && !(auto && t.mode == Mode::Pane)
    });
    if !ok {
        clear(owed, sid);
    }
}

fn on_obs(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    g: Gen,
    obs: &Obs,
    now: Millis,
) {
    let Some(s) = prev.sleeves.get(&sid) else {
        return;
    };
    if s.g != g || s.proc == Proc::Down {
        return;
    }
    let mut live = s.clone();
    if proc_up_after_evidence(prev, s) {
        live.proc = Proc::Up;
    }
    match obs {
        Obs::Ready { .. } => on_ready(owed, prev, &live),
        Obs::Progress { tool } => on_progress(owed, prev, &live, *tool, now),
        Obs::TurnSettled { end } => on_settled(owed, prev, &live, *end),
        Obs::CompactDone => on_compact_done(owed, prev, &live),
        Obs::Usage { tokens } => on_usage(owed, prev, &live, *tokens),
        Obs::Disk { bytes } => {
            if live.disk_limit.is_some_and(|l| *bytes >= l) && park_trip_clears(&live) {
                clear(owed, sid);
            }
        }
        Obs::ForbiddenPath { .. } if park_trip_clears(&live) => clear(owed, sid),
        _ => {}
    }
}

fn on_ready(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve) {
    if s.activity != Activity::Booting {
        return;
    }
    let mut now_s = s.clone();
    now_s.activity = Activity::Idle { seen: true };
    match (s.life, s.guard.state.clone()) {
        (Life::Launching, guard) => {
            now_s.life = Life::Running;
            if guard == GuardState::Resuming {
                now_s.guard.state = GuardState::Armed;
            }
            note_dequeue(owed, prev, &now_s);
            on_offer(owed, prev, &now_s);
        }
        (Life::Resleeving, GuardState::Launching) => {}
        _ => note_dequeue(owed, prev, &now_s),
    }
}

/// After Ready of a resume: a queue, an in-flight prompt, or a sleeve that cannot be prompted
/// drops the debt. A deliverable idle sleeve keeps it until `Continue`.
fn on_offer(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve) {
    if s.foreign.is_some() || s.retry_unwritten {
        return;
    }
    let Some(&seq) = owed.get(&s.id) else {
        return;
    };
    if seq != s.turn_seq {
        return;
    }
    if !s.queue.is_empty() || matches!(s.activity, Activity::Delivering { .. }) {
        clear(owed, s.id);
        return;
    }
    if deliverable(prev, s) {
        return;
    }
    if frozen(prev, s.id) || s.target.mode != Mode::Headless || !s.life.live() {
        clear(owed, s.id);
    }
}

fn on_settled(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve, end: TurnEnd) {
    if !matches!(
        s.activity,
        Activity::Working | Activity::Blocked { .. } | Activity::Interrupting
    ) {
        if owed.get(&s.id) == Some(&s.turn_seq) {
            clear(owed, s.id);
        }
        return;
    }
    if owed.get(&s.id) == Some(&s.turn_seq) {
        clear(owed, s.id);
    }
    let mut done = s.clone();
    done.activity = Activity::Idle { seen: false };
    done.turn = None;
    if done.guard.state == GuardState::Interrupting {
        done.guard.state = GuardState::Armed;
    }
    if matches!(end, TurnEnd::UsageLimit | TurnEnd::AuthFailed) && resleeve_trip_clears(&done) {
        clear(owed, s.id);
    }
    note_dequeue(owed, prev, &done);
}

fn on_progress(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    s: &Sleeve,
    tool: Option<u64>,
    now: Millis,
) {
    let mut live = s.clone();
    if let Activity::Delivering { effect } = s.activity
        && prev
            .effects
            .get(&effect)
            .is_some_and(|e| e.state != EffectState::Intent && !e.state.terminal())
    {
        live.activity = Activity::Working;
    }
    if !matches!(live.activity, Activity::Working | Activity::Blocked { .. }) {
        return;
    }
    let Some(h) = tool else {
        return;
    };
    let count = if live.loop_hash == Some(h) {
        live.loop_count + 1
    } else {
        1
    };
    if count >= LOOP_REPEATS {
        live.loop_count = count;
        if interrupt_resleeves(&live, now) {
            clear(owed, s.id);
        }
    }
}

fn on_compact_done(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve) {
    if s.activity != Activity::Compacting {
        return;
    }
    let blocked = prev.effects.values().any(|e| {
        matches!(e.kind, EffectKind::Compact { sleeve, g } if sleeve == s.id && g == s.g)
            && e.state == EffectState::Intent
    });
    if blocked {
        return;
    }
    let mut done = s.clone();
    done.activity = Activity::Idle { seen: true };
    note_dequeue(owed, prev, &done);
}

fn on_usage(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, s: &Sleeve, tokens: u64) {
    let over = s.budget.is_some_and(|b| s.used.saturating_add(tokens) >= b);
    if let Some(p) = s.parent {
        let root = &prev.sleeves[&p];
        if root
            .budget
            .is_some_and(|b| root.used.saturating_add(tokens) >= b)
            && park_trip_clears(root)
        {
            clear(owed, p);
        }
    }
    if over && park_trip_clears(s) {
        clear(owed, s.id);
    }
}

fn on_tick(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, now: Millis) {
    for s in prev.sleeves.values() {
        if s.proc == Proc::Down {
            continue;
        }
        if s.kill_at.is_some() || s.stop_at.is_some() {
            continue;
        }
        if matches!(s.guard.state, GuardState::Quiescing { .. }) {
            continue;
        }
        if s.activity == Activity::Booting && now.saturating_sub(s.boot_at) >= START_TIMEOUT {
            match s.guard.state {
                GuardState::Armed if s.life == Life::Launching && resleeve_trip_clears(s) => {
                    clear(owed, s.id);
                }
                _ => {}
            }
            continue;
        }
        let busy = matches!(s.activity, Activity::Working | Activity::Blocked { .. });
        let interrupt = (busy && now.saturating_sub(s.turn_at) >= TURN_WALLCLOCK)
            || (s.activity == Activity::Working && now.saturating_sub(s.progress_at) >= SILENT);
        if interrupt && interrupt_resleeves(s, now) {
            clear(owed, s.id);
        }
    }
}

enum Report {
    Acked,
    Nacked,
    Lost,
    Probed(ProbeResult),
}

fn report_seen(prev: &State, id: EffectId, attempt: u32, report: Report) -> Option<EffectState> {
    let e = prev.effects.get(&id)?;
    if e.attempt != attempt {
        return None;
    }
    use EffectState as S;
    match (e.state, report) {
        (S::Intent | S::Sent | S::Uncertain | S::Probing, Report::Acked) => Some(S::Confirmed),
        (S::Sent, Report::Nacked) => Some(S::Rejected),
        (S::Sent, Report::Lost) => Some(S::Probing),
        (S::Probing, Report::Probed(ProbeResult::Applied)) => Some(S::Confirmed),
        (S::Probing, Report::Probed(ProbeResult::NotApplied)) => Some(not_applied_seen(prev, e)),
        (S::Probing, Report::Probed(ProbeResult::Unknowable)) => Some(S::Uncertain),
        _ => None,
    }
}

fn not_applied_seen(prev: &State, e: &Effect) -> EffectState {
    if e.class == Class::Unsafe {
        return EffectState::Uncertain;
    }
    if e.attempt < MAX_ATTEMPTS && wanted(prev, e) {
        EffectState::Intent
    } else {
        EffectState::Failed
    }
}

fn wanted(prev: &State, e: &Effect) -> bool {
    match &e.kind {
        EffectKind::Provision { sleeve, .. } => prev.sleeves[sleeve].provision == Some(e.id),
        EffectKind::StartIncarnation { sleeve, g, .. } => {
            let s = &prev.sleeves[sleeve];
            s.g == *g
                && s.proc == Proc::Starting(e.id)
                && matches!(s.life, Life::Launching | Life::Resleeving)
        }
        EffectKind::StopIncarnation { sleeve, g }
        | EffectKind::KillScope { sleeve, g }
        | EffectKind::Interrupt { sleeve, g } => {
            let s = &prev.sleeves[sleeve];
            s.g == *g && s.proc != Proc::Down
        }
        EffectKind::MailDeliver {
            sleeve, request_id, ..
        } => prev.sleeves[sleeve]
            .messages
            .get(request_id)
            .is_some_and(|m| m.state == MsgState::InFlight(e.id)),
        EffectKind::ContinuationBrief { sleeve, .. } => {
            prev.sleeves[sleeve].guard.state == (GuardState::Capturing { brief: e.id })
        }
        EffectKind::NodeSubmit { job, .. } => {
            let j = &prev.jobs[job];
            j.submit == Some(e.id)
                && matches!(
                    j.state,
                    JobState::Submitting | JobState::SubmitUncertain { .. }
                )
        }
        EffectKind::NodeCancel { job, .. } => prev.jobs[job].state == JobState::Cancelling,
        EffectKind::NodeAckCollected { .. } => true,
        EffectKind::SendPrompt { .. }
        | EffectKind::Steer { .. }
        | EffectKind::RespondApproval { .. }
        | EffectKind::Compact { .. } => false,
    }
}

fn on_abandon(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, id: EffectId) {
    let Some(e) = prev.effects.get(&id) else {
        return;
    };
    if matches!(e.state, EffectState::Uncertain | EffectState::Probing) {
        on_effect(owed, prev, id, EffectState::Abandoned);
    }
}

fn on_effect(owed: &mut BTreeMap<SleeveId, u64>, prev: &State, id: EffectId, seen: EffectState) {
    let Some(e) = prev.effects.get(&id) else {
        return;
    };
    match &e.kind {
        EffectKind::StartIncarnation { sleeve, .. } => {
            if matches!(seen, EffectState::Rejected | EffectState::Failed)
                && prev.sleeves[sleeve].proc == Proc::Starting(id)
            {
                start_failed(owed, &prev.sleeves[sleeve]);
            }
        }
        EffectKind::Provision { sleeve, .. } => {
            let s = &prev.sleeves[sleeve];
            if seen.terminal()
                && s.provision == Some(id)
                && s.life == Life::Provisioning
                && seen != EffectState::Confirmed
            {
                clear(owed, *sleeve);
            }
        }
        EffectKind::SendPrompt { sleeve, .. } => on_prompt(owed, prev, *sleeve, id, seen),
        EffectKind::Compact { sleeve, .. } => {
            let s = &prev.sleeves[sleeve];
            if matches!(
                seen,
                EffectState::Rejected | EffectState::Failed | EffectState::Abandoned
            ) && s.activity == Activity::Compacting
            {
                let mut done = s.clone();
                done.activity = Activity::Idle { seen: true };
                note_dequeue(owed, prev, &done);
            }
        }
        _ => {}
    }
}

fn start_failed(owed: &mut BTreeMap<SleeveId, u64>, s: &Sleeve) {
    match (s.life, s.guard.state) {
        (Life::Launching, _) => clear(owed, s.id),
        (Life::Resleeving, GuardState::Launching) if s.guard.tried.len() >= MAX_TARGETS => {
            clear(owed, s.id);
        }
        _ => {}
    }
}

fn on_prompt(
    owed: &mut BTreeMap<SleeveId, u64>,
    prev: &State,
    sid: SleeveId,
    id: EffectId,
    seen: EffectState,
) {
    let s = &prev.sleeves[&sid];
    if s.guard.state == (GuardState::Briefing { effect: id }) {
        if matches!(
            seen,
            EffectState::Rejected
                | EffectState::Failed
                | EffectState::Abandoned
                | EffectState::Uncertain
        ) {
            clear(owed, sid);
        }
        return;
    }
    if s.activity == (Activity::Delivering { effect: id })
        && seen.terminal()
        && seen != EffectState::Confirmed
    {
        let mut done = s.clone();
        done.activity = Activity::Idle { seen: true };
        if let Some(r) = done.steer_slot.take() {
            done.queue.push_front(r);
        }
        note_dequeue(owed, prev, &done);
    }
}
