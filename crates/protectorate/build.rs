//! Embeds the Flutter web build when `PROTECTORATE_WEB_DIR` names one (the Makefile sets it to
//! `app/build/web`): `$OUT_DIR/web.rs` lists every file as a `pt_hub::WebFile` over
//! `include_bytes!`, with an ETag from its SHA-256. Without it the list is empty and the hub
//! serves a page saying the web build is not embedded.
//!
//! Embeds headscale and member for the target (`$OUT_DIR/net.rs`, gzipped with the system
//! `gzip`, with each binary's SHA-256): from `PROTECTORATE_NET_DIR`, else from `go/bin`, which
//! `scripts/net-build.sh` refreshes first when Go is on PATH and the Go sources are newer.
//! Without them the hub runs with its fleet disabled and says why.
//!
//! Files the page never loads stay out: debug symbols, the opt-in experimental CanvasKit, and the
//! skwasm renderers when the build has no `main.dart.wasm` (a dart2js build loads only
//! `canvaskit/` or `canvaskit/chromium/`).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("directory entry").path())
        .collect();
    entries.sort();
    for p in entries {
        let hidden = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_none_or(|n| n.starts_with('.'));
        if hidden {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn unused(rel: &str, wasm: bool) -> bool {
    rel.ends_with(".symbols")
        || rel.starts_with("canvaskit/experimental_webparagraph/")
        || (!wasm && (rel.starts_with("canvaskit/skwasm") || rel.starts_with("canvaskit/wimp")))
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

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// `pub static NET: pt_hub::Net`, embedding what is there and naming what is not.
fn net(out: &Path) -> String {
    println!("cargo:rerun-if-env-changed=PROTECTORATE_NET_DIR");
    let none = |why: String| {
        format!(
            "pub static NET: pt_hub::Net = pt_hub::Net {{ headscale: None, member: None, missing: {why:?} }};\n"
        )
    };
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "linux" {
        return none(format!("the hub embeds them only for Linux, not {os}"));
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_default()
        .as_str()
    {
        "aarch64" => "arm64".to_string(),
        "x86_64" => "amd64".to_string(),
        other => other.to_string(),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = ["headscale", "member"];
    let given = std::env::var_os("PROTECTORATE_NET_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    let dir = given.clone().unwrap_or_else(|| root.join("go/bin"));
    let file = |name: &str| dir.join(format!("{name}-linux-{arch}"));
    if given.is_none() {
        let sources = [
            root.join("go/go.mod"),
            root.join("go/go.sum"),
            root.join("go/cmd"),
            root.join("scripts/net-build.sh"),
        ];
        for s in &sources {
            println!("cargo:rerun-if-changed={}", s.display());
        }
        let src = sources.iter().filter_map(|s| newest(s)).max();
        let stale = names.iter().any(|n| {
            fs::metadata(file(n))
                .and_then(|m| m.modified())
                .map_or(true, |t| src.is_some_and(|s| s > t))
        });
        if stale && go_available() {
            for n in names {
                let ok = Command::new("sh")
                    .arg(root.join("scripts/net-build.sh"))
                    .args([n, "linux", &arch])
                    .status()
                    .is_ok_and(|s| s.success());
                if !ok {
                    println!("cargo:warning=scripts/net-build.sh {n} linux {arch} failed");
                }
            }
        }
    }
    let mut fields = Vec::new();
    let mut missing = Vec::new();
    for n in names {
        let path = file(n);
        println!("cargo:rerun-if-changed={}", path.display());
        let Ok(bytes) = fs::read(&path) else {
            missing.push(format!("no {}", path.display()));
            fields.push(format!("{n}: None"));
            continue;
        };
        let sha = pt_hash::to_hex(&pt_hash::sha256(&bytes));
        let gz = out.join(format!("{n}.gz"));
        let zipped = Command::new("gzip")
            .args(["-n", "-c"])
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("gzip {}: {e}", path.display()));
        assert!(zipped.status.success(), "gzip {} failed", path.display());
        fs::write(&gz, &zipped.stdout).unwrap_or_else(|e| panic!("write {}: {e}", gz.display()));
        let gz = gz.to_str().expect("UTF-8 OUT_DIR");
        fields.push(format!(
            "{n}: Some(pt_hub::Embedded {{ name: {n:?}, sha256: {sha:?}, gz: include_bytes!({gz:?}) }})"
        ));
    }
    if !missing.is_empty() && given.is_none() && !go_available() {
        missing.push("Go is not on PATH to build them".into());
    }
    if !missing.is_empty() {
        println!(
            "cargo:warning=headscale and member are not embedded ({}); the fleet is disabled",
            missing.join("; ")
        );
    }
    format!(
        "pub static NET: pt_hub::Net = pt_hub::Net {{ {}, missing: {:?} }};\n",
        fields.join(", "),
        missing.join("; ")
    )
}

fn main() {
    println!("cargo:rerun-if-env-changed=PROTECTORATE_WEB_DIR");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("net.rs"), net(&out)).expect("write net.rs");
    let mut src = String::from("pub static FILES: &[pt_hub::WebFile] = &[\n");
    let dir = std::env::var_os("PROTECTORATE_WEB_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    if let Some(dir) = dir {
        println!("cargo:rerun-if-changed={}", dir.display());
        if dir.join("index.html").is_file() {
            let root = fs::canonicalize(&dir).expect("canonical web dir");
            let mut files = Vec::new();
            walk(&root, &mut files);
            let wasm = root.join("main.dart.wasm").is_file();
            for f in &files {
                let rel = f.strip_prefix(&root).expect("under the root");
                let rel = rel.to_str().expect("UTF-8 web path");
                if unused(rel, wasm) {
                    continue;
                }
                let bytes = fs::read(f).unwrap_or_else(|e| panic!("read {}: {e}", f.display()));
                let etag = &pt_hash::to_hex(&pt_hash::sha256(&bytes))[..20];
                let abs = f.to_str().expect("UTF-8 web path");
                let _ = writeln!(
                    src,
                    "    pt_hub::WebFile {{ path: {rel:?}, etag: {etag:?}, bytes: include_bytes!({abs:?}) }},"
                );
            }
        } else {
            println!(
                "cargo:warning=PROTECTORATE_WEB_DIR={} has no index.html; the web app is not embedded",
                dir.display()
            );
        }
    }
    src.push_str("];\n");
    fs::write(out.join("web.rs"), src).expect("write web.rs");
}
