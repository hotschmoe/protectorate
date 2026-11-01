//! Updates (SPEC 6a). The hub sends `Update`; the node fetches the release from GitHub itself,
//! stages `<state>/releases/<tag>/protectorate-node` (checked against SHA256SUMS, then run once),
//! switches `releases/current` on probation and exits with [`RESTART_CODE`] for systemd to start
//! the new binary. The new process keeps the release once the hub makes contact (a `Hello` on a
//! `tag:hub` connection) within the window; otherwise it switches back and exits again. A binary
//! that cannot even start is switched back by `probation`, which the unit runs from
//! `releases/previous` before each start.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use pt_frame::msg::{ErrorCode, Update, UpdateReply};
use pt_update::{MAX_TRIAL_STARTS, Source, Store, Verdict};

/// The exit status that asks the service manager for a restart on `releases/current`.
pub const RESTART_CODE: i32 = 75;
const PROGRAM: &str = "protectorate-node";

pub const USAGE: &str = "  probation [--state-dir DIR]       run by the service before each start (from releases/previous):
                                    count a start of the release on trial, and switch back once
                                    it keeps failing or its window has passed";

/// The release store under the state dir, and what this process was started from.
pub struct Releases {
    store: Option<Store>,
    started: Option<String>,
    /// This process is the release on trial and the hub has not made contact yet.
    trial: AtomicBool,
    updating: Mutex<()>,
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
            updating: Mutex::new(()),
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
        if !self.trial.swap(false, Ordering::SeqCst) {
            return;
        }
        let Some(store) = &self.store else { return };
        match store.confirm() {
            Ok(true) => eprintln!(
                "protectorate-node: the hub made contact; release {} stays",
                self.started.as_deref().unwrap_or("-")
            ),
            Ok(false) => {}
            Err(e) => eprintln!("protectorate-node: confirming the update: {e}"),
        }
    }

    /// `Update`: stage, check and switch. The caller sends the reply, then calls
    /// [`restart_soon`] when `restart` is true.
    pub fn update(
        &self,
        u: &Update,
        running_jobs: usize,
    ) -> Result<(UpdateReply, bool), (ErrorCode, String)> {
        let unsupported = |m: String| (ErrorCode::Unsupported, m);
        if cfg!(windows) {
            return Err(unsupported(
                "updating a Windows node is not supported yet; reinstall it".into(),
            ));
        }
        let Ok(_one) = self.updating.try_lock() else {
            return Err((ErrorCode::Busy, "an update is already running".into()));
        };
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| (ErrorCode::Internal, "no release store".to_string()))?;
        store.started_from_current(PROGRAM).map_err(unsupported)?;
        if running_jobs > 0 {
            return Err((
                ErrorCode::Busy,
                format!("{running_jobs} jobs are running; update once they finish"),
            ));
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
        store
            .check_binary(&rel.tag, PROGRAM, "--version")
            .map_err(fail)?;
        if self.started.as_deref() == Some(rel.tag.as_str()) {
            return Ok((
                UpdateReply {
                    from: self.started(),
                    to: rel.tag,
                    reused: true,
                },
                false,
            ));
        }
        let sw = store
            .switch_on_probation(&rel.tag, Duration::from_secs(u.probation_secs.max(1)))
            .map_err(fail)?;
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
        Ok((
            UpdateReply {
                from: self.started().or(sw.from),
                to: sw.to,
                reused: staged.reused,
            },
            true,
        ))
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

/// `probation`: the unit's ExecStartPre.
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
