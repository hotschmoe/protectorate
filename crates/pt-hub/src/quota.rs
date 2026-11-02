//! Quota readings (SPEC 8): a `pt_quota::Cache` over the readers of the installed harnesses,
//! refreshed on a background thread every `quota_poll_secs`. The API, the snapshot and
//! `SelectTarget` read the last values; a changed reading goes out on the event stream.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, HubStreamMessage};
use pt_core::Harness;
use pt_quota::{Cache, Options, Reader};

use crate::exec::program;
use crate::hub::Hub;
use crate::lock::lock;
use crate::log::{info, warning};

pub struct Quotas {
    last: Mutex<Published>,
}

struct Published {
    rows: Vec<(api::Quota, Option<Duration>)>,
    at: Instant,
    stale_after: Duration,
}

impl Default for Quotas {
    fn default() -> Self {
        Quotas {
            last: Mutex::new(Published {
                rows: Vec::new(),
                at: Instant::now(),
                stale_after: Options::default().stale_after,
            }),
        }
    }
}

impl Quotas {
    pub fn list(&self) -> Vec<api::Quota> {
        let last = lock(&self.last);
        last.rows
            .iter()
            .map(|(q, age)| {
                let mut q = q.clone();
                if q.reading == api::QuotaReading::Ok
                    && age
                        .is_some_and(|age| age.saturating_add(last.at.elapsed()) > last.stale_after)
                {
                    let age = age.unwrap_or_default().saturating_add(last.at.elapsed());
                    q.reading = api::QuotaReading::Stale;
                    let stale = format!(
                        "stale ({} old)",
                        pt_quota::fmt_age(age.as_millis().min(u64::MAX as u128) as u64)
                    );
                    q.note = if q.note.is_empty() {
                        stale
                    } else {
                        format!("{stale}; {}", q.note)
                    };
                }
                q
            })
            .collect()
    }
}

/// A reader per harness whose program is installed; the local model also when `local_url` is
/// set. The readers look at the hub's own logins, the ones its sleeves use; the local one sends
/// the key of Pi's `pi_local_provider` and falls back to its `baseUrl`.
fn readers(hub: &Hub) -> Vec<Box<dyn Reader>> {
    let home = Some(hub.home.logins());
    let installed = |h| program(hub, h).is_some();
    let mut out: Vec<Box<dyn Reader>> = Vec::new();
    if installed(Harness::Claude) {
        out.push(Box::new(pt_quota::claude::Claude {
            home: home.clone(),
            base_url: None,
        }));
    }
    if installed(Harness::Codex) {
        out.push(Box::new(pt_quota::codex::Codex {
            home: home.clone(),
            base_url: None,
        }));
    }
    if installed(Harness::Grok) {
        out.push(Box::new(pt_quota::grok::Grok {
            home: home.clone(),
            base_url: None,
        }));
    }
    if installed(Harness::PiLocal) || !hub.config.local_url.is_empty() {
        out.push(Box::new(pt_quota::local::Local {
            base_url: hub.config.local_url.clone(),
            home,
            provider: hub.config.pi_local_provider.clone(),
            model: hub.config.pi_local_model.clone(),
        }));
    }
    out
}

/// Publishes the unread rows at once, then refreshes in the background until the hub is gone.
/// `quota_poll_secs` 0 never reads: every row stays `unknown`.
pub fn start(hub: &Arc<Hub>) {
    let mut opts = Options::default();
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("PT_TEST_QUOTA_STALE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        opts.stale_after = Duration::from_millis(ms);
    }
    let mut cache = Cache::new(readers(hub), opts);
    publish(hub, cache.quotas_with_age(), opts.stale_after);
    let every = hub.config.quota_poll_secs;
    if every == 0 {
        return info!("quota: reading is off (quota_poll_secs 0)");
    }
    let weak = Arc::downgrade(hub);
    let started = thread::Builder::new()
        .name("pt-quota".into())
        .spawn(move || {
            loop {
                let Some(hub) = weak.upgrade() else { return };
                let now = pt_quota::now_ms();
                cache.refresh(now, false);
                publish(&hub, cache.quotas_with_age(), opts.stale_after);
                drop(hub);
                thread::sleep(Duration::from_secs(every));
            }
        });
    if let Err(e) = started {
        warning!("quota: starting the reader thread: {e}");
    }
}

fn publish(hub: &Hub, now: Vec<(api::Quota, Option<Duration>)>, stale_after: Duration) {
    let mut last = lock(&hub.quotas.last);
    for (q, _) in &now {
        if !last.rows.iter().any(|(old, _)| old == q) {
            let quota = q.clone();
            hub.bus.push(|seq| HubStreamMessage::Quota { seq, quota });
        }
    }
    *last = Published {
        rows: now,
        at: Instant::now(),
        stale_after,
    };
}
