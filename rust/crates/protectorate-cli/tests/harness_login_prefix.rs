use std::process::Command;

#[test]
fn global_home_before_login_status_uses_harness_login_path() {
    let home =
        std::env::temp_dir().join(format!("protectorate-login-prefix-{}", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_protectorate-cli"))
        .args(["--home", home.to_str().unwrap(), "login", "--status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    for harness in ["claude", "codex", "grok", "pi"] {
        assert!(stdout.contains(&format!("{harness} ")), "{stdout}");
    }
    assert!(stdout.lines().all(|line| line.ends_with("missing")));
}
