//! Host-side sleeve runner. Tmux owns this process for the entire terminal
//! session; it owns and waits for the sandbox handle for the same duration.
//! A hub RPC never drops a live `ChildHandle`.

use protectorate_audit::{Audit, Entry};
use protectorate_sandbox::{
    egress::{self, Decision, Verdict},
    runtime::{self, LaunchOptions, StdioMode},
    Access, BindGrant, CacheOverlay, Egress, Limits, Policy, Provider, Request, ValidatedPlan,
    Workload,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsStr},
    fs,
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub state_root: PathBuf,
    pub user_home: PathBuf,
    pub runtime_dir: PathBuf,
    pub helper: PathBuf,
    pub workspace: PathBuf,
    pub command: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub selected_executable: Option<PathBuf>,
    pub read_only: Vec<ReadOnly>,
    #[serde(default)]
    pub tool_roots: Vec<PathBuf>,
    #[serde(default)]
    pub flutter_sdk_root: Option<PathBuf>,
    #[serde(default)]
    pub flutter_cache_lower: Option<PathBuf>,
    #[serde(default)]
    pub flutter_tools_lower: Option<PathBuf>,
    #[serde(default)]
    pub pub_cache_lower: Option<PathBuf>,
    #[serde(default)]
    pub go_mod_cache_lower: Option<PathBuf>,
    #[serde(default)]
    pub cli_executable: Option<PathBuf>,
    #[serde(default)]
    pub mcpbridge_executable: Option<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub cpu_percent: u32,
    pub memory_bytes: u64,
    pub tasks_max: u32,
    pub nested: bool,
    pub ready_file: PathBuf,
    pub egress: Option<Egress>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOnly {
    pub source: PathBuf,
    pub target: PathBuf,
}

/// Existing, narrow user tool roots suitable for a default sleeve. Each
/// source is still validated by the sandbox policy before it is mounted.
pub fn default_tool_roots(user_home: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut add = |path: PathBuf| {
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir())
            && fs::canonicalize(&path).is_ok_and(|real| real == path)
        {
            roots.push(path);
        }
    };
    for relative in [
        ".local/bin",
        ".local/lib",
        ".local/opt",
        ".local/go",
        ".local/share/pi-omp-setup",
        "go/bin",
        ".cargo/bin",
        ".rustup/toolchains",
        ".pub-cache/bin",
        ".pub-cache/global_packages",
    ] {
        add(user_home.join(relative));
    }
    let node_versions = user_home.join(".nvm/versions/node");
    if let Ok(entries) = fs::read_dir(node_versions) {
        let mut installed = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.join("bin/node").is_file())
            .collect::<Vec<_>>();
        installed.sort();
        if let Some(latest) = installed.pop() {
            add(latest);
        }
    }
    roots
}

pub fn run_file(path: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    if bytes.len() > 1 << 20 {
        return Err("sleeve config is too large".into());
    }
    let config: Config = serde_json::from_slice(&bytes)?;
    fs::remove_file(path)?;
    run(&config)
}

pub fn helper_main() -> runtime::Result<i32> {
    runtime::helper_main()
}

pub fn run(config: &Config) -> Result<i32, Box<dyn std::error::Error>> {
    let plan = plan(config)?;
    let mut options = LaunchOptions::new(
        &config.helper,
        &config.runtime_dir,
        &config.user_home,
        &config.workspace,
    );
    options.environment = config.environment.clone();
    options.environment.extend(prepare_private_caches(
        &config
            .state_root
            .join("sleeves")
            .join(&config.name)
            .join("home"),
        &config.user_home,
    )?);
    if config.pub_cache_lower.is_some() {
        options.environment.insert(
            "PUB_CACHE".into(),
            config
                .user_home
                .join(".pub-cache")
                .to_string_lossy()
                .into_owned(),
        );
    }
    if let Some(toolchain) = rustup_default_toolchain(&config.user_home, &config.tool_roots) {
        options
            .environment
            .entry("RUSTUP_TOOLCHAIN".into())
            .or_insert(toolchain);
    }
    options.environment.insert(
        "HOME".into(),
        config.user_home.to_string_lossy().into_owned(),
    );
    options
        .environment
        .insert("PROTECTORATE_SLEEVE".into(), config.name.clone());
    options.stdio = StdioMode::Inherit;
    options.egress_log = Some(egress_audit(
        Arc::new(Audit::new(config.state_root.join("audit"))),
        format!("sleeve:{}", config.name),
    ));
    let child = runtime::spawn(&plan, &options)?;
    // The ready file is created only after the sandbox reports successful setup.
    fs::write(&config.ready_file, b"ready\n")?;
    let status = child.wait()?;
    let _ = fs::remove_file(&config.ready_file);
    Ok(status.code().unwrap_or(128))
}

pub fn plan(config: &Config) -> Result<ValidatedPlan, Box<dyn std::error::Error>> {
    if !valid_name(&config.name) {
        return Err("invalid sleeve name".into());
    }
    let home = config
        .state_root
        .join("sleeves")
        .join(&config.name)
        .join("home");
    let root = config.state_root.join("sleeves").join(&config.name);
    let provider = match config.provider.as_deref() {
        None => None,
        Some("claude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        Some("grok") => Some(Provider::Grok),
        Some("pi") => Some(Provider::Pi),
        Some(_) => return Err("unknown terminal provider".into()),
    };
    if !config.ready_file.starts_with(&root) || config.ready_file.parent() != Some(root.as_path()) {
        return Err("readiness path must be in sleeve state".into());
    }
    fs::create_dir_all(&home)?;
    prepare_private_caches(&home, &config.user_home)?;
    let policy = Policy {
        state_root: config.state_root.clone(),
        user_home: config.user_home.clone(),
        protected: Vec::new(),
        provider_credentials: provider
            .map(|selected| {
                (
                    selected,
                    config.state_root.join("auth").join(selected.as_str()),
                )
            })
            .into_iter()
            .collect(),
        trusted_tool_roots: config.tool_roots.clone(),
        immutable_cache_roots: config
            .flutter_cache_lower
            .iter()
            .chain(config.flutter_tools_lower.iter())
            .chain(config.pub_cache_lower.iter())
            .chain(config.go_mod_cache_lower.iter())
            .cloned()
            .collect(),
        nested_cgroups_available: config.nested,
    };
    let plan = policy.plan(Request {
        workload: if config.nested {
            Workload::NestedRuntimeTestSleeve
        } else {
            Workload::TerminalSleeve
        },
        command: config.command.clone(),
        workspace: Some(config.workspace.clone()),
        managed_workspace_name: protectorate_sandbox::managed_workspace_name(
            &config.state_root,
            &config.workspace,
        ),
        private_home: Some(home),
        grants: config
            .read_only
            .iter()
            .map(|r| BindGrant {
                source: r.source.clone(),
                target: r.target.clone(),
                access: Access::ReadOnly,
            })
            .chain(config.cli_executable.iter().map(|source| BindGrant {
                source: source.clone(),
                target: PathBuf::from("/opt/protectorate/bin/protectorate"),
                access: Access::ReadOnly,
            }))
            .chain(config.mcpbridge_executable.iter().map(|source| BindGrant {
                source: source.clone(),
                target: PathBuf::from("/opt/protectorate/bin/protectorate-mcpbridge"),
                access: Access::ReadOnly,
            }))
            .collect(),
        tool_roots: config.tool_roots.clone(),
        selected_executable: config.selected_executable.clone(),
        credential: provider,
        overlays: flutter_overlay(
            &config.state_root,
            &config.name,
            config.flutter_sdk_root.as_deref(),
            config.flutter_cache_lower.as_deref(),
        )?
        .into_iter()
        .chain(flutter_tools_overlay(
            &config.state_root,
            &config.name,
            config.flutter_sdk_root.as_deref(),
            config.flutter_tools_lower.as_deref(),
        )?)
        .chain(pub_cache_overlay(
            &config.state_root,
            &config.name,
            &config.user_home,
            config.pub_cache_lower.as_deref(),
        )?)
        .chain(go_mod_cache_overlay(
            &config.state_root,
            &config.name,
            &config.user_home,
            config.go_mod_cache_lower.as_deref(),
        )?)
        .collect(),
        masks: Vec::new(),
        limits: Limits {
            cpu_percent: config.cpu_percent,
            memory_bytes: config.memory_bytes,
            tasks_max: config.tasks_max,
        },
        egress: config.egress.clone(),
    })?;
    Ok(plan)
}

/// Record each egress decision of one sandbox in the hub audit log.
pub fn egress_audit(audit: Arc<Audit>, actor: String) -> egress::Log {
    Arc::new(move |decision: &Decision| {
        audit.write(Entry {
            actor: actor.clone(),
            procedure: "egress".into(),
            subject: format!("{}:{}", decision.host, decision.port),
            outcome: match decision.verdict {
                Verdict::Allow => protectorate_audit::OUTCOME_OK,
                Verdict::Deny => protectorate_audit::OUTCOME_DENIED,
                Verdict::Error => protectorate_audit::OUTCOME_ERROR,
            }
            .into(),
            detail: decision.reason.clone(),
            ..Entry::default()
        })
    })
}

/// A published SDK cache is mounted over only `bin/cache`; the remaining SDK
/// stays read-only. The caller must explicitly trust and mount the SDK root.
pub fn flutter_overlay(
    state_root: &Path,
    name: &str,
    sdk: Option<&Path>,
    lower: Option<&Path>,
) -> io::Result<Option<CacheOverlay>> {
    let (Some(sdk), Some(lower)) = (sdk, lower) else {
        if sdk.is_some() || lower.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Flutter SDK and published cache must be configured together",
            ));
        }
        return Ok(None);
    };
    published_flutter_overlay(
        state_root,
        name,
        sdk,
        lower,
        "bin/cache",
        "flutter",
        ".protectorate-flutter-cache.json",
    )
    .map(Some)
}

pub fn flutter_tools_overlay(
    state_root: &Path,
    name: &str,
    sdk: Option<&Path>,
    lower: Option<&Path>,
) -> io::Result<Option<CacheOverlay>> {
    let Some(lower) = lower else {
        return Ok(None);
    };
    let sdk = sdk.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Flutter tools lower requires an SDK root",
        )
    })?;
    published_flutter_overlay(
        state_root,
        name,
        sdk,
        lower,
        "packages/flutter_tools/.dart_tool",
        "flutter-tools",
        ".protectorate-flutter-tools.json",
    )
    .map(Some)
}

/// `PUB_CACHE` is private to a sleeve; this overlay lends it immutable host
/// package and global-tool content without mounting the owner's home.
pub fn pub_cache_overlay(
    state_root: &Path,
    name: &str,
    user_home: &Path,
    lower: Option<&Path>,
) -> io::Result<Option<CacheOverlay>> {
    let Some(lower) = lower else { return Ok(None) };
    if !valid_name(name) || !lower.is_absolute() || !user_home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Pub overlay path",
        ));
    }
    if !lower.is_dir() || fs::canonicalize(lower)? != lower {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "published Pub cache must be a real directory",
        ));
    }
    let manifest = lower.join(".protectorate-pub-cache.json");
    let input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(manifest)?;
    let mut bytes = Vec::new();
    use io::Read;
    input.take(4096).read_to_end(&mut bytes)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    let digest = manifest["sha256"].as_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Pub cache manifest lacks hash")
    })?;
    if lower.file_name().and_then(OsStr::to_str) != Some(format!("pub-{digest}").as_str())
        || manifest["source"].as_str()
            != Some(user_home.join(".pub-cache").to_string_lossy().as_ref())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid published Pub cache generation",
        ));
    }
    let parent = state_root.join("sleeves").join(name).join("overlays/pub");
    fs::create_dir_all(&parent)?;
    let upper = parent.join("upper");
    let work = parent.join("work");
    fs::create_dir_all(&upper)?;
    fs::create_dir_all(&work)?;
    seed_pub_writable_dirs(lower, &upper)?;
    Ok(Some(CacheOverlay {
        lower: lower.into(),
        upper,
        work,
        target: user_home.join(".pub-cache"),
    }))
}

/// Reuse a published Go module snapshot while preserving private downloads
/// and extraction in each sleeve's writable upper layer.
pub fn go_mod_cache_overlay(
    state_root: &Path,
    name: &str,
    user_home: &Path,
    lower: Option<&Path>,
) -> io::Result<Option<CacheOverlay>> {
    let Some(lower) = lower else { return Ok(None) };
    if !valid_name(name) || !lower.is_absolute() || !user_home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Go module overlay path",
        ));
    }
    if !lower.is_dir() || fs::canonicalize(lower)? != lower {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "published Go module cache must be a real directory",
        ));
    }
    let manifest = lower.join(".protectorate-go-mod-cache.json");
    let input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(manifest)?;
    let mut bytes = Vec::new();
    use io::Read;
    input.take(4096).read_to_end(&mut bytes)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    let digest = manifest["sha256"].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Go module cache manifest lacks hash",
        )
    })?;
    if lower.file_name().and_then(OsStr::to_str) != Some(format!("go-mod-{digest}").as_str())
        || manifest["source"].as_str()
            != Some(user_home.join("go/pkg/mod").to_string_lossy().as_ref())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid published Go module cache generation",
        ));
    }
    let parent = state_root
        .join("sleeves")
        .join(name)
        .join("overlays/go-mod");
    fs::create_dir_all(&parent)?;
    let upper = parent.join("upper");
    let work = parent.join("work");
    fs::create_dir_all(&upper)?;
    fs::create_dir_all(&work)?;
    let upper_dir = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&upper)?;
    let mut count = 0;
    seed_go_writable_dirs(lower, &upper_dir, &mut count, 0)?;
    Ok(Some(CacheOverlay {
        lower: lower.into(),
        upper,
        work,
        target: user_home.join(".cache/go-mod/cache/download"),
    }))
}

fn seed_go_writable_dirs(
    lower: &Path,
    upper: &fs::File,
    count: &mut usize,
    depth: usize,
) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Go module cache directory depth exceeds limit",
        ));
    }
    for entry in fs::read_dir(lower)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "published Go module cache contains a symlink",
            ));
        }
        if !kind.is_dir() {
            continue;
        }
        *count += 1;
        if *count > 100_000 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Go module cache directory count exceeds limit",
            ));
        }
        let child = ensure_child(upper, &entry.file_name())?;
        seed_go_writable_dirs(&entry.path(), &child, count, depth + 1)?;
    }
    Ok(())
}

fn seed_pub_writable_dirs(lower: &Path, upper: &Path) -> io::Result<()> {
    let upper = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(upper)?;
    for group in ["hosted", "hosted-hashes", "git", "global_packages", "bin"] {
        let path = lower.join(group);
        if !path.is_dir() {
            continue;
        }
        let group_upper = ensure_child(&upper, OsStr::new(group))?;
        // Pub writes fresh package records under host names and updates
        // global snapshots. Only these shallow directories need copy-up.
        if matches!(
            group,
            "hosted" | "hosted-hashes" | "global_packages" | "git"
        ) {
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    ensure_child(&group_upper, &entry.file_name())?;
                }
            }
        }
    }
    Ok(())
}

fn published_flutter_overlay(
    state_root: &Path,
    name: &str,
    sdk: &Path,
    lower: &Path,
    target_relative: &str,
    label: &str,
    manifest_name: &str,
) -> io::Result<CacheOverlay> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !sdk.is_absolute()
        || !lower.is_absolute()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Flutter overlay path",
        ));
    }
    let cache = sdk.join(target_relative);
    if !cache.is_dir() || !lower.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Flutter SDK or published cache is missing",
        ));
    }
    let manifest = lower.join(manifest_name);
    let input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(manifest)?;
    let mut bytes = Vec::new();
    use io::Read;
    input.take(4096).read_to_end(&mut bytes)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    let expected = sdk.to_string_lossy();
    if manifest["sdk"].as_str() != Some(expected.as_ref())
        || manifest["sha256"].as_str().is_none_or(|digest| {
            lower.file_name().and_then(OsStr::to_str) != Some(format!("{label}-{digest}").as_str())
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flutter cache generation does not match the SDK",
        ));
    }
    let parent = state_root
        .join("sleeves")
        .join(name)
        .join("overlays")
        .join(label);
    fs::create_dir_all(&parent)?;
    let upper = parent.join("upper");
    let work = parent.join("work");
    fs::create_dir_all(&upper)?;
    fs::create_dir_all(&work)?;
    seed_flutter_metadata(lower, &upper)?;
    Ok(CacheOverlay {
        lower: lower.into(),
        upper,
        work,
        target: cache,
    })
}

// Flutter's shell bootstrap redirects into existing top-level stamp files
// (notably engine.realm). A read-only lower would copy up their 0400 mode.
// Seed only bounded metadata into the private upper with owner write access.
fn seed_flutter_metadata(lower: &Path, upper: &Path) -> io::Result<()> {
    const MAX_FILES: usize = 64;
    const MAX_FILE_BYTES: u64 = 64 * 1024;
    const MAX_TOTAL_BYTES: u64 = 1024 * 1024;
    let source = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(lower)?;
    let mut count = 0usize;
    let mut total = 0u64;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", source.as_raw_fd()))? {
        let name = entry?.file_name();
        let c_name = CString::new(name.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "NUL in Flutter cache filename")
        })?;
        let fd = unsafe {
            libc::openat(
                source.as_raw_fd(),
                c_name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut input = unsafe { fs::File::from_raw_fd(fd) };
        let metadata = input.metadata()?;
        if !metadata.is_file()
            || metadata.len() > MAX_FILE_BYTES
            || name == OsStr::new(".protectorate-flutter-cache.json")
        {
            continue;
        }
        count += 1;
        total += metadata.len();
        if count > MAX_FILES || total > MAX_TOTAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Flutter metadata seed exceeds limit",
            ));
        }
        let target = upper.join(&name);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&target)?.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Flutter metadata upper contains a non-file",
                    ));
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let copied = io::copy(&mut input, &mut output)?;
        if copied != metadata.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Flutter metadata changed while seeding",
            ));
        }
    }
    Ok(())
}

/// Selects the user's installed default only when its toolchain directory is
/// explicitly granted read-only. Rustup's config stays private to the sleeve.
pub fn rustup_default_toolchain(user_home: &Path, tool_roots: &[PathBuf]) -> Option<String> {
    let installed = user_home.join(".rustup/toolchains");
    if !tool_roots.iter().any(|root| root == &installed) {
        return None;
    }
    let settings = user_home.join(".rustup/settings.toml");
    if fs::symlink_metadata(&settings)
        .ok()?
        .file_type()
        .is_symlink()
        || fs::metadata(&settings).ok()?.len() > 4096
    {
        return None;
    }
    let text = fs::read_to_string(settings).ok()?;
    let value = text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "default_toolchain")
            .then_some(value.trim().strip_prefix('"')?.strip_suffix('"')?)
    })?;
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !installed.join(value).is_dir()
    {
        return None;
    }
    Some(value.to_owned())
}

/// Prepares the private login profile and small, empty package caches. No
/// host cache is scanned or copied. Descents are descriptor-anchored and
/// reject symlinks left by a sleeve.
pub fn prepare_private_caches(
    home: &Path,
    user_home: &Path,
) -> io::Result<BTreeMap<String, String>> {
    if fs::canonicalize(home)? != home {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private home has a symlink ancestor",
        ));
    }
    let root = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(home)?;
    ensure_login_profile(&root)?;
    let cache = ensure_child(&root, OsStr::new(".cache"))?;
    for name in ["go-build", "go-mod", "pub", "flutter"] {
        ensure_child(&cache, OsStr::new(name))?;
    }
    ensure_child(&root, OsStr::new(".cargo"))?;
    ensure_child(&root, OsStr::new(".rustup"))?;
    let inside = |relative: &str| user_home.join(relative).to_string_lossy().into_owned();
    Ok(BTreeMap::from([
        ("XDG_CACHE_HOME".into(), inside(".cache")),
        ("GOCACHE".into(), inside(".cache/go-build")),
        ("GOMODCACHE".into(), inside(".cache/go-mod")),
        ("PUB_CACHE".into(), inside(".cache/pub")),
        ("CARGO_HOME".into(), inside(".cargo")),
        ("RUSTUP_HOME".into(), inside(".rustup")),
    ]))
}

const LOGIN_PROFILE: &str = "# Protectorate sleeve tool path for login shells.\nif [ -n \"$PROTECTORATE_SLEEVE_PATH\" ]; then PATH=\"$PROTECTORATE_SLEEVE_PATH\"; export PATH; fi\n";

fn ensure_login_profile(home: &fs::File) -> io::Result<()> {
    let name = c".profile";
    let fd = unsafe {
        libc::openat(
            home.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd >= 0 {
        let mut profile = unsafe { fs::File::from_raw_fd(fd) };
        return profile.write_all(LOGIN_PROFILE.as_bytes());
    }
    let error = io::Error::last_os_error();
    if error.kind() != io::ErrorKind::AlreadyExists {
        return Err(error);
    }
    // A preserved sleeve home may already have a profile. Do not follow a
    // replacement symlink or overwrite edits made inside that home.
    let fd = unsafe {
        libc::openat(
            home.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let profile = unsafe { fs::File::from_raw_fd(fd) };
    if !profile.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "sleeve login profile is not a regular file",
        ));
    }
    Ok(())
}

fn ensure_child(parent: &fs::File, name: &OsStr) -> io::Result<fs::File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in cache name"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd >= 0 {
        return Ok(unsafe { fs::File::from_raw_fd(fd) });
    }
    let error = io::Error::last_os_error();
    if error.kind() != io::ErrorKind::NotFound {
        return Err(error);
    }
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 40
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

pub fn write_config(path: &Path, config: &Config) -> io::Result<()> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec(config).map_err(io::Error::other)?)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn default_tools_include_narrow_installs_and_skip_symlinked_roots() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("owner");
        fs::create_dir_all(home.join(".local/go/bin")).unwrap();
        fs::create_dir_all(home.join("go/bin")).unwrap();
        fs::create_dir_all(home.join("go/pkg/mod/cache/vcs/private/.git")).unwrap();
        fs::write(
            home.join("go/pkg/mod/cache/vcs/private/.git/config"),
            b"url = https://token@example.invalid/private.git",
        )
        .unwrap();
        fs::create_dir_all(home.join(".cargo/bin")).unwrap();
        fs::create_dir_all(home.join(".rustup/toolchains")).unwrap();
        fs::create_dir_all(home.join(".pub-cache/bin")).unwrap();
        fs::create_dir_all(home.join(".pub-cache/global_packages")).unwrap();
        fs::write(home.join(".pub-cache/credentials.json"), b"secret").unwrap();
        fs::create_dir_all(home.join(".nvm/versions/node/v22/bin")).unwrap();
        fs::create_dir_all(home.join(".nvm/versions/node/v24/bin")).unwrap();
        fs::write(home.join(".nvm/versions/node/v22/bin/node"), b"node").unwrap();
        fs::write(home.join(".nvm/versions/node/v24/bin/node"), b"node").unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/auth.json"), b"secret").unwrap();
        fs::create_dir_all(home.join(".local")).unwrap();
        symlink(home.join(".codex"), home.join(".local/opt")).unwrap();

        let roots = default_tool_roots(&home);
        assert!(roots.contains(&home.join(".local/go")));
        assert!(roots.contains(&home.join("go/bin")));
        assert!(!roots.contains(&home.join("go")));
        assert!(roots.contains(&home.join(".cargo/bin")));
        assert!(roots.contains(&home.join(".rustup/toolchains")));
        assert!(roots.contains(&home.join(".pub-cache/bin")));
        assert!(roots.contains(&home.join(".pub-cache/global_packages")));
        assert!(!roots.contains(&home.join(".pub-cache")));
        assert!(roots.contains(&home.join(".nvm/versions/node/v24")));
        assert!(!roots.contains(&home.join(".nvm/versions/node/v22")));
        assert!(!roots.contains(&home.join(".local/opt")));
        assert!(roots
            .iter()
            .all(|root| !root.starts_with(home.join(".codex"))));
    }

    #[test]
    fn private_caches_are_empty_and_reject_a_sleeve_symlink() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("state/sleeves/task/home");
        let owner = root.path().join("owner");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir(&owner).unwrap();
        let env = prepare_private_caches(&home, &owner).unwrap();
        assert_eq!(env["PUB_CACHE"], owner.join(".cache/pub").to_string_lossy());
        assert!(home.join(".cache/go-build").is_dir());
        assert!(home.join(".cache/go-mod").is_dir());
        assert!(home.join(".cache/pub").is_dir());
        assert!(home.join(".cargo").is_dir());
        assert_eq!(fs::read_dir(home.join(".cache/pub")).unwrap().count(), 0);
        assert_eq!(
            fs::read_to_string(home.join(".profile")).unwrap(),
            LOGIN_PROFILE
        );
        let login = std::process::Command::new("bash")
            .args(["-lc", "printf '%s' \"$PATH\""])
            .env("HOME", &home)
            .env("PROTECTORATE_SLEEVE_PATH", "/sleeve/tool/bin:/usr/bin")
            .output()
            .unwrap();
        assert!(login.status.success());
        assert_eq!(
            String::from_utf8(login.stdout).unwrap(),
            "/sleeve/tool/bin:/usr/bin"
        );

        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::remove_dir(home.join(".cache/pub")).unwrap();
        symlink(&outside, home.join(".cache/pub")).unwrap();
        assert!(prepare_private_caches(&home, &owner).is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }

    #[test]
    fn go_module_overlay_requires_matching_publication_and_seeds_private_dirs() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let owner = root.path().join("owner");
        let lower = state.join("cache/go-mod-testhash");
        fs::create_dir_all(lower.join("example.org/module/@v")).unwrap();
        fs::write(
            lower.join("example.org/module/@v/v1.0.0.mod"),
            b"module example.org/module\n",
        )
        .unwrap();
        fs::write(
            lower.join(".protectorate-go-mod-cache.json"),
            serde_json::json!({
                "source": owner.join("go/pkg/mod"), "sha256": "testhash"
            })
            .to_string(),
        )
        .unwrap();
        let overlay = go_mod_cache_overlay(&state, "task", &owner, Some(&lower))
            .unwrap()
            .unwrap();
        assert_eq!(overlay.target, owner.join(".cache/go-mod/cache/download"));
        let private = overlay.upper.join("example.org/module/@v/new.mod");
        fs::write(&private, b"private").unwrap();
        assert!(!lower.join("example.org/module/@v/new.mod").exists());
        let other = root.path().join("other");
        assert!(go_mod_cache_overlay(&state, "other", &other, Some(&lower)).is_err());
    }

    #[test]
    fn published_pub_uses_private_owner_path_and_writable_package_directories() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let owner = root.path().join("owner");
        let lower = state.join("cache/pub-test");
        fs::create_dir_all(lower.join("hosted/pub.dev")).unwrap();
        fs::create_dir_all(lower.join("global_packages/protoc_plugin")).unwrap();
        fs::create_dir(&owner).unwrap();
        fs::write(
            lower.join(".protectorate-pub-cache.json"),
            serde_json::json!({"source":owner.join(".pub-cache"),"sha256":"test"}).to_string(),
        )
        .unwrap();
        let overlay = pub_cache_overlay(&state, "task", &owner, Some(&lower))
            .unwrap()
            .unwrap();
        assert_eq!(overlay.target, owner.join(".pub-cache"));
        assert!(overlay.upper.join("hosted/pub.dev").is_dir());
        assert!(overlay.upper.join("global_packages/protoc_plugin").is_dir());
        fs::write(overlay.upper.join("hosted/pub.dev/private"), b"private").unwrap();
        assert!(!lower.join("hosted/pub.dev/private").exists());
        fs::write(
            lower.join(".protectorate-pub-cache.json"),
            serde_json::json!({"source":root.path().join("other/.pub-cache"),"sha256":"test"})
                .to_string(),
        )
        .unwrap();
        assert!(pub_cache_overlay(&state, "task", &owner, Some(&lower)).is_err());
        fs::write(
            lower.join(".protectorate-pub-cache.json"),
            serde_json::json!({"source":owner.join(".pub-cache"),"sha256":"test"}).to_string(),
        )
        .unwrap();
        assert!(pub_cache_overlay(&state, "task", &owner, Some(&lower)).is_ok());
        let linked = state.join("cache/pub-linked");
        symlink(&lower, &linked).unwrap();
        assert!(pub_cache_overlay(&state, "task", &owner, Some(&linked)).is_err());
    }

    #[test]
    fn host_rustup_default_requires_an_explicit_toolchain_root() {
        let root = tempfile::tempdir().unwrap();
        let owner = root.path().join("owner");
        let toolchains = owner.join(".rustup/toolchains");
        fs::create_dir_all(toolchains.join("stable-aarch64-unknown-linux-gnu")).unwrap();
        fs::write(
            owner.join(".rustup/settings.toml"),
            "default_toolchain = \"stable-aarch64-unknown-linux-gnu\"\n",
        )
        .unwrap();
        assert_eq!(rustup_default_toolchain(&owner, &[]), None);
        assert_eq!(
            rustup_default_toolchain(&owner, &[toolchains]),
            Some("stable-aarch64-unknown-linux-gnu".into())
        );
    }

    #[test]
    fn flutter_overlay_seeds_only_small_top_level_metadata() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let sdk = root.path().join("flutter");
        let lower = state.join("cache/flutter-test");
        fs::create_dir_all(sdk.join("bin/cache")).unwrap();
        fs::create_dir_all(&lower).unwrap();
        fs::write(lower.join("engine.realm"), b"realm\n").unwrap();
        fs::set_permissions(
            lower.join("engine.realm"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        fs::write(lower.join("large.snapshot"), vec![0u8; 65537]).unwrap();
        fs::write(
            lower.join(".protectorate-flutter-cache.json"),
            serde_json::json!({"sdk":sdk,"sha256":"test"}).to_string(),
        )
        .unwrap();
        let overlay = flutter_overlay(&state, "task", Some(&sdk), Some(&lower))
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read(overlay.upper.join("engine.realm")).unwrap(),
            b"realm\n"
        );
        assert_eq!(
            fs::metadata(overlay.upper.join("engine.realm"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!overlay.upper.join("large.snapshot").exists());
        let tools = state.join("cache/flutter-tools-test");
        fs::create_dir_all(sdk.join("packages/flutter_tools/.dart_tool")).unwrap();
        fs::create_dir_all(&tools).unwrap();
        fs::write(tools.join("package_config.json"), b"{}").unwrap();
        fs::write(
            tools.join(".protectorate-flutter-tools.json"),
            serde_json::json!({"sdk":sdk,"sha256":"test"}).to_string(),
        )
        .unwrap();
        let overlay = flutter_tools_overlay(&state, "task", Some(&sdk), Some(&tools))
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read(overlay.upper.join("package_config.json")).unwrap(),
            b"{}"
        );
        assert_eq!(
            overlay.target,
            sdk.join("packages/flutter_tools/.dart_tool")
        );
    }
}
