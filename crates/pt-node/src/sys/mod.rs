mod info;
mod receipt;

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
mod win32_proc;
#[cfg(windows)]
mod win32_service;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_presence;
#[cfg(windows)]
pub use windows::*;
#[cfg(windows)]
pub use windows_presence::{Platform, idle_probe};
#[cfg(windows)]
mod windows_job;
pub use info::{CpuTimes, Mem};
pub use receipt::{Orphan, Receipt};
#[cfg(windows)]
pub use win32::Job;
#[cfg(windows)]
pub use win32_service::{redirect_stdio, run_service, spawn_in_job};
#[cfg(windows)]
pub use windows_job::{Isolator, Proc};
