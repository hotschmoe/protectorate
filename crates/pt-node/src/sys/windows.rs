use super::{CpuInfo, OsInfo, parallelism, win32};
use crate::exec;
use crate::inventory::{Disk, Gpu, Toolchain};
use crate::json::clean;
use crate::sample::{CpuTimes, Mem};
use std::io;
use std::time::Duration;

const NT_CURRENT: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const GPU_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
}

pub fn os() -> OsInfo {
    let build: u64 = win32::hklm_string(NT_CURRENT, "CurrentBuild")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let ubr = win32::hklm_uint(NT_CURRENT, "UBR").unwrap_or(0);
    let mut name =
        win32::hklm_string(NT_CURRENT, "ProductName").unwrap_or_else(|| "Windows".into());
    // The registry keeps saying "Windows 10" on Windows 11.
    if build >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    let display = win32::hklm_string(NT_CURRENT, "DisplayVersion")
        .or_else(|| win32::hklm_string(NT_CURRENT, "ReleaseId"))
        .unwrap_or_default();
    OsInfo {
        name,
        version: format!("{display} (build {build}.{ubr})")
            .trim()
            .to_string(),
        kernel: format!("10.0.{build}.{ubr}"),
    }
}

pub fn cpu() -> CpuInfo {
    let threads = parallelism();
    CpuInfo {
        model: win32::hklm_string(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
        .unwrap_or_else(|| "unknown".into()),
        cores: win32::core_count().unwrap_or(threads),
        threads,
    }
}

pub fn ram_total() -> u64 {
    win32::memory().map_or(0, |m| m.total_phys)
}

pub fn memory() -> Option<Mem> {
    let m = win32::memory()?;
    Some(Mem {
        total: m.total_phys,
        available: m.avail_phys,
        swap_total: m.total_page.saturating_sub(m.total_phys),
        swap_free: m.avail_page.saturating_sub(m.avail_phys),
    })
}

pub fn cpu_times() -> Option<CpuTimes> {
    let (idle, kernel, user) = win32::system_times()?;
    let total = kernel.saturating_add(user);
    Some(CpuTimes {
        busy: total.saturating_sub(idle),
        total,
    })
}

pub fn load_avg() -> Option<[f64; 3]> {
    None
}

pub fn uptime() -> Option<u64> {
    Some(win32::uptime_ms() / 1000)
}

pub fn disks() -> Vec<Disk> {
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    win32::logical_drives()
        .into_iter()
        .filter(|root| matches!(win32::drive_type(root), DRIVE_FIXED | DRIVE_REMOTE))
        .filter_map(|root| {
            let (avail, total, _free) = win32::disk_space(&root)?;
            (total > 0).then(|| Disk {
                device: root.clone(),
                fs: win32::volume_fs(&root).unwrap_or_default(),
                mount: root,
                total_bytes: total,
                free_bytes: avail,
            })
        })
        .collect()
}

fn vswhere_property(exe: &str, prop: &str) -> Option<String> {
    let out = exec::run(
        exe,
        &["-latest", "-products", "*", "-property", prop],
        Duration::from_secs(10),
    )?;
    let line = out.stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.to_string())
}

/// MSVC is rarely on PATH from a service; vswhere finds the installation.
pub fn extra_toolchains() -> Vec<Toolchain> {
    let Some(base) = std::env::var_os("ProgramFiles(x86)") else {
        return Vec::new();
    };
    let exe = std::path::Path::new(&base).join(r"Microsoft Visual Studio\Installer\vswhere.exe");
    let Some(exe_s) = exe.to_str().filter(|_| exe.is_file()) else {
        return Vec::new();
    };
    let Some(version) = vswhere_property(exe_s, "installationVersion") else {
        return Vec::new();
    };
    let display = vswhere_property(exe_s, "displayName").unwrap_or_default();
    let path = vswhere_property(exe_s, "installationPath").unwrap_or_default();
    vec![Toolchain {
        name: "msvc".into(),
        version: clean(&format!("{display} {version}"), 128),
        path: clean(&path, 4096),
    }]
}

/// Display adapters from the registry class key; no WMI, no child process.
pub fn gpus() -> Vec<Gpu> {
    let mut out = Vec::new();
    for i in 0..32 {
        let key = format!(r"{GPU_CLASS}\{i:04}");
        let Some(desc) = win32::hklm_string(&key, "DriverDesc") else {
            continue;
        };
        let ids = win32::hklm_string(&key, "MatchingDeviceId").unwrap_or_default();
        out.push(Gpu {
            vendor: win32::hklm_string(&key, "ProviderName").unwrap_or_default(),
            name: clean(&desc, 128),
            vendor_id: hex_after(&ids, "VEN_"),
            device_id: hex_after(&ids, "DEV_"),
            vram_bytes: win32::hklm_uint(&key, "HardwareInformation.qwMemorySize")
                .or_else(|| win32::hklm_uint(&key, "HardwareInformation.MemorySize")),
            driver: win32::hklm_string(&key, "DriverVersion"),
        });
    }
    // The class key keeps ghost entries from replaced drivers: one adapter, the newest driver.
    let mut unique: Vec<Gpu> = Vec::new();
    for g in out {
        match unique
            .iter_mut()
            .find(|u| u.vendor_id == g.vendor_id && u.device_id == g.device_id && u.name == g.name)
        {
            Some(u) if version_key(&g.driver) > version_key(&u.driver) => *u = g,
            Some(_) => {}
            None => unique.push(g),
        }
    }
    unique
}

fn version_key(v: &Option<String>) -> Vec<u64> {
    v.as_deref()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn hex_after(s: &str, marker: &str) -> Option<u32> {
    let rest = &s[s.find(marker)? + marker.len()..];
    let hex: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
    u32::from_str_radix(&hex, 16).ok()
}

pub fn lower_to_idle(pid: u32) -> io::Result<()> {
    win32::set_idle_priority(pid)
        .map_err(|e| io::Error::new(e.kind(), format!("lower_to_idle: process {pid}: {e}")))
}
