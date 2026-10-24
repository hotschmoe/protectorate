#![cfg(unix)]
use crate::common;

use common::{echo_companion, installed_cli};
use std::process::Command;

#[test]
fn installed_cli_doctor_uses_sibling_rust_binary() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = Command::new(cli)
        .args([
            "--token-file",
            "/tmp/unused",
            "--json",
            "node",
            "doctor",
            "--home",
            "/tmp/rust-node",
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
        "node\ndoctor\n--json\n--home\n/tmp/rust-node\n"
    );
}
