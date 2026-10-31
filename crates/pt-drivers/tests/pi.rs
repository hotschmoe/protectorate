//! Real Pi runs against the owner's local endpoint (the `hotschmoe` provider in Pi's own
//! models.json), selected only with Pi's `--provider` and `--model` flags.

mod common;

use pt_drivers::{Harness, SpawnSpec};

fn local(spec: &mut SpawnSpec) {
    spec.provider =
        Some(std::env::var("PT_E2E_PI_PROVIDER").unwrap_or_else(|_| "hotschmoe".into()));
    spec.model = std::env::var("PT_E2E_PI_MODEL").ok().or_else(|| {
        Some("qwen3.8-27b-FP8-official-W8A16-mtp3-r187-xpugraph-cacheon-ctx200000-daily".into())
    });
}

#[test]
fn ready_resume_tool() {
    common::ready_resume_tool(Harness::Pi, local);
}

#[test]
fn interrupt() {
    common::interrupt(Harness::Pi, local);
}
