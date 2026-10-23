//! Bounded, node-local job source upload and staging.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_wire::{Code, RpcError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_UPLOAD: u64 = 4 << 30;
const MAX_TOTAL_UPLOAD: u64 = 8 << 30;
const MAX_CHUNK: usize = 4 << 20;
const MAX_UPLOADS: usize = 128;
const UPLOAD_IDLE: Duration = Duration::from_secs(3600);
const MAX_ARCHIVE_ENTRIES: usize = 200_000;

pub(crate) struct Sources {
    root: PathBuf,
    repo_root: PathBuf,
    uploads: Mutex<UploadState>,
}
#[derive(Default)]
struct UploadState {
    active: BTreeMap<String, Upload>,
    completed: BTreeMap<String, (u64, Instant)>,
    bytes: u64,
}
struct Upload {
    file: File,
    hash: Sha256,
    size: u64,
    touched: Instant,
}
#[derive(Clone)]
pub(crate) enum Source {
    None,
    File {
        name: String,
    },
    Archive {
        workdir: PathBuf,
    },
    Git {
        key: String,
        commit: String,
        bundle: bool,
    },
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
fn valid_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64)
        && commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !name.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | 0))
}
fn relative_path(path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\\') {
        return None;
    }
    let p = Path::new(path);
    if p.components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Some(p.to_path_buf())
    } else {
        None
    }
}
impl Source {
    pub(crate) fn parse(v: &Value) -> Result<Self, RpcError> {
        let key = v.get("repoKey").and_then(Value::as_str).unwrap_or_default();
        let commit = v.get("commit").and_then(Value::as_str).unwrap_or_default();
        let name = v
            .get("artifactName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let archive = v
            .get("artifactArchive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let bundle = v
            .get("bundleUploaded")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let workdir = v.get("workdir").and_then(Value::as_str).unwrap_or_default();
        if !key.is_empty() {
            if !valid_id(key)
                || !valid_commit(commit)
                || !name.is_empty()
                || archive
                || !workdir.is_empty()
            {
                return Err(RpcError::invalid("invalid git job source"));
            }
            return Ok(Self::Git {
                key: key.into(),
                commit: commit.into(),
                bundle,
            });
        }
        if !commit.is_empty()
            || bundle
            || (!name.is_empty() && archive)
            || (!workdir.is_empty() && !archive)
        {
            return Err(RpcError::invalid("conflicting job source fields"));
        }
        if archive {
            let workdir = if workdir.is_empty() {
                PathBuf::new()
            } else {
                relative_path(workdir).ok_or_else(|| RpcError::invalid("invalid workdir"))?
            };
            return Ok(Self::Archive { workdir });
        }
        if !name.is_empty() {
            if !valid_name(name) {
                return Err(RpcError::invalid("invalid artifact name"));
            }
            return Ok(Self::File { name: name.into() });
        }
        Ok(Self::None)
    }
    pub(crate) fn requires_upload(&self) -> bool {
        matches!(
            self,
            Self::File { .. } | Self::Archive { .. } | Self::Git { bundle: true, .. }
        )
    }
    pub(crate) fn repo_key(&self) -> Option<&str> {
        if let Self::Git { key, .. } = self {
            Some(key)
        } else {
            None
        }
    }
}
impl Sources {
    pub(crate) fn new(root: PathBuf) -> io::Result<Self> {
        let repo_root = root.join("repos");
        fs::create_dir_all(&repo_root)?;
        fs::create_dir_all(root.join("workspaces"))?;
        #[cfg(windows)]
        clear_owned_staging(&repo_root.join(".bundles"))?;
        Ok(Self {
            repo_root,
            root,
            uploads: Mutex::new(UploadState::default()),
        })
    }
    fn job_dir(&self, id: &str) -> PathBuf {
        self.root.join("jobs").join(id)
    }
    pub(crate) fn upload_path(&self, id: &str) -> PathBuf {
        self.job_dir(id).join("upload")
    }
    #[cfg(windows)]
    pub(crate) fn bundle_path(&self, id: &str) -> PathBuf {
        self.repo_root.join(".bundles").join(format!("{id}.bundle"))
    }
    #[cfg(windows)]
    pub(crate) fn archive_script_path(&self, id: &str) -> PathBuf {
        self.repo_root.join(".bundles").join(format!("{id}.ps1"))
    }
    #[cfg(windows)]
    pub(crate) fn create_archive_script(&self, id: &str, script: &str) -> io::Result<PathBuf> {
        fs::create_dir_all(self.repo_root.join(".bundles"))?;
        let path = self.archive_script_path(id);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(script.as_bytes())?;
        file.sync_all()?;
        crate::windows_runner::grant_runner_read(&path)?;
        Ok(path)
    }
    pub(crate) fn ready(&self, id: &str, source: &Source) -> Result<(), RpcError> {
        if source.requires_upload()
            && (!self.upload_path(id).is_file()
                || !self
                    .uploads
                    .lock()
                    .map_err(|_| io_error("upload lock poisoned"))?
                    .completed
                    .contains_key(id))
        {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "job source has not been uploaded",
            ));
        }
        Ok(())
    }
    pub(crate) fn prepare(&self, v: &Value) -> Result<Value, RpcError> {
        let id = v.get("jobId").and_then(Value::as_str).unwrap_or_default();
        let key = v.get("repoKey").and_then(Value::as_str).unwrap_or_default();
        let commit = v.get("commit").and_then(Value::as_str).unwrap_or_default();
        if !valid_id(id) || !valid_id(key) || !valid_commit(commit) {
            return Err(RpcError::invalid("invalid job id, repo key, or commit"));
        }
        let repo = self.repo_root.join(key);
        if !repo.join("HEAD").is_file() {
            return Ok(json!({"hasCommit":false}));
        }
        let _ = fs::write(repo.join(".pc-last-used"), b"");
        let repo_arg = git_path(&repo).map_err(io_rpc)?;
        let has_commit = git(&[
            "--git-dir",
            &repo_arg,
            "cat-file",
            "-e",
            &format!("{commit}^{{commit}}"),
        ])
        .is_ok();
        let refs = git(&[
            "--git-dir",
            &repo_arg,
            "for-each-ref",
            "--sort=-committerdate",
            "--count=8",
            "--format=%(objectname)",
            "refs/pc/",
        ])
        .unwrap_or_default();
        let known = String::from_utf8_lossy(&refs)
            .split_whitespace()
            .filter(|s| valid_commit(s))
            .take(8)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        Ok(json!({"hasCommit":has_commit,"knownCommits":known}))
    }
    pub(crate) fn upload(&self, v: &Value) -> Result<Value, RpcError> {
        let id = v.get("jobId").and_then(Value::as_str).unwrap_or_default();
        if !valid_id(id) {
            return Err(RpcError::invalid("invalid job id"));
        }
        let offset = v
            .get("offset")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| v.get("offset").and_then(Value::as_u64))
            .unwrap_or(0);
        let data = STANDARD
            .decode(v.get("data").and_then(Value::as_str).unwrap_or_default())
            .map_err(|_| RpcError::invalid("invalid upload data"))?;
        if data.len() > MAX_CHUNK {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "upload chunk exceeds 4 MiB",
            ));
        }
        let last = v.get("last").and_then(Value::as_bool).unwrap_or(false);
        let digest = v.get("sha256").and_then(Value::as_str).unwrap_or_default();
        let mut state = self
            .uploads
            .lock()
            .map_err(|_| io_error("upload lock poisoned"))?;
        let expired = state
            .active
            .iter()
            .filter(|(_, upload)| upload.touched.elapsed() >= UPLOAD_IDLE)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for expired_id in expired {
            if let Some(upload) = state.active.remove(&expired_id) {
                state.bytes -= upload.size;
            }
            let _ = fs::remove_file(self.job_dir(&expired_id).join("upload.part"));
        }
        if offset == 0 {
            if let Some(previous) = state.active.remove(id) {
                state.bytes -= previous.size;
            }
            if let Some((size, _)) = state.completed.remove(id) {
                state.bytes -= size;
            }
            let dir = self.job_dir(id);
            fs::create_dir_all(&dir).map_err(io_rpc)?;
            let _ = fs::remove_file(self.upload_path(id));
            if state.active.len() >= MAX_UPLOADS {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "too many concurrent uploads",
                ));
            }
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(dir.join("upload.part"))
                .map_err(io_rpc)?;
            state.active.insert(
                id.into(),
                Upload {
                    file,
                    hash: Sha256::new(),
                    size: 0,
                    touched: Instant::now(),
                },
            );
        }
        let upload = state.active.get(id).ok_or_else(|| {
            RpcError::new(Code::FailedPrecondition, "upload has no initial chunk")
        })?;
        if offset != upload.size {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("offset {offset}, node has {} bytes", upload.size),
            ));
        }
        if upload.size.saturating_add(data.len() as u64) > MAX_UPLOAD {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "source upload exceeds 4 GiB",
            ));
        }
        if state.bytes.saturating_add(data.len() as u64) > MAX_TOTAL_UPLOAD {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "node upload storage exceeds 8 GiB",
            ));
        }
        let upload = state.active.get_mut(id).expect("checked above");
        if let Err(error) = upload.file.write_all(&data) {
            if let Some(failed) = state.active.remove(id) {
                state.bytes -= failed.size;
            }
            let _ = fs::remove_file(self.job_dir(id).join("upload.part"));
            return Err(io_rpc(error));
        }
        upload.hash.update(&data);
        upload.size += data.len() as u64;
        upload.touched = Instant::now();
        let received = upload.size;
        let expected = if last {
            Some(format!("{:x}", upload.hash.clone().finalize()))
        } else {
            None
        };
        if last {
            if let Err(error) = upload.file.sync_all() {
                if let Some(failed) = state.active.remove(id) {
                    state.bytes -= failed.size - data.len() as u64;
                }
                let _ = fs::remove_file(self.job_dir(id).join("upload.part"));
                return Err(io_rpc(error));
            }
        }
        state.bytes += data.len() as u64;
        if last {
            if digest.len() != 64 || !digest.eq_ignore_ascii_case(expected.as_deref().unwrap()) {
                state.active.remove(id);
                state.bytes -= received;
                let _ = fs::remove_file(self.job_dir(id).join("upload.part"));
                return Err(RpcError::new(Code::DataLoss, "upload SHA-256 mismatch"));
            }
            state.active.remove(id);
            if let Err(error) =
                fs::rename(self.job_dir(id).join("upload.part"), self.upload_path(id))
            {
                state.bytes -= received;
                let _ = fs::remove_file(self.job_dir(id).join("upload.part"));
                return Err(io_rpc(error));
            }
            state
                .completed
                .insert(id.to_string(), (received, Instant::now()));
        }
        Ok(json!({"received":received.to_string()}))
    }
    pub(crate) fn cleanup(&self, id: &str, source: &Source, base: &Path) {
        if let Ok(mut state) = self.uploads.lock() {
            if let Some(upload) = state.active.remove(id) {
                state.bytes -= upload.size;
            }
            if let Some((size, _)) = state.completed.remove(id) {
                state.bytes -= size;
            }
        }
        let _ = fs::remove_dir_all(self.job_dir(id));
        #[cfg(windows)]
        let _ = fs::remove_file(self.bundle_path(id));
        #[cfg(windows)]
        let _ = fs::remove_file(self.archive_script_path(id));
        #[cfg(windows)]
        let owned_base = base.starts_with(&self.root);
        #[cfg(not(windows))]
        let owned_base = true;
        if source.repo_key().is_none() && base != self.job_dir(id) && owned_base {
            let _ = fs::remove_dir_all(base);
        }
    }
    pub(crate) fn sweep_orphans(&self, active: &BTreeSet<String>) {
        if let Ok(mut state) = self.uploads.lock() {
            let expired = state
                .completed
                .iter()
                .filter(|(id, (_, touched))| {
                    !active.contains(*id) && touched.elapsed() >= UPLOAD_IDLE
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for id in expired {
                if let Some((size, _)) = state.completed.remove(&id) {
                    state.bytes -= size;
                }
                let _ = fs::remove_dir_all(self.job_dir(&id));
            }
        }
    }
    /// Materialize a source below a private workspace base. Returns the working directory and cache reuse bit.
    pub(crate) fn stage(
        &self,
        id: &str,
        source: &Source,
        base: &Path,
        sandbox_home: Option<&Path>,
    ) -> Result<(PathBuf, bool), RpcError> {
        #[cfg(windows)]
        if sandbox_home.is_none() {
            fs::create_dir_all(base).map_err(io_rpc)?;
        }
        #[cfg(not(windows))]
        fs::create_dir_all(base).map_err(io_rpc)?;
        match source {
            Source::None => Ok((base.to_path_buf(), false)),
            Source::File { name } => {
                #[cfg(windows)]
                if sandbox_home.is_some() {
                    return Ok((base.to_path_buf(), false));
                }
                let src = self.upload_path(id);
                let dest = base.join(name);
                if src != dest {
                    fs::copy(src, &dest).map_err(io_rpc)?;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
                        .map_err(io_rpc)?;
                }
                Ok((base.to_path_buf(), false))
            }
            Source::Archive { workdir } => {
                let work = base.join("work");
                #[cfg(windows)]
                if sandbox_home.is_some() {
                    return Ok((work.join(workdir), false));
                }
                extract_zip(&self.upload_path(id), &work)?;
                let selected = work.join(workdir);
                if !selected.is_dir()
                    || fs::canonicalize(&selected)
                        .map_err(io_rpc)?
                        .strip_prefix(fs::canonicalize(&work).map_err(io_rpc)?)
                        .is_err()
                {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "workdir is not in the uploaded archive",
                    ));
                }
                Ok((selected, false))
            }
            Source::Git {
                key,
                commit,
                bundle,
            } => {
                let repo = self.repo_root.join(key);
                let repo_arg = git_path(&repo).map_err(io_rpc)?;
                let reused = repo.join("HEAD").is_file();
                if !reused {
                    fs::create_dir_all(&repo).map_err(io_rpc)?;
                    git(&["init", "--bare", "--quiet", &repo_arg]).map_err(io_rpc)?;
                }
                let refname = format!("refs/pc/{commit}");
                if *bundle {
                    git(&[
                        "--git-dir",
                        &repo_arg,
                        "fetch",
                        "--quiet",
                        &git_path(&self.upload_path(id)).map_err(io_rpc)?,
                        &format!("+{refname}:{refname}"),
                    ])
                    .map_err(io_rpc)?;
                }
                git(&[
                    "--git-dir",
                    &repo_arg,
                    "cat-file",
                    "-e",
                    &format!("{commit}^{{commit}}"),
                ])
                .map_err(|_| {
                    RpcError::new(Code::FailedPrecondition, "commit is absent from node cache")
                })?;
                git(&["--git-dir", &repo_arg, "update-ref", &refname, commit]).map_err(io_rpc)?;
                let work = base.join("work");
                if let Some(home) = sandbox_home {
                    #[cfg(windows)]
                    let _ = home;
                    #[cfg(not(windows))]
                    fs::create_dir_all(&work).map_err(io_rpc)?;
                    #[cfg(windows)]
                    let bundle = {
                        let directory = self.repo_root.join(".bundles");
                        fs::create_dir_all(&directory).map_err(io_rpc)?;
                        self.bundle_path(id)
                    };
                    #[cfg(not(windows))]
                    let bundle = home.join("source.bundle");
                    git_bundle_to_new_file(&repo_arg, &refname, &bundle).map_err(io_rpc)?;
                    #[cfg(windows)]
                    crate::windows_runner::grant_runner_read(&bundle).map_err(io_rpc)?;
                    fs::write(repo.join(".pc-last-used"), b"").map_err(io_rpc)?;
                    return Ok((work.clone(), work.join(".git").is_dir()));
                }
                // Keep complete Git metadata inside the sandbox's workspace. A linked
                // worktree's .git pointer would refer to the protected node cache.
                let workspace_reused = work.join(".git").is_dir();
                let work_arg = git_path(&work).map_err(io_rpc)?;
                if !workspace_reused {
                    fs::create_dir_all(&work).map_err(io_rpc)?;
                    git(&["-C", &work_arg, "init", "--quiet"]).map_err(io_rpc)?;
                }
                git(&[
                    "-C",
                    &work_arg,
                    "fetch",
                    "--quiet",
                    &repo_arg,
                    &format!("+{refname}:{refname}"),
                ])
                .map_err(io_rpc)?;
                git(&[
                    "-C", &work_arg, "checkout", "--quiet", "--force", "--detach", commit,
                ])
                .map_err(io_rpc)?;
                fs::write(repo.join(".pc-last-used"), b"").map_err(io_rpc)?;
                Ok((work, workspace_reused))
            }
        }
    }
}
#[cfg(windows)]
fn clear_owned_staging(directory: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
fn git(args: &[&str]) -> io::Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
/// Git writes only to a pipe. Opening the destination with create_new prevents a
/// job-owned symlink or existing file from redirecting a node-authority write.
fn git_bundle_to_new_file(repo: &str, refname: &str, destination: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut child = Command::new("git")
        .args(["--git-dir", repo, "bundle", "create", "-", refname])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let copy = io::copy(child.stdout.as_mut().expect("piped Git stdout"), &mut file);
    if copy.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    copy?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "git bundle create failed with {status}"
        )));
    }
    file.sync_all()
}
fn git_path(path: &Path) -> io::Result<String> {
    #[cfg(windows)]
    let path = crate::windows_runner::process_path(path)?;
    Ok(path.to_string_lossy().into_owned())
}
fn extract_zip(src: &Path, dst: &Path) -> Result<(), RpcError> {
    let file = File::open(src).map_err(io_rpc)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| RpcError::invalid(format!("invalid artifact archive: {e}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(RpcError::new(
            Code::ResourceExhausted,
            "archive has too many entries",
        ));
    }
    fs::create_dir_all(dst).map_err(io_rpc)?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| RpcError::invalid(e.to_string()))?;
        let name = entry.name();
        let rel = relative_path(name.trim_end_matches('/'))
            .ok_or_else(|| RpcError::invalid("unsafe archive path"))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(RpcError::invalid("archive symlink is forbidden"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| RpcError::new(Code::ResourceExhausted, "archive is too large"))?;
        if total > MAX_UPLOAD {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "archive is too large",
            ));
        }
        let path = dst.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(path).map_err(io_rpc)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_rpc)?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io_rpc)?;
        let declared = entry.size();
        #[cfg(unix)]
        let mode = entry.unix_mode();
        let copied = io::copy(&mut (&mut entry).take(declared + 1), &mut out).map_err(io_rpc)?;
        if copied != declared {
            return Err(RpcError::new(Code::DataLoss, "archive entry size changed"));
        }
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o777 | 0o600))
                .map_err(io_rpc)?;
        }
    }
    Ok(())
}
fn io_rpc(error: io::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn io_error(message: &str) -> RpcError {
    RpcError::new(Code::Internal, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_paths_and_fields() {
        assert!(!valid_id("../bad"));
        assert!(Source::parse(&json!({"artifactArchive":true,"workdir":"../outside"})).is_err());
        assert!(Source::parse(&json!({"artifactName":"../../outside"})).is_err());
    }
    #[test]
    fn upload_checks_offsets_and_digest() {
        let temp = tempfile::tempdir().unwrap();
        let sources = Sources::new(temp.path().to_path_buf()).unwrap();
        assert!(sources
            .upload(&json!({"jobId":"one","offset":"1","data":"YQ=="}))
            .is_err());
        assert_eq!(
            sources
                .upload(&json!({"jobId":"one","offset":"0","data":"YQ=="}))
                .unwrap()["received"],
            "1"
        );
        assert!(sources
            .upload(&json!({"jobId":"one","offset":"0","data":"Yg==","last":true,"sha256":"no"}))
            .is_err());
        let digest = format!("{:x}", Sha256::digest(b"ab"));
        sources
            .upload(&json!({"jobId":"one","offset":"0","data":"YQ=="}))
            .unwrap();
        sources
            .upload(&json!({"jobId":"one","offset":"1","data":"Yg==","last":true,"sha256":digest}))
            .unwrap();
        assert_eq!(fs::read(sources.upload_path("one")).unwrap(), b"ab");
    }
    #[test]
    fn archive_cannot_escape_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("unsafe.zip");
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file("../escape", zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(b"secret").unwrap();
        archive.finish().unwrap();
        assert!(extract_zip(&path, &temp.path().join("work")).is_err());
        assert!(!temp.path().join("escape").exists());
    }
    #[test]
    fn bundle_destination_must_be_new() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("source.bundle");
        fs::write(&destination, b"host data").unwrap();
        let error = git_bundle_to_new_file("unused", "unused", &destination).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&destination).unwrap(), b"host data");
    }
    #[cfg(unix)]
    #[test]
    fn bundle_destination_does_not_follow_symlink() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let host_file = temp.path().join("host-secret");
        let destination = temp.path().join("source.bundle");
        fs::write(&host_file, b"host data").unwrap();
        symlink(&host_file, &destination).unwrap();
        let error = git_bundle_to_new_file("unused", "unused", &destination).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&host_file).unwrap(), b"host data");
    }
}
