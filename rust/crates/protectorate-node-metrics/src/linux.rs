use crate::{accel, CpuTimes, DiskUsage, HostInfo, Partition, Probe};
use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct LinuxProbe {
    pub proc_root: std::path::PathBuf,
    pub sys_root: std::path::PathBuf,
    pub os_release: std::path::PathBuf,
    pub debian_version: std::path::PathBuf,
    /// Vendor accelerator tools, run only when sysfs reports such a card.
    pub tools: Arc<dyn accel::ToolRunner>,
    /// Previous cumulative sysfs counter readings: a rate needs two of them.
    pub accel: Arc<Mutex<accel::AccelState>>,
}

impl std::fmt::Debug for LinuxProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxProbe")
            .field("proc_root", &self.proc_root)
            .field("sys_root", &self.sys_root)
            .finish()
    }
}

impl Default for LinuxProbe {
    fn default() -> Self {
        Self {
            proc_root: "/proc".into(),
            sys_root: "/sys".into(),
            os_release: "/etc/os-release".into(),
            debian_version: "/etc/debian_version".into(),
            tools: Arc::new(accel::SystemTools::default()),
            accel: Arc::new(Mutex::new(accel::AccelState::default())),
        }
    }
}

impl LinuxProbe {
    fn read_proc(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(self.proc_root.join(name))
    }
}

impl Probe for LinuxProbe {
    fn engine(
        &self,
        host: Option<&str>,
        include_disk: bool,
    ) -> io::Result<Option<crate::ContainerEngine>> {
        crate::docker::probe(host, include_disk)
    }
    fn accelerators(&self) -> io::Result<(Vec<crate::GpuMetrics>, Vec<crate::AcceleratorMetrics>)> {
        let mut state = self
            .accel
            .lock()
            .map_err(|_| io::Error::other("accelerator state poisoned"))?;
        accel::probe_with_tools(&self.sys_root, &mut state, self.tools.as_ref())
    }
    fn cpu_times(&self) -> io::Result<CpuTimes> {
        parse_cpu_times(&self.read_proc("stat")?)
    }

    fn load_average(&self) -> io::Result<[f64; 3]> {
        parse_load_average(&self.read_proc("loadavg")?)
    }

    fn memory(&self) -> io::Result<crate::model::MemoryMetrics> {
        parse_memory(&self.read_proc("meminfo")?)
    }

    fn host(&self) -> io::Result<HostInfo> {
        let mut info = HostInfo {
            hostname: kernel_hostname().unwrap_or_default(),
            platform: std::env::consts::OS.to_owned(),
            uptime_seconds: 0,
        };
        if let Ok(raw) = fs::read_to_string(&self.os_release) {
            let mut version = String::new();
            for line in raw.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    let value = value.trim_matches(['"', '\'']);
                    match key {
                        "ID" => info.platform = value.to_owned(),
                        "VERSION_ID" => version = value.to_owned(),
                        _ => {}
                    }
                }
            }
            if info.platform == "debian" {
                if let Ok(v) = fs::read_to_string(&self.debian_version) {
                    version = v.trim().to_owned();
                }
            }
            if !version.is_empty() {
                info.platform.push(' ');
                info.platform.push_str(&version);
            }
        }
        info.uptime_seconds = parse_uptime(&self.read_proc("uptime")?)?;
        Ok(info)
    }

    fn partitions(&self) -> io::Result<Vec<Partition>> {
        Ok(parse_mounts(&self.read_proc("self/mounts")?))
    }

    fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::CString::new(mount.as_os_str().as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in mount path"))?;
            let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            // SAFETY: name is NUL terminated and stat points at writable storage.
            let rc = unsafe { libc::statvfs(name.as_ptr(), stat.as_mut_ptr()) };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: a successful statvfs initializes the entire structure.
            let stat = unsafe { stat.assume_init() };
            let size = stat.f_frsize;
            let blocks = stat.f_blocks;
            let free = stat.f_bfree;
            let available = stat.f_bavail;
            let used = blocks.saturating_sub(free).saturating_mul(size);
            let available = available.saturating_mul(size);
            let denominator = used.saturating_add(available);
            Ok(DiskUsage {
                total_bytes: blocks.saturating_mul(size),
                used_bytes: used,
                free_bytes: available,
                used_percent: if denominator == 0 {
                    0.0
                } else {
                    used as f64 / denominator as f64 * 100.0
                },
            })
        }
        #[cfg(not(unix))]
        {
            let _ = mount;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Linux disk probing requires Unix",
            ))
        }
    }
}

fn kernel_hostname() -> io::Result<String> {
    #[cfg(unix)]
    {
        let mut buffer = [0_u8; 256];
        // SAFETY: buffer is writable and its length is passed to gethostname.
        let rc = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        let end = buffer
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(buffer.len());
        Ok(String::from_utf8_lossy(&buffer[..end]).into_owned())
    }
    #[cfg(not(unix))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel hostname requires Unix",
        ))
    }
}

pub fn parse_cpu_times(raw: &str) -> io::Result<CpuTimes> {
    let mut words = raw.lines().next().unwrap_or_default().split_whitespace();
    if words.next() != Some("cpu") {
        return Err(invalid("invalid /proc/stat CPU counters"));
    }
    let mut values = [0_u64; 8];
    let mut count = 0;
    for (index, word) in words.take(8).enumerate() {
        values[index] = word.parse().map_err(|_| invalid("invalid CPU counter"))?;
        count += 1;
    }
    if count < 4 {
        return Err(invalid("missing /proc/stat CPU counters"));
    }
    Ok(CpuTimes { values })
}

pub fn parse_load_average(raw: &str) -> io::Result<[f64; 3]> {
    let mut values = [0.0; 3];
    let words: Vec<_> = raw.split_whitespace().take(3).collect();
    if words.len() != 3 {
        return Err(invalid("missing load averages"));
    }
    for (index, word) in words.iter().enumerate() {
        values[index] = word.parse().map_err(|_| invalid("invalid load average"))?;
    }
    Ok(values)
}

pub fn parse_memory(raw: &str) -> io::Result<crate::model::MemoryMetrics> {
    let mut values = std::collections::HashMap::new();
    for line in raw.lines() {
        let mut fields = line.split_whitespace();
        let (Some(key), Some(value)) = (fields.next(), fields.next()) else {
            continue;
        };
        let mut value: u64 = value
            .parse()
            .map_err(|_| invalid("invalid memory counter"))?;
        if fields.next() == Some("kB") {
            value = value
                .checked_mul(1024)
                .ok_or_else(|| invalid("memory counter overflow"))?;
        }
        values.insert(key.trim_end_matches(':'), value);
    }
    let get = |key| *values.get(key).unwrap_or(&0_u64);
    let total = get("MemTotal");
    if total == 0 {
        return Err(invalid("missing MemTotal in /proc/meminfo"));
    }
    let available = if let Some(value) = values.get("MemAvailable") {
        *value
    } else {
        get("MemFree")
            .saturating_add(get("Buffers"))
            .saturating_add(get("Cached"))
            .saturating_add(get("SReclaimable"))
            .saturating_sub(get("Shmem"))
    }
    .min(total);
    let used = total - available;
    let swap_total = get("SwapTotal");
    let swap_used = swap_total.saturating_sub(get("SwapFree"));
    Ok(crate::model::MemoryMetrics {
        total_bytes: total,
        used_bytes: used,
        available_bytes: available,
        used_percent: used as f64 / total as f64 * 100.0,
        swap_total_bytes: swap_total,
        swap_used_bytes: swap_used,
        swap_used_percent: if swap_total == 0 {
            0.0
        } else {
            (swap_used as f64 / swap_total as f64 * 100.0).clamp(0.0, 100.0)
        },
    })
}

pub fn parse_uptime(raw: &str) -> io::Result<i64> {
    let first = raw
        .split_whitespace()
        .next()
        .ok_or_else(|| invalid("empty /proc/uptime"))?;
    let value: f64 = first.parse().map_err(|_| invalid("invalid uptime"))?;
    if !value.is_finite() || value < 0.0 || value > i64::MAX as f64 {
        return Err(invalid("invalid uptime"));
    }
    Ok(value as i64)
}

pub fn parse_mounts(raw: &str) -> Vec<Partition> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(Partition {
                device: unescape_mount(fields.next()?),
                mount: unescape_mount(fields.next()?),
                fstype: fields.next()?.to_owned(),
            })
        })
        .collect()
}

fn unescape_mount(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
