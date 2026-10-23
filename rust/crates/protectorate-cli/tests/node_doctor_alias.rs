#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn installed_cli_doctor_uses_sibling_rust_binary() {
    let root = tempfile::tempdir().unwrap();
    let cli = root.path().join("protectorate-cli");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    let companion = root.path().join("protectorate-rs");
    fs::write(&companion, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&companion, fs::Permissions::from_mode(0o755)).unwrap();
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
