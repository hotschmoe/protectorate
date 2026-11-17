use pt_core::*;
use std::time::Instant;
fn main() {
    let mut journal = Vec::new();
    let mut push = |kind| journal.push(codec::encode(&Event { at: 0, kind }));
    for i in 0..7000 {
        let job = JobId(i);
        let node = NodeId(1);
        push(EventKind::CreateJob {
            request_id: format!("j{i}"),
            spec: JobSpec {
                digest: "digest".into(),
                arch: "x86_64".into(),
                command: "true".into(),
                even_if_present: false,
            },
        });
        push(EventKind::NodeChosen {
            job,
            attempt: 1,
            node,
            epoch: 1,
        });
        push(EventKind::Dispatched {
            effect: EffectId(i),
            attempt: 1,
        });
        push(EventKind::JobStatus {
            job,
            attempt: 1,
            node,
            report: JobReport::Refused { invalid: true },
        });
        for _ in 0..38 {
            push(EventKind::Tick);
        }
    }
    for _ in 0..6000 {
        push(EventKind::Tick);
    }
    let begin = Instant::now();
    let mut state = State::default();
    for bytes in &journal {
        let event = codec::decode(bytes).unwrap();
        apply(&mut state, &event);
    }
    let elapsed = begin.elapsed();
    assert_eq!(state.effects.len(), 7000);
    assert!(state.effects.values().all(|e| e.state.terminal()));
    println!(
        "events={} jobs={} effects={} terminal={} elapsed={:.3}s",
        journal.len(),
        state.jobs.len(),
        state.effects.len(),
        state
            .effects
            .values()
            .filter(|e| e.state.terminal())
            .count(),
        elapsed.as_secs_f64()
    );
}
