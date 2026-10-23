#![cfg(target_os = "linux")]
use protectorate_sandbox::{
    runtime::{self, LaunchOptions},
    Access, BindGrant, CacheOverlay, FileMask, Helper, Limits, Policy, Request, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn direct_harness_wait_kills_descendants_before_reaping_leader() {
    let fixture = Fixture::new();
    for with_output in [false, true] {
        let marker = fixture
            .root
            .path()
            .join(format!("descendant-{with_output}.pid"));
        let script = format!(
        "import subprocess; p=subprocess.Popen(['/usr/bin/sleep','60']); open({:?},'w').write(str(p.pid))",
        marker.to_string_lossy()
    );
        let plan = fixture
            .policy
            .plan(Request {
                workload: Workload::DirectHarnessDriver,
                command: vec!["/usr/bin/python3".into(), "-c".into(), script],
                workspace: None,
                managed_workspace_name: None,
                private_home: None,
                grants: Vec::new(),
                tool_roots: Vec::new(),
                selected_executable: None,
                credential: None,
                overlays: Vec::new(),
                masks: Vec::new(),
                egress: None,
                limits: Limits {
                    cpu_percent: 0,
                    memory_bytes: 0,
                    tasks_max: 0,
                },
            })
            .unwrap();
        let options = LaunchOptions::new("", "", &fixture.policy.user_home, fixture.root.path());
        let child = runtime::spawn_direct_harness(
            &plan,
            &LaunchOptions {
                stdio: runtime::StdioMode::Duplex,
                ..options
            },
        )
        .unwrap();
        let started = Instant::now();
        if with_output {
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
        } else {
            assert!(child.wait().unwrap().success());
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "inherited pipe remained open"
        );
        let descendant: i32 = fs::read_to_string(marker).unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let state = fs::read_to_string(format!("/proc/{descendant}/stat"));
            if state.is_err() || state.unwrap().split_whitespace().nth(2) == Some("Z") {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = fs::read_to_string(format!("/proc/{descendant}/stat"));
        let alive = state
            .as_ref()
            .is_ok_and(|s| s.split_whitespace().nth(2) != Some("Z"));
        if alive {
            unsafe {
                libc::kill(descendant, libc::SIGKILL);
            }
        }
        assert!(!alive, "direct harness descendant survived group cleanup");
    }
}
struct Fixture {
    root: tempfile::TempDir,
    policy: Policy,
    workspace: PathBuf,
    home: PathBuf,
    reference: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let owner = root.path().join("owner");
        let workspace = root.path().join("project");
        let home = state.join("sleeves/one/home");
        let reference = root.path().join("reference");
        for p in [
            &state,
            &owner,
            &workspace,
            &home,
            &reference,
            &state.join("masks"),
        ] {
            fs::create_dir_all(p).unwrap()
        }
        fs::write(state.join("masks/empty"), b"").unwrap();
        fs::write(
            reference.join("config"),
            b"source credential should be masked",
        )
        .unwrap();
        fs::write(reference.join("readme"), b"reference contents").unwrap();
        fs::write(
            root.path().join("outside-secret"),
            b"owner private material",
        )
        .unwrap();
        fs::write(state.join("token"), b"hub secret").unwrap();
        Self {
            root,
            workspace,
            home,
            reference,
            policy: Policy {
                state_root: state,
                user_home: owner,
                protected: Vec::new(),
                provider_credentials: BTreeMap::new(),
                trusted_tool_roots: Vec::new(),
                immutable_cache_roots: Vec::new(),
                nested_cgroups_available: false,
            },
        }
    }
    fn request(&self, script: String) -> Request {
        Request {
            workload: Workload::ChatSleeve,
            command: vec!["/usr/bin/sh".into(), "-c".into(), script],
            workspace: Some(self.workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(self.home.clone()),
            grants: vec![BindGrant {
                source: self.reference.clone(),
                target: self.reference.clone(),
                access: Access::ReadOnly,
            }],
            tool_roots: vec![],
            selected_executable: None,
            credential: None,
            overlays: vec![],
            masks: vec![FileMask {
                source: self.policy.state_root.join("masks/empty"),
                target: self.reference.join("config"),
            }],
            egress: None,
            limits: Limits {
                cpu_percent: 100,
                memory_bytes: 256 << 20,
                tasks_max: 64,
            },
        }
    }
    fn options(&self) -> LaunchOptions {
        LaunchOptions::new(
            env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
            self.root.path().join("runtime"),
            &self.policy.user_home,
            &self.workspace,
        )
    }
    fn wait_for(&self, name: &str) {
        let start = Instant::now();
        while !self.workspace.join(name).exists() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "workload marker {name} did not appear"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
#[test]
fn real_mount_namespace_workspace_secret_mask_and_readonly_smoke() {
    let f = Fixture::new();
    let script=format!("set -eu; printf workspace-ok > result; printf private-home > \"$HOME/private\"; test ! -e '{}'; test ! -e '{}'; test -r '{}/config'; test ! -s '{}/config'; test -z \"$(cat '{}/config')\"; test \"$(cat '{}/readme')\" = 'reference contents'; if touch '{}/forbidden' 2>/dev/null; then exit 88; fi; test -r /dev/null; test -w /dev/null; test -d /proc/1; test \"$(hostname)\" = protectorate; echo sandbox-ready",f.root.path().join("outside-secret").display(),f.policy.state_root.join("token").display(),f.reference.display(),f.reference.display(),f.reference.display(),f.reference.display(),f.reference.display());
    let plan = f.policy.plan(f.request(script)).unwrap();
    let child = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"));
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "status {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("sandbox-ready"));
    assert_eq!(
        fs::read(f.workspace.join("result")).unwrap(),
        b"workspace-ok"
    );
    assert_eq!(fs::read(f.home.join("private")).unwrap(), b"private-home");
    assert!(!f.reference.join("forbidden").exists());
    assert_eq!(
        fs::read(f.reference.join("config")).unwrap(),
        b"source credential should be masked"
    );
}

#[test]
fn real_streaming_handle_reads_before_exit_and_cancels() {
    use std::io::BufRead;
    let f = Fixture::new();
    let plan = f
        .policy
        .plan(f.request("echo first; sleep 30".into()))
        .unwrap();
    let mut child = runtime::spawn(&plan, &f.options()).unwrap();
    let mut stdout = std::io::BufReader::new(child.take_stdout().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    assert_eq!(first, "first\n");
    child.kill_handle().kill();
    let status = child.wait().unwrap();
    assert!(!status.success());
}

#[test]
fn real_nested_cgroup_delegation_is_bounded_and_writable() {
    let mut f = Fixture::new();
    f.policy.nested_cgroups_available = true;
    let script = "set -eu; root=/run/protectorate/cgroup; test -d \"$root\"; test \"$(cat \"$root/memory.max\")\" = max; grep -q '^0::/run$' /proc/self/cgroup; grep -qw cpu \"$root/cgroup.controllers\"; grep -qw memory \"$root/cgroup.controllers\"; grep -qw pids \"$root/cgroup.controllers\"; mkdir \"$root/child\"; printf '100000 100000\\n' > \"$root/child/cpu.max\"; printf '33554432\\n' > \"$root/child/memory.max\"; printf '16\\n' > \"$root/child/pids.max\"; test \"$(cat \"$root/child/memory.max\")\" = 33554432; rmdir \"$root/child\"; echo nested-ready";
    let mut request = f.request(script.into());
    request.workload = Workload::NestedRuntimeTestSleeve;
    let plan = f.policy.plan(request).unwrap();
    let child = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|e| panic!("REAL nested cgroup prerequisite failure (not skipped): {e}"));
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "status {} stdout {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("nested-ready"));
}

#[test]
fn real_nested_sleeve_launches_child_without_systemd() {
    let mut f = Fixture::new();
    f.policy.nested_cgroups_available = true;
    fs::copy(
        env!("CARGO_BIN_EXE_protectorate-sandbox-nested-probe"),
        f.workspace.join("nested-probe"),
    )
    .unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_protectorate-sandbox-helper"),
        f.workspace.join("sandbox-helper"),
    )
    .unwrap();
    let mut request = f.request(format!("{}/nested-probe", f.workspace.display()));
    request.command = vec![f
        .workspace
        .join("nested-probe")
        .to_string_lossy()
        .into_owned()];
    request.workload = Workload::NestedRuntimeTestSleeve;
    let plan = f.policy.plan(request).unwrap();
    let mut options = f.options();
    options.environment.insert(
        "NESTED_WORKSPACE".into(),
        f.workspace.to_string_lossy().into_owned(),
    );
    let child = runtime::spawn(&plan, &options)
        .unwrap_or_else(|e| panic!("REAL nested cgroup prerequisite failure (not skipped): {e}"));
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "status {} stdout {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("inner-delegated-ready"));
}
fn alive(pid: u32) -> bool {
    let path = PathBuf::from(format!("/proc/{pid}/stat"));
    match fs::read_to_string(path) {
        Ok(s) => !s
            .rsplit_once(") ")
            .is_some_and(|(_, state)| state.starts_with('Z')),
        Err(_) => false,
    }
}
fn wait_dead(pid: u32) {
    let start = Instant::now();
    while alive(pid) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "sandbox descendant {pid} survived cleanup"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn cancellation_kills_namespace_init_and_background_descendants() {
    let f = Fixture::new();
    let plan = f
        .policy
        .plan(f.request("sleep 300 & echo ready > ready; wait".into()))
        .unwrap();
    let mut child = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"));
    f.wait_for("ready");
    let init = child.init_pid;
    // The task/children proc file requires CONFIG_CHECKPOINT_RESTORE, which
    // some production kernels disable. Read the always-present PPid instead.
    let descendants: Vec<u32> = fs::read_dir("/proc")
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let stat = fs::read_to_string(entry.path().join("stat")).ok()?;
            let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
            (fields.get(1)?.parse::<u32>().ok()? == init).then_some(pid)
        })
        .collect();
    assert!(!descendants.is_empty());
    child.cancel().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    wait_dead(init);
    for pid in descendants {
        wait_dead(pid)
    }
}
#[test]
fn cache_overlay_shares_immutable_lower_with_private_upper() {
    use std::os::unix::fs::MetadataExt;
    let mut f = Fixture::new();
    let lower = f.policy.state_root.join("cache/published/data");
    let upper = f.policy.state_root.join("sleeves/one/overlays/test/upper");
    let work = f.policy.state_root.join("sleeves/one/overlays/test/work");
    let target = f.reference.join("cache");
    for path in [&lower, &upper, &work, &target] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(lower.join("shared"), b"published generation").unwrap();
    f.policy.immutable_cache_roots.push(lower.clone());
    let mut request = f.request(format!(
        "set -eu; test ! -e '/.overlay'; test ! -e \"$HOME/.cache/overlays\"; test ! -e '{upper}'; test ! -e '{work}'; test \"$(cat '{t}/shared')\" = 'published generation'; printf copy-up > '{t}/shared'; printf upper-write > '{t}/new'; if touch '{r}/blocked' 2>/dev/null; then exit 88; fi; echo overlay-ready",
        t = target.display(),
        r = f.reference.display(),
        upper = upper.display(),
        work = work.display()
    ));
    request.overlays.push(CacheOverlay {
        lower: lower.clone(),
        upper: upper.clone(),
        work: work.clone(),
        target: target.clone(),
    });
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|e| panic!("REAL overlay capability failure (not skipped): {e}"))
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "status {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("overlay-ready"));
    // The lower generation is shared, never copied or mutated: the pinned
    // published cache on the host is byte-identical after copy-up writes.
    assert_eq!(
        fs::read(lower.join("shared")).unwrap(),
        b"published generation"
    );
    assert!(!lower.join("new").exists());
    // Writes land in per-sleeve state outside the visible home and stay
    // private (0700). No staging mount remains in the workload namespace.
    assert!(!upper.starts_with(&f.home));
    assert!(!work.starts_with(&f.home));
    assert_eq!(fs::read(upper.join("new")).unwrap(), b"upper-write");
    assert_eq!(fs::read(upper.join("shared")).unwrap(), b"copy-up");
    assert_eq!(fs::metadata(&upper).unwrap().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(&work).unwrap().mode() & 0o777, 0o700);
}

#[test]
fn overlay_without_an_installable_target_fails_without_running_the_workload() {
    let mut f = Fixture::new();
    let lower = f.policy.state_root.join("cache/published/data");
    let upper = f.policy.state_root.join("sleeves/one/overlays/test/upper");
    let work = f.policy.state_root.join("sleeves/one/overlays/test/work");
    // The target directory does not exist inside the read-only reference
    // mount, so no mountpoint for the overlay can be installed.
    let target = f.reference.join("cache");
    for path in [&lower, &upper, &work] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(lower.join("shared"), b"published generation").unwrap();
    f.policy.immutable_cache_roots.push(lower.clone());
    let mut request = f.request("printf ran > result".into());
    request.overlays.push(CacheOverlay {
        lower: lower.clone(),
        upper: upper.clone(),
        work: work.clone(),
        target: target.clone(),
    });
    let plan = f.policy.plan(request).unwrap();
    // An overlay requirement the sandbox cannot satisfy must fail the
    // launch, never fall back to running the workload on the host.
    let error = runtime::spawn(&plan, &f.options())
        .and_then(|child| child.wait_with_output())
        .expect_err("an unsatisfiable overlay requirement must fail closed");
    let message = error.to_string();
    assert!(
        message.contains("overlay"),
        "failure must name the overlay requirement: {message}"
    );
    assert!(!f.workspace.join("result").exists());
    assert!(!upper.join("new").exists());
}

#[test]
fn temporary_filesystem_scales_with_the_workload_memory_allowance() {
    let f = Fixture::new();
    let cases: [(u64, u64); 5] = [
        (2 << 30, 1024),
        (256 << 20, 192),
        (128 << 20, 64),
        (64 << 20, 32),
        (0, 0),
    ];
    for (memory_bytes, expected_mib) in cases {
        let mut request = f.request(
            "set -eu; df -m /tmp | sed -n '2p'; df -m /var/tmp | sed -n '2p'; echo tmp-ready"
                .into(),
        );
        request.limits.memory_bytes = memory_bytes;
        if memory_bytes > 0 {
            // The temporary ceiling must stay strictly below the allowance:
            // tmpfs is charged to the cgroup and the workload still needs
            // room for its own memory.
            assert!(runtime::tmpfs_bytes(request.limits) < memory_bytes);
        }
        let plan = f.policy.plan(request).unwrap();
        let output = runtime::spawn(&plan, &f.options())
            .unwrap_or_else(|error| {
                panic!("REAL sandbox capability failure (not skipped): {error}")
            })
            .wait_with_output()
            .unwrap();
        assert!(
            output.status.success(),
            "status {} stderr {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        assert!(stdout.contains("tmp-ready"), "{stdout}");
        let sizes: Vec<u64> = stdout
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .filter_map(|field| field.parse().ok())
            .take(2)
            .collect();
        assert_eq!(sizes.len(), 2, "{stdout}");
        if expected_mib > 0 {
            assert_eq!(
                sizes,
                vec![expected_mib, expected_mib],
                "/tmp and /var/tmp must share one ceiling following the {memory_bytes} byte memory allowance"
            );
        } else {
            for size in sizes {
                assert!(
                    (512..=4096).contains(&size),
                    "an uncapped workload still gets a bounded temporary filesystem: {stdout}"
                );
            }
        }
    }
}

#[test]
fn a_build_sized_temporary_write_fails_closed_only_past_its_ceiling() {
    let f = Fixture::new();
    // A Go build of a repository this size peaks near 1.5 GiB in its
    // temporary work directory. 900 MiB must fit a 2 GiB workload, and the
    // ceiling must stay enforced instead of growing without bound. Writing
    // through /var/tmp consumes the same ceiling, because /var/tmp is a
    // bind mount of /tmp.
    let mut request = f.request(
        "set -eu; dd if=/dev/zero of=/tmp/build-work bs=1M count=900 status=none; stat -c %s /tmp/build-work; rm -f /tmp/build-work; if dd if=/dev/zero of=/var/tmp/overrun bs=1M count=1400 status=none 2>overrun.log; then exit 88; fi; grep -c 'No space left' overrun.log; echo ceiling-enforced".into(),
    );
    request.limits.memory_bytes = 2 << 30;
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|error| panic!("REAL sandbox capability failure (not skipped): {error}"))
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "status {} stdout {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("943718400"), "{stdout}");
    assert!(stdout.contains("ceiling-enforced"), "{stdout}");
    assert!(fs::metadata(f.workspace.join("overrun.log"))
        .map(|m| m.len() > 0)
        .unwrap_or(false));
}

#[test]
fn a_full_temporary_filesystem_fails_with_enospc_instead_of_an_oom_kill() {
    let f = Fixture::new();
    // Regression for the October 2026 sleeve OOM kills: with a 256 MiB
    // allowance, /tmp and /var/tmp each had a ceiling equal to the whole
    // allowance, so filling /tmp and then writing through /var/tmp pushed
    // the cgroup past memory.max and the kernel OOM-killed the workload
    // (no exit status, no diagnostic). Both paths now share one ceiling
    // that stays below the allowance, so the overrun fails with ENOSPC and
    // the workload survives to report it.
    let mut request = f.request(
        "set -eu; [ \"$(stat -c '%d:%i' /tmp)\" = \"$(stat -c '%d:%i' /var/tmp)\" ]; [ \"$(cat /var/tmp/reference/readme)\" = 'reference contents' ]; dd if=/dev/zero of=/tmp/first bs=1M count=188 status=none; if dd if=/dev/zero of=/var/tmp/second bs=1M count=100 status=none 2>overrun.log; then exit 88; fi; grep -c 'No space left' overrun.log; echo enospc-not-oom".into(),
    );
    request.limits.memory_bytes = 256 << 20;
    request.grants.push(BindGrant {
        source: f.reference.clone(),
        target: PathBuf::from("/var/tmp/reference"),
        access: Access::ReadOnly,
    });
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap_or_else(|error| panic!("REAL sandbox capability failure (not skipped): {error}"))
        .wait_with_output()
        .unwrap();
    assert!(
        output.status.success(),
        "status {} stdout {} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("enospc-not-oom"), "{stdout}");
}

#[test]
fn missing_helper_fails_closed() {
    let f = Fixture::new();
    let plan = f.policy.plan(f.request("exit 0".into())).unwrap();
    let mut options = f.options();
    options.helper = f.root.path().join("does-not-exist");
    assert!(runtime::spawn(&plan, &options).is_err());
    assert!(!f.workspace.join("result").exists());
}

#[test]
fn configured_sdk_bin_is_available_and_explicit_path_is_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let sdk = f.policy.user_home.join("development/flutter");
    fs::create_dir_all(sdk.join("bin")).unwrap();
    let executable = sdk.join("bin/flutter");
    fs::write(&executable, "#!/usr/bin/sh\necho sdk-working\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    f.policy.trusted_tool_roots.push(sdk.clone());
    let mut request = f.request("flutter > result".into());
    request.tool_roots.push(sdk.clone());
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "sdk-working\n"
    );
    let mut options = f.options();
    options
        .environment
        .insert("PATH".into(), "/explicit/custom".into());
    let mut request =
        f.request("printf '%s|%s' \"$PATH\" \"$PROTECTORATE_SLEEVE_PATH\" > result".into());
    request.tool_roots.push(sdk);
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &options)
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "/explicit/custom|/explicit/custom"
    );
    fs::write(
        f.home.join(".profile"),
        "if [ -n \"$PROTECTORATE_SLEEVE_PATH\" ]; then PATH=\"$PROTECTORATE_SLEEVE_PATH\"; export PATH; fi\n",
    )
    .unwrap();
    let mut request = f.request(String::new());
    request.command = vec![
        "/usr/bin/bash".into(),
        "-lc".into(),
        "printf '%s' \"$PATH\" > result".into(),
    ];
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &options)
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "/explicit/custom"
    );
}

#[test]
fn a_single_readonly_cli_file_is_visible_without_its_parent_directory() {
    let f = Fixture::new();
    let source = f.root.path().join("protectorate-cli");
    fs::write(&source, b"narrow-cli-proof\n").unwrap();
    fs::write(f.root.path().join("other-tool-secret"), b"hidden\n").unwrap();
    let target = PathBuf::from("/opt/protectorate/bin/protectorate");
    let mut request = f.request(format!(
        "cat {} > result; test ! -e {}",
        target.display(),
        f.root.path().join("other-tool-secret").display()
    ));
    request.grants.push(BindGrant {
        source,
        target,
        access: Access::ReadOnly,
    });
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "narrow-cli-proof\n"
    );
}

#[test]
fn a_cli_built_inside_the_writable_workspace_can_be_mounted_on_path() {
    let f = Fixture::new();
    let source = f.workspace.join("protectorate-cli");
    fs::write(&source, b"self-hosted-cli-proof\n").unwrap();
    let mut request = f.request("cat /opt/protectorate/bin/protectorate > result".into());
    request.grants.push(BindGrant {
        source,
        target: PathBuf::from("/opt/protectorate/bin/protectorate"),
        access: Access::ReadOnly,
    });
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "self-hosted-cli-proof\n"
    );
}

#[test]
fn grok_download_binary_runs_without_mounting_grok_account_data() {
    use std::os::unix::fs::PermissionsExt;

    let mut f = Fixture::new();
    let grok_home = f.policy.user_home.join(".grok");
    let downloads = grok_home.join("downloads");
    fs::create_dir_all(&downloads).unwrap();
    fs::write(grok_home.join("auth.json"), b"host-secret").unwrap();
    let executable = downloads.join("grok-linux-aarch64");
    fs::write(
        &executable,
        format!(
            "#!/usr/bin/sh\ntest ! -e '{}/auth.json' && printf grok-tool-ok > '{}'/result\n",
            grok_home.display(),
            f.workspace.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    f.policy.trusted_tool_roots.push(downloads.clone());
    let mut request = f.request(String::new());
    request.command = vec![executable.to_string_lossy().into_owned()];
    request.tool_roots.push(downloads);
    request.selected_executable = Some(executable);
    let plan = f.policy.plan(request).unwrap();
    let output = runtime::spawn(&plan, &f.options())
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(f.workspace.join("result")).unwrap(),
        "grok-tool-ok"
    );
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-C", dir.to_str().unwrap()])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@x")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@x")
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn git_helper_can_status_a_linked_worktree() {
    let f = Fixture::new();
    let repo = f.root.path().join("repo");
    let worktree = f.root.path().join("task");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@x",
            "commit",
            "--allow-empty",
            "-qm",
            "first",
        ],
    );
    git(
        &repo,
        &["worktree", "add", "-q", worktree.to_str().unwrap()],
    );
    fs::write(worktree.join("dirty.txt"), "in-progress\n").unwrap();
    let home = f.policy.state_root.join("helpers/git/home");
    fs::create_dir_all(&home).unwrap();
    let request = Request {
        workload: Workload::ReadOnlyHelper(Helper::Git),
        command: vec![
            "/usr/bin/git".into(),
            "--no-pager".into(),
            "--no-optional-locks".into(),
            "-c".into(),
            "safe.directory=*".into(),
            "-c".into(),
            "core.hooksPath=/dev/null".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
            "status".into(),
            "--short".into(),
            "--branch".into(),
        ],
        workspace: Some(worktree.clone()),
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
            memory_bytes: 128 << 20,
            tasks_max: 24,
        },
    };
    let plan = f.policy.plan(request).unwrap();
    assert!(
        plan.grants().iter().any(|grant| grant.source != worktree
            && grant.source.ends_with(".git")
            && grant.access == Access::ReadOnly),
        "worktree gitdir/commondir must be granted: {:?}",
        plan.grants()
    );
    let mut options = f.options();
    options.working_directory = worktree.clone();
    options.environment = BTreeMap::from([
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
        ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
    ]);
    let output = runtime::spawn(&plan, &options)
        .unwrap_or_else(|e| panic!("REAL sandbox capability failure (not skipped): {e}"))
        .wait_with_output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "status {} stdout {stdout} stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("##") && stdout.contains("dirty.txt"),
        "git status must see the worktree branch and dirty file: {stdout:?}"
    );
}
