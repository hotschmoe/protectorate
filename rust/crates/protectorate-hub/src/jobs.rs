//! Durable hub records, source shipping, and output relay for node jobs.
//! Node execution continues independently of a client WatchJob stream.
use super::{input, respond, timestamp, Hub};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use protectorate_audit::Entry as AuditEntry;
use protectorate_auth as auth;
use protectorate_connect_client::{Client, TrustRoots};
use protectorate_inventory::{Inventory, CARE, SHARED};
use protectorate_lease::{Entry, LeaseTable, Resource};
use protectorate_node_monitor::{Monitor, NodeRoute, VIRTUAL_PREFIX};
use protectorate_wire::{
    self as wire, Code, Result, RpcError, RpcRequest, StreamSender, UnaryServer,
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
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;
use zeroize::Zeroizing;

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

fn submit_audit_detail(job: &Value, task_grant: bool) -> String {
    let mode_value = job["mode"].as_str().unwrap_or_default();
    let mode = match mode_value {
        "JOB_MODE_ADMIN" => "admin (elevated, unsandboxed)",
        "JOB_MODE_PLAIN" => "plain (agent user, unsandboxed)",
        _ => "sandboxed",
    };
    let mut detail = format!("node {}, {mode}", job["node"].as_str().unwrap_or_default());
    if mode_value == "JOB_MODE_ADMIN" || task_grant {
        let command = job["command"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ");
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
    local_endpoint: Mutex<Option<String>>,
    token: Mutex<Option<Zeroizing<String>>>,
    trust: Mutex<Option<TrustRoots>>,
    route: Mutex<Option<NodeRoute>>,
    resume_deferred: AtomicBool,
    state: Mutex<BTreeMap<String, Stored>>,
    canceled_request_ids: Mutex<BTreeMap<String, u64>>,
    changed: Notify,
    admission: Option<Arc<RwLock<()>>>,
    draining: Option<Arc<Mutex<BTreeSet<String>>>>,
}

#[derive(Clone)]
struct Stored {
    job: Value,
    request_id: String,
    endpoint: String,
    cancel_requested: bool,
}

struct Placement {
    node: String,
    endpoint: String,
    mode: &'static str,
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
    fn parse(source: Option<&Value>) -> Result<Self> {
        let Some(source) = source.filter(|v| !v.is_null()) else {
            return Ok(Self::None);
        };
        if let Some(git) = source.get("git") {
            let raw = field(git, "repo")?;
            let repo = fs::canonicalize(raw)
                .map_err(|e| RpcError::invalid(format!("repository {raw:?}: {e}")))?;
            if !repo.is_dir() {
                return Err(RpcError::invalid("repository is not a directory"));
            }
            let rev = git
                .get("commit")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("HEAD");
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
            let key = format!(
                "{base}-{:02x}{:02x}{:02x}{:02x}",
                digest[0], digest[1], digest[2], digest[3]
            );
            return Ok(Self::Git { repo, key, commit });
        }
        if let Some(artifact) = source.get("artifact") {
            let raw = field(artifact, "path")?;
            let path = fs::canonicalize(raw)
                .map_err(|e| RpcError::invalid(format!("source artifact {raw:?}: {e}")))?;
            let meta = fs::metadata(&path).map_err(internal)?;
            let workdir = artifact
                .get("workdir")
                .and_then(Value::as_str)
                .unwrap_or("");
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
            return Ok(Self::File { path, name });
        }
        Err(RpcError::invalid("unknown job source kind"))
    }

    fn record(&self) -> Option<Value> {
        match self {
            Self::None => None,
            Self::File { path, .. } => Some(json!({"artifact":{"path":path.to_string_lossy()}})),
            Self::Directory { path, workdir } => {
                Some(json!({"artifact":{"path":path.to_string_lossy(),"workdir":workdir}}))
            }
            Self::Git { repo, commit, .. } => {
                Some(json!({"git":{"repo":repo.to_string_lossy(),"commit":commit}}))
            }
        }
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
            .map_err(internal)?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(internal)?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            let meta = fs::symlink_metadata(&path).map_err(internal)?;
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
            let rel = path.strip_prefix(root).map_err(internal)?;
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
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                pending.push(path);
            } else {
                raw_bytes = raw_bytes.saturating_add(meta.len());
                if raw_bytes > MAX_SOURCE_BYTES {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source directory exceeds 4 GiB",
                    ));
                }
                zip.start_file(name, options)
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                let mut input = File::open(&path).map_err(internal)?;
                let copied = std::io::copy(
                    &mut std::io::Read::by_ref(&mut input).take(meta.len() + 1),
                    &mut zip,
                )
                .map_err(internal)?;
                if copied != meta.len() {
                    return Err(RpcError::new(
                        Code::DataLoss,
                        "source file changed while archiving",
                    ));
                }
            }
        }
    }
    zip.finish()
        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    Ok(())
}

struct ArtifactSink {
    root: PathBuf,
    id: String,
    stage: Option<tempfile::TempDir>,
    current: Option<ArtifactFile>,
    files: Vec<Value>,
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

    fn write(&mut self, chunk: &Value) -> Result<()> {
        let path = field(chunk, "path")?;
        if !safe_relative(path) || path.len() > 4096 {
            return Err(RpcError::new(Code::DataLoss, "unsafe artifact path"));
        }
        if self.current.as_ref().is_some_and(|file| file.path != path) {
            return Err(RpcError::new(Code::DataLoss, "artifact frames interleaved"));
        }
        let data = STANDARD
            .decode(field(chunk, "data")?)
            .map_err(|e| RpcError::new(Code::DataLoss, format!("artifact data: {e}")))?;
        if data.len() > UPLOAD_CHUNK
            || self.total.saturating_add(data.len() as u64) > MAX_ARTIFACT_BYTES
        {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "artifacts exceed 4 GiB",
            ));
        }
        if self.current.is_none() {
            if self.files.len() >= 20_000 || self.files.iter().any(|v| v["path"] == path) {
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
                        .map_err(internal)?,
                );
            }
            let dest = self.stage.as_ref().unwrap().path().join(path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(internal)?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options.open(dest).map_err(internal)?;
            self.current = Some(ArtifactFile {
                path: path.into(),
                file,
                size: 0,
                mode: chunk.get("mode").and_then(number).unwrap_or(0) as u32,
            });
        }
        let current = self.current.as_mut().unwrap();
        current.file.write_all(&data).map_err(internal)?;
        current.size += data.len() as u64;
        self.total += data.len() as u64;
        if chunk.get("last").and_then(Value::as_bool) == Some(true) {
            let current = self.current.take().unwrap();
            current.file.sync_all().map_err(internal)?;
            #[cfg(unix)]
            fs::set_permissions(
                self.stage.as_ref().unwrap().path().join(&current.path),
                fs::Permissions::from_mode((current.mode & 0o777) | 0o600),
            )
            .map_err(internal)?;
            self.files.push(json!({"path":current.path,"sizeBytes":current.size.to_string(),"mode":current.mode}));
        }
        Ok(())
    }

    fn publish(&mut self) -> Result<(Vec<Value>, u64)> {
        if self.current.is_some() {
            return Err(RpcError::new(Code::DataLoss, "unfinished artifact file"));
        }
        if let Some(stage) = self.stage.take() {
            let dest = self.root.join(format!("{}.artifacts", self.id));
            if dest.exists() {
                fs::remove_dir_all(&dest).map_err(internal)?;
            }
            fs::rename(stage.path(), dest).map_err(internal)?;
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
            let id = field(&value, "id").map_err(|e| std::io::Error::other(e.message))?;
            if path.file_stem().and_then(|s| s.to_str()) != Some(id) || !valid_id(id) {
                return Err(std::io::Error::other("invalid persisted job id"));
            }
            let stored = Stored {
                job: value
                    .get("job")
                    .cloned()
                    .ok_or_else(|| std::io::Error::other("missing persisted job"))?,
                request_id: value
                    .get("requestId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                endpoint: value
                    .get("endpoint")
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
            let committed = stored.job.get("logBytes").and_then(number).unwrap_or(0);
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
            local_endpoint: Mutex::new(None),
            token: Mutex::new(None),
            trust: Mutex::new(None),
            route: Mutex::new(None),
            resume_deferred: AtomicBool::new(false),
            state: Mutex::new(state),
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
            .filter(|(_, stored)| stored.job["node"] == name && !terminal(&stored.job))
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn is_draining(&self, name: &str) -> bool {
        self.draining
            .as_ref()
            .is_some_and(|set| set.lock().unwrap().contains(name))
    }

    pub(super) fn uninstall_node(&self, address: &str) -> Result<Value> {
        let resolved = self.resolve_endpoint(address, address)?;
        if let Some(url) = resolved.strip_prefix(VIRTUAL_PREFIX) {
            return Client::new_virtual(url, Duration::from_secs(30))?
                .call("/protectorate.v1.NodeAgentService/Uninstall", &json!({}));
        }
        let token = self
            .token
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                RpcError::new(Code::Unavailable, "node agent token is not configured")
            })?;
        let trust = self.trust.lock().unwrap().clone();
        let client = Client::new_with_trust(
            &resolved,
            token.to_string(),
            Duration::from_secs(30),
            trust.as_ref(),
        )?;
        client.call("/protectorate.v1.NodeAgentService/Uninstall", &json!({}))
    }

    pub fn set_token(self: &Arc<Self>, token: String) -> std::result::Result<(), &'static str> {
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err("invalid node agent token");
        }
        *self.token.lock().unwrap() = Some(Zeroizing::new(token));
        if self.resume_deferred.load(Ordering::Acquire) {
            self.resume_pending(Some(false));
            return Ok(());
        }
        self.resume_pending(None);
        Ok(())
    }

    /// Keep persisted jobs dormant until startup has installed the live node route.
    pub fn defer_resume(&self) {
        self.resume_deferred.store(true, Ordering::Release);
    }

    /// Launch persisted jobs once after the authenticated tailnet is ready.
    pub fn resume_deferred(self: &Arc<Self>) {
        if self.resume_deferred.swap(false, Ordering::AcqRel) {
            self.resume_pending(Some(true));
        }
    }

    fn resume_pending(self: &Arc<Self>, remote: Option<bool>) {
        let resume = self
            .state
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| {
                !terminal(&s.job)
                    && remote.is_none_or(|remote| {
                        (s.job["node"].as_str() != Some(self.local_name.as_str())) == remote
                    })
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in resume {
            self.launch(id, true);
        }
    }

    pub fn set_local_endpoint(&self, endpoint: String) {
        *self.local_endpoint.lock().unwrap() = Some(endpoint);
    }

    pub fn set_trust(&self, trust: TrustRoots) {
        *self.trust.lock().unwrap() = Some(trust);
    }

    pub fn set_route(&self, route: NodeRoute) {
        *self.route.lock().unwrap() = Some(route);
    }

    fn resolve_endpoint(&self, name: &str, address: &str) -> Result<String> {
        match self.route.lock().unwrap().clone() {
            Some(route) => {
                route(name, address).map_err(|error| RpcError::new(Code::Unavailable, error))
            }
            None => endpoint(address),
        }
    }

    fn job_endpoint(&self, id: &str) -> Result<String> {
        let state = self.state.lock().unwrap();
        let stored = state
            .get(id)
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))?;
        let name = stored.job["node"].as_str().unwrap_or_default().to_owned();
        let old = stored.endpoint.clone();
        drop(state);
        if name == self.local_name || self.route.lock().unwrap().is_none() {
            return Ok(old);
        }
        let node = self
            .inventory
            .get(&name)
            .map_err(|error| RpcError::new(Code::Unavailable, error.to_string()))?;
        self.resolve_endpoint(&name, &node.address)
    }

    fn client(&self, endpoint: &str) -> Result<Client> {
        self.client_with_timeout(endpoint, NODE_CALL_TIMEOUT)
    }

    fn client_with_timeout(&self, endpoint: &str, timeout: Duration) -> Result<Client> {
        if let Some(endpoint) = endpoint.strip_prefix(VIRTUAL_PREFIX) {
            return Client::new_virtual(endpoint, timeout);
        }
        let token = self
            .token
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                RpcError::new(Code::Unavailable, "node agent token is not configured")
            })?;
        let trust = self.trust.lock().unwrap().clone();
        Client::new_with_trust(endpoint, token.to_string(), timeout, trust.as_ref())
    }

    fn snapshot(&self, id: &str) -> Result<Value> {
        self.state
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.job.clone())
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut Stored)) -> Result<Value> {
        let mut state = self.state.lock().unwrap();
        let stored = state
            .get_mut(id)
            .ok_or_else(|| RpcError::new(Code::NotFound, "job not found"))?;
        f(stored);
        persist(&self.root, id, stored)?;
        let job = stored.job.clone();
        drop(state);
        self.changed.notify_waiters();
        Ok(job)
    }

    fn place(
        &self,
        target: &Value,
        request: &RpcRequest,
        jobs: &BTreeMap<String, Stored>,
        monitor: &Monitor,
        leases: &LeaseTable,
    ) -> Result<Placement> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX);
        let leased = if let Some(id) = target.get("leaseId").and_then(Value::as_str) {
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
        if let Some(name) = leased
            .as_ref()
            .map(|resource| resource.node.as_str())
            .or_else(|| target.get("node").and_then(Value::as_str))
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
            let scopes = request
                .auth
                .scopes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            if tier == CARE && !request.auth.owner && !auth::admin_on(&scopes, &name) {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "tier care node requires an owner or node admin grant",
                ));
            }
            if let Some(resource) = &leased {
                if let Some(gpu) = resource.gpu {
                    let status = monitor.status(&name, &address, false);
                    if status["freshness"] == "NODE_FRESHNESS_UNREACHABLE" {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "node is unreachable",
                        ));
                    }
                    if !status["metrics"]["gpus"].as_array().is_some_and(|gpus| {
                        gpus.iter()
                            .any(|g| g.get("index").and_then(number).unwrap_or(0) == gpu as u64)
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
            let endpoint = if name == self.local_name {
                self.local_endpoint.lock().unwrap().clone().ok_or_else(|| {
                    RpcError::new(
                        Code::Unavailable,
                        "local node agent endpoint is not configured",
                    )
                })?
            } else {
                self.resolve_endpoint(&name, &address)?
            };
            return Ok(Placement {
                node: name,
                endpoint,
                mode: if tier == CARE {
                    "JOB_MODE_PLAIN"
                } else {
                    "JOB_MODE_SANDBOXED"
                },
                gpu: leased.and_then(|resource| resource.gpu),
            });
        }
        let selector = target
            .get("selector")
            .ok_or_else(|| RpcError::invalid("target is required"))?;
        let mut choices = Vec::new();
        let local = self.local_endpoint.lock().unwrap().clone();
        if let Some(url) = local {
            choices.push((self.local_name.clone(), "local".to_string(), url));
        }
        for info in self.inventory.list() {
            if info.tier != SHARED || info.invited {
                continue;
            }
            if let Ok(url) = self.resolve_endpoint(&info.name, &info.address) {
                choices.push((info.name, info.address, url));
            }
        }
        let mut best: Option<(f64, Placement)> = None;
        let mut saw_lease = false;
        for (name, address, url) in choices {
            if self.is_draining(&name) {
                continue;
            }
            if !monitor.is_fresh(&name, &address) {
                continue;
            }
            let status = monitor.status(&name, &address, false);
            let metrics = &status["metrics"];
            if !selector_matches(metrics, selector) {
                continue;
            }
            let gpu = if let Some(need) = selector
                .get("gpuVramBytes")
                .and_then(number)
                .filter(|n| *n > 0)
            {
                let mut candidates = metrics["gpus"].as_array().cloned().unwrap_or_default();
                candidates.sort_by_key(|g| std::cmp::Reverse(gpu_free(g)));
                let mut selected = None;
                for gpu in candidates {
                    // Protobuf JSON omits index zero, the common first GPU.
                    let Some(index) = gpu
                        .get("index")
                        .map(number)
                        .unwrap_or(Some(0))
                        .and_then(|n| u32::try_from(n).ok())
                    else {
                        continue;
                    };
                    if gpu_free(&gpu) < need || gpu_busy(&gpu) {
                        continue;
                    }
                    if !accel_matches(&gpu, selector["gpu"].as_str().unwrap_or_default()) {
                        continue;
                    }
                    if jobs.values().any(|s| {
                        !terminal(&s.job)
                            && s.job["node"] == name
                            && s.job.get("gpuIndex").and_then(number) == Some(index as u64)
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
                let want = selector["gpu"].as_str().unwrap_or_default();
                if !want.is_empty()
                    && !metrics["gpus"].as_array().is_some_and(|gpus| {
                        gpus.iter()
                            .any(|gpu| accel_matches(gpu, want) && !gpu_busy(gpu))
                    })
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
            let cpu = metrics["cpu"]["usagePercent"].as_f64().unwrap_or(0.0);
            let memory = metrics["memory"]["usedPercent"].as_f64().unwrap_or(0.0);
            let cores = metrics["cpu"]["logicalCores"]
                .as_f64()
                .unwrap_or(1.0)
                .max(1.0);
            let active = jobs
                .values()
                .filter(|s| s.job["node"] == name && !terminal(&s.job))
                .count() as f64;
            let score = (cpu + memory) / 2.0 + active * 100.0 / cores;
            let candidate = Placement {
                node: name.clone(),
                endpoint: url,
                mode: "JOB_MODE_SANDBOXED",
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
        request: &RpcRequest,
        value: Value,
        monitor: &Monitor,
        leases: &LeaseTable,
    ) -> Result<Value> {
        let command = value
            .get("command")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::invalid("command is required"))?;
        if command.is_empty()
            || command.len() > 256
            || command
                .iter()
                .any(|v| v.as_str().is_none_or(|s| s.is_empty() || s.contains('\0')))
        {
            return Err(RpcError::invalid("invalid command"));
        }
        let target = value
            .get("target")
            .ok_or_else(|| RpcError::invalid("target is required"))?;
        if target.get("leaseId").is_none()
            && target.get("node").is_none()
            && target.get("selector").is_none()
        {
            return Err(RpcError::invalid(
                "set a target: lease_id, node or selector",
            ));
        }
        let source = SourcePlan::parse(value.get("source"))?;
        let artifact_globs = value
            .get("artifactGlobs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if artifact_globs.len() > 128
            || artifact_globs
                .iter()
                .any(|v| v.as_str().is_none_or(|s| !safe_relative(s)))
        {
            return Err(RpcError::invalid("invalid artifact globs"));
        }
        let admin = value.get("admin").and_then(Value::as_bool).unwrap_or(false);
        if admin && target.get("selector").is_some() {
            return Err(RpcError::invalid(
                "an admin job names its node (or a lease), not a selector",
            ));
        }
        let env = value.get("env").cloned().unwrap_or_else(|| json!({}));
        let env_map = env
            .as_object()
            .ok_or_else(|| RpcError::invalid("invalid environment"))?;
        if env_map.len() > 128
            || env_map.iter().any(|(k, v)| {
                k.is_empty()
                    || k.len() > 256
                    || k.contains(['=', '\0'])
                    || v.as_str().is_none_or(|s| s.contains('\0'))
            })
        {
            return Err(RpcError::invalid("invalid environment"));
        }
        let timeout = value
            .get("timeout")
            .and_then(Value::as_str)
            .unwrap_or("3600s");
        let seconds = parse_seconds(timeout)?;
        if seconds <= 0.0 || seconds > 86400.0 {
            return Err(RpcError::invalid("timeout must be at most 24h"));
        }
        let limits = value.get("limits").cloned().unwrap_or_else(|| json!({}));
        if limits
            .get("memoryBytes")
            .and_then(number)
            .is_some_and(|n| n > 0 && n < 64 << 20)
        {
            return Err(RpcError::invalid("memory limit must be at least 64 MiB"));
        }
        let key = value
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !key.is_empty() {
            validate_request_id(key)?;
        }
        let mut state = self.state.lock().unwrap();
        if !key.is_empty() {
            if self
                .canceled_request_ids
                .lock()
                .unwrap()
                .get(key)
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
        let mut placement = self.place(target, request, &state, monitor, leases)?;
        if admin {
            let scopes = request
                .auth
                .scopes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            if !request.auth.owner && !auth::admin_on(&scopes, &placement.node) {
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
            let capability = status["metrics"]["jobs"]["admin"]
                .as_str()
                .unwrap_or_default();
            if capability.is_empty() {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "node cannot run admin jobs",
                ));
            }
            placement.mode = "JOB_MODE_ADMIN";
        }
        self.client(&placement.endpoint)?;
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
        let mut env_keys = env_map.keys().cloned().collect::<Vec<_>>();
        env_keys.sort();
        let mut job = json!({"id":id,"state":"JOB_STATE_PREPARING","target":target,"node":placement.node,"command":command,"envKeys":env_keys,"timeout":timeout,"submittedAt":timestamp(Utc::now()),"mode":placement.mode,"limits":limits});
        if !request.auth.task.is_empty() {
            job["task"] = json!(request.auth.task);
        }
        if let Some(record) = source.record() {
            job["source"] = record;
        }
        if !artifact_globs.is_empty() {
            job["artifactGlobs"] = json!(artifact_globs);
        }
        if let Some(gpu) = placement.gpu {
            job["gpuIndex"] = json!(gpu);
        }
        let stored = Stored {
            job: job.clone(),
            request_id: key.into(),
            endpoint: placement.endpoint,
            cancel_requested: false,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        options
            .open(self.root.join(format!("{id}.log")))
            .map_err(internal)?;
        if let Err(error) = persist(&self.root, &id, &stored) {
            let _ = fs::remove_file(self.root.join(format!("{id}.log")));
            return Err(error);
        }
        state.insert(id.clone(), stored);
        drop(state);
        self.changed.notify_waiters();
        let mut run_env = serde_json::Map::new();
        if let Some(gpu) = placement.gpu {
            for key in [
                "PROTECTORATE_GPU",
                "CUDA_VISIBLE_DEVICES",
                "HIP_VISIBLE_DEVICES",
            ] {
                run_env.insert(key.into(), json!(gpu.to_string()));
            }
        }
        run_env.extend(env_map.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut run = json!({"jobId":id,"command":command,"env":run_env,"timeout":timeout,"mode":placement.mode,"limits":limits,"artifactGlobs":artifact_globs});
        match &source {
            SourcePlan::None => {}
            SourcePlan::File { name, .. } => run["artifactName"] = json!(name),
            SourcePlan::Directory { workdir, .. } => {
                run["artifactArchive"] = json!(true);
                run["workdir"] = json!(workdir);
            }
            SourcePlan::Git { key, commit, .. } => {
                run["repoKey"] = json!(key);
                run["commit"] = json!(commit);
            }
        }
        self.launch_run(id, run, source);
        Ok(job)
    }

    fn launch_run(self: &Arc<Self>, id: String, run: Value, source: SourcePlan) {
        let jobs = Arc::clone(self);
        std::thread::spawn(move || jobs.run(id, Some((run, source))));
    }
    fn launch(self: &Arc<Self>, id: String, attach_only: bool) {
        let jobs = Arc::clone(self);
        std::thread::spawn(move || {
            jobs.run(
                id,
                if attach_only {
                    None
                } else {
                    Some((json!({}), SourcePlan::None))
                },
            )
        });
    }

    fn ship(&self, client: &Client, id: &str, source: &SourcePlan, run: &mut Value) -> Result<u64> {
        match source {
            SourcePlan::None => Ok(0),
            SourcePlan::File { path, .. } => {
                let file = File::open(path).map_err(internal)?;
                if file.metadata().map_err(internal)?.len() > MAX_SOURCE_BYTES {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source exceeds 4 GiB",
                    ));
                }
                self.upload(client, id, file)
            }
            SourcePlan::Directory { path, .. } => {
                let mut archive = tempfile::NamedTempFile::new_in(&self.root).map_err(internal)?;
                write_archive(path, archive.as_file_mut())?;
                if archive.as_file().metadata().map_err(internal)?.len() > MAX_SOURCE_BYTES {
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "source exceeds 4 GiB",
                    ));
                }
                let file = archive.reopen().map_err(internal)?;
                self.upload(client, id, file)
            }
            SourcePlan::Git { repo, key, commit } => {
                let prepared = client.call(
                    "/protectorate.v1.NodeAgentService/PrepareJob",
                    &json!({"jobId":id,"repoKey":key,"commit":commit}),
                )?;
                if prepared.get("hasCommit").and_then(Value::as_bool) == Some(true) {
                    return Ok(0);
                }
                let mut prereqs = Vec::new();
                for known in prepared
                    .get("knownCommits")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if prereqs.len() == 8 {
                        break;
                    }
                    if known == commit
                        || !matches!(known.len(), 40 | 64)
                        || !known
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        || prereqs.iter().any(|p| p == known)
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
                        prereqs.push(known.to_owned());
                    }
                }
                let mut bundle = tempfile::NamedTempFile::new_in(&self.root).map_err(internal)?;
                let mut header = String::from("# v2 git bundle\n");
                for prereq in &prereqs {
                    header.push_str(&format!("-{prereq} prerequisite\n"));
                }
                header.push_str(&format!("{commit} refs/pc/{commit}\n\n"));
                bundle.write_all(header.as_bytes()).map_err(internal)?;
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
                    .map_err(internal)?;
                for prereq in &prereqs {
                    stdin
                        .write_all(format!("^{prereq}\n").as_bytes())
                        .map_err(internal)?;
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
                .map_err(internal)?;
                if copied.saturating_add(header.len() as u64) > MAX_SOURCE_BYTES {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RpcError::new(
                        Code::ResourceExhausted,
                        "Git bundle exceeds 4 GiB",
                    ));
                }
                if !child.wait().map_err(internal)?.success() {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "git pack-objects failed",
                    ));
                }
                let sent = self.upload(client, id, bundle.reopen().map_err(internal)?)?;
                run["bundleUploaded"] = json!(true);
                Ok(sent)
            }
        }
    }

    fn upload(&self, client: &Client, id: &str, mut reader: impl Read) -> Result<u64> {
        let mut hash = Sha256::new();
        let mut offset = 0_u64;
        let mut buf = vec![0_u8; UPLOAD_CHUNK];
        loop {
            let n = reader.read(&mut buf).map_err(internal)?;
            if offset.saturating_add(n as u64) > MAX_SOURCE_BYTES {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "source exceeds 4 GiB",
                ));
            }
            hash.update(&buf[..n]);
            let last = n == 0;
            let mut request = json!({"jobId":id,"offset":offset.to_string(),"data":STANDARD.encode(&buf[..n]),"last":last});
            if last {
                request["sha256"] = json!(format!("{:x}", hash.clone().finalize()));
            }
            let response =
                client.call("/protectorate.v1.NodeAgentService/UploadJobData", &request)?;
            offset += n as u64;
            if response.get("received").and_then(number) != Some(offset) {
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
    fn run(self: Arc<Self>, id: String, run: Option<(Value, SourcePlan)>) {
        if let Some((mut request, source)) = run {
            let client = match self.job_endpoint(&id).and_then(|endpoint| {
                if endpoint.is_empty() {
                    Err(RpcError::new(Code::Unavailable, "node endpoint is missing"))
                } else {
                    self.client(&endpoint)
                }
            }) {
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
                let _ = self.finish(&id, "JOB_END_CANCELED", -1, "canceled before start", None);
                return;
            }
            let started = std::time::Instant::now();
            match self.ship(&client, &id, &source, &mut request) {
                Ok(sent) => {
                    let elapsed = started.elapsed();
                    if let Err(error) = self.update(&id, |s| {
                        s.job["sourceBytes"] = json!(sent.to_string());
                        s.job["transferTime"] = json!(format!(
                            "{}.{:09}s",
                            elapsed.as_secs(),
                            elapsed.subsec_nanos()
                        ));
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
                let _ = self.finish(&id, "JOB_END_CANCELED", -1, "canceled before start", None);
                return;
            }
            let mut artifacts = ArtifactSink::new(&self.root, &id);
            let _result = client.stream(
                "/protectorate.v1.NodeAgentService/RunJob",
                &request,
                |event| self.event(&id, event, &mut artifacts),
            );
            if self.snapshot(&id).is_ok_and(|job| terminal(&job)) {
                return;
            }
        }
        // RunJob may have started before transport dropped. The authenticated
        // route can be temporarily unavailable during a hub or tailnet restart;
        // resolve it again on every attempt rather than failing a live job or
        // holding a stale endpoint forever.
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
            let client = match self.job_endpoint(&id).and_then(|endpoint| {
                if endpoint.is_empty() {
                    Err(RpcError::new(Code::Unavailable, "node endpoint is missing"))
                } else {
                    self.client_with_timeout(&endpoint, RECOVERY_CALL_TIMEOUT)
                }
            }) {
                Ok(client) => client,
                Err(error) => {
                    if self
                        .snapshot(&id)
                        .is_ok_and(|job| job["node"] == self.local_name)
                    {
                        let _ = self.fail(&id, &error.message);
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
            };
            let mut artifacts = ArtifactSink::new(&self.root, &id);
            let offset = self
                .snapshot(&id)
                .ok()
                .and_then(|v| v.get("logBytes").and_then(number))
                .unwrap_or(0);
            let result = client.stream(
                "/protectorate.v1.NodeAgentService/AttachJob",
                &json!({"jobId":id,"fromOffset":offset.to_string()}),
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

    fn event(&self, id: &str, event: Value, artifacts: &mut ArtifactSink) -> Result<()> {
        if let Some(started) = event.get("started") {
            self.update(id, |s| {
                s.job["state"] = json!("JOB_STATE_RUNNING");
                if s.job.get("startedAt").is_none() {
                    s.job["startedAt"] = json!(timestamp(Utc::now()));
                }
                s.job["isolation"] = started.get("isolation").cloned().unwrap_or(Value::Null);
                s.job["workspaceReused"] = started
                    .get("workspaceReused")
                    .cloned()
                    .unwrap_or(Value::Null);
            })?;
            let cancel = self
                .state
                .lock()
                .unwrap()
                .get(id)
                .is_some_and(|s| s.cancel_requested);
            if cancel {
                let endpoint = self.job_endpoint(id)?;
                self.client(&endpoint)?.call(
                    "/protectorate.v1.NodeAgentService/CancelJob",
                    &json!({"jobId":id}),
                )?;
            }
            return Ok(());
        }
        if let Some(output) = event.get("output") {
            let data = STANDARD
                .decode(field(output, "data")?)
                .map_err(|e| RpcError::new(Code::DataLoss, e.to_string()))?;
            let old = self
                .snapshot(id)?
                .get("logBytes")
                .and_then(number)
                .unwrap_or(0);
            if old + data.len() as u64 > MAX_LOG_BYTES {
                return self.fail(id, "hub job log exceeded 64 MiB");
            }
            let mut file = OpenOptions::new()
                .append(true)
                .open(self.root.join(format!("{id}.log")))
                .map_err(internal)?;
            file.write_all(&data).map_err(internal)?;
            file.sync_data().map_err(internal)?;
            return self
                .update(id, |s| {
                    s.job["logBytes"] = json!((old + data.len() as u64).to_string())
                })
                .map(|_| ());
        }
        if let Some(finished) = event.get("finished") {
            let (files, bytes) = artifacts.publish().inspect_err(|error| {
                let _ = self.fail(id, &format!("store artifacts: {}", error.message));
            })?;
            let end = finished
                .get("end")
                .and_then(Value::as_str)
                .unwrap_or("JOB_END_ERROR");
            // Proto JSON omits the scalar zero for a successful process.
            let code = finished
                .get("exitCode")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            let error = finished.get("error").and_then(Value::as_str).unwrap_or("");
            return self.finish_record(
                id,
                end,
                code,
                error,
                finished.get("runTime").cloned(),
                Some((files, bytes)),
            );
        }
        if let Some(artifact) = event.get("artifact") {
            artifacts.write(artifact).inspect_err(|error| {
                let _ = self.fail(id, &format!("store artifacts: {}", error.message));
            })?;
            return Ok(());
        }
        Ok(())
    }
    fn finish(
        &self,
        id: &str,
        end: &str,
        code: i32,
        error: &str,
        run_time: Option<Value>,
    ) -> Result<()> {
        self.finish_record(id, end, code, error, run_time, None)
    }
    fn finish_record(
        &self,
        id: &str,
        end: &str,
        code: i32,
        error: &str,
        run_time: Option<Value>,
        artifacts: Option<(Vec<Value>, u64)>,
    ) -> Result<()> {
        self.update(id, |s| {
            s.job["state"] = json!(match end {
                "JOB_END_EXITED" if code == 0 => "JOB_STATE_SUCCEEDED",
                "JOB_END_EXITED" | "JOB_END_ERROR" => "JOB_STATE_FAILED",
                "JOB_END_TIMED_OUT" => "JOB_STATE_TIMED_OUT",
                "JOB_END_CANCELED" => "JOB_STATE_CANCELED",
                _ => "JOB_STATE_FAILED",
            });
            s.job["exitCode"] = json!(code);
            s.job["error"] = json!(error);
            s.job["finishedAt"] = json!(timestamp(Utc::now()));
            if let Some(run_time) = run_time {
                s.job["runTime"] = run_time;
            }
            if let Some((files, bytes)) = artifacts {
                s.job["artifacts"] = json!(files);
                s.job["artifactBytes"] = json!(bytes.to_string());
            }
        })
        .map(|_| ())
    }
    fn fail(&self, id: &str, error: &str) -> Result<()> {
        self.finish(id, "JOB_END_ERROR", -1, error, None)
    }

    pub(super) fn cancel(&self, id: &str) -> Result<Value> {
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
        let endpoint = self.job_endpoint(id)?;
        let client = self.client(&endpoint)?;
        let reply = client.call(
            "/protectorate.v1.NodeAgentService/CancelJob",
            &json!({"jobId":id}),
        )?;
        if reply.get("found") == Some(&Value::Bool(false)) {
            // The worker checks cancel_requested before launching.
            return self.snapshot(id);
        }
        self.snapshot(id)
    }

    fn job_for_request_id_or_cancel(&self, key: &str) -> Result<String> {
        validate_request_id(key)?;
        let state = self.state.lock().unwrap();
        if let Some((id, _)) = state.iter().find(|(_, stored)| stored.request_id == key) {
            return Ok(id.clone());
        }
        let mut canceled = self.canceled_request_ids.lock().unwrap();
        let now = unix_seconds();
        canceled.retain(|_, expires| *expires > now);
        if !canceled.contains_key(key) && canceled.len() >= MAX_CANCELED_REQUEST_IDS {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "too many request ids in use; retry later",
            ));
        }
        let expires = now + REQUEST_ID_WINDOW.as_secs();
        let prior = canceled.insert(key.to_owned(), expires);
        if let Err(error) = persist_canceled_request_ids(&self.root, &canceled) {
            if let Some(prior) = prior {
                canceled.insert(key.to_owned(), prior);
            } else {
                canceled.remove(key);
            }
            return Err(error);
        }
        Err(RpcError::new(
            Code::NotFound,
            format!("no job for request id {key:?}; a later submit with it is refused"),
        ))
    }
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let submit = Arc::clone(&hub);
    server.register("/protectorate.v1.JobService/SubmitJob", move |request| {
        let hub = Arc::clone(&submit);
        async move {
            let value = input(&request)?;
            let jobs = Arc::clone(&hub.jobs);
            let monitor = Arc::clone(&hub.node_monitor);
            let leases = Arc::clone(&hub.leases);
            let request2 = request.clone();
            let job = tokio::task::spawn_blocking(move || {
                jobs.submit(&request2, value, &monitor, &leases)
            })
            .await
            .map_err(super::join_error)??;
            hub.audit.write(AuditEntry {
                actor: request.auth.actor.clone(),
                token_id: request.auth.token_id.clone(),
                tailnet: request.auth.tailnet.clone(),
                task: request.auth.task.clone(),
                procedure: request.method.procedure.clone(),
                subject: job["id"].as_str().unwrap_or_default().into(),
                outcome: "ok".into(),
                detail: submit_audit_detail(&job, !request.auth.task.is_empty()),
                ..AuditEntry::default()
            });
            hub.emit(
                "job.updated",
                job["id"].as_str().unwrap_or_default(),
                json!({"state":job["state"],"node":job["node"]}),
            );
            respond(&request, job)
        }
    })?;
    let get = Arc::clone(&hub);
    server.register("/protectorate.v1.JobService/GetJob", move |request| {
        let hub = Arc::clone(&get);
        async move {
            let id = input(&request)?
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let job = hub.jobs.snapshot(&id)?;
            respond(&request, job)
        }
    })?;
    let list = Arc::clone(&hub);
    server.register("/protectorate.v1.JobService/ListJobs", move |request| {
        let hub = Arc::clone(&list);
        async move {
            let value = input(&request)?;
            let state = value
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let limit = value.get("limit").and_then(Value::as_i64).unwrap_or(50);
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
                .filter(|s| state.is_empty() || s.job["state"] == state)
                .map(|s| s.job.clone())
                .collect::<Vec<_>>();
            jobs.sort_by(|a, b| {
                b["submittedAt"]
                    .as_str()
                    .cmp(&a["submittedAt"].as_str())
                    .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
            });
            jobs.truncate(limit);
            respond(&request, json!({"jobs":jobs}))
        }
    })?;
    let cancel = Arc::clone(&hub);
    server.register("/protectorate.v1.JobService/CancelJob", move |request| {
        let hub = Arc::clone(&cancel);
        async move {
            let value = input(&request)?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            let key = value
                .get("requestId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.is_empty() == key.is_empty() {
                return Err(RpcError::invalid("set exactly one of id and request_id"));
            }
            let id = if !key.is_empty() {
                hub.jobs.job_for_request_id_or_cancel(key)?
            } else {
                id.into()
            };
            let jobs = Arc::clone(&hub.jobs);
            let job = tokio::task::spawn_blocking(move || jobs.cancel(&id))
                .await
                .map_err(super::join_error)??;
            respond(&request, job)
        }
    })?;
    server.register_streaming(
        "/protectorate.v1.JobService/WatchJob",
        move |request, sender| {
            let hub = Arc::clone(&hub);
            async move {
                watch(&hub.jobs, &request, sender).await?;
                Ok(Default::default())
            }
        },
    )?;
    Ok(())
}

async fn watch(jobs: &Jobs, request: &RpcRequest, sender: StreamSender) -> Result<()> {
    let value = input(request)?;
    let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
    let mut offset = value.get("fromOffset").and_then(number).unwrap_or(0);
    let follow = value
        .get("follow")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut last_state = String::new();
    loop {
        let notified = jobs.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let job = jobs.snapshot(id)?;
        let state = job["state"].as_str().unwrap_or_default();
        if state != last_state {
            send(request, &sender, json!({"job":job})).await?;
            last_state = state.into();
        }
        let size = job.get("logBytes").and_then(number).unwrap_or(0);
        if offset > size {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "offset exceeds job log",
            ));
        }
        if offset < size {
            let count = (size - offset).min(CHUNK as u64) as usize;
            let path = jobs.root.join(format!("{id}.log"));
            let mut file = File::open(path).map_err(internal)?;
            file.seek(SeekFrom::Start(offset)).map_err(internal)?;
            let mut data = vec![0; count];
            file.read_exact(&mut data).map_err(internal)?;
            send(
                request,
                &sender,
                json!({"log":{"offset":offset.to_string(),"data":STANDARD.encode(data)}}),
            )
            .await?;
            offset += count as u64;
            continue;
        }
        if !follow || terminal(&job) {
            return Ok(());
        }
        tokio::select! { _=notified=>{}, _=sender.cancelled()=>return Err(RpcError::new(Code::Canceled,"stream cancelled")) }
    }
}
async fn send(request: &RpcRequest, sender: &StreamSender, value: Value) -> Result<()> {
    let bytes = serde_json::to_vec(&value).map_err(internal)?;
    sender
        .send(wire::decode_json(request.method.output(), &bytes)?)
        .await
}
fn persist(root: &Path, id: &str, stored: &Stored) -> Result<()> {
    let bytes=serde_json::to_vec(&json!({"id":id,"job":stored.job,"requestId":stored.request_id,"endpoint":stored.endpoint,"cancelRequested":stored.cancel_requested})).map_err(internal)?;
    let mut nonce = [0_u8; 8];
    getrandom::getrandom(&mut nonce).map_err(internal)?;
    let temp = root.join(format!(
        ".{id}-{}.tmp",
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temp).map_err(internal)?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, root.join(format!("{id}.json")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(internal)
}
fn persist_canceled_request_ids(root: &Path, canceled: &BTreeMap<String, u64>) -> Result<()> {
    let bytes = serde_json::to_vec(canceled).map_err(internal)?;
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(internal)?;
    file.write_all(&bytes).map_err(internal)?;
    file.as_file().sync_all().map_err(internal)?;
    file.persist(root.join("canceled-requests"))
        .map_err(internal)?;
    Ok(())
}
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
fn terminal(job: &Value) -> bool {
    matches!(
        job.get("state").and_then(Value::as_str),
        Some(
            "JOB_STATE_SUCCEEDED"
                | "JOB_STATE_FAILED"
                | "JOB_STATE_CANCELED"
                | "JOB_STATE_TIMED_OUT"
        )
    )
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid(format!("missing {k}")))
}
fn number(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}
fn recovery_deadline(job: &Value) -> SystemTime {
    let submitted = job
        .get("submittedAt")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(SystemTime::from)
        .unwrap_or_else(SystemTime::now);
    let runtime = job
        .get("timeout")
        .and_then(Value::as_str)
        .and_then(|value| parse_seconds(value).ok())
        .filter(|seconds| *seconds > 0.0 && *seconds <= 86400.0)
        .map(Duration::from_secs_f64)
        .unwrap_or(Duration::from_secs(3600));
    submitted
        .checked_add(runtime)
        .and_then(|time| time.checked_add(RECOVERY_GRACE))
        .unwrap_or_else(SystemTime::now)
}
fn parse_seconds(s: &str) -> Result<f64> {
    s.strip_suffix('s')
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .ok_or_else(|| RpcError::invalid("invalid timeout"))
}
fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(Code::Internal, e.to_string())
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn random_id() -> Result<String> {
    let mut bytes = [0; 12];
    getrandom::getrandom(&mut bytes).map_err(internal)?;
    Ok(format!(
        "{}-{}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn endpoint(address: &str) -> Result<String> {
    let (scheme, authority) = if let Some(a) = address.strip_prefix("https://") {
        ("https", a)
    } else if let Some(a) = address.strip_prefix("http://") {
        ("http", a)
    } else if address.starts_with("127.0.0.1") || address.starts_with("[::1]") {
        ("http", address)
    } else {
        ("https", address)
    };
    let has_port = if authority.starts_with('[') {
        authority.contains("]:")
    } else {
        authority.contains(':')
    };
    let authority = if has_port {
        authority.to_string()
    } else {
        format!("{authority}:7475")
    };
    let url = format!("{scheme}://{authority}");
    if scheme == "http" && !authority.starts_with("127.0.0.1:") && !authority.starts_with("[::1]:")
    {
        return Err(RpcError::invalid("node HTTP endpoint requires loopback"));
    }
    Ok(url)
}
fn lease_error(error: protectorate_lease::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn selector_matches(metrics: &Value, selector: &Value) -> bool {
    for key in ["os", "arch"] {
        let want = selector[key].as_str().unwrap_or_default();
        if !want.is_empty()
            && !metrics[key]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(want))
        {
            return false;
        }
    }
    if let Some(need) = selector.get("minFreeMemoryBytes").and_then(number) {
        if metrics["memory"]
            .get("availableBytes")
            .and_then(number)
            .unwrap_or(0)
            < need
        {
            return false;
        }
    }
    let want_npu = selector["npu"].as_str().unwrap_or_default();
    if !want_npu.is_empty()
        && !metrics["npus"]
            .as_array()
            .is_some_and(|npus| npus.iter().any(|npu| accel_matches(npu, want_npu)))
    {
        return false;
    }
    true
}
fn accel_matches(device: &Value, want: &str) -> bool {
    let haystack = format!(
        "{} {}",
        device["vendor"].as_str().unwrap_or_default(),
        device["name"].as_str().unwrap_or_default()
    )
    .to_ascii_lowercase();
    haystack.contains(&want.trim().to_ascii_lowercase())
}
fn gpu_free(gpu: &Value) -> u64 {
    gpu.get("memoryTotalBytes")
        .and_then(number)
        .unwrap_or(0)
        .saturating_sub(gpu.get("memoryUsedBytes").and_then(number).unwrap_or(0))
}
fn gpu_busy(gpu: &Value) -> bool {
    let total = gpu.get("memoryTotalBytes").and_then(number).unwrap_or(0);
    let used = gpu.get("memoryUsedBytes").and_then(number).unwrap_or(0);
    gpu["utilizationPercent"].as_f64().unwrap_or(0.0) >= 90.0
        || total > 0 && used.saturating_mul(100) / total >= 90
}

#[cfg(test)]
mod tests {
    use super::{endpoint, recovery_deadline, Jobs, Stored};
    use protectorate_inventory::{Inventory, Node};
    use serde_json::json;
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, SystemTime},
    };

    #[test]
    fn recovery_deadline_is_bounded_by_original_job_timeout() {
        let old = json!({"submittedAt":"2020-01-01T00:00:00Z","timeout":"1s"});
        assert!(recovery_deadline(&old) < SystemTime::now());
        let current = json!({"submittedAt":chrono::Utc::now().to_rfc3339(),"timeout":"10s"});
        assert!(recovery_deadline(&current) > SystemTime::now());
    }

    #[test]
    fn cancel_before_submit_persists_across_restart() {
        let temp = tempfile::tempdir().unwrap();
        let inventory = Arc::new(Inventory::open(temp.path().join("nodes.json"), "hub").unwrap());
        let root = temp.path().join("jobs");
        let jobs = Jobs::open(&root, Arc::clone(&inventory), "hub".into()).unwrap();
        let key = "cancel-before-submit-0001";
        assert_eq!(
            jobs.job_for_request_id_or_cancel(key).unwrap_err().code,
            protectorate_wire::Code::NotFound
        );
        drop(jobs);
        let reopened = Jobs::open(&root, inventory, "hub".into()).unwrap();
        assert!(reopened
            .canceled_request_ids
            .lock()
            .unwrap()
            .get(key)
            .is_some_and(|expires| *expires > super::unix_seconds()));
        assert_eq!(
            reopened
                .job_for_request_id_or_cancel("bad")
                .unwrap_err()
                .code,
            protectorate_wire::Code::InvalidArgument
        );
    }

    #[test]
    fn persisted_job_resumes_once_after_route_installation() {
        let temp = tempfile::tempdir().unwrap();
        let inventory = Arc::new(Inventory::open(temp.path().join("nodes.json"), "hub").unwrap());
        inventory
            .add(
                Node {
                    name: "node".into(),
                    address: "node".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        let jobs = Arc::new(Jobs::open(temp.path().join("jobs"), inventory, "hub".into()).unwrap());
        jobs.state.lock().unwrap().insert(
            "job1".into(),
            Stored {
                job: json!({"node":"node","state":"JOB_STATE_RUNNING"}),
                request_id: String::new(),
                endpoint: "http://127.0.0.1:1".into(),
                cancel_requested: false,
            },
        );
        jobs.state.lock().unwrap().insert(
            "local1".into(),
            Stored {
                job: json!({"node":"hub","state":"JOB_STATE_RUNNING"}),
                request_id: String::new(),
                endpoint: String::new(),
                cancel_requested: false,
            },
        );
        jobs.defer_resume();
        jobs.set_token("legacy-direct-token".into()).unwrap();
        for _ in 0..100 {
            if jobs.state.lock().unwrap()["local1"].job["state"] == "JOB_STATE_FAILED" {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            jobs.state.lock().unwrap()["local1"].job["state"],
            "JOB_STATE_FAILED"
        );
        assert_eq!(
            jobs.state.lock().unwrap()["job1"].job["state"],
            "JOB_STATE_RUNNING"
        );

        let calls = Arc::new(AtomicUsize::new(0));
        let route_calls = Arc::clone(&calls);
        jobs.set_route(Arc::new(move |_, _| {
            route_calls.fetch_add(1, Ordering::SeqCst);
            Err("authenticated node is unavailable".into())
        }));
        jobs.resume_deferred();
        for _ in 0..100 {
            if calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        jobs.resume_deferred();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        jobs.state.lock().unwrap().get_mut("job1").unwrap().job["state"] =
            json!("JOB_STATE_SUCCEEDED");
    }

    #[test]
    fn endpoint_defaults_ports_and_keeps_http_on_loopback() {
        assert_eq!(endpoint("127.0.0.1").unwrap(), "http://127.0.0.1:7475");
        assert_eq!(endpoint("[::1]").unwrap(), "http://[::1]:7475");
        assert_eq!(endpoint("https://compute").unwrap(), "https://compute:7475");
        assert!(endpoint("http://compute:7475").is_err());
        assert!(endpoint("http://127.0.0.1.evil:7475").is_err());
    }
}
