//! The background threads of the subsystems whose state the hub holds: the fleet start, quota
//! reads and history ingest. Each holds the hub weakly and ends once it is gone; what one
//! round does is the subsystem's own.

use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;

use pt_core::Harness;
use pt_fleet::{Fleet, PollConfig};
use pt_quota::{Cache, Options, Reader};

use crate::exec::program;
use crate::fleet::{self, FleetFlags, Net, Up};
use crate::hub::Hub;
use crate::log::{info, warning};

/// How often the history ingest catches up.
const HISTORY_POLL: Duration = Duration::from_secs(1);
/// Longest wait after a failed history commit before the next catch-up.
const HISTORY_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Starts the fleet in the background when one is configured. A configured fleet that cannot
/// start (no embedded binaries, a bad config, headscale failing) leaves the hub running with
/// node and job methods answering `unavailable` and the reason.
pub fn start_fleet(hub: &Arc<Hub>, flags: FleetFlags, net: Net) {
    let cfg = match fleet::config(&hub.config, &hub.home, &flags) {
        Ok(None) => return info!("fleet: not configured"),
        Ok(Some(c)) => c,
        Err(e) => {
            warning!("fleet: {e}");
            return hub.fleet.down(e);
        }
    };
    hub.fleet.starting();
    let weak = Arc::downgrade(hub);
    let started = thread::Builder::new()
        .name("pt-fleet-start".into())
        .spawn(move || {
            let Some(hub) = weak.upgrade() else { return };
            match run_fleet(&hub, cfg, &net) {
                Ok(up) => hub.fleet.started(up),
                Err(e) => {
                    warning!("fleet: {e}");
                    hub.fleet.down(e);
                }
            }
        });
    if let Err(e) = started {
        hub.fleet.down(format!("starting the fleet thread: {e}"));
    }
}

fn run_fleet(hub: &Arc<Hub>, mut cfg: pt_fleet::Config, net: &Net) -> Result<Up, String> {
    let (headscale, member) = fleet::unpack(&hub.home, net)?;
    info!(
        "fleet: headscale {} and member {}",
        headscale.display(),
        member.display()
    );
    cfg.headscale = headscale;
    cfg.member = member;
    let (dir, url) = (cfg.dir.clone(), cfg.server_url.clone());
    let fleet = Fleet::start(cfg).map_err(|e| e.to_string())?;
    let fp = fleet.control_fingerprint().map_err(|e| e.to_string())?;
    info!("fleet up: {} control {url} key {fp}", dir.display());
    let every = Duration::from_secs(hub.config.fleet.poll_secs.max(1));
    let poll = PollConfig {
        presence_every: every,
        list_every: every,
        ..PollConfig::default()
    };
    let weak: Weak<Hub> = Arc::downgrade(hub);
    let poller = fleet
        .poll(poll, move |e| {
            if let Some(hub) = weak.upgrade() {
                hub.fleet.published(&hub.bus, e);
            }
        })
        .map_err(|e| {
            fleet.shutdown();
            e.to_string()
        })?;
    Ok(Up {
        fleet,
        poller: Arc::new(poller),
    })
}

/// A reader per harness whose program is installed; the local model also when `local_url` is
/// set. The readers look at the hub's own logins, the ones its sleeves use; the local one sends
/// the key of Pi's `pi_local_provider` and falls back to its `baseUrl`.
fn quota_readers(hub: &Hub) -> Vec<Box<dyn Reader>> {
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
pub fn start_quotas(hub: &Arc<Hub>) {
    let mut opts = Options::default();
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("PT_TEST_QUOTA_STALE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        opts.stale_after = Duration::from_millis(ms);
    }
    let mut cache = Cache::new(quota_readers(hub), opts);
    hub.quotas
        .publish(&hub.bus, cache.quotas_with_age(), opts.stale_after);
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
                hub.quotas
                    .publish(&hub.bus, cache.quotas_with_age(), opts.stale_after);
                drop(hub);
                thread::sleep(Duration::from_secs(every));
            }
        });
    if let Err(e) = started {
        warning!("quota: starting the reader thread: {e}");
    }
}

/// The ingest thread: a catch-up pass about once a second until the hub is gone.
pub fn start_history(hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    let started = thread::Builder::new()
        .name("pt-history".into())
        .spawn(move || {
            let mut pause = HISTORY_POLL;
            loop {
                let Some(hub) = weak.upgrade() else { return };
                let failed = hub.history.catch_up(&hub.home, || hub.view(), None);
                drop(hub);
                thread::sleep(pause);
                pause = if failed {
                    pause.saturating_mul(2).min(HISTORY_BACKOFF_MAX)
                } else {
                    HISTORY_POLL
                };
            }
        });
    if let Err(e) = started {
        warning!("history: starting the ingest thread: {e}");
    }
}
