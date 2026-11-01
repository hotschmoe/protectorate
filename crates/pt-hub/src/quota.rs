//! Quota readings (SPEC 8): a `pt_quota::Cache` over the readers of the installed harnesses,
//! refreshed on a background thread every `quota_poll_secs`. The API, the snapshot and
//! `SelectTarget` read the last values; a changed reading goes out on the event stream.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use pt_api::{self as api, HubStreamMessage};
use pt_core::Harness;
use pt_quota::{Cache, Options, Reader};

use crate::exec::program;
use crate::hub::Hub;
use crate::log::{info, warning};
use crate::transcript::lock;

#[derive(Default)]
pub struct Quotas {
    last: Mutex<Vec<api::Quota>>,
}

impl Quotas {
    pub fn list(&self) -> Vec<api::Quota> {
        lock(&self.last).clone()
    }
}

/// A reader per harness whose program is installed; the local model also when `local_url` is
/// set. The readers look at the owner's login files, never the sleeves' seeded copies; the local
/// one sends the key of Pi's `pi_local_provider` and falls back to its `baseUrl`.
fn readers(hub: &Hub) -> Vec<Box<dyn Reader>> {
    let home = Some(hub.owner_home.clone());
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
        }));
    }
    out
}

/// Publishes the unread rows at once, then refreshes in the background until the hub is gone.
/// `quota_poll_secs` 0 never reads: every row stays `unknown`.
pub fn start(hub: &Arc<Hub>) {
    let mut cache = Cache::new(readers(hub), Options::default());
    publish(hub, cache.quotas(pt_quota::now_ms()));
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
                publish(&hub, cache.quotas(now));
                drop(hub);
                thread::sleep(Duration::from_secs(every));
            }
        });
    if let Err(e) = started {
        warning!("quota: starting the reader thread: {e}");
    }
}

fn publish(hub: &Hub, now: Vec<api::Quota>) {
    let mut last = lock(&hub.quotas.last);
    for q in &now {
        if !last.contains(q) {
            let quota = q.clone();
            hub.bus.push(|seq| HubStreamMessage::Quota { seq, quota });
        }
    }
    *last = now;
}
