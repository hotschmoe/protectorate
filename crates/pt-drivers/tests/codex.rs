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
