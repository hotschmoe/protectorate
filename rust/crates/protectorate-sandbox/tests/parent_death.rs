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

fn launch(root: PathBuf, nested: bool, script: &str) -> runtime::ChildHandle {
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
        nested_cgroups_available: nested,
    };
    let plan = policy
        .plan(Request {
            workload: if nested {
                Workload::NestedRuntimeTestSleeve
            } else {
                Workload::ChatSleeve
            },
            command: vec!["/usr/bin/sh".into(), "-c".into(), script.into()],
            workspace: Some(workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            egress: None,
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
    runtime::spawn(&plan, &options)
        .unwrap_or_else(|error| panic!("real sandbox capability failure: {error}"))
}

fn run_fixture(root: PathBuf, nested: bool) {
    let child = launch(root.clone(), nested, "sleep 300");
    fs::write(root.join("init-pid"), child.init_pid.to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    drop(child);
}

fn active_scopes(supervisor: u32) -> Vec<String> {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--all",
            "--plain",
            "--no-legend",
            &format!("protectorate-{supervisor}-*.scope"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

fn supervisor_death(nested: bool, kill_after: Option<Duration>) {
    let temp = tempfile::tempdir().unwrap();
    let mut supervisor = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            if nested {
                "parent_death::losing_supervisor_releases_nested_scope"
            } else {
                "parent_death::losing_supervisor_kills_namespace_init"
            },
            "--nocapture",
        ])
        .env(
            "PROTECTORATE_PARENT_DEATH_FIXTURE",
            if nested { "nested" } else { "bounded" },
        )
        .env("PROTECTORATE_PARENT_DEATH_ROOT", temp.path())
        .spawn()
        .unwrap();
    let marker = temp.path().join("init-pid");
    let init = if let Some(delay) = kill_after {
        std::thread::sleep(delay);
        None
    } else {
        wait_for(&marker);
        let init: u32 = fs::read_to_string(&marker).unwrap().parse().unwrap();
        assert!(alive(init));
        Some(init)
    };
    supervisor.kill().unwrap();
    supervisor.wait().unwrap();
    let started = Instant::now();
    if let Some(init) = init {
        while alive(init) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "namespace init {init} survived its supervisor"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let pid = supervisor.id();
    // A launcher killed before systemd attached its process leaves an empty
    // scope behind; the next launch on the host releases it.
    let next = tempfile::tempdir().unwrap();
    launch(next.path().to_path_buf(), false, "true")
        .wait_with_output()
        .unwrap();
    loop {
        let scopes = active_scopes(pid);
        if scopes.is_empty() {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "scopes {scopes:?} outlived their supervisor and processes"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn fixture_root() -> Option<(PathBuf, bool)> {
    let kind = std::env::var("PROTECTORATE_PARENT_DEATH_FIXTURE").ok()?;
    let root = std::env::var_os("PROTECTORATE_PARENT_DEATH_ROOT").unwrap();
    Some((PathBuf::from(root), kind == "nested"))
}

#[test]
fn losing_supervisor_kills_namespace_init() {
    if let Some((root, nested)) = fixture_root() {
        run_fixture(root, nested);
        return;
    }
    supervisor_death(false, None);
}

#[test]
fn losing_supervisor_releases_nested_scope() {
    if let Some((root, nested)) = fixture_root() {
        run_fixture(root, nested);
        return;
    }
    supervisor_death(true, None);
}

#[test]
fn losing_supervisor_mid_launch_releases_scope() {
    if let Some((root, nested)) = fixture_root() {
        run_fixture(root, nested);
        return;
    }
    for millis in (0..100).step_by(3) {
        supervisor_death(true, Some(Duration::from_millis(millis)));
    }
}
