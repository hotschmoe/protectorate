//! Harness ranking: which sleeve harness has spare subscription capacity that
//! would otherwise expire unused at a reset. Only Fresh data counts.
use crate::cache::{Freshness, ProviderQuota};
use crate::fmt_age;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Duration;

/// Spare capacity below which pi on the local LLM outranks a subscription.
const LOCAL_SCORE: f64 = 1.0;
/// Stands in for a window whose reset time is unknown.
const UNKNOWN_RESET_HOURS: f64 = 168.0;
/// The time to reset at which a window's weight halves.
const URGENCY_HALF_HOURS: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Grok,
    /// pi, backed by the local LLM.
    Pi,
}

impl Harness {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TaskSize {
    fn min_usable(&self) -> f64 {
        match self {
            Self::Small => 5.0,
            Self::Large => 30.0,
            Self::Medium => 15.0,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Large => "large",
            Self::Medium => "medium",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct HarnessSuggestion {
    /// 1 is the best candidate.
    pub rank: i32,
    pub harness: Harness,
    /// The quota subscription behind the harness, "local" for pi.
    pub provider: String,
    /// False when the data is not Fresh, or usable capacity is below what the
    /// task size needs.
    pub recommended: bool,
    /// Orders the recommended candidates; 0 when not recommended.
    pub score: f64,
    pub freshness: Freshness,
    /// Remaining capacity of the tightest window: the most that can be spent now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usable_percent: Option<f64>,
    /// The part of usable_percent expected to be unused at the scoring window's reset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiring_percent: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub window: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_in: Option<u64>,
    pub reason: String,
}

struct WindowScore {
    name: String,
    expiring: f64,
    hours: f64,
    known: bool,
    estimated: bool,
    score: f64,
}

/// Scores each harness from the provider quotas, best first.
pub fn rank_harnesses(
    quotas: &[ProviderQuota],
    size: TaskSize,
    now: DateTime<Utc>,
) -> Vec<HarnessSuggestion> {
    let mut by_provider: Vec<&ProviderQuota> = Vec::new();
    for q in quotas {
        match by_provider.iter().position(|p| p.provider == q.provider) {
            Some(i) => {
                let prev = by_provider[i];
                if prev.freshness != Freshness::Fresh && q.freshness == Freshness::Fresh {
                    by_provider[i] = q;
                }
            }
            None => by_provider.push(q),
        }
    }
    let find = |name: &str| by_provider.iter().copied().find(|q| q.provider == name);

    let mut out: Vec<HarnessSuggestion> = Vec::new();
    for (h, provider) in [
        (Harness::Claude, "claude"),
        (Harness::Codex, "codex"),
        (Harness::Grok, "grok"),
    ] {
        let mut s = HarnessSuggestion {
            rank: 0,
            harness: h,
            provider: provider.to_string(),
            recommended: false,
            score: 0.0,
            freshness: Freshness::Unknown,
            usable_percent: None,
            expiring_percent: None,
            window: String::new(),
            resets_in: None,
            reason: "not monitored: no quota reader for this provider".to_string(),
        };
        if let Some(q) = find(provider) {
            score_provider(&mut s, q, size, now);
        }
        out.push(s);
    }
    out.push(local_suggestion(find("local")));

    out.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then(
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.harness.cmp(&b.harness))
    });
    for (i, s) in out.iter_mut().enumerate() {
        s.rank = i as i32 + 1;
    }
    out
}

fn local_suggestion(q: Option<&ProviderQuota>) -> HarnessSuggestion {
    let mut s = HarnessSuggestion {
        rank: 0,
        harness: Harness::Pi,
        provider: "local".to_string(),
        recommended: true,
        score: LOCAL_SCORE,
        freshness: Freshness::Unknown,
        usable_percent: None,
        expiring_percent: None,
        window: String::new(),
        resets_in: None,
        reason: String::new(),
    };
    match q {
        None => {
            s.reason =
                "local LLM via pi: no quota, always available fallback (no local endpoint monitored)"
                    .to_string();
        }
        Some(q) if q.freshness == Freshness::Fresh => {
            s.freshness = q.freshness;
            s.reason = "local LLM via pi: no quota, reachable".to_string();
            if !q.models.is_empty() {
                s.reason += &format!(" ({})", q.models.join(", "));
            }
        }
        Some(q) => {
            s.freshness = q.freshness;
            s.reason = "local LLM via pi: no quota, always available fallback (endpoint not confirmed reachable)".to_string();
        }
    }
    s
}

fn score_provider(
    s: &mut HarnessSuggestion,
    q: &ProviderQuota,
    size: TaskSize,
    now: DateTime<Utc>,
) {
    s.freshness = q.freshness;
    match q.freshness {
        Freshness::Fresh => {}
        Freshness::Stale => {
            let age = q.age.unwrap_or_default();
            s.reason = format!(
                "quota data is stale ({} old); not counted as spare",
                fmt_age(age)
            );
            return;
        }
        Freshness::Unknown => {
            s.reason = "no quota data yet; not counted as spare".to_string();
            if !q.error.is_empty() {
                s.reason += &format!(" ({})", q.error);
            }
            return;
        }
    }

    let mut cands: Vec<WindowScore> = Vec::new();
    let mut usable = 100.0f64;
    let mut tight = String::new();
    for w in &q.windows {
        if let Some(r) = w.resets_at {
            if r <= now {
                continue;
            }
        }
        let remaining = (100.0 - w.used_percent).clamp(0.0, 100.0);
        if tight.is_empty() || remaining < usable {
            usable = remaining;
            tight = w.name.clone();
        }
        let mut ws = WindowScore {
            name: w.name.clone(),
            expiring: remaining,
            hours: UNKNOWN_RESET_HOURS,
            known: false,
            estimated: false,
            score: 0.0,
        };
        if let Some(e) = w.estimated_unused_percent_at_reset {
            ws.expiring = e.min(remaining);
            ws.estimated = true;
        }
        if let Some(r) = w.resets_at {
            ws.hours = (r - now).num_minutes() as f64 / 60.0;
            ws.known = true;
        }
        cands.push(ws);
    }
    if cands.is_empty() {
        s.reason =
            "no usable windows reported (none, or all already reset); wait for the next quota read"
                .to_string();
        return;
    }

    let mut best_idx = 0usize;
    let mut best_score = f64::NEG_INFINITY;
    for (i, c) in cands.iter_mut().enumerate() {
        c.expiring = c.expiring.min(usable);
        c.score = c.expiring / (1.0 + c.hours / URGENCY_HALF_HOURS);
        if i == 0 || c.score > best_score {
            best_score = c.score;
            best_idx = i;
        }
    }
    let best = &cands[best_idx];
    s.usable_percent = Some(usable);
    s.expiring_percent = Some(best.expiring);
    s.window = best.name.clone();
    if best.known {
        s.resets_in = Some((best.hours * 3600.0) as u64);
    }

    let need = size.min_usable();
    if usable < need {
        s.reason = format!(
            "only {:.0}% usable now ({} window), below the {:.0}% a {} task needs",
            usable,
            tight,
            need,
            size.name()
        );
        return;
    }
    s.recommended = true;
    s.score = best.score;
    let basis = if best.estimated {
        "projected"
    } else {
        "remaining, no pace estimate"
    };
    let reset = if best.known {
        format!(
            "resets in {}",
            fmt_age(Duration::from_secs_f64(best.hours * 3600.0))
        )
    } else {
        "reset time unknown".to_string()
    };
    s.reason = format!(
        "{} window {}, ~{:.0}% expiring unused ({}); {:.0}% usable now ({})",
        best.name, reset, best.expiring, basis, usable, tight
    );
}
