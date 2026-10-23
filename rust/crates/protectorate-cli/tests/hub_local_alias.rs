#![cfg(unix)]
mod common;

use common::{echo_companion, installed_cli};
use std::process::Command;

#[test]
fn installed_cli_forwards_hub_serve_and_backup_to_sibling() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    let serve = Command::new(&cli)
        .args([
            "--home",
            "/tmp/state",
            "hub",
            "serve",
            "--listen",
            "127.0.0.1:0",
        ])
        .output()
        .unwrap();
    assert!(
        serve.status.success(),
        "{}",
        String::from_utf8_lossy(&serve.stderr)
    );
    assert_eq!(
        String::from_utf8(serve.stdout).unwrap(),
        "serve\n--home\n/tmp/state\n--listen\n127.0.0.1:0\n"
    );

    let backup = Command::new(&cli)
        .args([
            "hub",
            "backup",
            "--home",
            "/tmp/state",
            "--to",
            "/tmp/archives",
        ])
        .output()
        .unwrap();
    assert!(
        backup.status.success(),
        "{}",
        String::from_utf8_lossy(&backup.stderr)
    );
    assert_eq!(
        String::from_utf8(backup.stdout).unwrap(),
        "hub\nbackup\n--home\n/tmp/state\n--to\n/tmp/archives\n"
    );
}

#[test]
fn installed_cli_forwards_hub_publish_commands_to_sibling() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    echo_companion(root.path());
    for (args, expected) in [
        (
            vec![
                "--home",
                "/tmp/state",
                "hub",
                "publish-flutter-cache",
                "--flutter-sdk-root",
                "/opt/flutter",
                "--max-bytes",
                "3221225472",
            ],
            "hub\npublish-flutter-cache\n--home\n/tmp/state\n--flutter-sdk-root\n/opt/flutter\n--max-bytes\n3221225472\n",
        ),
        (
            vec![
                "hub",
                "publish-flutter-tools",
                "--home",
                "/tmp/state",
                "--flutter-sdk-root",
                "/opt/flutter",
            ],
            "hub\npublish-flutter-tools\n--home\n/tmp/state\n--flutter-sdk-root\n/opt/flutter\n",
        ),
        (
            vec![
                "hub",
                "publish-pub-cache",
                "--pub-cache-root",
                "/tmp/pub-cache",
            ],
            "hub\npublish-pub-cache\n--pub-cache-root\n/tmp/pub-cache\n",
        ),
    ] {
        let output = Command::new(&cli).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected, "{args:?}");
    }
}

#[test]
fn installed_cli_reports_missing_hub_companion() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    let output = Command::new(&cli)
        .env("PATH", empty_path.path())
        .args(["hub", "backup"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protectorate-rs"), "{stderr}");
}

#[test]
fn installed_cli_reports_missing_companion_for_hub_publish_commands() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path());
    let empty_path = tempfile::tempdir().unwrap();
    for subcommand in [
        "publish-flutter-cache",
        "publish-flutter-tools",
        "publish-pub-cache",
    ] {
        let output = Command::new(&cli)
            .env("PATH", empty_path.path())
            .args(["hub", subcommand])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{subcommand}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("protectorate-rs"), "{subcommand}: {stderr}");
    }
}
