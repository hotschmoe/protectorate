//! One supervised child process: started from a fresh `Command` each time, restarted with
//! backoff when it exits or fails its health check, stopped with SIGTERM and then SIGKILL. Its
//! stdout and stderr are appended to one log file. Used for headscale (inside bwrap) and the hub
//! member.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{Error, Result};

const POLL: Duration = Duration::from_millis(250);
const HEALTH_EVERY: Duration = Duration::from_secs(5);
const HEALTH_FAILS: u32 = 3;
const STOP_GRACE: Duration = Duration::from_secs(10);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// A run this long resets the backoff.
const STABLE_RUN: Duration = Duration::from_secs(60);

type Health = Box<dyn Fn() -> bool + Send + Sync>;

pub(crate) struct Spec {
    pub name: &'static str,
    pub command: Box<dyn Fn() -> Result<Command> + Send + Sync>,
    pub log: PathBuf,
    pub health: Health,
    pub ready_within: Duration,
    /// Signal the child's own child (bwrap's sandboxed process) rather than the child.
    pub signal_grandchild: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ChildState {
    pub pid: Option<u32>,
    pub healthy: bool,
    pub starts: u64,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct Flags {
    state: ChildState,
    paused: bool,
    stopping: bool,
    /// The supervisor thread has returned: no child runs and none will start.
    done: bool,
}

struct Shared {
    spec: Spec,
    flags: Mutex<Flags>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Flags> {
        self.flags.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub(crate) struct Supervisor {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    pub fn start(spec: Spec) -> Result<Supervisor> {
        let name = spec.name;
        let shared = Arc::new(Shared {
            spec,
            flags: Mutex::new(Flags::default()),
            cv: Condvar::new(),
        });
        let s = shared.clone();
        let thread = thread::Builder::new()
            .name(format!("sup-{name}"))
            .spawn(move || {
                let _done = Done(&s);
                run(&s);
            })
            .map_err(|e| Error::new(format!("starting the {name} supervisor: {e}")))?;
        Ok(Supervisor {
            shared,
            thread: Some(thread),
        })
    }

    pub fn state(&self) -> ChildState {
        self.shared.lock().state.clone()
    }

    /// Waits until the child passes its health check, or `timeout`.
    pub fn wait_healthy(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut f = self.shared.lock();
        while !f.state.healthy {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::new(format!(
                    "{} not healthy within {} s{}",
                    self.shared.spec.name,
                    timeout.as_secs(),
                    f.state
                        .last_error
                        .as_ref()
                        .map_or(String::new(), |e| format!(": {e}"))
                )));
            }
            f = self
                .shared
                .cv
                .wait_timeout(f, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        Ok(())
    }

    /// Stops the child and keeps it stopped until `resume`; returns once it has exited.
    pub fn pause(&self) {
        let mut f = self.shared.lock();
        f.paused = true;
        self.shared.cv.notify_all();
        while f.state.pid.is_some() {
            f = self.shared.cv.wait(f).unwrap_or_else(|p| p.into_inner());
        }
    }

    pub fn resume(&self) {
        self.shared.lock().paused = false;
        self.shared.cv.notify_all();
    }

    /// Stops the child for good and returns once the supervisor thread is done.
    pub fn stop(&self) {
        let mut f = self.shared.lock();
        f.stopping = true;
        self.shared.cv.notify_all();
        while !f.done {
            f = self.shared.cv.wait(f).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Kills the current child so the supervisor starts a new one (tests, and a stuck child).
    pub fn kill_child(&self) -> Result<()> {
        let pid = self.shared.lock().state.pid;
        match pid {
            Some(pid) => signal(pid, "KILL"),
            None => Err(Error::new(format!(
                "{} is not running",
                self.shared.spec.name
            ))),
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shared.lock().stopping = true;
        self.shared.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Marks the supervisor done when its thread returns, panics included.
struct Done<'a>(&'a Shared);

impl Drop for Done<'_> {
    fn drop(&mut self) {
        self.0.lock().done = true;
        self.0.cv.notify_all();
    }
}

fn run(s: &Shared) {
    let mut backoff = BACKOFF_MIN;
    loop {
        {
            let mut f = s.lock();
            while f.paused && !f.stopping {
                f = s.cv.wait(f).unwrap_or_else(|p| p.into_inner());
            }
            if f.stopping {
                return;
            }
        }
        let started = Instant::now();
        let outcome = match spawn(&s.spec) {
            Ok(child) => watch(s, child),
            Err(e) => Exit::Failed(e.to_string()),
        };
        let mut f = s.lock();
        f.state.pid = None;
        f.state.healthy = false;
        if let Exit::Failed(e) = &outcome {
            eprintln!("pt-fleet: {}: {e}", s.spec.name);
            f.state.last_error = Some(e.clone());
        }
        s.cv.notify_all();
        if f.stopping {
            return;
        }
        if f.paused || matches!(outcome, Exit::Asked) {
            continue;
        }
        if started.elapsed() >= STABLE_RUN {
            backoff = BACKOFF_MIN;
        }
        let deadline = Instant::now() + backoff;
        while !f.stopping {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            f =
                s.cv.wait_timeout(f, left)
                    .unwrap_or_else(|p| p.into_inner())
                    .0;
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

fn spawn(spec: &Spec) -> Result<Child> {
    let name = spec.name;
    if let Some(dir) = spec.log.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| Error::new(format!("creating {}: {e}", dir.display())))?;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.log)
        .map_err(|e| Error::new(format!("opening {}: {e}", spec.log.display())))?;
    let log2 = log
        .try_clone()
        .map_err(|e| Error::new(format!("{}: {e}", spec.log.display())))?;
    let mut cmd = (spec.command)()?;
    cmd.stdin(Stdio::null()).stdout(log).stderr(log2);
    // The child gets SIGTERM if this supervisor thread dies (pt-sys: the spawning thread).
    pt_sys::pdeathsig_on_spawn(&mut cmd, libc::SIGTERM)
        .map_err(|e| Error::new(format!("{name}: {e}")))?;
    cmd.spawn()
        .map_err(|e| Error::new(format!("starting {name} ({:?}): {e}", cmd.get_program())))
}

enum Exit {
    /// Paused or stopping.
    Asked,
    Failed(String),
}

fn watch(s: &Shared, mut child: Child) -> Exit {
    let spec = &s.spec;
    {
        let mut f = s.lock();
        f.state.pid = Some(child.id());
        f.state.starts += 1;
    }
    let started = Instant::now();
    let mut healthy = false;
    let mut last_check = Instant::now();
    let mut fails = 0;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Exit::Failed(format!("exited: {status}")),
            Ok(None) => {}
            Err(e) => return Exit::Failed(format!("waiting: {e}")),
        }
        {
            let f = s.lock();
            if f.stopping || f.paused {
                drop(f);
                stop(spec, &mut child);
                return Exit::Asked;
            }
        }
        if !healthy {
            if (spec.health)() {
                healthy = true;
                last_check = Instant::now();
                let mut f = s.lock();
                f.state.healthy = true;
                f.state.last_error = None;
                s.cv.notify_all();
            } else if started.elapsed() >= spec.ready_within {
                stop(spec, &mut child);
                return Exit::Failed(format!(
                    "not healthy within {} s",
                    spec.ready_within.as_secs()
                ));
            }
        } else if last_check.elapsed() >= HEALTH_EVERY {
            last_check = Instant::now();
            if (spec.health)() {
                fails = 0;
            } else {
                fails += 1;
                if fails >= HEALTH_FAILS {
                    stop(spec, &mut child);
                    return Exit::Failed(format!("failed {fails} health checks in a row"));
                }
            }
        }
        let f = s.lock();
        if !f.stopping && !f.paused {
            let _ = s.cv.wait_timeout(f, POLL);
        }
    }
}

/// SIGTERM, then SIGKILL after the grace period; always reaps the child.
fn stop(spec: &Spec, child: &mut Child) {
    let target = if spec.signal_grandchild {
        grandchild(child.id()).unwrap_or(child.id())
    } else {
        child.id()
    };
    let _ = signal(target, "TERM");
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The first process whose parent is `pid`, from /proc.
fn grandchild(pid: u32) -> Option<u32> {
    let path = format!("/proc/{pid}/task/{pid}/children");
    if let Ok(s) = fs::read_to_string(&path) {
        return s.split_whitespace().next().and_then(|p| p.parse().ok());
    }
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(child) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // The fields after the parenthesised command name: state, ppid, ...
        let ppid = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|p| p.parse::<u32>().ok());
        if ppid == Some(pid) {
            return Some(child);
        }
    }
    None
}

/// Sends a signal through the system `kill`, which keeps this crate free of unsafe code.
pub(crate) fn signal(pid: u32, sig: &str) -> Result<()> {
    let out = Command::new("kill")
        .arg(format!("-{sig}"))
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::new(format!("running kill -{sig} {pid}: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "kill -{sig} {pid}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}
