use protectorate_sandbox::{
    Access, BindGrant, CacheOverlay, Cgroup, FileMask, Helper, Limits, Network, PlanKind, Policy,
    Provider, Request, Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture {
    dir: tempfile::TempDir,
    policy: Policy,
    workspace: PathBuf,
    private_home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let owner = dir.path().join("owner");
        let workspace = dir.path().join("project");
        let private_home = state.join("sleeves/one/home");
        for path in [&state, &owner, &workspace, &private_home] {
            fs::create_dir_all(path).unwrap();
        }
        let creds = state.join("auth/claude");
        fs::create_dir_all(&creds).unwrap();
        let policy = Policy {
            state_root: state,
            user_home: owner,
            protected: Vec::new(),
            provider_credentials: BTreeMap::from([(Provider::Claude, creds)]),
            trusted_tool_roots: Vec::new(),
            immutable_cache_roots: Vec::new(),
            nested_cgroups_available: false,
        };
        Self {
            dir,
            policy,
            workspace,
            private_home,
        }
    }

    fn request(&self, workload: Workload) -> Request {
        let private_home = match workload {
            Workload::ReadOnlyHelper(_) => self.policy.state_root.join("helpers/one/home"),
            Workload::SharedNodeJob => self.policy.state_root.join("jobs/one/home"),
            _ => self.private_home.clone(),
        };
        fs::create_dir_all(&private_home).unwrap();
        Request {
            workload,
            command: vec!["/usr/bin/true".into()],
            workspace: Some(self.workspace.clone()),
            managed_workspace_name: None,
            private_home: Some(private_home),
            grants: Vec::new(),
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: Limits {
                cpu_percent: 100,
                memory_bytes: 1 << 30,
                tasks_max: 100,
            },
        }
    }
}

fn ro(path: &Path) -> BindGrant {
    BindGrant {
        source: path.to_path_buf(),
        target: path.to_path_buf(),
        access: Access::ReadOnly,
    }
}

#[test]
fn chat_and_terminal_plans_have_private_home_selected_credential_and_shared_network() {
    let f = Fixture::new();
    for workload in [Workload::ChatSleeve, Workload::TerminalSleeve] {
        let mut req = f.request(workload);
        req.credential = Some(Provider::Claude);
        let plan = f.policy.plan(req).unwrap();
        assert_eq!(plan.kind(), PlanKind::Sandboxed);
        assert_eq!(plan.network(), Network::Shared);
        assert_eq!(plan.cgroup(), Some(Cgroup::Bounded));
        assert_eq!(plan.private_home(), Some(f.private_home.as_path()));
        assert_eq!(
            plan.credential().unwrap().target,
            Path::new("/run/protectorate/auth/claude")
        );
        assert_eq!(plan.grants()[0].access, Access::ReadWrite);
    }
}

#[test]
fn named_managed_workspace_is_narrow_and_sibling_state_is_denied() {
    let f = Fixture::new();
    let managed = f.policy.state_root.join("workspaces/one");
    let sibling = f.policy.state_root.join("workspaces/two");
    fs::create_dir_all(&managed).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    let mut request = f.request(Workload::ChatSleeve);
    request.workspace = Some(managed.clone());
    assert!(f.policy.plan(request.clone()).is_err());
    request.managed_workspace_name = Some("one".into());
    assert!(f.policy.plan(request.clone()).is_ok());
    request.workspace = Some(sibling);
    assert!(f.policy.plan(request.clone()).is_err());
    request.workspace = Some(f.policy.state_root.join("auth"));
    assert!(f.policy.plan(request.clone()).is_err());
    request.workspace = Some(f.policy.state_root.clone());
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn managed_base_view_grant_is_read_only_and_limited_to_its_sleeve() {
    let f = Fixture::new();
    let workspace = f.policy.state_root.join("workspaces/one");
    let own = f.policy.state_root.join("base-views/one");
    let sibling = f.policy.state_root.join("base-views/two");
    for path in [&workspace, &own, &sibling] {
        fs::create_dir_all(path).unwrap();
    }
    let mut request = f.request(Workload::ChatSleeve);
    request.workspace = Some(workspace);
    request.managed_workspace_name = Some("one".into());
    request.grants.push(ro(&own));
    assert!(f.policy.plan(request.clone()).is_ok());
    request.grants = vec![ro(&sibling)];
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants = vec![ro(&own.join("config"))];
    fs::write(own.join("config"), "safe").unwrap();
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants = vec![BindGrant {
        source: own.clone(),
        target: own.clone(),
        access: Access::ReadWrite,
    }];
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn symlink_into_credentials_or_sibling_state_cannot_escape_policy() {
    let f = Fixture::new();
    let secret = f.policy.user_home.join(".ssh");
    fs::create_dir_all(&secret).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = f.dir.path().join("secret-link");
        symlink(&secret, &link).unwrap();
        let mut request = f.request(Workload::ChatSleeve);
        request.workspace = Some(link);
        assert!(f.policy.plan(request).is_err());
        let managed = f.policy.state_root.join("workspaces/one");
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        symlink(f.policy.state_root.join("auth"), &managed).unwrap();
        let mut request = f.request(Workload::ChatSleeve);
        request.workspace = Some(managed);
        request.managed_workspace_name = Some("one".into());
        assert!(f.policy.plan(request).is_err());
    }
}

#[test]
fn sibling_module_grants_work_but_overlapping_ro_rw_and_shadowing_fail() {
    let f = Fixture::new();
    let lib = f.dir.path().join("lib");
    fs::create_dir_all(&lib).unwrap();
    let mut request = f.request(Workload::ChatSleeve);
    request.grants.push(ro(&lib));
    assert_eq!(f.policy.plan(request.clone()).unwrap().grants().len(), 2);
    request.grants.push(BindGrant {
        source: lib.clone(),
        target: f.workspace.join("lib"),
        access: Access::ReadOnly,
    });
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants.pop();
    request.grants.push(BindGrant {
        source: f.workspace.clone(),
        target: lib.join("writable"),
        access: Access::ReadOnly,
    });
    assert!(f.policy.plan(request).is_err());
    let mut request = f.request(Workload::ChatSleeve);
    request.grants.push(BindGrant {
        source: lib,
        target: PathBuf::from("/usr/share"),
        access: Access::ReadOnly,
    });
    assert!(f.policy.plan(request).is_err());

    let mut request = f.request(Workload::ChatSleeve);
    request.grants.push(BindGrant {
        source: f.workspace.clone(),
        target: f.dir.path().join("reference"),
        access: Access::ReadOnly,
    });
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn named_managed_sibling_accepts_real_path_or_validated_read_only_remap() {
    let f = Fixture::new();
    let app = f.policy.state_root.join("workspaces/app");
    let lib = f.policy.state_root.join("workspaces/lib");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&lib).unwrap();
    let mut request = f.request(Workload::ChatSleeve);
    request.workspace = Some(app.clone());
    request.managed_workspace_name = Some("app".into());
    request.grants.push(ro(&lib));
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.grants()[1].target, lib);
    request.grants[0].target = PathBuf::from("/deps/lib");
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.grants()[1].target, PathBuf::from("/deps/lib"));
    request.grants[0].target = f.policy.state_root.join("auth");
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants[0].target = PathBuf::from("/run/protectorate/auth/pi");
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants[0].target = PathBuf::from("/deps/lib");
    request.grants[0].access = Access::ReadWrite;
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants[0].access = Access::ReadOnly;
    request.grants[0].source = f.policy.state_root.join("workspaces");
    request.grants[0].target = request.grants[0].source.clone();
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants[0].source = f.policy.state_root.join("auth");
    request.grants[0].target = request.grants[0].source.clone();
    assert!(f.policy.plan(request).is_err());

    assert_eq!(
        protectorate_sandbox::managed_workspace_name(&f.policy.state_root, &app),
        Some("app".into())
    );
    let mut helper = f.request(Workload::ReadOnlyHelper(protectorate_sandbox::Helper::Git));
    helper.workspace = Some(app);
    helper.managed_workspace_name = Some("app".into());
    assert!(f.policy.plan(helper).is_ok());
}

#[test]
fn symlinked_bind_target_cannot_redirect_mount_into_secrets() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let secret = f.policy.user_home.join(".ssh");
        fs::create_dir_all(&secret).unwrap();
        let link = f.dir.path().join("alias");
        symlink(&secret, &link).unwrap();
        let source = f.dir.path().join("safe");
        fs::create_dir_all(&source).unwrap();
        let mut request = f.request(Workload::ChatSleeve);
        request.grants.push(BindGrant {
            source,
            target: link.join("keys"),
            access: Access::ReadOnly,
        });
        assert!(f.policy.plan(request).is_err());
    }
}

#[test]
fn configured_cargo_and_rustup_roots_support_a_checked_executable_chain() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let mut f = Fixture::new();
        let cargo_bin = f.policy.user_home.join(".cargo/bin");
        let toolchains = f.policy.user_home.join(".rustup/toolchains");
        let native = toolchains.join("stable/bin/cargo");
        fs::create_dir_all(&cargo_bin).unwrap();
        fs::create_dir_all(native.parent().unwrap()).unwrap();
        fs::write(&native, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&native, fs::Permissions::from_mode(0o755)).unwrap();
        let shim = cargo_bin.join("cargo");
        symlink(&native, &shim).unwrap();
        f.policy.trusted_tool_roots = vec![cargo_bin.clone(), toolchains.clone()];
        let mut request = f.request(Workload::ChatSleeve);
        request.tool_roots = vec![cargo_bin.clone(), toolchains.clone()];
        request.selected_executable = Some(shim.clone());
        request.command[0] = shim.to_string_lossy().into_owned();
        let plan = f.policy.plan(request.clone()).unwrap();
        assert_eq!(plan.selected_executable(), Some(shim.as_path()));
        assert_eq!(plan.tool_grants().len(), 2);
        assert!(plan
            .tool_grants()
            .iter()
            .all(|grant| grant.access == Access::ReadOnly));

        let mut wrong_command = request.clone();
        wrong_command.command[0] = "/usr/bin/true".into();
        assert!(f.policy.plan(wrong_command).is_err());

        request.tool_roots.pop();
        assert!(f.policy.plan(request.clone()).is_err());
        request.tool_roots.push(toolchains.clone());
        request
            .tool_roots
            .push(f.policy.user_home.join(".cargo/credentials"));
        fs::create_dir_all(request.tool_roots.last().unwrap()).unwrap();
        assert!(f.policy.plan(request.clone()).is_err());

        request.tool_roots.pop();
        request.grants.push(ro(&cargo_bin));
        assert!(f.policy.plan(request.clone()).is_err());
        request.grants.pop();
        request.grants.push(BindGrant {
            source: cargo_bin.clone(),
            target: cargo_bin,
            access: Access::ReadWrite,
        });
        assert!(f.policy.plan(request).is_err());
    }
}

#[test]
fn go_binary_root_is_allowed_without_mounting_the_owner_module_cache() {
    let mut f = Fixture::new();
    let go_root = f.policy.user_home.join("go");
    let go_bin = go_root.join("bin");
    fs::create_dir_all(go_root.join("pkg/mod/cache/vcs")).unwrap();
    fs::create_dir_all(&go_bin).unwrap();
    f.policy.trusted_tool_roots = vec![go_root.clone(), go_bin.clone()];
    let mut request = f.request(Workload::TerminalSleeve);
    request.tool_roots = vec![go_bin];
    assert!(f.policy.plan(request.clone()).is_ok());
    request.tool_roots = vec![go_root];
    assert!(f.policy.plan(request).is_err());
}

#[cfg(unix)]
#[test]
fn grok_downloads_can_be_a_tool_root_without_exposing_grok_account_data() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let grok_home = f.policy.user_home.join(".grok");
    let downloads = grok_home.join("downloads");
    let account = grok_home.join("auth.json");
    fs::create_dir_all(&downloads).unwrap();
    fs::write(&account, b"secret").unwrap();
    let executable = downloads.join("grok-linux-aarch64");
    fs::write(&executable, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    f.policy.trusted_tool_roots = vec![downloads.clone()];
    let mut request = f.request(Workload::ChatSleeve);
    request.tool_roots = vec![downloads.clone()];
    request.selected_executable = Some(executable.clone());
    request.command[0] = executable.to_string_lossy().into_owned();
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.tool_grants()[0].source, downloads);
    assert_eq!(plan.tool_grants()[0].access, Access::ReadOnly);
    request.tool_roots = vec![grok_home.clone()];
    f.policy.trusted_tool_roots.push(grok_home);
    assert!(f.policy.plan(request).is_err());
}

#[cfg(unix)]
#[test]
fn claude_versions_can_be_a_tool_root_without_exposing_claude_account_data() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let claude_home = f.policy.user_home.join(".local/share/claude");
    let versions = claude_home.join("versions");
    fs::create_dir_all(&versions).unwrap();
    fs::write(claude_home.join(".credentials.json"), b"secret").unwrap();
    let executable = versions.join("2.1.305");
    fs::write(&executable, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    f.policy.trusted_tool_roots = vec![versions.clone()];
    let mut request = f.request(Workload::ChatSleeve);
    request.tool_roots = vec![versions.clone()];
    request.selected_executable = Some(executable.clone());
    request.command[0] = executable.to_string_lossy().into_owned();
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.tool_grants()[0].source, versions);
    assert_eq!(plan.tool_grants()[0].access, Access::ReadOnly);
    request.tool_roots = vec![claude_home.clone()];
    f.policy.trusted_tool_roots.push(claude_home);
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn tool_root_and_executable_symlinks_cannot_enter_secret_paths() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let mut f = Fixture::new();
        let config = f.policy.user_home.join(".config");
        fs::create_dir_all(&config).unwrap();
        let cargo_bin = f.policy.user_home.join(".cargo/bin");
        fs::create_dir_all(cargo_bin.parent().unwrap()).unwrap();
        symlink(&config, &cargo_bin).unwrap();
        f.policy.trusted_tool_roots.push(cargo_bin.clone());
        let mut request = f.request(Workload::ChatSleeve);
        request.tool_roots.push(cargo_bin);
        assert!(f.policy.plan(request).is_err());

        let real_bin = f.policy.user_home.join(".rustup/toolchains");
        fs::create_dir_all(&real_bin).unwrap();
        let secret = config.join("auth");
        fs::write(&secret, b"secret").unwrap();
        let bad = real_bin.join("tool");
        symlink(secret, &bad).unwrap();
        f.policy.trusted_tool_roots.push(real_bin.clone());
        let mut request = f.request(Workload::ChatSleeve);
        request.tool_roots.push(real_bin);
        request.selected_executable = Some(bad);
        assert!(f.policy.plan(request).is_err());
    }
}

#[test]
fn configured_flutter_sdk_can_be_a_tool_root_without_exposing_home() {
    let mut f = Fixture::new();
    let flutter = f.policy.user_home.join("development/flutter");
    fs::create_dir_all(&flutter).unwrap();
    f.policy.trusted_tool_roots.push(flutter.clone());
    let mut request = f.request(Workload::ChatSleeve);
    request.tool_roots.push(flutter.clone());
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.tool_grants()[0].source, flutter);
    assert_eq!(plan.tool_grants()[0].access, Access::ReadOnly);

    request.tool_roots = vec![f.policy.user_home.join(".config")];
    fs::create_dir_all(&request.tool_roots[0]).unwrap();
    f.policy
        .trusted_tool_roots
        .push(request.tool_roots[0].clone());
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn git_helper_binds_linked_worktree_gitdir_outside_the_workspace() {
    let f = Fixture::new();
    let repo_git = f.dir.path().join("repo/.git");
    let worktree_git = repo_git.join("worktrees/task");
    let worktree = f.dir.path().join("task");
    fs::create_dir_all(repo_git.join("objects")).unwrap();
    fs::write(repo_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(&worktree_git).unwrap();
    fs::write(worktree_git.join("commondir"), "../..\n").unwrap();
    fs::create_dir_all(&worktree).unwrap();
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", worktree_git.display()),
    )
    .unwrap();

    let mut request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    request.workspace = Some(worktree.clone());
    let plan = f.policy.plan(request).unwrap();
    let sources: Vec<_> = plan
        .grants()
        .iter()
        .map(|grant| grant.source.clone())
        .collect();
    assert!(sources.contains(&worktree));
    assert!(
        sources.iter().any(|source| source == &repo_git),
        "common git dir must be mounted so git status can read objects: {sources:?}"
    );
    assert!(plan
        .grants()
        .iter()
        .all(|grant| grant.access == Access::ReadOnly));
    assert!(!sources.iter().any(|source| source == &worktree_git));
}

#[test]
fn git_helper_refuses_a_linked_gitdir_inside_protected_host_data() {
    let f = Fixture::new();
    let secret = f.policy.user_home.join(".ssh/hidden.git");
    let worktree = f.dir.path().join("task");
    fs::create_dir_all(secret.join("objects")).unwrap();
    fs::write(secret.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(&worktree).unwrap();
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", secret.display()),
    )
    .unwrap();
    let mut request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    request.workspace = Some(worktree.clone());
    let error = f.policy.plan(request).unwrap_err().to_string();
    assert!(error.contains("which is not readable"), "{error}");
    assert!(error.contains(&worktree.display().to_string()), "{error}");
}

#[test]
fn ordinary_git_checkout_does_not_add_extra_helper_binds() {
    let f = Fixture::new();
    fs::create_dir_all(f.workspace.join(".git/objects")).unwrap();
    fs::write(f.workspace.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    let plan = f.policy.plan(request).unwrap();
    assert_eq!(plan.grants().len(), 1);
    assert_eq!(plan.grants()[0].source, f.workspace);
}

fn git(dir: &Path, args: &[&str]) {
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
fn git_helper_binds_a_real_git_worktree_commondir() {
    let f = Fixture::new();
    let repo = f.dir.path().join("repo");
    let worktree = f.dir.path().join("task");
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
    let gitdir = fs::read_to_string(worktree.join(".git")).unwrap();
    assert!(
        gitdir.contains("gitdir:"),
        "real worktree must use a gitdir file: {gitdir:?}"
    );

    let mut request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    request.workspace = Some(worktree.clone());
    let plan = f.policy.plan(request).unwrap();
    let sources: Vec<_> = plan
        .grants()
        .iter()
        .map(|grant| grant.source.clone())
        .collect();
    let repo_git = fs::canonicalize(repo.join(".git")).unwrap();
    assert!(
        sources.iter().any(|source| source == &repo_git),
        "real worktree commondir must be granted so git status can read objects: {sources:?}"
    );
    assert!(plan
        .grants()
        .iter()
        .all(|grant| grant.access == Access::ReadOnly));

    let mut chat = f.request(Workload::ChatSleeve);
    chat.workspace = Some(worktree);
    let chat_plan = f.policy.plan(chat).unwrap();
    assert!(
        !chat_plan
            .grants()
            .iter()
            .any(|grant| grant.source == repo_git),
        "ordinary sleeves keep the Go workspace-only mount; extra git dirs are Git-helper only"
    );
}

#[test]
fn readonly_helpers_and_plain_jobs_are_distinct() {
    let f = Fixture::new();
    let mut request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    let plan = f.policy.plan(request.clone()).unwrap();
    assert!(plan
        .grants()
        .iter()
        .all(|grant| grant.access == Access::ReadOnly));
    request.credential = Some(Provider::Claude);
    assert!(f.policy.plan(request.clone()).is_err());
    request.credential = None;
    request.grants.push(BindGrant {
        source: f.workspace.clone(),
        target: f.dir.path().join("other"),
        access: Access::ReadWrite,
    });
    assert!(f.policy.plan(request).is_err());
    let mut request = f.request(Workload::PlainNodeJob);
    request.workspace = None;
    request.private_home = None;
    let plain = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plain.kind(), PlanKind::IntentionallyUnsandboxedJob);
    assert_eq!(plain.cgroup(), None);
    request.credential = Some(Provider::Claude);
    assert!(f.policy.plan(request).is_err());

    let job = f.request(Workload::SharedNodeJob);
    let plan = f.policy.plan(job).unwrap();
    assert_eq!(plan.kind(), PlanKind::Sandboxed);
    assert_eq!(plan.cgroup(), Some(Cgroup::Bounded));
}

#[test]
fn private_home_cannot_be_auth_or_another_workloads_home() {
    let f = Fixture::new();
    let mut request = f.request(Workload::ChatSleeve);
    request.private_home = Some(f.policy.state_root.join("auth/claude"));
    assert!(f.policy.plan(request.clone()).is_err());
    request.private_home = Some(f.policy.state_root.join("jobs/one/home"));
    fs::create_dir_all(request.private_home.as_ref().unwrap()).unwrap();
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn nested_runtime_tests_require_limits_and_actual_delegation() {
    let mut f = Fixture::new();
    let mut request = f.request(Workload::NestedRuntimeTestSleeve);
    assert!(f.policy.plan(request.clone()).is_err());
    f.policy.nested_cgroups_available = true;
    assert_eq!(
        f.policy.plan(request.clone()).unwrap().cgroup(),
        Some(Cgroup::DelegatedNested)
    );
    request.limits.cpu_percent = 0;
    assert!(f.policy.plan(request.clone()).is_err());
    request.limits.cpu_percent = 100;
    request.limits.memory_bytes = 0;
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn immutable_overlay_layers_under_readonly_tool_mount_without_copying_lower() {
    let mut f = Fixture::new();
    let tool = f.dir.path().join("flutter");
    let target = tool.join("bin/cache");
    let lower = f.policy.state_root.join("cache/published/data");
    let upper = f
        .policy
        .state_root
        .join("sleeves/one/overlays/flutter/upper");
    let work = f
        .policy
        .state_root
        .join("sleeves/one/overlays/flutter/work");
    for path in [&target, &lower, &upper, &work] {
        fs::create_dir_all(path).unwrap();
    }
    f.policy.immutable_cache_roots.push(lower.clone());
    let mut request = f.request(Workload::ChatSleeve);
    request.grants.push(ro(&tool));
    request.overlays.push(CacheOverlay {
        lower: lower.clone(),
        upper: upper.clone(),
        work: work.clone(),
        target: target.clone(),
    });
    let plan = f.policy.plan(request.clone()).unwrap();
    assert_eq!(plan.overlays().len(), 1);
    assert_eq!(plan.overlays()[0].lower, lower);
    assert!(!upper.starts_with(&f.private_home));
    request.overlays[0].upper = f.private_home.join(".cache/overlays/flutter/upper");
    fs::create_dir_all(&request.overlays[0].upper).unwrap();
    assert!(f.policy.plan(request.clone()).is_err());
    request.overlays[0].upper = upper;
    request.overlays[0].work = f.policy.state_root.join("jobs/one/overlays/flutter/work");
    fs::create_dir_all(&request.overlays[0].work).unwrap();
    assert!(f.policy.plan(request.clone()).is_err());
    request.overlays[0].work = work;
    request.grants.pop();
    request.grants.push(BindGrant {
        source: tool.clone(),
        target: tool,
        access: Access::ReadWrite,
    });
    assert!(f.policy.plan(request.clone()).is_err());
    request.grants.pop();
    request.overlays[0].lower = f.workspace.clone();
    assert!(f.policy.plan(request).is_err());
}

#[test]
fn masks_use_empty_regular_state_files_not_dev_null() {
    let f = Fixture::new();
    let repo = f.dir.path().join("base.git");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("config"), b"secret").unwrap();
    let empty = f.policy.state_root.join("masks/empty");
    fs::create_dir_all(empty.parent().unwrap()).unwrap();
    fs::write(&empty, b"").unwrap();
    let mut request = f.request(Workload::ReadOnlyHelper(Helper::Git));
    request.grants.push(ro(&repo));
    request.masks.push(FileMask {
        source: empty.clone(),
        target: repo.join("config"),
    });
    assert!(f.policy.plan(request.clone()).is_ok());
    request.masks[0].source = PathBuf::from("/dev/null");
    assert!(f.policy.plan(request.clone()).is_err());
    request.masks[0].source = empty;
    request.masks[0].target = f.dir.path().join("unmounted/config");
    assert!(f.policy.plan(request).is_err());
}
