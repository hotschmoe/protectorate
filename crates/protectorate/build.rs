//! Embeds the Flutter web build when `PROTECTORATE_WEB_DIR` names one (the Makefile sets it to
//! `app/build/web`): `$OUT_DIR/web.rs` lists every file as a `pt_hub::WebFile` over
//! `include_bytes!`, with an ETag from its SHA-256. Without it the list is empty and the hub
//! serves a page saying the web build is not embedded.
//!
//! Files the page never loads stay out: debug symbols, the opt-in experimental CanvasKit, and the
//! skwasm renderers when the build has no `main.dart.wasm` (a dart2js build loads only
//! `canvaskit/` or `canvaskit/chromium/`).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

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

fn main() {
    println!("cargo:rerun-if-env-changed=PROTECTORATE_WEB_DIR");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
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
