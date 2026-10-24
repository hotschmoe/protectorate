//! Kill-on-close Windows Job Objects shared by every crate that launches a
//! process tree: create the child suspended, `assign` it, then `resume_process`.
use std::{io, mem, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
        JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

/// Closing the last handle kills every member. The handle is unnamed and not
/// inheritable, so only the creating process holds it.
pub struct Job(HANDLE);

// SAFETY: a job handle is a kernel object reference usable from any thread.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn new() -> io::Result<Self> {
        Self::with_limits(0, 0)
    }

    /// Zero means unlimited. The CPU cap is a hard percentage of the whole machine.
    pub fn with_limits(cpu_percent: u32, memory_bytes: u64) -> io::Result<Self> {
        // SAFETY: null attributes and name create an unnamed job whose handle is not inheritable.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(context("create job object", io::Error::last_os_error()));
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if memory_bytes > 0 {
            limits.JobMemoryLimit = usize::try_from(memory_bytes).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "memory limit exceeds platform size",
                )
            })?;
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
        }
        // SAFETY: the buffer and size match the extended limit information class.
        let applied = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
            )
        };
        if applied == 0 {
            return Err(context("limit job object", io::Error::last_os_error()));
        }
        if let Some(rate) = cpu_rate(
            cpu_percent,
            std::thread::available_parallelism().map_or(1, usize::from),
        ) {
            let mut cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE
                    | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
                ..Default::default()
            };
            cpu.Anonymous.CpuRate = rate;
            // SAFETY: the buffer and size match the CPU rate information class.
            let applied = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectCpuRateControlInformation,
                    (&cpu as *const JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                    mem::size_of_val(&cpu) as u32,
                )
            };
            if applied == 0 {
                return Err(context("cpu-limit job object", io::Error::last_os_error()));
            }
        }
        Ok(job)
    }

    /// # Safety
    /// `process` must be a live process handle with `PROCESS_SET_QUOTA |
    /// PROCESS_TERMINATE` access, still suspended so it cannot run or spawn
    /// before it joins.
    pub unsafe fn assign(&self, process: HANDLE) -> io::Result<()> {
        // SAFETY: the caller guarantees the handle; this job is live.
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(context("assign to job object", io::Error::last_os_error()));
        }
        Ok(())
    }

    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: this is our live job handle; termination applies to all members.
        if unsafe { TerminateJobObject(self.0, 1) } == 0 {
            return Err(context("terminate job object", io::Error::last_os_error()));
        }
        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the owned job handle is closed exactly once, killing the members.
        unsafe { CloseHandle(self.0) };
    }
}

/// Resumes every thread of a process created with `CREATE_SUSPENDED`.
///
/// # Safety
/// `process` must be a live process handle with `PROCESS_SUSPEND_RESUME` access.
pub unsafe fn resume_process(process: HANDLE) -> io::Result<()> {
    // SAFETY: the caller guarantees the handle.
    let status = unsafe { NtResumeProcess(process) };
    if status < 0 {
        return Err(io::Error::other(format!(
            "resume process: NTSTATUS {status:#x}"
        )));
    }
    Ok(())
}

fn context(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation}: {error}"))
}

fn cpu_rate(cpu_percent: u32, cores: usize) -> Option<u32> {
    (cpu_percent > 0 && cores > 0)
        .then(|| ((u64::from(cpu_percent) * 100) / cores as u64).clamp(1, 10_000) as u32)
}
