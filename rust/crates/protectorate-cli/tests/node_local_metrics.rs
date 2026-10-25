use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn local_metrics_collects_without_hub_or_token() {
    let watch = tempfile::tempdir().unwrap();
    fs::write(watch.path().join("sample"), b"local-metrics").unwrap();
    let cli = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_protectorate"))
            .env_remove("PROTECTORATE_TOKEN")
            .env_remove("PROTECTORATE_TOKEN_FILE")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let watched = watch.path().to_str().unwrap();
    let output = cli(&[
        "--hub",
        "http://127.0.0.1:1",
        "node",
        "metrics",
        "--local",
        "--watch",
        watched,
        "--json",
    ]);
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["os"], std::env::consts::OS);
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    assert_eq!(json["arch"], arch);
    assert!(!json["hostname"].as_str().unwrap_or_default().is_empty());
    assert_eq!(json["watchDirs"][0]["path"], watched);
    assert!(
        json["watchDirs"][0]["sizeBytes"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            >= b"local-metrics".len() as u64
    );
    let text = cli(&["node", "metrics", "--local"]);
    assert!(text.starts_with("hostname"));
    assert!(text.contains("platform"));
}
