use crate::json::{self, Obj, clean};
use crate::{exec, sys};
use std::time::Duration;

pub struct Gpu {
    pub vendor: String,
    pub name: String,
    pub vendor_id: Option<u32>,
    pub device_id: Option<u32>,
    pub vram_bytes: Option<u64>,
    pub driver: Option<String>,
}

pub struct Disk {
    pub mount: String,
    pub device: String,
    pub fs: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

pub struct Toolchain {
    pub name: String,
    pub version: String,
    pub path: String,
}

pub struct Inventory {
    pub collected_at: u64,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel: String,
    pub arch: String,
    pub cpu_model: String,
    pub cores: u32,
    pub threads: u32,
    pub ram_total_bytes: u64,
    pub gpus: Vec<Gpu>,
    pub disks: Vec<Disk>,
    pub toolchains: Vec<Toolchain>,
}

const NAME_MAX: usize = 128;
const PATH_MAX: usize = 4096;

pub fn collect() -> Inventory {
    let os = sys::os();
    let cpu = sys::cpu();
    let (toolchains, gpus) = std::thread::scope(|s| {
        let tc = s.spawn(toolchains);
        let gpus = sys::gpus();
        (tc.join().unwrap_or_default(), gpus)
    });
    Inventory {
        collected_at: crate::unix_now(),
        hostname: clean(&sys::hostname(), NAME_MAX),
        os_name: clean(&os.name, NAME_MAX),
        os_version: clean(&os.version, NAME_MAX),
        kernel: clean(&os.kernel, NAME_MAX),
        arch: std::env::consts::ARCH.to_string(),
        cpu_model: clean(&cpu.model, NAME_MAX),
        cores: cpu.cores,
        threads: cpu.threads,
        ram_total_bytes: sys::ram_total(),
        gpus,
        disks: sys::disks(),
        toolchains,
    }
}

#[cfg(unix)]
const PROBES: &[(&str, &str, &[&str])] = &[
    ("cargo", "cargo", &["--version"]),
    ("rustc", "rustc", &["--version"]),
    ("go", "go", &["version"]),
    ("zig", "zig", &["version"]),
    ("flutter", "flutter", &["--version"]),
    ("git", "git", &["--version"]),
    ("gcc", "gcc", &["--version"]),
];

#[cfg(windows)]
const PROBES: &[(&str, &str, &[&str])] = &[
    ("cargo", "cargo", &["--version"]),
    ("rustc", "rustc", &["--version"]),
    ("go", "go", &["version"]),
    ("zig", "zig", &["version"]),
    ("flutter", "flutter", &["--version"]),
    ("git", "git", &["--version"]),
    ("gcc", "gcc", &["--version"]),
    ("cl", "cl", &[]),
    ("link", "link", &[]),
];

fn toolchains() -> Vec<Toolchain> {
    let mut found: Vec<Toolchain> = std::thread::scope(|s| {
        let handles: Vec<_> = PROBES
            .iter()
            .map(|&(name, bin, args)| s.spawn(move || probe(name, bin, args)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()?).collect()
    });
    found.extend(sys::extra_toolchains());
    found
}

fn probe(name: &str, bin: &str, args: &[&str]) -> Option<Toolchain> {
    let path = exec::which(bin)?;
    let out = exec::run(
        path.to_str()?,
        args,
        Duration::from_secs(if name == "flutter" { 30 } else { 5 }),
    )?;
    let text = if out.stdout.trim().is_empty() {
        &out.stderr
    } else {
        &out.stdout
    };
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(Toolchain {
        name: name.to_string(),
        version: clean(line, NAME_MAX),
        path: clean(&path.to_string_lossy(), PATH_MAX),
    })
}

impl Gpu {
    fn to_json(&self) -> String {
        Obj::new()
            .str("vendor", &self.vendor)
            .str("name", &self.name)
            .opt_uint("vendor_id", self.vendor_id.map(u64::from))
            .opt_uint("device_id", self.device_id.map(u64::from))
            .opt_uint("vram_bytes", self.vram_bytes)
            .opt_str("driver", self.driver.as_deref())
            .finish()
    }
}

impl Disk {
    pub(crate) fn to_json(&self) -> String {
        Obj::new()
            .str("mount", &self.mount)
            .str("device", &self.device)
            .str("fs", &self.fs)
            .uint("total_bytes", self.total_bytes)
            .uint("free_bytes", self.free_bytes)
            .finish()
    }
}

impl Toolchain {
    fn to_json(&self) -> String {
        Obj::new()
            .str("name", &self.name)
            .str("version", &self.version)
            .str("path", &self.path)
            .finish()
    }
}

impl Inventory {
    pub fn to_json(&self) -> String {
        Obj::new()
            .uint("collected_at", self.collected_at)
            .str("hostname", &self.hostname)
            .str("os_name", &self.os_name)
            .str("os_version", &self.os_version)
            .str("kernel", &self.kernel)
            .str("arch", &self.arch)
            .str("cpu_model", &self.cpu_model)
            .uint("cores", u64::from(self.cores))
            .uint("threads", u64::from(self.threads))
            .uint("ram_total_bytes", self.ram_total_bytes)
            .raw("gpus", &json::array(self.gpus.iter().map(Gpu::to_json)))
            .raw("disks", &json::array(self.disks.iter().map(Disk::to_json)))
            .raw(
                "toolchains",
                &json::array(self.toolchains.iter().map(Toolchain::to_json)),
            )
            .finish()
    }
}
