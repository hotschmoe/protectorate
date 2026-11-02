//! `supervise`: what the systemd unit does for a Linux node, as one process; the Windows service
//! runs it (the SCM has no ExecStartPre and cannot restart a service from inside it). Before
//! each start it finishes any release switch a crash interrupted and runs
//! `releases/previous/protectorate-node probation` in a kill-on-close job with a timeout, then
//! runs `releases/current/protectorate-node serve --state-dir DIR ...` with that path as argv[0],
//! so the node may take updates. Unlike systemd, a failed or hung probation does not keep the
//! node down (see [`prestart`]). It starts the node again 1 s after it exits [`RESTART_CODE`]
//! (an update or a rollback) and 5 s after any other exit. Stopping the supervisor ends the node
//! and a running probation: on Windows they run in kill-on-close Job Objects, on Linux they get
//! SIGTERM (and SIGKILL should the supervisor die).

use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::update::RESTART_CODE;

pub const USAGE: &str = "  supervise --state-dir DIR [--service] SERVE-OPTIONS...
                                    run the node as its systemd unit does: before each start
                                    releases/previous probation, then releases/current serve
                                    --state-dir DIR SERVE-OPTIONS, again after it exits;
                                    --service: Windows, under the service manager (what
                                    install-service registers), logging to DIR\\node.log";

const PROGRAM: &str = "protectorate-node";
const UPDATE_DELAY: Duration = Duration::from_secs(1);
const RESTART_DELAY: Duration = Duration::from_secs(5);
/// `probation` takes milliseconds; one that runs this long is hung.
const PROBATION_TIMEOUT: Duration = Duration::from_secs(60);

struct Plan {
    state: PathBuf,
    serve: Vec<String>,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut state = None;
    let mut service = false;
    let mut serve = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--state-dir" => {
                state = Some(PathBuf::from(it.next().ok_or("--state-dir needs a value")?))
            }
            "--service" if cfg!(windows) => service = true,
            "--stdio" => return Err("supervise: serve runs with a listener, not --stdio".into()),
            _ => serve.push(a.clone()),
        }
    }
    let state = state.ok_or("supervise needs --state-dir")?;
    if !state.is_absolute() {
        return Err(format!("--state-dir {}: must be absolute", state.display()));
    }
    let mut full = vec!["--state-dir".to_string(), state.display().to_string()];
    full.extend(serve.iter().cloned());
    crate::flags::check(&full).map_err(|e| format!("supervise: {e}"))?;
    let plan = Plan { state, serve: full };
    if service {
        return as_service(plan);
    }
    supervise(&plan, None, stop_flag()?)
}

#[cfg(unix)]
type Stop = Option<pt_sys::TermFlag>;
#[cfg(not(unix))]
type Stop = Option<std::convert::Infallible>;

#[cfg(unix)]
fn stop_flag() -> Result<Stop, String> {
    pt_sys::install_term_handler()
        .map(Some)
        .map_err(|e| format!("installing the termination handler: {e}"))
}

#[cfg(not(unix))]
fn stop_flag() -> Result<Stop, String> {
    Ok(None)
}

/// Sleeps for `d`; true when the supervisor was asked to stop meanwhile.
fn pause(stop: &Stop, d: Duration) -> bool {
    #[cfg(unix)]
    if let Some(t) = stop {
        return t.wait_timeout(d);
    }
    #[cfg(not(unix))]
    let _ = stop;
    thread::sleep(d);
    false
}

/// Under the SCM: stdio goes to `<state>\node.log`, RUNNING is reported at once, and stopping
/// the service ends this process, whose job ends the node.
#[cfg(windows)]
fn as_service(plan: Plan) -> Result<(), String> {
    let path = plan.state.join("node.log");
    let open = || {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("opening {}: {e}", path.display()))
    };
    pt_node::windows::redirect_stdio(open()?)
        .map_err(|e| format!("logging to {}: {e}", path.display()))?;
    let log = open()?;
    eprintln!(
        "protectorate-node: service {} supervisor {} starting (pid {})",
        crate::manifest::SERVICE,
        pt_version::VERSION,
        std::process::id()
    );
    let result = pt_node::windows::run_service(
        crate::manifest::SERVICE,
        Box::new(move |ready| {
            ready();
            supervise(&plan, Some(&log), None)
        }),
    );
    match &result {
        Ok(()) => eprintln!("protectorate-node: service stopped"),
        Err(e) => eprintln!("protectorate-node: service failed: {e}"),
    }
    result
}

#[cfg(not(windows))]
fn as_service(_plan: Plan) -> Result<(), String> {
    Err("--service is for the Windows service".into())
}

fn supervise(plan: &Plan, log: Option<&File>, stop: Stop) -> Result<(), String> {
    let releases = plan.state.join("releases");
    let exe = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    let mut starts = 0u64;
    loop {
        if !prestart(
            &plan.state,
            &releases.join("previous").join(&exe),
            log,
            &stop,
        ) {
            return Ok(());
        }
        starts += 1;
        let release = std::fs::read_link(releases.join("current"))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "-".into());
        eprintln!("protectorate-node: supervise: start {starts}, release {release}");
        let code = match run_node(&releases.join("current").join(&exe), plan, log, &stop) {
            Ok(Some(code)) => code,
            Ok(None) => return Ok(()),
            Err(e) => {
                eprintln!("protectorate-node: supervise: {e}");
                -1
            }
        };
        let delay = if code == RESTART_CODE {
            UPDATE_DELAY
        } else {
            RESTART_DELAY
        };
        eprintln!(
            "protectorate-node: supervise: the node exited {code}; starting it in {} s",
            delay.as_secs()
        );
        if pause(&stop, delay) {
            return Ok(());
        }
    }
}

/// Finishes an interrupted switch (so `previous` exists), then runs the previous release's
/// `probation` in a kill-on-close job, for at most [`PROBATION_TIMEOUT`]. Should that fail or
/// hang, this binary's own probation code counts the start instead, and the node starts on
/// `current` whatever happened: `current` is the release the store names (any other binary
/// would not run as `releases/current` and could take no update), and a release on trial still
/// has its own watch and the next start's count. False when the supervisor was stopped.
fn prestart(state: &Path, previous: &Path, log: Option<&File>, stop: &Stop) -> bool {
    let say = |m: String| eprintln!("protectorate-node: supervise: {m}");
    if let Err(e) = pt_update::Store::open(state) {
        say(format!("releases: {e}"));
    }
    let failed = match probation(state, previous, log, stop) {
        Ok(Some(())) => return true,
        Ok(None) => return false,
        Err(e) => e,
    };
    say(format!(
        "{failed}; counting the start with this supervisor's probation"
    ));
    let verdict =
        pt_update::Store::open(state).and_then(|s| s.start_attempt(pt_update::MAX_TRIAL_STARTS));
    match verdict {
        Ok(pt_update::Verdict::RolledBack(sw)) => say(format!(
            "release {} rolled back to {}",
            sw.from.as_deref().unwrap_or("-"),
            sw.to
        )),
        Ok(pt_update::Verdict::Waiting { remaining }) => say(format!(
            "release on probation, {} s left for the hub to make contact",
            remaining.as_secs()
        )),
        Ok(
            pt_update::Verdict::Idle
            | pt_update::Verdict::Confirmed
            | pt_update::Verdict::Kept { .. },
        ) => {}
        Err(e) => say(format!("probation: {e}; starting releases/current anyway")),
    }
    true
}

/// `Some` when the previous release's probation succeeded, `None` when the supervisor was
/// stopped meanwhile.
fn probation(
    state: &Path,
    previous: &Path,
    log: Option<&File>,
    stop: &Stop,
) -> Result<Option<()>, String> {
    #[cfg(debug_assertions)]
    let timeout = std::env::var("PT_TEST_PROBATION_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(PROBATION_TIMEOUT, Duration::from_millis);
    #[cfg(not(debug_assertions))]
    let timeout = PROBATION_TIMEOUT;
    let mut cmd = Command::new(previous);
    cmd.args(["probation", "--state-dir"]).arg(state);
    output_to(&mut cmd, log)?;
    let (mut child, _job) =
        spawn(&mut cmd).map_err(|e| format!("running {} probation: {e}", previous.display()))?;
    match wait(&mut child, stop, Some(timeout))? {
        Waited::Exited(0) => Ok(Some(())),
        Waited::Exited(code) => Err(format!("{} probation exited {code}", previous.display())),
        Waited::TimedOut => {
            end(&mut child);
            Err(format!(
                "{} probation did not finish in {} s and was killed",
                previous.display(),
                timeout.as_secs()
            ))
        }
        Waited::Stopped => {
            end(&mut child);
            Ok(None)
        }
    }
}

enum Waited {
    Exited(i32),
    TimedOut,
    Stopped,
}

fn wait(child: &mut Child, stop: &Stop, timeout: Option<Duration>) -> Result<Waited, String> {
    let deadline = timeout.map(|t| std::time::Instant::now() + t);
    loop {
        if let Some(st) = child
            .try_wait()
            .map_err(|e| format!("waiting for {}: {e}", child.id()))?
        {
            return Ok(Waited::Exited(st.code().unwrap_or(-1)));
        }
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            return Ok(Waited::TimedOut);
        }
        if pause(stop, Duration::from_millis(100)) {
            return Ok(Waited::Stopped);
        }
    }
}

fn output_to(cmd: &mut Command, log: Option<&File>) -> Result<(), String> {
    cmd.stdin(Stdio::null());
    if let Some(f) = log {
        let clone = || f.try_clone().map_err(|e| format!("node.log: {e}"));
        cmd.stdout(clone()?).stderr(clone()?);
    }
    Ok(())
}

/// Runs the node until it exits (its code) or the supervisor is stopped (`None`).
fn run_node(
    bin: &Path,
    plan: &Plan,
    log: Option<&File>,
    stop: &Stop,
) -> Result<Option<i32>, String> {
    let mut cmd = Command::new(bin);
    cmd.arg("serve").args(&plan.serve);
    output_to(&mut cmd, log)?;
    let (mut child, _job) =
        spawn(&mut cmd).map_err(|e| format!("starting {}: {e}", bin.display()))?;
    match wait(&mut child, stop, None)? {
        Waited::Exited(code) => Ok(Some(code)),
        Waited::TimedOut | Waited::Stopped => {
            end(&mut child);
            Ok(None)
        }
    }
}

#[cfg(windows)]
fn spawn(cmd: &mut Command) -> std::io::Result<(Child, pt_node::windows::Job)> {
    let job = pt_node::windows::Job::without_descendants()?;
    let child = pt_node::windows::spawn_in_job(cmd, &job)?;
    Ok((child, job))
}

#[cfg(unix)]
fn spawn(cmd: &mut Command) -> std::io::Result<(Child, ())> {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    if cfg!(target_os = "linux") {
        pt_sys::pdeathsig_on_spawn(cmd, pt_sys::SIGKILL)?;
    }
    Ok((cmd.spawn()?, ()))
}

/// SIGTERM, then SIGKILL after 10 s.
#[cfg(unix)]
fn end(child: &mut Child) {
    let _ = pt_sys::kill_group(child.id(), pt_sys::SIGTERM);
    for _ in 0..100 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = pt_sys::kill_group(child.id(), pt_sys::SIGKILL);
    let _ = child.wait();
}

#[cfg(not(unix))]
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
