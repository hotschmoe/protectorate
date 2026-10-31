//! Real Grok Build runs over ACP.

mod common;

use pt_drivers::{Harness, SpawnSpec};

fn cheap(spec: &mut SpawnSpec) {
    spec.model =
        Some(std::env::var("PT_E2E_GROK_MODEL").unwrap_or_else(|_| "grok-4.7-build-fast".into()));
}

#[test]
fn ready_resume_tool() {
    common::ready_resume_tool(Harness::Grok, cheap);
}
