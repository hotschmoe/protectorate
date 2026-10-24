//! Windows process trees for node jobs. A job object owns every descendant,
//! but it does not change the process token or isolate the agent's files.
use protectorate_sandbox::job::{resume_process, Job};
use std::{io, os::windows::process::CommandExt};
use tokio::process::{Child, Command};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

pub(crate) fn spawn(
    command: &mut Command,
    cpu_percent: u32,
    memory_bytes: u64,
) -> io::Result<(Child, Job)> {
    let job = Job::with_limits(cpu_percent, memory_bytes)?;
    command
        .as_std_mut()
        .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
    let mut child = command.spawn()?;
    let attached = child
        .raw_handle()
        .ok_or_else(|| io::Error::other("job process has no handle"))
        .and_then(|process| {
            // SAFETY: the child is suspended and its handle stays live inside `child`.
            unsafe { job.assign(process).and_then(|_| resume_process(process)) }
        });
    if let Err(error) = attached {
        let _ = child.start_kill();
        return Err(error);
    }
    Ok((child, job))
}
