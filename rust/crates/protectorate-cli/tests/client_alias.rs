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
fn installed_cli_client_invite_uses_local_hub_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let companion = root.path().join("protectorate-rs");
    fs::write(&companion, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&companion, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(&cli)
        .args([
            "--hub",
            "https://unrelated-hub",
            "--home",
            "/tmp/local-hub",
            "client",
            "invite",
            "--name",
            "laptop",
            "--expires",
            "2h",
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
        "client\ninvite\n--home\n/tmp/local-hub\n--name\nlaptop\n--expires\n2h\n"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_client_register_reports_missing_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = Command::new(&cli)
        .env("PATH", empty_path.path())
        .args(["client", "register", "registration-key"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}
