//! For build scripts: the Go blobs of `go/bin` (`<name>-<goos>-<goarch>[.exe]`, or a `.gz` of
//! one made elsewhere) as `Embedded` literals over `include_bytes!`. Without
//! `PROTECTORATE_NET_DIR`, stale or missing blobs are rebuilt first by `scripts/net-build.sh`
//! when Go is on PATH, one build script at a time (a lock in `go/bin`).

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// The Go names of the target being built (`CARGO_CFG_TARGET_*`).
pub struct Target {
    pub goos: String,
    pub goarch: String,
}

impl Target {
    pub fn current() -> Target {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let goarch = match var("CARGO_CFG_TARGET_ARCH").as_str() {
            "aarch64" => "arm64".to_string(),
            "x86_64" => "amd64".to_string(),
            other => other.to_string(),
        };
        let goos = match var("CARGO_CFG_TARGET_OS").as_str() {
            "macos" => "darwin".to_string(),
            other => other.to_string(),
        };
        Target { goos, goarch }
    }

    pub fn blob(&self, name: &str) -> String {
        let ext = if self.goos == "windows" { ".exe" } else { "" };
        format!("{name}-{}-{}{ext}", self.goos, self.goarch)
    }
}

/// The newest modification time under `p`.
fn newest(p: &Path) -> Option<SystemTime> {
    let m = fs::metadata(p).ok()?;
    if !m.is_dir() {
        return m.modified().ok();
    }
    fs::read_dir(p)
        .ok()?
        .flatten()
        .filter_map(|e| newest(&e.path()))
        .max()
}

pub fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Where the blobs for `names` are: `PROTECTORATE_NET_DIR`, else `<root>/go/bin`, refreshed
/// first when a Go source is newer than a blob or a blob is missing. `root` is the repository.
pub fn blob_dir(root: &Path, names: &[&str], t: &Target) -> PathBuf {
    println!("cargo:rerun-if-env-changed=PROTECTORATE_NET_DIR");
    if let Some(d) = std::env::var_os("PROTECTORATE_NET_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    let dir = root.join("go/bin");
    let sources = [
        root.join("go/go.mod"),
        root.join("go/go.sum"),
        root.join("go/cmd"),
        root.join("scripts/net-build.sh"),
    ];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", s.display());
    }
    let stale = || {
        let src = sources.iter().filter_map(|s| newest(s)).max();
        names
            .iter()
            .filter(|n| {
                fs::metadata(dir.join(t.blob(n)))
                    .and_then(|m| m.modified())
                    .map_or(true, |m| src.is_some_and(|s| s > m))
            })
            .copied()
            .collect::<Vec<_>>()
    };
    if stale().is_empty() || !go_available() {
        return dir;
    }
    // Two build scripts (hub and node) may want the same blob at once.
    let lock = fs::create_dir_all(&dir)
        .and_then(|()| File::create(dir.join(".build.lock")))
        .and_then(|f| f.lock().map(|()| f));
    if let Err(e) = &lock {
        println!("cargo:warning=locking {}: {e}", dir.display());
    }
    for n in stale() {
        let ok = Command::new("sh")
            .arg(root.join("scripts/net-build.sh"))
            .args([n, &t.goos, &t.goarch])
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            println!(
                "cargo:warning=scripts/net-build.sh {n} {} {} failed",
                t.goos, t.goarch
            );
        }
    }
    dir
}

/// `Some(<ty> { name, sha256, gz: include_bytes!(..) })` for blob `name` in `dir`, with its gzip
/// written under `out`; or why there is none. A raw blob is gzipped with the system `gzip`; a
/// `.gz` alone (gzipped on another machine) is inflated here for its SHA-256.
pub fn literal(ty: &str, name: &str, dir: &Path, t: &Target, out: &Path) -> Result<String, String> {
    let raw = dir.join(t.blob(name));
    let given_gz = dir.join(format!("{}.gz", t.blob(name)));
    println!("cargo:rerun-if-changed={}", raw.display());
    println!("cargo:rerun-if-changed={}", given_gz.display());
    // Named by content, so a rerun (a new version stamp) reuses the gzip already made.
    let (sha, gz_path) = if raw.is_file() {
        let bytes = fs::read(&raw).map_err(|e| format!("reading {}: {e}", raw.display()))?;
        let sha = pt_hash::to_hex(&pt_hash::sha256(&bytes));
        let gz_path = out.join(format!("{name}-{sha}.gz"));
        if !gz_path.is_file() {
            let zipped = Command::new("gzip")
                .args(["-n", "-9", "-c"])
                .arg(&raw)
                .output()
                .map_err(|e| format!("gzip {}: {e}", raw.display()))?;
            if !zipped.status.success() {
                return Err(format!("gzip {} failed", raw.display()));
            }
            let tmp = gz_path.with_extension("tmp");
            fs::write(&tmp, &zipped.stdout)
                .and_then(|()| fs::rename(&tmp, &gz_path))
                .map_err(|e| format!("writing {}: {e}", gz_path.display()))?;
        }
        (sha, gz_path)
    } else if given_gz.is_file() {
        let gz = fs::read(&given_gz).map_err(|e| format!("reading {}: {e}", given_gz.display()))?;
        let bytes = crate::gunzip(&gz, crate::MAX_UNPACKED)
            .map_err(|e| format!("{}: {e}", given_gz.display()))?;
        let sha = pt_hash::to_hex(&pt_hash::sha256(&bytes));
        let gz_path = out.join(format!("{name}-{sha}.gz"));
        fs::write(&gz_path, &gz).map_err(|e| format!("writing {}: {e}", gz_path.display()))?;
        (sha, gz_path)
    } else {
        return Err(format!("no {}", raw.display()));
    };
    let gz = gz_path.to_str().ok_or("OUT_DIR is not UTF-8")?;
    Ok(format!(
        "Some({ty} {{ name: {name:?}, sha256: {sha:?}, gz: include_bytes!({gz:?}) }})"
    ))
}
