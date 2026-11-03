//! The background threads of the subsystems whose state the hub holds: the fleet start, doctor,
//! harness updates, quota reads and history ingest. Each holds the hub weakly and ends once it
//! is gone; what one round does is the subsystem's own.

use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{EventKind as Kind, Level, Subscription};
use pt_core::{EventKind, Harness, Life, ParkReason};
use pt_doctor::Report;
use pt_fleet::{Fleet, PollConfig};
use pt_quota::{Cache, Options, Reader};

use crate::envoy::{ENVOY, envoy_id};
use crate::exec::program;
use crate::fleet::{self, FleetFlags, Net, Up};
use crate::hub::Hub;
use crate::log::{info, warning};
use crate::logins;

const DOCTOR_EVERY: Duration = Duration::from_secs(6 * 3600);
/// The fewest seconds between two renewals of an idle-expired grok login.
const GROK_RENEW_EVERY: Duration = Duration::from_secs(600);
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
    let opts = Options::default();
    #[cfg(debug_assertions)]
    let opts = match std::env::var("PT_TEST_QUOTA_STALE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(ms) => Options {
            stale_after: Duration::from_millis(ms),
            ..opts
        },
        None => opts,
    };
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
            let mut renewed: Option<Instant> = None;
            loop {
                let Some(hub) = weak.upgrade() else { return };
                let now = pt_quota::now_ms();
                cache.refresh(now, false);
                if cache.renewable().contains(&Subscription::Grok)
                    && renewed.is_none_or(|t| t.elapsed() >= GROK_RENEW_EVERY)
                    && let Some(grok) = program(&hub, Harness::Grok)
                {
                    renewed = Some(Instant::now());
                    match logins::renew_grok(&hub.home.logins(), &grok) {
                        Ok(()) => {
                            info!("quota: renewed the idle grok login with `grok models`");
                            cache.reread(Subscription::Grok);
                            cache.refresh(pt_quota::now_ms(), false);
                        }
                        Err(e) => warning!("quota: renewing the idle grok login: {e}"),
                    }
                }
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

/// Loads the cached report, then runs the checks at once and every 6 h (or when asked).
pub fn start_doctor(hub: &Arc<Hub>, net: Net) {
    hub.doctor.load(&hub.home, net);
    let weak = Arc::downgrade(hub);
    let spawned = thread::Builder::new()
        .name("pt-doctor".into())
        .spawn(move || {
            loop {
                let Some(hub) = weak.upgrade() else { return };
                run_doctor(&hub);
                hub.doctor.wait_asked(DOCTOR_EVERY);
            }
        });
    if let Err(e) = spawned {
        warning!("doctor: starting its thread: {e}");
    }
}

/// Runs doctor now, as its thread does.
pub fn run_doctor(hub: &Hub) -> Arc<Report> {
    hub.doctor
        .run(&hub.home, &hub.owner_home, &hub.config, &hub.bus, |r| {
            unpark_envoy(hub, r)
        })
}

/// Envoy parked by the gate starts again once its harness is eligible.
fn unpark_envoy(hub: &Hub, report: &Report) {
    let view = hub.view();
    let Some(id) = envoy_id(&view.state) else {
        return;
    };
    let Some(Life::Parked(ParkReason::Ineligible { harness })) =
        view.state.sleeve(id).map(|s| s.life)
    else {
        return;
    };
    let name = harness.as_str();
    if !report.eligible(name) {
        return;
    }
    hub.bus.event(
        Kind::Sleeve,
        Level::Info,
        ENVOY,
        "owner",
        &format!("doctor: {name} is eligible again; starting Envoy"),
    );
    hub.submit(EventKind::Start {
        sleeve: id,
        target: None,
        effort: None,
    });
}

/// Starts the harness update thread. With `harness_update_hours` 0 it runs only what is asked
/// for.
pub fn start_harness_updates(hub: &Arc<Hub>) {
    hub.harness_updates.load(&hub.home);
    let every = Duration::from_secs(hub.config.harness_update_hours.saturating_mul(3600));
    let weak = Arc::downgrade(hub);
    let spawned = thread::Builder::new()
        .name("pt-harness-update".into())
        .spawn(move || {
            loop {
                let Some(hub) = weak.upgrade() else { return };
                let u = &hub.harness_updates;
                let nap = u.round(&hub.doctor, &hub.bus, &hub.home, || hub.view(), every);
                u.nap(nap);
            }
        });
    if let Err(e) = spawned {
        warning!("harness update: starting its thread: {e}");
    }
}
