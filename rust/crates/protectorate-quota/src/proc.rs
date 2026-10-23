//! Child-process plumbing shared by the CLI-backed readers.

use std::process::{Child, Command};

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

pub fn spawn(cmd: &mut Command) -> std::io::Result<ManagedChild> {
    configure(cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

        let job = Job::new()?;
        cmd.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let mut child = cmd.spawn()?;
        if let Err(error) = job.attach(&child) {
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
struct Job(usize);

#[cfg(windows)]
impl Job {
    fn new() -> std::io::Result<Self> {
        use std::{mem, ptr};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Self(handle as usize);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let applied = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
            )
        };
        if applied == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    fn attach(&self, child: &Child) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let process = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        if unsafe { AssignProcessToJobObject(self.0 as _, process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let result = unsafe { NtResumeProcess(process) };
        if result < 0 {
            return Err(std::io::Error::other(format!(
                "resume quota process failed: NTSTATUS {result:#x}"
            )));
        }
        Ok(())
    }

    fn terminate(&self) -> std::io::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        if unsafe { TerminateJobObject(self.0 as _, 1) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };

    #[test]
    fn kill_group_terminates_windows_child() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "proc::tests::sleeping_child", "--nocapture"])
            .env("PROTECTORATE_QUOTA_KILL_TEST_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = spawn(&mut command).unwrap();
        let output = std::io::BufReader::new(child.stdout.take().unwrap());
        assert!(
            output
                .lines()
                .any(|line| line.is_ok_and(|line| line == "ready")),
            "sleeping child never reached its test body"
        );
        kill_group(&mut child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let exited = child.try_wait().unwrap().is_some();
        if !exited {
            child.kill().unwrap();
            child.wait().unwrap();
        }
        assert!(exited, "kill_group did not terminate the child");
    }

    #[test]
    fn kill_group_terminates_windows_descendant() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "proc::tests::sleeping_child", "--nocapture"])
            .env("PROTECTORATE_QUOTA_KILL_TEST_DESCENDANT", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = spawn(&mut command).unwrap();
        let output = std::io::BufReader::new(child.stdout.take().unwrap());
        let pid: u32 = output
            .lines()
            .map_while(std::result::Result::ok)
            .find_map(|line| line.strip_prefix("grandchild=")?.parse().ok())
            .expect("descendant PID was not reported");
        let descendant = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!descendant.is_null(), "descendant exited before cleanup");
        kill_group(&mut child);
        child.wait().unwrap();
        let result = unsafe { WaitForSingleObject(descendant, 5000) };
        unsafe { CloseHandle(descendant) };
        assert_eq!(result, 0, "descendant survived quota process cleanup");
    }

    #[test]
    fn sleeping_child() {
        if std::env::var_os("PROTECTORATE_QUOTA_KILL_TEST_DESCENDANT").is_some() {
            let mut grandchild = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "proc::tests::sleeping_grandchild", "--nocapture"])
                .env("PROTECTORATE_QUOTA_KILL_TEST_GRANDCHILD", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            println!("grandchild={}", grandchild.id());
            let _ = grandchild.wait();
        }
        if std::env::var_os("PROTECTORATE_QUOTA_KILL_TEST_CHILD").is_some() {
            println!("ready");
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    #[test]
    fn sleeping_grandchild() {
        if std::env::var_os("PROTECTORATE_QUOTA_KILL_TEST_GRANDCHILD").is_some() {
            std::thread::sleep(Duration::from_secs(30));
        }
    }
}
