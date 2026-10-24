#![cfg(unix)]
use crate::common;

use common::{echo_companion, installed_cli};
use std::{
    path::Path,
    process::{Command, Output},
};

fn run(cli: &Path, args: &[&str], path: Option<&Path>) -> Output {
    let mut command = Command::new(cli);
    command.args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

#[test]
fn installed_cli_node_uninstall_forwards_local_argv_to_sibling() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let cases: &[(&[&str], &str)] = &[
        (
            &["node", "uninstall", "--home", "/tmp/node", "--dry-run"],
            "node\nuninstall\n--home\n/tmp/node\n--dry-run\n",
        ),
        (
            &[
                "--token-file",
                "/tmp/unused",
                "--hub",
                "https://hub",
                "--timeout",
                "30",
                "--home",
                "/tmp/node",
                "--dry-run",
                "node",
                "uninstall",
            ],
            "node\nuninstall\n--home\n/tmp/node\n--dry-run\n",
        ),
    ];
    for (args, expected) in cases {
        let output = run(&cli, args, None);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), *expected);
    }
}

#[test]
fn installed_cli_node_uninstall_refuses_json_and_unknown_leading_options() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let cases: &[(&[&str], &str)] = &[
        (&["--json", "node", "uninstall"], "does not support --json"),
        (
            &["--ca-cert", "/tmp/ca.pem", "node", "uninstall"],
            "unknown option before node uninstall",
        ),
        (&["--hub", "node", "uninstall"], "--hub requires a value"),
        (&["--home", "node", "uninstall"], "--home requires a value"),
    ];
    for (args, expected) in cases {
        let output = run(&cli, args, None);
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

#[test]
fn installed_cli_node_uninstall_reports_missing_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = run(
        &cli,
        &["node", "uninstall", "--dry-run"],
        Some(empty_path.path()),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}

#[test]
fn installed_cli_node_install_runner_says_the_backend_is_windows_only() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let output = run(
        &cli,
        &[
            "node",
            "install-runner",
            "--home",
            "/tmp/node",
            "--job-root",
            "/tmp/jobs",
        ],
        None,
    );
    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("supported on Windows only"), "{stderr}");
}

#[test]
fn installed_cli_leaves_other_commands_to_the_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let cases: &[&[&str]] = &[
        &[
            "backlog",
            "add",
            "node",
            "uninstall",
            "--hub",
            "http://127.0.0.1:1",
        ],
        &["node", "rm", "uninstall", "--hub", "http://127.0.0.1:1"],
        &["node", "show", "uninstall", "--hub", "http://127.0.0.1:1"],
    ];
    for args in cases {
        let output = run(&cli, args, None);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "", "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("protectorate-rs"), "{args:?}: {stderr}");
    }
}
