//! Host measurements for node inventory. Linux and Windows probes read CPU,
//! memory, uptime and filesystems. Each section can fail without hiding the
//! rest of the snapshot; failures appear in sorted warnings.
//! Platform probes supply available accelerator and container measurements.
//! Sleeve measurements can be attached to the returned [`NodeMetrics`].

pub mod accel;
pub mod dirs;
pub mod docker;
#[cfg(windows)]
mod docker_pipe;
mod json;
pub mod linux;
pub mod model;
pub mod windows;

pub use json::metrics_json;
pub use model::*;

use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    /// user, nice, system, idle, iowait, irq, softirq, steal.
    pub values: [u64; 8],
}

impl CpuTimes {
    pub fn busy_percent_since(self, previous: Self) -> f64 {
        let total = self
            .values
            .iter()
            .sum::<u64>()
            .saturating_sub(previous.values.iter().sum());
        if total == 0 {
            return 0.0;
        }
        let idle = self.values[3].saturating_add(self.values[4]);
        let old_idle = previous.values[3].saturating_add(previous.values[4]);
        ((total.saturating_sub(idle.saturating_sub(old_idle))) as f64 / total as f64 * 100.0)
            .clamp(0.0, 100.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub hostname: String,
    pub platform: String,
    pub uptime_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub device: String,
    pub mount: String,
    pub fstype: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub used_percent: f64,
}

/// Replaceable system reads. `disk_usage` may block in a kernel call; the
/// collector isolates each mount and only allows one outstanding read of it.
pub trait Probe: Send + Sync + 'static {
    fn cpu_times(&self) -> io::Result<CpuTimes>;
    /// Three load averages. [`std::io::ErrorKind::Unsupported`] means this
    /// platform has none: the collector leaves the fields unset and adds no warning.
    fn load_average(&self) -> io::Result<[f64; 3]>;
    fn memory(&self) -> io::Result<MemoryMetrics>;
    fn host(&self) -> io::Result<HostInfo>;
    fn partitions(&self) -> io::Result<Vec<Partition>>;
    fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage>;
    fn accelerators(&self) -> io::Result<(Vec<GpuMetrics>, Vec<AcceleratorMetrics>)> {
        Ok((Vec::new(), Vec::new()))
    }
    fn engine(
        &self,
        _host: Option<&str>,
        _include_disk: bool,
    ) -> io::Result<Option<ContainerEngine>> {
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub version: String,
    pub min_age: Duration,
    pub initial_cpu_sample: Duration,
    pub filesystem_timeout: Duration,
    pub home: PathBuf,
    pub watch_paths: Vec<PathBuf>,
    pub slow_interval: Duration,
    pub dir_budget: Duration,
    pub docker_host: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            version: String::new(),
            min_age: Duration::from_secs(2),
            initial_cpu_sample: Duration::from_millis(250),
            filesystem_timeout: Duration::from_secs(5),
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            watch_paths: Vec::new(),
            slow_interval: Duration::from_secs(15 * 60),
            dir_budget: Duration::from_secs(60),
            docker_host: None,
        }
    }
}

struct State {
    last: Option<(Instant, NodeMetrics)>,
    cpu_previous: Option<CpuTimes>,
    watch_at: Option<Instant>,
    watch_dirs: Vec<DirSize>,
    disk_at: Option<Instant>,
    disk: Option<EngineDiskUsage>,
    disk_measured_at: Option<SystemTime>,
}

pub struct Collector<P: Probe = linux::LinuxProbe> {
    probe: Arc<P>,
    options: Options,
    os: &'static str,
    state: Mutex<State>,
    outstanding_mounts: Arc<Mutex<HashSet<String>>>,
}

impl Collector<linux::LinuxProbe> {
    pub fn linux(options: Options) -> Self {
        let mut collector = Self::new(linux::LinuxProbe::default(), options);
        if cfg!(target_os = "android") {
            collector.os = "android";
        }
        collector
    }
}

impl Collector<windows::WindowsProbe> {
    /// Host metrics from Windows system calls. Load averages stay unset.
    /// An empty home is filled from `USERPROFILE`. The system drive is the
    /// primary filesystem when it is present; the profile volume is the fallback.
    pub fn windows(mut options: Options) -> Self {
        if options.home.as_os_str().is_empty() {
            if let Some(profile) = std::env::var_os("USERPROFILE") {
                if !profile.is_empty() {
                    options.home = PathBuf::from(profile);
                }
            }
        }
        let mut collector = Self::new(windows::WindowsProbe, options);
        collector.os = "windows";
        collector
    }
}

impl<P: Probe> Collector<P> {
    pub fn new(probe: P, options: Options) -> Self {
        Self {
            probe: Arc::new(probe),
            options,
            os: "linux",
            state: Mutex::new(State {
                last: None,
                cpu_previous: None,
                watch_at: None,
                watch_dirs: Vec::new(),
                disk_at: None,
                disk: None,
                disk_measured_at: None,
            }),
            outstanding_mounts: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns a snapshot. Calls within `min_age` share the same sample.
    /// All errors except a poisoned state lock are reported as warnings.
    pub fn collect(&self) -> io::Result<NodeMetrics> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("collector state poisoned"))?;
        if let Some((at, snapshot)) = &state.last {
            if at.elapsed() < self.options.min_age {
                return Ok(snapshot.clone());
            }
        }
        let mut out = NodeMetrics {
            os: self.os.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            agent_version: self.options.version.clone(),
            collected_at: Some(SystemTime::now()),
            ..NodeMetrics::default()
        };
        match self.probe.host() {
            Ok(host) => {
                out.hostname = host.hostname;
                out.platform = host.platform;
                out.uptime_seconds = host.uptime_seconds;
            }
            Err(err) => out.warnings.push(format!("host: {err}")),
        }
        let mut cpu = CpuMetrics {
            logical_cores: std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(1),
            ..CpuMetrics::default()
        };
        match self.probe.load_average() {
            Ok([one, five, fifteen]) => {
                cpu.load1 = Some(one);
                cpu.load5 = Some(five);
                cpu.load15 = Some(fifteen);
            }
            Err(err) if err.kind() == io::ErrorKind::Unsupported => {}
            Err(err) => out.warnings.push(format!("cpu load: {err}")),
        }
        let mut previous = state.cpu_previous;
        if previous.is_none() {
            match self.probe.cpu_times() {
                Ok(now) => {
                    previous = Some(now);
                    if !self.options.initial_cpu_sample.is_zero() {
                        thread::sleep(self.options.initial_cpu_sample);
                    }
                }
                Err(err) => out.warnings.push(format!("cpu: {err}")),
            }
        }
        if let Some(before) = previous {
            match self.probe.cpu_times() {
                Ok(now) => {
                    cpu.usage_percent = now.busy_percent_since(before);
                    state.cpu_previous = Some(now);
                }
                Err(err) => out.warnings.push(format!("cpu: {err}")),
            }
        }
        out.cpu = Some(cpu);
        match self.probe.memory() {
            Ok(memory) => out.memory = Some(memory),
            Err(err) => out.warnings.push(format!("memory: {err}")),
        }
        match self.probe.accelerators() {
            Ok((gpus, npus)) => {
                out.gpus = gpus;
                out.npus = npus;
            }
            Err(err) => out.warnings.push(format!("accelerators: {err}")),
        }
        if !self.options.watch_paths.is_empty()
            && state
                .watch_at
                .is_none_or(|at| at.elapsed() >= self.options.slow_interval)
        {
            state.watch_dirs = self
                .options
                .watch_paths
                .iter()
                .map(|path| dirs::measure_dir(path, self.options.dir_budget))
                .collect();
            state.watch_at = Some(Instant::now());
        }
        out.watch_dirs = state.watch_dirs.clone();
        let include_disk = state
            .disk_at
            .is_none_or(|at| at.elapsed() >= self.options.slow_interval);
        match self
            .probe
            .engine(self.options.docker_host.as_deref(), include_disk)
        {
            Ok(Some(mut engine)) => {
                if include_disk {
                    if let Some(disk) = engine.disk.clone() {
                        state.disk = Some(disk);
                        state.disk_measured_at = engine.disk_measured_at;
                        state.disk_at = Some(Instant::now());
                    }
                } else {
                    engine.disk = state.disk.clone();
                    engine.disk_measured_at = state.disk_measured_at;
                    if let Some((_, previous)) = &state.last {
                        if let Some(old_engine) = &previous.engine {
                            for container in &mut engine.containers {
                                if let Some(old) = old_engine
                                    .containers
                                    .iter()
                                    .find(|old| old.id == container.id)
                                {
                                    container.writable_layer_bytes = old.writable_layer_bytes;
                                    container.rootfs_bytes = old.rootfs_bytes;
                                }
                            }
                        }
                    }
                }
                out.engine = Some(engine);
            }
            Ok(None) => {}
            Err(err) => out.warnings.push(format!("container engine: {err}")),
        }
        let partitions = match self.probe.partitions() {
            Ok(partitions) => partitions
                .into_iter()
                .filter(keep_partition)
                .collect::<Vec<_>>(),
            Err(err) => {
                out.warnings.push(format!("disk: {err}"));
                Vec::new()
            }
        };
        let partitions = if partitions.is_empty() {
            vec![fallback_partition(self.os)]
        } else {
            partitions
        };
        // Start every mount before waiting on any one of them. A dead network
        // mount then costs one timeout window, even when several are stuck.
        let results = thread::scope(|scope| {
            let handles = partitions
                .into_iter()
                .map(|part| scope.spawn(move || self.probe_filesystem(part)))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(io::Error::other("filesystem probe worker panicked"))
                    })
                })
                .collect::<Vec<_>>()
        });
        for result in results {
            match result {
                Ok(fs) => out.filesystems.push(fs),
                Err(err) => out.warnings.push(format!("disk: {err}")),
            }
        }
        dedupe_filesystems(&mut out.filesystems);
        if self.os == "windows" {
            let drive = std::env::var("SystemDrive").unwrap_or_default();
            let home = self.options.home.to_string_lossy();
            windows::mark_windows_primary(&mut out.filesystems, &drive, home.as_ref());
        } else {
            mark_primary(&mut out.filesystems, &self.options.home);
        }
        out.warnings.sort();
        state.last = Some((Instant::now(), out.clone()));
        Ok(out)
    }

    fn probe_filesystem(&self, part: Partition) -> io::Result<FilesystemMetrics> {
        let mount = part.mount.clone();
        {
            let mut active = self
                .outstanding_mounts
                .lock()
                .map_err(|_| io::Error::other("filesystem state poisoned"))?;
            if !active.insert(mount.clone()) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{mount}: stale, previous probe still blocked"),
                ));
            }
        }
        let probe = self.probe.clone();
        let active = self.outstanding_mounts.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = probe.disk_usage(Path::new(&mount));
            if let Ok(mut mounts) = active.lock() {
                mounts.remove(&mount);
            }
            let _ = sender.send(result);
        });
        let usage = receiver
            .recv_timeout(self.options.filesystem_timeout)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{}: timed out", part.mount),
                )
            })??;
        Ok(FilesystemMetrics {
            mount: part.mount,
            device: part.device,
            fstype: part.fstype,
            total_bytes: usage.total_bytes,
            used_bytes: usage.used_bytes,
            free_bytes: usage.free_bytes,
            used_percent: usage.used_percent,
            primary: false,
        })
    }
}

fn fallback_partition(os: &str) -> Partition {
    let mount = if os == "windows" {
        windows::windows_root(&std::env::var("SystemDrive").unwrap_or_default())
    } else {
        "/".to_owned()
    };
    Partition {
        device: mount.clone(),
        mount,
        fstype: "unknown".into(),
    }
}

pub fn keep_partition(part: &Partition) -> bool {
    const PSEUDO: &[&str] = &[
        "tmpfs",
        "devtmpfs",
        "squashfs",
        "ramfs",
        "proc",
        "sysfs",
        "cgroup",
        "cgroup2",
        "devpts",
        "efivarfs",
        "iso9660",
        "udf",
        "fuse.portal",
        "fuse.gvfsd-fuse",
        "autofs",
        "binfmt_misc",
        "bpf",
        "tracefs",
        "debugfs",
        "securityfs",
        "pstore",
        "configfs",
        "mqueue",
        "hugetlbfs",
        "nsfs",
        "fusectl",
        "rpc_pipefs",
    ];
    if part.fstype.is_empty()
        || PSEUDO.contains(&part.fstype.to_ascii_lowercase().as_str())
        || (part.fstype == "overlay" && part.mount != "/")
    {
        return false;
    }
    ![
        "/etc/",
        "/snap/",
        "/var/lib/docker/",
        "/var/lib/containers/",
        "/run/",
        "/proc",
        "/sys",
    ]
    .iter()
    .any(|prefix| part.mount.starts_with(prefix))
}

pub fn dedupe_filesystems(filesystems: &mut Vec<FilesystemMetrics>) {
    filesystems.sort_by(|a, b| {
        a.mount
            .len()
            .cmp(&b.mount.len())
            .then(a.mount.cmp(&b.mount))
    });
    let mut seen = HashSet::new();
    filesystems.retain(|fs| fs.total_bytes > 0 && seen.insert((fs.device.clone(), fs.total_bytes)));
    filesystems.sort_by(|a, b| a.mount.cmp(&b.mount));
}

pub fn mark_primary(filesystems: &mut [FilesystemMetrics], home: &Path) {
    let root = holding(filesystems, Path::new("/"));
    let chosen = holding(filesystems, home)
        .filter(|&i| root.is_none_or(|r| filesystems[i].total_bytes > filesystems[r].total_bytes))
        .or(root);
    if let Some(index) = chosen {
        filesystems[index].primary = true;
    }
}

fn holding(filesystems: &[FilesystemMetrics], path: &Path) -> Option<usize> {
    filesystems
        .iter()
        .enumerate()
        .filter(|(_, fs)| path.starts_with(&fs.mount))
        .max_by_key(|(_, fs)| fs.mount.len())
        .map(|(index, _)| index)
}

#[cfg(test)]
mod windows_collect_tests {
    use super::*;
    use std::{io, path::Path, time::Duration};

    struct Drives;

    impl Probe for Drives {
        fn cpu_times(&self) -> io::Result<CpuTimes> {
            Ok(CpuTimes {
                values: [10, 0, 10, 80, 0, 0, 0, 0],
            })
        }
        fn load_average(&self) -> io::Result<[f64; 3]> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "load averages unavailable on Windows",
            ))
        }
        fn memory(&self) -> io::Result<MemoryMetrics> {
            Ok(memory_sample())
        }
        fn host(&self) -> io::Result<HostInfo> {
            Ok(HostInfo {
                hostname: "win".into(),
                platform: "windows 10.0.22631".into(),
                uptime_seconds: 9,
            })
        }
        fn partitions(&self) -> io::Result<Vec<Partition>> {
            Ok(vec![drive(r"C:\"), drive(r"D:\")])
        }
        fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
            let total = if mount == Path::new(r"D:\") {
                2_000
            } else {
                100
            };
            Ok(DiskUsage {
                total_bytes: total,
                used_bytes: total / 2,
                free_bytes: total / 2,
                used_percent: 50.0,
            })
        }
    }

    struct NoDrives;

    impl Probe for NoDrives {
        fn cpu_times(&self) -> io::Result<CpuTimes> {
            Ok(CpuTimes::default())
        }
        fn load_average(&self) -> io::Result<[f64; 3]> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "no load"))
        }
        fn memory(&self) -> io::Result<MemoryMetrics> {
            Ok(MemoryMetrics::default())
        }
        fn host(&self) -> io::Result<HostInfo> {
            Ok(HostInfo::default())
        }
        fn partitions(&self) -> io::Result<Vec<Partition>> {
            Ok(Vec::new())
        }
        fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
            Err(io::Error::other(format!("probed {}", mount.display())))
        }
    }

    fn drive(mount: &str) -> Partition {
        Partition {
            device: mount.into(),
            mount: mount.into(),
            fstype: "NTFS".into(),
        }
    }

    fn memory_sample() -> MemoryMetrics {
        MemoryMetrics {
            total_bytes: 100,
            available_bytes: 40,
            used_bytes: 60,
            used_percent: 60.0,
            ..MemoryMetrics::default()
        }
    }

    #[test]
    fn windows_os_uses_system_drive_primary_and_skips_load() {
        let home = r"D:\Users\alice";
        let mut collector = Collector::new(
            Drives,
            Options {
                min_age: Duration::from_secs(30),
                initial_cpu_sample: Duration::ZERO,
                home: home.into(),
                ..Options::default()
            },
        );
        collector.os = "windows";
        let snapshot = collector.collect().unwrap();
        assert_eq!(snapshot.os, "windows");
        assert_eq!(snapshot.platform, "windows 10.0.22631");
        assert_eq!(snapshot.uptime_seconds, 9);
        let cpu = snapshot.cpu.unwrap();
        assert!(cpu.load1.is_none() && cpu.load5.is_none() && cpu.load15.is_none());
        assert!(snapshot
            .warnings
            .iter()
            .all(|warning| !warning.starts_with("cpu load:")));
        let mut expected = snapshot.filesystems.clone();
        for fs in &mut expected {
            fs.primary = false;
        }
        let drive = std::env::var("SystemDrive").unwrap_or_default();
        windows::mark_windows_primary(&mut expected, &drive, home);
        assert_eq!(snapshot.filesystems, expected);
        assert!(snapshot.filesystems.iter().any(|fs| fs.primary));
    }

    #[test]
    fn empty_windows_drive_list_falls_back_to_system_drive() {
        let mut collector = Collector::new(
            NoDrives,
            Options {
                initial_cpu_sample: Duration::ZERO,
                filesystem_timeout: Duration::from_millis(50),
                ..Options::default()
            },
        );
        collector.os = "windows";
        let snapshot = collector.collect().unwrap();
        let expected = windows::windows_root(&std::env::var("SystemDrive").unwrap_or_default());
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains(&format!("probed {expected}"))));
    }
}
