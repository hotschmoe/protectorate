//! Resolves the release stamp once, for `pt_version::VERSION`. The `git describe` fallback is
//! rerun when HEAD, the branch it names or the tags move; a dirty mark follows the last such
//! change, not every edit (REVISIT).

use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

fn clean(v: &str) -> Option<String> {
    let v = v.trim();
    let ok = !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-~".contains(&b));
    ok.then(|| v.to_string())
}

fn track_git() {
    let dir = |flag: &str| git(&["rev-parse", flag]).map(PathBuf::from);
    let (Some(gitdir), Some(common)) = (dir("--absolute-git-dir"), dir("--git-common-dir")) else {
        return;
    };
    let common = if common.is_absolute() {
        common
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(common)
    };
    let mut paths = vec![
        gitdir.join("HEAD"),
        common.join("packed-refs"),
        common.join("refs/tags"),
    ];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        paths.push(common.join(branch));
    }
    for p in paths {
        println!("cargo:rerun-if-changed={}", p.display());
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=PROTECTORATE_BUILD_VERSION");
    let given = std::env::var("PROTECTORATE_BUILD_VERSION").ok();
    let version = match given.as_deref().and_then(clean) {
        Some(v) => v,
        None => {
            if given.as_deref().is_some_and(|v| !v.trim().is_empty()) {
                println!(
                    "cargo:warning=PROTECTORATE_BUILD_VERSION={given:?} is not a plain version; ignored"
                );
            }
            track_git();
            git(&[
                "describe", "--tags", "--match", "v[0-9]*", "--always", "--dirty",
            ])
            .and_then(|v| clean(&v))
            .unwrap_or_else(|| "dev".into())
        }
    };
    println!("cargo:rustc-env=PT_BUILD_VERSION={version}");
}
