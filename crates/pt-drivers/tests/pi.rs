//! Real Pi runs against a local OpenAI-compatible endpoint, selected only with Pi's
//! `--provider` and `--model` flags: `PT_E2E_PI_PROVIDER` (a provider in Pi's own models.json)
//! and `PT_E2E_PI_MODEL`. Both tests skip while either is unset; the Makefile reads them from a
//! gitignored `.env.e2e` when it exists.

mod common;

use std::time::Duration;

use common::{LONG, READY, Run, program, skip, spec, temp_repo};
use pt_drivers::{Ack, DriverError, Entry, Harness, SpawnSpec, Stop};

fn configured() -> bool {
    let set = |v| std::env::var(v).is_ok_and(|s| !s.is_empty());
    if set("PT_E2E_PI_PROVIDER") && set("PT_E2E_PI_MODEL") {
        return true;
    }
    eprintln!("SKIP pi: set PT_E2E_PI_PROVIDER and PT_E2E_PI_MODEL, or put them in .env.e2e");
    false
}

fn local(spec: &mut SpawnSpec) {
    spec.provider = std::env::var("PT_E2E_PI_PROVIDER").ok();
    spec.model = std::env::var("PT_E2E_PI_MODEL").ok();
}

#[test]
fn ready_resume_tool() {
    if !configured() {
        return;
    }
    common::ready_resume_tool(Harness::Pi, local);
}

#[test]
fn interrupt() {
    if !configured() {
        return;
    }
    common::interrupt(Harness::Pi, local);
}

/// A prompt an extension command consumes (`disposition: handled`) still starts and ends a
/// turn, so the session takes the next prompt; a steer for an ended turn is refused.
#[test]
fn handled_prompt() {
    let Some(program) = program(Harness::Pi) else {
        return skip(Harness::Pi, "binary not found");
    };
    let repo = temp_repo("pi-handled");
    let ext = repo.join("pt-noop.ts");
    std::fs::write(
        &ext,
        "export default function (pi) {\n  pi.registerCommand(\"pt-noop\", { description: \"noop\", handler: async () => {} });\n}\n",
    )
    .expect("write extension");
    let mut s = spec(Harness::Pi, &program, &repo);
    local(&mut s);
    s.extra_args = vec!["--extension".into(), ext.display().to_string()];
    let mut run = Run::start(s);
    run.ready(Duration::from_secs(90)).expect("ready");
    let req = run.send("/pt-noop");
    let turn = run.collect(Duration::from_secs(60), |_, _| {});
    assert_eq!(
        turn.settled,
        Some((Stop::Other, Some("handled".into()))),
        "the handled prompt settles"
    );
    let ack = run.session.wait_ack(&req, Duration::from_secs(1));
    let Ack::Confirmed {
        native_turn_id: Some(native),
    } = ack
    else {
        panic!("handled prompt ack {ack:?}");
    };
    assert!(common::has(&run.log, |e| matches!(
        e,
        Entry::TurnStarted { request_id: Some(r), .. } if *r == req
    )));
    match run.session.steer("late", "owner", "hi", &native) {
        Err(DriverError::TurnEnded) => {}
        other => panic!("steer into an ended turn: {other:?}"),
    }
    let turn = run.turn(READY, Duration::from_secs(240));
    assert!(common::says_ready(&turn.text), "next reply {:?}", turn.text);
    run.stop();
}

/// A steer names the running turn from its ack and lands in it.
#[test]
fn steer() {
    let Some(program) = program(Harness::Pi) else {
        return skip(Harness::Pi, "binary not found");
    };
    let repo = temp_repo("pi-steer");
    let mut s = spec(Harness::Pi, &program, &repo);
    local(&mut s);
    let mut run = Run::start(s);
    run.ready(Duration::from_secs(90)).expect("ready");
    let req = run.send(LONG);
    let Ack::Confirmed {
        native_turn_id: Some(native),
    } = run.session.wait_ack(&req, Duration::from_secs(60))
    else {
        panic!("prompt not confirmed");
    };
    let mut steered = false;
    let turn = run.collect(Duration::from_secs(240), |session, entry| {
        if !steered && matches!(entry, Entry::AssistantText { r#final: false, .. }) {
            steered = true;
            session
                .steer(
                    "steer-1",
                    "owner",
                    "Stop now and reply only with the word STEERED.",
                    &native,
                )
                .expect("steer");
        }
    });
    assert!(steered, "a delta arrived");
    assert!(turn.settled.is_some(), "the turn settled");
    assert_eq!(
        run.session.wait_ack("steer-1", Duration::from_secs(1)),
        Ack::Confirmed {
            native_turn_id: Some(native.clone())
        }
    );
    match run.session.steer("steer-2", "owner", "hi", &native) {
        Err(DriverError::TurnEnded) => {}
        other => panic!("steer after settle: {other:?}"),
    }
    run.stop();
}

/// An effort Pi does not take is dropped with a notice, and the session starts on its default.
#[test]
fn unknown_effort_falls_back() {
    if !configured() {
        return;
    }
    let Some(program) = program(Harness::Pi) else {
        return skip(Harness::Pi, "binary not found");
    };
    let repo = temp_repo("pi-effort");
    let mut s = spec(Harness::Pi, &program, &repo);
    local(&mut s);
    s.effort = Some("bogus".into());
    let mut run = Run::start(s);
    run.ready(Duration::from_secs(90)).expect("ready");
    let log = run.stop();
    assert!(common::has(&log, |e| matches!(
        e,
        Entry::Notice { code: Some(c), .. } if c == "effort"
    )));
    assert!(common::has(&log, |e| matches!(
        e,
        Entry::SessionInfo { effort, .. } if effort.as_deref() != Some("bogus")
    )));
}
