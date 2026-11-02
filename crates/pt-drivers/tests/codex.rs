//! Real Codex app-server runs through the standalone native binary.

mod common;

use pt_drivers::{Harness, SpawnSpec};

fn cheap(spec: &mut SpawnSpec) {
    spec.model = Some(std::env::var("PT_E2E_CODEX_MODEL").unwrap_or_else(|_| "gpt-6-luna".into()));
    spec.effort = Some("low".into());
}

#[test]
fn ready_resume_tool() {
    common::ready_resume_tool(Harness::Codex, cheap);
}

/// An effort the model does not advertise is dropped with a notice, and the thread still opens.
#[test]
fn unadvertised_effort_falls_back() {
    let Some(program) = common::program(Harness::Codex) else {
        return common::skip(Harness::Codex, "binary not found");
    };
    let repo = common::temp_repo("codex-effort");
    let mut s = common::spec(Harness::Codex, &program, &repo);
    cheap(&mut s);
    s.effort = Some("bogus".into());
    let mut run = common::Run::start(s);
    run.ready(std::time::Duration::from_secs(90))
        .expect("ready");
    let log = run.stop();
    assert!(common::has(&log, |e| matches!(
        e,
        pt_drivers::Entry::Notice { code: Some(c), .. } if c == "effort"
    )));
    assert!(common::has(&log, |e| matches!(
        e,
        pt_drivers::Entry::SessionInfo { effort, .. } if effort.as_deref() != Some("bogus")
    )));
}

/// An error reply to the handshake (resuming a thread that does not exist) ends the process
/// at once, so the hub does not wait out its start timeout.
#[test]
fn refused_handshake_ends_the_session() {
    let Some(program) = common::program(Harness::Codex) else {
        return common::skip(Harness::Codex, "binary not found");
    };
    let repo = common::temp_repo("codex-refused");
    let mut s = common::spec(Harness::Codex, &program, &repo);
    s.resume = Some("00000000-0000-4000-8000-000000000000".into());
    let mut run = common::Run::start(s);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut errored = false;
    loop {
        match run.next(deadline) {
            Some(pt_drivers::Entry::Error { .. }) => errored = true,
            Some(pt_drivers::Entry::Exited { requested, .. }) => {
                assert!(!requested, "the driver ended it, nobody asked");
                break;
            }
            Some(_) => {}
            None => panic!("the session outlived a refused handshake"),
        }
    }
    assert!(errored, "the refusal is reported before the exit");
}
