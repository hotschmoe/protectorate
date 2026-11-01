//! The task-size class that `select_target` takes (D27): code derives it, the owner may fix it,
//! Envoy may nudge it one class, and code clamps the result.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TaskSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TaskSize {
    pub fn name(self) -> &'static str {
        match self {
            TaskSize::Small => "small",
            TaskSize::Medium => "medium",
            TaskSize::Large => "large",
        }
    }

    pub fn parse(s: &str) -> Option<TaskSize> {
        match s {
            "small" => Some(TaskSize::Small),
            "medium" => Some(TaskSize::Medium),
            "large" => Some(TaskSize::Large),
            _ => None,
        }
    }

    /// Percent of quota that must be usable now for a task of this size.
    pub fn min_usable_pct(self) -> f64 {
        match self {
            TaskSize::Small => 5.0,
            TaskSize::Medium => 15.0,
            TaskSize::Large => 30.0,
        }
    }

    /// One class up (`delta` > 0) or down (`delta` < 0), never past the ends.
    pub fn nudged(self, delta: i8) -> TaskSize {
        match (self, delta.signum()) {
            (TaskSize::Small, 1) => TaskSize::Medium,
            (TaskSize::Medium, 1) => TaskSize::Large,
            (TaskSize::Large, -1) => TaskSize::Medium,
            (TaskSize::Medium, -1) => TaskSize::Small,
            (s, _) => s,
        }
    }
}

/// Cost bounds between the classes, in whatever unit the history records (for example tokens):
/// a median at or below `small_max` is small, at or above `large_min` is large.
#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub small_max: u64,
    pub large_min: u64,
}

/// The median of the last `n` costs (the lower middle for an even count).
pub fn median_of_last(costs: &[u64], n: usize) -> Option<u64> {
    let tail = &costs[costs.len().saturating_sub(n)..];
    if tail.is_empty() {
        return None;
    }
    let mut sorted = tail.to_vec();
    sorted.sort_unstable();
    Some(sorted[(sorted.len() - 1) / 2])
}

/// The class for a task in a workspace: the owner's class if set (Envoy cannot move it),
/// otherwise the class of the median of the last `n` costs (medium with no history), moved at
/// most one class by `envoy_nudge`.
pub fn derive(
    history: &[u64],
    n: usize,
    thresholds: Thresholds,
    owner: Option<TaskSize>,
    envoy_nudge: i8,
) -> TaskSize {
    if let Some(owner) = owner {
        return owner;
    }
    let base = match median_of_last(history, n) {
        None => TaskSize::Medium,
        Some(m) if m <= thresholds.small_max => TaskSize::Small,
        Some(m) if m >= thresholds.large_min => TaskSize::Large,
        Some(_) => TaskSize::Medium,
    };
    base.nudged(envoy_nudge)
}
