//! Public CLI delegation preserves MCP's stdio protocol and exit status.
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::Duration;

fn retry_executable_busy<T>(mut run: impl FnMut() -> io::Result<T>) -> T {
    for attempt in 0..5 {
        match run() {
            Err(error) if error.kind() == io::ErrorKind::ExecutableFileBusy && attempt < 4 => {
                std::thread::sleep(Duration::from_millis(25));
            }
            result => return result.unwrap(),
        }
    }
    unreachable!()
}

#[test]
fn forwards_mcp_to_sibling_without_writing_on_stdout() {
    let home = tempfile::tempdir().unwrap();
    let cli = home.path().join("protectorate-cli");
    let bridge = home.path().join("protectorate-mcpbridge");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    fs::write(&bridge, "#!/bin/sh\nprintf '%s\\n' \"$@\" >&2\ncat\n").unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o755)).unwrap();

    let mut command = Command::new(&cli);
    command
        .args(["--hub", "https://hub", "mcp", "--list"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = retry_executable_busy(|| command.spawn());
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
    let cli = home.path().join("protectorate-cli");
    let bridge = home.path().join("protectorate-mcpbridge");
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    fs::write(&bridge, "#!/bin/sh\nexit 23\n").unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o755)).unwrap();

    let status = retry_executable_busy(|| Command::new(&cli).args(["mcp", "--list"]).status());
    assert_eq!(status.code(), Some(23));
}

#[test]
fn saved_device_token_precedes_private_local_owner_token() {
    let temp = tempfile::tempdir().unwrap();
    let cli = temp.path().join("protectorate-cli");
    let bridge = temp.path().join("protectorate-mcpbridge");
    let state = temp.path().join("state");
    let credentials = state.join("credentials");
    fs::create_dir_all(&credentials).unwrap();
    fs::set_permissions(&credentials, fs::Permissions::from_mode(0o700)).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_protectorate-cli"), &cli).unwrap();
    fs::write(&bridge, "#!/bin/sh\nprintf '%s' \"$PROTECTORATE_TOKEN\"\n").unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o755)).unwrap();

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
    fs::write(
        &credential,
        format!(r#"{{"hub":"{hub}","secret":"device-secret"}}"#),
    )
    .unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();

    let run = || {
        retry_executable_busy(|| {
            Command::new(&cli)
                .args(["--home", state.to_str().unwrap(), "mcp"])
                .env_remove("PROTECTORATE_TOKEN")
                .env_remove("PROTECTORATE_TOKEN_FILE")
                .env_remove("PROTECTORATE_HOME")
                .output()
        })
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"device-secret");

    fs::remove_file(&credential).unwrap();
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"owner-secret");

    fs::set_permissions(&owner, fs::Permissions::from_mode(0o644)).unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("private regular file"));
}
