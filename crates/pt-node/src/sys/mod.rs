#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux_presence;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use linux::*;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use linux_presence::Platform;

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
