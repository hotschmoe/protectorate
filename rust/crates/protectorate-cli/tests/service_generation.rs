#![cfg(unix)]
use crate::common;

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

fn installed_cli(root: &Path, with_companion: bool) -> PathBuf {
    let cli = common::installed_cli(root);
    if with_companion {
        fs::write(root.join("protectorate-rs"), b"companion fixture").unwrap();
    }
    cli
}

#[test]
fn systemd_generation_uses_sibling_rust_server_and_keeps_serve_flags() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), true);
    let output = Command::new(&cli)
        .args([
            "service",
            "systemd",
            "hub",
            "--home",
            "/tmp/hub state",
            "--",
            "--listen",
            "127.0.0.1:7480",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let unit = String::from_utf8(output.stdout).unwrap();
    assert!(
        unit.contains(&format!(
            "ExecStart={} serve --listen 127.0.0.1:7480",
            root.path().join("protectorate-rs").display()
        )),
        "{unit}"
    );
    assert!(
        unit.contains("Environment=\"PROTECTORATE_HOME=/tmp/hub state\""),
        "{unit}"
    );
    assert!(!unit.contains("protectorate hub serve"), "{unit}");
    for key in [
        "Restart=on-failure\n",
        "RestartSec=5\n",
        "StartLimitIntervalSec=0\n",
    ] {
        assert!(unit.contains(key), "unit lacks {key:?}: {unit}");
    }
}

#[test]
fn systemd_write_and_install_never_overwrite_existing_unit() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), true);
    let out = root.path().join("generated");
    let first = Command::new(&cli)
        .args([
            "service",
            "systemd",
            "node",
            "--home",
            "/tmp/node",
            "--write",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let path = out.join("protectorate-node.service");
    let original = fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&original).contains("node serve"));
    let second = Command::new(&cli)
        .args([
            "service",
            "systemd",
            "node",
            "--home",
            "/tmp/node",
            "--write",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(fs::read(&path).unwrap(), original);

    let home = root.path().join("user-home");
    fs::create_dir(&home).unwrap();
    let installed = Command::new(&cli)
        .env("HOME", &home)
        .args([
            "service",
            "systemd",
            "hub",
            "--home",
            "/tmp/hub",
            "--install",
        ])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let unit = home.join(".config/systemd/user/protectorate-hub.service");
    let original = fs::read(&unit).unwrap();
    assert!(String::from_utf8_lossy(&original).contains(" serve\n"));
    let retry = Command::new(&cli)
        .env("HOME", &home)
        .args([
            "service",
            "systemd",
            "hub",
            "--home",
            "/tmp/hub",
            "--install",
        ])
        .output()
        .unwrap();
    assert!(!retry.status.success());
    assert_eq!(fs::read(&unit).unwrap(), original);
}

#[test]
fn windows_task_file_is_utf16le_and_keeps_node_serve_arguments() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let out = root.path().join("windows");
    let output = Command::new(&cli)
        .args([
            "service",
            "windows-task",
            "--binary",
            r"C:\Program Files\Protectorate\protectorate-rs.exe",
            "--home",
            r"C:\Node State",
            "--write",
            out.to_str().unwrap(),
            "--trigger",
            "logon",
            "--",
            "--allow-plain-jobs",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = fs::read(out.join("protectorate-node.xml")).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xfe]);
    let xml = String::from_utf16(
        &bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(xml.contains("conhost.exe"), "{xml}");
    assert!(xml.contains("node serve"), "{xml}");
    assert!(xml.contains("--allow-plain-jobs"), "{xml}");
    assert!(xml.contains("C:\\Node State"), "{xml}");
}

#[test]
fn windows_service_prints_commands_and_missing_default_companion_is_clear() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let output = Command::new(&cli)
        .args([
            "service",
            "install-windows",
            "--binary",
            r"C:\Protectorate\protectorate-rs.exe",
            "--home",
            r"C:\State",
            "--",
            "--listen",
            "127.0.0.1:7475",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let script = String::from_utf8(output.stdout).unwrap();
    assert!(
        script.contains("sc.exe create protectorate-node"),
        "{script}"
    );
    assert!(
        script.contains("node serve --listen 127.0.0.1:7475"),
        "{script}"
    );

    let missing = Command::new(&cli)
        .args(["service", "systemd", "hub", "--home", "/tmp/hub"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        stderr.contains("protectorate-rs") && stderr.contains("--binary"),
        "{stderr}"
    );
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    fs::metadata(path).unwrap().permissions().mode() & 0o100 != 0
}

#[test]
fn termux_write_generates_executable_run_and_boot_scripts() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let out = root.path().join("services");
    let binary = root.path().join("protectorate-rs 'quoted' $literal");
    let home = root.path().join("state 'quoted' $(touch injected)");
    let args = [
        "service",
        "termux",
        "--binary",
        binary.to_str().unwrap(),
        "--home",
        home.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--",
        "--listen",
        "127.0.0.1:7475",
    ];

    let first = Command::new(&cli).args(args).output().unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8(first.stdout).unwrap();
    let notes = String::from_utf8(first.stderr).unwrap();
    assert!(stdout.contains("wrote "), "{stdout}");
    assert!(notes.contains("sv-enable protectorate-node"), "{notes}");

    let run = out.join("protectorate-node/run");
    let boot = out.join("boot/protectorate-node");
    let run_text = fs::read_to_string(&run).unwrap();
    let boot_text = fs::read_to_string(&boot).unwrap();
    assert!(
        run_text.starts_with("#!/data/data/com.termux/files/usr/bin/sh\nexec 2>&1\n"),
        "{run_text}"
    );
    assert!(
        run_text.contains("node serve --listen 127.0.0.1:7475"),
        "{run_text}"
    );
    assert!(
        run_text.contains("'\\''quoted'\\'' $literal'"),
        "{run_text}"
    );
    assert!(run_text.contains("PROTECTORATE_HOME='"), "{run_text}");
    assert!(run_text.contains("$(touch injected)"), "{run_text}");
    assert!(boot_text.contains("termux-wake-lock"), "{boot_text}");
    assert!(executable(&run) && executable(&boot));

    let second = Command::new(&cli).args(args).output().unwrap();
    assert!(!second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("exists"), "{stderr}");
    assert_eq!(fs::read_to_string(&run).unwrap(), run_text);
    assert_eq!(fs::read_to_string(&boot).unwrap(), boot_text);
}

#[test]
fn termux_write_checks_both_destinations_before_creating_either_script() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let out = root.path().join("services");
    let boot = out.join("boot/protectorate-node");
    fs::create_dir_all(boot.parent().unwrap()).unwrap();
    fs::write(&boot, b"existing boot script").unwrap();

    let output = Command::new(&cli)
        .args([
            "service",
            "termux",
            "--binary",
            "/data/data/com.termux/files/usr/bin/protectorate-rs",
            "--home",
            "/data/data/com.termux/files/home/.protectorate",
            "--write",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exists"));
    assert!(!out.join("protectorate-node/run").exists());
    assert_eq!(fs::read(boot).unwrap(), b"existing boot script");
}

#[test]
fn termux_print_mode_labels_both_target_paths() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let output = Command::new(&cli)
        .args([
            "service",
            "termux",
            "--binary",
            "/data/data/com.termux/files/usr/bin/protectorate-rs",
            "--home",
            "/data/data/com.termux/files/home/.protectorate",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.starts_with(
            "# $PREFIX/var/service/protectorate-node/run\n#!/data/data/com.termux/files/usr/bin/sh"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "# ~/.termux/boot/protectorate-node\n#!/data/data/com.termux/files/usr/bin/sh"
        ),
        "{text}"
    );
    assert!(
        text.contains("exec /data/data/com.termux/files/usr/bin/protectorate-rs node serve"),
        "{text}"
    );
    assert!(!root.path().join("protectorate-node").exists());
}

#[test]
fn termux_install_writes_only_into_a_genuine_termux_layout() {
    let root = tempfile::tempdir().unwrap();
    let cli = installed_cli(root.path(), false);
    let prefix = root.path().join("com.termux/files/usr");
    let home = root.path().join("com.termux/files/home");
    fs::create_dir_all(prefix.join("bin")).unwrap();
    fs::write(prefix.join("bin/sh"), "#!/bin/sh\n").unwrap();
    let binary = "/data/data/com.termux/files/usr/bin/protectorate-rs";

    let missing = Command::new(&cli)
        .env_remove("PREFIX")
        .args(["service", "termux", "--binary", binary, "--install"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(stderr.contains("needs PREFIX"), "{stderr}");

    let foreign = Command::new(&cli)
        .env("PREFIX", root.path().join("usr"))
        .args(["service", "termux", "--binary", binary, "--install"])
        .output()
        .unwrap();
    assert!(!foreign.status.success());
    let stderr = String::from_utf8_lossy(&foreign.stderr);
    assert!(stderr.contains("com.termux/files/usr"), "{stderr}");

    let foreign_home = Command::new(&cli)
        .env("PREFIX", &prefix)
        .env("HOME", root.path().join("elsewhere"))
        .args(["service", "termux", "--binary", binary, "--install"])
        .output()
        .unwrap();
    assert!(!foreign_home.status.success());
    let stderr = String::from_utf8_lossy(&foreign_home.stderr);
    assert!(stderr.contains("the home beside PREFIX"), "{stderr}");

    let installed = Command::new(&cli)
        .env("PREFIX", &prefix)
        .env("HOME", &home)
        .args([
            "service",
            "termux",
            "--binary",
            binary,
            "--home",
            "/data/data/com.termux/files/home/.protectorate",
            "--install",
        ])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let run = prefix.join("var/service/protectorate-node/run");
    let boot = home.join(".termux/boot/protectorate-node");
    let run_text = fs::read_to_string(&run).unwrap();
    let boot_text = fs::read_to_string(&boot).unwrap();
    assert!(
        run_text.contains("exec /data/data/com.termux/files/usr/bin/protectorate-rs node serve"),
        "{run_text}"
    );
    assert!(boot_text.contains("termux-wake-lock"), "{boot_text}");
    assert!(executable(&run) && executable(&boot));

    let retry = Command::new(&cli)
        .env("PREFIX", &prefix)
        .env("HOME", &home)
        .args([
            "service",
            "termux",
            "--binary",
            binary,
            "--home",
            "/data/data/com.termux/files/home/.protectorate",
            "--install",
        ])
        .output()
        .unwrap();
    assert!(!retry.status.success());
    let stderr = String::from_utf8_lossy(&retry.stderr);
    assert!(stderr.contains("exists"), "{stderr}");
    assert_eq!(fs::read_to_string(&run).unwrap(), run_text);
    assert_eq!(fs::read_to_string(&boot).unwrap(), boot_text);
}
