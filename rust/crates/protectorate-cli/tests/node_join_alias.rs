#![cfg(unix)]
use crate::common;

use common::{echo_companion, installed_cli};
use std::process::Command;

#[test]
fn installed_cli_node_join_uses_sibling_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
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
