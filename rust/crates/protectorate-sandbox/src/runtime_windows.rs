//! Windows direct harness launch. Sandboxed workloads remain unsupported.
use crate::job::{resume_process, Job};
use crate::{PlanKind, ValidatedPlan};
use std::{
    collections::BTreeMap,
    io,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
};

#[derive(Debug)]
pub enum LaunchError {
    Invalid(&'static str),
    Unsupported(&'static str),
    Io {
        operation: &'static str,
        source: io::Error,
    },
}
impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid direct harness launch: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported sandbox capability: {message}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
        }
    }
}
impl std::error::Error for LaunchError {}
pub type Result<T> = std::result::Result<T, LaunchError>;

fn ioerr(operation: &'static str) -> impl FnOnce(io::Error) -> LaunchError {
    move |source| LaunchError::Io { operation, source }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StdioMode {
    #[default]
    Capture,
    Duplex,
    Inherit,
}

#[derive(Clone, Debug)]
pub struct LaunchOptions {
    pub helper: PathBuf,
    pub runtime_dir: PathBuf,
    pub home_target: PathBuf,
    pub working_directory: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub setup_timeout: Duration,
    pub stdio: StdioMode,
}
impl LaunchOptions {
    pub fn new(
        helper: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
        home_target: impl Into<PathBuf>,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            helper: helper.into(),
            runtime_dir: runtime_dir.into(),
            home_target: home_target.into(),
            working_directory: working_directory.into(),
            environment: BTreeMap::new(),
            setup_timeout: Duration::from_secs(15),
            stdio: StdioMode::Capture,
        }
    }
}

fn attach(job: &Job, child: &Child) -> Result<()> {
    let process = child.as_raw_handle() as HANDLE;
    // SAFETY: the child is suspended and its handle is live for both calls.
    unsafe {
        job.assign(process)
            .map_err(ioerr("assign direct harness job"))?;
        resume_process(process).map_err(ioerr("resume direct harness"))
    }
}

#[derive(Clone)]
pub struct KillHandle {
    job: Arc<Job>,
    alive: Arc<AtomicBool>,
}
impl KillHandle {
    pub fn kill(&self) {
        if self.alive.swap(false, Ordering::AcqRel) {
            let _ = self.job.terminate();
        }
    }
}

pub struct ChildHandle {
    child: Option<Child>,
    kill: KillHandle,
    pub init_pid: u32,
}
impl ChildHandle {
    pub fn id(&self) -> u32 {
        self.child.as_ref().map_or(0, Child::id)
    }
    pub fn unit(&self) -> &str {
        ""
    }
    pub fn kill_handle(&self) -> KillHandle {
        self.kill.clone()
    }
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }
    pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.kill.kill();
        Ok(())
    }
    pub fn wait(mut self) -> Result<ExitStatus> {
        let mut child = self
            .child
            .take()
            .ok_or(LaunchError::Invalid("child already waited"))?;
        let status = child.wait().map_err(ioerr("wait direct harness"))?;
        self.kill.kill();
        Ok(status)
    }
}
impl Drop for ChildHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            self.kill.kill();
            let _ = child.wait();
        }
    }
}

pub fn spawn_direct_harness(plan: &ValidatedPlan, options: &LaunchOptions) -> Result<ChildHandle> {
    if plan.kind() != PlanKind::IntentionallyUnsandboxedHarness {
        return Err(LaunchError::Invalid("explicit host driver plan required"));
    }
    if options.stdio != StdioMode::Duplex || !options.working_directory.is_dir() {
        return Err(LaunchError::Invalid(
            "duplex stdio and existing cwd required",
        ));
    }
    let command = plan.command();
    if command.is_empty() || !Path::new(&command[0]).is_absolute() {
        return Err(LaunchError::Invalid("absolute executable required"));
    }
    let job = Arc::new(Job::new().map_err(ioerr("create direct harness job"))?);
    let mut process = Command::new(&command[0]);
    process
        .args(&command[1..])
        .current_dir(&options.working_directory)
        .envs(&options.environment)
        .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = process.spawn().map_err(ioerr("start direct harness"))?;
    if let Err(error) = attach(&job, &child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let init_pid = child.id();
    Ok(ChildHandle {
        child: Some(child),
        kill: KillHandle {
            job,
            alive: Arc::new(AtomicBool::new(true)),
        },
        init_pid,
    })
}

pub fn spawn(_plan: &ValidatedPlan, _options: &LaunchOptions) -> Result<ChildHandle> {
    Err(LaunchError::Unsupported(
        "Windows sandboxed sleeves are unavailable",
    ))
}
