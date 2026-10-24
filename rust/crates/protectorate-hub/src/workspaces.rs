//! Bounded workspace discovery for SleeveService. Repository metadata is read
//! as data; no command from a workspace is ever run by these RPCs.
use super::Hub;
use protectorate_sleeve_registry::State;
use protectorate_wire::{v1, v1::sleeve_service, Call, Code, Result, RpcError, UnaryServer};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs,
    io::{self, Read},
    os::fd::{AsRawFd, FromRawFd},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

const MAX_RESULTS: usize = 500;
const MAX_SCANNED: usize = 10_000;
const MAX_GIT_FILE: u64 = 1 << 20;

pub(super) struct Config {
    state: PathBuf,
    roots: Vec<PathBuf>,
    references: Vec<PathBuf>,
    user_home: Option<PathBuf>,
}

impl Config {
    pub(super) fn new(home: &Path) -> Self {
        let state = fs::canonicalize(home).unwrap_or_else(|_| {
            if home.is_absolute() {
                home.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(home)
            }
        });
        let user_home = std::env::var_os("HOME").and_then(|p| fs::canonicalize(p).ok());
        Self {
            state,
            roots: Vec::new(),
            references: Vec::new(),
            user_home,
        }
    }

    pub(super) fn state_root(&self) -> &Path {
        &self.state
    }

    pub(super) fn set_roots(&mut self, roots: Vec<PathBuf>) -> io::Result<()> {
        self.roots = canonical_directories(roots)?;
        Ok(())
    }

    pub(super) fn set_references(&mut self, refs: Vec<PathBuf>) -> io::Result<()> {
        self.references = canonical_directories(refs)?;
        Ok(())
    }

    fn managed(&self) -> PathBuf {
        self.state.join("workspaces")
    }

    fn required_name(&self, path: &Path) -> String {
        path.strip_prefix(self.managed())
            .ok()
            .and_then(|p| p.components().next())
            .and_then(|p| match p {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn reason(&self, path: &Path) -> Option<&'static str> {
        if path == self.state
            || self.state.starts_with(path)
            || path.starts_with(&self.state) && !path.starts_with(self.managed())
        {
            return Some("inside Protectorate state");
        }
        if path.starts_with(self.managed()) {
            return if self.required_name(path).is_empty() {
                Some("workspace name is required")
            } else {
                None
            };
        }
        if path == Path::new("/")
            || [
                "/proc", "/sys", "/dev", "/run", "/etc", "/usr", "/var", "/opt", "/bin", "/sbin",
                "/lib", "/lib64", "/boot", "/root",
            ]
            .iter()
            .any(|root| path == Path::new(root) || path.starts_with(root))
        {
            return Some("system directory");
        }
        if let Some(home) = &self.user_home {
            if path == home || home.starts_with(path) {
                return Some("contains the user home directory");
            }
            let protected = [
                ".claude",
                ".codex",
                ".grok",
                ".pi",
                ".ssh",
                ".gnupg",
                ".aws",
                ".cargo",
                ".local/share",
                ".config",
                ".docker",
                ".kube",
                ".netrc",
                ".git-credentials",
                ".npmrc",
                ".pypirc",
                ".password-store",
                ".pub-cache",
            ];
            if protected.iter().any(|p| {
                let secret = home.join(p);
                let lexical = path.starts_with(&secret) || secret.starts_with(path);
                let resolved = fs::canonicalize(&secret)
                    .ok()
                    .is_some_and(|real| path.starts_with(&real) || real.starts_with(path));
                lexical || resolved
            }) {
                return Some("contains credentials or application data");
            }
        }
        None
    }

    fn in_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

fn canonical_directories(paths: Vec<PathBuf>) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in paths {
        let real = fs::canonicalize(path)?;
        if !real.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace root is not a directory",
            ));
        }
        out.push(real);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let list = Arc::clone(&hub);
    server.unary(sleeve_service::LIST_WORKSPACES, move |_: Call<_>| {
        let hub = Arc::clone(&list);
        async move {
            let workspaces = tokio::task::spawn_blocking(move || list_workspaces(&hub))
                .await
                .map_err(super::join_error)?;
            Ok(v1::ListWorkspacesResponse { workspaces })
        }
    })?;
    server.unary(
        sleeve_service::BROWSE_WORKSPACES,
        move |call: Call<v1::BrowseWorkspacesRequest>| {
            let hub = Arc::clone(&hub);
            async move {
                if call.auth.actor.starts_with("token:sleeve:") {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "sleeve identity tokens cannot browse workspace directories",
                    ));
                }
                let path = call.request.path;
                tokio::task::spawn_blocking(move || browse(&hub.workspaces, &owners(&hub), &path))
                    .await
                    .map_err(super::join_error)?
            }
        },
    )?;
    Ok(())
}

fn list_workspaces(hub: &Hub) -> Vec<v1::Workspace> {
    let cfg = &hub.workspaces;
    let mut by_path = BTreeMap::<PathBuf, v1::Workspace>::new();
    if fs::canonicalize(cfg.managed()).ok().as_deref() == Some(cfg.managed().as_path()) {
        if let Ok(entries) = fs::read_dir(cfg.managed()) {
            for entry in entries.flatten() {
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    add_workspace(cfg, &mut by_path, &entry.path());
                }
            }
        }
    }
    for sleeve in hub.sleeves.list() {
        if sleeve.name == "envoy" || sleeve.workspace.is_empty() {
            continue;
        }
        if let Some(row) = add_workspace(cfg, &mut by_path, Path::new(&sleeve.workspace)) {
            if !sleeve.killed {
                row.busy = sleeve.state == State::Running;
                row.required_name = sleeve.name.clone();
                row.sleeve = sleeve.name;
            }
        }
    }
    let mut references = cfg.references.clone();
    references.extend(by_path.keys().map(|p| p.join("references")));
    for path in references {
        if let Ok(real) = fs::canonicalize(path) {
            if real.is_dir() && cfg.reason(&real).is_none() {
                by_path
                    .entry(real.clone())
                    .or_insert_with(|| v1::Workspace {
                        name: file_name(&real),
                        path: real.to_string_lossy().into_owned(),
                        read_only: true,
                        ..Default::default()
                    });
            }
        }
    }
    let mut result: Vec<_> = by_path.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    result
}

fn add_workspace<'a>(
    cfg: &Config,
    by_path: &'a mut BTreeMap<PathBuf, v1::Workspace>,
    path: &Path,
) -> Option<&'a mut v1::Workspace> {
    let real = fs::canonicalize(path).ok()?;
    if !real.is_dir() || cfg.reason(&real).is_some() {
        return None;
    }
    Some(by_path.entry(real.clone()).or_insert_with(|| {
        let (git_remote, git_branch, _) = git_details(&real);
        v1::Workspace {
            name: file_name(&real),
            path: real.to_string_lossy().into_owned(),
            git_remote,
            git_branch,
            required_name: cfg.required_name(&real),
            ..Default::default()
        }
    }))
}

/// Each live sleeve's resolved workspace and name. One sleeve per directory:
/// a killed sleeve gives its directory up, an exited one keeps it.
pub(super) fn owners(hub: &Hub) -> Vec<(PathBuf, String)> {
    hub.sleeves
        .list()
        .into_iter()
        .filter(|row| !row.killed && !row.workspace.is_empty())
        .map(|row| {
            let path =
                fs::canonicalize(&row.workspace).unwrap_or_else(|_| PathBuf::from(&row.workspace));
            (path, row.name)
        })
        .collect()
}

/// The sleeve whose workspace is `path` or overlaps it, if any.
pub(super) fn owner_of<'a>(
    owners: &'a [(PathBuf, String)],
    path: &Path,
) -> Option<&'a (PathBuf, String)> {
    owners.iter().find(|(owned, _)| owned == path).or_else(|| {
        owners
            .iter()
            .find(|(owned, _)| owned.starts_with(path) || path.starts_with(owned))
    })
}

fn browse(
    cfg: &Config,
    owners: &[(PathBuf, String)],
    path: &str,
) -> Result<v1::BrowseWorkspacesResponse> {
    if path.is_empty() {
        let mut directories = Vec::new();
        for root in &cfg.roots {
            // A replaced root symlink must not broaden the browse authority.
            if fs::canonicalize(root).ok().as_deref() != Some(root) || !root.is_dir() {
                continue;
            }
            if directories.len() >= MAX_RESULTS {
                break;
            }
            directories.push(directory(cfg, owners, root, file_name(root)));
        }
        return Ok(v1::BrowseWorkspacesResponse {
            directories,
            truncated: cfg.roots.len() > MAX_RESULTS,
            ..Default::default()
        });
    }
    let requested = Path::new(path);
    if !requested.is_absolute() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "browse path must be absolute",
        ));
    }
    let real = fs::canonicalize(requested)
        .map_err(|e| RpcError::new(Code::InvalidArgument, format!("resolve browse path: {e}")))?;
    if !cfg.in_root(&real) {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "browse path is outside workspace roots",
        ));
    }
    if !real.is_dir() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "browse path is not a directory",
        ));
    }
    let parent = real
        .parent()
        .filter(|p| cfg.in_root(p) && !cfg.roots.iter().any(|r| r == &real));
    let mut directories = Vec::new();
    let mut truncated = false;
    let entries = fs::read_dir(&real)
        .map_err(|e| RpcError::new(Code::InvalidArgument, format!("read browse directory: {e}")))?;
    for (scanned, entry) in entries.enumerate() {
        if scanned == MAX_SCANNED {
            truncated = true;
            break;
        }
        let entry = entry
            .map_err(|e| RpcError::new(Code::Internal, format!("read browse directory: {e}")))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(child) = fs::canonicalize(entry.path()) else {
            continue;
        };
        if !child.is_dir() || !cfg.in_root(&child) {
            continue;
        }
        if directories.len() == MAX_RESULTS {
            truncated = true;
            break;
        }
        directories.push(directory(cfg, owners, &child, name));
    }
    directories.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v1::BrowseWorkspacesResponse {
        path: real.to_string_lossy().into_owned(),
        parent: parent
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        directories,
        truncated,
    })
}

fn directory(
    cfg: &Config,
    owners: &[(PathBuf, String)],
    path: &Path,
    name: String,
) -> v1::WorkspaceDirectory {
    let (git_remote, git_branch, is_git) = git_details(path);
    let owner = owner_of(owners, path);
    let mut required_name = cfg.required_name(path);
    let reason = match (cfg.reason(path), owner) {
        (Some(reason), _) => reason.to_owned(),
        (None, Some((owned, sleeve))) if owned == path => {
            required_name = sleeve.clone();
            String::new()
        }
        (None, Some((owned, sleeve))) => {
            format!("overlaps sleeve {sleeve}'s workspace {}", owned.display())
        }
        (None, None) => String::new(),
    };
    v1::WorkspaceDirectory {
        name,
        path: path.to_string_lossy().into_owned(),
        is_git,
        git_branch,
        git_remote,
        acceptable: reason.is_empty(),
        reason,
        required_name,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub(super) fn git_details(path: &Path) -> (String, String, bool) {
    let git = path.join(".git");
    match fs::symlink_metadata(&git) {
        Ok(meta) if meta.is_file() => return (String::new(), String::new(), true),
        Ok(meta) if meta.is_dir() => {}
        _ => return (String::new(), String::new(), false),
    }
    let branch = read_git_file(&git, "HEAD")
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|s| s.trim().strip_prefix("ref: refs/heads/").map(str::to_owned))
        .unwrap_or_default();
    let remote = read_git_file(&git, "config")
        .map(|bytes| origin(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();
    (remote, branch, true)
}

fn read_git_file(git: &Path, name: &str) -> Option<Vec<u8>> {
    let dir_path = CString::new(git.as_os_str().as_encoded_bytes()).ok()?;
    let name = CString::new(name).ok()?;
    // The directory and file are opened relative to stable file descriptors.
    // This prevents an untrusted checkout from redirecting metadata reads
    // through a symlink while the hub is inspecting it.
    let dir_fd = unsafe {
        libc::open(
            dir_path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if dir_fd < 0 {
        return None;
    }
    let dir = unsafe { fs::File::from_raw_fd(dir_fd) };
    let file_fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if file_fd < 0 {
        return None;
    }
    let file = unsafe { fs::File::from_raw_fd(file_fd) };
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_GIT_FILE {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_GIT_FILE + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_GIT_FILE).then_some(bytes)
}

fn origin(config: &str) -> String {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .eq_ignore_ascii_case("[remote \"origin\"]");
        } else if in_origin {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim().eq_ignore_ascii_case("url") {
                    return redact_http_userinfo(value.trim());
                }
            }
        }
    }
    String::new()
}

fn redact_http_userinfo(remote: &str) -> String {
    let Some((scheme, rest)) = remote.split_once("://") else {
        return remote.into();
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return remote.into();
    }
    let authority_end = rest.find(&['/', '?', '#'][..]).unwrap_or(rest.len());
    let (authority, suffix) = rest.split_at(authority_end);
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    format!("{scheme}://{host}{suffix}")
}
