//! Node metrics, file transfer and job RPCs. The listener authenticates the
//! hub with mutual TLS before any request reaches this router.
mod admin;
mod artifacts;
mod files;
mod jobs;
mod journal;
mod retention;
mod sources;
mod uninstall;
mod update;
#[cfg(windows)]
mod windows_process;
#[cfg(windows)]
mod windows_runner;
use axum::{http::HeaderMap, Router};
use files::Files;
pub use jobs::Jobs;
use jobs::Jobs as JobStore;
#[cfg(target_os = "linux")]
pub use jobs::SandboxConfig;
use protectorate_node_metrics::{metrics_proto, Collector, NodeMetrics, Probe};
use protectorate_wire::{
    self as wire,
    v1::{self, get_file_response::Part, node_agent_service},
    AuthContext, Call, Catalog, Code, Method, RpcError, UnaryServer,
};
use protectorate_xfer::Kind;
pub use retention::DiskLimits;
use std::{
    io,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use uninstall::{Plan as UninstallPlan, Stop as UninstallStop};
pub use update::Restart as UpdateRestart;
pub use update::{
    guardian_confirm_ready, guardian_restart_target, guardian_update_state, recover_failed_trial,
};
#[cfg(windows)]
pub use windows_runner::install_windows_runner;

/// Checks and serves a renewed node certificate (PEM) from the next TLS
/// handshake on.
pub type CertRenewal = Arc<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

pub trait MetricsCollector: Send + Sync + 'static {
    fn collect(&self) -> io::Result<NodeMetrics>;
}
impl<P: Probe> MetricsCollector for Collector<P> {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Collector::collect(self)
    }
}

/// Numbers the metrics this agent process sends: `instance` names the
/// process and `seq` counts within it, so a receiver can drop a late sample
/// and still accept the first one after a restart.
struct Beat {
    instance: String,
    seq: AtomicU64,
}
impl Beat {
    fn stamp(&self, metrics: &mut v1::NodeMetrics) {
        metrics.instance = self.instance.clone();
        metrics.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
    }
}

#[derive(Clone)]
pub struct Agent {
    beat: Arc<Beat>,
    collector: Arc<dyn MetricsCollector>,
    renew: Option<CertRenewal>,
    files: Option<Arc<Files>>,
    jobs: Option<Arc<JobStore>>,
    updater: Option<Arc<update::Updater>>,
    uninstaller: Option<Arc<uninstall::Uninstaller>>,
}
impl Agent {
    pub fn new(collector: Arc<dyn MetricsCollector>) -> Self {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        Self {
            beat: Arc::new(Beat {
                instance: format!("{started:x}"),
                seq: AtomicU64::new(0),
            }),
            collector,
            renew: None,
            files: None,
            jobs: None,
            updater: None,
            uninstaller: None,
        }
    }
    pub fn with_certificate_renewal(mut self, renew: CertRenewal) -> Self {
        self.renew = Some(renew);
        self
    }
    /// Enables file RPCs. Relative remote paths resolve under `root`.
    pub fn with_files(mut self, root: impl AsRef<Path>, unconfined: bool) -> io::Result<Self> {
        self.files = Some(Arc::new(Files::new(root, unconfined)?));
        Ok(self)
    }
    /// Enables bounded restart recovery for file uploads and readable
    /// directory uploads. The receipt store lives under the private node home.
    pub fn with_files_durable(
        mut self,
        root: impl AsRef<Path>,
        unconfined: bool,
        home: impl AsRef<Path>,
    ) -> io::Result<Self> {
        self.files = Some(Arc::new(Files::new_with_receipts(root, unconfined, home)?));
        Ok(self)
    }
    pub fn with_jobs(mut self, jobs: Jobs) -> Self {
        self.jobs = Some(Arc::new(jobs));
        self
    }
    pub fn with_update(
        mut self,
        exe: impl AsRef<Path>,
        version: impl Into<String>,
        restart: UpdateRestart,
    ) -> io::Result<Self> {
        let files = self.files.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "node updates require file transfer root",
            )
        })?;
        self.updater = Some(Arc::new(update::Updater::new(
            Arc::clone(files),
            exe.as_ref().to_path_buf(),
            version.into(),
            restart,
        )?));
        Ok(self)
    }
    pub fn with_uninstall(
        mut self,
        plan: UninstallPlan,
        systemctl: impl Into<std::path::PathBuf>,
        stop: UninstallStop,
    ) -> Self {
        self.uninstaller = Some(Arc::new(uninstall::Uninstaller::new(
            plan,
            systemctl.into(),
            stop,
        )));
        self
    }
    /// Must run before the listener starts. True means a previous trial was
    /// rolled back and this process should restart on its restored binary.
    pub fn start_update_trial(&self) -> io::Result<bool> {
        self.updater
            .as_ref()
            .map_or(Ok(false), |updater| updater.start())
    }
    /// The job table, for the server's hang watchdog to list like a poll.
    pub fn jobs(&self) -> Option<Arc<Jobs>> {
        self.jobs.clone()
    }
    pub fn router(self) -> wire::Result<Router> {
        let catalog = Catalog::embedded()?;
        let authorize = Arc::new(|method: &Method, _: &HeaderMap| {
            if !method.node_local {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "node-local procedure required",
                ));
            }
            Ok(AuthContext {
                actor: "hub".into(),
                ..AuthContext::default()
            })
        });
        // A raw 4 MiB chunk becomes about 5.6 MiB in protobuf JSON base64.
        let mut server = UnaryServer::new(catalog, authorize)
            .with_max_request_bytes(8 << 20)
            .with_max_stream_message_bytes(8 << 20);
        let agent = Arc::new(self);
        let jobs = agent.jobs.clone();
        server.unary(node_agent_service::PREPARE_JOB, move |call: Call<_>| {
            let jobs = jobs.clone();
            async move { jobs.ok_or_else(no_jobs)?.prepare(call.request).await }
        })?;
        let jobs = agent.jobs.clone();
        server.unary(node_agent_service::UPLOAD_JOB_DATA, move |call: Call<_>| {
            let jobs = jobs.clone();
            async move { jobs.ok_or_else(no_jobs)?.upload(call.request).await }
        })?;
        let jobs = agent.jobs.clone();
        server.unary(node_agent_service::LIST_JOBS, move |_: Call<_>| {
            let jobs = jobs.clone();
            async move { Ok(jobs.ok_or_else(no_jobs)?.list().await) }
        })?;
        let jobs = agent.jobs.clone();
        server.unary(node_agent_service::CANCEL_JOB, move |call: Call<_>| {
            let jobs = jobs.clone();
            async move { Ok(jobs.ok_or_else(no_jobs)?.cancel(&call.request.job_id).await) }
        })?;
        let jobs = agent.jobs.clone();
        server.stream(node_agent_service::RUN_JOB, move |call: Call<_>, sender| {
            let jobs = jobs.clone();
            async move {
                let jobs = jobs.ok_or_else(no_jobs)?;
                let id = jobs.start(&call.request).await?;
                jobs.attach(sender, &id, 0).await?;
                Ok(Default::default())
            }
        })?;
        let jobs = agent.jobs.clone();
        server.stream(
            node_agent_service::ATTACH_JOB,
            move |call: Call<_>, sender| {
                let jobs = jobs.clone();
                async move {
                    let jobs = jobs.ok_or_else(no_jobs)?;
                    let offset = usize::try_from(call.request.from_offset).unwrap_or(usize::MAX);
                    jobs.attach(sender, &call.request.job_id, offset).await?;
                    Ok(Default::default())
                }
            },
        )?;
        let get = Arc::clone(&agent);
        server.unary(node_agent_service::GET_METRICS, move |_: Call<_>| {
            let collector = Arc::clone(&get.collector);
            let beat = Arc::clone(&get.beat);
            let updater = get.updater.clone();
            let jobs = get.jobs.clone();
            async move {
                let mut metrics = collect(collector).await?;
                if let Some(jobs) = jobs {
                    metrics.jobs = Some(jobs.isolation());
                }
                let mut response = metrics_proto(&metrics);
                beat.stamp(&mut response);
                if let Some(updater) = updater {
                    let reported = metrics.agent_version;
                    response.update = tokio::task::spawn_blocking(move || {
                        updater
                            .touch(&reported)
                            .map_err(|e| format!("confirm node update: {e}"))?;
                        updater
                            .state()
                            .map_err(|e| format!("read node update: {e}"))
                    })
                    .await
                    .map_err(join_error)?
                    .map_err(|e| RpcError::new(Code::Internal, e))?;
                }
                Ok(response)
            }
        })?;
        let updater = agent.updater.clone();
        server.unary(node_agent_service::APPLY_UPDATE, move |call: Call<_>| {
            let updater = updater.clone();
            async move {
                let updater = updater.ok_or_else(|| {
                    RpcError::new(Code::Unimplemented, "this node does not accept updates")
                })?;
                tokio::task::spawn_blocking(move || updater.apply(&call.request))
                    .await
                    .map_err(join_error)?
            }
        })?;
        let renew = agent.renew.clone();
        server.unary(
            node_agent_service::RENEW_CERTIFICATE,
            move |call: Call<_>| {
                let renew = renew.clone();
                async move {
                    let renew = renew.ok_or_else(|| {
                        RpcError::new(Code::Unimplemented, "this agent serves no certificate")
                    })?;
                    renew(&call.request.certificate)
                        .map_err(|e| RpcError::invalid(format!("certificate: {e}")))?;
                    Ok(v1::RenewCertificateResponse {})
                }
            },
        )?;
        let uninstaller = agent.uninstaller.clone();
        server.unary(node_agent_service::UNINSTALL, move |_: Call<_>| {
            let uninstaller = uninstaller.clone();
            async move {
                let uninstaller = uninstaller.ok_or_else(|| {
                    RpcError::new(
                        Code::Unimplemented,
                        "node self-uninstall is unavailable on this platform",
                    )
                })?;
                Ok(uninstaller.schedule())
            }
        })?;
        let files = agent.files.clone();
        server.unary(node_agent_service::PUT_FILE, move |call: Call<_>| {
            let files = files.clone();
            async move {
                let files = files.ok_or_else(no_files)?;
                tokio::task::spawn_blocking(move || files.put(call.request))
                    .await
                    .map_err(join_error)?
            }
        })?;
        let files = agent.files.clone();
        server.stream(
            node_agent_service::GET_FILE,
            move |call: Call<_>, sender| {
                let files = files.clone();
                async move {
                    let files = files.ok_or_else(no_files)?;
                    let request: v1::GetFileRequest = call.request;
                    let compress = !request.no_compress;
                    let (resolved, mut source) =
                        tokio::task::spawn_blocking(move || files.open(&request.path, compress))
                            .await
                            .map_err(join_error)??;
                    let header = source.header();
                    let kind = match header.kind {
                        Kind::File => v1::FileKind::File,
                        Kind::Directory => v1::FileKind::Directory,
                    };
                    sender
                        .send(&file_part(Part::Header(v1::FileHeader {
                            kind: kind as i32,
                            mode: header.mode,
                            size: header.size,
                            path: resolved.to_string_lossy().into_owned(),
                        })))
                        .await?;
                    loop {
                        if sender.is_cancelled() {
                            return Err(RpcError::new(Code::Canceled, "stream cancelled"));
                        }
                        let (next, chunk) = tokio::task::spawn_blocking(move || {
                            let result = source.next_chunk();
                            (source, result)
                        })
                        .await
                        .map_err(join_error)?;
                        source = next;
                        let chunk = chunk.map_err(RpcError::internal)?;
                        let Some(chunk) = chunk else { break };
                        sender
                            .send(&file_part(Part::Chunk(v1::FileChunk {
                                data: chunk.data,
                                compressed: chunk.compressed,
                                raw_size: chunk.raw_size,
                            })))
                            .await?;
                    }
                    let trailer = tokio::task::spawn_blocking(move || source.trailer())
                        .await
                        .map_err(join_error)?
                        .map_err(|e| RpcError::new(Code::Aborted, e.to_string()))?;
                    sender
                        .send(&file_part(Part::Trailer(v1::FileTrailer {
                            size: trailer.size,
                            sha256: trailer.sha256,
                            skipped: trailer.skipped,
                        })))
                        .await?;
                    Ok(Default::default())
                }
            },
        )?;
        server.stream(
            node_agent_service::WATCH_METRICS,
            move |call: Call<_>, sender| {
                let collector = Arc::clone(&agent.collector);
                let beat = Arc::clone(&agent.beat);
                async move {
                    let seconds = match call.request.interval_seconds {
                        seconds if seconds <= 0 => 10,
                        seconds => seconds.max(5) as u64,
                    };
                    let mut interval = tokio::time::interval(Duration::from_secs(seconds));
                    loop {
                        tokio::select! {
                            _ = sender.cancelled() => break,
                            _ = interval.tick() => {
                                let mut metrics = metrics_proto(&collect(Arc::clone(&collector)).await?);
                                beat.stamp(&mut metrics);
                                sender.send(&metrics).await?;
                            }
                        }
                    }
                    Ok(Default::default())
                }
            },
        )?;
        Ok(server.router())
    }
}
async fn collect(collector: Arc<dyn MetricsCollector>) -> wire::Result<NodeMetrics> {
    tokio::task::spawn_blocking(move || collector.collect())
        .await
        .map_err(join_error)?
        .map_err(|e| RpcError::new(Code::Unavailable, format!("collect metrics: {e}")))
}
fn file_part(part: Part) -> v1::GetFileResponse {
    v1::GetFileResponse { part: Some(part) }
}
fn join_error(error: tokio::task::JoinError) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn no_jobs() -> RpcError {
    RpcError::new(Code::Unimplemented, "this node does not run jobs")
}
fn no_files() -> RpcError {
    RpcError::new(
        Code::Unimplemented,
        "this node does not serve file transfers",
    )
}
