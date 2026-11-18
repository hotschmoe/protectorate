#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_sandbox::SandboxSpec;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("pt-sandbox-fuzz-{}-{now}", std::process::id()));
        for path in ["workspace", "home", "scratch", "git/child/nested", "other"] {
            std::fs::create_dir_all(root.join(path)).expect("create sandbox fixture");
        }
        root
    })
}

fuzz_target!(|data: &[u8]| {
    let [git, mask, ..] = data else { return };
    let root = fixture();
    let git = root.join(["git", "git/child"][usize::from(*git) % 2]);
    let mask = root.join(["git", "git/child", "git/child/nested", "other"][usize::from(*mask) % 4]);
    let mut spec = SandboxSpec::new(
        "fuzz-mask",
        1,
        root.join("workspace"),
        root.join("home"),
        root.join("scratch"),
    );
    spec.scope = false;
    spec.git_dirs.push(git.clone());
    assert!(spec.command().is_ok(), "unmasked sandbox is valid");
    spec.masks.push(mask.clone());
    let overlaps = git.starts_with(&mask) || mask.starts_with(&git);
    assert_eq!(
        spec.command().is_err(),
        overlaps,
        "git {git:?}, mask {mask:?}"
    );
});
