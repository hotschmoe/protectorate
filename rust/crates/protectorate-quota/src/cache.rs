//! The polling cache: one last-good snapshot per provider, with freshness.
use crate::{Ctx, Reader, Snapshot, Transport, Window};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct QuotaOptions {
    /// Poll period.
    pub interval: Duration,
    /// Good data older than this is Stale.
    pub stale_after: Duration,
    /// Per-provider floor between forced reads.
    pub min_refresh: Duration,
    /// Per-read bound.
    pub read_timeout: Duration,
}

impl Default for QuotaOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(300),
            stale_after: Duration::from_secs(600),
            min_refresh: Duration::from_secs(30),
            read_timeout: Duration::from_secs(45),
        }
    }
}

impl QuotaOptions {
    fn with_defaults(mut self) -> Self {
        if self.interval.is_zero() {
            self.interval = Duration::from_secs(300);
        }
        if self.stale_after.is_zero() {
            self.stale_after = 2 * self.interval;
        }
        if self.min_refresh.is_zero() {
            self.min_refresh = Duration::from_secs(30);
        }
        if self.read_timeout.is_zero() {
            self.read_timeout = Duration::from_secs(45);
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Freshness {
    /// No read has ever succeeded: no data, not spare capacity.
    Unknown,
    /// The last good read is recent.
    Fresh,
    /// Only an old good read exists; must not be treated as current capacity.
    Stale,
}

#[derive(Clone, Debug, Serialize)]
pub struct QuotaWindow {
    pub name: String,
    pub used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
    /// Window length when the name says so, else 0.
    pub window_seconds: i64,
    /// Estimate of the percent still unused at reset, assuming the pace from
    /// window start to fetch time holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_unused_percent_at_reset: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResetCredit {
    pub kind: String,
    pub count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderQuota {
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub plan: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    pub freshness: Freshness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<Duration>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resets: Vec<ResetCredit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<u64>,
}

#[derive(Clone, Default)]
struct ProvState {
    good: Option<Snapshot>,
    last_attempt: Option<DateTime<Utc>>,
    last_err: String,
}

type UpdateFn = Box<dyn Fn(&ProviderQuota) + Send + Sync>;

pub struct QuotaCache {
    opts: QuotaOptions,
    readers: Vec<Arc<dyn Reader>>,
    read_mu: Vec<Mutex<()>>,
    state: RwLock<Vec<ProvState>>,
    on_update: Mutex<Option<UpdateFn>>,
}

impl QuotaCache {
    pub fn new(readers: Vec<Arc<dyn Reader>>, opts: QuotaOptions) -> Self {
        let n = readers.len();
        Self {
            opts: opts.with_defaults(),
            readers,
            read_mu: (0..n).map(|_| Mutex::new(())).collect(),
            state: RwLock::new(vec![ProvState::default(); n]),
            on_update: Mutex::new(None),
        }
    }

    /// Called after every provider read with the resulting quota view.
    pub fn set_on_update<F>(&self, f: F)
    where
        F: Fn(&ProviderQuota) + Send + Sync + 'static,
    {
        *self.on_update.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    pub fn options(&self) -> &QuotaOptions {
        &self.opts
    }

    /// Polls immediately, then every interval, until `stop` is set.
    pub fn run(&self, http: &dyn Transport, stop: &AtomicBool) {
        self.refresh(&Ctx::now(), http, false);
        while !stop.load(Ordering::Relaxed) {
            let mut left = self.opts.interval;
            while !left.is_zero() && !stop.load(Ordering::Relaxed) {
                let step = left.min(SLEEP_STEP);
                std::thread::sleep(step);
                left -= step;
            }
            if stop.load(Ordering::Relaxed) {
                return;
            }
            self.refresh(&Ctx::now(), http, false);
        }
    }

    /// Reads every provider not read within `min_refresh`.
    pub fn refresh(&self, ctx: &Ctx, http: &dyn Transport, forced: bool) {
        std::thread::scope(|s| {
            for i in 0..self.readers.len() {
                s.spawn(move || self.read_one(ctx, http, i, forced));
            }
        });
    }

    /// Serializes reads per provider, so concurrent forced refreshes collapse
    /// into one read.
    fn read_one(&self, ctx: &Ctx, http: &dyn Transport, i: usize, forced: bool) {
        let _guard = self.read_mu[i].lock().unwrap_or_else(|e| e.into_inner());

        let last = self
            .state
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(i)
            .and_then(|s| s.last_attempt);
        if forced {
            if let Some(l) = last {
                if ctx
                    .now
                    .signed_duration_since(l)
                    .to_std()
                    .unwrap_or_default()
                    < self.opts.min_refresh
                {
                    return;
                }
            }
        }

        let sub = ctx.bounded(self.opts.read_timeout);
        let snap = crate::safe_read(self.readers[i].as_ref(), &sub, http);
        if ctx.expired() {
            return;
        }

        let pq = {
            let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
            let st = &mut state[i];
            st.last_attempt = Some(ctx.now);
            st.last_err = snap.error.clone();
            if snap.error.is_empty() {
                st.good = Some(snap);
            }
            self.provider_quota(i, &state, ctx.now)
        };
        let cb = self.on_update.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cb) = cb.as_ref() {
            cb(&pq);
        }
    }

    /// Every provider's quota with freshness computed at `now`.
    pub fn snapshot(&self, now: DateTime<Utc>) -> Vec<ProviderQuota> {
        let state = self.state.read().unwrap_or_else(|e| e.into_inner());
        (0..self.readers.len())
            .map(|i| self.provider_quota(i, &state, now))
            .collect()
    }

    fn provider_quota(&self, i: usize, state: &[ProvState], now: DateTime<Utc>) -> ProviderQuota {
        let st = &state[i];
        let mut pq = ProviderQuota {
            provider: self.readers[i].provider().to_string(),
            plan: String::new(),
            source: String::new(),
            freshness: Freshness::Unknown,
            fetched_at: None,
            age: None,
            error: st.last_err.clone(),
            checked_at: st.last_attempt,
            windows: Vec::new(),
            resets: Vec::new(),
            models: Vec::new(),
            latency: None,
        };
        let Some(g) = st.good.as_ref() else {
            return pq;
        };
        let age = std::cmp::max(
            now.signed_duration_since(g.fetched_at.unwrap_or(now)),
            chrono::Duration::zero(),
        );
        pq.freshness = Freshness::Fresh;
        if age
            > chrono::Duration::from_std(self.opts.stale_after).unwrap_or(chrono::Duration::zero())
        {
            pq.freshness = Freshness::Stale;
        }
        pq.plan = g.plan.clone();
        pq.source = g.source.clone();
        pq.fetched_at = g.fetched_at;
        pq.age = Some(age.to_std().unwrap_or_default());
        pq.models = g.models.clone();
        pq.latency = g.latency;
        for w in &g.windows {
            let length = window_length(&w.name);
            let est = projected_unused(w, length, g.fetched_at.unwrap_or(now), now);
            pq.windows.push(QuotaWindow {
                name: w.name.clone(),
                used_percent: w.used_percent,
                resets_at: w.resets_at,
                window_seconds: length.as_secs() as i64,
                estimated_unused_percent_at_reset: est,
            });
        }
        for r in &g.resets {
            pq.resets.push(ResetCredit {
                kind: r.kind.clone(),
                count: r.count,
                expires_at: r.expires_at,
            });
        }
        pq
    }
}

const SLEEP_STEP: Duration = Duration::from_millis(100);

/// Parses the trailing token of a window name such as "5h", "7d" or
/// "secondary 7d"; unknown names give 0.
pub fn window_length(name: &str) -> Duration {
    let Some(tok) = name.split_whitespace().last() else {
        return Duration::ZERO;
    };
    if tok == "weekly" {
        return Duration::from_secs(7 * 24 * 3600);
    }
    if tok.len() < 2 {
        return Duration::ZERO;
    }
    let (num, unit) = tok.split_at(tok.len() - 1);
    let n: i64 = match num.parse() {
        Ok(n) if n > 0 => n,
        _ => return Duration::ZERO,
    };
    match unit {
        "h" => Duration::from_secs(n as u64 * 3600),
        "d" => Duration::from_secs(n as u64 * 24 * 3600),
        _ => Duration::ZERO,
    }
}

const MIN_ELAPSED_FRACTION: f64 = 0.05;

/// Extrapolates the pace between window start and the fetch time to the reset
/// instant.
fn projected_unused(
    w: &Window,
    length: Duration,
    fetched: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<f64> {
    if length.is_zero() {
        return None;
    }
    let resets = w.resets_at?;
    if now > resets {
        return None;
    }
    let start = resets - chrono::Duration::from_std(length).unwrap_or(chrono::Duration::zero());
    let elapsed = fetched
        .signed_duration_since(start)
        .to_std()
        .unwrap_or_default();
    let frac = elapsed.as_secs_f64() / length.as_secs_f64();
    if !(MIN_ELAPSED_FRACTION..=1.0).contains(&frac) {
        return None;
    }
    Some((100.0 - w.used_percent / frac).clamp(0.0, 100.0))
}
