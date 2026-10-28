use std::{fs, process::Command};

#[test]
fn backup_and_restore_commands_round_trip_selected_hub_state() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let archives = root.path().join("archives");
    let restored = root.path().join("restored");
    fs::create_dir_all(home.join("auth")).unwrap();
    fs::write(home.join("directory.json"), b"workspace directory").unwrap();
    fs::write(home.join("identity.json"), b"device and token identities").unwrap();
    fs::write(home.join("auth/secret"), b"not in backup").unwrap();
    let binary = env!("CARGO_BIN_EXE_protectorate-rs");
    let created = Command::new(binary)
        .args(["hub", "backup", "--home"])
        .arg(&home)
        .arg("--to")
        .arg(&archives)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let output = String::from_utf8(created.stdout).unwrap();
    let mut lines = output.lines();
    let archive = std::path::PathBuf::from(lines.next().unwrap());
    let checksum = lines.next().unwrap();
    assert!(checksum.starts_with("sha256 "));
    assert_eq!(checksum.len(), "sha256 ".len() + 64);
    assert!(archive.is_file());
    assert!(archive.with_extension("pbackup.sha256").is_file());
    let result = Command::new(binary)
        .args(["hub", "restore"])
        .arg(&archive)
        .arg("--home")
        .arg(&restored)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(restored.join("directory.json")).unwrap(),
        b"workspace directory"
    );
    assert_eq!(
        fs::read(restored.join("identity.json")).unwrap(),
        b"device and token identities"
    );
    assert!(!restored.join("auth/secret").exists());
    let repeat = Command::new(binary)
        .args(["hub", "restore"])
        .arg(&archive)
        .arg("--home")
        .arg(&restored)
        .output()
        .unwrap();
    assert!(!repeat.status.success());
}
