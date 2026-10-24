//! Child-process plumbing shared by the CLI-backed readers.

use std::io;
use std::process::{Child, Command, ExitStatus};

#[cfg(windows)]
use protectorate_sandbox::job::Job;

pub struct ManagedChild {
    child: Child,
    #[cfg(windows)]
    job: Job,
}

impl std::ops::Deref for ManagedChild {
    type Target = Child;

    fn deref(&self) -> &Child {
        &self.child
    }
}

impl std::ops::DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl ManagedChild {
    /// Whether the leader has exited. On Unix it is left unreaped, so its
    /// PID, which names the group, stays reserved until `reap_group`.
    pub fn exited(&mut self) -> bool {
        #[cfg(unix)]
        {
            exited_unreaped(self.child.id(), libc::WNOHANG).unwrap_or(true)
        }
        #[cfg(not(unix))]
        {
            !matches!(self.child.try_wait(), Ok(None))
        }
    }

    /// Waits for the leader, kills whatever is left of its group, then reaps
    /// it. Killing first would hit live work; reaping first would let the
    /// group kill reach a process that reused the PID.
    pub fn reap_group(&mut self) -> io::Result<ExitStatus> {
        #[cfg(unix)]
        exited_unreaped(self.child.id(), 0)?;
        #[cfg(not(unix))]
        self.child.wait()?;
        kill_group(self);
        self.child.wait()
    }
}

#[cfg(unix)]
fn exited_unreaped(pid: u32, flags: libc::c_int) -> io::Result<bool> {
    loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let flags = libc::WEXITED | libc::WNOWAIT | flags;
        if unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, flags) } == 0 {
            // WNOHANG leaves si_pid zero while the child is still running.
            return Ok(unsafe { info.si_pid() } != 0);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

pub fn spawn(cmd: &mut Command) -> std::io::Result<ManagedChild> {
    configure(cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

        let job = Job::new()?;
        cmd.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let mut child = cmd.spawn()?;
        if let Err(error) = attach(&job, &child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(ManagedChild { child, job })
    }
    #[cfg(not(windows))]
    {
        Ok(ManagedChild {
            child: cmd.spawn()?,
        })
    }
}

/// Puts the child in its own process group so wrapper scripts (the codex npm
/// shim) and their native child are terminated together.
pub fn configure(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

/// Terminates the child's whole group, including children that inherited stdout.
pub fn kill_group(child: &mut ManagedChild) {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        if child.job.terminate().is_err() {
            let _ = child.child.kill();
        }
    }
}

#[cfg(windows)]
fn attach(job: &Job, child: &Child) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    let process = child.as_raw_handle();
    // SAFETY: the child is suspended and its handle is live for both calls.
    unsafe {
        job.assign(process)?;
        protectorate_sandbox::job::resume_process(process)
            .map_err(|error| std::io::Error::other(format!("resume quota process failed: {error}")))
    }
}

/// Finds an executable on PATH, the way `exec.LookPath` does.
pub fn look_path(bin: &str) -> Option<std::path::PathBuf> {
    use std::fs;
    use std::path::Path;

    fn usable(p: &Path) -> Option<std::path::PathBuf> {
        let md = fs::metadata(p).ok()?;
        if !md.is_file() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if md.permissions().mode() & 0o111 == 0 {
                return None;
            }
        }
        Some(p.to_path_buf())
    }

    if bin.contains('/') || bin.contains('\\') {
        return usable(Path::new(bin));
    }
    for dir in std::env::split_paths(&std::env::var("PATH").unwrap_or_default()) {
        if let Some(p) = usable(&dir.join(bin)) {
            return Some(p);
        }
    }
    None
}
