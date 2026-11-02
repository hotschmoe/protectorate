use std::time::Duration;

use pt_api::{QuotaWindow, Subscription};

use crate::error::Result;

/// What a reader found in one successful read.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub plan: String,
    pub windows: Vec<QuotaWindow>,
    pub note: String,
}

/// A read-only quota source for one subscription.
pub trait Reader: Send + Sync {
    fn subscription(&self) -> Subscription;
    /// `now_ms` is the clock for expiry checks; `timeout` bounds the whole read.
    fn read(&self, now_ms: u64, timeout: Duration) -> Result<Snapshot>;
}

pub(crate) fn clamp_pct(p: f64) -> Option<f64> {
    p.is_finite().then(|| p.max(0.0))
}

/// `300` -> `5h`, `10080` -> `wk`, `1440` -> `day`; other lengths are `Nd`, `Nh` or `Nm`.
pub(crate) fn window_label(mins: i64) -> String {
    match mins {
        m if m <= 0 => "window".to_string(),
        10080 => "wk".to_string(),
        1440 => "day".to_string(),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

pub(crate) fn window(
    label: impl Into<String>,
    used_pct: f64,
    resets_at_ms: Option<u64>,
) -> QuotaWindow {
    QuotaWindow {
        label: label.into(),
        used_pct: clamp_pct(used_pct),
        resets_at_ms: resets_at_ms.map(pt_api::EpochMs),
        length_ms: None,
    }
}
