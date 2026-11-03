//! Real Claude Code runs, on the cheapest model by flag.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use common::{Run, program, skip, spec, temp_repo};
use pt_drivers::{
    Ack, Decision, DriverError, Entry, ErrorClass, Harness, Permission, PromptKind, SpawnSpec,
    Stop, ToolStatus,
};

#[test]
fn foreign_result_does_not_retire_a_written_prompt() {
    if common::on_path("python3").is_none() {
        return;
    }
    let repo = temp_repo("claude-foreign");
    let fake = repo.join("claude");
    std::fs::write(
        &fake,
        r#"#!/usr/bin/env python3
import json, os, sys, time
def emit(msg): print(json.dumps(msg), flush=True)
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("type") != "user": continue
    emit({"type":"assistant", "message":{"id":"foreign", "content":[{"type":"text", "text":"background answer"}]}})
    while not os.path.exists("release-result"): time.sleep(0.01)
    emit({"type":"result", "subtype":"success", "is_error":False, "stop_reason":"end_turn"})
    while not os.path.exists("release-echo"): time.sleep(0.01)
    msg["isReplay"] = True
    emit(msg)
    emit({"type":"assistant", "message":{"id":"ours", "content":[{"type":"text", "text":"own answer"}]}})
    emit({"type":"result", "subtype":"success", "is_error":False, "stop_reason":"end_turn"})
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut run = Run::start(spec(Harness::Claude, &fake, &repo));
    run.ready(Duration::from_secs(5)).expect("ready");
    let request = run.send("A");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match run.next(deadline) {
            Some(Entry::TurnStarted {
                request_id: None, ..
            }) => break,
            Some(_) => {}
            None => panic!("the foreign turn did not start"),
        }
    }
    assert!(matches!(
        run.session
            .send_user("next", "owner", PromptKind::Prompt, "B"),
        Err(DriverError::Busy)
    ));
    assert!(matches!(
        run.session.steer("steer", "owner", "change", "turn"),
        Err(DriverError::Unsupported(_))
    ));
    std::fs::write(repo.join("release-result"), "release").unwrap();
    let foreign = run.collect(Duration::from_secs(5), |_, _| {});
    assert_eq!(foreign.text, "background answer");
    assert_eq!(foreign.settled.map(|s| s.0), Some(Stop::EndTurn));
    assert_eq!(run.session.wait_ack(&request, Duration::ZERO), Ack::Pending);
    assert!(common::has(&run.log, |e| matches!(
        e,
        Entry::TurnStarted {
            request_id: None,
            ..
        }
    )));
    std::fs::write(repo.join("release-echo"), "release").unwrap();
    let ours = run.collect(Duration::from_secs(5), |_, _| {});
    assert_eq!(ours.text, "own answer");
    assert_eq!(ours.settled.map(|s| s.0), Some(Stop::EndTurn));
    assert!(matches!(
        run.session.wait_ack(&request, Duration::from_secs(1)),
        Ack::Confirmed { .. }
    ));
    let log = run.stop();
    assert!(common::has(
        &log,
        |e| matches!(e, Entry::TurnStarted { request_id: Some(r), .. } if *r == request)
    ));
    assert!(!common::has(
        &log,
        |e| matches!(e, Entry::Notice { code: Some(c), .. } if c == "unechoed" || c == "stale_echo")
    ));
}

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
