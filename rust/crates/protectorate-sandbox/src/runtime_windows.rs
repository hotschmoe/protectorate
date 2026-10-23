//! Windows direct harness launch. Sandboxed workloads remain unsupported.
use crate::{PlanKind, ValidatedPlan};
use std::{
    collections::BTreeMap,
    io,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
    },
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

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

struct Job(HANDLE);
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Job {
    fn new() -> Result<Self> {
        // SAFETY: null attributes and name create a private unnamed Job Object.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(ioerr("create direct harness job")(
                io::Error::last_os_error(),
            ));
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the buffer and size match the selected information class.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(ioerr("limit direct harness job")(io::Error::last_os_error()));
        }
        Ok(job)
    }

    fn attach(&self, child: &Child) -> Result<()> {
        let process = child.as_raw_handle() as HANDLE;
        // SAFETY: the spawned process is suspended and both handles are live.
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(ioerr("assign direct harness job")(
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: this resumes only the suspended process assigned above.
        let status = unsafe { NtResumeProcess(process) };
        if status < 0 {
            return Err(LaunchError::Io {
                operation: "resume direct harness",
                source: io::Error::other(format!("NTSTATUS {status:#x}")),
            });
        }
        Ok(())
    }

    fn terminate(&self) {
        // SAFETY: this job is private to the direct harness process tree.
        unsafe { TerminateJobObject(self.0, 1) };
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the owned job handle is closed exactly once.
        unsafe { CloseHandle(self.0) };
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
            self.job.terminate();
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
    let job = Arc::new(Job::new()?);
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
    if let Err(error) = job.attach(&child) {
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
