//! Real Pi runs against a local OpenAI-compatible endpoint, selected only with Pi's
//! `--provider` and `--model` flags: `PT_E2E_PI_PROVIDER` (a provider in Pi's own models.json)
//! and `PT_E2E_PI_MODEL`. Both tests skip while either is unset; the Makefile reads them from a
//! gitignored `.env.e2e` when it exists.

mod common;

use pt_drivers::{Harness, SpawnSpec};

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
