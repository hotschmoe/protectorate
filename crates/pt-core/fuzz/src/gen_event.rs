//! An arbitrary `Event` from fuzz bytes, covering every variant and field.

use pt_core::*;

use crate::Bytes;

fn target(b: &mut Bytes) -> Target {
    const H: [Harness; 6] = [
        Harness::Claude,
        Harness::Codex,
        Harness::Grok,
        Harness::Antigravity,
        Harness::PiVendor,
        Harness::PiLocal,
    ];
    Target {
        harness: H[b.below(6) as usize],
        mode: if b.bool() { Mode::Pane } else { Mode::Headless },
        provider: b.string(),
        model: b.string(),
    }
}

fn opt<T>(b: &mut Bytes, f: impl FnOnce(&mut Bytes) -> T) -> Option<T> {
    if b.bool() { Some(f(b)) } else { None }
}

fn party(b: &mut Bytes) -> Party {
    match b.below(4) {
        0 => Party::Owner,
        1 => Party::Envoy,
        2 => Party::Agent,
        _ => Party::Sleeve(SleeveId(b.u64())),
    }
}

fn ask(b: &mut Bytes) -> Ask {
    const C: [Choice; 3] = [Choice::AllowOnce, Choice::AllowSession, Choice::Deny];
    Ask {
        title: b.string(),
        summary: b.string(),
        detail: b.string(),
        options: (0..b.below(4)).map(|_| C[b.below(3) as usize]).collect(),
    }
}

fn obs(b: &mut Bytes) -> Obs {
    const ENDS: [TurnEnd; 5] = [
        TurnEnd::Done,
        TurnEnd::Interrupted,
        TurnEnd::UsageLimit,
        TurnEnd::AuthFailed,
        TurnEnd::Error,
    ];
    const V: [Verdict; 4] = [
        Verdict::Idle,
        Verdict::Working,
        Verdict::Blocked,
        Verdict::NoMatch,
    ];
    match b.below(13) {
        0 => Obs::Ready {
            session: b.string(),
        },
        1 => Obs::Progress {
            tool: opt(b, |b| b.u64()),
        },
        2 => Obs::ApprovalRequested {
            id: b.string(),
            ask: opt(b, ask),
        },
        3 => Obs::ApprovalResolved { id: b.string() },
        4 => Obs::TurnSettled {
            end: ENDS[b.below(5) as usize],
        },
        5 => Obs::CompactDone,
        6 => Obs::Desync { detail: b.string() },
        7 => Obs::Screen(V[b.below(4) as usize]),
        8 => Obs::Usage { tokens: b.u64() },
        9 => Obs::Disk { bytes: b.u64() },
        10 => Obs::ForbiddenPath { path: b.string() },
        11 => Obs::TurnBegan,
        _ => Obs::ForeignTurnSettled {
            end: ENDS[b.below(5) as usize],
        },
    }
}

fn report(b: &mut Bytes) -> JobReport {
    match b.below(6) {
        0 => JobReport::Running { offset: b.u64() },
        1 => JobReport::Exited {
            code: b.u64() as i32,
        },
        2 => JobReport::Interrupted,
        3 => JobReport::Cancelled,
        4 => JobReport::Refused { invalid: b.bool() },
        _ => JobReport::Unknown { epoch: b.u64() },
    }
}

pub fn event(b: &mut Bytes) -> Event {
    let at = b.u64();
    let s = |b: &mut Bytes| SleeveId(b.u64());
    let g = |b: &mut Bytes| Gen(b.u64());
    let e = |b: &mut Bytes| EffectId(b.u64());
    let j = |b: &mut Bytes| JobId(b.u64());
    let a = |b: &mut Bytes| b.u64() as u32;
    use EventKind as E;
    let kind = match b.below(36) {
        0 => E::Spawn(SpawnSpec {
            request_id: b.string(),
            name: b.string(),
            parent: opt(b, s),
            layout: [Layout::Root, Layout::Worktree, Layout::View][b.below(3) as usize],
            claim: Claim {
                path: b.string(),
                writable: b.bool(),
            },
            target: target(b),
            resleeve_to: (0..b.below(3)).map(|_| target(b)).collect(),
            budget: opt(b, |b| b.u64()),
            disk_limit: opt(b, |b| b.u64()),
            effort: opt(b, |b| b.string()),
            d9: b.bool(),
            by_envoy: b.bool(),
        }),
        1 => E::Send {
            to: s(b),
            request_id: b.string(),
            from: party(b),
            agent: opt(b, |b| b.string()),
            text: b.string(),
            mode: if b.bool() {
                SendMode::Steer
            } else {
                SendMode::Queue
            },
            d9: b.bool(),
        },
        2 => E::Interrupt { sleeve: s(b) },
        3 => E::Compact { sleeve: s(b) },
        4 => E::RespondApproval {
            sleeve: s(b),
            approval: b.string(),
            allow: b.bool(),
            note: b.string(),
        },
        5 => E::Stop { sleeve: s(b) },
        6 => E::Remove { sleeve: s(b) },
        7 => E::Start {
            sleeve: s(b),
            target: opt(b, target),
            effort: opt(b, |b| b.string()),
        },
        8 => E::Resleeve {
            sleeve: s(b),
            target: opt(b, target),
            effort: opt(b, |b| b.string()),
        },
        9 => E::Abandon { effect: e(b) },
        10 => E::Viewed { sleeve: s(b) },
        11 => E::CreateJob {
            request_id: b.string(),
            spec: JobSpec {
                digest: b.string(),
                arch: b.string(),
                command: b.string(),
                even_if_present: b.bool(),
            },
        },
        12 => E::CancelJob { job: j(b) },
        13 => E::AbandonJob { job: j(b) },
        14 => E::RequeueJob { job: j(b) },
        15 => E::Dispatched {
            effect: e(b),
            attempt: a(b),
        },
        16 => E::Acked {
            effect: e(b),
            attempt: a(b),
            native: opt(b, |b| b.string()),
        },
        17 => E::Nacked {
            effect: e(b),
            attempt: a(b),
            reason: b.string(),
        },
        18 => E::Lost {
            effect: e(b),
            attempt: a(b),
        },
        19 => E::Probed {
            effect: e(b),
            attempt: a(b),
            result: [
                ProbeResult::Applied,
                ProbeResult::NotApplied,
                ProbeResult::Unknowable,
            ][b.below(3) as usize],
        },
        20 => E::Launched {
            sleeve: s(b),
            g: g(b),
            scope: b.string(),
            pid: a(b),
        },
        21 => E::ScopeProbed {
            sleeve: s(b),
            g: g(b),
            live: b.bool(),
        },
        22 => E::TargetChosen {
            sleeve: s(b),
            g: g(b),
            chosen: match b.below(3) {
                0 => Chosen::Target(target(b)),
                1 => Chosen::NoTarget,
                _ => Chosen::Ineligible(target(b).harness),
            },
        },
        23 => E::NodeChosen {
            job: j(b),
            attempt: a(b),
            node: NodeId(b.u64()),
            epoch: b.u64(),
        },
        24 => E::JobStatus {
            job: j(b),
            attempt: a(b),
            node: NodeId(b.u64()),
            report: report(b),
        },
        25 => E::ArtifactsChecked {
            job: j(b),
            attempt: a(b),
            result: match b.below(3) {
                0 => ArtifactCheck::Verified,
                1 => ArtifactCheck::Retryable,
                _ => ArtifactCheck::Permanent,
            },
        },
        26 => E::Observed {
            sleeve: s(b),
            g: g(b),
            obs: obs(b),
        },
        27 => E::Exited {
            sleeve: s(b),
            g: g(b),
            cause: match b.below(3) {
                0 => ExitCause::Exit(b.u64() as i32),
                1 => ExitCause::Killed,
                _ => ExitCause::Oom,
            },
        },
        28 => E::Tick,
        29 => E::HubRecovered,
        30 => E::JobNodeRemoved {
            job: j(b),
            node: NodeId(b.u64()),
        },
        31 => E::RequestApproval {
            sleeve: s(b),
            ask: ask(b),
            timeout: b.u64(),
        },
        32 => E::Continue {
            sleeve: s(b),
            text: b.string(),
        },
        33 => E::ArtifactCollection {
            job: j(b),
            attempt: a(b),
            refusals: (0..b.below(3))
                .map(|_| ArtifactRefusal {
                    path: b.string(),
                    reason: b.string(),
                })
                .collect(),
            incomplete: opt(b, |b| b.string()),
        },
        34 => E::Report {
            sleeve: s(b),
            to: s(b),
            turn: b.u64(),
            text: b.string(),
        },
        _ => E::Unwritten {
            effect: e(b),
            attempt: a(b),
        },
    };
    Event { at, kind }
}
