//! Windows process trees for node jobs. A job object owns every descendant,
//! but it does not change the process token or isolate the agent's files.
use std::{io, mem, os::windows::process::CommandExt, ptr};
use tokio::process::{Child, Command};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
            JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
            JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{
            OpenProcess, CREATE_NO_WINDOW, CREATE_SUSPENDED, PROCESS_SET_QUOTA,
            PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE,
        },
    },
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

pub(crate) struct ProcessTree {
    job: usize,
}

impl ProcessTree {
    pub(crate) fn spawn(
        command: &mut Command,
        cpu_percent: u32,
        memory_bytes: u64,
    ) -> io::Result<(Child, Self)> {
        let job = Self::new(cpu_percent, memory_bytes)?;
        command
            .as_std_mut()
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let mut child = command.spawn()?;
        let result = job.attach(
            child
                .id()
                .ok_or_else(|| io::Error::other("job process has no ID"))?,
        );
        if let Err(error) = result {
            let _ = child.start_kill();
            return Err(error);
        }
        Ok((child, job))
    }

    pub(crate) fn new(cpu_percent: u32, memory_bytes: u64) -> io::Result<Self> {
        // SAFETY: null attributes and name create an unshared, unnamed job.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self {
            job: handle as usize,
        };
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
        // SAFETY: info pointer and size match the requested job information class.
        let applied = unsafe {
            SetInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
            )
        };
        if applied == 0 {
            return Err(io::Error::last_os_error());
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
            // SAFETY: info pointer and size match the CPU rate information class.
            let applied = unsafe {
                SetInformationJobObject(
                    job.handle(),
                    JobObjectCpuRateControlInformation,
                    (&cpu as *const JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                    mem::size_of_val(&cpu) as u32,
                )
            };
            if applied == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(job)
    }

    fn attach(&self, pid: u32) -> io::Result<()> {
        // SAFETY: the PID is from the child just created suspended above.
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SUSPEND_RESUME,
                0,
                pid,
            )
        };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: both handles are valid; the child cannot run until resumed.
        let result = self
            .attach_process(process)
            .and_then(|_| resume_process(process));
        // SAFETY: OpenProcess gave this owned handle; the job retains its member.
        unsafe {
            CloseHandle(process);
        }
        result
    }

    pub(crate) fn attach_process(&self, process: HANDLE) -> io::Result<()> {
        // SAFETY: caller owns a suspended process and this valid job handle.
        if unsafe { AssignProcessToJobObject(self.handle(), process) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate(&self) -> io::Result<()> {
        // SAFETY: this is our live job handle and termination applies to all members.
        if unsafe { TerminateJobObject(self.handle(), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn handle(&self) -> HANDLE {
        self.job as HANDLE
    }
}

pub(crate) fn resume_process(process: HANDLE) -> io::Result<()> {
    // SAFETY: the process has already been assigned to a job object.
    let status = unsafe { NtResumeProcess(process) };
    if status < 0 {
        Err(io::Error::other(format!(
            "resume job process: NTSTATUS {status:#x}"
        )))
    } else {
        Ok(())
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        // SAFETY: closing the sole job handle invokes KILL_ON_JOB_CLOSE.
        unsafe {
            CloseHandle(self.handle());
        }
    }
}

fn cpu_rate(cpu_percent: u32, cores: usize) -> Option<u32> {
    (cpu_percent > 0 && cores > 0)
        .then(|| ((u64::from(cpu_percent) * 100) / cores as u64).clamp(1, 10_000) as u32)
}

#[cfg(test)]
mod tests {
    use super::{cpu_rate, ProcessTree};
    use std::{mem, ptr, time::Duration};
    use tokio::{
        io::{AsyncBufReadExt, BufReader},
        process::Command,
    };
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::JobObjects::{
            JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
            JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_JOB_MEMORY,
        },
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };

    #[test]
    fn cpu_limit_is_machine_wide_job_rate() {
        assert_eq!(cpu_rate(0, 8), None);
        assert_eq!(cpu_rate(100, 8), Some(1250));
        assert_eq!(cpu_rate(1, 128), Some(1));
        assert_eq!(cpu_rate(1000, 1), Some(10_000));
    }

    #[test]
    fn job_object_applies_cpu_and_memory_limits() {
        let memory = 256 << 20;
        let job = ProcessTree::new(100, memory).unwrap();
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: the buffer matches this job information class and the handle is live.
        let applied = unsafe {
            QueryInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                mem::size_of_val(&limits) as u32,
                ptr::null_mut(),
            )
        };
        assert_ne!(applied, 0);
        assert_ne!(
            limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_JOB_MEMORY,
            0
        );
        assert_eq!(limits.JobMemoryLimit, memory as usize);

        let mut cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        // SAFETY: the buffer matches this job information class and the handle is live.
        let applied = unsafe {
            QueryInformationJobObject(
                job.handle(),
                JobObjectCpuRateControlInformation,
                (&mut cpu as *mut JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                mem::size_of_val(&cpu) as u32,
                ptr::null_mut(),
            )
        };
        assert_ne!(applied, 0);
        assert_eq!(
            cpu.ControlFlags
                & (JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP),
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP
        );
        // SAFETY: ControlFlags enable the rate arm of the union.
        let actual = unsafe { cpu.Anonymous.CpuRate };
        assert_eq!(
            actual,
            cpu_rate(
                100,
                std::thread::available_parallelism().map_or(1, usize::from)
            )
            .unwrap()
        );
    }

    #[tokio::test]
    async fn job_termination_reaps_a_suspended_then_resumed_command() {
        let mut command = Command::new("cmd.exe");
        command.args(["/C", "ping -n 31 127.0.0.1 >NUL"]);
        command.kill_on_drop(true);
        let (mut child, job) = ProcessTree::spawn(&mut command, 0, 0).unwrap();
        job.terminate().unwrap();
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("job process did not exit")
            .unwrap();
    }

    #[tokio::test]
    async fn job_termination_reaps_descendants() {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile", "-Command",
            "$child = Start-Process ping.exe -ArgumentList '-n 31 127.0.0.1' -PassThru -WindowStyle Hidden; Write-Output $child.Id; Start-Sleep -Seconds 30",
        ]);
        command
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true);
        let (mut parent, job) = ProcessTree::spawn(&mut command, 0, 0).unwrap();
        let stdout = parent.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await
            .expect("descendant PID was not reported")
            .unwrap()
            .expect("no descendant PID");
        let pid: u32 = line.trim().parse().unwrap();
        // SAFETY: the PID came from Start-Process and this handle is used only to wait.
        let descendant = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(
            !descendant.is_null(),
            "descendant exited before cancellation"
        );
        job.terminate().unwrap();
        tokio::time::timeout(Duration::from_secs(5), parent.wait())
            .await
            .expect("parent did not exit")
            .unwrap();
        // SAFETY: OpenProcess returned an owned handle; 0 means it exited.
        let result = unsafe { WaitForSingleObject(descendant, 5000) };
        unsafe { CloseHandle(descendant) };
        assert_eq!(result, 0, "descendant survived job termination");
    }
}
