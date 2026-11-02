//! Real Claude Code runs, on the cheapest model by flag.

mod common;

use std::time::Duration;

use common::{Run, program, skip, spec, temp_repo};
use pt_drivers::{Decision, Entry, ErrorClass, Harness, Permission, SpawnSpec, Stop, ToolStatus};

fn haiku(spec: &mut SpawnSpec) {
    spec.model = Some("haiku".into());
}

#[test]
fn ready_resume_tool() {
    common::ready_resume_tool(Harness::Claude, haiku);
}

#[test]
fn interrupt() {
    common::interrupt(Harness::Claude, haiku);
}

/// Supervised: the Bash call arrives as approval_request and runs after allow_once.
#[test]
fn supervised_approval() {
    let Some(program) = program(Harness::Claude) else {
        return skip(Harness::Claude, "binary not found");
    };
    let repo = temp_repo("claude-approval");
    let mut s = spec(Harness::Claude, &program, &repo);
    haiku(&mut s);
    s.permission = Permission::Supervised;
    let mut run = Run::start(s);
    run.ready(Duration::from_secs(30)).expect("ready");
    run.send("Use the Bash tool to run exactly this command: touch approved.txt\nThen reply DONE.");
    let turn = run.collect(Duration::from_secs(180), |session, entry| {
        if let Entry::ApprovalRequest { id, .. } = entry {
            session
                .respond_approval(id, Decision::AllowOnce)
                .expect("respond");
        }
    });
    if turn.auth_failed() {
        run.stop();
        return skip(Harness::Claude, "no usable login");
    }
    eprintln!("claude approval turn: {turn:?}");
    assert!(!turn.approvals.is_empty(), "an approval was requested");
    assert_eq!(turn.settled.map(|s| s.0), Some(Stop::EndTurn));
    assert!(
        turn.tool_results
            .iter()
            .any(|(_, s)| *s == ToolStatus::Completed)
    );
    assert!(
        repo.join("approved.txt").exists(),
        "the approved command ran"
    );
    let log = run.stop();
    assert!(common::has(&log, |e| matches!(
        e,
        Entry::ApprovalDecision {
            decision: Decision::AllowOnce,
            by: None,
            ..
        }
    )));
}

/// A background command that finishes mid-turn comes back as a replayed user message Claude
/// queued itself (origin task-notification): a notice, not a protocol error.
#[test]
fn background_task_notification() {
    let Some(program) = program(Harness::Claude) else {
        return skip(Harness::Claude, "binary not found");
    };
    let repo = temp_repo("claude-bg");
    let mut s = spec(Harness::Claude, &program, &repo);
    haiku(&mut s);
    let mut run = Run::start(s);
    run.ready(Duration::from_secs(30)).expect("ready");
    run.send(
        "First run the shell command `sleep 2; echo bg` with run_in_background true. Then, in \
         the foreground, run `sleep 8; echo fg`. Then reply DONE.",
    );
    let turn = run.collect(Duration::from_secs(180), |_, _| {});
    if turn.auth_failed() {
        run.stop();
        return skip(Harness::Claude, "no usable login");
    }
    assert!(
        !turn.errors.iter().any(|(c, _)| *c == ErrorClass::Protocol),
        "{:?}",
        turn.errors
    );
    let log = run.stop();
    assert!(common::has(&log, |e| matches!(
        e,
        Entry::Notice { code: Some(c), .. } if c == "harness_input"
    )));
}
