//! The answer to the core's `SelectTarget` (SPEC 8): among the sleeve's ordered `resleeve_to`
//! candidates that are headless, not excluded and eligible, `pt_quota::select_with_fallback`
//! picks by fresh quota, then a stale row that still has spare, then a candidate with no
//! reading. A failed reading is not spent, and neither is a harness that has no meter.
//! Each candidate keeps its model. A model cap gates that model. No model and `default`
//! are not blocked by a cap; one under the minimum only loses a tie and is named in the reason.

use pt_api as api;
use pt_core::{Mode, Target};
use pt_quota::{Candidate, TaskSize};

use crate::wire::api_harness;

/// The target and why it was chosen.
pub fn select_target(
    now_ms: u64,
    candidates: &[Target],
    exclude: &[Target],
    eligible: &dyn Fn(&Target) -> bool,
    quotas: &[api::Quota],
) -> Result<(Target, String), String> {
    let open: Vec<&Target> = candidates
        .iter()
        .filter(|t| t.mode == Mode::Headless && !exclude.contains(t) && eligible(t))
        .collect();
    let allow: Vec<Candidate> = open
        .iter()
        .map(|t| Candidate {
            harness: api_harness(t.harness),
            model: t.model.clone(),
        })
        .collect();
    let (idx, why) = pt_quota::select_with_fallback(now_ms, TaskSize::Medium, &allow, quotas)?;
    let target = open
        .get(idx)
        .ok_or_else(|| "no eligible resleeve target".to_string())?;
    Ok(((*target).clone(), why))
}
