//! Public CLI delegation preserves MCP's stdio protocol and exit status.
#![cfg(unix)]

mod common;

use common::{installed_cli, write_executable};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

#[test]
fn forwards_mcp_to_sibling_without_writing_on_stdout() {
    let home = tempfile::tempdir().unwrap();
    let cli = installed_cli(home.path());
    write_executable(
        &home.path().join("protectorate-mcpbridge"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >&2\ncat\n",
    );

    let mut command = Command::new(&cli);
    command
        .args(["--hub", "https://hub", "mcp", "--list"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol frame\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"protocol frame\n");
    assert_eq!(output.stderr, b"--hub\nhttps://hub\n--list\n");
}

#[test]
fn forwards_bridge_exit_status() {
    let home = tempfile::tempdir().unwrap();
    let cli = installed_cli(home.path());
    write_executable(
        &home.path().join("protectorate-mcpbridge"),
        "#!/bin/sh\nexit 23\n",
    );

    let status = Command::new(&cli).args(["mcp", "--list"]).status().unwrap();
    assert_eq!(status.code(), Some(23));
}

#[test]
fn saved_device_key_precedes_private_local_owner_token() {
    let temp = tempfile::tempdir().unwrap();
    let cli = installed_cli(temp.path());
    let state = temp.path().join("state");
    let credentials = state.join("credentials");
    fs::create_dir_all(&credentials).unwrap();
    fs::set_permissions(&credentials, fs::Permissions::from_mode(0o700)).unwrap();
    write_executable(
        &temp.path().join("protectorate-mcpbridge"),
        "#!/bin/sh\nprintf '%s|%s' \"$PROTECTORATE_DEVICE_CREDENTIAL\" \"$PROTECTORATE_TOKEN\"\n",
    );

    let owner = state.join("token");
    fs::write(&owner, "owner-secret\n").unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o600)).unwrap();
    let hub = "http://127.0.0.1:7470";
    let digest = Sha256::digest(hub.as_bytes());
    let name = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let credential = credentials.join(format!("{name}.json"));
    let (_, pkcs8) = protectorate_auth::proof::DeviceKey::generate().unwrap();
    use base64::Engine;
    let key = base64::engine::general_purpose::STANDARD.encode(pkcs8);
    fs::write(&credential, format!(r#"{{"hub":"{hub}","key":"{key}"}}"#)).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();

    let run = || {
        Command::new(&cli)
            .args(["--home", state.to_str().unwrap(), "mcp"])
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .env_remove("PROTECTORATE_HOME")
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}|", credential.display())
    );

    fs::remove_file(&credential).unwrap();
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"|owner-secret");

    fs::set_permissions(&owner, fs::Permissions::from_mode(0o644)).unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private regular file"));
}
