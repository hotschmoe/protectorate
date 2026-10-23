#![cfg(target_os = "linux")]

use protectorate_sleeve_runner::Config;
use std::{collections::BTreeMap, fs, path::PathBuf};

// Publish the owner's installed Pub cache first and pass its printed path as
// PROTECTORATE_TEST_PUB_LOWER. This is an explicit host integration check.
#[test]
#[ignore]
fn installed_dart_resolves_a_cached_package_inside_a_private_pub_overlay() {
    let lower = PathBuf::from(std::env::var("PROTECTORATE_TEST_PUB_LOWER").unwrap());
    let owner = PathBuf::from(std::env::var("HOME").unwrap());
    let dart_sdk = PathBuf::from(std::env::var("PROTECTORATE_TEST_DART_SDK").unwrap_or_else(
        |_| {
            owner
                .join("development/flutter/bin/cache/dart-sdk")
                .to_string_lossy()
                .into_owned()
        },
    ));
    assert!(dart_sdk.join("bin/dart").is_file());
    let root = tempfile::tempdir().unwrap();
    let state = lower.parent().unwrap().parent().unwrap().to_path_buf();
    let runtime = root.path().join("runtime");
    let workspace = root.path().join("workspace");
    for path in [&runtime, &workspace] {
        fs::create_dir(path).unwrap();
    }
    fs::write(workspace.join("pubspec.yaml"), "name: protectorate_pub_probe\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  collection: 1.19.1\n").unwrap();
    let name = format!("pub-probe-{}", std::process::id());
    let sleeve = state.join("sleeves").join(&name);
    fs::create_dir_all(&sleeve).unwrap();
    let config = Config {
        name, state_root: state.clone(), user_home: owner.clone(),
        runtime_dir: runtime, helper: PathBuf::from(env!("CARGO_BIN_EXE_protectorate-sleeve-sandbox-helper")),
        workspace: workspace.clone(),
        command: vec!["/usr/bin/sh".into(), "-c".into(), "set -eu; test \"$PUB_CACHE\" = \"$HOME/.pub-cache\"; command -v dart; command -v protoc-gen-dart; protoc-gen-dart </dev/null > plugin-response.bin; test -s plugin-response.bin; dart pub get --offline; test -f .dart_tool/package_config.json; printf private > \"$PUB_CACHE/hosted/pub.dev/protectorate-private-proof\"".into()],
        provider: None, selected_executable: None, read_only: Vec::new(),
        tool_roots: vec![dart_sdk], flutter_sdk_root: None,
        flutter_cache_lower: None, flutter_tools_lower: None, pub_cache_lower: Some(lower.clone()),
        go_mod_cache_lower: None,
        cli_executable: None, mcpbridge_executable: None, environment: BTreeMap::new(),
        cpu_percent: 0, memory_bytes: 0, tasks_max: 64, nested: false,
        ready_file: sleeve.join("ready"),
        egress: None,
    };
    assert_eq!(protectorate_sleeve_runner::run(&config).unwrap(), 0);
    assert!(!fs::read(workspace.join("plugin-response.bin"))
        .unwrap()
        .is_empty());
    assert!(workspace.join(".dart_tool/package_config.json").is_file());
    assert!(!lower
        .join("hosted/pub.dev/protectorate-private-proof")
        .exists());
    assert!(!owner
        .join(".pub-cache/hosted/pub.dev/protectorate-private-proof")
        .exists());
    assert_eq!(
        fs::read(sleeve.join("overlays/pub/upper/hosted/pub.dev/protectorate-private-proof"))
            .unwrap(),
        b"private"
    );
    let mut second = config.clone();
    second.name = format!("pub-probe-b-{}", std::process::id());
    let second_state = state.join("sleeves").join(&second.name);
    second.ready_file = second_state.join("ready");
    second.command = vec![
        "/usr/bin/sh".into(),
        "-c".into(),
        "set -eu; test ! -e \"$PUB_CACHE/hosted/pub.dev/protectorate-private-proof\"; test -f \"$PUB_CACHE/hosted/pub.dev/collection-1.19.1/pubspec.yaml\"; printf second > \"$PUB_CACHE/hosted/pub.dev/protectorate-second-proof\"".into(),
    ];
    assert_eq!(protectorate_sleeve_runner::run(&second).unwrap(), 0);
    assert_eq!(
        fs::read(second_state.join("overlays/pub/upper/hosted/pub.dev/protectorate-second-proof"))
            .unwrap(),
        b"second"
    );
    assert!(!lower
        .join("hosted/pub.dev/protectorate-second-proof")
        .exists());
    assert!(!owner
        .join(".pub-cache/hosted/pub.dev/protectorate-second-proof")
        .exists());
}
