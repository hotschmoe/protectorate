//! `doctor` in the hub (SPEC 7, D33): pt-doctor runs at start and every 6 h on its own thread,
//! and its report is cached in `<home>/doctor.json`. The report gates every spawn, start and
//! resleeve: the API refuses an ineligible harness up front, the executor refuses to launch one,
//! and resleeve skips it. Envoy whose own harness is refused parks (one owner notice, no resleeve
//! loop) and starts again once a later run finds that harness eligible.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pt_api::{EventKind as Kind, Level};
use pt_core::Harness;
use pt_doctor::Report;

use crate::bus::Bus;
use crate::envoy::ENVOY;
use crate::fleet::Net;
use crate::home::{Config, Home, now_ms};
use crate::lock::lock;
use crate::log::{info, warning};

/// How long a launch waits for the first report of a hub that has none cached.
const FIRST_WAIT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Doctor {
    report: Mutex<Option<Arc<Report>>>,
    changed: Condvar,
    /// Held for the length of one run.
    running: Mutex<()>,
    refresh: Mutex<bool>,
    wake: Condvar,
    net: Mutex<Option<Net>>,
    /// The embedded headscale and member, unpacked once per process: hashing them on every
    /// run would cost seconds.
    unpacked: Mutex<Option<(Option<PathBuf>, Option<PathBuf>)>>,
}

impl Doctor {
    pub fn report(&self) -> Option<Arc<Report>> {
        lock(&self.report).clone()
    }

    fn set(&self, r: Arc<Report>) {
        *lock(&self.report) = Some(r);
        self.changed.notify_all();
    }

    /// The report, waiting up to `timeout` for a first one.
    pub fn wait_report(&self, timeout: Duration) -> Option<Arc<Report>> {
        let deadline = Instant::now() + timeout;
        let mut r = lock(&self.report);
        while r.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            r = self
                .changed
                .wait_timeout(r, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        r.clone()
    }

    /// Keeps `net` for the runs and loads the cached report.
    pub fn load(&self, home: &Home, net: Net) {
        *lock(&self.net) = Some(net);
        match Report::load(&home.doctor()) {
            Ok(r) => self.set(Arc::new(r)),
            Err(e) if home.doctor().exists() => warning!("doctor: ignoring the cache: {e}"),
            Err(_) => {}
        }
    }

    /// Runs every check now (a few seconds), saves `doctor.json` and announces what changed.
    /// `after` runs before this run ends, so it never overlaps another run.
    pub fn run(
        &self,
        home: &Home,
        owner_home: &Path,
        config: &Config,
        bus: &Bus,
        after: impl FnOnce(&Report),
    ) -> Arc<Report> {
        let _one = lock(&self.running);
        let net = *lock(&self.net);
        let net = net.unwrap_or(Net {
            headscale: None,
            member: None,
            missing: "",
        });
        let report = Arc::new(pt_doctor::run(&self.checks(home, owner_home, config, &net)));
        if let Err(e) = report.save(&home.doctor()) {
            warning!("doctor: {e}");
        }
        let before = self.report();
        self.set(report.clone());
        announce(bus, before.as_deref(), &report);
        after(&report);
        report
    }

    /// Asks the doctor thread for a run now.
    pub fn refresh(&self) {
        *lock(&self.refresh) = true;
        self.wake.notify_all();
    }

    /// Waits until a run is asked for or `timeout` passes, and takes the ask.
    pub fn wait_asked(&self, timeout: Duration) {
        let mut asked = lock(&self.refresh);
        let deadline = Instant::now() + timeout;
        while !*asked {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            asked = self
                .wake
                .wait_timeout(asked, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        *asked = false;
    }

    /// Why `h` may not be used, from the report as it stands; a hub with no report yet allows a
    /// harness its config does not rule out.
    pub fn check(&self, config: &Config, h: Harness) -> Result<(), String> {
        match self.report() {
            Some(r) => verdict(&r, h),
            None => match unconfigured(config, h) {
                Some(why) => Err(format!("harness {} is {why}", h.as_str())),
                None => Ok(()),
            },
        }
    }

    /// The executor's gate before a launch. It waits for the first report of a hub with none
    /// cached, so nothing launches before doctor has spoken once.
    pub fn gate_launch(&self, bus: &Bus, name: &str, h: Harness) -> Result<(), String> {
        let res = match self.wait_report(FIRST_WAIT) {
            Some(r) => verdict(&r, h),
            None => Ok(()),
        };
        if let Err(why) = &res {
            let what = if name == ENVOY {
                "Envoy parks until it is fixed"
            } else {
                "it resleeves to an eligible harness or parks"
            };
            bus.event(
                Kind::Sleeve,
                Level::Alarm,
                name,
                "owner",
                &format!("launch refused: {why}; {what}"),
            );
        }
        res
    }

    /// The checks as the hub runs them: its own harness logins, the owner's PATH, the harness
    /// programs the config launches, and the headscale and member this binary embeds.
    fn checks(&self, home: &Home, owner_home: &Path, c: &Config, net: &Net) -> pt_doctor::Config {
        let mut cfg = pt_doctor::Config::from_env();
        cfg.home = home.logins();
        cfg.disk_paths = vec![home.root.clone(), owner_home.to_path_buf()];
        for (name, p) in [
            ("claude", &c.claude_program),
            ("codex", &c.codex_program),
            ("grok", &c.grok_program),
            ("pi", &c.pi_program),
        ] {
            if p.contains('/') {
                cfg.programs.push((name.to_string(), PathBuf::from(p)));
            } else if p != name
                && let Some(found) = std::env::var_os("PATH").and_then(|path| {
                    std::env::split_paths(&path)
                        .map(|d| d.join(p))
                        .find(|f| f.is_file())
                })
            {
                cfg.programs.push((name.to_string(), found));
            }
        }
        if let Some(why) = unconfigured(c, Harness::PiLocal) {
            cfg.unconfigured
                .push((Harness::PiLocal.as_str().to_string(), why.to_string()));
        }
        let (headscale, member) = lock(&self.unpacked)
            .get_or_insert_with(|| {
                let dir = home.net();
                (
                    net.headscale.and_then(|e| e.unpack(&dir).ok()),
                    net.member.and_then(|e| e.unpack(&dir).ok()),
                )
            })
            .clone();
        cfg.headscale = headscale;
        cfg.member = member;
        cfg
    }
}

fn announce(bus: &Bus, before: Option<&Report>, now: &Report) {
    let old: &[String] = before.map_or(&[], |b| &b.problems);
    for p in now.problems.iter().filter(|p| !old.contains(p)) {
        bus.event(
            Kind::Hub,
            Level::Warn,
            "doctor",
            "owner",
            &format!("doctor: {p}"),
        );
    }
    for p in old.iter().filter(|p| !now.problems.contains(p)) {
        bus.event(
            Kind::Hub,
            Level::Info,
            "doctor",
            "owner",
            &format!("doctor: resolved: {p}"),
        );
    }
    info!(
        "doctor: {} problems, ineligible: {}",
        now.problems.len(),
        if now.ineligible.is_empty() {
            "none".to_string()
        } else {
            now.ineligible.join(", ")
        }
    );
}

/// Why the config alone rules `h` out: the local model needs a provider and a model.
fn unconfigured(c: &Config, h: Harness) -> Option<&'static str> {
    (h == Harness::PiLocal && (c.pi_local_provider.is_empty() || c.pi_local_model.is_empty()))
        .then_some(
            "not configured: set pi_local_provider (a provider in Pi's models.json) and \
         pi_local_model in config.json, then restart the hub",
        )
}

fn verdict(r: &Report, h: Harness) -> Result<(), String> {
    let name = h.as_str();
    let Some(entry) = r.harnesses.iter().find(|x| x.name == name) else {
        return Err(format!("doctor does not know the harness {name}"));
    };
    if entry.eligible {
        return Ok(());
    }
    Err(format!(
        "harness {name} is ineligible ({}; doctor ran {} s ago, `protectorate doctor` shows it)",
        entry.reasons.join("; "),
        now_ms().saturating_sub(r.checked_ms) / 1000
    ))
}
