use crate::{clean, exec, sys};
use pt_frame::msg::{Inventory, Toolchain};
use std::time::Duration;

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
