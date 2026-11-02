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
