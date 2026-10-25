//! Durable hub records, source shipping, and output relay for node jobs.
//! Node execution continues independently of a client WatchJob stream.
use super::Hub;
use chrono::Utc;
use protectorate_audit::Entry as AuditEntry;
use protectorate_auth as auth;
use protectorate_connect_client::Client;
use protectorate_fsutil::{hex, random_hex, unix_millis, unix_seconds};
use protectorate_inventory::{Inventory, CARE, SHARED};
use protectorate_lease::{Entry, LeaseTable, Resource};
use protectorate_node_monitor::{Dialer, Monitor};
use protectorate_wire::{
    self as wire,
    v1::{
        self, job_event, job_service, job_source, job_target::Target, node_agent_service,
        watch_job_response, JobEnd, JobMode, JobState,
    },
    AuthContext, Call, Code, ReflectMessage, Result, RpcError, StreamSender, UnaryServer,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime},
};
use tokio::sync::Notify;

const MAX_JOBS: usize = 4096;
const MAX_LOG_BYTES: u64 = 64 << 20;
const CHUNK: usize = 64 << 10;
const NODE_CALL_TIMEOUT: Duration = Duration::from_secs(86460);
const RECOVERY_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const RECOVERY_GRACE: Duration = Duration::from_secs(600);
const MAX_SOURCE_BYTES: u64 = 4 << 30;
const MAX_ARTIFACT_BYTES: u64 = 4 << 30;
const MAX_ARCHIVE_ENTRIES: usize = 200_000;
const UPLOAD_CHUNK: usize = 1 << 20;
const MAX_AUDIT_COMMAND_BYTES: usize = 512;
const REQUEST_ID_WINDOW: Duration = Duration::from_secs(3600);
const MAX_CANCELED_REQUEST_IDS: usize = 10_000;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_TIMEOUT: Duration = Duration::from_secs(86400);

fn submit_audit_detail(job: &v1::Job, task_grant: bool) -> String {
    let mode = match job.mode() {
        JobMode::Admin => "admin (elevated, unsandboxed)",
        JobMode::Plain => "plain (agent user, unsandboxed)",
        _ => "sandboxed",
    };
    let mut detail = format!("node {}, {mode}", job.node);
    if job.mode() == JobMode::Admin || task_grant {
        let command = job.command.join(" ");
        let quoted = format!("{command:?}");
        let mut end = quoted.len().min(MAX_AUDIT_COMMAND_BYTES);
        while !quoted.is_char_boundary(end) {
            end -= 1;
        }
        detail.push_str(", command ");
        detail.push_str(&quoted[..end]);
        if end < quoted.len() {
            detail.push_str("...");
        }
    }
    detail
}

pub struct Jobs {
    root: PathBuf,
    inventory: Arc<Inventory>,
    local_name: String,
    dialer: Dialer,
    state: Mutex<BTreeMap<String, Stored>>,
    writer: Mutex<()>,
    canceled_request_ids: Mutex<BTreeMap<String, i64>>,
    changed: Notify,
    admission: Option<Arc<RwLock<()>>>,
    draining: Option<Arc<Mutex<BTreeSet<String>>>>,
}

#[derive(Clone)]
struct Stored {
    job: v1::Job,
    request_id: String,
    cancel_requested: bool,
}

struct Placement {
    node: String,
    mode: JobMode,
    gpu: Option<u32>,
}

#[derive(Clone)]
enum SourcePlan {
    None,
    File {
        path: PathBuf,
        name: String,
    },
    Directory {
        path: PathBuf,
        workdir: String,
    },
    Git {
        repo: PathBuf,
        key: String,
        commit: String,
    },
}

impl SourcePlan {
    fn parse(source: Option<&v1::JobSource>) -> Result<Self> {
        let Some(source) = source else {
            return Ok(Self::None);
        };
        match &source.kind {
            Some(job_source::Kind::Git(git)) => Self::git(git),
            Some(job_source::Kind::Artifact(artifact)) => Self::artifact(artifact),
            None => Err(RpcError::invalid("unknown job source kind")),
        }
    }

    fn git(git: &v1::GitSource) -> Result<Self> {
        let raw = required(&git.repo, "repo")?;
        let repo = fs::canonicalize(raw)
            .map_err(|e| RpcError::invalid(format!("repository {raw:?}: {e}")))?;
        if !repo.is_dir() {
            return Err(RpcError::invalid("repository is not a directory"));
        }
        let rev = if git.commit.is_empty() {
            "HEAD"
        } else {
            git.commit.as_str()
        };
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{rev}^{{commit}}"),
            ])
            .output()
            .map_err(|e| RpcError::invalid(format!("resolve Git commit: {e}")))?;
        if !output.status.success() {
            return Err(RpcError::invalid(
                "Git revision does not resolve to a commit",
            ));
        }
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !matches!(commit.len(), 40 | 64)
            || !commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(RpcError::invalid("Git returned an invalid commit id"));
        }
        let base = repo
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo");
        let mut base = base
            .to_ascii_lowercase()
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                    ch
                } else {
                    '-'
                }
            })
            .collect::<String>();
        base = base
            .trim_matches(['.', '_', '-'])
            .chars()
            .take(40)
            .collect();
        if base.is_empty() {
            base = "repo".into();
        }
        let digest = Sha256::digest(repo.to_string_lossy().as_bytes());
        let key = format!("{base}-{}", hex(&digest[..4]));
        Ok(Self::Git { repo, key, commit })
    }

    fn artifact(artifact: &v1::ArtifactSource) -> Result<Self> {
        let raw = required(&artifact.path, "path")?;
        let path = fs::canonicalize(raw)
            .map_err(|e| RpcError::invalid(format!("source artifact {raw:?}: {e}")))?;
        let meta = fs::metadata(&path).map_err(RpcError::internal)?;
        let workdir = artifact.workdir.as_str();
        if meta.is_dir() {
            if !workdir.is_empty() && !safe_relative(workdir) {
                return Err(RpcError::invalid("invalid artifact workdir"));
            }
            return Ok(Self::Directory {
                path,
                workdir: workdir.into(),
            });
        }
        if !meta.is_file() || !workdir.is_empty() {
            return Err(RpcError::invalid(
                "artifact must be a file, or a directory for workdir",
            ));
        }
        let name = path
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or_default();
        if !safe_name(name) {
            return Err(RpcError::invalid("invalid artifact file name"));
        }
        let name = name.to_owned();
        Ok(Self::File { path, name })
    }

    fn record(&self) -> Option<v1::JobSource> {
        let kind = match self {
            Self::None => return None,
            Self::File { path, .. } => job_source::Kind::Artifact(v1::ArtifactSource {
                path: path.to_string_lossy().into_owned(),
                workdir: String::new(),
            }),
            Self::Directory { path, workdir } => job_source::Kind::Artifact(v1::ArtifactSource {
                path: path.to_string_lossy().into_owned(),
                workdir: workdir.clone(),
            }),
            Self::Git { repo, commit, .. } => job_source::Kind::Git(v1::GitSource {
                repo: repo.to_string_lossy().into_owned(),
                commit: commit.clone(),
            }),
        };
        Some(v1::JobSource { kind: Some(kind) })
    }
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !name.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | 0))
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn write_archive(root: &Path, out: &mut File) -> Result<()> {
    let mut zip = zip::ZipWriter::new(out);
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0_usize;
    let mut raw_bytes = 0_u64;
    while let Some(dir) = pending.pop() {
        let mut children = fs::read_dir(&dir)
            .map_err(RpcError::internal)?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(RpcError::internal)?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            let meta = fs::symlink_metadata(&path).map_err(RpcError::internal)?;
            if !meta.is_file() && !meta.is_dir() {
                continue;
            }
            entries += 1;
            if entries > MAX_ARCHIVE_ENTRIES {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "source directory has too many entries",
                ));
            }
            let rel = path.strip_prefix(root).map_err(RpcError::internal)?;
            let name = rel
                .to_str()
                .ok_or_else(|| RpcError::invalid("source directory has a non-UTF-8 path"))?
                .replace('\\', "/");
            if !safe_relative(&name) {
                return Err(RpcError::invalid("unsafe source directory path"));
            }
            #[cfg(unix)]
            let mode = meta.permissions().mode() & 0o777;
            #[cfg(not(unix))]
            let mode = if meta.is_dir() { 0o700 } else { 0o600 };
            let options = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(mode);
            if meta.is_dir() {
                zip.add_directory(format!("{name}/"), options)
                    .map_err(RpcError::internal)?;
                pending.push(path);
            } else {
                raw_bytes = raw_bytes.saturating_add(meta.len());
                if raw_bytes > MAX_SOURCE_BYTES {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source directory exceeds 4 GiB",
                    ));
                }
                zip.start_file(name, options).map_err(RpcError::internal)?;
                let mut input = File::open(&path).map_err(RpcError::internal)?;
                let copied = std::io::copy(
                    &mut std::io::Read::by_ref(&mut input).take(meta.len() + 1),
                    &mut zip,
                )
                .map_err(RpcError::internal)?;
                if copied != meta.len() {
                    return Err(RpcError::new(
                        Code::DataLoss,
                        "source file changed while archiving",
                    ));
                }
            }
        }
    }
    zip.finish().map_err(RpcError::internal)?;
    Ok(())
}

struct ArtifactSink {
    root: PathBuf,
    id: String,
    stage: Option<tempfile::TempDir>,
    current: Option<ArtifactFile>,
    files: Vec<v1::JobArtifact>,
    total: u64,
}

struct ArtifactFile {
    path: String,
    file: File,
    size: u64,
    mode: u32,
}

impl ArtifactSink {
    fn new(root: &Path, id: &str) -> Self {
        Self {
            root: root.into(),
            id: id.into(),
            stage: None,
            current: None,
            files: vec![],
            total: 0,
        }
    }

    fn write(&mut self, chunk: v1::JobArtifactChunk) -> Result<()> {
        let path = chunk.path.as_str();
        if !safe_relative(path) || path.len() > 4096 {
            return Err(RpcError::new(Code::DataLoss, "unsafe artifact path"));
        }
        if self.current.as_ref().is_some_and(|file| file.path != path) {
            return Err(RpcError::new(Code::DataLoss, "artifact frames interleaved"));
        }
        let data = &chunk.data;
        if data.len() > UPLOAD_CHUNK
            || self.total.saturating_add(data.len() as u64) > MAX_ARTIFACT_BYTES
        {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "artifacts exceed 4 GiB",
            ));
        }
        if self.current.is_none() {
            if self.files.len() >= 20_000 || self.files.iter().any(|file| file.path == path) {
                return Err(RpcError::new(
                    Code::DataLoss,
                    "too many or duplicate artifacts",
                ));
            }
            if self.stage.is_none() {
                self.stage = Some(
                    tempfile::Builder::new()
                        .prefix(&format!(".{}-artifacts-", self.id))
                        .tempdir_in(&self.root)
                        .map_err(RpcError::internal)?,
                );
            }
            let dest = self.stage.as_ref().unwrap().path().join(path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(RpcError::internal)?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options.open(dest).map_err(RpcError::internal)?;
            self.current = Some(ArtifactFile {
                path: path.into(),
                file,
                size: 0,
                mode: chunk.mode,
            });
        }
        let current = self.current.as_mut().unwrap();
        current.file.write_all(data).map_err(RpcError::internal)?;
        current.size += data.len() as u64;
        self.total += data.len() as u64;
        if chunk.last {
            let current = self.current.take().unwrap();
            current.file.sync_all().map_err(RpcError::internal)?;
            #[cfg(unix)]
            fs::set_permissions(
                self.stage.as_ref().unwrap().path().join(&current.path),
                fs::Permissions::from_mode((current.mode & 0o777) | 0o600),
            )
            .map_err(RpcError::internal)?;
            self.files.push(v1::JobArtifact {
                path: current.path,
                size_bytes: current.size,
                mode: current.mode,
            });
        }
        Ok(())
    }

    fn publish(&mut self) -> Result<(Vec<v1::JobArtifact>, u64)> {
        if self.current.is_some() {
            return Err(RpcError::new(Code::DataLoss, "unfinished artifact file"));
        }
        if let Some(stage) = self.stage.take() {
            let dest = self.root.join(format!("{}.artifacts", self.id));
            if dest.exists() {
                fs::remove_dir_all(&dest).map_err(RpcError::internal)?;
            }
            fs::rename(stage.path(), dest).map_err(RpcError::internal)?;
        }
        Ok((self.files.clone(), self.total))
    }
}

impl Jobs {
    pub fn open(
        root: impl Into<PathBuf>,
        inventory: Arc<Inventory>,
        local_name: String,
    ) -> std::io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let mut state = BTreeMap::new();
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let bytes = fs::read(&path)?;
            let value: Value = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if path.file_stem().and_then(|s| s.to_str()) != Some(id) || !valid_id(id) {
                return Err(std::io::Error::other("invalid persisted job id"));
            }
            let job = value
                .get("job")
                .ok_or_else(|| std::io::Error::other("missing persisted job"))?;
            let job = serde_json::to_vec(job).map_err(std::io::Error::other)?;
            let stored = Stored {
                job: wire::decode(&job).map_err(|error| {
                    std::io::Error::other(format!("persisted job {id}: {}", error.message))
                })?,
                request_id: value
                    .get("requestId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                cancel_requested: value
                    .get("cancelRequested")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            state.insert(id.to_string(), stored);
        }
        // A crash can land after a log append and before the offset commit.
        for (id, stored) in &state {
            let committed = stored.job.log_bytes;
            let file = OpenOptions::new()
                .write(true)
                .open(root.join(format!("{id}.log")))?;
            if file.metadata()?.len() < committed {
                return Err(std::io::Error::other(
                    "persisted job log is shorter than its recorded offset",
                ));
            }
            file.set_len(committed)?;
        }
        if state.len() > MAX_JOBS {
            return Err(std::io::Error::other("job record limit exceeded"));
        }
        let canceled_path = root.join("canceled-requests");
        let canceled_request_ids = if canceled_path.exists() {
            serde_json::from_slice(&fs::read(canceled_path)?).map_err(std::io::Error::other)?
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            root,
            inventory,
            local_name,
            dialer: Dialer::default(),
            state: Mutex::new(state),
            writer: Mutex::new(()),
            canceled_request_ids: Mutex::new(canceled_request_ids),
            changed: Notify::new(),
            admission: None,
            draining: None,
        })
    }

    pub(super) fn with_admission(
        mut self,
        admission: Arc<RwLock<()>>,
        draining: Arc<Mutex<BTreeSet<String>>>,
    ) -> Self {
        self.admission = Some(admission);
        self.draining = Some(draining);
        self
    }

    pub(super) fn active_on(&self, name: &str) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, stored)| stored.job.node == name && !terminal(&stored.job))
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn is_draining(&self, name: &str) -> bool {
        self.draining
            .as_ref()
            .is_some_and(|set| set.lock().unwrap().contains(name))
    }

    pub(super) fn uninstall_node(&self, name: &str) -> Result<v1::UninstallResponse> {
        self.client_with_timeout(name, Duration::from_secs(30))?
            .unary(node_agent_service::UNINSTALL, &v1::UninstallRequest {})
    }

    pub fn set_node_tls(&self, tls: protectorate_connect_client::NodeTls) {
        self.dialer.set(tls);
    }

    /// Launches every persisted job that has not finished.
    pub(super) fn resume(self: &Arc<Self>) {
        let resume = self
            .state
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| !terminal(&s.job))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in resume {
            let jobs = Arc::clone(self);
            std::thread::spawn(move || jobs.run(id, None));
        }
    }

    fn job_node(&self, id: &str) -> Result<String> {
        let state = self.state.lock().unwrap();
        let stored = state
            .get(id)
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))?;
        Ok(stored.job.node.clone())
    }

    fn client(&self, node: &str) -> Result<Client> {
        self.client_with_timeout(node, NODE_CALL_TIMEOUT)
    }

    fn client_with_timeout(&self, node: &str, timeout: Duration) -> Result<Client> {
        if node == self.local_name {
            return Err(super::no_local_agent());
        }
        let info = self
            .inventory
            .get(node)
            .map_err(|error| RpcError::new(Code::Unavailable, error.to_string()))?;
        self.dialer.client(&info, timeout)
    }

    fn snapshot(&self, id: &str) -> Result<v1::Job> {
        self.state
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.job.clone())
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut Stored)) -> Result<v1::Job> {
        let _writer = self.writer.lock().unwrap();
        let mut stored = self
            .state
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))?;
        f(&mut stored);
        persist(&self.root, id, &stored)?;
        let job = stored.job.clone();
        self.state.lock().unwrap().insert(id.to_owned(), stored);
        self.changed.notify_waiters();
        Ok(job)
    }

    fn place(
        &self,
        target: &Target,
        caller: &AuthContext,
        jobs: &BTreeMap<String, Stored>,
        monitor: &Monitor,
        leases: &LeaseTable,
    ) -> Result<Placement> {
        let now = unix_millis();
        let leased = if let Target::LeaseId(id) = target {
            if id.is_empty() {
                return Err(RpcError::invalid("lease id is required"));
            }
            let entry = leases.get(id, now).map_err(|error| match error {
                protectorate_lease::Error::NotFound(_) => {
                    RpcError::new(Code::NotFound, error.to_string())
                }
                _ => lease_error(error),
            })?;
            let Entry::Active(lease) = entry else {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "lease is queued and not active",
                ));
            };
            Some(lease.resource)
        } else {
            None
        };
        let named = match target {
            Target::Node(name) => Some(name.as_str()),
            _ => None,
        };
        if let Some(name) = leased
            .as_ref()
            .map(|resource| resource.node.as_str())
            .or(named)
        {
            let name = name.to_ascii_lowercase();
            if self.is_draining(&name) {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    format!("node {name} is being removed"),
                ));
            }
            let (address, tier, invited) = if name == self.local_name {
                ("local".to_string(), SHARED.to_string(), false)
            } else {
                let info = self
                    .inventory
                    .get(&name)
                    .map_err(|_| RpcError::new(Code::NotFound, "node not found"))?;
                (info.address, info.tier, info.invited)
            };
            if invited {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "node has not joined",
                ));
            }
            let scopes = caller.scopes.iter().map(String::as_str).collect::<Vec<_>>();
            if tier == CARE && !caller.owner && !auth::admin_on(&scopes, &name) {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "tier care node requires an owner or node admin grant",
                ));
            }
            if let Some(resource) = &leased {
                if let Some(gpu) = resource.gpu {
                    let status = monitor.status(&name, &address, false);
                    if status.freshness() == v1::NodeFreshness::Unreachable {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "node is unreachable",
                        ));
                    }
                    if !status.metrics.is_some_and(|metrics| {
                        metrics
                            .gpus
                            .iter()
                            .any(|g| u32::try_from(g.index) == Ok(gpu))
                    }) {
                        return Err(RpcError::new(Code::NotFound, "node has no leased GPU"));
                    }
                }
            } else {
                let resource = Resource {
                    node: name.clone(),
                    gpu: None,
                };
                if leases
                    .blocked(&resource, now)
                    .map_err(lease_error)?
                    .is_some()
                {
                    return Err(RpcError::new(Code::ResourceExhausted, "node is leased"));
                }
            }
            if name == self.local_name {
                return Err(super::no_local_agent());
            }
            return Ok(Placement {
                node: name,
                mode: if tier == CARE {
                    JobMode::Plain
                } else {
                    JobMode::Sandboxed
                },
                gpu: leased.and_then(|resource| resource.gpu),
            });
        }
        let Target::Selector(selector) = target else {
            return Err(RpcError::invalid("target is required"));
        };
        let choices = self
            .inventory
            .list()
            .into_iter()
            .filter(|info| info.tier == SHARED && !info.invited)
            .map(|info| (info.name, info.address));
        let mut best: Option<(f64, Placement)> = None;
        let mut saw_lease = false;
        for (name, address) in choices {
            if self.is_draining(&name) {
                continue;
            }
            if !monitor.is_fresh(&name, &address) {
                continue;
            }
            let Some(metrics) = monitor.status(&name, &address, false).metrics else {
                continue;
            };
            if !selector_matches(&metrics, selector) {
                continue;
            }
            let gpu = if selector.gpu_vram_bytes > 0 {
                let need = selector.gpu_vram_bytes;
                let mut candidates = metrics.gpus.iter().collect::<Vec<_>>();
                candidates.sort_by_key(|g| std::cmp::Reverse(gpu_free(g)));
                let mut selected = None;
                for gpu in candidates {
                    let Ok(index) = u32::try_from(gpu.index) else {
                        continue;
                    };
                    if gpu_free(gpu) < need || gpu_busy(gpu) {
                        continue;
                    }
                    if !accel_matches(&gpu.vendor, &gpu.name, &selector.gpu) {
                        continue;
                    }
                    if jobs.values().any(|s| {
                        !terminal(&s.job)
                            && s.job.node == name
                            && s.job.gpu_index.and_then(|i| u32::try_from(i).ok()) == Some(index)
                    }) {
                        saw_lease = true;
                        continue;
                    }
                    let resource = Resource {
                        node: name.clone(),
                        gpu: Some(index),
                    };
                    if leases
                        .blocked(&resource, now)
                        .map_err(lease_error)?
                        .is_some()
                    {
                        saw_lease = true;
                        continue;
                    }
                    selected = Some(index);
                    break;
                }
                let Some(index) = selected else { continue };
                Some(index)
            } else {
                let want = selector.gpu.as_str();
                if !want.is_empty()
                    && !metrics
                        .gpus
                        .iter()
                        .any(|gpu| accel_matches(&gpu.vendor, &gpu.name, want) && !gpu_busy(gpu))
                {
                    continue;
                }
                let resource = Resource {
                    node: name.clone(),
                    gpu: None,
                };
                if leases
                    .blocked(&resource, now)
                    .map_err(lease_error)?
                    .is_some()
                {
                    saw_lease = true;
                    continue;
                }
                None
            };
            let cpu = metrics.cpu.as_ref().map_or(0.0, |cpu| cpu.usage_percent);
            let memory = metrics
                .memory
                .as_ref()
                .map_or(0.0, |memory| memory.used_percent);
            let cores = metrics
                .cpu
                .as_ref()
                .map_or(1.0, |cpu| f64::from(cpu.logical_cores))
                .max(1.0);
            let active = jobs
                .values()
                .filter(|s| s.job.node == name && !terminal(&s.job))
                .count() as f64;
            let score = (cpu + memory) / 2.0 + active * 100.0 / cores;
            let candidate = Placement {
                node: name.clone(),
                mode: JobMode::Sandboxed,
                gpu,
            };
            if best
                .as_ref()
                .is_none_or(|(prior, p)| score < *prior || score == *prior && name < p.node)
            {
                best = Some((score, candidate));
            }
        }
        best.map(|(_, p)| p).ok_or_else(|| {
            RpcError::new(
                if saw_lease {
                    Code::ResourceExhausted
                } else {
                    Code::FailedPrecondition
                },
                "no fresh shared node matches selector",
            )
        })
    }

    fn submit(
        self: &Arc<Self>,
        caller: &AuthContext,
        request: v1::SubmitJobRequest,
        monitor: &Monitor,
        leases: &LeaseTable,
        run_id: String,
    ) -> Result<v1::Job> {
        let command = request.command;
        if command.is_empty() {
            return Err(RpcError::invalid("command is required"));
        }
        if command.len() > 256 || command.iter().any(|s| s.is_empty() || s.contains('\0')) {
            return Err(RpcError::invalid("invalid command"));
        }
        let target = request
            .target
            .ok_or_else(|| RpcError::invalid("target is required"))?;
        let Some(chosen) = &target.target else {
            return Err(RpcError::invalid(
                "set a target: lease_id, node or selector",
            ));
        };
        let source = SourcePlan::parse(request.source.as_ref())?;
        let artifact_globs = request.artifact_globs;
        if artifact_globs.len() > 128 || artifact_globs.iter().any(|s| !safe_relative(s)) {
            return Err(RpcError::invalid("invalid artifact globs"));
        }
        if request.admin && matches!(chosen, Target::Selector(_)) {
            return Err(RpcError::invalid(
                "an admin job names its node (or a lease), not a selector",
            ));
        }
        let env = request.env;
        if env.len() > 128
            || env.iter().any(|(k, v)| {
                k.is_empty() || k.len() > 256 || k.contains(['=', '\0']) || v.contains('\0')
            })
        {
            return Err(RpcError::invalid("invalid environment"));
        }
        let timeout = wire::duration(request.timeout.as_ref(), "timeout")?;
        let timeout = if timeout.is_zero() {
            DEFAULT_TIMEOUT
        } else {
            timeout
        };
        if timeout > MAX_TIMEOUT {
            return Err(RpcError::invalid("timeout must be at most 24h"));
        }
        let limits = request.limits.unwrap_or_default();
        if limits.memory_bytes > 0 && limits.memory_bytes < 64 << 20 {
            return Err(RpcError::invalid("memory limit must be at least 64 MiB"));
        }
        let key = request.request_id;
        if !key.is_empty() {
            validate_request_id(&key)?;
        }
        let writer = self.writer.lock().unwrap();
        let state = self.state.lock().unwrap();
        if !key.is_empty() {
            if self
                .canceled_request_ids
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|expires| *expires > unix_seconds())
            {
                return Err(RpcError::new(
                    Code::Aborted,
                    "the submit was canceled by its request id before it reached the hub",
                ));
            }
            if let Some(existing) = state.values().find(|s| s.request_id == key) {
                return Ok(existing.job.clone());
            }
        }
        if state.len() >= MAX_JOBS {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "hub job record limit reached",
            ));
        }
        let mut placement = self.place(chosen, caller, &state, monitor, leases)?;
        if request.admin {
            let scopes = caller.scopes.iter().map(String::as_str).collect::<Vec<_>>();
            if !caller.owner && !auth::admin_on(&scopes, &placement.node) {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    format!(
                        "admin jobs on {} need an admin grant for that node",
                        placement.node
                    ),
                ));
            }
            let address = if placement.node == self.local_name {
                "local".to_owned()
            } else {
                self.inventory
                    .get(&placement.node)
                    .map_err(|_| RpcError::new(Code::NotFound, "node not found"))?
                    .address
            };
            if !monitor.is_fresh(&placement.node, &address) {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "node has no fresh admin capability report",
                ));
            }
            let status = monitor.status(&placement.node, &address, false);
            if status
                .metrics
                .and_then(|metrics| metrics.jobs)
                .is_none_or(|jobs| jobs.admin.is_empty())
            {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "node cannot run admin jobs",
                ));
            }
            placement.mode = JobMode::Admin;
        }
        self.client(&placement.node)?;
        // Placement may have read leases while a drain was starting. Commit
        // the new record only after crossing the shared admission barrier.
        let _admission = self
            .admission
            .as_ref()
            .map(|lock| lock.read().unwrap_or_else(|error| error.into_inner()));
        if self.is_draining(&placement.node) {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("node {} is being removed", placement.node),
            ));
        }
        let id = random_id()?;
        let mut env_keys = env.keys().cloned().collect::<Vec<_>>();
        env_keys.sort();
        let job = v1::Job {
            id: id.clone(),
            state: JobState::Preparing as i32,
            target: Some(target),
            node: placement.node.clone(),
            gpu_index: placement.gpu.map(|gpu| gpu as i32),
            source: source.record(),
            command: command.clone(),
            env_keys,
            timeout: Some(wire::proto_duration(timeout)),
            artifact_globs: artifact_globs.clone(),
            submitted_at: Some(SystemTime::now().into()),
            mode: placement.mode as i32,
            task: caller.task.clone(),
            limits: Some(limits.clone()),
            run_id,
            ..Default::default()
        };
        let stored = Stored {
            job: job.clone(),
            request_id: key,
            cancel_requested: false,
        };
        drop(state);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        options
            .open(self.root.join(format!("{id}.log")))
            .map_err(RpcError::internal)?;
        if let Err(error) = persist(&self.root, &id, &stored) {
            let _ = fs::remove_file(self.root.join(format!("{id}.log")));
            return Err(error);
        }
        self.state.lock().unwrap().insert(id.clone(), stored);
        drop(writer);
        self.changed.notify_waiters();
        let mut run_env = BTreeMap::new();
        if let Some(gpu) = placement.gpu {
            for key in [
                "PROTECTORATE_GPU",
                "CUDA_VISIBLE_DEVICES",
                "HIP_VISIBLE_DEVICES",
            ] {
                run_env.insert(key.to_owned(), gpu.to_string());
            }
        }
        run_env.extend(env);
        let mut run = v1::RunJobRequest {
            job_id: id.clone(),
            command,
            env: run_env,
            timeout: Some(wire::proto_duration(timeout)),
            artifact_globs,
            mode: placement.mode as i32,
            limits: Some(limits),
            ..Default::default()
        };
        match &source {
            SourcePlan::None => {}
            SourcePlan::File { name, .. } => run.artifact_name = name.clone(),
            SourcePlan::Directory { workdir, .. } => {
                run.artifact_archive = true;
                run.workdir = workdir.clone();
            }
            SourcePlan::Git { key, commit, .. } => {
                run.repo_key = key.clone();
                run.commit = commit.clone();
            }
        }
        let jobs = Arc::clone(self);
        std::thread::spawn(move || jobs.run(id, Some((run, source))));
        Ok(job)
    }

    fn ship(
        &self,
        client: &Client,
        id: &str,
        source: &SourcePlan,
        run: &mut v1::RunJobRequest,
    ) -> Result<u64> {
        match source {
            SourcePlan::None => Ok(0),
            SourcePlan::File { path, .. } => {
                let file = File::open(path).map_err(RpcError::internal)?;
                if file.metadata().map_err(RpcError::internal)?.len() > MAX_SOURCE_BYTES {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source exceeds 4 GiB",
                    ));
                }
                self.upload(client, id, file)
            }
            SourcePlan::Directory { path, .. } => {
                let mut archive =
                    tempfile::NamedTempFile::new_in(&self.root).map_err(RpcError::internal)?;
                write_archive(path, archive.as_file_mut())?;
                if archive
                    .as_file()
                    .metadata()
                    .map_err(RpcError::internal)?
                    .len()
                    > MAX_SOURCE_BYTES
                {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source exceeds 4 GiB",
                    ));
                }
                let file = archive.reopen().map_err(RpcError::internal)?;
                self.upload(client, id, file)
            }
            SourcePlan::Git { repo, key, commit } => {
                let prepared = client.unary(
                    node_agent_service::PREPARE_JOB,
                    &v1::PrepareJobRequest {
                        job_id: id.into(),
                        repo_key: key.clone(),
                        commit: commit.clone(),
                    },
                )?;
                if prepared.has_commit {
                    return Ok(0);
                }
                let mut prereqs = Vec::<String>::new();
                for known in &prepared.known_commits {
                    if prereqs.len() == 8 {
                        break;
                    }
                    if known == commit
                        || !matches!(known.len(), 40 | 64)
                        || !known
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        || prereqs.contains(known)
                    {
                        continue;
                    }
                    let present = Command::new("git")
                        .arg("-C")
                        .arg(repo)
                        .args(["cat-file", "-e", &format!("{known}^{{commit}}")])
                        .status()
                        .is_ok_and(|status| status.success());
                    if present {
                        prereqs.push(known.clone());
                    }
                }
                let mut bundle =
                    tempfile::NamedTempFile::new_in(&self.root).map_err(RpcError::internal)?;
                let mut header = String::from("# v2 git bundle\n");
                for prereq in &prereqs {
                    header.push_str(&format!("-{prereq} prerequisite\n"));
                }
                header.push_str(&format!("{commit} refs/pc/{commit}\n\n"));
                bundle
                    .write_all(header.as_bytes())
                    .map_err(RpcError::internal)?;
                let mut child = Command::new("git")
                    .arg("-C")
                    .arg(repo)
                    .args([
                        "pack-objects",
                        "--revs",
                        "--stdout",
                        "--thin",
                        "--delta-base-offset",
                        "-q",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| {
                        RpcError::new(Code::FailedPrecondition, format!("git pack-objects: {e}"))
                    })?;
                let mut stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| RpcError::new(Code::Internal, "git stdin unavailable"))?;
                stdin
                    .write_all(format!("{commit}\n").as_bytes())
                    .map_err(RpcError::internal)?;
                for prereq in &prereqs {
                    stdin
                        .write_all(format!("^{prereq}\n").as_bytes())
                        .map_err(RpcError::internal)?;
                }
                drop(stdin);
                let mut stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| RpcError::new(Code::Internal, "git stdout unavailable"))?;
                let copied = std::io::copy(
                    &mut stdout.by_ref().take(MAX_SOURCE_BYTES + 1),
                    bundle.as_file_mut(),
                )
                .map_err(RpcError::internal)?;
                if copied.saturating_add(header.len() as u64) > MAX_SOURCE_BYTES {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "Git bundle exceeds 4 GiB",
                    ));
                }
                if !child.wait().map_err(RpcError::internal)?.success() {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "git pack-objects failed",
                    ));
                }
                let sent = self.upload(client, id, bundle.reopen().map_err(RpcError::internal)?)?;
                run.bundle_uploaded = true;
                Ok(sent)
            }
        }
    }

    fn upload(&self, client: &Client, id: &str, mut reader: impl Read) -> Result<u64> {
        let mut hash = Sha256::new();
        let mut offset = 0_u64;
        let mut buf = vec![0_u8; UPLOAD_CHUNK];
        loop {
            let n = reader.read(&mut buf).map_err(RpcError::internal)?;
            if offset.saturating_add(n as u64) > MAX_SOURCE_BYTES {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "source exceeds 4 GiB",
                ));
            }
            hash.update(&buf[..n]);
            let last = n == 0;
            let request = v1::UploadJobDataRequest {
                job_id: id.into(),
                offset,
                data: buf[..n].to_vec(),
                last,
                sha256: if last {
                    format!("{:x}", hash.clone().finalize())
                } else {
                    String::new()
                },
            };
            let response = client.unary(node_agent_service::UPLOAD_JOB_DATA, &request)?;
            offset += n as u64;
            if response.received != offset {
                return Err(RpcError::new(
                    Code::DataLoss,
                    "node acknowledged the wrong upload offset",
                ));
            }
            if last {
                return Ok(offset);
            }
        }
    }
    fn run(self: Arc<Self>, id: String, run: Option<(v1::RunJobRequest, SourcePlan)>) {
        if let Some((mut request, source)) = run {
            let client = match self.job_node(&id).and_then(|node| self.client(&node)) {
                Ok(client) => client,
                Err(error) => {
                    let _ = self.fail(&id, &error.message);
                    return;
                }
            };
            if self
                .state
                .lock()
                .unwrap()
                .get(&id)
                .is_some_and(|s| s.cancel_requested)
            {
                let _ = self.finish(&id, JobEnd::Canceled, -1, "canceled before start");
                return;
            }
            let started = std::time::Instant::now();
            match self.ship(&client, &id, &source, &mut request) {
                Ok(sent) => {
                    let elapsed = started.elapsed();
                    if let Err(error) = self.update(&id, |s| {
                        s.job.source_bytes = sent;
                        s.job.transfer_time = Some(wire::proto_duration(elapsed));
                    }) {
                        let _ =
                            self.fail(&id, &format!("record source transfer: {}", error.message));
                        return;
                    }
                }
                Err(error) => {
                    let _ = self.fail(&id, &format!("ship source: {}", error.message));
                    return;
                }
            }
            if self
                .state
                .lock()
                .unwrap()
                .get(&id)
                .is_some_and(|s| s.cancel_requested)
            {
                let _ = self.finish(&id, JobEnd::Canceled, -1, "canceled before start");
                return;
            }
            let mut artifacts = ArtifactSink::new(&self.root, &id);
            let _result = client.server_stream(node_agent_service::RUN_JOB, &request, |event| {
                self.event(&id, event, &mut artifacts)
            });
            if self.snapshot(&id).is_ok_and(|job| terminal(&job)) {
                return;
            }
        }
        // RunJob may have started before transport dropped. The node can be
        // unreachable for a while (a restart, a moved address); look it up
        // again on every attempt rather than failing a live job.
        let deadline = self
            .snapshot(&id)
            .map(|job| recovery_deadline(&job))
            .unwrap_or_else(|_| SystemTime::now());
        loop {
            if self.snapshot(&id).map_or(true, |job| terminal(&job)) {
                return;
            }
            if SystemTime::now() >= deadline {
                let _ = self.fail(
                    &id,
                    "node stayed unreachable past job timeout and recovery grace",
                );
                return;
            }
            let client = match self
                .job_node(&id)
                .and_then(|node| self.client_with_timeout(&node, RECOVERY_CALL_TIMEOUT))
            {
                Ok(client) => client,
                Err(error) => {
                    if self
                        .snapshot(&id)
                        .is_ok_and(|job| job.node == self.local_name)
                    {
                        let _ = self.fail(&id, &error.message);
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
            };
            let mut artifacts = ArtifactSink::new(&self.root, &id);
            let from_offset = self.snapshot(&id).map_or(0, |job| job.log_bytes);
            let result = client.server_stream(
                node_agent_service::ATTACH_JOB,
                &v1::AttachJobRequest {
                    job_id: id.clone(),
                    from_offset,
                },
                |event| self.event(&id, event, &mut artifacts),
            );
            if self.snapshot(&id).is_ok_and(|job| terminal(&job)) {
                return;
            }
            match result {
                Ok(()) if self.snapshot(&id).is_ok_and(|j| terminal(&j)) => return,
                Err(e) if e.code == Code::NotFound => {
                    let _ = self.fail(&id, "node no longer has the job");
                    return;
                }
                Err(e)
                    if matches!(
                        e.code,
                        Code::Unimplemented
                            | Code::InvalidArgument
                            | Code::FailedPrecondition
                            | Code::PermissionDenied
                            | Code::Unauthenticated
                    ) =>
                {
                    let _ = self.fail(&id, &format!("node cannot reattach job: {}", e.message));
                    return;
                }
                _ => std::thread::sleep(Duration::from_secs(2)),
            }
        }
    }

    fn event(&self, id: &str, event: v1::JobEvent, artifacts: &mut ArtifactSink) -> Result<()> {
        match event.event {
            Some(job_event::Event::Started(started)) => {
                self.update(id, |s| {
                    s.job.state = JobState::Running as i32;
                    if s.job.started_at.is_none() {
                        s.job.started_at = Some(SystemTime::now().into());
                    }
                    s.job.isolation = started.isolation;
                    s.job.workspace_reused = started.workspace_reused;
                })?;
                let cancel = self
                    .state
                    .lock()
                    .unwrap()
                    .get(id)
                    .is_some_and(|s| s.cancel_requested);
                if cancel {
                    self.client(&self.job_node(id)?)?.unary(
                        node_agent_service::CANCEL_JOB,
                        &v1::NodeAgentServiceCancelJobRequest { job_id: id.into() },
                    )?;
                }
                Ok(())
            }
            Some(job_event::Event::Output(output)) => {
                let data = output.data;
                let old = self.snapshot(id)?.log_bytes;
                if old + data.len() as u64 > MAX_LOG_BYTES {
                    return self.fail(id, "hub job log exceeded 64 MiB");
                }
                let mut file = OpenOptions::new()
                    .append(true)
                    .open(self.root.join(format!("{id}.log")))
                    .map_err(RpcError::internal)?;
                file.write_all(&data).map_err(RpcError::internal)?;
                file.sync_data().map_err(RpcError::internal)?;
                self.update(id, |s| s.job.log_bytes = old + data.len() as u64)
                    .map(|_| ())
            }
            Some(job_event::Event::Finished(finished)) => {
                let published = artifacts.publish().inspect_err(|error| {
                    let _ = self.fail(id, &format!("store artifacts: {}", error.message));
                })?;
                self.finish_record(id, finished, Some(published))
            }
            Some(job_event::Event::Artifact(artifact)) => {
                artifacts.write(artifact).inspect_err(|error| {
                    let _ = self.fail(id, &format!("store artifacts: {}", error.message));
                })
            }
            None => Ok(()),
        }
    }
    fn finish(&self, id: &str, end: JobEnd, exit_code: i32, error: &str) -> Result<()> {
        let finished = v1::JobFinished {
            end: end as i32,
            exit_code,
            error: error.into(),
            run_time: None,
        };
        self.finish_record(id, finished, None)
    }
    fn finish_record(
        &self,
        id: &str,
        finished: v1::JobFinished,
        artifacts: Option<(Vec<v1::JobArtifact>, u64)>,
    ) -> Result<()> {
        let code = finished.exit_code;
        let state = match finished.end() {
            JobEnd::Exited if code == 0 => JobState::Succeeded,
            JobEnd::TimedOut => JobState::TimedOut,
            JobEnd::Canceled => JobState::Canceled,
            JobEnd::Killed => JobState::Killed,
            JobEnd::Exited | JobEnd::Error | JobEnd::Unspecified => JobState::Failed,
        };
        self.update(id, |s| {
            s.job.state = state as i32;
            s.job.exit_code = code;
            s.job.error = finished.error;
            s.job.finished_at = Some(SystemTime::now().into());
            if finished.run_time.is_some() {
                s.job.run_time = finished.run_time;
            }
            if let Some((files, bytes)) = artifacts {
                s.job.artifacts = files;
                s.job.artifact_bytes = bytes;
            }
        })
        .map(|_| ())
    }
    fn fail(&self, id: &str, error: &str) -> Result<()> {
        self.finish(id, JobEnd::Error, -1, error)
    }

    pub(super) fn cancel(&self, id: &str) -> Result<v1::Job> {
        let done = {
            let state = self.state.lock().unwrap();
            let s = state
                .get(id)
                .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))?;
            terminal(&s.job)
        };
        if done {
            return self.snapshot(id);
        }
        self.update(id, |s| s.cancel_requested = true)?;
        // When the node has no such job yet, the worker sees cancel_requested
        // before launching it.
        self.client(&self.job_node(id)?)?.unary(
            node_agent_service::CANCEL_JOB,
            &v1::NodeAgentServiceCancelJobRequest { job_id: id.into() },
        )?;
        self.snapshot(id)
    }

    fn job_for_request_id_or_cancel(&self, key: &str) -> Result<String> {
        validate_request_id(key)?;
        let _writer = self.writer.lock().unwrap();
        if let Some((id, _)) = self
            .state
            .lock()
            .unwrap()
            .iter()
            .find(|(_, stored)| stored.request_id == key)
        {
            return Ok(id.clone());
        }
        let mut canceled = self.canceled_request_ids.lock().unwrap().clone();
        let now = unix_seconds();
        canceled.retain(|_, expires| *expires > now);
        if !canceled.contains_key(key) && canceled.len() >= MAX_CANCELED_REQUEST_IDS {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "too many request ids in use; retry later",
            ));
        }
        let expires = now + REQUEST_ID_WINDOW.as_secs() as i64;
        canceled.insert(key.to_owned(), expires);
        persist_canceled_request_ids(&self.root, &canceled)?;
        *self.canceled_request_ids.lock().unwrap() = canceled;
        Err(RpcError::new(
            Code::NotFound,
            format!("no job for request id {key:?}; a later submit with it is refused"),
        ))
    }
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let submit = Arc::clone(&hub);
    server.unary(job_service::SUBMIT_JOB, move |call: Call<_>| {
        let hub = Arc::clone(&submit);
        async move {
            let jobs = Arc::clone(&hub.jobs);
            let monitor = Arc::clone(&hub.node_monitor);
            let leases = Arc::clone(&hub.leases);
            let caller = call.auth.clone();
            let run_id = super::actor_run(&hub, &call.auth.actor);
            let job = tokio::task::spawn_blocking(move || {
                jobs.submit(&caller, call.request, &monitor, &leases, run_id)
            })
            .await
            .map_err(super::join_error)??;
            hub.audit.write(AuditEntry {
                actor: call.auth.actor.clone(),
                token_id: call.auth.token_id.clone(),
                task: call.auth.task.clone(),
                procedure: call.method.procedure.clone(),
                subject: job.id.clone(),
                outcome: "ok".into(),
                detail: submit_audit_detail(&job, !call.auth.task.is_empty()),
                ..AuditEntry::default()
            });
            hub.emit(
                "job.updated",
                &job.id,
                json!({"state": job.state().as_str_name(), "node": job.node}),
            );
            Ok(job)
        }
    })?;
    let get = Arc::clone(&hub);
    server.unary(
        job_service::GET_JOB,
        move |call: Call<v1::GetJobRequest>| {
            let hub = Arc::clone(&get);
            async move { hub.jobs.snapshot(&call.request.id) }
        },
    )?;
    let list = Arc::clone(&hub);
    server.unary(
        job_service::LIST_JOBS,
        move |call: Call<v1::ListJobsRequest>| {
            let hub = Arc::clone(&list);
            async move {
                let state = call.request.state;
                let limit = call.request.limit;
                if !(0..=1000).contains(&limit) {
                    return Err(RpcError::invalid("invalid limit"));
                }
                let limit = if limit == 0 { 50 } else { limit as usize };
                let mut jobs = hub
                    .jobs
                    .state
                    .lock()
                    .unwrap()
                    .values()
                    .filter(|s| state == 0 || s.job.state == state)
                    .map(|s| s.job.clone())
                    .collect::<Vec<_>>();
                let submitted = |job: &v1::Job| job.submitted_at.map(|at| (at.seconds, at.nanos));
                jobs.sort_by(|a, b| {
                    submitted(b)
                        .cmp(&submitted(a))
                        .then_with(|| b.id.cmp(&a.id))
                });
                jobs.truncate(limit);
                Ok(v1::ListJobsResponse { jobs })
            }
        },
    )?;
    let cancel = Arc::clone(&hub);
    server.unary(
        job_service::CANCEL_JOB,
        move |call: Call<v1::CancelJobRequest>| {
            let hub = Arc::clone(&cancel);
            async move {
                let v1::CancelJobRequest { id, request_id } = call.request;
                if id.is_empty() == request_id.is_empty() {
                    return Err(RpcError::invalid("set exactly one of id and request_id"));
                }
                let id = if request_id.is_empty() {
                    id
                } else {
                    hub.jobs.job_for_request_id_or_cancel(&request_id)?
                };
                let jobs = Arc::clone(&hub.jobs);
                tokio::task::spawn_blocking(move || jobs.cancel(&id))
                    .await
                    .map_err(super::join_error)?
            }
        },
    )?;
    server.stream(job_service::WATCH_JOB, move |call: Call<_>, sender| {
        let hub = Arc::clone(&hub);
        async move {
            watch(&hub.jobs, call.request, sender).await?;
            Ok(Default::default())
        }
    })?;
    Ok(())
}

async fn watch(jobs: &Jobs, request: v1::WatchJobRequest, sender: StreamSender) -> Result<()> {
    let v1::WatchJobRequest {
        id,
        from_offset: mut offset,
        follow,
    } = request;
    let mut last_state = None;
    loop {
        let notified = jobs.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let job = jobs.snapshot(&id)?;
        let size = job.log_bytes;
        let done = terminal(&job);
        if last_state != Some(job.state) {
            last_state = Some(job.state);
            sender
                .send(&v1::WatchJobResponse {
                    event: Some(watch_job_response::Event::Job(job)),
                })
                .await?;
        }
        if offset > size {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "offset exceeds job log",
            ));
        }
        if offset < size {
            let count = (size - offset).min(CHUNK as u64) as usize;
            let path = jobs.root.join(format!("{id}.log"));
            let mut file = File::open(path).map_err(RpcError::internal)?;
            file.seek(SeekFrom::Start(offset))
                .map_err(RpcError::internal)?;
            let mut data = vec![0; count];
            file.read_exact(&mut data).map_err(RpcError::internal)?;
            sender
                .send(&v1::WatchJobResponse {
                    event: Some(watch_job_response::Event::Log(v1::JobLog { offset, data })),
                })
                .await?;
            offset += count as u64;
            continue;
        }
        if !follow || done {
            return Ok(());
        }
        tokio::select! { _=notified=>{}, _=sender.cancelled()=>return Err(RpcError::new(Code::Canceled,"stream cancelled")) }
    }
}
fn persist(root: &Path, id: &str, stored: &Stored) -> Result<()> {
    let job =
        serde_json::to_value(stored.job.transcode_to_dynamic()).map_err(RpcError::internal)?;
    let bytes = serde_json::to_vec(&json!({"id":id,"job":job,"requestId":stored.request_id,"cancelRequested":stored.cancel_requested})).map_err(RpcError::internal)?;
    let temp = root.join(format!(
        ".{id}-{}.tmp",
        random_hex(8).map_err(RpcError::internal)?
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temp).map_err(RpcError::internal)?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, root.join(format!("{id}.json")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(RpcError::internal)
}
fn persist_canceled_request_ids(root: &Path, canceled: &BTreeMap<String, i64>) -> Result<()> {
    let bytes = serde_json::to_vec(canceled).map_err(RpcError::internal)?;
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(RpcError::internal)?;
    file.write_all(&bytes).map_err(RpcError::internal)?;
    file.as_file().sync_all().map_err(RpcError::internal)?;
    file.persist(root.join("canceled-requests"))
        .map_err(RpcError::internal)?;
    Ok(())
}
fn validate_request_id(key: &str) -> Result<()> {
    if key.len() < 8
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(RpcError::invalid("invalid request id"));
    }
    Ok(())
}
fn terminal(job: &v1::Job) -> bool {
    matches!(
        job.state(),
        JobState::Succeeded
            | JobState::Failed
            | JobState::Canceled
            | JobState::TimedOut
            | JobState::Killed
    )
}
fn required<'a>(value: &'a str, name: &str) -> Result<&'a str> {
    if value.is_empty() {
        return Err(RpcError::invalid(format!("missing {name}")));
    }
    Ok(value)
}
fn recovery_deadline(job: &v1::Job) -> SystemTime {
    let submitted = job
        .submitted_at
        .and_then(|at| SystemTime::try_from(at).ok())
        .unwrap_or_else(SystemTime::now);
    let runtime = job
        .timeout
        .and_then(|timeout| Duration::try_from(timeout).ok())
        .filter(|timeout| !timeout.is_zero() && *timeout <= MAX_TIMEOUT)
        .unwrap_or(DEFAULT_TIMEOUT);
    submitted
        .checked_add(runtime)
        .and_then(|time| time.checked_add(RECOVERY_GRACE))
        .unwrap_or_else(SystemTime::now)
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn random_id() -> Result<String> {
    Ok(format!(
        "{}-{}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        random_hex(12).map_err(RpcError::internal)?
    ))
}
fn lease_error(error: protectorate_lease::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn selector_matches(metrics: &v1::NodeMetrics, selector: &v1::NodeSelector) -> bool {
    for (want, have) in [(&selector.os, &metrics.os), (&selector.arch, &metrics.arch)] {
        if !want.is_empty() && !have.eq_ignore_ascii_case(want) {
            return false;
        }
    }
    let available = metrics
        .memory
        .as_ref()
        .map_or(0, |memory| memory.available_bytes);
    if available < selector.min_free_memory_bytes {
        return false;
    }
    let want_npu = selector.npu.as_str();
    !(!want_npu.is_empty()
        && !metrics
            .npus
            .iter()
            .any(|npu| accel_matches(&npu.vendor, &npu.name, want_npu)))
}
fn accel_matches(vendor: &str, name: &str, want: &str) -> bool {
    format!("{vendor} {name}")
        .to_ascii_lowercase()
        .contains(&want.trim().to_ascii_lowercase())
}
fn gpu_free(gpu: &v1::GpuMetrics) -> u64 {
    gpu.memory_total_bytes
        .saturating_sub(gpu.memory_used_bytes.unwrap_or(0))
}
fn gpu_busy(gpu: &v1::GpuMetrics) -> bool {
    let total = gpu.memory_total_bytes;
    let used = gpu.memory_used_bytes.unwrap_or(0);
    gpu.utilization_percent >= 90.0 || total > 0 && used.saturating_mul(100) / total >= 90
}
