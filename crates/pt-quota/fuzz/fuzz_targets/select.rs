#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_api::{Harness, Mode, Quota, QuotaWindow, Subscription};
use pt_quota::{
    Candidate, Health, State, TaskSize, Thresholds, derive, select_target, select_with_fallback,
    spare_trigger, state, window_length_ms,
};
use pt_quota_fuzz::{Bytes, HARNESSES, quota};

fuzz_target!(|data: &[u8]| {
    let mut b = Bytes::new(data);
    let now = 1_800_000_000_000u64 + u64::from(b.u32());
    let size = [TaskSize::Small, TaskSize::Medium, TaskSize::Large][b.below(3)];
    let mode = if b.bool() { Mode::Headless } else { Mode::Pane };
    let pick = |b: &mut Bytes| -> Vec<Harness> {
        let mask = b.u8();
        HARNESSES
            .iter()
            .enumerate()
            .filter(|(i, _)| mask >> i & 1 == 1)
            .map(|(_, h)| *h)
            .collect()
    };
    let allow_h = pick(&mut b);
    let exclude = pick(&mut b);
    let allow: Vec<Candidate> = allow_h
        .iter()
        .map(|harness| Candidate {
            harness: *harness,
            model: ["", "opus", "sonnet", "claude-opus-4-6"][b.below(4)].to_string(),
        })
        .collect();
    let mut health = Vec::new();
    for h in HARNESSES {
        if b.bool() {
            health.push(Health {
                harness: h,
                healthy: b.bool(),
            });
        }
    }
    let quotas: Vec<_> = (0..b.below(8)).map(|_| quota(&mut b, now)).collect();

    // SPEC 8, stated on the outputs: an epoch reset is not a reset, and a stored length of 0
    // is not a length. Clearing either leaves the pick unchanged.
    let same_pick = |altered: &[Quota]| {
        assert_eq!(
            select_target(now, size, &allow, mode, &exclude, &quotas, &health),
            select_target(now, size, &allow, mode, &exclude, altered, &health)
        );
        assert_eq!(
            select_with_fallback(now, size, &allow, &quotas),
            select_with_fallback(now, size, &allow, altered)
        );
    };
    same_pick(&map_windows(&quotas, |w| {
        if w.resets_at_ms.is_some_and(|r| r.0 == 0) {
            w.resets_at_ms = None;
        }
    }));
    same_pick(&map_windows(&quotas, |w| {
        if w.length_ms == Some(0) {
            w.length_ms = None;
        }
    }));

    let eligible = |h: Harness| {
        allow.iter().any(|c| c.harness == h)
            && !exclude.contains(&h)
            && health.iter().all(|e| e.harness != h || e.healthy)
    };
    let got = select_target(now, size, &allow, mode, &exclude, &quotas, &health);

    if let Some(t) = &got {
        assert!(
            eligible(t.harness),
            "picked an excluded, disallowed or unhealthy harness: {t:?}"
        );
        let first_fresh = quotas
            .iter()
            .find(|q| q.subscription == t.subscription && state(q) == State::Fresh);
        let Some(fresh) = first_fresh else {
            panic!("picked a target with no fresh reading: {t:?}");
        };
        assert!(t.score.is_finite() && t.score >= 0.0);
        // SPEC 8. A fresh pick is spare: nothing under the minimum still blocks.
        if metered(t.harness).is_some() {
            let min = size.min_usable_pct();
            assert!(
                !still_enforcing(fresh, now, min, &t.model),
                "a fresh exhausted row was picked: {t:?}"
            );
            match open_usable(fresh, now, &t.model, min) {
                Some(usable) if usable + 1e-9 >= min => {}
                Some(usable) => panic!("picked a row at {usable}% usable: {t:?}"),
                None => panic!("picked a row with nothing usable: {t:?}"),
            }
            // SPEC 8. A cap under the minimum does not block an unspecified model. When that
            // candidate is chosen, the reason names the tightest such cap.
            if let Some((_, label)) = limiting_cap(fresh, &t.model, min, now) {
                let phrase = format!("; {label} under the minimum");
                assert!(
                    t.reason.contains(&phrase),
                    "chosen unspecified model did not name {label}: {}",
                    t.reason
                );
            }
        }
        if t.harness == Harness::PiLocal {
            assert_eq!(t.subscription, Subscription::Local);
            let without_local: Vec<Candidate> = allow
                .iter()
                .filter(|c| c.harness != Harness::PiLocal)
                .cloned()
                .collect();
            assert!(
                select_target(now, size, &without_local, mode, &exclude, &quotas, &health)
                    .is_none(),
                "fell back to local while a subscription was eligible"
            );
        } else {
            assert!(t.score >= 0.0);
            // Excluding the winner can only lower the best score.
            let mut ex2 = exclude.clone();
            ex2.push(t.harness);
            if let Some(next) = select_target(now, size, &allow, mode, &ex2, &quotas, &health)
                && next.harness != Harness::PiLocal
            {
                assert!(
                    next.score <= t.score,
                    "winner is not the best: {t:?} vs {next:?}"
                );
            }
        }
    } else {
        // Nothing eligible: no allowed, healthy harness has a fresh quota for its subscription.
        let without_local: Vec<Candidate> = allow
            .iter()
            .filter(|c| c.harness != Harness::PiLocal)
            .cloned()
            .collect();
        assert!(
            select_target(now, size, &without_local, mode, &exclude, &quotas, &health).is_none()
        );
    }

    // Stale, unknown and error readings alone never produce a pick.
    let unfresh: Vec<_> = quotas
        .iter()
        .filter(|q| state(q) != State::Fresh)
        .cloned()
        .collect();
    assert!(select_target(now, size, &allow, mode, &exclude, &unfresh, &health).is_none());

    // The hub drops excluded and ineligible harnesses, then passes that list
    // (pt-hub/src/select.rs). `select_with_fallback` calls `select_target` with
    // headless mode and none of those filters, so both calls must see `open`.
    let open: Vec<Candidate> = allow
        .iter()
        .filter(|c| eligible(c.harness))
        .cloned()
        .collect();
    let picked = select_target(now, size, &open, Mode::Headless, &[], &quotas, &[]);
    let fallback = select_with_fallback(now, size, &open, &quotas);
    match (&picked, &fallback) {
        (Some(t), Ok((idx, _))) => {
            let c = &open[*idx];
            assert_eq!(t.harness, c.harness);
            assert_eq!(t.model, c.model);
        }
        (Some(t), Err(e)) => panic!("fallback dropped {t:?}: {e}"),
        (None, Ok((idx, reason))) => {
            let c = &open[*idx];
            let h = c.harness;
            assert!(
                matches!(
                    h,
                    Harness::Claude | Harness::Codex | Harness::Grok | Harness::PiLocal
                ),
                "unmetered harness was spent: {h:?}"
            );
            if let Some(sub) = metered(h)
                && let Some(q) = quotas.iter().find(|q| q.subscription == sub)
            {
                assert_ne!(state(q), State::Error, "a failed reading was spent");
                // SPEC 8. Caps for other models are no reading for this candidate.
                if only_other_scopes(q, &c.model) {
                    assert!(
                        reason.contains("without a fresh quota reading"),
                        "other models' caps were not treated as no reading: {reason}"
                    );
                }
                if matches!(state(q), State::Fresh | State::Stale) {
                    let min = size.min_usable_pct();
                    let enforcing = still_enforcing(q, now, min, &c.model);
                    let usable = open_usable(q, now, &c.model, min);
                    // A fresh row under the minimum is exhausted. Fallback must not spend it.
                    if state(q) == State::Fresh
                        && (enforcing || usable.is_some_and(|u| u + 1e-9 < min))
                    {
                        panic!("a fresh exhausted row was spent");
                    }
                    assert!(!enforcing, "a passed exhausted window was spent");
                    if let Some(usable) = usable {
                        assert!(usable + 1e-9 >= min, "row at {usable}% usable was spent");
                    }
                    if let Some((_, label)) = limiting_cap(q, &c.model, min, now) {
                        let phrase = format!("; {label} under the minimum");
                        assert!(
                            reason.contains(&phrase),
                            "spent unspecified model did not name {label}: {reason}"
                        );
                    }
                }
            }
            if h == Harness::PiLocal
                && let Some(q) = quotas
                    .iter()
                    .find(|q| q.subscription == Subscription::Local)
            {
                assert_ne!(state(q), State::Error, "a failed local reading was spent");
                assert_ne!(state(q), State::Stale, "a stale local reading was spent");
            }
        }
        (None, Err(reason)) => {
            let min = size.min_usable_pct();
            let mut cap: Option<(f64, String)> = None;
            for c in &open {
                // SPEC 8. Park only when no candidate is unknown, reset, or missing a row.
                if unknown_or_reset(c, &quotas, now, min) {
                    panic!("parked while {c:?} could be spent with no spare");
                }
                if let Some(sub) = metered(c.harness)
                    && let Some(q) = quotas.iter().find(|q| q.subscription == sub)
                    && state(q) == State::Stale
                    && !still_enforcing(q, now, min, &c.model)
                    && let Some(usable) = open_usable(q, now, &c.model, min)
                    && usable + 1e-9 >= min
                {
                    panic!("stale row at {usable}% usable was parked");
                }
                if let Some(sub) = metered(c.harness)
                    && let Some(q) = quotas.iter().find(|q| q.subscription == sub)
                    && matches!(state(q), State::Fresh | State::Stale)
                    && let Some(found) = limiting_cap(q, &c.model, min, now)
                    && cap.as_ref().is_none_or(|(have, _)| found.0 < *have)
                {
                    cap = Some(found);
                }
            }
            if let Some((_, label)) = cap {
                let phrase = format!("; {label} under the minimum");
                assert!(
                    reason.contains(&phrase),
                    "parked unspecified model did not name {label}: {reason}"
                );
            }
        }
    }

    // Spare trigger: nothing while high-priority work is queued; fresh only; above threshold; sorted.
    let threshold = f64::from(b.u8()) * 100.0 / 255.0;
    assert!(spare_trigger(now, &quotas, threshold, true).is_empty());
    let spare = spare_trigger(now, &quotas, threshold, false);
    for s in &spare {
        assert!(s.unused_pct > threshold && s.unused_pct <= 100.0);
        assert!(
            quotas
                .iter()
                .any(|q| q.subscription == s.subscription && state(q) == State::Fresh)
        );
    }
    assert!(spare.windows(2).all(|w| w[0].unused_pct >= w[1].unused_pct));
    assert!(spare_trigger(now, &unfresh, threshold, false).is_empty());

    // Task size: the owner's class is final; otherwise Envoy moves it at most one class.
    let history: Vec<u64> = (0..b.below(12)).map(|_| u64::from(b.u32())).collect();
    let th = Thresholds {
        small_max: u64::from(b.u32()),
        large_min: u64::from(b.u32()),
    };
    let n = b.below(10);
    let nudge = b.u8() as i8;
    let owner = [
        None,
        Some(TaskSize::Small),
        Some(TaskSize::Medium),
        Some(TaskSize::Large),
    ][b.below(4)];
    let got = derive(&history, n, th, owner, nudge);
    if let Some(o) = owner {
        assert_eq!(got, o);
    } else {
        let base = derive(&history, n, th, None, 0);
        assert!(got.max(base) as i8 - got.min(base) as i8 <= 1);
        if nudge == 0 {
            assert_eq!(got, base);
        }
    }
});

fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// SPEC 8. No model, or the model `default`.
fn unspecified(model: &str) -> bool {
    let folded = fold(model);
    folded.is_empty() || folded == "default"
}

/// SPEC 8. `opusplan` is both Opus and Sonnet. Otherwise each contained family counts.
fn families(s: &str) -> Vec<&'static str> {
    let folded = fold(s);
    if folded.contains("opusplan") {
        return vec!["opus", "sonnet"];
    }
    ["sonnet", "opus", "haiku"]
        .into_iter()
        .filter(|name| folded.contains(name))
        .collect()
}

/// SPEC 8. An account window always applies: anything that is not `wk` plus a name, including
/// bare `wk`. A cap applies to the same family (opus, sonnet, haiku), ignoring case and
/// version, so `Sonnet 4.5` applies to `sonnet` and `claude-sonnet-4-5`. `opusplan` applies
/// to both Opus and Sonnet. No model and `default` are not gated by a cap. A scope that is
/// not one of those families still applies when its name is at least three letters and the
/// model name contains it.
fn applies(label: &str, model: &str) -> bool {
    let Some(rest) = label.strip_prefix("wk ") else {
        return true;
    };
    if rest.is_empty() || unspecified(model) {
        return rest.is_empty();
    }
    let caps = families(rest);
    if caps.is_empty() {
        let name = fold(rest);
        return name.len() >= 3 && fold(model).contains(&name);
    }
    let got = families(model);
    caps.iter().any(|cap| got.contains(cap))
}

/// SPEC 8. Tightest `wk <name>` cap under `min` on an unspecified model. An open cap does
/// not count, and neither does one whose reset has passed. The chosen or parked reason
/// names it as `; <label> under the minimum`.
fn limiting_cap(q: &Quota, model: &str, min: f64, now_ms: u64) -> Option<(f64, String)> {
    if !unspecified(model) {
        return None;
    }
    let mut best: Option<(f64, String)> = None;
    for w in &q.windows {
        let Some(scope) = w.label.strip_prefix("wk ") else {
            continue;
        };
        if scope.is_empty() {
            continue;
        }
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            continue;
        };
        if w.resets_at_ms
            .map(|r| r.0)
            .filter(|t| *t > 0)
            .is_some_and(|r| r <= now_ms)
        {
            continue;
        }
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        if remaining >= min {
            continue;
        }
        if best.as_ref().is_none_or(|(have, _)| remaining < *have) {
            best = Some((remaining, w.label.clone()));
        }
    }
    best
}

fn map_windows(quotas: &[Quota], mut f: impl FnMut(&mut QuotaWindow)) -> Vec<Quota> {
    let mut out = quotas.to_vec();
    for q in &mut out {
        for w in &mut q.windows {
            f(w);
        }
    }
    out
}

/// SPEC 8. A row whose only finite windows are caps for other models is no reading
/// for this candidate.
fn only_other_scopes(q: &Quota, model: &str) -> bool {
    let mut saw = false;
    for w in &q.windows {
        if !w.used_pct.is_some_and(|u| u.is_finite()) {
            continue;
        }
        saw = true;
        if applies(&w.label, model) {
            return false;
        }
    }
    saw
}

fn metered(h: Harness) -> Option<Subscription> {
    match h {
        Harness::Claude => Some(Subscription::Claude),
        Harness::Codex => Some(Subscription::Codex),
        Harness::Grok => Some(Subscription::Grok),
        _ => None,
    }
}

/// SPEC 8. Park is wrong when this candidate may be spent with no spare.
///
/// No row, a reading that was never a window for this model, or one whose gating
/// windows have reset. `pi_vendor` and Antigravity are unmetered, not a missing
/// reading. A local row that was never read may be spent; a stale or failed one
/// may not.
fn unknown_or_reset(c: &Candidate, quotas: &[Quota], now: u64, min: f64) -> bool {
    if matches!(c.harness, Harness::PiVendor | Harness::Antigravity) {
        return false;
    }
    if c.harness == Harness::PiLocal {
        return quotas
            .iter()
            .find(|q| q.subscription == Subscription::Local)
            .map(|q| state(q) == State::Unknown)
            .unwrap_or(true);
    }
    let Some(sub) = metered(c.harness) else {
        return false;
    };
    let Some(q) = quotas.iter().find(|q| q.subscription == sub) else {
        return true;
    };
    match state(q) {
        State::Unknown => true,
        State::Error => false,
        State::Fresh | State::Stale => {
            if only_other_scopes(q, &c.model) {
                return true;
            }
            if still_enforcing(q, now, min, &c.model) {
                return false;
            }
            // A window that still counts is spare or exhausted, not unknown or reset.
            if open_usable(q, now, &c.model, min).is_some() {
                return false;
            }
            true
        }
    }
}

/// SPEC 8. Stored length when it is above zero, otherwise the length the label
/// names, otherwise five hours.
fn bound_ms(w: &QuotaWindow) -> u64 {
    const FIVE_HOURS_MS: u64 = 5 * 3_600_000;
    if let Some(stored) = w.length_ms
        && stored > 0
    {
        return stored;
    }
    window_length_ms(&w.label).unwrap_or(FIVE_HOURS_MS)
}

/// Tightest remaining percent of windows that have not reset. `None` when nothing scores.
fn open_usable(q: &Quota, now_ms: u64, model: &str, min: f64) -> Option<f64> {
    let mut usable = None;
    for w in &q.windows {
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            continue;
        };
        if !applies(&w.label, model) {
            continue;
        }
        // SPEC 8. A stale exhausted window with no reset stops blocking once the row is older
        // than that window's length, or five hours when the length is unknown.
        if state(q) == State::Stale
            && w.resets_at_ms.map(|r| r.0).filter(|t| *t > 0).is_none()
            && (100.0 - used).clamp(0.0, 100.0) < min
            && now_ms.saturating_sub(q.fetched_at_ms.0) >= bound_ms(w)
        {
            continue;
        }
        // A reset at or before the epoch is not a reset. A later stamp that has passed is.
        if w.resets_at_ms
            .map(|r| r.0)
            .filter(|t| *t > 0)
            .is_some_and(|r| r <= now_ms)
        {
            continue;
        }
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        usable = Some(usable.map_or(remaining, |m: f64| m.min(remaining)));
    }
    usable
}

/// SPEC 8. An exhausted window whose reset has passed still blocks when the row was fetched
/// before that reset and now is within ten minutes after it, or when the reset was already
/// behind at the fetch by less than ten minutes and now is within ten minutes after the
/// fetch. A stamp at or before the epoch is not a reset. One already long past at fetch
/// time counts as reset.
fn still_enforcing(q: &Quota, now_ms: u64, min: f64, model: &str) -> bool {
    const TEN_MINUTES_MS: u64 = 10 * 60 * 1000;
    let fetched = q.fetched_at_ms.0;
    q.windows.iter().any(|w| {
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            return false;
        };
        if !applies(&w.label, model) {
            return false;
        }
        if (100.0 - used).clamp(0.0, 100.0) >= min {
            return false;
        }
        let Some(stamp) = w.resets_at_ms.map(|r| r.0).filter(|t| *t > 0) else {
            return false;
        };
        if stamp > now_ms {
            return false;
        }
        if fetched < stamp {
            now_ms < stamp.saturating_add(TEN_MINUTES_MS)
        } else {
            let behind = fetched.saturating_sub(stamp);
            behind < TEN_MINUTES_MS && now_ms < fetched.saturating_add(TEN_MINUTES_MS)
        }
    })
}
