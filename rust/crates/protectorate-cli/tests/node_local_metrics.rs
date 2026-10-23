use protectorate_cli::{parse, run_with_status, Command};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn local_metrics_needs_explicit_local_and_keeps_watch_paths() {
    assert!(parse(["node", "metrics"]).is_err());
    assert!(parse(["node", "metrics", "--watch", "/tmp"]).is_err());
    assert!(parse(["node", "metrics", "--local=false"]).is_err());
    let invocation = parse([
        "node",
        "metrics",
        "--json",
        "--watch",
        "/tmp/a",
        "--local",
        "--watch=/tmp/b",
    ])
    .unwrap();
    assert!(invocation.json);
    assert_eq!(
        invocation.command,
        Command::NodeMetricsLocal {
            watch: vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
        }
    );
}

#[test]
fn local_metrics_collects_without_hub_or_token() {
    let watch = tempfile::tempdir().unwrap();
    fs::write(watch.path().join("sample"), b"local-metrics").unwrap();
    let args = [
        "--hub".to_owned(),
        "http://127.0.0.1:1".to_owned(),
        "node".to_owned(),
        "metrics".to_owned(),
        "--local".to_owned(),
        "--watch".to_owned(),
        watch.path().to_string_lossy().into_owned(),
        "--json".to_owned(),
    ];
    let output = run_with_status(&args).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["os"], std::env::consts::OS);
    assert_eq!(json["arch"], std::env::consts::ARCH);
    assert!(!json["hostname"].as_str().unwrap_or_default().is_empty());
    assert_eq!(
        json["watchDirs"][0]["path"],
        watch.path().to_string_lossy().as_ref()
    );
    assert!(
        json["watchDirs"][0]["sizeBytes"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            >= b"local-metrics".len() as u64
    );
    let text = run_with_status(["node", "metrics", "--local"]).unwrap();
    assert!(text.starts_with("hostname"));
    assert!(text.contains("platform"));
}
