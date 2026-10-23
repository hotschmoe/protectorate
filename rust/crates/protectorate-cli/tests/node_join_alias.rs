#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

#[cfg(unix)]
fn installed_cli(root: &Path) -> std::path::PathBuf {
    let cli = root.join("protectorate-cli");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    cli
}

#[cfg(unix)]
#[test]
fn installed_cli_node_join_uses_sibling_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let companion = root.path().join("protectorate-rs");
    fs::write(&companion, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&companion, fs::Permissions::from_mode(0o755)).unwrap();
    let join = Command::new(&cli)
        .args([
            "--home",
            "/tmp/node",
            "node",
            "join",
            "--hub",
            "https://hub",
            "--code-file",
            "/tmp/join.code",
        ])
        .output()
        .unwrap();
    assert!(
        join.status.success(),
        "{}",
        String::from_utf8_lossy(&join.stderr)
    );
    assert_eq!(
        String::from_utf8(join.stdout).unwrap(),
        "node\njoin\n--home\n/tmp/node\n--hub\nhttps://hub\n--code-file\n/tmp/join.code\n"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_reports_missing_node_join_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = Command::new(&cli)
        .env("PATH", empty_path.path())
        .args(["node", "join", "--hub", "https://hub"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}
