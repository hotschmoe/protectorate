#![cfg(target_os = "linux")]

use protectorate_sleeve_runner::Config;
use std::{collections::BTreeMap, fs, path::PathBuf};

// Publish the owner's download cache explicitly, then pass its generation as
// PROTECTORATE_TEST_GO_MOD_LOWER. The test builds with network access disabled.
#[test]
#[ignore]
fn installed_go_builds_offline_with_private_module_overlay() {
    let lower = PathBuf::from(std::env::var("PROTECTORATE_TEST_GO_MOD_LOWER").unwrap());
    let owner = PathBuf::from(std::env::var("HOME").unwrap());
    let state = lower.parent().unwrap().parent().unwrap().to_path_buf();
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let workspace = root.path().join("workspace");
    fs::create_dir(&runtime).unwrap();
    fs::create_dir(&workspace).unwrap();
    fs::write(
        workspace.join("go.mod"),
        "module example.org/cacheprobe\n\ngo 1.24\n\nrequire github.com/google/uuid v1.6.0\n",
    )
    .unwrap();
    fs::write(
        workspace.join("go.sum"),
        "github.com/google/uuid v1.6.0 h1:NIvaJDMOsjHA8n1jAhLSgzrAzy1Hgr+hNrb57e+94F0=\ngithub.com/google/uuid v1.6.0/go.mod h1:TIyPZe4MgqvfeYDBFedMoGGpEw/LqOeaOT+nhxU+yHo=\n",
    )
    .unwrap();
    fs::write(
        workspace.join("main.go"),
        "package main\nimport (\"fmt\"; \"github.com/google/uuid\")\nfunc main() { fmt.Print(uuid.Nil.String()) }\n",
    )
    .unwrap();
    let name = format!("go-mod-probe-{}", std::process::id());
    let sleeve = state.join("sleeves").join(&name);
    fs::create_dir_all(&sleeve).unwrap();
    let config = Config {
        name,
        state_root: state.clone(),
        user_home: owner.clone(),
        runtime_dir: runtime,
        helper: PathBuf::from(env!("CARGO_BIN_EXE_protectorate-sleeve-sandbox-helper")),
        workspace: workspace.clone(),
        command: vec![
            "/usr/bin/sh".into(),
            "-c".into(),
            "set -eu; export GOPROXY=off; go build -o offline-probe .; ./offline-probe > result.txt; test -d \"$GOMODCACHE/github.com/google/uuid@v1.6.0\"; printf private > \"$GOMODCACHE/cache/download/private-proof\""
                .into(),
        ],
        provider: None,
        selected_executable: None,
        read_only: Vec::new(),
        tool_roots: vec![owner.join(".local/go")],
        flutter_sdk_root: None,
        flutter_cache_lower: None,
        flutter_tools_lower: None,
        pub_cache_lower: None,
        go_mod_cache_lower: Some(lower.clone()),
        cli_executable: None,
        mcpbridge_executable: None,
        environment: BTreeMap::new(),
        cpu_percent: 0,
        memory_bytes: 0,
        tasks_max: 4096,
        nested: false,
        ready_file: sleeve.join("ready"),
    };
    assert_eq!(protectorate_sleeve_runner::run(&config).unwrap(), 0);
    assert_eq!(
        fs::read_to_string(workspace.join("result.txt")).unwrap(),
        "00000000-0000-0000-0000-000000000000"
    );
    assert!(sleeve.join("overlays/go-mod/upper/private-proof").exists());
    assert!(!lower.join("private-proof").exists());
}
