//! The answer to the core's `SelectTarget` (SPEC 8): among the sleeve's ordered `resleeve_to`
//! candidates that are headless, not excluded and eligible, `pt_quota::select_target` picks by
//! fresh quota; when a reading is unavailable, the first candidate without one may be used.

use pt_api as api;
use pt_core::{Mode, Target};
use pt_quota::TaskSize;

use crate::wire::api_harness;

/// The target and why it was chosen.
pub fn select_target(
    now_ms: u64,
    candidates: &[Target],
    exclude: &[Target],
    eligible: &dyn Fn(&Target) -> bool,
    quotas: &[api::Quota],
) -> Result<(Target, String), &'static str> {
    let open: Vec<&Target> = candidates
        .iter()
        .filter(|t| t.mode == Mode::Headless && !exclude.contains(t) && eligible(t))
        .collect();
    let allow: Vec<api::Harness> = open.iter().map(|t| api_harness(t.harness)).collect();
    let (chosen, why) = pt_quota::select_with_fallback(now_ms, TaskSize::Medium, &allow, quotas)?;
    let target = open
        .iter()
        .find(|t| api_harness(t.harness) == chosen)
        .ok_or("no eligible resleeve target")?;
    Ok(((*target).clone(), why))
}
