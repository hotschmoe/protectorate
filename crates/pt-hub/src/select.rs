//! `select_target` (SPEC 8), the pure answer to the core's `SelectTarget`: the first candidate in
//! the sleeve's ordered `resleeve_to` list that is not excluded and is eligible. Quota weighting is
//! not in yet (REVISIT).

use pt_core::{Mode, Target};

pub fn select_target(
    candidates: &[Target],
    exclude: &[Target],
    eligible: &dyn Fn(&Target) -> bool,
) -> Option<Target> {
    candidates
        .iter()
        .find(|t| t.mode == Mode::Headless && !exclude.contains(t) && eligible(t))
        .cloned()
}
