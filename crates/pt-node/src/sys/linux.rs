use super::{CpuInfo, OsInfo, parallelism};
use crate::clean;
use crate::exec;
use crate::sample::{CpuTimes, Mem};
use pt_frame::msg::{Disk, Gpu, Toolchain};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

#[allow(unsafe_code)]
mod ffi {
    use std::io;

    /// (total, free-to-unprivileged) bytes of the filesystem at `path`.
    pub fn statvfs(path: &str) -> io::Result<(u64, u64)> {
        let c = std::ffi::CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path holds a NUL"))?;
        let mut st = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `c` is a valid NUL-terminated string and `st` is a writable statvfs; on success
        // (return 0) the kernel has fully initialised it.
        let rc = unsafe { libc::statvfs(c.as_ptr(), st.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: statvfs returned 0, so the struct is initialised.
        let st = unsafe { st.assume_init() };
        #[allow(clippy::unnecessary_cast)]
        let (frsize, blocks, bavail) = (st.f_frsize as u64, st.f_blocks as u64, st.f_bavail as u64);
        Ok((blocks.saturating_mul(frsize), bavail.saturating_mul(frsize)))
    }

    pub fn set_nice(tid: u32, nice: i32) -> io::Result<()> {
        // SAFETY: setpriority takes plain integers and touches no memory of ours.
        let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Idle I/O scheduling class (class 3, data 0) for one thread.
    pub fn set_ioprio_idle(tid: u32) -> io::Result<()> {
        const IOPRIO_WHO_PROCESS: libc::c_long = 1;
        const IOPRIO_CLASS_IDLE_SHIFTED: libc::c_long = 3 << 13;
        // SAFETY: ioprio_set takes three plain integers and touches no memory of ours.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_ioprio_set,
                IOPRIO_WHO_PROCESS,
                libc::c_long::from(tid),
                IOPRIO_CLASS_IDLE_SHIFTED,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn read(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn hostname() -> String {
    read("/proc/sys/kernel/hostname")
        .or_else(|| read("/etc/hostname"))
        .unwrap_or_else(|| "unknown".into())
}

fn os_release_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix(key)?.strip_prefix('=')?;
        Some(v.trim().trim_matches('"').to_string())
    })
}

pub fn os() -> OsInfo {
    let text = read("/etc/os-release")
        .or_else(|| read("/usr/lib/os-release"))
        .unwrap_or_default();
    let name = os_release_field(&text, "NAME").unwrap_or_else(|| "Linux".into());
    let version = os_release_field(&text, "VERSION_ID")
        .or_else(|| os_release_field(&text, "BUILD_ID"))
        .unwrap_or_default();
    OsInfo {
        name,
        version,
        kernel: read("/proc/sys/kernel/osrelease").unwrap_or_default(),
    }
}

fn cpu_online(dir: &Path) -> bool {
    read(dir.join("online")).is_none_or(|v| v == "1")
}

pub fn cpu() -> CpuInfo {
    let mut ids = BTreeSet::new();
    let mut threads = 0u32;
    if let Ok(rd) = fs::read_dir("/sys/devices/system/cpu") {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_cpu = name
                .strip_prefix("cpu")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if !is_cpu || !cpu_online(&e.path()) {
                continue;
            }
            threads += 1;
            let topo = e.path().join("topology");
            let pkg = read(topo.join("physical_package_id")).unwrap_or_default();
            let core = read(topo.join("core_id")).unwrap_or(name.clone());
            ids.insert((pkg, core));
        }
    }
    if threads == 0 {
        threads = parallelism();
    }
    let cores = if ids.is_empty() {
        threads
    } else {
        ids.len() as u32
    };
    CpuInfo {
        model: cpu_model(),
        cores,
        threads,
    }
}

fn cpu_model() -> String {
    let info = read("/proc/cpuinfo").unwrap_or_default();
    for key in ["model name", "Model", "Hardware"] {
        let found = info.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key && !v.trim().is_empty()).then(|| v.trim().to_string())
        });
        if let Some(v) = found {
            return v;
        }
    }
    // Arm cores carry no name in cpuinfo; lscpu knows the part numbers.
    if let Some(out) = exec::run("lscpu", &[], Duration::from_secs(3)) {
        let mut names: Vec<String> = Vec::new();
        for l in out.stdout.lines() {
            if let Some(v) = l.strip_prefix("Model name:") {
                let v = v.trim().to_string();
                if !names.contains(&v) {
                    names.push(v);
                }
            }
        }
        if !names.is_empty() {
            return names.join(" + ");
        }
    }
    read("/sys/firmware/devicetree/base/model")
        .map(|s| s.trim_end_matches('\0').to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn meminfo() -> Vec<(String, u64)> {
    let text = read("/proc/meminfo").unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let kb: u64 = v.trim().trim_end_matches("kB").trim().parse().ok()?;
            Some((k.to_string(), kb.saturating_mul(1024)))
        })
        .collect()
}

fn mem_field(m: &[(String, u64)], key: &str) -> Option<u64> {
    m.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

pub fn ram_total() -> u64 {
    mem_field(&meminfo(), "MemTotal").unwrap_or(0)
}

pub fn memory() -> Option<Mem> {
    let m = meminfo();
    let total = mem_field(&m, "MemTotal")?;
    Some(Mem {
        total,
        available: mem_field(&m, "MemAvailable").or_else(|| mem_field(&m, "MemFree"))?,
        swap_total: mem_field(&m, "SwapTotal").unwrap_or(0),
        swap_free: mem_field(&m, "SwapFree").unwrap_or(0),
    })
}

pub fn cpu_times() -> Option<CpuTimes> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().next()?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .filter_map(|n| n.parse().ok())
        .collect();
    if v.len() < 5 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let idle = v[3] + v[4];
    Some(CpuTimes {
        busy: total.saturating_sub(idle),
        total,
    })
}

pub fn load_avg() -> Option<[f64; 3]> {
    let text = read("/proc/loadavg")?;
    let mut it = text.split_whitespace().take(3).map(|n| n.parse::<f64>());
    Some([it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?])
}

pub fn uptime() -> Option<u64> {
    let text = read("/proc/uptime")?;
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(secs as u64)
}

fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let n = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            out.push(n);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const NETWORK_FS: &[&str] = &["nfs", "nfs4", "cifs", "smb3", "zfs", "ceph", "fuse.sshfs"];
const SKIP_FS: &[&str] = &["squashfs", "iso9660", "udf", "overlay"];

pub fn disks() -> Vec<Disk> {
    let text = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mut seen_dev: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(mount), Some(fs_type)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let real = dev.starts_with("/dev/") && !dev.starts_with("/dev/loop");
        if SKIP_FS.contains(&fs_type) || !(real || NETWORK_FS.contains(&fs_type)) {
            continue;
        }
        let mount = unescape_mount(mount);
        // A btrfs/bind mount of one device repeats the same space; keep the first.
        if real && !seen_dev.insert(dev.to_string()) {
            continue;
        }
        let Ok((total, free)) = ffi::statvfs(&mount) else {
            continue;
        };
        if total == 0 {
            continue;
        }
        out.push(Disk {
            mount: clean(&mount, 4096),
            device: clean(&unescape_mount(dev), 128),
            fs: clean(fs_type, 32),
            total_bytes: total,
            free_bytes: free,
        });
    }
    out
}

pub fn extra_toolchains() -> Vec<Toolchain> {
    Vec::new()
}

fn pci_ids_lookup(vendor: u32, device: u32) -> (Option<String>, Option<String>) {
    let text = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
        .iter()
        .find_map(|p| fs::read_to_string(p).ok());
    let Some(text) = text else {
        return (None, None);
    };
    let vkey = format!("{vendor:04x}  ");
    let dkey = format!("\t{device:04x}  ");
    let mut vname = None;
    for line in text.lines() {
        if vname.is_none() {
            if let Some(n) = line.strip_prefix(&vkey) {
                vname = Some(n.trim().to_string());
            }
        } else if let Some(n) = line.strip_prefix(&dkey) {
            return (vname, Some(n.trim().to_string()));
        } else if !line.starts_with('\t') && !line.starts_with('#') && !line.is_empty() {
            break;
        }
    }
    (vname, None)
}

fn hex_file(p: impl AsRef<Path>) -> Option<u32> {
    let s = read(p)?;
    u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

fn vendor_fallback(id: u32) -> &'static str {
    match id {
        0x10de => "NVIDIA",
        0x1002 => "AMD",
        0x8086 => "Intel",
        0x1af4 => "Red Hat (virtio)",
        0x15ad => "VMware",
        0x1234 => "QEMU",
        0x1b36 => "Red Hat (QEMU)",
        0x1414 => "Microsoft",
        0x13b5 => "Arm",
        _ => "unknown",
    }
}

pub fn gpus() -> Vec<Gpu> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/class/drm") else {
        return out;
    };
    let mut cards: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    cards.sort_by_key(|n| n[4..].parse::<u32>().unwrap_or(0));
    for card in cards {
        let dev = Path::new("/sys/class/drm").join(&card).join("device");
        let driver = fs::read_link(dev.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let vram = read(dev.join("mem_info_vram_total")).and_then(|s| s.parse::<u64>().ok());
        let (vid, did) = (hex_file(dev.join("vendor")), hex_file(dev.join("device")));
        let (vendor, name) = match (vid, did) {
            (Some(v), Some(d)) => {
                let (vn, dn) = pci_ids_lookup(v, d);
                (
                    vn.unwrap_or_else(|| vendor_fallback(v).to_string()),
                    dn.unwrap_or_else(|| format!("{v:04x}:{d:04x}")),
                )
            }
            _ => {
                // Platform (SoC) GPU: no PCI ids, the driver name is all the kernel offers.
                let compat = read(dev.join("of_node/compatible"))
                    .map(|s| s.replace('\0', ","))
                    .map(|s| s.trim_end_matches(',').to_string());
                (
                    "platform".to_string(),
                    compat
                        .or_else(|| driver.clone())
                        .unwrap_or_else(|| card.clone()),
                )
            }
        };
        out.push(Gpu {
            vendor: clean(&vendor, 128),
            name: clean(&name, 128),
            vendor_id: vid,
            device_id: did,
            vram_bytes: vram,
            driver,
        });
    }
    fill_nvidia(&mut out);
    out
}

/// nvidia-smi, when present, adds the marketing name and VRAM the kernel does not expose.
fn fill_nvidia(gpus: &mut [Gpu]) {
    if !gpus.iter().any(|g| g.vendor_id == Some(0x10de)) || exec::which("nvidia-smi").is_none() {
        return;
    }
    let Some(out) = exec::run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ],
        Duration::from_secs(8),
    ) else {
        return;
    };
    let mut rows = out.stdout.lines();
    for g in gpus.iter_mut().filter(|g| g.vendor_id == Some(0x10de)) {
        let Some(row) = rows.next() else { break };
        let f: Vec<&str> = row.split(',').map(str::trim).collect();
        if f.len() == 3 {
            g.name = clean(f[0], 128);
            g.vram_bytes = f[1].parse::<u64>().ok().map(|mib| mib * 1024 * 1024);
            g.driver = Some(format!("nvidia {}", f[2]));
        }
    }
}

fn descendants(root: u32) -> Vec<u32> {
    let mut parent_of: Vec<(u32, u32)> = Vec::new();
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(e.path().join("stat")) else {
                continue;
            };
            // comm may hold spaces and parens; the fields after the last ')' are fixed.
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
                continue;
            };
            if let Some(ppid) = rest
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok())
            {
                parent_of.push((pid, ppid));
            }
        }
    }
    let mut all = vec![root];
    let mut i = 0;
    while i < all.len() {
        let p = all[i];
        for &(pid, ppid) in &parent_of {
            if ppid == p && !all.contains(&pid) {
                all.push(pid);
            }
        }
        i += 1;
    }
    all
}

pub fn lower_to_idle(pid: u32) -> io::Result<()> {
    if !Path::new(&format!("/proc/{pid}")).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("lower_to_idle: process {pid} does not exist"),
        ));
    }
    let mut first_err: Option<io::Error> = None;
    for p in descendants(pid) {
        let tids: Vec<u32> = fs::read_dir(format!("/proc/{p}/task"))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        for tid in tids {
            for (what, r) in [
                ("setpriority", ffi::set_nice(tid, 19)),
                ("ioprio_set", ffi::set_ioprio_idle(tid)),
            ] {
                match r {
                    Ok(()) => {}
                    Err(e) if p == pid && first_err.is_none() => {
                        first_err = Some(io::Error::new(
                            e.kind(),
                            format!("lower_to_idle: {what} on thread {tid} of {pid}: {e}"),
                        ));
                    }
                    Err(_) => {}
                }
            }
        }
    }
    first_err.map_or(Ok(()), Err)
}
