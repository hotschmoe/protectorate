//! Live-test driver executed inside a delegated outer sleeve.
#[cfg(target_os = "linux")]
fn main() {
    use protectorate_sandbox::{runtime, Limits, Policy, Request, Workload};
    use std::{collections::BTreeMap, fs, path::PathBuf};

    let home = PathBuf::from(std::env::var_os("HOME").expect("outer private HOME"));
    let workspace = PathBuf::from(std::env::var_os("NESTED_WORKSPACE").expect("workspace"));
    let state = home.join("nested-state");
    let private_home = state.join("sleeves/child/home");
    let owner = home.join("child-owner");
    let runtime_dir = home.join("nested-runtime");
    for path in [&private_home, &owner, &runtime_dir] {
        fs::create_dir_all(path).expect("create nested runtime layout");
    }
    let policy = Policy {
        state_root: state,
        user_home: owner.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: true,
    };
    let request = Request {
        workload: Workload::ChatSleeve,
        command: vec![
            "/usr/bin/sh".into(),
            "-c".into(),
            "set -eu; grep -q '^0::/$' /proc/self/cgroup; test ! -e /run/protectorate/cgroup; printf inner-ok > nested-result".into(),
        ],
        workspace: Some(workspace.clone()),
        managed_workspace_name: None,
        private_home: Some(private_home),
        grants: Vec::new(),
        tool_roots: Vec::new(),
        selected_executable: None,
        credential: None,
        overlays: Vec::new(),
        masks: Vec::new(),
        egress: None,
        limits: Limits {
            cpu_percent: 50,
            memory_bytes: 128 << 20,
            tasks_max: 32,
        },
    };
    let plan = policy.plan(request.clone()).expect("plan inner sleeve");
    let options = runtime::LaunchOptions::new(
        workspace.join("sandbox-helper"),
        runtime_dir,
        owner,
        workspace.clone(),
    );
    let child = runtime::spawn(&plan, &options).expect("spawn inner sleeve in delegated cgroup");
    let scope = PathBuf::from("/run/protectorate/cgroup").join(child.unit());
    assert_eq!(
        fs::read_to_string(scope.join("cpu.max")).unwrap().trim(),
        "50000 100000"
    );
    assert_eq!(
        fs::read_to_string(scope.join("memory.max")).unwrap().trim(),
        "134217728"
    );
    assert_eq!(
        fs::read_to_string(scope.join("pids.max")).unwrap().trim(),
        "32"
    );
    let output = child.wait_with_output().expect("wait inner sleeve");
    assert!(!scope.exists(), "direct child cgroup was not removed");
    assert!(
        output.status.success(),
        "inner sleeve status {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(workspace.join("nested-result")).unwrap(),
        b"inner-ok"
    );
    let mut nested_request = request;
    nested_request.workload = Workload::NestedRuntimeTestSleeve;
    nested_request.command[2] = "set -eu; root=/run/protectorate/cgroup; grep -q '^0::/run$' /proc/self/cgroup; test -d \"$root\"; mkdir \"$root/grandchild\"; printf '16\\n' > \"$root/grandchild/pids.max\"; rmdir \"$root/grandchild\"; printf nested-inner-ok > nested-delegated-result".into();
    let nested_plan = policy
        .plan(nested_request)
        .expect("plan delegated inner sleeve");
    let nested_child =
        runtime::spawn(&nested_plan, &options).expect("spawn delegated inner sleeve");
    let nested_scope = PathBuf::from("/run/protectorate/cgroup").join(nested_child.unit());
    let nested_output = nested_child
        .wait_with_output()
        .expect("wait delegated inner sleeve");
    assert!(
        !nested_scope.exists(),
        "delegated child cgroup was not removed"
    );
    assert!(
        nested_output.status.success(),
        "delegated inner sleeve status {} stderr {}",
        nested_output.status,
        String::from_utf8_lossy(&nested_output.stderr)
    );
    assert_eq!(
        fs::read(workspace.join("nested-delegated-result")).unwrap(),
        b"nested-inner-ok"
    );
    println!("inner-delegated-ready");
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("nested probe requires Linux");
    std::process::exit(125);
}
