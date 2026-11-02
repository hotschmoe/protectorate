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
/// How long a passed reset still blocks. Matches the cache's default stale window: the vendor
/// can keep enforcing the old counter briefly, and a row fetched before the reset is the one
/// that still carries it. After this, the window counts as reset.
const RESET_GRACE_MS: u64 = 600_000;

/// A reset at or before the epoch is "no reset": the percent still counts.
fn reset_at(w: &QuotaWindow) -> Option<u64> {
    w.resets_at_ms.map(|r| r.0).filter(|t| *t > 0)
}

/// Account windows always gate. A Claude model cap (`wk <name>`) gates a candidate only when
/// the candidate's model name contains that name.
fn gates(label: &str, model: &str) -> bool {
    let Some(scope) = label.strip_prefix("wk ") else {
        return true;
    };
    if scope.is_empty() {
        return true;
    }
    let scope = letters(scope);
    let model = letters(model);
    scope.len() >= 3 && model.contains(&scope)
}

fn letters(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// The row has finite usage windows, and none of them apply to `model`.
fn other_scopes_only(q: &Quota, model: &str) -> bool {
    let mut any = false;
    for w in &q.windows {
        if w.used_pct.filter(|u| u.is_finite()).is_none() {
            continue;
        }
        any = true;
        if gates(&w.label, model) {
            return false;
        }
    }
    any
}

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

/// One ordered resleeve candidate. An empty `model` gates on the account windows only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub harness: Harness,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub harness: Harness,
    pub model: String,
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

/// Window length from its label: `5h`, `5m`, `3d`, `wk`, `day`, `mo` or a word like `wk sonnet`.
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
                    (Some(n), "m") => n.checked_mul(60_000),
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
    let length = w
        .length_ms
        .filter(|&ms| ms > 0)
        .or_else(|| window_length_ms(&w.label))? as f64;
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
fn score(q: &Quota, now_ms: u64, model: &str) -> Option<Scored> {
    struct Cand {
        label: String,
        remaining: f64,
        projected: Option<f64>,
        hours: Option<f64>,
    }
    let mut cands = Vec::new();
    for w in &q.windows {
        if !gates(&w.label, model) {
            continue;
        }
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            continue;
        };
        if reset_at(w).is_some_and(|r| r <= now_ms) {
            continue;
        }
        cands.push(Cand {
            label: w.label.clone(),
            remaining: (100.0 - used).clamp(0.0, 100.0),
            projected: projected_unused(w, used, q.fetched_at_ms.0, now_ms),
            hours: reset_at(w).map(|r| (r - now_ms) as f64 / HOUR_MS),
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

/// A window whose reset has passed and whose remaining percent is under `min` still blocks,
/// for `RESET_GRACE_MS` after the stamp, when this row was fetched before that reset.
fn past_blocks(q: &Quota, now_ms: u64, min: f64, model: &str) -> bool {
    q.windows.iter().any(|w| {
        if !gates(&w.label, model) {
            return false;
        }
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            return false;
        };
        let Some(reset) = reset_at(w) else {
            return false;
        };
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        reset <= now_ms
            && remaining < min
            && q.fetched_at_ms.0 < reset
            && now_ms < reset.saturating_add(RESET_GRACE_MS)
    })
}

fn row(quotas: &[Quota], sub: Subscription) -> Option<&Quota> {
    quotas.iter().find(|q| q.subscription == sub)
}

fn fresh_for(quotas: &[Quota], sub: Subscription) -> Option<&Quota> {
    row(quotas, sub).filter(|q| state(q) == State::Fresh)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Availability {
    /// Fresh spare, or a stale row whose last windows still clear the minimum.
    Available,
    Exhausted,
    /// No row, a row that was never a reading, or a row with nothing left to score.
    Unknown,
    /// Every gating window's reset has passed, so the old percent no longer applies.
    Reset,
    /// No quota reader exists for this harness, so it is not a fallback.
    Unmetered,
}

/// Whether a candidate has usable quota, insufficient quota, or no usable reading.
///
/// A stale subscription keeps the windows of its last good read. Under the task minimum that
/// is exhausted, the same as a fresh row; with spare it may still be chosen. A row whose
/// gating windows have all reset is not a missing reading. Unknown is a row with nothing to
/// score.
fn availability(now_ms: u64, size: TaskSize, cand: &Candidate, quotas: &[Quota]) -> Availability {
    let harness = cand.harness;
    let model = cand.model.as_str();
    if harness == Harness::PiLocal {
        // No percent windows. A fresh answer may be spent. A stale or failed one may not:
        // the endpoint worked and then stopped. Never read stays unknown.
        return match row(quotas, Subscription::Local).map(state) {
            Some(State::Fresh) => Availability::Available,
            Some(State::Error | State::Stale) => Availability::Exhausted,
            _ => Availability::Unknown,
        };
    }
    let Some((_, subscription)) = SUBSCRIBED.iter().find(|(h, _)| *h == harness) else {
        // PiVendor and Antigravity have no reader. Missing is not "unknown": spending
        // them would use an API key with no window.
        return Availability::Unmetered;
    };
    let Some(q) = row(quotas, *subscription) else {
        return Availability::Unknown;
    };
    // `Unknown` is no login, an expired login, the reader being off, or not read yet. `Error`
    // is a read that failed (parse, HTTP, timeout) and left no windows to score.
    if state(q) == State::Error {
        return Availability::Exhausted;
    }
    if state(q) != State::Fresh && state(q) != State::Stale {
        return Availability::Unknown;
    }
    // A row that only names other models' caps is not "no reading": spending it would ignore them.
    if other_scopes_only(q, model) {
        return Availability::Exhausted;
    }
    if past_blocks(q, now_ms, size.min_usable_pct(), model) {
        return Availability::Exhausted;
    }
    match score(q, now_ms, model) {
        Some(s) if s.usable < size.min_usable_pct() => Availability::Exhausted,
        Some(_) => Availability::Available,
        None if counted_reset(q, now_ms, model) => Availability::Reset,
        None => Availability::Unknown,
    }
}

/// A gating window whose reset has passed. Once it no longer blocks, the row is a reset,
/// not a candidate with no reading.
fn counted_reset(q: &Quota, now_ms: u64, model: &str) -> bool {
    q.windows.iter().any(|w| {
        gates(&w.label, model)
            && w.used_pct.filter(|u| u.is_finite()).is_some()
            && reset_at(w).is_some_and(|r| r <= now_ms)
    })
}

/// Selects from already eligible candidates, retaining their order when readings are missing.
///
/// Fresh spare wins through `select_target`. That includes a fresh local model, which beats a
/// stale row with spare. A stale subscription whose last windows still clear the minimum is
/// next, ahead of a candidate with no reading, so an exhausted account is not spent and a
/// subscription that still had room is not parked.
pub fn select_with_fallback(
    now_ms: u64,
    size: TaskSize,
    candidates: &[Candidate],
    quotas: &[Quota],
) -> Result<(usize, String), &'static str> {
    if let Some(target) = select_target(now_ms, size, candidates, Mode::Headless, &[], quotas, &[])
        && let Some(idx) = candidates
            .iter()
            .position(|c| c.harness == target.harness && c.model == target.model)
    {
        return Ok((idx, target.reason));
    }
    let avail = |cand: &Candidate| availability(now_ms, size, cand, quotas);
    if let Some(idx) = candidates
        .iter()
        .position(|cand| avail(cand) == Availability::Available)
    {
        return Ok((
            idx,
            "first eligible in resleeve_to whose last reading still has spare quota".into(),
        ));
    }
    if let Some(idx) = candidates
        .iter()
        .position(|cand| matches!(avail(cand), Availability::Unknown | Availability::Reset))
    {
        let reason = match avail(&candidates[idx]) {
            Availability::Reset => "first eligible in resleeve_to whose quota windows have reset",
            _ => "first eligible in resleeve_to without a fresh quota reading",
        };
        return Ok((idx, reason.into()));
    }
    if candidates.is_empty() {
        Err("no eligible resleeve target")
    } else if candidates
        .iter()
        .all(|cand| avail(cand) == Availability::Unmetered)
    {
        Err("no eligible resleeve target has a quota meter")
    } else {
        Err("all eligible resleeve targets have insufficient fresh quota")
    }
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
    allow: &[Candidate],
    mode: Mode,
    exclude: &[Harness],
    quotas: &[Quota],
    health: &[Health],
) -> Option<Target> {
    let usable = |h: Harness| {
        allow.iter().any(|c| c.harness == h)
            && !exclude.contains(&h)
            && health.iter().all(|e| e.harness != h || e.healthy)
            && supports(h, mode)
    };
    let mut best: Option<Target> = None;
    for (harness, subscription) in SUBSCRIBED {
        if !usable(harness) {
            continue;
        }
        let Some(q) = fresh_for(quotas, subscription) else {
            continue;
        };
        for cand in allow.iter().filter(|c| c.harness == harness) {
            if past_blocks(q, now_ms, size.min_usable_pct(), &cand.model) {
                continue;
            }
            if other_scopes_only(q, &cand.model) {
                continue;
            }
            let Some(s) = score(q, now_ms, &cand.model) else {
                continue;
            };
            if s.usable < size.min_usable_pct() || best.as_ref().is_some_and(|b| s.score <= b.score)
            {
                continue;
            }
            let reset = match s.hours {
                Some(h) => format!("resets in {}", crate::fmt_age((h * HOUR_MS) as u64)),
                None => "reset time unknown".to_string(),
            };
            best = Some(Target {
                harness,
                model: cand.model.clone(),
                subscription,
                score: s.score,
                reason: format!(
                    "{} window {reset}: ~{:.0}% expiring unused, {:.0}% usable now",
                    s.window, s.expiring, s.usable
                ),
            });
        }
    }
    if best.is_some() {
        return best;
    }
    if usable(Harness::PiLocal) && fresh_for(quotas, Subscription::Local).is_some() {
        let model = allow
            .iter()
            .find(|c| c.harness == Harness::PiLocal)
            .map(|c| c.model.clone())
            .unwrap_or_default();
        return Some(Target {
            harness: Harness::PiLocal,
            model,
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
    score(q, now_ms, "").map(|s| s.expiring)
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
