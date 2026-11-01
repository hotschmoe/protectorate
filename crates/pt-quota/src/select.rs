//! `select_target` (SPEC 8): which harness a new incarnation gets, from fresh quota only, plus
//! the spare-quota trigger (D29). Both are pure.

use pt_api::{Harness, Mode, Quota, QuotaWindow, Subscription};

use crate::task_size::TaskSize;
use crate::{State, state};

/// The time to reset at which a window's weight halves.
const URGENCY_HALF_HOURS: f64 = 48.0;
/// Stands in for a window whose reset time is unknown.
const UNKNOWN_RESET_HOURS: f64 = 168.0;
/// Pace extrapolation needs at least this much of the window elapsed.
const MIN_ELAPSED_FRACTION: f64 = 0.05;
const HOUR_MS: f64 = 3_600_000.0;

/// The harnesses that draw on a metered subscription, in tie-break order.
const SUBSCRIBED: [(Harness, Subscription); 3] = [
    (Harness::Claude, Subscription::Claude),
    (Harness::Codex, Subscription::Codex),
    (Harness::Grok, Subscription::Grok),
];

/// A harness a caller knows to be down (node offline, binary missing, probe failing).
/// A harness with no entry is taken as healthy.
#[derive(Clone, Copy, Debug)]
pub struct Health {
    pub harness: Harness,
    pub healthy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub harness: Harness,
    pub subscription: Subscription,
    /// Weighted unused quota at reset; 0 for the local fallback.
    pub score: f64,
    pub reason: String,
}

struct Scored {
    score: f64,
    usable: f64,
    expiring: f64,
    window: String,
    hours: Option<f64>,
}

/// Window length from its label: `5h`, `3d`, `wk`, `day`, `mo` or a word like `wk sonnet`.
pub fn window_length_ms(label: &str) -> Option<u64> {
    const HOUR: u64 = 3_600_000;
    for tok in label.split_whitespace() {
        let ms = match tok {
            "wk" | "weekly" => Some(7 * 24 * HOUR),
            "day" | "daily" => Some(24 * HOUR),
            "mo" | "monthly" => Some(30 * 24 * HOUR),
            _ if tok.len() >= 2 && tok.is_char_boundary(tok.len() - 1) => {
                let (num, unit) = tok.split_at(tok.len() - 1);
                let n = num.parse::<u64>().ok().filter(|&n| n > 0);
                match (n, unit) {
                    (Some(n), "h") => n.checked_mul(HOUR),
                    (Some(n), "d") => n.checked_mul(24 * HOUR),
                    _ => None,
                }
            }
            _ => None,
        };
        if ms.is_some() {
            return ms;
        }
    }
    None
}

/// Extrapolates the pace from window start to the fetch time out to the reset.
fn projected_unused(w: &QuotaWindow, used: f64, fetched_ms: u64, now_ms: u64) -> Option<f64> {
    let length = window_length_ms(&w.label)? as f64;
    let resets = w.resets_at_ms?.0;
    if now_ms > resets {
        return None;
    }
    let start = resets as f64 - length;
    let frac = (fetched_ms as f64 - start) / length;
    if !(MIN_ELAPSED_FRACTION..=1.0).contains(&frac) {
        return None;
    }
    Some((100.0 - used / frac).clamp(0.0, 100.0))
}

/// Scores a fresh quota by its windows that have not yet reset. `None` when nothing is usable.
fn score(q: &Quota, now_ms: u64) -> Option<Scored> {
    struct Cand {
        label: String,
        remaining: f64,
        projected: Option<f64>,
        hours: Option<f64>,
    }
    let mut cands = Vec::new();
    for w in &q.windows {
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            continue;
        };
        if w.resets_at_ms.is_some_and(|r| r.0 <= now_ms) {
            continue;
        }
        cands.push(Cand {
            label: w.label.clone(),
            remaining: (100.0 - used).clamp(0.0, 100.0),
            projected: projected_unused(w, used, q.fetched_at_ms.0, now_ms),
            hours: w.resets_at_ms.map(|r| (r.0 - now_ms) as f64 / HOUR_MS),
        });
    }
    let usable = cands
        .iter()
        .map(|c| c.remaining)
        .fold(f64::INFINITY, f64::min);
    let mut best: Option<Scored> = None;
    for c in &cands {
        let expiring = c
            .projected
            .map_or(c.remaining, |p| p.min(c.remaining))
            .min(usable);
        let score = expiring / (1.0 + c.hours.unwrap_or(UNKNOWN_RESET_HOURS) / URGENCY_HALF_HOURS);
        if best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(Scored {
                score,
                usable,
                expiring,
                window: c.label.clone(),
                hours: c.hours,
            });
        }
    }
    best
}

fn fresh_for(quotas: &[Quota], sub: Subscription) -> Option<&Quota> {
    quotas
        .iter()
        .find(|q| q.subscription == sub && state(q) == State::Fresh)
}

/// Picks the harness for a new incarnation, or `None` when nothing is eligible.
///
/// - A subscription is eligible when its harness is allowed, supports `mode`, is not excluded
///   or unhealthy, and has a fresh reading with at least `size.min_usable_pct()` usable in its
///   tightest window. Stale, unknown and error readings never count.
/// - The best is the highest `unused at reset / (1 + hours to reset / 48)`; ties go to the
///   earlier of Claude, Codex, Grok.
/// - Only when none is eligible does the local model (`PiLocal`) win, and only if it is allowed,
///   not excluded or unhealthy, and its own reading is fresh (its endpoint answered).
pub fn select_target(
    now_ms: u64,
    size: TaskSize,
    allow: &[Harness],
    mode: Mode,
    exclude: &[Harness],
    quotas: &[Quota],
    health: &[Health],
) -> Option<Target> {
    let usable = |h: Harness| {
        allow.contains(&h)
            && !exclude.contains(&h)
            && health.iter().all(|e| e.harness != h || e.healthy)
            && supports(h, mode)
    };
    let mut best: Option<Target> = None;
    for (harness, subscription) in SUBSCRIBED {
        if !usable(harness) {
            continue;
        }
        let Some(s) = fresh_for(quotas, subscription).and_then(|q| score(q, now_ms)) else {
            continue;
        };
        if s.usable < size.min_usable_pct() || best.as_ref().is_some_and(|b| s.score <= b.score) {
            continue;
        }
        let reset = match s.hours {
            Some(h) => format!("resets in {}", crate::fmt_age((h * HOUR_MS) as u64)),
            None => "reset time unknown".to_string(),
        };
        best = Some(Target {
            harness,
            subscription,
            score: s.score,
            reason: format!(
                "{} window {reset}: ~{:.0}% expiring unused, {:.0}% usable now",
                s.window, s.expiring, s.usable
            ),
        });
    }
    if best.is_some() {
        return best;
    }
    if usable(Harness::PiLocal) && fresh_for(quotas, Subscription::Local).is_some() {
        return Some(Target {
            harness: Harness::PiLocal,
            subscription: Subscription::Local,
            score: 0.0,
            reason: "no subscription with fresh spare quota; local model is reachable".to_string(),
        });
    }
    None
}

fn supports(h: Harness, mode: Mode) -> bool {
    match mode {
        Mode::Headless => h != Harness::Antigravity,
        Mode::Pane => true,
    }
}

/// A subscription whose quota is projected to go unused (D29).
#[derive(Clone, Debug, PartialEq)]
pub struct Spare {
    pub harness: Harness,
    pub subscription: Subscription,
    pub unused_pct: f64,
}

/// Projected unused percent at reset of the best-weighted window of a fresh quota.
pub fn spare_quota(now_ms: u64, q: &Quota) -> Option<f64> {
    if state(q) != State::Fresh {
        return None;
    }
    score(q, now_ms).map(|s| s.expiring)
}

/// The spare-quota trigger (D29): every metered subscription whose projected unused quota at
/// reset is above `threshold_pct`, most spare first, and nothing while high-priority work is
/// queued. Fresh readings only.
pub fn spare_trigger(
    now_ms: u64,
    quotas: &[Quota],
    threshold_pct: f64,
    high_priority_queued: bool,
) -> Vec<Spare> {
    if high_priority_queued {
        return Vec::new();
    }
    let mut out: Vec<Spare> = SUBSCRIBED
        .iter()
        .filter_map(|&(harness, subscription)| {
            let unused = spare_quota(now_ms, fresh_for(quotas, subscription)?)?;
            (unused > threshold_pct).then_some(Spare {
                harness,
                subscription,
                unused_pct: unused,
            })
        })
        .collect();
    out.sort_by(|a, b| b.unused_pct.total_cmp(&a.unused_pct));
    out
}
