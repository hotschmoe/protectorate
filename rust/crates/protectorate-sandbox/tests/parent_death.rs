#![cfg(target_os = "linux")]

use protectorate_sandbox::{
    runtime::{self, LaunchOptions},
    Limits, Policy, Request, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .is_some_and(|stat| {
            !stat
                .rsplit_once(") ")
                .is_some_and(|(_, state)| state.starts_with('Z'))
        })
}

fn wait_for(path: &Path) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "missing {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_fixture(root: PathBuf) {
    let state = root.join("state");
    let owner = root.join("owner");
    let workspace = root.join("workspace");
    let home = state.join("sleeves/test/home");
    for path in [&state, &owner, &workspace, &home] {
        fs::create_dir_all(path).unwrap();
    }
    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let plan = policy
        .plan(Request {
            workload: Workload::ChatSleeve,
            command: vec!["/usr/bin/sh".into(), "-c".into(), "sleep 300".into()],
            workspace: Some(workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: Limits {
                cpu_percent: 100,
                memory_bytes: 256 << 20,
                tasks_max: 64,
            },
        })
        .unwrap();
    let options = LaunchOptions::new(
        env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
        root.join("runtime"),
        owner,
        workspace,
    );
    let child = runtime::spawn(&plan, &options)
        .unwrap_or_else(|error| panic!("real sandbox capability failure: {error}"));
    fs::write(root.join("init-pid"), child.init_pid.to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    drop(child);
}

#[test]
fn losing_supervisor_kills_namespace_init() {
    if let Some(root) = std::env::var_os("PROTECTORATE_PARENT_DEATH_FIXTURE") {
        run_fixture(PathBuf::from(root));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let mut supervisor = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "losing_supervisor_kills_namespace_init",
            "--nocapture",
        ])
        .env("PROTECTORATE_PARENT_DEATH_FIXTURE", temp.path())
        .spawn()
        .unwrap();
    let marker = temp.path().join("init-pid");
    wait_for(&marker);
    let init: u32 = fs::read_to_string(marker).unwrap().parse().unwrap();
    assert!(alive(init));
    supervisor.kill().unwrap();
    supervisor.wait().unwrap();
    let started = Instant::now();
    while alive(init) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "namespace init {init} survived its supervisor"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
