//! Linux accelerator inventory and gauges. Unknown utilization or temperature
//! is `-1`, matching the wire convention; absent devices are omitted entirely.
//!
//! sysfs owns the inventory and every device's identity: bus id, integration
//! and kernel driver. Vendor tools (nvidia-smi, rocm-smi, xpu-smi) add live
//! gauges on top. A tool that is missing, fails or runs past its budget simply
//! contributes nothing, and a device no tool covers keeps whatever its driver
//! reports in sysfs.
use crate::{AcceleratorMetrics, GpuMetrics};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, OnceLock},
    thread,
    time::{Duration, Instant},
};

const TOOL_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_TOOL_OUTPUT: u64 = 1 << 20;
pub(crate) const NVIDIA_ARGS: [&str; 2] = [
    "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,pci.bus_id,driver_version",
    "--format=csv,noheader,nounits",
];
const ROCM_ARGS: [&str; 6] = [
    "--showproductname",
    "--showuse",
    "--showmeminfo",
    "vram",
    "--showtemp",
    "--json",
];

fn read(path: impl AsRef<Path>) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_owned())
}
fn hex(path: impl AsRef<Path>) -> Option<u64> {
    u64::from_str_radix(read(path)?.trim_start_matches("0x"), 16).ok()
}
fn number(path: impl AsRef<Path>) -> Option<f64> {
    read(path)?.parse().ok()
}
fn link_name(path: impl AsRef<Path>) -> Option<String> {
    Some(
        fs::read_link(path)
            .ok()?
            .file_name()?
            .to_string_lossy()
            .into_owned(),
    )
}
fn entries(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return vec![];
    };
    let mut out: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    out.sort();
    out
}
/// Directories under `path` whose name starts with `prefix`, in name order.
fn subdirectories(path: &Path, prefix: &str) -> Vec<PathBuf> {
    entries(path)
        .into_iter()
        .filter(|entry| {
            entry.is_dir()
                && entry
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .collect()
}
fn driver(dev: &Path, root: &Path) -> String {
    let Some(name) = link_name(dev.join("driver")) else {
        return String::new();
    };
    let version = link_name(dev.join("driver/module"))
        .and_then(|module| read(root.join("module").join(module).join("version")))
        .unwrap_or_default();
    if version.is_empty() {
        name
    } else {
        format!(
            "{name} {}",
            version.split_whitespace().next().unwrap_or_default()
        )
    }
}
fn vendor(id: u64) -> String {
    match id {
        0x8086 => "intel".into(),
        0x1002 | 0x1022 => "amd".into(),
        0x10de => "nvidia".into(),
        0x5143 | 0x17cb => "qualcomm".into(),
        0x13b5 => "arm".into(),
        0x106b => "apple".into(),
        _ => format!("0x{id:x}"),
    }
}
fn percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

pub fn parse_pci_names(raw: &str) -> HashMap<(u64, u64), String> {
    let mut names = HashMap::new();
    let mut current = None;
    for line in raw.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with('\t') {
            current = line
                .split_whitespace()
                .next()
                .and_then(|id| u64::from_str_radix(id, 16).ok());
            continue;
        }
        if line.starts_with("\t\t") {
            continue;
        }
        let Some(vendor) = current else {
            continue;
        };
        let Some((id, name)) = line.trim_start_matches('\t').split_once("  ") else {
            continue;
        };
        if let Ok(device) = u64::from_str_radix(id, 16) {
            names.insert((vendor, device), name.trim().into());
        }
    }
    names
}

fn pci_name(vendor: u64, device: u64) -> Option<&'static String> {
    static NAMES: OnceLock<HashMap<(u64, u64), String>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            [
                "/usr/share/hwdata/pci.ids",
                "/usr/share/misc/pci.ids",
                "/usr/share/pci.ids",
            ]
            .iter()
            .find_map(|path| fs::read_to_string(path).ok())
            .map_or_else(HashMap::new, |raw| parse_pci_names(&raw))
        })
        .get(&(vendor, device))
}

/// A device's devfreq load, reported as "NN@FREQHz".
fn load(dev: &Path) -> Option<f64> {
    subdirectories(&dev.join("devfreq"), "")
        .into_iter()
        .find_map(|df| {
            let raw = read(df.join("load"))?;
            raw.split('@').next()?.parse::<f64>().ok()
        })
}
fn frequency(dev: &Path) -> u64 {
    subdirectories(&dev.join("devfreq"), "")
        .into_iter()
        .find_map(|df| number(df.join("cur_freq")).map(|value| value.max(0.0) as u64))
        .unwrap_or(0)
}
fn mebibytes(value: f64) -> u64 {
    (value * (1 << 20) as f64).clamp(0.0, u64::MAX as f64) as u64
}

/// The package or edge sensor of a device, else its first hwmon reading.
fn temperature(dev: &Path) -> f64 {
    let mut fallback = None;
    for hwmon in entries(&dev.join("hwmon")) {
        for label in entries(&hwmon) {
            let Some(name) = label
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let Some(stem) = name.strip_suffix("_label") else {
                continue;
            };
            if !stem.starts_with("temp") {
                continue;
            }
            if matches!(read(&label).as_deref(), Some("pkg") | Some("edge")) {
                if let Some(value) = number(hwmon.join(format!("{stem}_input"))) {
                    return value / 1000.0;
                }
            }
        }
        if fallback.is_none() {
            fallback = number(hwmon.join("temp1_input")).map(|value| value / 1000.0);
        }
    }
    fallback.unwrap_or(-1.0)
}

/// Cumulative sysfs counters only become a rate against the previous reading.
#[derive(Default)]
pub struct AccelState {
    counters: HashMap<String, (f64, Instant)>,
}

impl AccelState {
    /// Share of wall time the counter advanced since the previous reading,
    /// `-1` on the first reading or a counter reset.
    fn rate(&mut self, key: &Path, value: f64, at: Instant, unit: Duration) -> f64 {
        let previous = self
            .counters
            .insert(key.to_string_lossy().into_owned(), (value, at));
        let Some((previous, previous_at)) = previous else {
            return -1.0;
        };
        let wall = at.saturating_duration_since(previous_at);
        if wall.is_zero() || value < previous {
            return -1.0;
        }
        percent((value - previous) * unit.as_secs_f64() / wall.as_secs_f64() * 100.0)
    }
}

/// i915 keeps its render-engine idle residency under the card it created, xe
/// per tile and gt. Only the "-rc" states measure the render engine being idle.
fn idle_files(dev: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for card in subdirectories(&dev.join("drm"), "") {
        let rc6 = card.join("gt/gt0/rc6_residency_ms");
        if rc6.is_file() {
            out.push(rc6);
            continue;
        }
        let legacy = card.join("power/rc6_residency_ms");
        if legacy.is_file() {
            out.push(legacy);
        }
    }
    if !out.is_empty() {
        return out;
    }
    for tile in subdirectories(dev, "tile") {
        for gt in subdirectories(&tile, "gt") {
            for idle in subdirectories(&gt, "gtidle") {
                if !read(idle.join("name")).is_some_and(|name| name.ends_with("-rc")) {
                    continue;
                }
                let residency = idle.join("idle_residency_ms");
                if residency.is_file() {
                    out.push(residency);
                }
            }
        }
    }
    out
}

/// Share of wall time the render engine was out of its idle power state.
fn idle_percent(dev: &Path, state: &mut AccelState, at: Instant) -> Option<f64> {
    let files = idle_files(dev);
    if files.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for file in &files {
        total += number(file)?;
    }
    Some(state.rate(
        &files[0],
        total / files.len() as f64,
        at,
        Duration::from_millis(1),
    ))
}

/// GPU busy share: the driver's own gauge, then devfreq load, then the idle
/// power state a GPU would otherwise sit in. `-1` when it reports none.
fn utilization(dev: &Path, state: &mut AccelState, at: Instant) -> f64 {
    if let Some(value) = number(dev.join("gpu_busy_percent")) {
        return percent(value);
    }
    if let Some(value) = load(dev) {
        return percent(value);
    }
    match idle_percent(dev, state, at) {
        Some(idle) if idle >= 0.0 => percent(100.0 - idle),
        _ => -1.0,
    }
}

/// NPU busy share, which adds the cumulative busy time Intel's NPU reports.
fn npu_utilization(dev: &Path, state: &mut AccelState, at: Instant) -> f64 {
    if let Some(value) = number(dev.join("npu_busy_percent")) {
        return percent(value);
    }
    if let Some(value) = load(dev) {
        return percent(value);
    }
    let busy = dev.join("npu_busy_time_us");
    match number(&busy) {
        Some(value) => state.rate(&busy, value, at, Duration::from_micros(1)),
        None => -1.0,
    }
}

/// Inventory plus current gauges. `root` is `/sys` in production and a
/// directory fixture in tests. A missing sysfs tree yields an empty inventory.
pub fn probe(
    root: &Path,
    state: &mut AccelState,
) -> io::Result<(Vec<GpuMetrics>, Vec<AcceleratorMetrics>)> {
    let at = Instant::now();
    let mut gpus = Vec::new();
    let mut npus = Vec::new();
    let mut seen_npu = HashSet::new();
    for dev in entries(&root.join("bus/pci/devices")) {
        if hex(dev.join("class")).is_none_or(|class| class >> 16 != 0x03) {
            continue;
        }
        let Some(vid) = hex(dev.join("vendor")) else {
            continue;
        };
        if [
            0x1af4, 0x1234, 0x15ad, 0x1b36, 0x1414, 0x1013, 0x1a03, 0x80ee, 0x102b,
        ]
        .contains(&vid)
        {
            continue;
        }
        let did = hex(dev.join("device")).unwrap_or_default();
        let bus_id = dev
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let integrated = bus_id.split(':').nth(1).is_some_and(|bus| bus == "00");
        let mut gpu = GpuMetrics {
            index: gpus.len() as i32,
            vendor: vendor(vid),
            name: pci_name(vid, did)
                .cloned()
                .unwrap_or_else(|| format!("{} GPU [{vid:04x}:{did:04x}]", vendor(vid))),
            driver: driver(&dev, root),
            bus_id,
            integrated,
            utilization_percent: utilization(&dev, state, at),
            temperature_celsius: temperature(&dev),
            ..GpuMetrics::default()
        };
        // Dedicated VRAM exists only on a discrete card, and only amdgpu and
        // i915 publish it; xe keeps it to itself.
        if !gpu.integrated && matches!(gpu.vendor.as_str(), "amd" | "intel") {
            gpu.memory_total_bytes =
                number(dev.join("mem_info_vram_total")).map_or(0, |value| value.max(0.0) as u64);
            gpu.memory_used_bytes =
                number(dev.join("mem_info_vram_used")).map(|v| v.max(0.0) as u64);
        }
        gpus.push(gpu);
    }
    for dev in entries(&root.join("bus/platform/devices")) {
        let name = dev
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let driver_name = link_name(dev.join("driver")).unwrap_or_default();
        let drv = driver(&dev, root);
        let gpu = match driver_name.as_str() {
            "mali" | "panthor" | "panfrost" | "lima" => Some(("arm", "Mali")),
            "adreno" => Some(("qualcomm", "Adreno")),
            "etnaviv-gpu" => Some(("vivante", "Vivante GPU")),
            "v3d" => Some(("broadcom", "VideoCore")),
            "powervr" => Some(("imagination", "PowerVR")),
            "asahi" => Some(("apple", "Apple GPU")),
            _ => None,
        };
        if let Some((vendor, default_name)) = gpu {
            gpus.push(GpuMetrics {
                index: gpus.len() as i32,
                vendor: vendor.into(),
                name: read(dev.join("gpuinfo"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or(default_name.into()),
                driver: drv.clone(),
                bus_id: name.clone(),
                integrated: true,
                utilization_percent: utilization(&dev, state, at),
                temperature_celsius: -1.0,
                ..GpuMetrics::default()
            });
        }
        let npu = match driver_name.as_str() {
            "armchina" => Some(("armchina", "Zhouyi NPU")),
            "rknpu" => Some(("rockchip", "RKNPU")),
            _ => None,
        };
        if let Some((vendor, model)) = npu {
            seen_npu.insert(fs::canonicalize(&dev).unwrap_or(dev.clone()));
            npus.push(AcceleratorMetrics {
                kind: "npu".into(),
                vendor: vendor.into(),
                name: model.into(),
                driver: drv,
                bus_id: name,
                utilization_percent: npu_utilization(&dev, state, at),
                frequency_hz: frequency(&dev),
            });
        }
    }
    for class in entries(&root.join("class/accel")) {
        let dev = class.join("device");
        if seen_npu.contains(&fs::canonicalize(&dev).unwrap_or(dev.clone())) {
            continue;
        }
        let driver_name = link_name(dev.join("driver")).unwrap_or_default();
        let (known_vendor, model) = match driver_name.as_str() {
            "intel_vpu" => ("intel", "Intel NPU"),
            "amdxdna" => ("amd", "AMD NPU"),
            "qaic" => ("qualcomm", "Qualcomm Cloud AI"),
            "rocket" => ("rockchip", "Rockchip NPU"),
            "ethosu" => ("arm", "Ethos-U NPU"),
            _ => ("", ""),
        };
        let ven = if known_vendor.is_empty() {
            hex(dev.join("vendor"))
                .map(vendor)
                .unwrap_or(driver_name.clone())
        } else {
            known_vendor.into()
        };
        if ven.is_empty() {
            continue;
        }
        let name = if model.is_empty() {
            format!("{ven} NPU")
        } else {
            model.into()
        };
        let freq = number(dev.join("npu_current_frequency_mhz"))
            .map(|v| (v.max(0.0) * 1_000_000.0) as u64)
            .unwrap_or_else(|| frequency(&dev));
        npus.push(AcceleratorMetrics {
            kind: "npu".into(),
            vendor: ven,
            name,
            driver: driver(&dev, root),
            bus_id: class
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            utilization_percent: npu_utilization(&dev, state, at),
            frequency_hz: freq,
        });
    }
    Ok((gpus, npus))
}

/// Runs one vendor accelerator tool. Implementations bound the run and report
/// nothing when the tool is missing, fails or outlives its budget.
pub trait ToolRunner: Send + Sync {
    fn run(&self, name: &str, args: &[&str]) -> Option<String>;
}

/// Runs the tool as a child process with a hard timeout.
#[derive(Clone, Copy, Debug)]
pub struct SystemTools {
    pub timeout: Duration,
}

impl Default for SystemTools {
    fn default() -> Self {
        Self {
            timeout: TOOL_TIMEOUT,
        }
    }
}

impl ToolRunner for SystemTools {
    fn run(&self, name: &str, args: &[&str]) -> Option<String> {
        let Ok(mut child) = Command::new(name)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return None;
        };
        // Reading on its own thread keeps a chatty tool from deadlocking
        // against the bounded wait for the process to exit.
        let (sender, receiver) = mpsc::channel();
        let stdout = child.stdout.take()?;
        thread::spawn(move || {
            let mut raw = Vec::new();
            let _ = stdout.take(MAX_TOOL_OUTPUT).read_to_end(&mut raw);
            let _ = sender.send(raw);
        });
        let deadline = Instant::now().checked_add(self.timeout)?;
        let remaining = || deadline.saturating_duration_since(Instant::now());
        let succeeded = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if remaining() > Duration::ZERO => {
                    thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
            }
        };
        if !succeeded {
            return None;
        }
        // The child is gone, so the pipe closes; the grace covers the reader
        // draining what it already wrote.
        let grace = remaining().max(Duration::from_millis(250));
        receiver
            .recv_timeout(grace)
            .ok()
            .map(|raw| String::from_utf8_lossy(&raw).into_owned())
    }
}

pub fn parse_nvidia_csv(raw: &str) -> Vec<GpuMetrics> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        if fields.len() < 8 {
            continue;
        }
        let Ok(index) = fields[0].parse() else {
            continue;
        };
        let mb = |s: &str| s.parse::<u64>().ok().map(|n| n.saturating_mul(1 << 20));
        let bus_id = if let Some((domain, rest)) = fields[6].split_once(':') {
            format!("{}:{rest}", &domain[domain.len().saturating_sub(4)..])
        } else {
            fields[6].to_owned()
        };
        out.push(GpuMetrics {
            index,
            vendor: "nvidia".into(),
            name: fields[1].into(),
            utilization_percent: fields[2].parse().unwrap_or(-1.0),
            memory_used_bytes: mb(fields[3]),
            memory_total_bytes: mb(fields[4]).unwrap_or(0),
            temperature_celsius: fields[5].parse().unwrap_or(-1.0),
            bus_id,
            driver: fields[7].into(),
            ..GpuMetrics::default()
        });
    }
    out
}

/// A tool value carries units and notes: "40.0 C", "1%", "19327352832", "N/A".
fn numeric(text: &str) -> Option<f64> {
    let token = text
        .split_whitespace()
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let value: f64 = token.trim_end_matches('%').parse().ok()?;
    value.is_finite().then_some(value)
}
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}
/// Vendor tools rename their fields between releases, so labels are matched
/// case-insensitively against a list of the names seen in the wild.
fn field<'a>(object: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let map = object.as_object()?;
    names.iter().find_map(|name| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    })
}
fn field_number(object: &Value, names: &[&str]) -> Option<f64> {
    field(object, names).and_then(|value| numeric(&text_of(value)))
}
/// A bare PCI id ("0x744c") is what a tool falls back to when it has no model
/// name; it is not something to show a person.
fn looks_like_device_id(text: &str) -> bool {
    let body = text
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    !body.is_empty() && body.len() <= 8 && body.chars().all(|c| c.is_ascii_hexdigit())
}

/// `rocm-smi --json` keys its cards "cardN" beside entries such as "system".
fn rocm_cards(raw: &str) -> Vec<(usize, Value)> {
    let Ok(Value::Object(map)) = serde_json::from_str(raw) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in map {
        if let Ok(number) = key.trim_start_matches("card").parse::<usize>() {
            out.push((number, value));
        }
    }
    out.sort_by_key(|(number, _)| *number);
    out
}

fn rocm_name(card: &Value) -> String {
    const NAMES: [&str; 3] = ["Card Product Name", "Card Series", "Card Model"];
    let mut fallback = String::new();
    for name in NAMES {
        let Some(text) = field(card, &[name]).map(text_of) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if !looks_like_device_id(&text) {
            return text;
        }
        fallback = text;
    }
    fallback
}

/// rocm-smi names its temperature sensor differently per release; prefer the
/// package or edge sensor over memory or board sensors.
fn rocm_temperature(card: &Value) -> Option<f64> {
    let mut best: Option<(u8, f64)> = None;
    let map = card.as_object()?;
    for (key, value) in map {
        let key = key.to_ascii_lowercase();
        if !key.contains("temperature") {
            continue;
        }
        let rank = if key.contains("edge") || key.contains("pkg") {
            0
        } else if key.contains("gpu") {
            1
        } else {
            2
        };
        let Some(value) = numeric(&text_of(value)).filter(|value| *value >= 0.0) else {
            continue;
        };
        if best.is_none_or(|(best_rank, _)| rank < best_rank) {
            best = Some((rank, value));
        }
    }
    best.map(|(_, value)| value)
}

/// `rocm-smi --showproductname --showuse --showmeminfo vram --showtemp --json`.
/// Each card is tied to its sysfs card node so the merge can recognize it.
pub fn parse_rocm_smi(raw: &str, sys_root: &Path) -> Vec<GpuMetrics> {
    rocm_cards(raw)
        .into_iter()
        .map(|(number, card)| {
            let bus_id = link_name(
                sys_root
                    .join("class/drm")
                    .join(format!("card{number}"))
                    .join("device"),
            )
            .unwrap_or_default();
            GpuMetrics {
                index: 0,
                vendor: "amd".into(),
                name: rocm_name(&card),
                utilization_percent: field_number(
                    &card,
                    &["GPU use (%)", "GPU Utilization (%)", "GPU utilization (%)"],
                )
                .map(percent)
                .unwrap_or(-1.0),
                memory_used_bytes: field_number(
                    &card,
                    &["VRAM Total Used Memory (B)", "Used Memory (B)"],
                )
                .filter(|value| *value >= 0.0)
                .map(|value| value as u64),
                memory_total_bytes: field_number(
                    &card,
                    &["VRAM Total Memory (B)", "Total Memory (B)"],
                )
                .filter(|value| *value >= 0.0)
                .map_or(0, |value| value as u64),
                temperature_celsius: rocm_temperature(&card).unwrap_or(-1.0),
                integrated: false,
                driver: String::new(),
                bus_id,
            }
        })
        .collect()
}

/// One Intel device from `xpu-smi discovery -j`.
#[derive(Clone, Debug, Default)]
pub struct XpuDevice {
    pub id: String,
    pub bus_id: String,
    pub memory_total_bytes: u64,
}

/// Live figures `xpu-smi` reports for one device, keyed by PCI address.
#[derive(Clone, Debug, Default)]
pub struct XpuReading {
    pub utilization_percent: f64,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: u64,
}

/// Device ids and their PCI addresses. Field names differ between the Level
/// Zero and oneAPI builds of the tool.
pub fn parse_xpu_discovery(raw: &str) -> Vec<XpuDevice> {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(list) = value
        .get("device_list")
        .or_else(|| value.get("deviceList"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    list.iter()
        .map(|device| XpuDevice {
            id: field(device, &["device_id", "deviceID", "deviceId", "id"])
                .map(text_of)
                .unwrap_or_default(),
            bus_id: field(
                device,
                &[
                    "pci_bdf_address",
                    "pciBdfAddress",
                    "device_location",
                    "deviceLocation",
                    "pci_address",
                ],
            )
            .map(text_of)
            .unwrap_or_default()
            .to_ascii_lowercase(),
            memory_total_bytes: field_number(
                device,
                &["device_total_memory", "deviceTotalMemory", "total_memory"],
            )
            .map_or(0, |value| value.max(0.0) as u64),
        })
        .filter(|device| !device.id.is_empty())
        .collect()
}

/// `xpu-smi dump` CSV. Column order follows the requested `-m` list, so every
/// field is found by its header.
fn csv_field<'a>(fields: &[&'a str], index: Option<usize>) -> Option<&'a str> {
    index
        .and_then(|index| fields.get(index).copied())
        .map(str::trim)
}

pub fn parse_xpu_dump(raw: &str, devices: &[XpuDevice]) -> HashMap<String, XpuReading> {
    let mut lines = raw.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(header) = lines.next() else {
        return HashMap::new();
    };
    let columns: Vec<String> = header
        .split(',')
        .map(|column| column.trim().to_ascii_lowercase())
        .collect();
    let column = |exact: &str, contains: &str| {
        columns
            .iter()
            .position(|name| name == exact)
            .or_else(|| columns.iter().position(|name| name.contains(contains)))
    };
    let Some(id) = column("deviceid", "device id") else {
        return HashMap::new();
    };
    let util = column("gpu utilization (%)", "gpu utilization");
    let used = column("gpu memory used (mib)", "memory used");
    let total = column("gpu memory total (mib)", "memory total");
    let mut out = HashMap::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(id) = csv_field(&fields, Some(id)) else {
            continue;
        };
        let Some(device) = devices.iter().find(|device| device.id == id) else {
            continue;
        };
        let reading = XpuReading {
            utilization_percent: csv_field(&fields, util)
                .and_then(numeric)
                .map(percent)
                .unwrap_or(-1.0),
            memory_used_bytes: csv_field(&fields, used)
                .and_then(numeric)
                .filter(|value| *value >= 0.0)
                .map(mebibytes),
            memory_total_bytes: csv_field(&fields, total)
                .and_then(numeric)
                .filter(|value| *value >= 0.0)
                .map_or(device.memory_total_bytes, mebibytes),
        };
        if !device.bus_id.is_empty() {
            out.insert(device.bus_id.clone(), reading);
        }
    }
    out
}

fn xpu_readings(tools: &dyn ToolRunner) -> HashMap<String, XpuReading> {
    let Some(raw) = tools.run("xpu-smi", &["discovery", "-j"]) else {
        return HashMap::new();
    };
    let devices = parse_xpu_discovery(&raw);
    if devices.is_empty() {
        return HashMap::new();
    }
    let ids: Vec<String> = devices.iter().map(|device| device.id.clone()).collect();
    let Some(dump) = tools.run(
        "xpu-smi",
        &["dump", "-d", &ids.join(","), "-m", "0,18", "-n", "1"],
    ) else {
        return HashMap::new();
    };
    parse_xpu_dump(&dump, &devices)
}

/// Fills Intel discrete gauges from xpu-smi, matched by PCI address. An
/// integrated GPU shares system memory and reports neither.
pub fn apply_xpu(gpus: &mut [GpuMetrics], readings: &HashMap<String, XpuReading>) {
    for gpu in gpus {
        if gpu.vendor != "intel" || gpu.integrated {
            continue;
        }
        let Some(reading) = readings.get(&gpu.bus_id.to_ascii_lowercase()) else {
            continue;
        };
        if reading.utilization_percent >= 0.0 {
            gpu.utilization_percent = reading.utilization_percent;
        }
        if reading.memory_used_bytes.is_some() {
            gpu.memory_used_bytes = reading.memory_used_bytes;
        }
        if reading.memory_total_bytes > 0 {
            gpu.memory_total_bytes = reading.memory_total_bytes;
        }
    }
}

/// Vendor gauges over the sysfs reading of the same card. sysfs keeps the
/// identity: bus id, integration and the kernel driver name.
fn overlay(gpu: &mut GpuMetrics, tool: &GpuMetrics) {
    if !tool.name.is_empty() && !looks_like_device_id(&tool.name) {
        gpu.name = tool.name.clone();
    }
    if tool.utilization_percent >= 0.0 {
        gpu.utilization_percent = tool.utilization_percent;
    }
    if tool.temperature_celsius >= 0.0 {
        gpu.temperature_celsius = tool.temperature_celsius;
    }
    if tool.memory_used_bytes.is_some() {
        gpu.memory_used_bytes = tool.memory_used_bytes;
    }
    if tool.memory_total_bytes > 0 {
        gpu.memory_total_bytes = tool.memory_total_bytes;
    }
    if gpu.driver.is_empty() {
        gpu.driver = tool.driver.clone();
    }
}

/// Prefer vendor live gauges while retaining sysfs-only cards and bus data.
pub fn merge_gpus(mut sysfs: Vec<GpuMetrics>, tools: &[GpuMetrics]) -> Vec<GpuMetrics> {
    for tool in tools {
        let matched = sysfs.iter_mut().find(|gpu| {
            gpu.vendor == tool.vendor
                && !gpu.bus_id.is_empty()
                && gpu.bus_id.eq_ignore_ascii_case(&tool.bus_id)
        });
        match matched {
            Some(gpu) => overlay(gpu, tool),
            None => sysfs.push(tool.clone()),
        }
    }
    sysfs.sort_by(|a, b| {
        a.integrated
            .cmp(&b.integrated)
            .then(a.bus_id.cmp(&b.bus_id))
    });
    for (index, gpu) in sysfs.iter_mut().enumerate() {
        gpu.index = index as i32;
        if gpu.integrated {
            gpu.memory_total_bytes = 0;
            gpu.memory_used_bytes = None;
        }
    }
    sysfs
}

/// sysfs inventory with vendor tool gauges on top. A tool runs only for a
/// vendor the inventory saw, or when the inventory is empty and the hardware
/// is unknown; xpu-smi only ever refines cards sysfs already found.
pub fn probe_with_tools(
    root: &Path,
    state: &mut AccelState,
    tools: &dyn ToolRunner,
) -> io::Result<(Vec<GpuMetrics>, Vec<AcceleratorMetrics>)> {
    let (mut gpus, npus) = probe(root, state)?;
    let unknown = gpus.is_empty();
    let mut gauges = Vec::new();
    if unknown || gpus.iter().any(|gpu| gpu.vendor == "nvidia") {
        if let Some(raw) = tools.run("nvidia-smi", &NVIDIA_ARGS) {
            gauges.extend(parse_nvidia_csv(&raw));
        }
    }
    if unknown || gpus.iter().any(|gpu| gpu.vendor == "amd") {
        if let Some(raw) = tools.run("rocm-smi", &ROCM_ARGS) {
            gauges.extend(parse_rocm_smi(&raw, root));
        }
    }
    if gpus
        .iter()
        .any(|gpu| gpu.vendor == "intel" && !gpu.integrated)
    {
        apply_xpu(&mut gpus, &xpu_readings(tools));
    }
    Ok((merge_gpus(gpus, &gauges), npus))
}
