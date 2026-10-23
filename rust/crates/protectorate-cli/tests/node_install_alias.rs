#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

#[cfg(unix)]
fn installed_cli(root: &Path) -> std::path::PathBuf {
    let cli = root.join("protectorate-cli");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    cli
}

#[cfg(unix)]
fn echo_companion(root: &Path) -> std::path::PathBuf {
    let companion = root.join("protectorate-rs");
    fs::write(&companion, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&companion, fs::Permissions::from_mode(0o755)).unwrap();
    companion
}

#[cfg(unix)]
#[test]
fn installed_cli_node_install_forwards_install_argv_to_sibling() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = Command::new(&cli)
        .args([
            "--token-file",
            "/tmp/unused",
            "node",
            "install",
            "--hub",
            "https://hub",
            "--name",
            "gpu1",
            "--home",
            "/tmp/node",
            "--code-file",
            "/tmp/join.code",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "node\ninstall\n--hub\nhttps://hub\n--name\ngpu1\n--home\n/tmp/node\n--code-file\n/tmp/join.code\n--dry-run\n"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_node_install_ignores_the_global_hub_flag() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = Command::new(&cli)
        .args(["--hub", "https://hub", "node", "install", "--name", "gpu1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "node\ninstall\n--name\ngpu1\n"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_node_install_reports_missing_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = Command::new(&cli)
        .env("PATH", empty_path.path())
        .args(["node", "install", "--hub", "https://hub", "--name", "gpu1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}
