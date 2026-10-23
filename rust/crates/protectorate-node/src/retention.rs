//! Disk accounting for node-owned job artifacts and Git caches.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_SCAN_ENTRIES: usize = 500_000;
pub(crate) type ExternalCleanup<'a> = Option<&'a dyn Fn(&Path) -> io::Result<()>>;

#[derive(Clone, Copy, Debug)]
pub struct DiskLimits {
    /// Persistent Git objects and warm workspaces, across all repositories.
    pub git_cache_bytes: u64,
    /// Retained job artifacts, across all finished jobs.
    pub artifact_bytes: u64,
}
impl Default for DiskLimits {
    fn default() -> Self {
        Self {
            git_cache_bytes: 32 << 30,
            artifact_bytes: 8 << 30,
        }
    }
}

pub(crate) fn tree_bytes(path: &Path) -> io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut stack = vec![path.to_path_buf()];
    let mut entries = 0usize;
    let mut total = 0u64;
    while let Some(path) = stack.pop() {
        entries += 1;
        if entries > MAX_SCAN_ENTRIES {
            return Err(io::Error::other("job disk scan has too many entries"));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
            continue;
        }
        if metadata.is_file() {
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("job disk size overflow"))?;
        } else if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                stack.push(entry?.path());
            }
        }
    }
    Ok(total)
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_windows_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

pub(crate) fn artifact_bytes(root: &Path) -> io::Result<u64> {
    let runs = root.join("runs");
    if !runs.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    for entry in fs::read_dir(runs)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        total = total
            .checked_add(tree_bytes(&entry.path().join("artifacts"))?)
            .ok_or_else(|| io::Error::other("artifact size overflow"))?;
    }
    Ok(total)
}

pub(crate) fn repo_workspace_name(key: &str, mode: &str) -> String {
    let digest = Sha256::digest(format!("{key}\0{mode}").as_bytes());
    let name = format!("r_{:x}", digest);
    name[..64].to_string()
}

pub(crate) fn prune_git_caches(
    root: &Path,
    repo_root: &Path,
    external_workspaces: Option<&Path>,
    external_cleanup: ExternalCleanup<'_>,
    active: &BTreeSet<String>,
    limit: u64,
) -> io::Result<u64> {
    let repos = repo_root;
    fs::create_dir_all(repos)?;
    let local_workspaces = root.join("workspaces");
    let mut caches = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(repos)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let key = entry.file_name().to_string_lossy().into_owned();
        if !crate::sources::valid_id(&key) {
            continue;
        }
        let repo = entry.path();
        let names = [
            repo_workspace_name(&key, "plain"),
            repo_workspace_name(&key, "admin"),
            repo_workspace_name(&key, "sandboxed"),
        ];
        let mut paths = Vec::with_capacity(names.len() * 2);
        let mut bytes = tree_bytes(&repo)?;
        for name in names {
            let local = local_workspaces.join(&name);
            bytes = bytes
                .checked_add(tree_bytes(&local)?)
                .ok_or_else(|| io::Error::other("cache size overflow"))?;
            paths.push((local.clone(), false));
            if let Some(external_root) = external_workspaces {
                let external = external_root.join(name);
                if external != local {
                    bytes = bytes
                        .checked_add(tree_bytes(&external)?)
                        .ok_or_else(|| io::Error::other("cache size overflow"))?;
                    paths.push((external, true));
                }
            }
        }
        total = total
            .checked_add(bytes)
            .ok_or_else(|| io::Error::other("cache size overflow"))?;
        let modified = fs::metadata(repo.join(".pc-last-used"))
            .or_else(|_| fs::metadata(&repo))?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        caches.push((modified, key, repo, paths, bytes));
    }
    caches.sort_by_key(|item| item.0);
    for (_, key, repo, paths, bytes) in caches {
        if total <= limit {
            break;
        }
        if active.contains(&key) {
            continue;
        }
        for (path, external) in paths {
            if external {
                if let Some(cleanup) = external_cleanup {
                    cleanup(&path)?;
                } else {
                    remove_dir_if_present(&path)?;
                }
            } else {
                remove_dir_if_present(&path)?;
            }
        }
        remove_dir_if_present(&repo)?;
        total -= bytes;
    }
    Ok(total)
}
fn remove_dir_if_present(path: &PathBuf) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && !is_windows_reparse(&metadata) =>
        {
            fs::remove_dir_all(path)
        }
        Ok(_) => Err(io::Error::other(format!(
            "cache path is not an owned directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
