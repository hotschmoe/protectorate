#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_core::*;
use pt_core_fuzz::sim::{Policy, Sim};
use pt_core_fuzz::{Bytes, script};

const PATHS: [&str; 6] = ["/w/a", "/w/b", "/w/c", "/w/a/x", "/wt/1", "/wt/2"];

fn target(b: &mut Bytes) -> Target {
    let (harness, mode) = [
        (Harness::Claude, Mode::Headless),
        (Harness::Codex, Mode::Headless),
        (Harness::PiLocal, Mode::Headless),
        (Harness::PiDurable, Mode::Headless),
        (Harness::Antigravity, Mode::Pane),
        (Harness::Claude, Mode::Pane),
    ][b.below(6) as usize];
    Target {
        harness,
        mode,
        provider: "p".into(),
        model: "m".into(),
    }
}

fn sleeve(sim: &Sim, b: &mut Bytes) -> SleeveId {
    SleeveId(b.below(sim.st.next_sleeve + 1))
}

fn command(sim: &mut Sim, b: &mut Bytes) {
    use EventKind as E;
    let s = sleeve(sim, b);
    let kind = match b.below(18) {
        0 | 1 => {
            let parent = if b.below(3) == 0 {
                None
            } else {
                Some(sleeve(sim, b))
            };
            let layout = match (parent, b.bool()) {
                (None, _) => Layout::Root,
                (Some(_), false) => Layout::Worktree,
                (Some(_), true) => Layout::View,
            };
            let n = sim.st.next_sleeve;
            E::Spawn(SpawnSpec {
                request_id: format!("spawn-{}", b.below(n + 2)),
                name: format!("s{}", b.below(n + 2)),
                parent,
                layout,
                claim: Claim {
                    path: PATHS[b.below(6) as usize].into(),
                    writable: layout != Layout::View,
                },
                target: target(b),
                resleeve_to: (0..b.below(3)).map(|_| target(b)).collect(),
                budget: b.bool().then(|| 1000 + b.below(4000)),
                disk_limit: b.bool().then(|| 1 << 30),
                effort: b.bool().then(|| "high".into()),
            })
        }
        2 | 3 => {
            let from = if b.bool() {
                Party::Owner
            } else {
                Party::Sleeve(sleeve(sim, b))
            };
            E::Send {
                to: s,
                request_id: format!("m{}", b.below(12)),
                from,
                text: "do the thing".into(),
                mode: if b.below(3) == 0 {
                    SendMode::Steer
                } else {
                    SendMode::Queue
                },
            }
        }
        4 => E::Interrupt { sleeve: s },
        5 => E::Compact { sleeve: s },
        6 => {
            // An approval the core knows (most of the time), else a made-up one.
            let known: Vec<(String, SleeveId)> = sim
                .st
                .approvals
                .values()
                .map(|a| (a.id.clone(), a.sleeve))
                .collect();
            let (approval, sleeve) = match known.len() {
                n if n > 0 && b.below(4) != 0 => known[b.below(n as u64) as usize].clone(),
                _ => (
                    tool_approval_id(s, Gen(b.below(4)), &format!("a{}", b.below(2))),
                    s,
                ),
            };
            E::RespondApproval {
                sleeve,
                approval,
                allow: b.bool(),
                note: String::new(),
            }
        }
        7 => E::Stop { sleeve: s },
        8 => E::Remove { sleeve: s },
        9 => E::Start {
            sleeve: s,
            target: b.bool().then(|| target(b)),
            effort: b.bool().then(|| "high".into()),
        },
        10 => E::Resleeve {
            sleeve: s,
            target: b.bool().then(|| target(b)),
            effort: b.bool().then(|| "low".into()),
        },
        11 => E::Abandon {
            effect: EffectId(b.below(sim.st.next_effect + 1)),
        },
        12 => E::CreateJob {
            request_id: format!("j{}", b.below(4)),
            spec: JobSpec {
                digest: "d".into(),
                arch: "arm64".into(),
                command: "make test".into(),
                even_if_present: false,
            },
        },
        13 => E::CancelJob {
            job: JobId(b.below(sim.st.next_job + 1)),
        },
        14 => E::RequeueJob {
            job: JobId(b.below(sim.st.next_job + 1)),
        },
        15 => E::RequestApproval {
            sleeve: s,
            ask: Ask {
                title: "remove sleeve x".into(),
                ..Ask::default()
            },
            timeout: [0, SECOND, 15 * MINUTE, 2 * HOUR, Millis::MAX][b.below(5) as usize],
        },
        16 => E::JobNodeRemoved {
            job: JobId(b.below(sim.st.next_job + 1)),
            node: NodeId(1 + b.below(3)),
        },
        _ => E::AbandonJob {
            job: JobId(b.below(sim.st.next_job + 1)),
        },
    };
    sim.apply(kind);
}

fn obs(b: &mut Bytes) -> Obs {
    match b.below(14) {
        0 | 1 => Obs::Ready {
            session: "sess".into(),
        },
        2 | 3 => Obs::Progress {
            tool: b.bool().then(|| b.below(2)),
        },
        4 => Obs::ApprovalRequested {
            id: format!("a{}", b.below(2)),
            ask: b.bool().then(|| Ask {
                title: "Bash".into(),
                summary: "rm -rf build".into(),
                detail: String::new(),
                options: vec![Choice::AllowOnce, Choice::Deny],
            }),
        },
        5 => Obs::ApprovalResolved {
            id: format!("a{}", b.below(2)),
        },
        6 | 7 => Obs::TurnSettled {
            end: [
                TurnEnd::Done,
                TurnEnd::Done,
                TurnEnd::Interrupted,
                TurnEnd::UsageLimit,
                TurnEnd::AuthFailed,
                TurnEnd::Error,
            ][b.below(6) as usize],
        },
        8 => Obs::CompactDone,
        9 => Obs::Desync {
            detail: "frame".into(),
        },
        10 => Obs::Screen(
            [
                Verdict::Idle,
                Verdict::Working,
                Verdict::Blocked,
                Verdict::NoMatch,
            ][b.below(4) as usize],
        ),
        11 => Obs::Usage {
            tokens: b.below(3000),
        },
        12 => Obs::Disk {
            bytes: b.below(2) << 30,
        },
        _ => Obs::ForbiddenPath {
            path: ".github/workflows/x".into(),
        },
    }
}

fuzz_target!(|data: &[u8]| {
    if script::run(data) {
        return;
    }
    let mut b = Bytes::new(data);
    let mut sim = Sim::new();
    while !b.done() && sim.steps < 3000 {
        match b.u8() % 16 {
            0..=2 => command(&mut sim, &mut b),
            3..=6 => {
                if !sim.pending.is_empty() {
                    let i = b.below(sim.pending.len() as u64) as usize;
                    let o = b.u8();
                    let p = Policy {
                        start: o,
                        prompt: o,
                        stop: o >> 2,
                        other: o,
                        probe: o >> 3,
                        select: o >> 1,
                        poll: o >> 4,
                        fetch: o >> 5,
                    };
                    sim.exec_at(i, &p);
                }
            }
            7..=9 => {
                let live: Vec<(SleeveId, Gen)> = sim.alive.iter().copied().collect();
                if !live.is_empty() {
                    let (s, g) = live[b.below(live.len() as u64) as usize];
                    let o = obs(&mut b);
                    sim.observe(s, g, o);
                }
            }
            10 => {
                let s = sleeve(&sim, &mut b);
                let g = Gen(b.below(4));
                let o = obs(&mut b);
                sim.observe(s, g, o);
            }
            11 => {
                let live: Vec<(SleeveId, Gen)> = sim.alive.iter().copied().collect();
                if !live.is_empty() {
                    let (s, g) = live[b.below(live.len() as u64) as usize];
                    let cause = [ExitCause::Exit(1), ExitCause::Oom, ExitCause::Killed]
                        [b.below(3) as usize];
                    sim.crash_process(s, g, cause);
                }
            }
            12 => {
                let by = [
                    SECOND,
                    10 * SECOND,
                    31 * SECOND,
                    61 * SECOND,
                    6 * MINUTE,
                    11 * MINUTE,
                    16 * MINUTE,
                    31 * MINUTE,
                    91 * MINUTE,
                    2 * HOUR,
                    25 * HOUR,
                    Millis::MAX,
                ][b.below(12) as usize];
                sim.tick(by);
            }
            13 => sim.commit(),
            14 => {
                let empty = b.bool();
                sim.crash(empty);
            }
            _ => sim.run(&Policy::default()),
        }
    }
    sim.finish();
});
