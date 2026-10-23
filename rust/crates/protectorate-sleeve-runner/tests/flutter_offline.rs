#![cfg(target_os = "linux")]

use protectorate_sleeve_runner::Config;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

// Host integration check. Publish all three generations into one state root
// before running this ignored test; the owner Pub cache must match $HOME.
#[test]
#[ignore]
fn installed_flutter_creates_and_builds_an_offline_project_in_a_sleeve() {
    let sdk = PathBuf::from(std::env::var("PROTECTORATE_TEST_FLUTTER_SDK").unwrap());
    let flutter_lower = PathBuf::from(std::env::var("PROTECTORATE_TEST_FLUTTER_LOWER").unwrap());
    let tools_lower =
        PathBuf::from(std::env::var("PROTECTORATE_TEST_FLUTTER_TOOLS_LOWER").unwrap());
    let pub_lower = PathBuf::from(std::env::var("PROTECTORATE_TEST_PUB_LOWER").unwrap());
    let state = flutter_lower
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for lower in [&flutter_lower, &tools_lower, &pub_lower] {
        assert_eq!(lower.parent(), Some(state.join("cache").as_path()));
        assert!(lower.is_dir());
    }
    let owner = PathBuf::from(std::env::var("HOME").unwrap());
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let workspace = root.path().join("project");
    fs::create_dir(&runtime).unwrap();
    fs::create_dir(&workspace).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("flutter-offline-{}-{nonce}", std::process::id());
    let sleeve = state.join("sleeves").join(&name);
    fs::create_dir_all(&sleeve).unwrap();
    let target = match std::env::consts::ARCH {
        "aarch64" => "linux-arm64",
        "x86_64" => "linux-x64",
        arch => panic!("unsupported Flutter host architecture: {arch}"),
    };
    let config = Config {
        name,
        state_root: state,
        user_home: owner.clone(),
        runtime_dir: runtime,
        helper: PathBuf::from(env!("CARGO_BIN_EXE_protectorate-sleeve-sandbox-helper")),
        workspace: workspace.clone(),
        command: vec![
            "/usr/bin/sh".into(),
            "-c".into(),
            format!("set -eu; test \"$PUB_CACHE\" = \"$HOME/.pub-cache\"; flutter create --offline --platforms=linux --project-name protectorate_sleeve_probe .; flutter build bundle --no-pub --target-platform={target}; test -f build/flutter_assets/AssetManifest.bin; printf private > \"$PUB_CACHE/protectorate-flutter-proof\"; printf private > \"$FLUTTER_ROOT/bin/cache/protectorate-flutter-proof\""),
        ],
        provider: None,
        selected_executable: None,
        read_only: Vec::new(),
        tool_roots: vec![sdk.clone()],
        flutter_sdk_root: Some(sdk.clone()),
        flutter_cache_lower: Some(flutter_lower.clone()),
        flutter_tools_lower: Some(tools_lower.clone()),
        pub_cache_lower: Some(pub_lower.clone()),
        go_mod_cache_lower: None,
        cli_executable: None,
        mcpbridge_executable: None,
        environment: BTreeMap::from([
            ("FLUTTER_SUPPRESS_ANALYTICS".into(), "true".into()),
            ("CI".into(), "true".into()),
            ("FLUTTER_ROOT".into(), sdk.to_string_lossy().into_owned()),
        ]),
        cpu_percent: 200,
        memory_bytes: 6 * 1024 * 1024 * 1024,
        tasks_max: 128,
        nested: false,
        ready_file: sleeve.join("ready"),
    };
    assert_eq!(protectorate_sleeve_runner::run(&config).unwrap(), 0);
    assert!(workspace.join("pubspec.lock").is_file());
    assert!(workspace
        .join("build/flutter_assets/AssetManifest.bin")
        .is_file());
    assert!(!pub_lower.join("protectorate-flutter-proof").exists());
    assert!(!flutter_lower.join("protectorate-flutter-proof").exists());
    assert!(!owner.join(".pub-cache/protectorate-flutter-proof").exists());
    assert!(!sdk.join("bin/cache/protectorate-flutter-proof").exists());
    assert_eq!(
        fs::read(sleeve.join("overlays/pub/upper/protectorate-flutter-proof")).unwrap(),
        b"private"
    );
    assert_eq!(
        fs::read(sleeve.join("overlays/flutter/upper/protectorate-flutter-proof")).unwrap(),
        b"private"
    );
}
