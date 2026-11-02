//! `doctor` in the hub (SPEC 7, D33): pt-doctor runs at start and every 6 h on its own thread,
//! and its report is cached in `<home>/doctor.json`. The report gates every spawn, start and
//! resleeve: the API refuses an ineligible harness up front, the executor refuses to launch one,
//! and resleeve skips it. Envoy whose own harness is refused parks (one owner notice, no resleeve
//! loop) and starts again once a later run finds that harness eligible.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, EventKind as Kind, Level};
use pt_core::{EventKind, Harness, Life, ParkReason, SleeveId};
use pt_doctor::Report;
use pt_json::FromJson;

use crate::fleet::Net;
use crate::home::now_ms;
use crate::hub::{ENVOY, Hub};
use crate::log::{info, warning};
use crate::transcript::lock;
use crate::wire;

const EVERY: Duration = Duration::from_secs(6 * 3600);
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
    /// Sleeves whose launch the gate refused, with why; cleared by the next allowed launch.
    gated: Mutex<BTreeMap<SleeveId, String>>,
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

    pub fn gated(&self, id: SleeveId) -> Option<String> {
        lock(&self.gated).get(&id).cloned()
    }
}

/// The checks as the hub runs them: its own harness logins, the owner's PATH, the harness
/// programs the config launches, and the headscale and member this binary embeds.
fn config(hub: &Hub, net: &Net) -> pt_doctor::Config {
    let mut cfg = pt_doctor::Config::from_env();
    cfg.home = hub.home.logins();
    cfg.disk_paths = vec![hub.home.root.clone(), hub.owner_home.clone()];
    let c = &hub.config;
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
    for h in [Harness::PiLocal, Harness::PiDurable] {
        if let Some(why) = unconfigured(hub, h) {
            cfg.unconfigured
                .push((wire::harness_wire(h).to_string(), why.to_string()));
        }
    }
    let (headscale, member) = lock(&hub.doctor.unpacked)
        .get_or_insert_with(|| {
            let dir = hub.home.net();
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

/// Runs every check now (a few seconds), saves `doctor.json` and announces what changed.
pub fn run_now(hub: &Hub) -> Arc<Report> {
    let _one = lock(&hub.doctor.running);
    let net = *lock(&hub.doctor.net);
    let net = net.unwrap_or(Net {
        headscale: None,
        member: None,
        missing: "",
    });
    let report = Arc::new(pt_doctor::run(&config(hub, &net)));
    if let Err(e) = report.save(&hub.home.doctor()) {
        warning!("doctor: {e}");
    }
    let before = hub.doctor.report();
    hub.doctor.set(report.clone());
    announce(hub, before.as_deref(), &report);
    unpark_envoy(hub, &report);
    report
}

fn announce(hub: &Hub, before: Option<&Report>, now: &Report) {
    let old: &[String] = before.map_or(&[], |b| &b.problems);
    for p in now.problems.iter().filter(|p| !old.contains(p)) {
        hub.event(
            Kind::Hub,
            Level::Warn,
            "doctor",
            "owner",
            &format!("doctor: {p}"),
        );
    }
    for p in old.iter().filter(|p| !now.problems.contains(p)) {
        hub.event(
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

/// Envoy parked by the gate starts again once its harness is eligible.
fn unpark_envoy(hub: &Hub, report: &Report) {
    let view = hub.view();
    let Some(id) = Hub::envoy_id(&view.state) else {
        return;
    };
    let Some(s) = view.state.sleeve(id) else {
        return;
    };
    if s.life != Life::Parked(ParkReason::NoTarget) || hub.doctor.gated(id).is_none() {
        return;
    }
    let name = wire::harness_wire(s.target.harness);
    if !report.eligible(name) {
        return;
    }
    lock(&hub.doctor.gated).remove(&id);
    hub.event(
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

/// Loads the cached report, then runs the checks at once and every 6 h (or when asked).
pub fn start(hub: &Arc<Hub>, net: Net) {
    *lock(&hub.doctor.net) = Some(net);
    match Report::load(&hub.home.doctor()) {
        Ok(r) => hub.doctor.set(Arc::new(r)),
        Err(e) if hub.home.doctor().exists() => warning!("doctor: ignoring the cache: {e}"),
        Err(_) => {}
    }
    let weak = Arc::downgrade(hub);
    let spawned = thread::Builder::new()
        .name("pt-doctor".into())
        .spawn(move || {
            loop {
                let Some(hub) = weak.upgrade() else { return };
                run_now(&hub);
                let d = &hub.doctor;
                let mut asked = lock(&d.refresh);
                let deadline = Instant::now() + EVERY;
                while !*asked {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    asked = d
                        .wake
                        .wait_timeout(asked, left)
                        .unwrap_or_else(|p| p.into_inner())
                        .0;
                }
                *asked = false;
            }
        });
    if let Err(e) = spawned {
        warning!("doctor: starting its thread: {e}");
    }
}

/// Asks the doctor thread for a run now.
pub fn refresh(hub: &Hub) {
    *lock(&hub.doctor.refresh) = true;
    hub.doctor.wake.notify_all();
}

/// Why the config alone rules `h` out: the local model's harnesses need a provider and a model.
fn unconfigured(hub: &Hub, h: Harness) -> Option<&'static str> {
    let c = &hub.config;
    (matches!(h, Harness::PiLocal | Harness::PiDurable)
        && (c.pi_local_provider.is_empty() || c.pi_local_model.is_empty()))
    .then_some(
        "not configured: set pi_local_provider (a provider in Pi's models.json) and \
         pi_local_model in config.json, then restart the hub",
    )
}

/// Why `h` may not be used, from the report as it stands; a hub with no report yet allows a
/// harness its config does not rule out.
pub fn check(hub: &Hub, h: Harness) -> Result<(), String> {
    match hub.doctor.report() {
        Some(r) => verdict(&r, h),
        None => match unconfigured(hub, h) {
            Some(why) => Err(format!("harness {} is {why}", wire::harness_wire(h))),
            None => Ok(()),
        },
    }
}

fn verdict(r: &Report, h: Harness) -> Result<(), String> {
    let name = wire::harness_wire(h);
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

/// The executor's gate before a launch. It waits for the first report of a hub with none
/// cached, so nothing launches before doctor has spoken once.
pub fn gate_launch(hub: &Hub, id: SleeveId, name: &str, h: Harness) -> Result<(), String> {
    let res = match hub.doctor.wait_report(FIRST_WAIT) {
        Some(r) => verdict(&r, h),
        None => Ok(()),
    };
    match &res {
        Ok(()) => {
            lock(&hub.doctor.gated).remove(&id);
        }
        Err(why) => {
            lock(&hub.doctor.gated).insert(id, why.clone());
            let what = if name == ENVOY {
                "Envoy parks until it is fixed"
            } else {
                "it resleeves to an eligible harness or parks"
            };
            hub.event(
                Kind::Sleeve,
                Level::Alarm,
                name,
                "owner",
                &format!("launch refused: {why}; {what}"),
            );
        }
    }
    res
}

/// The API's gate: SpawnSleeve, StartSleeve and Resleeve onto an ineligible harness are refused
/// before they reach the core. Bodies that do not parse are left to the handler.
pub fn precheck(hub: &Hub, method: &str, body: &pt_json::Value) -> Result<(), ApiError> {
    use pt_api::Method;
    use pt_api::method as m;
    let harness = if method == m::SpawnSleeve::NAME {
        api::SpawnSleeveRequest::from_json(body)
            .ok()
            .map(|r| wire::core_harness(r.harness))
    } else if method == m::Resleeve::NAME {
        api::ResleeveRequest::from_json(body)
            .ok()
            .map(|r| wire::core_harness(r.target))
    } else if method == m::StartSleeve::NAME {
        api::StartSleeveRequest::from_json(body)
            .ok()
            .and_then(|r| match r.target {
                Some(t) => Some(wire::core_harness(t)),
                None => {
                    let view = hub.view();
                    let st = &view.state;
                    st.sleeves
                        .values()
                        .find(|s| {
                            s.life != Life::Removed
                                && (s.id.0.to_string() == r.sleeve_id || s.name == r.sleeve_id)
                        })
                        .map(|s| s.target.harness)
                }
            })
    } else {
        None
    };
    match harness {
        Some(h) => check(hub, h).map_err(|why| ApiError::new(ErrorCode::Conflict, why)),
        None => Ok(()),
    }
}

/// `GetDoctor`'s answer: the report compacted, with the hub's own harness update runs.
pub fn api_report(hub: &Hub, r: Option<&Report>) -> api::DoctorReport {
    let Some(r) = r else {
        return api::DoctorReport {
            checked_at_ms: api::EpochMs(0),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            harnesses: Vec::new(),
            gh_logged_in: false,
            gh_account: String::new(),
            sandbox_ok: false,
            problems: vec!["doctor has not finished its first run".into()],
        };
    };
    let runs = hub.harness_updates.last_runs();
    api::DoctorReport {
        checked_at_ms: api::EpochMs(r.checked_ms),
        os: r.os.clone(),
        arch: r.arch.clone(),
        harnesses: r
            .harnesses
            .iter()
            .map(|h| {
                let last = runs.get(&h.binary.name);
                api::DoctorHarness {
                    name: h.name.clone(),
                    present: h.binary.present,
                    path: h.binary.path.clone(),
                    version: h.binary.version.clone(),
                    install: h.binary.install.as_str().into(),
                    login: h.credentials.present,
                    eligible: h.eligible,
                    reasons: h.reasons.clone(),
                    update: h.update.clone(),
                    updated_at_ms: last.map(|l| api::EpochMs(l.at_ms)),
                    update_note: last.map_or_else(String::new, |l| l.note.clone()),
                }
            })
            .collect(),
        gh_logged_in: r.gh.logged_in,
        gh_account: r.gh.account.clone(),
        sandbox_ok: r.sandbox.bwrap.present
            && r.sandbox.bwrap.error.is_empty()
            && r.sandbox.systemd_run_user,
        problems: r.problems.clone(),
    }
}
