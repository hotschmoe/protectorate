//! Native harness updates (D33, D34): every `harness_update_hours` the hub runs each installed
//! harness program's own update command (`claude update`, `codex update`, `grok update`,
//! `pi update self`, as doctor lists them), one at a time on this thread. A program runs only
//! while no sleeve on any of its harnesses is mid-turn; otherwise it waits and is tried again
//! every few seconds. A running sleeve keeps the binary it started with. The output goes to the
//! hub log, each run is one event line with the version before and after, and the last run per
//! program is kept in `<home>/harness-updates.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use pt_api::{self as api, EventKind as Kind, HarnessUpdateState, Level};
use pt_core::{Activity, Harness, Proc, Sleeve};
use pt_doctor::Report;
use pt_json::record;

use crate::bus::Bus;
use crate::doctor::Doctor;
use crate::home::{Home, now_ms, write_private};
use crate::lock::lock;
use crate::log::{info, warning};
use crate::view::View;
use crate::wire;

const RUN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a program held back by a busy sleeve is tried again.
const RETRY: Duration = Duration::from_secs(3);
/// The longest sleep between checks for due programs.
const NAP: Duration = Duration::from_secs(600);

record! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Run {
        pub at_ms: u64,
        pub ok: bool,
        pub from: String,
        pub to: String,
        /// The event line.
        pub note: String,
    }
}

#[derive(Default)]
pub struct Updater {
    last: Mutex<BTreeMap<String, Run>>,
    /// Programs asked for by `UpdateHarnesses`, run as soon as they are idle.
    wanted: Mutex<BTreeSet<String>>,
    wake: Condvar,
    running: Mutex<Option<String>>,
}

impl Updater {
    pub fn last_runs(&self) -> BTreeMap<String, Run> {
        lock(&self.last).clone()
    }

    /// Loads the last run per program from `harness-updates.json`.
    pub fn load(&self, home: &Home) {
        if let Ok(bytes) = std::fs::read(home.harness_updates()) {
            match pt_json::from_bytes::<BTreeMap<String, Run>>(&bytes) {
                Ok(m) => *lock(&self.last) = m,
                Err(e) => warning!(
                    "harness update: ignoring {}: {e}",
                    home.harness_updates().display()
                ),
            }
        }
    }

    /// Sleeps after a round: until a program is asked for, or `nap`; only briefly while one
    /// already waits for its sleeves.
    pub fn nap(&self, nap: Duration) {
        let wanted = lock(&self.wanted);
        if wanted.is_empty() {
            drop(self.wake.wait_timeout(wanted, nap));
        } else {
            drop(wanted);
            thread::sleep(RETRY.min(nap));
        }
    }

    /// `UpdateHarnesses`: queues the programs and says, per program, what happens now.
    pub fn request(
        &self,
        doctor: &Doctor,
        view: impl Fn() -> Arc<View>,
        names: &[String],
    ) -> Vec<api::HarnessUpdate> {
        let report = doctor.report();
        let known = report.as_deref().map(programs).unwrap_or_default();
        let names: Vec<String> = if names.is_empty() {
            known.iter().map(|p| p.name.clone()).collect()
        } else {
            names.to_vec()
        };
        let running = lock(&self.running).clone();
        let mut out = Vec::new();
        for n in names {
            let (state, detail) = match known.iter().find(|p| p.name == n) {
                None => (
                    HarnessUpdateState::Skipped,
                    "not installed, or it has no update command".to_string(),
                ),
                Some(_) if running.as_deref() == Some(n.as_str()) => {
                    (HarnessUpdateState::Skipped, "already running".to_string())
                }
                Some(p) => {
                    lock(&self.wanted).insert(n.clone());
                    match busy_sleeve(&view(), &p.harnesses) {
                        Some(s) => (
                            HarnessUpdateState::Deferred,
                            format!(
                                "sleeve {s} is mid-turn; it runs once every {n} sleeve is idle"
                            ),
                        ),
                        None => (HarnessUpdateState::Started, p.argv.join(" ")),
                    }
                }
            };
            out.push(api::HarnessUpdate {
                program: n,
                state,
                detail,
            });
        }
        self.wake.notify_all();
        out
    }

    /// Runs every program that is due or asked for and idle; returns how long to sleep.
    pub fn round(
        &self,
        doctor: &Doctor,
        bus: &Bus,
        home: &Home,
        view: impl Fn() -> Arc<View>,
        every: Duration,
    ) -> Duration {
        let Some(report) = doctor.wait_report(NAP) else {
            return NAP;
        };
        let now = now_ms();
        let every_ms = every.as_millis() as u64;
        let mut next = NAP;
        let mut ran = false;
        for p in programs(&report) {
            let last = lock(&self.last).get(&p.name).map(|r| r.at_ms);
            let asked = lock(&self.wanted).contains(&p.name);
            let due_at = match last {
                _ if every.is_zero() => None,
                None => Some(now),
                Some(at) => Some(at.saturating_add(every_ms)),
            };
            let due = asked || due_at.is_some_and(|t| t <= now);
            if !due {
                if let Some(t) = due_at {
                    next = next.min(Duration::from_millis(t - now));
                }
                continue;
            }
            if let Some(s) = busy_sleeve(&view(), &p.harnesses) {
                info!(
                    "harness update: {} waits for sleeve {s} to finish its turn",
                    p.name
                );
                next = next.min(RETRY);
                lock(&self.wanted).insert(p.name.clone());
                continue;
            }
            lock(&self.wanted).remove(&p.name);
            self.run(bus, home, &p);
            ran = true;
        }
        if ran {
            doctor.refresh();
        }
        next.max(Duration::from_millis(100))
    }

    fn run(&self, bus: &Bus, home: &Home, p: &Program) {
        *lock(&self.running) = Some(p.name.clone());
        let shown = {
            let mut a = p.argv.clone();
            a[0] = p.name.clone();
            a.join(" ")
        };
        let from = version(&p.path);
        info!("harness update: running {} ({from})", p.argv.join(" "));
        let args: Vec<&str> = p.argv[1..].iter().map(String::as_str).collect();
        let out = pt_doctor::run_program(Path::new(&p.argv[0]), &args, RUN_TIMEOUT);
        let (ok, why) = match &out {
            Ok(o) => {
                for line in o.stdout.lines().chain(o.stderr.lines()) {
                    if !line.trim().is_empty() {
                        info!("harness update {}: {line}", p.name);
                    }
                }
                (o.ok, if o.ok { String::new() } else { o.failure() })
            }
            Err(e) => (false, e.clone()),
        };
        let to = version(&p.path);
        let note = match (ok, from == to) {
            (true, true) => format!("{}: {to}, up to date ({shown})", p.name),
            (true, false) => format!("{}: {from} -> {to} ({shown})", p.name),
            (false, _) => format!("{}: {shown} failed: {why}; now {to}", p.name),
        };
        let level = if ok { Level::Info } else { Level::Warn };
        bus.event(Kind::Hub, level, "harness-update", "owner", &note);
        info!("harness update: {note}");
        let r = Run {
            at_ms: now_ms(),
            ok,
            from,
            to,
            note,
        };
        let all = {
            let mut last = lock(&self.last);
            last.insert(p.name.clone(), r);
            last.clone()
        };
        let text = pt_json::ToJson::to_json(&all).to_string();
        if let Err(e) = write_private(&home.harness_updates(), text.as_bytes()) {
            warning!("harness update: {e}");
        }
        *lock(&self.running) = None;
    }
}

/// One harness program as doctor found it, and the harnesses that run it.
struct Program {
    name: String,
    path: String,
    argv: Vec<String>,
    harnesses: Vec<Harness>,
}

fn programs(r: &Report) -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    for h in &r.harnesses {
        let Some(core) = wire::harness_from(&h.name) else {
            continue;
        };
        if !h.binary.present || h.update.is_empty() {
            continue;
        }
        match out.iter_mut().find(|p| p.name == h.binary.name) {
            Some(p) => p.harnesses.push(core),
            None => out.push(Program {
                name: h.binary.name.clone(),
                path: h.binary.path.clone(),
                argv: h.update.clone(),
                harnesses: vec![core],
            }),
        }
    }
    out
}

/// Mid-turn, or about to be: launching, delivering, working, blocked, compacting or unknown.
fn busy(s: &Sleeve) -> bool {
    match s.proc {
        Proc::Starting(_) => true,
        Proc::Up => !matches!(s.activity, Activity::Idle { .. } | Activity::Ended),
        Proc::Down => false,
    }
}

/// The first live sleeve on one of `harnesses` that is mid-turn.
fn busy_sleeve(view: &View, harnesses: &[Harness]) -> Option<String> {
    view.state
        .sleeves
        .values()
        .find(|s| s.life.live() && harnesses.contains(&s.target.harness) && busy(s))
        .map(|s| s.name.clone())
}

fn version(path: &str) -> String {
    match pt_doctor::run_program(Path::new(path), &["--version"], VERSION_TIMEOUT) {
        Ok(o) if o.ok => o.first_line(),
        Ok(o) => format!("unknown ({})", o.failure()),
        Err(e) => format!("unknown ({e})"),
    }
}
