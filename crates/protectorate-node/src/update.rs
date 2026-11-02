//! Updates (SPEC 6a). The hub sends `Update`; the node fetches the release from GitHub itself,
//! stages `<state>/releases/<tag>/protectorate-node` (checked against SHA256SUMS, then run once),
//! switches `releases/current` on probation and exits with [`RESTART_CODE`] for the service
//! manager to start the new binary: systemd on Linux, the service's `supervise` on Windows. The
//! new process keeps the release once it has fully answered a non-Hello request on a `tag:hub`
//! connection; otherwise it switches back and exits again. A binary that cannot even start is
//! switched back by `probation`, which the unit (or `supervise`) runs from `releases/previous`
//! before each start.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use pt_frame::msg::{ErrorCode, JobInfo, SubmitJob, Update, UpdateReply};
use pt_node::{Runner, jobs::JobError};
use pt_update::{MAX_TRIAL_STARTS, Source, Store, Verdict};

/// The exit status that asks the service manager for a restart on `releases/current`.
pub const RESTART_CODE: i32 = 75;
const PROGRAM: &str = "protectorate-node";
/// The `--version` line of a binary that keeps the supervisor's start contract: it runs as
/// `releases/current/protectorate-node serve`, exits [`RESTART_CODE`] to be restarted, and
/// switches release pointers on Windows.
pub const SUPERVISED: &str = "supervise: 1";

pub const USAGE: &str = "  probation [--state-dir DIR]       run by the service before each start (from releases/previous):
                                    count a start of the release on trial, and switch back once
                                    it keeps failing or its window has passed";

/// The release store under the state dir, and what this process was started from.
pub struct Releases {
    store: Option<Store>,
    started: Option<String>,
    /// This process is the release on trial and the hub has not made contact yet.
    trial: AtomicBool,
    updating: Mutex<bool>,
}

struct Admission<'a> {
    updating: &'a Mutex<bool>,
    switched: bool,
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        if !self.switched {
            *self.updating.lock().unwrap_or_else(|p| p.into_inner()) = false;
        }
    }
}

fn restart(why: &str) -> ! {
    eprintln!("protectorate-node: {why}; exiting {RESTART_CODE} to restart");
    std::process::exit(RESTART_CODE)
}

impl Releases {
    /// Opens the store and settles any probation: a trial whose window passed is switched back
    /// (and this process exits), one still open is watched until the hub confirms it.
    pub fn open(state: &Path) -> Releases {
        let store = Store::open(state)
            .map_err(|e| eprintln!("protectorate-node: releases: {e}"))
            .ok();
        let started = store.as_ref().and_then(Store::running_release);
        let r = Releases {
            store,
            started,
            trial: AtomicBool::new(false),
            updating: Mutex::new(false),
        };
        let Some(store) = &r.store else { return r };
        let on_trial = store
            .probation()
            .ok()
            .flatten()
            .is_some_and(|p| Some(&p.version) == r.started.as_ref());
        if !on_trial {
            return r;
        }
        match store.check_probation() {
            Ok(Verdict::RolledBack(sw)) => restart(&format!(
                "release {} was not confirmed in time; switched back to {}",
                sw.from.as_deref().unwrap_or("-"),
                sw.to
            )),
            Ok(Verdict::Waiting { remaining }) => {
                eprintln!(
                    "protectorate-node: release {} on probation; the hub has {} s to make contact",
                    r.started.as_deref().unwrap_or("-"),
                    remaining.as_secs()
                );
                r.trial.store(true, Ordering::SeqCst);
                let store = store.clone();
                thread::spawn(move || watch(store));
            }
            Ok(Verdict::Idle | Verdict::Confirmed) => {}
            Err(e) => eprintln!("protectorate-node: probation: {e}"),
        }
        r
    }

    pub fn started(&self) -> Option<String> {
        self.started.clone()
    }

    /// The hub made contact: the release on trial stays.
    pub fn contact(&self) {
        if !self.trial.load(Ordering::SeqCst) {
            return;
        }
        let (Some(store), Some(started)) = (&self.store, &self.started) else {
            return;
        };
        match store.confirm(started) {
            Ok(true) => {
                self.trial.store(false, Ordering::SeqCst);
                eprintln!(
                    "protectorate-node: the hub made contact; release {} stays",
                    self.started.as_deref().unwrap_or("-")
                );
            }
            Ok(false) => {}
            Err(e) => eprintln!("protectorate-node: confirming the update: {e}"),
        }
    }

    pub fn submit(&self, runner: &Runner, spec: SubmitJob) -> Result<JobInfo, JobError> {
        let updating = self.updating.lock().unwrap_or_else(|p| p.into_inner());
        if *updating {
            return Err(JobError {
                code: ErrorCode::Busy,
                message: "an update is in progress".into(),
            });
        }
        runner.submit(spec)
    }

    /// `Update`: stage, check, switch and schedule the restart before sending the reply.
    pub fn update(&self, u: &Update, runner: &Runner) -> Result<UpdateReply, (ErrorCode, String)> {
        let unsupported = |m: String| (ErrorCode::Unsupported, m);
        let mut gate = self.updating.lock().unwrap_or_else(|p| p.into_inner());
        if *gate {
            return Err((ErrorCode::Busy, "an update is already running".into()));
        }
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| (ErrorCode::Internal, "no release store".to_string()))?;
        store.started_from_current(PROGRAM).map_err(unsupported)?;
        let running_jobs = runner.running();
        if running_jobs > 0 {
            return Err((
                ErrorCode::Busy,
                format!("{running_jobs} jobs are running; update once they finish"),
            ));
        }
        *gate = true;
        drop(gate);
        let mut admission = Admission {
            updating: &self.updating,
            switched: false,
        };
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("PROTECTORATE_NODE_TEST_UPDATE_PAUSE") {
            eprintln!("protectorate-node: update admitted for testing");
            let until = std::time::Instant::now() + Duration::from_secs(20);
            while std::fs::read_to_string(&path).is_ok_and(|s| s.trim() == "1")
                && std::time::Instant::now() < until
            {
                thread::sleep(Duration::from_millis(20));
            }
        }
        let fail = |e: pt_update::Error| {
            let code = match e.kind {
                pt_update::ErrorKind::Release | pt_update::ErrorKind::Verify => {
                    ErrorCode::BadRequest
                }
                _ => ErrorCode::Internal,
            };
            (code, e.message)
        };
        let src = Source::new(&u.api, &u.repo).map_err(fail)?;
        let rel = src.release(&u.version).map_err(fail)?;
        let arch = pt_update::arch()
            .ok_or_else(|| unsupported(format!("no build for {}", std::env::consts::ARCH)))?;
        let staged = store
            .stage(
                &src,
                &rel,
                &pt_update::node_picks(std::env::consts::OS, arch),
            )
            .map_err(fail)?;
        let reports = store
            .check_binary(&rel.tag, PROGRAM, "--version")
            .map_err(fail)?;
        // The Windows supervisor (`supervise`) restarts only a node that exits RESTART_CODE and
        // switches pointers the Windows way; an older binary would strand the node there.
        if cfg!(windows) && !reports.lines().any(|l| l.trim() == SUPERVISED) {
            return Err((
                ErrorCode::BadRequest,
                format!(
                    "{} predates the Windows service supervisor (its --version has no \
                     '{SUPERVISED}'); a Windows node cannot run it",
                    rel.tag
                ),
            ));
        }
        if self.started.as_deref() == Some(rel.tag.as_str()) {
            return Ok(UpdateReply {
                from: self.started(),
                to: rel.tag,
                reused: true,
            });
        }
        let sw = store
            .switch_on_probation(&rel.tag, Duration::from_secs(u.probation_secs.max(1)))
            .map_err(fail)?;
        admission.switched = true;
        restart_soon(sw.to.clone());
        eprintln!(
            "protectorate-node: switched from {} to {}{}",
            sw.from.as_deref().unwrap_or("-"),
            sw.to,
            if staged.reused {
                " (already staged)"
            } else {
                ""
            }
        );
        Ok(UpdateReply {
            from: self.started().or(sw.from),
            to: sw.to,
            reused: staged.reused,
        })
    }
}

/// Lets the reply reach the hub, then exits for the restart.
pub fn restart_soon(to: String) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        restart(&format!("updated to {to}"));
    });
}

fn watch(store: Store) {
    loop {
        thread::sleep(Duration::from_secs(1));
        #[cfg(debug_assertions)]
        if std::env::var_os("PROTECTORATE_NODE_TEST_PAUSE_WATCH").is_some() {
            continue;
        }
        match store.check_probation() {
            Ok(Verdict::Waiting { .. }) => {}
            Ok(Verdict::RolledBack(sw)) => restart(&format!(
                "the hub made no contact in time; switched back from {} to {}",
                sw.from.as_deref().unwrap_or("-"),
                sw.to
            )),
            Ok(Verdict::Idle | Verdict::Confirmed) => return,
            Err(e) => eprintln!("protectorate-node: probation: {e}"),
        }
    }
}

/// `probation`: the unit's ExecStartPre, and `supervise`'s step before each start.
pub fn probation(args: &[String]) -> Result<(), String> {
    let mut state = crate::join::system_state()?;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--state-dir" => {
                state = it
                    .next()
                    .map(Into::into)
                    .ok_or("--state-dir needs a value")?
            }
            other => return Err(format!("probation: unknown option '{other}'")),
        }
    }
    let store = Store::open(&state).map_err(|e| e.to_string())?;
    match store
        .start_attempt(MAX_TRIAL_STARTS)
        .map_err(|e| e.to_string())?
    {
        Verdict::Idle | Verdict::Confirmed => {}
        Verdict::Waiting { remaining } => println!(
            "release on probation, {} s left for the hub to make contact",
            remaining.as_secs()
        ),
        Verdict::RolledBack(sw) => println!(
            "release {} rolled back to {}",
            sw.from.as_deref().unwrap_or("-"),
            sw.to
        ),
    }
    Ok(())
}
