use protectorate_node_metrics::{
    linux::{parse_cpu_times, parse_memory, parse_mounts},
    windows::{
        cpu_times_from_system, disk_from_space, mark_windows_primary, memory_from_counters,
        platform_name, split_multi_sz, uptime_from_ticks, windows_root,
    },
    Collector, CpuTimes, DiskUsage, FilesystemMetrics, HostInfo, MemoryMetrics, Options, Partition,
    Probe,
};
use std::{
    io,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

#[test]
fn parses_linux_counters_and_mount_escapes() {
    let cpu = parse_cpu_times("cpu  10 1 5 84 2 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
    assert_eq!(cpu.values[3], 84);
    let memory = parse_memory("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 20 kB\nCached: 30 kB\nSReclaimable: 10 kB\nShmem: 15 kB\nSwapTotal: 200 kB\nSwapFree: 50 kB\n").unwrap();
    assert_eq!(memory.available_bytes, 145 * 1024);
    assert_eq!(memory.swap_used_bytes, 150 * 1024);
    let parts = parse_mounts("/dev/foo /media/my\\040disk ext4 rw 0 0\n");
    assert_eq!(parts[0].mount, "/media/my disk");
}

#[test]
fn cpu_usage_tracks_delta_and_clamps_counter_reset() {
    let first = CpuTimes {
        values: [10, 0, 10, 80, 0, 0, 0, 0],
    };
    let second = CpuTimes {
        values: [40, 0, 20, 140, 0, 0, 0, 0],
    };
    assert_eq!(second.busy_percent_since(first), 40.0);
    assert_eq!(first.busy_percent_since(second), 0.0);
}

#[derive(Default)]
struct Fake {
    reads: AtomicUsize,
    load_fault: bool,
}

impl Probe for Fake {
    fn cpu_times(&self) -> io::Result<CpuTimes> {
        let index = self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(CpuTimes {
            values: if index == 0 {
                [0, 0, 0, 100, 0, 0, 0, 0]
            } else {
                [30, 0, 10, 160, 0, 0, 0, 0]
            },
        })
    }
    fn load_average(&self) -> io::Result<[f64; 3]> {
        if self.load_fault {
            Err(io::Error::other("test load fault"))
        } else {
            Ok([1.0, 2.0, 3.0])
        }
    }
    fn memory(&self) -> io::Result<MemoryMetrics> {
        Err(io::Error::other("test memory fault"))
    }
    fn host(&self) -> io::Result<HostInfo> {
        Ok(HostInfo {
            hostname: "node".into(),
            platform: "test linux".into(),
            uptime_seconds: 12,
        })
    }
    fn partitions(&self) -> io::Result<Vec<Partition>> {
        Ok(vec![
            Partition {
                device: "root".into(),
                mount: "/".into(),
                fstype: "ext4".into(),
            },
            Partition {
                device: "root".into(),
                mount: "/bind".into(),
                fstype: "ext4".into(),
            },
            Partition {
                device: "data".into(),
                mount: "/home".into(),
                fstype: "xfs".into(),
            },
            Partition {
                device: "tmp".into(),
                mount: "/run/thing".into(),
                fstype: "tmpfs".into(),
            },
        ])
    }
    fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
        let total = if mount == Path::new("/home") {
            200
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

#[test]
fn collects_partial_snapshot_dedupes_and_caches() {
    let options = Options {
        min_age: Duration::from_secs(30),
        initial_cpu_sample: Duration::ZERO,
        home: "/home/alice".into(),
        ..Options::default()
    };
    let collector = Collector::new(Fake::default(), options);
    let first = collector.collect().unwrap();
    assert_eq!(first.os, "linux");
    assert_eq!(first.hostname, "node");
    assert_eq!(first.cpu.as_ref().unwrap().usage_percent, 40.0);
    assert_eq!(first.warnings, ["memory: test memory fault"]);
    assert_eq!(first.filesystems.len(), 2);
    assert!(
        first
            .filesystems
            .iter()
            .find(|fs| fs.mount == "/home")
            .unwrap()
            .primary
    );
    let second = collector.collect().unwrap();
    assert_eq!(first.collected_at, second.collected_at);
}

#[test]
fn load_average_failure_is_reported_without_dropping_cpu() {
    let collector = Collector::new(
        Fake {
            load_fault: true,
            ..Fake::default()
        },
        Options {
            initial_cpu_sample: Duration::ZERO,
            ..Options::default()
        },
    );
    let snapshot = collector.collect().unwrap();
    assert_eq!(snapshot.cpu.as_ref().unwrap().usage_percent, 40.0);
    assert!(snapshot
        .warnings
        .contains(&"cpu load: test load fault".to_owned()));
}

#[test]
fn live_linux_probe_has_cpu_memory_and_a_primary_disk() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let collector = Collector::linux(Options {
        initial_cpu_sample: Duration::from_millis(10),
        ..Options::default()
    });
    let snapshot = collector.collect().unwrap();
    assert!(snapshot.cpu.is_some(), "{:?}", snapshot.warnings);
    assert!(snapshot.memory.is_some(), "{:?}", snapshot.warnings);
    assert!(!snapshot.filesystems.is_empty(), "{:?}", snapshot.warnings);
    assert!(snapshot.filesystems.iter().any(|fs| fs.primary));
}

struct HangingDisk {
    gate: Arc<(Mutex<bool>, Condvar)>,
    reads: Arc<AtomicUsize>,
}

impl Probe for HangingDisk {
    fn cpu_times(&self) -> io::Result<CpuTimes> {
        Ok(CpuTimes {
            values: [10, 0, 0, 90, 0, 0, 0, 0],
        })
    }
    fn load_average(&self) -> io::Result<[f64; 3]> {
        Ok([0.0; 3])
    }
    fn memory(&self) -> io::Result<MemoryMetrics> {
        Ok(MemoryMetrics::default())
    }
    fn host(&self) -> io::Result<HostInfo> {
        Ok(HostInfo::default())
    }
    fn partitions(&self) -> io::Result<Vec<Partition>> {
        Ok(vec![Partition {
            device: "hung".into(),
            mount: "/hung".into(),
            fstype: "ext4".into(),
        }])
    }
    fn disk_usage(&self, _: &Path) -> io::Result<DiskUsage> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let (lock, signal) = &*self.gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = signal.wait(released).unwrap();
        }
        Ok(DiskUsage {
            total_bytes: 100,
            ..DiskUsage::default()
        })
    }
}

#[test]
fn timed_out_mount_is_not_probed_again_while_blocked() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let reads = Arc::new(AtomicUsize::new(0));
    let collector = Collector::new(
        HangingDisk {
            gate: gate.clone(),
            reads: reads.clone(),
        },
        Options {
            min_age: Duration::ZERO,
            initial_cpu_sample: Duration::ZERO,
            filesystem_timeout: Duration::from_millis(20),
            ..Options::default()
        },
    );
    let first = collector.collect().unwrap();
    assert!(first
        .warnings
        .iter()
        .any(|warning| warning.contains("timed out")));
    let second = collector.collect().unwrap();
    assert!(second
        .warnings
        .iter()
        .any(|warning| warning.contains("previous probe still blocked")));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    let (lock, signal) = &*gate;
    *lock.lock().unwrap() = true;
    signal.notify_all();
}

struct TwoHangingDisks(HangingDisk);

impl Probe for TwoHangingDisks {
    fn cpu_times(&self) -> io::Result<CpuTimes> {
        self.0.cpu_times()
    }
    fn load_average(&self) -> io::Result<[f64; 3]> {
        self.0.load_average()
    }
    fn memory(&self) -> io::Result<MemoryMetrics> {
        self.0.memory()
    }
    fn host(&self) -> io::Result<HostInfo> {
        self.0.host()
    }
    fn partitions(&self) -> io::Result<Vec<Partition>> {
        Ok(["/hung-a", "/hung-b"]
            .into_iter()
            .map(|mount| Partition {
                device: mount.into(),
                mount: mount.into(),
                fstype: "ext4".into(),
            })
            .collect())
    }
    fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
        self.0.disk_usage(mount)
    }
}

#[test]
fn two_hung_mounts_share_one_timeout_window() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let reads = Arc::new(AtomicUsize::new(0));
    let timeout = Duration::from_millis(300);
    let collector = Collector::new(
        TwoHangingDisks(HangingDisk {
            gate: gate.clone(),
            reads: reads.clone(),
        }),
        Options {
            min_age: Duration::ZERO,
            initial_cpu_sample: Duration::ZERO,
            filesystem_timeout: timeout,
            ..Options::default()
        },
    );
    let started = Instant::now();
    let snapshot = collector.collect().unwrap();
    let elapsed = started.elapsed();
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert_eq!(
        snapshot
            .warnings
            .iter()
            .filter(|w| w.contains("timed out"))
            .count(),
        2
    );
    assert!(
        elapsed < Duration::from_millis(520),
        "two mount timeouts took {elapsed:?}"
    );
    let (lock, signal) = &*gate;
    *lock.lock().unwrap() = true;
    signal.notify_all();
}

#[test]
fn windows_counter_conversion_matches_published_fields() {
    let first = cpu_times_from_system(1_000, 1_400, 200);
    assert_eq!(first.values, [200, 0, 400, 1_000, 0, 0, 0, 0]);
    let second = cpu_times_from_system(1_500, 2_200, 500);
    let busy = second.busy_percent_since(first);
    assert!((busy - (600.0 / 1_100.0 * 100.0)).abs() < 1e-9);
    assert_eq!(cpu_times_from_system(50, 40, 10).values[2], 0);

    let memory = memory_from_counters(1_000, 250, 400, 100);
    assert_eq!(memory.used_bytes, 750);
    assert_eq!(memory.used_percent, 75.0);
    assert_eq!(memory.swap_used_bytes, 100);
    assert_eq!(memory.swap_used_percent, 25.0);
    let unavailable = memory_from_counters(1_000, 1_500, 0, 50);
    assert_eq!(unavailable.used_bytes, 0);
    assert_eq!(unavailable.used_percent, 0.0);
    assert_eq!(unavailable.available_bytes, 1_500);
    assert_eq!(unavailable.swap_total_bytes, 0);
    let over_swap = memory_from_counters(100, 40, 10, 50);
    assert_eq!(over_swap.swap_used_bytes, 50);
    assert_eq!(over_swap.swap_used_percent, 100.0);

    let disk = disk_from_space(1_000, 300, 400);
    assert_eq!(disk.used_bytes, 600);
    assert_eq!(disk.free_bytes, 300);
    assert!((disk.used_percent - (600.0 / 900.0 * 100.0)).abs() < 1e-9);
    assert_eq!(disk_from_space(0, 0, 0).used_percent, 0.0);
    assert_eq!(disk_from_space(100, 10, 150).used_bytes, 0);

    assert_eq!(uptime_from_ticks(12_345), 12);
    assert_eq!(
        uptime_from_ticks(u64::MAX),
        i64::try_from(u64::MAX / 1000).unwrap()
    );
    assert_eq!(platform_name(10, 0, 22631), "windows 10.0.22631");
    assert_eq!(windows_root("C:"), r"C:\");
    assert_eq!(windows_root(""), r"\");
}

#[test]
fn windows_drive_strings_split_on_embedded_nuls() {
    let mut buffer = Vec::new();
    for drive in [r"C:\", r"D:\"] {
        buffer.extend(drive.encode_utf16());
        buffer.push(0);
    }
    buffer.push(0);
    assert_eq!(split_multi_sz(&buffer), [r"C:\", r"D:\"]);
    assert!(split_multi_sz(&[0]).is_empty());
}

#[test]
fn windows_primary_prefers_system_drive_then_profile() {
    struct Case {
        drive: &'static str,
        home: &'static str,
        mounts: &'static [&'static str],
        primary: Option<&'static str>,
    }
    let cases = [
        Case {
            drive: "C:",
            home: r"D:\Users\alice",
            mounts: &[r"C:\", r"D:\"],
            primary: Some(r"C:\"),
        },
        Case {
            drive: "C:",
            home: r"D:\Users\alice",
            mounts: &[r"D:\"],
            primary: Some(r"D:\"),
        },
        Case {
            drive: "",
            home: r"D:\Users\alice",
            mounts: &[r"C:\", r"D:\"],
            primary: Some(r"D:\"),
        },
        Case {
            drive: "C:",
            home: "",
            mounts: &[r"C:\", r"D:\"],
            primary: Some(r"C:\"),
        },
        Case {
            drive: "",
            home: "",
            mounts: &[r"C:\", r"D:\"],
            primary: None,
        },
        Case {
            drive: "c:",
            home: "C:/Users/alice",
            mounts: &[r"C:\"],
            primary: Some(r"C:\"),
        },
        Case {
            drive: "",
            home: r"C:\Users\alice",
            mounts: &[r"C:\", r"C:\Users"],
            primary: Some(r"C:\Users"),
        },
        Case {
            drive: "C:",
            home: r"C:\Users\alice",
            mounts: &[r"C:\", r"C:\Users"],
            primary: Some(r"C:\"),
        },
    ];
    for case in cases {
        let mut filesystems: Vec<FilesystemMetrics> = case
            .mounts
            .iter()
            .map(|mount| FilesystemMetrics {
                mount: (*mount).to_owned(),
                device: (*mount).to_owned(),
                fstype: "NTFS".into(),
                total_bytes: 100,
                ..FilesystemMetrics::default()
            })
            .collect();
        mark_windows_primary(&mut filesystems, case.drive, case.home);
        let primary = filesystems
            .iter()
            .find(|fs| fs.primary)
            .map(|fs| fs.mount.as_str());
        assert_eq!(
            primary, case.primary,
            "drive {} home {}",
            case.drive, case.home
        );
    }
}

#[test]
fn windows_constructor_labels_the_snapshot_and_omits_load() {
    let snapshot = Collector::windows(Options {
        initial_cpu_sample: Duration::ZERO,
        filesystem_timeout: Duration::from_millis(50),
        ..Options::default()
    })
    .collect()
    .unwrap();
    assert_eq!(snapshot.os, "windows");
    let cpu = snapshot.cpu.unwrap();
    assert!(cpu.load1.is_none() && cpu.load5.is_none() && cpu.load15.is_none());
    assert!(snapshot
        .warnings
        .iter()
        .all(|warning| !warning.starts_with("cpu load:")));
}
