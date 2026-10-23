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
fn installed_cli_node_serve_forwards_agent_argv_to_sibling() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = Command::new(&cli)
        .args([
            "--home",
            "/tmp/node",
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/agent.token",
            "--job-concurrency",
            "2",
            "--file-unconfined",
            "--watch-dir",
            "/tmp/build-cache",
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
        "node\nserve\n--home\n/tmp/node\n--agent-token-file\n/tmp/agent.token\n--job-concurrency\n2\n--file-unconfined\n--watch-dir\n/tmp/build-cache\n"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_node_serve_drops_global_rpc_options_and_rejects_json() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = Command::new(&cli)
        .args([
            "--hub",
            "https://hub",
            "--token-file",
            "/tmp/unused",
            "--timeout",
            "30",
            "node",
            "serve",
            "--watch-dir",
            "/tmp/build-cache",
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
        "node\nserve\n--watch-dir\n/tmp/build-cache\n"
    );

    let json = Command::new(&cli)
        .args(["--json", "node", "serve"])
        .output()
        .unwrap();
    assert!(!json.status.success());
    let stderr = String::from_utf8_lossy(&json.stderr);
    assert!(
        stderr.contains("node serve does not support --json"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn installed_cli_node_serve_reports_missing_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = Command::new(&cli)
        .env("PATH", empty_path.path())
        .args(["node", "serve", "--agent-token-file", "/tmp/agent.token"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}
