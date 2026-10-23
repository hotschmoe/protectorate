#![cfg(unix)]
mod common;

use common::{echo_companion, installed_cli};
use std::process::Command;

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
