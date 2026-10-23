#![cfg(target_os = "linux")]

use protectorate_sandbox::{
    runtime::{self, LaunchOptions},
    Access, BindGrant, Limits, Policy, Request, Workload,
};
use std::{collections::BTreeMap, fs, process::Command};

#[test]
fn sleeve_builds_with_a_named_read_only_sibling_module() {
    sleeve_build(false);
}

#[test]
fn sleeve_builds_with_a_remapped_read_only_sibling_module() {
    sleeve_build(true);
}

fn sleeve_build(remap: bool) {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let owner = temp.path().join("owner");
    let workspace = state.join("workspaces/app");
    let sibling = state.join("workspaces/lib");
    let target = if remap {
        "/deps/lib".into()
    } else {
        sibling.clone()
    };
    let include = if remap { "/deps/lib" } else { "../lib" };
    let home = state.join("sleeves/build/home");
    for path in [&state, &owner, &workspace, &sibling, &home] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(
        workspace.join("main.c"),
        "#include \"module.h\"\nint main(void) { return module_answer() != 42; }\n",
    )
    .unwrap();
    fs::write(
        sibling.join("module.h"),
        "static int module_answer(void) { return 42; }\n",
    )
    .unwrap();

    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let request = Request {
        workload: Workload::ChatSleeve,
        command: vec![
            "/usr/bin/cc".into(),
            "-I".into(),
            include.into(),
            "main.c".into(),
            "-o".into(),
            "program".into(),
        ],
        workspace: Some(workspace.clone()),
        managed_workspace_name: Some("app".into()),
        private_home: Some(home),
        grants: vec![BindGrant {
            source: sibling.clone(),
            target,
            access: Access::ReadOnly,
        }],
        tool_roots: Vec::new(),
        selected_executable: None,
        credential: None,
        overlays: Vec::new(),
        masks: Vec::new(),
        limits: Limits {
            cpu_percent: 100,
            memory_bytes: 512 << 20,
            tasks_max: 64,
        },
    };
    let plan = policy.plan(request).unwrap();
    let options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
        temp.path().join("runtime"),
        owner,
        &workspace,
    );
    let output = runtime::spawn(&plan, &options)
        .unwrap_or_else(|error| {
            panic!("real sleeve build needs sandbox host capabilities: {error}")
        })
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "compiler failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(workspace.join("program"))
        .status()
        .unwrap()
        .success());
    assert_eq!(
        fs::read_to_string(sibling.join("module.h")).unwrap(),
        "static int module_answer(void) { return 42; }\n"
    );
}
