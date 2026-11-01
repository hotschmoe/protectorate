#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux_presence;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use linux::*;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use linux_presence::Platform;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux_job;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use linux_job::{Isolator, Proc};

#[cfg(windows)]
mod win32;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_presence;
#[cfg(windows)]
pub use windows::*;
#[cfg(windows)]
pub use windows_presence::Platform;
#[cfg(windows)]
mod windows_job;
#[cfg(windows)]
pub use windows_job::{Isolator, Proc};

pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel: String,
}

pub struct CpuInfo {
    pub model: String,
    pub cores: u32,
    pub threads: u32,
}

pub fn parallelism() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// What the node records about a spawned job, enough to find its processes after a restart.
#[derive(Clone, Debug)]
pub struct Receipt {
    pub pid: u32,
    /// Linux `/proc/<pid>/stat` start time, so a reused pid is never mistaken for the job.
    pub start_time: Option<u64>,
    pub scope: Option<String>,
}

/// A job found unfinished in the journal at startup.
pub enum Orphan {
    /// Its processes were still running and have been killed (never on Windows: kill-on-close).
    #[cfg_attr(windows, allow(dead_code))]
    Killed,
    /// Nothing of it runs any more.
    Gone,
}
