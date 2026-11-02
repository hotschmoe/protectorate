#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_api::{Harness, Mode, Quota, QuotaWindow, Subscription};
use pt_quota::{
    Candidate, Health, State, TaskSize, Thresholds, derive, select_target, select_with_fallback,
    spare_trigger, state,
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
        assert!(
            first_fresh.is_some(),
            "picked a target with no fresh reading: {t:?}"
        );
        assert!(t.score.is_finite() && t.score >= 0.0);
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

    // The fallback may spend a stale row only when its last open windows still clear the
    // minimum. `select_target` itself stays fresh-only, and the fallback agrees with it.
    let fallback = select_with_fallback(now, size, &allow, &quotas);
    match (&got, &fallback) {
        (Some(t), Ok((idx, _))) => {
            let c = &allow[*idx];
            assert_eq!(t.harness, c.harness);
            assert_eq!(t.model, c.model);
        }
        (Some(t), Err(e)) => panic!("fallback dropped {t:?}: {e}"),
        (None, Ok((idx, _))) => {
            let c = &allow[*idx];
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
                assert!(
                    !only_other_scopes(q, &c.model),
                    "a row with only other models' caps was spent"
                );
                if matches!(state(q), State::Fresh | State::Stale) {
                    assert!(
                        !still_enforcing(q, now, size.min_usable_pct(), &c.model),
                        "a passed exhausted window was spent"
                    );
                }
                if state(q) == State::Stale
                    && let Some(usable) = open_usable(q, now, &c.model)
                {
                    assert!(
                        usable + 1e-9 >= size.min_usable_pct(),
                        "stale row at {usable}% usable was spent"
                    );
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
        (None, Err(_)) => {
            for c in &allow {
                if let Some(sub) = metered(c.harness)
                    && let Some(q) = quotas.iter().find(|q| q.subscription == sub)
                    && state(q) == State::Stale
                    && !still_enforcing(q, now, size.min_usable_pct(), &c.model)
                    && let Some(usable) = open_usable(q, now, &c.model)
                    && usable + 1e-9 >= size.min_usable_pct()
                {
                    panic!("stale row at {usable}% usable was parked");
                }
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

/// SPEC 8. An account window always applies: anything that is not `wk` plus a name, including
/// bare `wk` and a `wk` with no name. A name is the letters and digits that follow `wk`. Shorter
/// than three letters it is not a model cap. Otherwise it applies only when the candidate's
/// model contains that name (`opus` applies to `claude-opus-4-6`, not to Sonnet).
fn applies(label: &str, model: &str) -> bool {
    let Some(rest) = label.strip_prefix("wk ") else {
        return true;
    };
    let fold = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    };
    let name = fold(rest);
    if name.is_empty() {
        return true;
    }
    name.len() >= 3 && fold(model).contains(&name)
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

fn only_other_scopes(q: &Quota, model: &str) -> bool {
    let mut saw = false;
    for w in &q.windows {
        if w.used_pct.filter(|u| u.is_finite()).is_none() {
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

/// Tightest remaining percent of windows that have not reset. `None` when nothing scores.
fn open_usable(q: &Quota, now_ms: u64, model: &str) -> Option<f64> {
    let mut usable = None;
    for w in &q.windows {
        if !applies(&w.label, model) {
            continue;
        }
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            continue;
        };
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
        if !applies(&w.label, model) {
            return false;
        }
        let Some(used) = w.used_pct.filter(|u| u.is_finite()) else {
            return false;
        };
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
