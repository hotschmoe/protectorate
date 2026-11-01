//! The answer to the core's `SelectTarget` (SPEC 8): among the sleeve's ordered `resleeve_to`
//! candidates that are headless, not excluded and eligible, `pt_quota::select_target` picks by
//! fresh quota; when no quota reading makes one eligible, the first in the owner's order wins.

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
) -> Option<(Target, String)> {
    let open: Vec<&Target> = candidates
        .iter()
        .filter(|t| t.mode == Mode::Headless && !exclude.contains(t) && eligible(t))
        .collect();
    let allow: Vec<api::Harness> = open.iter().map(|t| api_harness(t.harness)).collect();
    let by_quota = pt_quota::select_target(
        now_ms,
        TaskSize::Medium,
        &allow,
        api::Mode::Headless,
        &[],
        quotas,
        &[],
    )
    .and_then(|pick| {
        let t = open
            .iter()
            .find(|t| api_harness(t.harness) == pick.harness)?;
        Some(((*t).clone(), pick.reason))
    });
    by_quota.or_else(|| {
        open.first().map(|t| {
            (
                (*t).clone(),
                "first eligible in resleeve_to (no fresh quota)".into(),
            )
        })
    })
}
