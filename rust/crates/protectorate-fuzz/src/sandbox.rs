//! Requests are decoded from bytes against a fixed host tree that contains
//! secrets, other workloads' state and symlinks into both. Any plan the
//! policy accepts must keep all of them out of the sandbox.
use protectorate_sandbox::{
    Access, BindGrant, CacheOverlay, FileMask, Helper, Limits, PlanKind, Policy, Provider, Request,
    Workload,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

const DIRS: &[&str] = &[
    "home/.ssh",
    "home/.claude",
    "home/.cargo/bin",
    "home/secret",
    "home/projects/app/src",
    "home/projects/lib",
    "state/sleeves/s1/home",
    "state/sleeves/s1/overlays/up",
    "state/sleeves/s1/overlays/work",
    "state/sleeves/s2/home",
    "state/helpers/h1/home",
    "state/workspaces/w1/sub",
    "state/workspaces/w2",
    "state/base-views/w1",
    "state/auth/claude",
    "state/auth/codex",
    "state/cache/gen1",
    "state/masks",
    "tools/sdk/bin",
];
const FILES: &[&str] = &[
    "home/.ssh/id_ed25519",
    "home/.claude/.credentials.json",
    "home/secret/token",
    "home/file.txt",
    "home/projects/app/src/main.rs",
    "state/masks/empty",
    "tools/sdk/bin/tool",
];
const LINKS: &[(&str, &str)] = &[
    ("home/projects/app/escape", "../../.ssh"),
    ("home/projects/app/self", "."),
    ("home/projects/to-state", "../../state"),
    ("home/projects/to-secret", "../secret"),
    ("home/projects/to-auth", "../../state/auth/claude"),
    ("state/workspaces/w2/link", "../../sleeves/s2/home"),
    ("tools/sdk/bin/escape", "../../../home/.ssh/id_ed25519"),
];
const CANDIDATES: &[&str] = &[
    "home",
    "home/projects/app",
    "home/projects/lib",
    "home/projects/app/escape",
    "home/projects/app/self",
    "home/projects/to-state",
    "home/projects/to-secret",
    "home/projects/to-auth",
    "home/.ssh",
    "home/.claude",
    "home/secret",
    "home/file.txt",
    "home/.cargo/bin",
    "state",
    "state/sleeves/s1/home",
    "state/sleeves/s2/home",
    "state/helpers/h1/home",
    "state/sleeves/s1/overlays/up",
    "state/sleeves/s1/overlays/work",
    "state/workspaces/w1",
    "state/workspaces/w1/sub",
    "state/workspaces/w2",
    "state/workspaces/w2/link",
    "state/base-views/w1",
    "state/auth/claude",
    "state/cache/gen1",
    "state/masks/empty",
    "tools/sdk",
    "tools/sdk/bin/tool",
    "tools/sdk/bin/escape",
    "",
];
const SYSTEM: &[&str] = &[
    "/",
    "/etc",
    "/proc",
    "/usr",
    "/bin/sh",
    "/run/protectorate/x",
];

struct Input<'a> {
    data: &'a [u8],
    root: &'a Path,
}

impl Input<'_> {
    fn byte(&mut self) -> u8 {
        let Some((&b, rest)) = self.data.split_first() else {
            return 0;
        };
        self.data = rest;
        b
    }
    fn flag(&mut self) -> bool {
        self.byte() & 1 == 1
    }
    fn string(&mut self) -> String {
        let len = (self.byte() % 48) as usize;
        let len = len.min(self.data.len());
        let (head, rest) = self.data.split_at(len);
        self.data = rest;
        String::from_utf8_lossy(head).into_owned()
    }
    fn path(&mut self) -> PathBuf {
        let choice = self.byte() as usize;
        if let Some(c) = CANDIDATES.get(choice) {
            return self.root.join(c);
        }
        if let Some(s) = SYSTEM.get(choice - CANDIDATES.len()) {
            return PathBuf::from(s);
        }
        PathBuf::from(format!("{}/{}", self.root.display(), self.string()))
    }
    fn option_path(&mut self) -> Option<PathBuf> {
        self.flag().then(|| self.path())
    }
    fn paths(&mut self, max: u8) -> Vec<PathBuf> {
        (0..self.byte() % max).map(|_| self.path()).collect()
    }
}

fn tree(root: &Path) {
    for dir in DIRS {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in FILES {
        fs::write(root.join(file), b"x").unwrap();
    }
    fs::set_permissions(
        root.join("tools/sdk/bin/tool"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    for (link, target) in LINKS {
        symlink(target, root.join(link)).unwrap();
    }
}

fn request(input: &mut Input) -> Request {
    let workload = match input.byte() % 7 {
        0 => Workload::ChatSleeve,
        1 => Workload::TerminalSleeve,
        2 => Workload::NestedRuntimeTestSleeve,
        3 => Workload::ReadOnlyHelper(Helper::Git),
        4 => Workload::ReadOnlyHelper(Helper::Handoff),
        5 => Workload::ReadOnlyHelper(Helper::Catalog),
        _ => Workload::DirectHarnessDriver,
    };
    let command = match input.byte() % 3 {
        0 => vec!["/bin/sh".into()],
        1 => vec![input.path().to_string_lossy().into_owned()],
        _ => vec![input.string()],
    };
    let workspace = input.option_path();
    let managed_workspace_name = match input.byte() % 4 {
        0 => None,
        1 => Some("w1".into()),
        2 => Some("w2".into()),
        _ => Some(input.string()),
    };
    let private_home = input.option_path();
    let grants = (0..input.byte() % 4)
        .map(|_| BindGrant {
            source: input.path(),
            target: input.path(),
            access: if input.flag() {
                Access::ReadWrite
            } else {
                Access::ReadOnly
            },
        })
        .collect();
    let tool_roots = input.paths(3);
    let selected_executable = input.option_path();
    let credential = match input.byte() % 5 {
        0 => Some(Provider::Claude),
        1 => Some(Provider::Codex),
        2 => Some(Provider::Grok),
        3 => Some(Provider::Pi),
        _ => None,
    };
    let overlays = (0..input.byte() % 2)
        .map(|_| CacheOverlay {
            lower: input.path(),
            upper: input.path(),
            work: input.path(),
            target: input.path(),
        })
        .collect();
    let masks = (0..input.byte() % 2)
        .map(|_| FileMask {
            source: input.path(),
            target: input.path(),
        })
        .collect();
    let limits = Limits {
        cpu_percent: u32::from(input.byte()) * 10,
        memory_bytes: [0, 64 << 20, 1 << 30, 1][input.byte() as usize % 4],
        tasks_max: [0, 1, 512][input.byte() as usize % 3],
    };
    Request {
        workload,
        command,
        workspace,
        managed_workspace_name,
        private_home,
        grants,
        tool_roots,
        selected_executable,
        credential,
        overlays,
        masks,
        limits,
        egress: None,
    }
}

pub fn sandbox_plan(data: &[u8]) {
    let scratch = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(scratch.path()).unwrap();
    tree(&root);
    let home = root.join("home");
    let state = root.join("state");
    let mut input = Input { data, root: &root };
    let nested_cgroups_available = input.flag();
    let request = request(&mut input);
    let policy = Policy {
        state_root: state.clone(),
        user_home: home.clone(),
        protected: vec![home.join("secret")],
        provider_credentials: BTreeMap::from([
            (Provider::Claude, state.join("auth/claude")),
            (Provider::Codex, home.join(".claude")),
        ]),
        trusted_tool_roots: vec![
            root.join("tools/sdk"),
            home.join(".cargo/bin"),
            home.join(".ssh"),
            state.join("auth"),
        ],
        immutable_cache_roots: vec![state.join("cache/gen1")],
        nested_cgroups_available,
    };
    let workload = request.workload;
    let Ok(plan) = policy.plan(request) else {
        return;
    };
    if plan.kind() != PlanKind::Sandboxed {
        assert_eq!(workload, Workload::DirectHarnessDriver);
        assert!(plan.grants().is_empty() && plan.tool_grants().is_empty());
        assert!(plan.credential().is_none() && plan.private_home().is_none());
        assert!(plan.overlays().is_empty() && plan.masks().is_empty());
        return;
    }
    let private = plan
        .private_home()
        .expect("sandboxed plan has a private home");
    assert!(
        private.starts_with(&state) && private != state,
        "private home {} outside state",
        private.display()
    );
    let secrets = [
        home.join(".ssh"),
        home.join(".claude"),
        home.join("secret"),
        state.join("auth"),
        state.join("sleeves"),
        state.join("helpers"),
    ];
    for grant in plan.grants().iter().chain(plan.tool_grants()) {
        let source = &grant.source;
        let real = fs::canonicalize(source).expect("granted source exists");
        assert_eq!(&real, source, "granted source is not canonical");
        assert!(
            !secrets.iter().any(|s| real.starts_with(s)),
            "grant exposes {}",
            real.display()
        );
        assert!(
            !home.starts_with(&real) && !state.starts_with(&real),
            "grant {} exposes the whole home or state",
            real.display()
        );
        if real.starts_with(&state) {
            assert!(
                real.starts_with(state.join("workspaces"))
                    || real.starts_with(state.join("base-views")),
                "grant exposes state {}",
                real.display()
            );
        }
        if grant.access == Access::ReadWrite {
            assert!(!real.starts_with(state.join("cache")), "writable cache");
        }
        assert!(
            grant.target != home && !grant.target.starts_with(private),
            "grant target {} shadows the private home",
            grant.target.display()
        );
    }
    if let Some(credential) = plan.credential() {
        assert!(!matches!(workload, Workload::ReadOnlyHelper(_)));
        assert_eq!(credential.source, state.join("auth/claude"));
    }
    for overlay in plan.overlays() {
        assert!(overlay.lower.starts_with(state.join("cache/gen1")));
        let workload_state = private.parent().unwrap().join("overlays");
        assert!(overlay.upper.starts_with(&workload_state));
        assert!(overlay.work.starts_with(&workload_state));
    }
}
