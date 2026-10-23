use protectorate_node_metrics::{
    accel, accel::ToolRunner, dirs, docker, linux::LinuxProbe, Collector, ContainerMetrics, Options,
};
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    os::unix::{fs::symlink, net::UnixListener},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A PCI display controller bound to `driver` under a sysfs fixture.
fn pci_gpu(root: &Path, bus_id: &str, vendor: &str, device: &str, driver: &str) -> PathBuf {
    let dev = root.join("bus/pci/devices").join(bus_id);
    write_file(&dev.join("class"), "0x030000\n");
    write_file(&dev.join("vendor"), &format!("{vendor}\n"));
    write_file(&dev.join("device"), &format!("{device}\n"));
    symlink(format!("/bus/pci/drivers/{driver}"), dev.join("driver")).unwrap();
    dev
}

/// Vendor tool output, keyed by tool name, with the calls recorded.
#[derive(Default)]
struct Tools {
    outputs: HashMap<String, String>,
    calls: Mutex<Vec<String>>,
}

impl accel::ToolRunner for Tools {
    fn run(&self, name: &str, args: &[&str]) -> Option<String> {
        self.calls.lock().unwrap().push(name.to_owned());
        let key = match args.first() {
            Some(first) => format!("{name} {first}"),
            None => name.to_owned(),
        };
        self.outputs.get(&key).cloned()
    }
}

impl Tools {
    fn called(&self, name: &str) -> bool {
        self.calls.lock().unwrap().iter().any(|call| call == name)
    }
}

const ROCM_JSON: &str = r#"{
    "system": { "System Name": "N/A" },
    "card2": {
        "Card Product Name": "AMD Radeon RX 6800",
        "GPU use (%)": "8",
        "Temperature (Sensor edge) (C)": "39.0 C",
        "VRAM Total Memory (B)": "17179869184",
        "VRAM Total Used Memory (B)": "1073741824"
    },
    "card0": {
        "Card Series": "0x744c",
        "Card Product Name": "AMD Radeon RX 7900 XTX",
        "GPU use (%)": "37",
        "VRAM Total Memory (B)": "19327352832",
        "VRAM Total Used Memory (B)": "2147483648"
    }
}"#;

const XPU_DISCOVERY: &str = r#"{
    "device_list": [
        { "device_id": 0, "device_name": "Intel(R) Arc(TM) Pro B60", "pci_bdf_address": "0000:05:00.0", "device_total_memory": 24117248000 },
        { "device_id": 1, "device_name": "Intel(R) UHD Graphics", "pci_bdf_address": "0000:00:02.0", "device_total_memory": 0 },
        { "device_id": 2, "device_name": "Intel(R) Arc(TM) B580", "device_location": "0000:06:00.0" }
    ],
    "device_count": 3,
    "return_code": 0
}"#;

const XPU_DUMP: &str = "device_timestamp, DeviceId, GPU Memory Used (MiB) , GPU Utilization (%) \n2025-06-01 10:00:00.000, 0, 3072, 42\n2025-06-01 10:00:00.000, 1, 128, N/A\n2025-06-01 10:00:00.000, 9, 1, 1\n";

/// An AMD discrete card, an Intel discrete card, an Intel integrated card and
/// the drm card node rocm-smi numbers its first card after.
fn gpu_fixture(root: &Path) {
    let amd = pci_gpu(root, "0000:03:00.0", "0x1002", "0xffff", "amdgpu");
    write_file(&amd.join("gpu_busy_percent"), "5\n");
    write_file(&amd.join("mem_info_vram_total"), "19327352832\n");
    write_file(&amd.join("mem_info_vram_used"), "1048576\n");
    write_file(&amd.join("hwmon/hwmon0/temp1_label"), "vdd\n");
    write_file(&amd.join("hwmon/hwmon0/temp1_input"), "38000\n");
    write_file(&amd.join("hwmon/hwmon0/temp2_label"), "edge\n");
    write_file(&amd.join("hwmon/hwmon0/temp2_input"), "61000\n");
    let intel = pci_gpu(root, "0000:05:00.0", "0x8086", "0xffff", "i915");
    write_file(&intel.join("drm/card0/gt/gt0/rc6_residency_ms"), "0\n");
    write_file(&intel.join("mem_info_vram_used"), "1048576\n");
    let igpu = pci_gpu(root, "0000:00:02.0", "0x8086", "0xffff", "i915");
    write_file(&igpu.join("drm/card1/gt/gt0/rc6_residency_ms"), "0\n");
    write_file(&igpu.join("mem_info_vram_total"), "134217728\n");
    fs::create_dir_all(root.join("class/drm/card0")).unwrap();
    symlink(
        "../../devices/pci0000:00/0000:00:1f.0/0000:03:00.0",
        root.join("class/drm/card0/device"),
    )
    .unwrap();
}

fn gpu_tools() -> Tools {
    Tools {
        outputs: [
            (
                "rocm-smi --showproductname".to_owned(),
                ROCM_JSON.to_owned(),
            ),
            ("xpu-smi discovery".to_owned(), XPU_DISCOVERY.to_owned()),
            ("xpu-smi dump".to_owned(), XPU_DUMP.to_owned()),
        ]
        .into_iter()
        .collect(),
        calls: Mutex::new(Vec::new()),
    }
}

#[test]
fn sysfs_discovers_real_gpu_and_npu_sources() {
    let root = tempfile::tempdir().unwrap();
    let pci = root.path().join("bus/pci/devices/0000:03:00.0");
    fs::create_dir_all(pci.join("hwmon/hwmon0")).unwrap();
    fs::write(pci.join("class"), "0x030000\n").unwrap();
    fs::write(pci.join("vendor"), "0x1002\n").unwrap();
    fs::write(pci.join("device"), "0x744c\n").unwrap();
    fs::write(pci.join("gpu_busy_percent"), "37\n").unwrap();
    fs::write(pci.join("mem_info_vram_total"), "17179869184\n").unwrap();
    fs::write(pci.join("mem_info_vram_used"), "1234\n").unwrap();
    fs::write(pci.join("hwmon/hwmon0/temp1_input"), "50500\n").unwrap();
    symlink("/bus/pci/drivers/amdgpu", pci.join("driver")).unwrap();
    let npu = root.path().join("class/accel/accel0/device");
    fs::create_dir_all(npu.join("devfreq/npu0")).unwrap();
    symlink("/bus/pci/drivers/amdxdna", npu.join("driver")).unwrap();
    fs::write(npu.join("npu_current_frequency_mhz"), "800\n").unwrap();
    fs::write(npu.join("devfreq/npu0/load"), "43@800000000\n").unwrap();
    let (gpus, npus) = accel::probe(root.path(), &mut accel::AccelState::default()).unwrap();
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].vendor, "amd");
    assert_eq!(gpus[0].utilization_percent, 37.0);
    assert_eq!(gpus[0].memory_used_bytes, Some(1234));
    assert_eq!(gpus[0].temperature_celsius, 50.5);
    assert_eq!(npus.len(), 1);
    assert_eq!(npus[0].vendor, "amd");
    assert_eq!(npus[0].frequency_hz, 800_000_000);
    assert_eq!(npus[0].utilization_percent, 43.0);
}

#[test]
fn nvidia_tool_gauges_merge_with_sysfs_identity() {
    let cards =
        accel::parse_nvidia_csv("0, RTX 6000, 83, 100, 200, 71, 00000000:03:00.0, 565.57\n");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].memory_used_bytes, Some(100 << 20));
    assert_eq!(cards[0].bus_id, "0000:03:00.0");
    let mut sysfs = cards.clone();
    sysfs[0].name = "nvidia GPU [10de:1234]".into();
    sysfs[0].driver = "nvidia 565.57.01".into();
    sysfs[0].utilization_percent = -1.0;
    let merged = accel::merge_gpus(sysfs, &cards);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].name, "RTX 6000");
    assert_eq!(merged[0].utilization_percent, 83.0);
    assert_eq!(merged[0].driver, "nvidia 565.57.01");
    assert_eq!(merged[0].bus_id, "0000:03:00.0");
}

#[test]
fn an_integrated_gpu_reports_no_dedicated_memory() {
    let cards = accel::parse_nvidia_csv("0, Tegra, 83, 100, 200, 71, 0000:00:02.0, 565.57\n");
    let sysfs = vec![protectorate_node_metrics::GpuMetrics {
        vendor: "nvidia".into(),
        name: "nvidia GPU [10de:1234]".into(),
        bus_id: "0000:00:02.0".into(),
        integrated: true,
        ..Default::default()
    }];
    let merged = accel::merge_gpus(sysfs, &cards);
    assert_eq!(merged[0].memory_total_bytes, 0);
    assert_eq!(merged[0].memory_used_bytes, None);
}

#[test]
fn rocm_smi_gauges_tolerate_label_and_unit_drift() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("class/drm/card0")).unwrap();
    symlink(
        "../../devices/pci0000:00/0000:00:1f.0/0000:03:00.0",
        root.path().join("class/drm/card0/device"),
    )
    .unwrap();
    let cards = accel::parse_rocm_smi(ROCM_JSON, root.path());
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].bus_id, "0000:03:00.0");
    assert_eq!(cards[0].name, "AMD Radeon RX 7900 XTX");
    assert_eq!(cards[0].utilization_percent, 37.0);
    // No temperature label in this release: unknown, not zero.
    assert_eq!(cards[0].temperature_celsius, -1.0);
    assert_eq!(cards[0].memory_total_bytes, 19327352832);
    assert_eq!(cards[0].memory_used_bytes, Some(2147483648));
    assert_eq!(cards[1].bus_id, "");
    assert_eq!(cards[1].name, "AMD Radeon RX 6800");
    assert_eq!(cards[1].temperature_celsius, 39.0);
    let older = accel::parse_rocm_smi(
        r#"{"card0": {"Card series": "Radeon 780M", "GPU use (%)": "N/A",
             "Temperature (Sensor memory) (C)": "50.0 C",
             "Total Memory (B)": "536870912", "Used Memory (B)": "268435456"}}"#,
        root.path(),
    );
    assert_eq!(older[0].name, "Radeon 780M");
    assert_eq!(older[0].utilization_percent, -1.0);
    assert_eq!(older[0].temperature_celsius, 50.0);
    assert_eq!(older[0].memory_total_bytes, 536870912);
    assert_eq!(older[0].memory_used_bytes, Some(268435456));
    assert!(accel::parse_rocm_smi("not json", root.path()).is_empty());
}

#[test]
fn xpu_smi_matches_dump_columns_by_header_not_position() {
    let devices = accel::parse_xpu_discovery(XPU_DISCOVERY);
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[0].id, "0");
    assert_eq!(devices[0].bus_id, "0000:05:00.0");
    assert_eq!(devices[0].memory_total_bytes, 24117248000);
    assert_eq!(devices[2].bus_id, "0000:06:00.0");
    let readings = accel::parse_xpu_dump(XPU_DUMP, &devices);
    assert_eq!(readings.len(), 2);
    let arc = &readings["0000:05:00.0"];
    assert_eq!(arc.utilization_percent, 42.0);
    assert_eq!(arc.memory_used_bytes, Some(3072 << 20));
    assert_eq!(arc.memory_total_bytes, 24117248000);
    assert_eq!(readings["0000:00:02.0"].utilization_percent, -1.0);
    assert_eq!(readings["0000:00:02.0"].memory_used_bytes, Some(128 << 20));
    assert!(accel::parse_xpu_discovery("{\"device_list\":[]}").is_empty());
    assert!(accel::parse_xpu_dump("no device id here", &devices).is_empty());
}

#[test]
fn intel_idle_residency_turns_into_utilization() {
    let root = tempfile::tempdir().unwrap();
    let dev = pci_gpu(root.path(), "0000:03:00.0", "0x8086", "0xffff", "i915");
    write_file(&dev.join("drm/card0/gt/gt0/rc6_residency_ms"), "0\n");
    write_file(&dev.join("mem_info_vram_total"), "16777216000\n");
    write_file(&dev.join("mem_info_vram_used"), "3221225472\n");
    let mut state = accel::AccelState::default();
    let mut util = || {
        accel::probe(root.path(), &mut state)
            .unwrap()
            .0
            .remove(0)
            .utilization_percent
    };
    // A rate needs two readings, so the first pass reports nothing.
    assert_eq!(util(), -1.0);
    // A render engine that never enters RC6 is fully busy.
    assert_eq!(util(), 100.0);
    write_file(&dev.join("drm/card0/gt/gt0/rc6_residency_ms"), "10000000\n");
    assert_eq!(util(), 0.0);
    // A counter that went backwards has no usable rate.
    write_file(&dev.join("drm/card0/gt/gt0/rc6_residency_ms"), "5\n");
    assert_eq!(util(), -1.0);
    let gpu = accel::probe(root.path(), &mut accel::AccelState::default())
        .unwrap()
        .0
        .remove(0);
    assert_eq!(gpu.memory_total_bytes, 16777216000);
    assert_eq!(gpu.memory_used_bytes, Some(3221225472));
}

#[test]
fn xe_counts_only_the_render_compute_idle_state() {
    let root = tempfile::tempdir().unwrap();
    let dev = pci_gpu(root.path(), "0000:03:00.0", "0x8086", "0xffff", "xe");
    write_file(&dev.join("tile0/gt0/gtidle0/name"), "gt0-rc6\n");
    write_file(&dev.join("tile0/gt0/gtidle0/idle_residency_ms"), "900000\n");
    write_file(&dev.join("tile0/gt0/gtidle1/name"), "gt0-rc\n");
    write_file(&dev.join("tile0/gt0/gtidle1/idle_residency_ms"), "0\n");
    let mut state = accel::AccelState::default();
    let mut util = || {
        accel::probe(root.path(), &mut state)
            .unwrap()
            .0
            .remove(0)
            .utilization_percent
    };
    assert_eq!(util(), -1.0);
    assert_eq!(util(), 100.0);
}

#[test]
fn sysfs_temperature_prefers_the_package_or_edge_sensor() {
    let root = tempfile::tempdir().unwrap();
    let labeled = pci_gpu(root.path(), "0000:03:00.0", "0x1002", "0xffff", "amdgpu");
    write_file(&labeled.join("hwmon/hwmon0/temp1_label"), "vdd\n");
    write_file(&labeled.join("hwmon/hwmon0/temp1_input"), "38000\n");
    write_file(&labeled.join("hwmon/hwmon0/temp2_label"), "edge\n");
    write_file(&labeled.join("hwmon/hwmon0/temp2_input"), "61000\n");
    let plain = pci_gpu(root.path(), "0000:04:00.0", "0x1002", "0xffff", "amdgpu");
    write_file(&plain.join("hwmon/hwmon0/temp1_input"), "45000\n");
    let gpus = accel::probe(root.path(), &mut accel::AccelState::default())
        .unwrap()
        .0;
    assert_eq!(gpus[0].temperature_celsius, 61.0);
    assert_eq!(gpus[1].temperature_celsius, 45.0);
}

#[test]
fn vendor_tools_fill_the_sysfs_inventory() {
    let root = tempfile::tempdir().unwrap();
    gpu_fixture(root.path());
    let tools = gpu_tools();
    let mut state = accel::AccelState::default();
    let mut gpus = || {
        accel::probe_with_tools(root.path(), &mut state, &tools)
            .unwrap()
            .0
    };
    gpus();
    let gpus = gpus();
    assert_eq!(gpus.len(), 4);
    assert_eq!(
        gpus.iter().map(|gpu| gpu.index).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(
        gpus.iter()
            .map(|gpu| gpu.bus_id.as_str())
            .collect::<Vec<_>>(),
        ["", "0000:03:00.0", "0000:05:00.0", "0000:00:02.0"]
    );
    // A card rocm-smi sees and sysfs does not is still reported.
    assert_eq!(gpus[0].name, "AMD Radeon RX 6800");
    assert_eq!(gpus[0].utilization_percent, 8.0);
    let amd = &gpus[1];
    assert_eq!(amd.name, "AMD Radeon RX 7900 XTX");
    assert_eq!(amd.utilization_percent, 37.0);
    assert_eq!(amd.memory_used_bytes, Some(2147483648));
    assert_eq!(amd.memory_total_bytes, 19327352832);
    // rocm-smi reported no temperature, so the sysfs edge sensor stands.
    assert_eq!(amd.temperature_celsius, 61.0);
    assert_eq!(amd.driver, "amdgpu");
    let intel = &gpus[2];
    assert_eq!(intel.utilization_percent, 42.0);
    assert_eq!(intel.memory_used_bytes, Some(3072 << 20));
    assert_eq!(intel.memory_total_bytes, 24117248000);
    assert_eq!(intel.driver, "i915");
    // xpu-smi also reports the iGPU, but an integrated card shares system
    // memory and keeps its own sysfs utilization.
    let igpu = &gpus[3];
    assert!(igpu.integrated);
    assert_eq!(igpu.utilization_percent, 100.0);
    assert_eq!(igpu.memory_total_bytes, 0);
    assert_eq!(igpu.memory_used_bytes, None);
    // No NVIDIA card in the inventory, so nvidia-smi is never run.
    assert!(tools.called("rocm-smi") && tools.called("xpu-smi"));
    assert!(!tools.called("nvidia-smi"));
}

#[test]
fn missing_or_broken_tools_leave_the_inventory_alone() {
    let root = tempfile::tempdir().unwrap();
    gpu_fixture(root.path());
    let mut state = accel::AccelState::default();
    let absent = Tools::default();
    let gpus = accel::probe_with_tools(root.path(), &mut state, &absent)
        .unwrap()
        .0;
    assert_eq!(gpus.len(), 3);
    assert!(!absent.called("nvidia-smi"));
    assert!(absent.called("rocm-smi"));
    let broken = Tools {
        outputs: [
            (
                "rocm-smi --showproductname".to_owned(),
                "usage: rocm-smi [options]".to_owned(),
            ),
            ("xpu-smi discovery".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect(),
        calls: Mutex::new(Vec::new()),
    };
    let gpus = accel::probe_with_tools(root.path(), &mut state, &broken)
        .unwrap()
        .0;
    assert_eq!(gpus.len(), 3);
    let amd = gpus
        .iter()
        .find(|gpu| gpu.vendor == "amd" && !gpu.integrated)
        .unwrap();
    assert_eq!(amd.utilization_percent, 5.0);
    assert_eq!(amd.name, "amd GPU [1002:ffff]");
    let intel = gpus
        .iter()
        .find(|gpu| gpu.vendor == "intel" && !gpu.integrated)
        .unwrap();
    assert_eq!(intel.memory_total_bytes, 0);
    assert_eq!(intel.memory_used_bytes, Some(1048576));
}

#[cfg(unix)]
#[test]
fn system_tool_runs_are_bounded_and_quiet() {
    let tools = accel::SystemTools {
        timeout: Duration::from_secs(5),
    };
    assert_eq!(
        tools.run("sh", &["-c", "printf found"]).as_deref(),
        Some("found")
    );
    assert!(tools.run("protectorate-no-such-tool", &[]).is_none());
    assert!(tools.run("sh", &["-c", "exit 3"]).is_none());
    let tight = accel::SystemTools {
        timeout: Duration::from_millis(200),
    };
    let started = Instant::now();
    assert!(tight.run("sleep", &["30"]).is_none());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "a timed out tool took {:?}",
        started.elapsed()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_real_sysfs_and_tool_probe_never_fails_or_invents_a_card() {
    let tools = accel::SystemTools::default();
    let (gpus, npus) =
        accel::probe_with_tools(Path::new("/sys"), &mut accel::AccelState::default(), &tools)
            .unwrap();
    for gpu in &gpus {
        assert!(!gpu.vendor.is_empty(), "{gpu:?}");
        assert!(
            (0.0..=100.0).contains(&gpu.utilization_percent) || gpu.utilization_percent == -1.0,
            "{gpu:?}"
        );
        assert!(
            (0.0..=200.0).contains(&gpu.temperature_celsius) || gpu.temperature_celsius == -1.0,
            "{gpu:?}"
        );
        if gpu.integrated {
            assert_eq!(gpu.memory_total_bytes, 0, "{gpu:?}");
            assert_eq!(gpu.memory_used_bytes, None, "{gpu:?}");
        }
    }
    assert_eq!(
        gpus.iter().map(|gpu| gpu.index).collect::<Vec<_>>(),
        (0..gpus.len() as i32).collect::<Vec<_>>()
    );
    assert!(npus
        .iter()
        .all(|npu| !npu.vendor.is_empty() && npu.kind == "npu"));
}

#[test]
fn collector_reports_vendor_gauges_without_warning_about_them() {
    let root = tempfile::tempdir().unwrap();
    gpu_fixture(root.path());
    let proc = root.path().join("proc");
    write_file(&proc.join("stat"), "cpu  10 1 5 84 2 0 0 0 0\n");
    write_file(&proc.join("loadavg"), "0.10 0.20 0.30 1/123 456\n");
    write_file(
        &proc.join("meminfo"),
        "MemTotal: 1000000 kB\nMemFree: 400000 kB\n",
    );
    write_file(&proc.join("uptime"), "1234.56 1000\n");
    write_file(
        &proc.join("self/mounts"),
        &format!("fixture {} ext4 rw 0 0\n", root.path().display()),
    );
    write_file(
        &root.path().join("os-release"),
        "ID=testlinux\nVERSION_ID=1\n",
    );
    let probe = LinuxProbe {
        proc_root: proc,
        sys_root: root.path().to_path_buf(),
        os_release: root.path().join("os-release"),
        debian_version: root.path().join("debian_version"),
        tools: Arc::new(gpu_tools()),
        accel: Arc::new(Mutex::new(accel::AccelState::default())),
    };
    let collector = Collector::new(
        probe,
        Options {
            min_age: Duration::ZERO,
            initial_cpu_sample: Duration::ZERO,
            home: root.path().to_path_buf(),
            docker_host: Some("unix:///protectorate-absent.sock".into()),
            ..Options::default()
        },
    );
    let snapshot = collector.collect().unwrap();
    assert_eq!(snapshot.gpus.len(), 4);
    assert!(snapshot
        .gpus
        .iter()
        .any(|gpu| gpu.name == "AMD Radeon RX 7900 XTX" && gpu.utilization_percent == 37.0));
    assert!(snapshot.gpus.iter().any(|gpu| gpu.vendor == "intel"
        && !gpu.integrated
        && gpu.memory_used_bytes == Some(3072 << 20)));
    assert!(!snapshot
        .gpus
        .iter()
        .any(|gpu| gpu.integrated && gpu.memory_total_bytes > 0));
    assert!(
        snapshot
            .warnings
            .iter()
            .all(|warning| warning.starts_with("container engine")),
        "{:?}",
        snapshot.warnings
    );
}

#[test]
fn pci_ids_names_devices_without_confusing_subsystem_rows() {
    let names = accel::parse_pci_names(
        "1002  AMD\n\t744c  Radeon RX 7900\n\t\t1043  ASUS board\n8086  Intel\n\t56c0  Arc Pro\n",
    );
    assert_eq!(names.get(&(0x1002, 0x744c)).unwrap(), "Radeon RX 7900");
    assert_eq!(names.get(&(0x8086, 0x56c0)).unwrap(), "Arc Pro");
    assert_eq!(names.len(), 2);
}

#[test]
fn watched_directory_skips_symlinks_and_reports_budget() {
    let root = tempfile::tempdir().unwrap();
    let watched = root.path().join("watched");
    let outside = root.path().join("outside");
    fs::create_dir_all(watched.join("child")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(watched.join("a"), b"123").unwrap();
    fs::write(watched.join("child/b"), b"12345").unwrap();
    fs::write(outside.join("large"), vec![0; 1024]).unwrap();
    symlink(&outside, watched.join("link")).unwrap();
    let size = dirs::measure_dir(&watched, Duration::from_secs(1));
    assert!(size.complete, "{}", size.error);
    assert_eq!(size.size_bytes, 8);
    let expired = dirs::measure_dir(&watched, Duration::ZERO);
    assert!(!expired.complete);
    assert!(expired.size_bytes <= 8);
}

#[test]
fn docker_disk_and_stats_match_engine_shapes() {
    let (disk, sizes) = docker::parse_disk(&json!({
        "LayersSize":100,"Images":[{"Size":80,"Containers":0},{"Size":50,"Containers":1}],
        "Containers":[{"Id":"abc","State":"exited","SizeRw":15,"SizeRootFs":60}],
        "Volumes":[{"UsageData":{"Size":20,"RefCount":0}}],
        "BuildCache":[{"Size":10,"InUse":false}]
    }));
    assert_eq!(disk.images.size_bytes, 100);
    assert_eq!(disk.images.reclaimable_bytes, 80);
    assert_eq!(disk.containers.reclaimable_bytes, 15);
    assert_eq!(disk.volumes.size_bytes, 20);
    assert_eq!(disk.build_cache.reclaimable_bytes, 10);
    assert_eq!(sizes, vec![("abc".into(), 15, 60)]);
    let mut container = ContainerMetrics::default();
    docker::apply_stats(
        &mut container,
        &json!({
            "memory_stats":{"usage":500,"limit":1000,"stats":{"inactive_file":100}},
            "cpu_stats":{"cpu_usage":{"total_usage":30},"system_cpu_usage":200,"online_cpus":4},
            "precpu_stats":{"cpu_usage":{"total_usage":10},"system_cpu_usage":100}
        }),
    );
    assert_eq!(container.memory_used_bytes, 400);
    assert_eq!(container.memory_percent, 40.0);
    assert_eq!(container.cpu_percent, 80.0);
}

#[test]
fn docker_unix_socket_fetches_version_list_disk_and_stats() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("docker.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap();
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/version" => json!({"Version":"27.0","ApiVersion":"1.47","MinAPIVersion":"1.24"}),
                "/v1.43/containers/json?all=1" => json!([{"Id":"abcdef0123456789","Names":["/example"],"Image":"alpine","State":"running","Created":100}]),
                "/v1.43/system/df" => json!({"LayersSize":10,"Containers":[{"Id":"abcdef0123456789","State":"running","SizeRw":3,"SizeRootFs":12}]}),
                "/v1.43/containers/abcdef0123456789/stats?stream=false" => json!({"memory_stats":{"usage":200,"limit":1000,"stats":{"inactive_file":50}},"cpu_stats":{"cpu_usage":{"total_usage":30},"system_cpu_usage":200,"online_cpus":2},"precpu_stats":{"cpu_usage":{"total_usage":10},"system_cpu_usage":100}}),
                other => panic!("unexpected Docker request {other}"),
            }.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let engine = docker::probe(Some(&format!("unix://{}", socket.display())), true)
        .unwrap()
        .unwrap();
    server.join().unwrap();
    assert_eq!(engine.version, "27.0");
    assert_eq!(engine.disk.unwrap().containers.size_bytes, 3);
    assert_eq!(engine.containers.len(), 1);
    assert_eq!(engine.containers[0].name, "example");
    assert_eq!(engine.containers[0].writable_layer_bytes, 3);
    assert_eq!(engine.containers[0].memory_used_bytes, 150);
    assert_eq!(engine.containers[0].cpu_percent, 40.0);
}
