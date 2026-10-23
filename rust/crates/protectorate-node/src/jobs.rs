//! Node-local job execution and reattach state. A disconnected stream does not stop a job.
//! Restart recovery remains an explicit unsupported case.
use crate::admin::{self, Admin};
use crate::artifacts::{self, Artifact};
use crate::journal;
use crate::retention::{self, DiskLimits};
use crate::sources::{valid_id, Source, Sources};
use crate::{decode, respond};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use protectorate_node_metrics::JobIsolation;
use protectorate_wire::{Code, RpcError, RpcRequest, RpcResponse, StreamSender};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{watch, Mutex, Notify, Semaphore},
};

const MAX_LOG: usize = 64 << 20;
const LOG_TRUNCATED: &[u8] = b"\n[Protectorate job output truncated at 64 MiB]\n";
const RETENTION: Duration = Duration::from_secs(3600);
const MAX_RETAINED_JOBS: usize = 4096;
const MAX_TOTAL_LOG: usize = 256 << 20;
const CHUNK: usize = 64 << 10;

#[derive(Clone)]
pub struct Jobs {
    root: PathBuf,
    repo_root: PathBuf,
    sources: Arc<Sources>,
    repo_locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    artifact_lock: Arc<Mutex<()>>,
    disk_limits: DiskLimits,
    runs: Arc<Mutex<BTreeMap<String, Arc<Run>>>>,
    slots: Arc<Semaphore>,
    log_budget: Arc<AtomicUsize>,
    allow_plain: bool,
    admin: Option<Admin>,
    admin_note: String,
    #[cfg(target_os = "linux")]
    sandbox: Option<SandboxConfig>,
    #[cfg(windows)]
    runner: Option<Arc<crate::windows_runner::Runner>>,
    #[cfg(windows)]
    runner_root: Option<PathBuf>,
}
#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct SandboxConfig {
    pub policy: protectorate_sandbox::Policy,
    pub options: protectorate_sandbox::runtime::LaunchOptions,
    /// `None` gives jobs loopback only.
    pub egress: Option<protectorate_sandbox::Egress>,
}
struct Run {
    state: Mutex<State>,
    changed: Notify,
    cancel: watch::Sender<bool>,
    log_budget: Arc<AtomicUsize>,
    persist_dir: Option<PathBuf>,
}
#[derive(Default)]
struct State {
    started: Option<Value>,
    started_at: Option<Instant>,
    log: Vec<u8>,
    log_truncated: bool,
    finished: Option<Value>,
    finished_at: Option<SystemTime>,
    budget_used: usize,
    artifacts: Vec<Artifact>,
}
#[derive(Clone)]
struct Spec {
    id: String,
    command: Vec<String>,
    env: BTreeMap<String, String>,
    mode: Mode,
    timeout: Duration,
    cpu_percent: u32,
    memory_bytes: u64,
    source: Source,
    artifact_globs: Vec<String>,
}
#[derive(Clone, Copy)]
enum Mode {
    Plain,
    Admin,
    Sandboxed,
}

impl Jobs {
    pub fn new(root: impl AsRef<Path>, concurrency: usize) -> io::Result<Self> {
        if concurrency == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "job concurrency must be positive",
            ));
        }
        fs::create_dir_all(root.as_ref())?;
        let root = fs::canonicalize(root.as_ref())?;
        let scratch = root.join("jobs");
        match fs::symlink_metadata(&scratch) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                fs::remove_dir_all(&scratch)?
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "job scratch path is not a directory",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::create_dir(&scratch)?;
        let sources = Arc::new(Sources::new(root.clone())?);
        let recovered = journal::recover(&root, RETENTION)?;
        if recovered.len() > MAX_RETAINED_JOBS {
            return Err(io::Error::other("too many retained node jobs"));
        }
        let log_budget = Arc::new(AtomicUsize::new(0));
        let mut runs = BTreeMap::new();
        for saved in recovered {
            let artifacts =
                artifacts::restore(&saved.dir.join("artifacts")).map_err(io::Error::other)?;
            let used = saved.log.len().saturating_add(LOG_TRUNCATED.len());
            if !reserve_exact(&log_budget, used) {
                return Err(io::Error::other("recovered node logs exceed 256 MiB"));
            }
            let (cancel, _) = watch::channel(false);
            runs.insert(
                saved.id,
                Arc::new(Run {
                    state: Mutex::new(State {
                        started: saved.started,
                        log: saved.log,
                        finished: Some(saved.finished),
                        finished_at: Some(saved.finished_at),
                        budget_used: used,
                        artifacts,
                        ..State::default()
                    }),
                    changed: Notify::new(),
                    cancel,
                    log_budget: Arc::clone(&log_budget),
                    persist_dir: Some(saved.dir),
                }),
            );
        }
        let (admin, admin_note) = admin::probe();
        Ok(Self {
            repo_root: root.join("repos"),
            root,
            sources,
            repo_locks: Arc::new(Mutex::new(BTreeMap::new())),
            artifact_lock: Arc::new(Mutex::new(())),
            disk_limits: DiskLimits::default(),
            runs: Arc::new(Mutex::new(runs)),
            slots: Arc::new(Semaphore::new(concurrency)),
            log_budget,
            allow_plain: false,
            admin,
            admin_note,
            #[cfg(target_os = "linux")]
            sandbox: None,
            #[cfg(windows)]
            runner: None,
            #[cfg(windows)]
            runner_root: None,
        })
    }
    /// Keep node records and sources under the node profile; only sandboxed
    /// workspaces live in the dedicated runner's shared root.
    #[cfg(windows)]
    pub fn open_windows(home: impl AsRef<Path>, concurrency: usize) -> io::Result<Self> {
        let runner = crate::windows_runner::Runner::load(home.as_ref())?;
        if let Some(runner) = &runner {
            reset_windows_runner_scratch(runner)?;
        }
        let mut jobs = Self::new(home, concurrency)?;
        jobs.runner_root = runner.as_ref().map(|runner| runner.root.clone());
        jobs.runner = runner.map(Arc::new);
        Ok(jobs)
    }
    /// Explicitly permits the hub's plain-mode requests to run as the agent user.
    pub fn with_plain_mode(mut self) -> Self {
        self.allow_plain = true;
        self
    }
    pub fn with_disk_limits(mut self, limits: DiskLimits) -> Self {
        self.disk_limits = limits;
        self
    }
    /// The execution capabilities this node reports to the hub for placement.
    pub fn isolation(&self) -> JobIsolation {
        #[cfg(target_os = "linux")]
        let sandbox = if self.sandbox.is_some() {
            "rust-namespaces"
        } else {
            "none"
        };
        #[cfg(windows)]
        let sandbox = if self.runner.is_some() {
            "runner"
        } else {
            "none"
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        let sandbox = "none";
        #[cfg(windows)]
        let note = if self.runner.is_some() {
            format!(
                "sandboxed jobs run as the dedicated runner account in a Job Object; {}",
                self.admin_note
            )
        } else {
            format!("same-user Job Object process tree; sandboxed jobs require a separate runner account; {}", self.admin_note)
        };
        #[cfg(not(windows))]
        let note = self.admin_note.clone();
        JobIsolation {
            sandbox: sandbox.into(),
            note,
            admin: self.admin.as_ref().map_or("", Admin::label).into(),
        }
    }
    #[cfg(target_os = "linux")]
    pub fn with_sandbox(mut self, config: SandboxConfig) -> Self {
        self.sandbox = Some(config);
        self
    }
    pub async fn start(&self, value: Value) -> Result<String, RpcError> {
        self.sweep_expired().await;
        let spec = Spec::parse(&value)?;
        if matches!(spec.mode, Mode::Plain) && !self.allow_plain {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "plain jobs are disabled on this node",
            ));
        }
        if matches!(spec.mode, Mode::Admin) && self.admin.is_none() {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("this node cannot run admin jobs: {}", self.admin_note),
            ));
        }
        if matches!(spec.mode, Mode::Sandboxed) {
            #[cfg(target_os = "linux")]
            if self.sandbox.is_none() {
                return Err(RpcError::new(
                    Code::Unavailable,
                    "sandboxed jobs require a configured sandbox",
                ));
            }
            #[cfg(windows)]
            if self.runner.is_none() {
                return Err(RpcError::new(Code::FailedPrecondition,
                    "sandboxed Windows jobs require the dedicated runner account; run install_windows_runner and restart the node"));
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            return Err(RpcError::new(
                Code::Unimplemented,
                "sandboxed node jobs are unavailable on this platform",
            ));
        }
        let (cancel, _) = watch::channel(false);
        let run = {
            let mut runs = self.runs.lock().await;
            if runs.contains_key(&spec.id) {
                return Err(RpcError::new(Code::AlreadyExists, "job already exists"));
            }
            self.sources.ready(&spec.id, &spec.source)?;
            if runs.len() >= MAX_RETAINED_JOBS
                || !reserve_exact(&self.log_budget, LOG_TRUNCATED.len())
            {
                return Err(RpcError::new(
                    Code::ResourceExhausted,
                    "node job retention is full",
                ));
            }
            let persist_dir = match journal::create(&self.root, &spec.id) {
                Ok(path) => path,
                Err(error) => {
                    self.log_budget
                        .fetch_sub(LOG_TRUNCATED.len(), Ordering::AcqRel);
                    return Err(RpcError::new(
                        Code::Internal,
                        format!("create node job record: {error}"),
                    ));
                }
            };
            let run = Arc::new(Run {
                state: Mutex::new(State {
                    budget_used: LOG_TRUNCATED.len(),
                    ..State::default()
                }),
                changed: Notify::new(),
                cancel,
                log_budget: Arc::clone(&self.log_budget),
                persist_dir: Some(persist_dir),
            });
            runs.insert(spec.id.clone(), Arc::clone(&run));
            run
        };
        let jobs = self.clone();
        let id = spec.id.clone();
        tokio::spawn(async move {
            jobs.execute(spec, run).await;
        });
        Ok(id)
    }
    pub async fn prepare(&self, request: &RpcRequest) -> Result<RpcResponse, RpcError> {
        let input = serde_json::to_value(&request.message).map_err(internal)?;
        let sources = Arc::clone(&self.sources);
        let value = tokio::task::spawn_blocking(move || sources.prepare(&input))
            .await
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
        respond(request, value)
    }
    pub async fn upload(&self, request: &RpcRequest) -> Result<RpcResponse, RpcError> {
        let input = serde_json::to_value(&request.message).map_err(internal)?;
        let id = input
            .get("jobId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if self.runs.lock().await.contains_key(id) {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "job has already started",
            ));
        }
        let sources = Arc::clone(&self.sources);
        let value = tokio::task::spawn_blocking(move || sources.upload(&input))
            .await
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
        respond(request, value)
    }
    pub async fn list(&self, request: &RpcRequest) -> Result<RpcResponse, RpcError> {
        self.sweep_expired().await;
        let runs = self
            .runs
            .lock()
            .await
            .iter()
            .map(|(id, run)| (id.clone(), Arc::clone(run)))
            .collect::<Vec<_>>();
        let mut jobs = Vec::new();
        for (id, run) in runs {
            let state = run.state.lock().await;
            let mut item = json!({"jobId": id, "started": state.started.is_some(), "logBytes": state.log.len().to_string()});
            if let Some(finished) = &state.finished {
                item["finished"] = finished.clone();
            }
            if let Some(finished_at) = state.finished_at {
                item["finishedAt"] =
                    json!(DateTime::<Utc>::from(finished_at)
                        .to_rfc3339_opts(SecondsFormat::Nanos, true));
            }
            jobs.push(item);
        }
        respond(request, json!({"jobs": jobs}))
    }
    pub async fn cancel(&self, request: &RpcRequest) -> Result<RpcResponse, RpcError> {
        self.sweep_expired().await;
        let input = serde_json::to_value(&request.message).map_err(internal)?;
        let id = input
            .get("jobId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let run = self.runs.lock().await.get(id).cloned();
        let found = if let Some(run) = run {
            if run.state.lock().await.finished.is_some() {
                false
            } else {
                run.cancel.send_replace(true);
                true
            }
        } else {
            false
        };
        respond(request, json!({"found": found}))
    }
    pub async fn attach(
        &self,
        request: &RpcRequest,
        sender: StreamSender,
        id: &str,
        mut offset: usize,
    ) -> Result<(), RpcError> {
        self.sweep_expired().await;
        let run = self
            .runs
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| RpcError::new(Code::NotFound, "node has no such job"))?;
        let mut sent_started = false;
        loop {
            let notified = run.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let state = run.state.lock().await;
            if offset > state.log.len() {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "output offset exceeds job log",
                ));
            }
            let started = if sent_started {
                None
            } else {
                state.started.clone()
            };
            let data = state.log[offset..std::cmp::min(offset + CHUNK, state.log.len())].to_vec();
            let finished = if offset + data.len() == state.log.len() {
                state.finished.clone()
            } else {
                None
            };
            let artifacts = if finished.is_some() {
                state.artifacts.clone()
            } else {
                vec![]
            };
            drop(state);
            if let Some(started) = started {
                sender
                    .send(decode(request, json!({"started":started}))?)
                    .await?;
                sent_started = true;
            }
            if !data.is_empty() {
                offset += data.len();
                sender
                    .send(decode(
                        request,
                        json!({"output":{"data":STANDARD.encode(data)}}),
                    )?)
                    .await?;
                continue;
            }
            if let Some(finished) = finished {
                for artifact in artifacts {
                    let mut file = fs::File::open(&artifact.file)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let mut first = true;
                    loop {
                        let mut buf = vec![0u8; CHUNK];
                        let n = file
                            .read(&mut buf)
                            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                        buf.truncate(n);
                        let last = n < CHUNK;
                        let mut frame =
                            json!({"path":artifact.path,"data":STANDARD.encode(&buf),"last":last});
                        if first {
                            frame["mode"] = json!(artifact.mode);
                            first = false;
                        }
                        sender
                            .send(decode(request, json!({"artifact":frame}))?)
                            .await?;
                        if last {
                            break;
                        }
                    }
                }
                sender
                    .send(decode(request, json!({"finished":finished}))?)
                    .await?;
                return Ok(());
            }
            tokio::select! { _ = notified => {}, _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "stream cancelled")) }
        }
    }
    async fn execute(&self, spec: Spec, run: Arc<Run>) {
        let permit = tokio::select! { p = self.slots.clone().acquire_owned() => p, _ = wait_cancel(run.cancel.subscribe()) => { run.finish("JOB_END_CANCELED", -1, "").await; return; } };
        if permit.is_err() {
            run.finish("JOB_END_ERROR", -1, "job scheduler closed")
                .await;
            return;
        }
        let dir = self.root.join("jobs").join(&spec.id);
        if let Err(e) = fs::create_dir_all(&dir) {
            run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
            return;
        }
        let repo_guard = if let Some(key) = spec.source.repo_key() {
            let lock = self
                .repo_locks
                .lock()
                .await
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone();
            match tokio::select! { guard = lock.lock_owned() => Some(guard), _ = wait_cancel(run.cancel.subscribe()) => None }
            {
                Some(guard) => Some(guard),
                None => {
                    run.finish("JOB_END_CANCELED", -1, "").await;
                    return;
                }
            }
        } else {
            None
        };
        let name = if let Some(key) = spec.source.repo_key() {
            let mode = match spec.mode {
                Mode::Plain => "plain",
                Mode::Admin => "admin",
                Mode::Sandboxed => "sandboxed",
            };
            retention::repo_workspace_name(key, mode)
        } else {
            let digest = Sha256::digest(spec.id.as_bytes());
            let name = format!("j_{:x}", digest);
            name[..64].to_string()
        };
        let base = match spec.mode {
            Mode::Plain | Mode::Admin if spec.source.repo_key().is_none() => dir.clone(),
            Mode::Plain | Mode::Admin => {
                #[cfg(target_os = "linux")]
                if let Some(config) = &self.sandbox {
                    config.policy.state_root.join("workspaces").join(&name)
                } else {
                    self.root.join("workspaces").join(&name)
                }
                #[cfg(not(target_os = "linux"))]
                self.root.join("workspaces").join(&name)
            }
            Mode::Sandboxed => {
                #[cfg(target_os = "linux")]
                {
                    self.sandbox
                        .as_ref()
                        .map(|config| config.policy.state_root.join("workspaces").join(&name))
                        .unwrap_or_else(|| dir.clone())
                }
                #[cfg(windows)]
                {
                    let shared = self.runner_root.as_ref().unwrap_or(&self.root);
                    if spec.source.repo_key().is_some() {
                        shared.join("workspaces").join(&name)
                    } else {
                        shared.join("jobs").join(&spec.id)
                    }
                }
                #[cfg(not(any(target_os = "linux", windows)))]
                {
                    dir.clone()
                }
            }
        };
        match spec.mode {
            Mode::Plain => self.execute_plain(&spec, &base, &run, None).await,
            Mode::Admin => {
                self.execute_plain(&spec, &base, &run, self.admin.as_ref())
                    .await
            }
            Mode::Sandboxed => {
                #[cfg(target_os = "linux")]
                self.execute_sandbox(&spec, &base, &run).await;
                #[cfg(windows)]
                self.execute_windows_runner(&spec, &base, &run).await;
            }
        }
        let source = spec.source.clone();
        let id = spec.id.clone();
        let sources = Arc::clone(&self.sources);
        #[cfg(windows)]
        let runner_base = base.clone();
        let _ = tokio::task::spawn_blocking(move || sources.cleanup(&id, &source, &base)).await;
        #[cfg(windows)]
        if matches!(spec.mode, Mode::Sandboxed) && spec.source.repo_key().is_none() {
            if let Some(runner) = &self.runner {
                if let Err(error) =
                    cleanup_windows_runner_workspace(runner, &runner_base, &spec.id).await
                {
                    eprintln!("protectorate node: runner scratch cleanup: {error}");
                }
            }
        }
        drop(repo_guard);
        if spec.source.repo_key().is_some() {
            let mut locks = self.repo_locks.lock().await;
            let active = locks
                .iter()
                .filter(|(_, lock)| Arc::strong_count(lock) > 1)
                .map(|(key, _)| key.clone())
                .collect::<BTreeSet<_>>();
            let root = self.root.clone();
            let repo_root = self.repo_root.clone();
            #[cfg(target_os = "linux")]
            let external = self
                .sandbox
                .as_ref()
                .map(|config| config.policy.state_root.join("workspaces"));
            #[cfg(windows)]
            let external = self
                .runner_root
                .as_ref()
                .map(|root| root.join("workspaces"));
            #[cfg(windows)]
            let runner = self.runner.clone();
            #[cfg(not(any(target_os = "linux", windows)))]
            let external: Option<PathBuf> = None;
            let limit = self.disk_limits.git_cache_bytes;
            let result = tokio::task::spawn_blocking(move || {
                #[cfg(windows)]
                let cleanup = |path: &Path| {
                    let runner = runner
                        .as_ref()
                        .ok_or_else(|| io::Error::other("runner unavailable for cache pruning"))?;
                    remove_windows_runner_directory(runner, path)
                };
                #[cfg(windows)]
                let external_cleanup = external
                    .as_ref()
                    .map(|_| &cleanup as &dyn Fn(&Path) -> io::Result<()>);
                #[cfg(not(windows))]
                let external_cleanup: retention::ExternalCleanup<'_> = None;
                retention::prune_git_caches(
                    &root,
                    &repo_root,
                    external.as_deref(),
                    external_cleanup,
                    &active,
                    limit,
                )
            })
            .await;
            if let Err(error) =
                result.unwrap_or_else(|error| Err(io::Error::other(error.to_string())))
            {
                eprintln!("protectorate node: prune Git job cache: {error}");
            }
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
    }
    async fn sweep_expired(&self) {
        let mut runs = self.runs.lock().await;
        let mut expired = Vec::new();
        for (id, run) in runs.iter() {
            let state = run.state.lock().await;
            if state.finished_at.is_some_and(|at| {
                SystemTime::now()
                    .duration_since(at)
                    .is_ok_and(|age| age >= RETENTION)
            }) {
                expired.push(id.clone());
            }
        }
        let _artifact_guard = self.artifact_lock.lock().await;
        for id in expired {
            if let Some(run) = runs.remove(&id) {
                let state = run.state.lock().await;
                self.log_budget
                    .fetch_sub(state.budget_used, Ordering::AcqRel);
                let _ = fs::remove_dir_all(self.root.join("runs").join(&id));
            }
        }
        let active = runs.keys().cloned().collect::<BTreeSet<_>>();
        self.sources.sweep_orphans(&active);
    }
    async fn execute_plain(&self, spec: &Spec, dir: &Path, run: &Arc<Run>, admin: Option<&Admin>) {
        let sources = Arc::clone(&self.sources);
        let source = spec.source.clone();
        let id = spec.id.clone();
        let base = dir.to_path_buf();
        #[cfg(target_os = "linux")]
        let stage_home = if matches!(source, Source::Git { .. }) {
            self.sandbox.as_ref().map(|config| {
                let digest = Sha256::digest(spec.id.as_bytes());
                let name = format!("j_{digest:x}");
                config
                    .policy
                    .state_root
                    .join("jobs")
                    .join(&name[..64])
                    .join("home")
            })
        } else {
            None
        };
        #[cfg(windows)]
        let stage_home: Option<PathBuf> = None;
        #[cfg(not(any(target_os = "linux", windows)))]
        let stage_home: Option<PathBuf> = None;
        if let Some(home) = &stage_home {
            if let Err(error) = fs::create_dir_all(home) {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        }
        let (workdir, reused) = match tokio::task::spawn_blocking(move || {
            sources.stage(&id, &source, &base, stage_home.as_deref())
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
            Err(error) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        };
        if let Source::Git { commit, .. } = &spec.source {
            #[cfg(target_os = "linux")]
            if let Some(config) = &self.sandbox {
                if let Err(error) =
                    run_linux_git_stage(config, dir, &workdir, commit, &spec.id, spec.timeout).await
                {
                    run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                    return;
                }
            }
            #[cfg(not(target_os = "linux"))]
            let _ = commit;
        }
        let mut command = match admin {
            Some(admin) => Command::from(admin.command(&spec.command, spec.env.keys().cloned())),
            None => Command::new(&spec.command[0]),
        };
        if admin.is_none() {
            command.args(&spec.command[1..]);
        }
        command
            .current_dir(&workdir)
            .envs(&spec.env)
            .env("PROTECTORATE_JOB_ID", &spec.id)
            .env("PROTECTORATE_JOB_DIR", &workdir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        #[cfg(windows)]
        let spawned = crate::windows_process::ProcessTree::spawn(
            &mut command,
            spec.cpu_percent,
            spec.memory_bytes,
        );
        #[cfg(not(windows))]
        let spawned = spawn_unless_busy(&mut command).await;
        #[cfg(windows)]
        let (mut child, process_tree) = match spawned {
            Ok(pair) => pair,
            Err(e) => {
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
                return;
            }
        };
        #[cfg(not(windows))]
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
                return;
            }
        };
        #[cfg(not(windows))]
        let pid = child.id().unwrap_or(0);
        run.started(admin.map_or("plain", Admin::label), reused)
            .await;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        #[cfg(windows)]
        let mut stdout = tokio::spawn(read_log(out, Arc::clone(run)));
        #[cfg(not(windows))]
        let stdout = tokio::spawn(read_log(out, Arc::clone(run)));
        #[cfg(windows)]
        let mut stderr = tokio::spawn(read_log(err, Arc::clone(run)));
        #[cfg(not(windows))]
        let stderr = tokio::spawn(read_log(err, Arc::clone(run)));
        #[cfg(not(windows))]
        let result = tokio::select! {
            status = child.wait() => ("JOB_END_EXITED", status),
            _ = tokio::time::sleep(spec.timeout) => { kill_group(pid); ("JOB_END_TIMED_OUT", child.wait().await) },
            _ = wait_cancel(run.cancel.subscribe()) => { kill_group(pid); ("JOB_END_CANCELED", child.wait().await) },
        };
        #[cfg(windows)]
        let result = tokio::select! {
            status = child.wait() => ("JOB_END_EXITED", status),
            _ = tokio::time::sleep(spec.timeout) => { let _ = process_tree.terminate(); ("JOB_END_TIMED_OUT", bounded_wait(&mut child).await) },
            _ = wait_cancel(run.cancel.subscribe()) => { let _ = process_tree.terminate(); ("JOB_END_CANCELED", bounded_wait(&mut child).await) },
        };
        #[cfg(windows)]
        drop(process_tree);
        #[cfg(windows)]
        if tokio::time::timeout(Duration::from_secs(5), async {
            let _ = (&mut stdout).await;
            let _ = (&mut stderr).await;
        })
        .await
        .is_err()
        {
            stdout.abort();
            stderr.abort();
        }
        #[cfg(not(windows))]
        {
            let _ = stdout.await;
            let _ = stderr.await;
        }
        match result.1 {
            Ok(status) => {
                let (end, code, error) = exit_outcome(result.0, &status);
                self.finish_with_artifacts(spec, &workdir, run, end, code, &error)
                    .await
            }
            Err(e) => {
                #[cfg(windows)]
                run.finish(result.0, -1, &e.to_string()).await;
                #[cfg(not(windows))]
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
            }
        }
    }
    #[cfg(windows)]
    async fn execute_windows_runner(&self, spec: &Spec, base: &Path, run: &Arc<Run>) {
        let Some(runner) = self.runner.clone() else {
            run.finish("JOB_END_ERROR", -1, "dedicated runner is unavailable")
                .await;
            return;
        };
        if let Err(error) = prepare_windows_runner_workspace(
            &runner,
            base,
            matches!(spec.source, Source::Git { .. }),
            &spec.id,
            spec.timeout,
        )
        .await
        {
            run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
            return;
        }
        let sources = Arc::clone(&self.sources);
        let source = spec.source.clone();
        let id = spec.id.clone();
        let base = base.to_path_buf();
        let stage_dir = self.root.join("jobs").join(&spec.id);
        let (workdir, reused) = match tokio::task::spawn_blocking(move || {
            sources.stage(&id, &source, &base, Some(stage_dir.as_path()))
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
            Err(error) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        };
        if let Source::Git { commit, .. } = &spec.source {
            let bundle = self.sources.bundle_path(&spec.id);
            if let Err(error) =
                run_windows_git_stage(&runner, &workdir, &bundle, commit, &spec.id, spec.timeout)
                    .await
            {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        }
        if matches!(spec.source, Source::File { .. } | Source::Archive { .. }) {
            if let Err(error) = run_windows_uploaded_stage(
                &runner,
                &self.sources,
                &spec.source,
                &workdir,
                &spec.id,
                spec.timeout,
            )
            .await
            {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        }
        let command = spec.command.clone();
        let mut env = spec.env.clone();
        if matches!(spec.source, Source::Git { .. }) {
            let safe = match crate::windows_runner::process_path(&workdir) {
                Ok(path) => path.to_string_lossy().replace('\\', "/"),
                Err(error) => {
                    run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                    return;
                }
            };
            env.insert("GIT_CONFIG_COUNT".into(), "1".into());
            env.insert("GIT_CONFIG_KEY_0".into(), "safe.directory".into());
            env.insert("GIT_CONFIG_VALUE_0".into(), safe);
        }
        protectorate_sandbox::limit_build_jobs(&mut env, spec.cpu_percent);
        let id = spec.id.clone();
        let path = workdir.clone();
        let cpu = spec.cpu_percent;
        let memory = spec.memory_bytes;
        let mut process = match tokio::task::spawn_blocking(move || {
            runner.spawn(&command, &env, &path, &id, cpu, memory)
        })
        .await
        {
            Ok(Ok(process)) => process,
            Ok(Err(error)) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
            Err(error) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        };
        run.started("runner", reused).await;
        let output = process.take_output().expect("runner output pipe");
        let mut reader = tokio::spawn(read_log(tokio::fs::File::from_std(output), Arc::clone(run)));
        let (end, result) = tokio::select! {
            status = process.wait() => ("JOB_END_EXITED", status),
            _ = tokio::time::sleep(spec.timeout) => {
                let _ = process.terminate();
                ("JOB_END_TIMED_OUT", bounded_runner_wait(&process).await)
            },
            _ = wait_cancel(run.cancel.subscribe()) => {
                let _ = process.terminate();
                ("JOB_END_CANCELED", bounded_runner_wait(&process).await)
            },
        };
        drop(process); // KILL_ON_JOB_CLOSE reaps any remaining descendants.
        if tokio::time::timeout(Duration::from_secs(5), &mut reader)
            .await
            .is_err()
        {
            reader.abort();
        }
        match result {
            Ok(code) => {
                self.finish_with_artifacts(spec, &workdir, run, end, code, "")
                    .await
            }
            Err(error) => run.finish(end, -1, &error.to_string()).await,
        }
    }
    #[cfg(target_os = "linux")]
    async fn execute_sandbox(&self, spec: &Spec, base: &Path, run: &Arc<Run>) {
        use protectorate_sandbox::{Limits, Request, Workload};
        let Some(config) = self.sandbox.clone() else {
            run.finish("JOB_END_ERROR", -1, "sandbox unavailable").await;
            return;
        };
        // Sandbox private-home names have a narrower grammar than public job IDs.
        let digest = Sha256::digest(spec.id.as_bytes());
        let name = format!("j_{:x}", digest);
        let name = name[..64].to_string();
        let home = config
            .policy
            .state_root
            .join("jobs")
            .join(&name)
            .join("home");
        let workspace = base.to_path_buf();
        let managed_name = workspace
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or_default()
            .to_string();
        if let Err(e) = fs::create_dir_all(&home) {
            run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
            return;
        }
        if let Err(e) = fs::create_dir_all(&workspace) {
            run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
            return;
        }
        let sources = Arc::clone(&self.sources);
        let source = spec.source.clone();
        let id = spec.id.clone();
        let base = workspace.clone();
        let stage_home = home.clone();
        let (workdir, reused) = match tokio::task::spawn_blocking(move || {
            sources.stage(&id, &source, &base, Some(&stage_home))
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
            Err(error) => {
                run.finish("JOB_END_ERROR", -1, &error.to_string()).await;
                return;
            }
        };
        let mut command = spec.command.clone();
        if !matches!(spec.source, Source::Git { .. }) && !Path::new(&command[0]).is_absolute() {
            let executable = if command[0].contains('/') {
                let resolved = fs::canonicalize(workdir.join(&command[0]));
                match resolved {
                    Ok(path) if path.starts_with(&workspace) => {
                        let executable = fs::metadata(&path).is_ok_and(|metadata| {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                        });
                        executable.then_some(path)
                    }
                    _ => None,
                }
            } else {
                system_executable(&command[0])
            };
            match executable {
                Some(path) => command[0] = path.to_string_lossy().into_owned(),
                None => {
                    run.finish(
                        "JOB_END_ERROR",
                        -1,
                        "command is not an accessible executable",
                    )
                    .await;
                    return;
                }
            }
        }
        if let Source::Git { commit, .. } = &spec.source {
            let script = "set -e; if [ ! -d \"$1/.git\" ]; then git -C \"$1\" init --quiet; fi; git -C \"$1\" fetch --quiet \"$2\" \"+$3:$3\"; git -C \"$1\" checkout --quiet --force --detach \"$4\"; shift 4; exec \"$@\"";
            let mut staged = vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "git-stage".into(),
                workdir.to_string_lossy().into_owned(),
                config
                    .policy
                    .user_home
                    .join("source.bundle")
                    .to_string_lossy()
                    .into_owned(),
                format!("refs/pc/{commit}"),
                commit.clone(),
            ];
            staged.extend(command);
            command = staged;
        }
        let req = Request {
            workload: Workload::SharedNodeJob,
            command,
            workspace: Some(workspace.clone()),
            managed_workspace_name: Some(managed_name),
            private_home: Some(home),
            grants: vec![],
            tool_roots: vec![],
            selected_executable: None,
            credential: None,
            overlays: vec![],
            masks: vec![],
            egress: config.egress.clone(),
            limits: Limits {
                cpu_percent: spec.cpu_percent,
                memory_bytes: spec.memory_bytes,
                tasks_max: 256,
            },
        };
        let plan = match config.policy.plan(req) {
            Ok(plan) => plan,
            Err(e) => {
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
                return;
            }
        };
        let mut options = config.options;
        options.working_directory = workdir.clone();
        options.environment.extend(spec.env.clone());
        options
            .environment
            .insert("PROTECTORATE_JOB_ID".into(), spec.id.clone());
        options.environment.insert(
            "PROTECTORATE_JOB_DIR".into(),
            options.working_directory.display().to_string(),
        );
        let mut handle = match tokio::task::spawn_blocking(move || {
            protectorate_sandbox::runtime::spawn(&plan, &options)
        })
        .await
        {
            Ok(Ok(handle)) => handle,
            Ok(Err(e)) => {
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
                return;
            }
            Err(e) => {
                run.finish("JOB_END_ERROR", -1, &e.to_string()).await;
                return;
            }
        };
        let kill = handle.kill_handle();
        let stdout = handle.take_stdout();
        let stderr = handle.take_stderr();
        run.started("native", reused).await;
        let stdout = stdout.map(|pipe| {
            tokio::task::spawn_blocking({
                let run = Arc::clone(run);
                move || read_log_blocking(pipe, run)
            })
        });
        let stderr = stderr.map(|pipe| {
            tokio::task::spawn_blocking({
                let run = Arc::clone(run);
                move || read_log_blocking(pipe, run)
            })
        });
        let mut wait = tokio::task::spawn_blocking(move || handle.wait());
        let (end, result) = tokio::select! {
            result = &mut wait => ("JOB_END_EXITED", result),
            _ = tokio::time::sleep(spec.timeout) => { kill.kill(); ("JOB_END_TIMED_OUT", wait.await) },
            _ = wait_cancel(run.cancel.subscribe()) => { kill.kill(); ("JOB_END_CANCELED", wait.await) },
        };
        if let Some(reader) = stdout {
            let _ = reader.await;
        }
        if let Some(reader) = stderr {
            let _ = reader.await;
        }
        match result {
            Ok(Ok(status)) => {
                let (end, code, error) = exit_outcome(end, &status);
                self.finish_with_artifacts(spec, &workdir, run, end, code, &error)
                    .await;
            }
            Ok(Err(e)) => run.finish("JOB_END_ERROR", -1, &e.to_string()).await,
            Err(e) => run.finish("JOB_END_ERROR", -1, &e.to_string()).await,
        }
    }
    async fn finish_with_artifacts(
        &self,
        spec: &Spec,
        workdir: &Path,
        run: &Arc<Run>,
        end: &str,
        code: i32,
        error: &str,
    ) {
        let mut result_error = error.to_string();
        if matches!(
            end,
            "JOB_END_EXITED" | "JOB_END_TIMED_OUT" | "JOB_END_KILLED"
        ) && !spec.artifact_globs.is_empty()
        {
            let _retention_guard = self.artifact_lock.lock().await;
            let root = self.root.clone();
            let used = tokio::task::spawn_blocking(move || retention::artifact_bytes(&root)).await;
            let remaining = match used {
                Ok(Ok(used)) => self.disk_limits.artifact_bytes.saturating_sub(used),
                Ok(Err(error)) => {
                    run.finish(end, code, &format!("count retained artifacts: {error}"))
                        .await;
                    return;
                }
                Err(error) => {
                    run.finish(end, code, &format!("count retained artifacts: {error}"))
                        .await;
                    return;
                }
            };
            let workdir = workdir.to_path_buf();
            let globs = spec.artifact_globs.clone();
            let destination = self.root.join("runs").join(&spec.id).join("artifacts");
            match tokio::task::spawn_blocking(move || {
                artifacts::collect(&workdir, &globs, &destination, remaining)
            })
            .await
            {
                Ok(Ok(files)) => run.state.lock().await.artifacts = files,
                Ok(Err(e)) => {
                    if result_error.is_empty() {
                        result_error = format!("collect artifacts: {e}");
                    }
                }
                Err(e) => {
                    if result_error.is_empty() {
                        result_error = format!("collect artifacts: {e}");
                    }
                }
            }
        }
        run.finish(end, code, &result_error).await;
    }
}
#[cfg(target_os = "linux")]
fn system_executable(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') {
        return None;
    }
    for dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
        let candidate = Path::new(dir).join(name);
        let Ok(real) = fs::canonicalize(candidate) else {
            continue;
        };
        if !["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .any(|root| real.starts_with(root))
        {
            continue;
        }
        let Ok(metadata) = fs::metadata(&real) else {
            continue;
        };
        use std::os::unix::fs::PermissionsExt;
        if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
            return Some(real);
        }
    }
    None
}
#[cfg(target_os = "linux")]
fn read_log_blocking<R: io::Read>(mut reader: R, run: Arc<Run>) {
    let handle = tokio::runtime::Handle::current();
    let mut buf = [0; CHUNK];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => handle.block_on(run.append(&buf[..n])),
        }
    }
}
async fn wait_cancel(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow_and_update() {
        if rx.changed().await.is_err() {
            break;
        }
    }
}
async fn read_log<R: tokio::io::AsyncRead + Unpin>(mut reader: R, run: Arc<Run>) {
    let mut buf = [0; CHUNK];
    while let Ok(n) = reader.read(&mut buf).await {
        if n == 0 {
            break;
        }
        run.append(&buf[..n]).await;
    }
}
impl Run {
    async fn started(&self, isolation: &str, reused: bool) {
        let mut state = self.state.lock().await;
        state.started = Some(json!({"workspaceReused":reused,"isolation":isolation}));
        state.started_at = Some(Instant::now());
        if let Some(dir) = &self.persist_dir {
            if let Err(error) = journal::write_record(dir, state.started.as_ref(), None, None) {
                eprintln!("protectorate node: persist job start: {error}");
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
    async fn append(&self, bytes: &[u8]) {
        self.append_with_limit(bytes, MAX_LOG).await;
    }
    async fn append_with_limit(&self, bytes: &[u8], limit: usize) {
        assert!(limit >= LOG_TRUNCATED.len());
        let mut state = self.state.lock().await;
        if state.log_truncated {
            return;
        }
        let remaining = (limit - LOG_TRUNCATED.len()).saturating_sub(state.log.len());
        let granted = reserve(&self.log_budget, bytes.len().min(remaining));
        state.log.extend_from_slice(&bytes[..granted]);
        state.budget_used += granted;
        let mut persisted = bytes[..granted].to_vec();
        if granted < bytes.len() {
            state.log.extend_from_slice(LOG_TRUNCATED);
            state.log_truncated = true;
            persisted.extend_from_slice(LOG_TRUNCATED);
        }
        if let Some(dir) = &self.persist_dir {
            if let Err(error) = journal::append_log(dir, &persisted) {
                eprintln!("protectorate node: persist job output: {error}");
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
    async fn finish(&self, end: &str, code: i32, error: &str) {
        let mut state = self.state.lock().await;
        let error = if state.log_truncated && error.is_empty() {
            "job output truncated at 64 MiB"
        } else {
            error
        };
        state.finished = Some(json!({"end":end,"exitCode":code,"error":error}));
        if let Some(started_at) = state.started_at {
            let elapsed = started_at.elapsed();
            state.finished.as_mut().unwrap()["runTime"] = json!(format!(
                "{}.{:09}s",
                elapsed.as_secs(),
                elapsed.subsec_nanos()
            ));
        }
        state.finished_at = Some(SystemTime::now());
        if let Some(dir) = &self.persist_dir {
            if let Err(error) = journal::replace_log(dir, &state.log).and_then(|_| {
                journal::write_record(
                    dir,
                    state.started.as_ref(),
                    state.finished.as_ref(),
                    state.finished_at,
                )
            }) {
                eprintln!("protectorate node: persist job finish: {error}");
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
}
fn reserve(budget: &AtomicUsize, want: usize) -> usize {
    loop {
        let used = budget.load(Ordering::Acquire);
        let amount = want.min(MAX_TOTAL_LOG.saturating_sub(used));
        if budget
            .compare_exchange(used, used + amount, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return amount;
        }
    }
}
fn reserve_exact(budget: &AtomicUsize, want: usize) -> bool {
    loop {
        let used = budget.load(Ordering::Acquire);
        if want > MAX_TOTAL_LOG.saturating_sub(used) {
            return false;
        }
        if budget
            .compare_exchange(used, used + want, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return true;
        }
    }
}
impl Spec {
    fn parse(v: &Value) -> Result<Self, RpcError> {
        let id = v.get("jobId").and_then(Value::as_str).unwrap_or_default();
        if !valid_id(id) {
            return Err(RpcError::invalid("invalid job id"));
        }
        let command = v
            .get("command")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::invalid("command is empty"))?
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| RpcError::invalid("command must contain strings"))?;
        if command.is_empty() || command[0].is_empty() || command.iter().any(|s| s.contains('\0')) {
            return Err(RpcError::invalid("command is empty or contains NUL"));
        }
        let source = Source::parse(v)?;
        let artifact_globs = v
            .get("artifactGlobs")
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .map(|value| value.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or(Some(vec![]))
            .ok_or_else(|| RpcError::invalid("artifact globs must be strings"))?;
        artifacts::validate(&artifact_globs).map_err(RpcError::invalid)?;
        let mode = match v
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("JOB_MODE_UNSPECIFIED")
        {
            "JOB_MODE_PLAIN" => Mode::Plain,
            "JOB_MODE_UNSPECIFIED" | "JOB_MODE_SANDBOXED" => Mode::Sandboxed,
            "JOB_MODE_ADMIN" => Mode::Admin,
            _ => return Err(RpcError::invalid("invalid job mode")),
        };
        let mut env = BTreeMap::new();
        if let Some(map) = v.get("env").and_then(Value::as_object) {
            for (key, value) in map {
                if key.is_empty()
                    || !key.bytes().enumerate().all(|(index, byte)| {
                        byte == b'_'
                            || byte.is_ascii_alphabetic()
                            || index > 0 && byte.is_ascii_digit()
                    })
                {
                    return Err(RpcError::invalid("invalid environment name"));
                }
                let value = value
                    .as_str()
                    .ok_or_else(|| RpcError::invalid("invalid environment value"))?;
                if value.contains('\0') {
                    return Err(RpcError::invalid("environment contains NUL"));
                }
                env.insert(key.clone(), value.to_string());
            }
        }
        let timeout = v
            .get("timeout")
            .and_then(Value::as_str)
            .and_then(parse_duration)
            .unwrap_or(Duration::from_secs(3600));
        let cpu_percent = v
            .get("limits")
            .and_then(|v| v.get("cpuPercent"))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0);
        let memory_bytes = v
            .get("limits")
            .and_then(|v| v.get("memoryBytes"))
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                v.get("limits")
                    .and_then(|v| v.get("memoryBytes"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0);
        Ok(Self {
            id: id.to_string(),
            command,
            env,
            mode,
            timeout,
            cpu_percent,
            memory_bytes,
            source,
            artifact_globs,
        })
    }
}
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.strip_suffix('s')?;
    let seconds: f64 = s.parse().ok()?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 86400.0 {
        return None;
    }
    Some(Duration::from_secs_f64(seconds))
}
fn internal(e: serde_json::Error) -> RpcError {
    RpcError::new(Code::Internal, e.to_string())
}
/// How a command that the node did not stop itself ended: a signal nobody on
/// the node sent is a kill (a resource limit or the OOM killer).
fn exit_outcome(
    end: &'static str,
    status: &std::process::ExitStatus,
) -> (&'static str, i32, String) {
    // The sandbox helper reports a signal that ends its workload as status 137.
    let signal = if end == "JOB_END_EXITED" {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(status);
        #[cfg(not(unix))]
        let signal = None;
        signal.or_else(|| (status.code() == Some(137)).then_some(9))
    } else {
        None
    };
    if let Some(signal) = signal {
        return ("JOB_END_KILLED", -1, format!("killed by signal {signal}"));
    }
    (end, status.code().unwrap_or(-1), String::new())
}

/// Spawns `command`, retrying while its executable is busy. A source staged
/// just before is written in this process, so a child that another job forks
/// meanwhile holds it open for writing until that child execs (ETXTBSY).
#[cfg(not(windows))]
async fn spawn_unless_busy(command: &mut Command) -> io::Result<tokio::process::Child> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match command.spawn() {
            Err(error)
                if error.kind() == io::ErrorKind::ExecutableFileBusy
                    && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            result => return result,
        }
    }
}

#[cfg(not(windows))]
fn kill_group(pid: u32) {
    if pid > 0 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

#[cfg(target_os = "linux")]
async fn run_linux_git_stage(
    config: &SandboxConfig,
    workspace: &Path,
    workdir: &Path,
    commit: &str,
    id: &str,
    timeout: Duration,
) -> io::Result<()> {
    use protectorate_sandbox::{Limits, Request, Workload};
    let digest = Sha256::digest(id.as_bytes());
    let name = format!("j_{digest:x}");
    let home = config
        .policy
        .state_root
        .join("jobs")
        .join(&name[..64])
        .join("home");
    let refname = format!("refs/pc/{commit}");
    let script = "set -e; if [ ! -d \"$1/.git\" ]; then git -C \"$1\" init --quiet; fi; git -C \"$1\" fetch --quiet \"$2\" \"+$3:$3\"; git -C \"$1\" checkout --quiet --force --detach \"$4\"";
    let plan = config
        .policy
        .plan(Request {
            workload: Workload::SharedNodeJob,
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "git-stage".into(),
                workdir.to_string_lossy().into_owned(),
                config
                    .policy
                    .user_home
                    .join("source.bundle")
                    .to_string_lossy()
                    .into_owned(),
                refname,
                commit.into(),
            ],
            workspace: Some(workspace.to_path_buf()),
            managed_workspace_name: workspace
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            private_home: Some(home),
            grants: vec![],
            tool_roots: vec![],
            selected_executable: None,
            credential: None,
            overlays: vec![],
            masks: vec![],
            egress: None,
            limits: Limits {
                cpu_percent: 0,
                memory_bytes: 0,
                tasks_max: 256,
            },
        })
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut options = config.options.clone();
    options.working_directory = workdir.to_path_buf();
    let mut handle =
        tokio::task::spawn_blocking(move || protectorate_sandbox::runtime::spawn(&plan, &options))
            .await
            .map_err(io::Error::other)?
            .map_err(|error| io::Error::other(error.to_string()))?;
    let kill = handle.kill_handle();
    let stdout = handle
        .take_stdout()
        .map(|mut pipe| tokio::task::spawn_blocking(move || io::copy(&mut pipe, &mut io::sink())));
    let stderr = handle
        .take_stderr()
        .map(|mut pipe| tokio::task::spawn_blocking(move || io::copy(&mut pipe, &mut io::sink())));
    let mut wait = tokio::task::spawn_blocking(move || handle.wait());
    let status = match tokio::time::timeout(timeout, &mut wait).await {
        Ok(result) => result
            .map_err(io::Error::other)?
            .map_err(|error| io::Error::other(error.to_string()))?,
        Err(_) => {
            kill.kill();
            let _ = wait.await;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "sandboxed Git staging timed out",
            ));
        }
    };
    if let Some(reader) = stdout {
        let _ = reader.await;
    }
    if let Some(reader) = stderr {
        let _ = reader.await;
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "sandboxed Git staging exited with {status}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
async fn bounded_wait(child: &mut tokio::process::Child) -> io::Result<std::process::ExitStatus> {
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.start_kill();
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "job process did not exit after termination",
            ))
        }
    }
}

#[cfg(windows)]
fn remove_windows_runner_directory(
    runner: &crate::windows_runner::Runner,
    path: &Path,
) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    if !path.starts_with(runner.root.join("workspaces")) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runner cache path is outside workspaces",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_attributes() & 0x400 != 0 => {
            return Err(io::Error::other("runner cache path is a reparse point"));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::other("runner cache path is not a directory"));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
        _ => {}
    }
    let literal = powershell_path(path)?;
    let script = format!(
        "$ErrorActionPreference='Stop'; if([IO.Directory]::Exists({literal})) {{ [IO.Directory]::Delete({literal}, $true) }}"
    );
    let command = vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ];
    let mut process = runner.spawn(&command, &BTreeMap::new(), &runner.root, "prune", 0, 0)?;
    drop(process.take_output());
    let code = process.wait_blocking()?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "runner cache deletion failed ({code})"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn reset_windows_runner_scratch(runner: &crate::windows_runner::Runner) -> io::Result<()> {
    let scratch = runner.root.join("jobs");
    let literal = powershell_path(&scratch)?;
    let script = format!(
        "$ErrorActionPreference='Stop'; if([IO.Directory]::Exists({literal})) {{ [IO.Directory]::Delete({literal}, $true) }}; [IO.Directory]::CreateDirectory({literal}) | Out-Null"
    );
    let command = vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ];
    let mut process = runner.spawn(&command, &BTreeMap::new(), &runner.root, "startup", 0, 0)?;
    drop(process.take_output());
    let code = process.wait_blocking()?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "runner scratch reset failed ({code})"
        )));
    }
    Ok(())
}

#[cfg(windows)]
async fn cleanup_windows_runner_workspace(
    runner: &Arc<crate::windows_runner::Runner>,
    base: &Path,
    id: &str,
) -> io::Result<()> {
    let literal = powershell_path(base)?;
    let script = format!(
        "$ErrorActionPreference='Stop'; if([IO.Directory]::Exists({literal})) {{ [IO.Directory]::Delete({literal}, $true) }}"
    );
    let command = vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ];
    run_windows_staging_command(runner, command, &runner.root, id, Duration::from_secs(30)).await
}

#[cfg(windows)]
async fn prepare_windows_runner_workspace(
    runner: &Arc<crate::windows_runner::Runner>,
    base: &Path,
    git: bool,
    id: &str,
    timeout: Duration,
) -> io::Result<()> {
    let mut script = format!(
        "$ErrorActionPreference='Stop'; [IO.Directory]::CreateDirectory({}) | Out-Null",
        powershell_path(base)?
    );
    if git {
        script.push_str(&format!(
            "; [IO.Directory]::CreateDirectory({}) | Out-Null",
            powershell_path(&base.join("work"))?
        ));
    }
    let command = vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ];
    run_windows_staging_command(runner, command, &runner.root, id, timeout).await
}

#[cfg(windows)]
async fn run_windows_uploaded_stage(
    runner: &Arc<crate::windows_runner::Runner>,
    sources: &Sources,
    source: &Source,
    workdir: &Path,
    id: &str,
    timeout: Duration,
) -> io::Result<()> {
    let upload = sources.upload_path(id);
    crate::windows_runner::grant_runner_read(&upload)
        .map_err(|error| io::Error::new(error.kind(), format!("grant upload read: {error}")))?;
    let command = match source {
        Source::File { name } => vec![
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            format!(
                "$ErrorActionPreference='Stop'; [IO.File]::Copy({}, {}, $true)",
                powershell_path(&upload)?,
                powershell_path(&workdir.join(name))?
            ),
        ],
        Source::Archive { workdir: relative } => {
            let mut root = workdir.to_path_buf();
            for _ in relative.components() {
                root.pop();
            }
            let script = sources.create_archive_script(id, WINDOWS_ARCHIVE_STAGE)?;
            vec![
                "powershell.exe".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                crate::windows_runner::process_path(&script)?
                    .to_string_lossy()
                    .into_owned(),
                crate::windows_runner::process_path(&upload)?
                    .to_string_lossy()
                    .into_owned(),
                crate::windows_runner::process_path(&root)?
                    .to_string_lossy()
                    .into_owned(),
            ]
        }
        _ => return Ok(()),
    };
    let path = if let Source::Archive { workdir: relative } = source {
        let mut root = workdir.to_path_buf();
        for _ in relative.components() {
            root.pop();
        }
        root.pop();
        root
    } else {
        workdir.to_path_buf()
    };
    run_windows_staging_command(runner, command, &path, id, timeout).await?;
    if let Source::Archive { .. } = source {
        let selected = fs::canonicalize(workdir)?;
        let mut root = workdir.to_path_buf();
        if let Source::Archive { workdir: relative } = source {
            for _ in relative.components() {
                root.pop();
            }
        }
        if !selected.is_dir() || !selected.starts_with(fs::canonicalize(root)?) {
            return Err(io::Error::other(
                "archive workdir is outside extracted source",
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn run_windows_staging_command(
    runner: &Arc<crate::windows_runner::Runner>,
    command: Vec<String>,
    path: &Path,
    id: &str,
    timeout: Duration,
) -> io::Result<()> {
    let runner = Arc::clone(runner);
    let path = path.to_path_buf();
    let job_id = id.to_owned();
    let mut process = tokio::task::spawn_blocking(move || {
        runner.spawn(&command, &BTreeMap::new(), &path, &job_id, 0, 0)
    })
    .await
    .map_err(io::Error::other)?
    .map_err(|error| io::Error::new(error.kind(), format!("spawn source staging: {error}")))?;
    let output = process
        .take_output()
        .ok_or_else(|| io::Error::other("runner source staging pipe missing"))?;
    let reader = tokio::spawn(async move {
        let mut file = tokio::fs::File::from_std(output);
        let mut chunk = [0u8; 4096];
        let mut first = Vec::new();
        loop {
            let count = file.read(&mut chunk).await?;
            if count == 0 {
                break;
            }
            let keep = (8192usize.saturating_sub(first.len())).min(count);
            first.extend_from_slice(&chunk[..keep]);
        }
        Ok::<_, io::Error>(first)
    });
    let code = match tokio::time::timeout(timeout, process.wait()).await {
        Ok(result) => result?,
        Err(_) => {
            let _ = process.terminate();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runner source staging timed out",
            ));
        }
    };
    drop(process);
    let output = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "runner source output did not close",
            )
        })?
        .map_err(io::Error::other)??;
    if code != 0 {
        return Err(io::Error::other(format!(
            "runner source staging failed ({code}): {}",
            String::from_utf8_lossy(&output).trim()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn powershell_path(path: &Path) -> io::Result<String> {
    let path = crate::windows_runner::process_path(path)?;
    Ok(format!("'{}'", path.to_string_lossy().replace('\'', "''")))
}

#[cfg(windows)]
const WINDOWS_ARCHIVE_STAGE: &str = r#"
$ErrorActionPreference='Stop';
$source=$args[0];
$dest=$args[1];
Add-Type -AssemblyName System.IO.Compression.FileSystem;
$archive=[IO.Compression.ZipFile]::OpenRead($source);
try {
    if($archive.Entries.Count -gt 200000) { throw 'archive has too many entries' }
    $root=[IO.Path]::GetFullPath($dest);
    [IO.Directory]::CreateDirectory($root) | Out-Null;
    $prefix=$root.TrimEnd([char]'\') + '\';
    [long]$total=0;
    foreach($entry in $archive.Entries) {
        $name=$entry.FullName;
        if([string]::IsNullOrEmpty($name) -or $name.StartsWith('/') -or $name.IndexOfAny([char[]]@('\',':')) -ge 0) { throw 'unsafe archive path' }
        $parts=$name.TrimEnd('/').Split('/');
        foreach($part in $parts) { if($part -eq '' -or $part -eq '.' -or $part -eq '..') { throw 'unsafe archive path' } }
        $mode=($entry.ExternalAttributes -shr 16) -band 65535;
        if(($mode -band 61440) -eq 40960) { throw 'archive symlink is forbidden' }
        $total += $entry.Length;
        if($total -gt 4294967296) { throw 'archive is too large' }
        $target=[IO.Path]::GetFullPath([IO.Path]::Combine($root, $name.Replace('/', '\')));
        if(!$target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'unsafe archive path' }
        if($name.EndsWith('/')) {
            [IO.Directory]::CreateDirectory($target) | Out-Null;
        } else {
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null;
            $input=$entry.Open();
            try {
                $output=[IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None);
                try {
                    [long]$written=0;
                    $buffer=New-Object byte[] 65536;
                    while(($count=$input.Read($buffer, 0, $buffer.Length)) -gt 0) {
                        $written += $count;
                        if($written -gt $entry.Length) { throw 'archive entry size changed' }
                        $output.Write($buffer, 0, $count);
                    }
                    if($written -ne $entry.Length) { throw 'archive entry size changed' }
                } finally { $output.Dispose() }
            } finally { $input.Dispose() }
        }
    }
} finally { $archive.Dispose() }
"#;

#[cfg(windows)]
async fn run_windows_git_stage(
    runner: &Arc<crate::windows_runner::Runner>,
    workdir: &Path,
    bundle: &Path,
    commit: &str,
    id: &str,
    timeout: Duration,
) -> io::Result<()> {
    let git = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|path| path.join("git.exe"))
        .find(|path| path.is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Git executable is unavailable"))?;
    let git = crate::windows_runner::process_path(&git)?
        .to_string_lossy()
        .into_owned();
    let work = crate::windows_runner::process_path(workdir)?
        .to_string_lossy()
        .into_owned();
    let source = crate::windows_runner::process_path(bundle)?
        .to_string_lossy()
        .into_owned();
    let safe_directory = format!("safe.directory={}", work.replace('\\', "/"));
    let refname = format!("refs/pc/{commit}");
    let mut commands = Vec::new();
    if !workdir.join(".git").is_dir() {
        commands.push(vec![
            git.clone(),
            "-c".into(),
            safe_directory.clone(),
            "-C".into(),
            work.clone(),
            "init".into(),
            "--quiet".into(),
        ]);
    }
    commands.push(vec![
        git.clone(),
        "-c".into(),
        safe_directory.clone(),
        "-C".into(),
        work.clone(),
        "fetch".into(),
        "--quiet".into(),
        source,
        format!("+{refname}:{refname}"),
    ]);
    commands.push(vec![
        git,
        "-c".into(),
        safe_directory,
        "-C".into(),
        work,
        "checkout".into(),
        "--quiet".into(),
        "--force".into(),
        "--detach".into(),
        commit.into(),
    ]);
    for command in commands {
        let runner = Arc::clone(runner);
        let path = workdir.to_path_buf();
        let job_id = id.to_owned();
        let command_name = command[5].clone();
        let mut process = tokio::task::spawn_blocking(move || {
            runner.spawn(&command, &BTreeMap::new(), &path, &job_id, 0, 0)
        })
        .await
        .map_err(io::Error::other)??;
        let output = process
            .take_output()
            .ok_or_else(|| io::Error::other("runner Git pipe missing"))?;
        let reader = tokio::spawn(async move {
            let mut file = tokio::fs::File::from_std(output);
            let mut chunk = [0u8; 4096];
            let mut first = Vec::new();
            loop {
                let count = file.read(&mut chunk).await?;
                if count == 0 {
                    break;
                }
                let keep = (8192usize.saturating_sub(first.len())).min(count);
                first.extend_from_slice(&chunk[..keep]);
            }
            Ok::<_, io::Error>(first)
        });
        let code = match tokio::time::timeout(timeout, process.wait()).await {
            Ok(result) => result?,
            Err(_) => {
                let _ = process.terminate();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("runner Git {command_name} timed out"),
                ));
            }
        };
        drop(process);
        let output = tokio::time::timeout(Duration::from_secs(5), reader)
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "runner Git output did not close")
            })?
            .map_err(io::Error::other)??;
        if code != 0 {
            return Err(io::Error::other(format!(
                "runner Git {command_name} failed ({code}): {}",
                String::from_utf8_lossy(&output).trim()
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn bounded_runner_wait(process: &crate::windows_runner::RunnerProcess) -> io::Result<i32> {
    tokio::time::timeout(Duration::from_secs(5), process.wait())
        .await
        .unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runner process did not exit after termination",
            ))
        })
}
