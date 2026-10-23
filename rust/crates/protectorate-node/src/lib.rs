//! Authenticated node metrics, file transfer and job RPCs.
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
use axum::{
    http::{header, HeaderMap},
    Router,
};
use files::Files;
pub use jobs::Jobs;
use jobs::Jobs as JobStore;
#[cfg(target_os = "linux")]
pub use jobs::SandboxConfig;
use protectorate_node_metrics::{Collector, NodeMetrics, Probe};
use protectorate_wire::{
    self as wire, AuthContext, Catalog, Code, Method, RpcError, RpcRequest, RpcResponse,
    UnaryServer,
};
use protectorate_xfer::Kind;
pub use retention::DiskLimits;
use serde_json::{json, Value};
use std::{io, path::Path, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
pub use uninstall::{Plan as UninstallPlan, Stop as UninstallStop};
pub use update::Restart as UpdateRestart;
pub use update::{
    guardian_confirm_ready, guardian_restart_target, guardian_update_state, recover_failed_trial,
};
#[cfg(windows)]
pub use windows_runner::install_windows_runner;
use zeroize::Zeroizing;

pub trait MetricsCollector: Send + Sync + 'static {
    fn collect(&self) -> io::Result<NodeMetrics>;
}
impl<P: Probe> MetricsCollector for Collector<P> {
    fn collect(&self) -> io::Result<NodeMetrics> {
        Collector::collect(self)
    }
}

#[derive(Clone)]
pub struct Agent {
    collector: Arc<dyn MetricsCollector>,
    token: Arc<Zeroizing<String>>,
    files: Option<Arc<Files>>,
    jobs: Option<Arc<JobStore>>,
    updater: Option<Arc<update::Updater>>,
    uninstaller: Option<Arc<uninstall::Uninstaller>>,
    desk: Option<Arc<protectorate_desk_node::Controller>>,
}
impl Agent {
    pub fn new(collector: Arc<dyn MetricsCollector>, token: impl Into<String>) -> Self {
        Self {
            collector,
            token: Arc::new(Zeroizing::new(token.into())),
            files: None,
            jobs: None,
            updater: None,
            uninstaller: None,
            desk: None,
        }
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
    /// Loads the node's persisted Windows desktop policy and native driver.
    #[allow(unused_mut)] // Windows installs the controller; other platforms retain Unimplemented.
    pub fn with_desk_home(mut self, home: impl AsRef<Path>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let controller = Arc::new(protectorate_desk_node::Controller::open(
                home.as_ref().to_path_buf(),
            )?);
            controller.start_background();
            self.desk = Some(controller);
        }
        #[cfg(not(windows))]
        let _ = home;
        Ok(self)
    }

    /// Test/integration seam for a desktop whose input and availability can be controlled.
    pub fn with_desktop(mut self, controller: Arc<protectorate_desk_node::Controller>) -> Self {
        self.desk = Some(controller);
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
    pub fn router(self) -> wire::Result<Router> {
        self.build_router(false)
    }

    /// Serve only behind a transport that checks the authenticated tailnet
    /// peer ID and current `tag:hub` membership for every HTTP request.
    pub fn router_trusted_tailnet(self) -> wire::Result<Router> {
        self.build_router(true)
    }

    fn build_router(self, trusted_tailnet: bool) -> wire::Result<Router> {
        let catalog = Catalog::embedded()?;
        let secret = Arc::clone(&self.token);
        let authorize = Arc::new(move |method: &Method, headers: &HeaderMap| {
            if !method.node_local {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "node-local procedure required",
                ));
            }
            if trusted_tailnet {
                return Ok(AuthContext {
                    actor: "hub".into(),
                    ..AuthContext::default()
                });
            }
            let got = headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .ok_or_else(|| {
                    RpcError::new(Code::Unauthenticated, "missing or invalid bearer token")
                })?;
            if secret.is_empty() || !bool::from(secret.as_bytes().ct_eq(got.as_bytes())) {
                return Err(RpcError::new(
                    Code::Unauthenticated,
                    "missing or invalid bearer token",
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
        server.register(
            "/protectorate.v1.NodeAgentService/PrepareJob",
            move |request| {
                let jobs = jobs.clone();
                async move { jobs.ok_or_else(no_jobs)?.prepare(&request).await }
            },
        )?;
        let jobs = agent.jobs.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/UploadJobData",
            move |request| {
                let jobs = jobs.clone();
                async move { jobs.ok_or_else(no_jobs)?.upload(&request).await }
            },
        )?;
        let jobs = agent.jobs.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/ListJobs",
            move |request| {
                let jobs = jobs.clone();
                async move { jobs.ok_or_else(no_jobs)?.list(&request).await }
            },
        )?;
        let jobs = agent.jobs.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/CancelJob",
            move |request| {
                let jobs = jobs.clone();
                async move { jobs.ok_or_else(no_jobs)?.cancel(&request).await }
            },
        )?;
        let jobs = agent.jobs.clone();
        server.register_streaming(
            "/protectorate.v1.NodeAgentService/RunJob",
            move |request, sender| {
                let jobs = jobs.clone();
                async move {
                    let jobs = jobs.ok_or_else(no_jobs)?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let id = jobs.start(input).await?;
                    jobs.attach(&request, sender, &id, 0).await?;
                    Ok(Default::default())
                }
            },
        )?;
        let jobs = agent.jobs.clone();
        server.register_streaming(
            "/protectorate.v1.NodeAgentService/AttachJob",
            move |request, sender| {
                let jobs = jobs.clone();
                async move {
                    let jobs = jobs.ok_or_else(no_jobs)?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let id = input
                        .get("jobId")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let offset = input
                        .get("fromOffset")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse::<usize>().ok())
                        .or_else(|| {
                            input
                                .get("fromOffset")
                                .and_then(Value::as_u64)
                                .and_then(|n| usize::try_from(n).ok())
                        })
                        .unwrap_or(0);
                    jobs.attach(&request, sender, id, offset).await?;
                    Ok(Default::default())
                }
            },
        )?;
        let get = Arc::clone(&agent);
        server.register(
            "/protectorate.v1.NodeAgentService/GetMetrics",
            move |request| {
                let collector = Arc::clone(&get.collector);
                let updater = get.updater.clone();
                let jobs = get.jobs.clone();
                async move {
                    let mut metrics = tokio::task::spawn_blocking(move || collector.collect())
                        .await
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                        .map_err(|e| {
                            RpcError::new(Code::Unavailable, format!("collect metrics: {e}"))
                        })?;
                    if let Some(jobs) = jobs {
                        metrics.jobs = Some(jobs.isolation());
                    }
                    let mut response = metrics_json(&metrics);
                    if let Some(updater) = updater {
                        let reported = metrics.agent_version.clone();
                        let touch = Arc::clone(&updater);
                        tokio::task::spawn_blocking(move || touch.touch(&reported))
                            .await
                            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                            .map_err(|e| {
                                RpcError::new(Code::Internal, format!("confirm node update: {e}"))
                            })?;
                        if let Some(state) = tokio::task::spawn_blocking(move || updater.state())
                            .await
                            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                            .map_err(|e| {
                                RpcError::new(Code::Internal, format!("read node update: {e}"))
                            })?
                        {
                            response["update"] = state;
                        }
                    }
                    respond(&request, response)
                }
            },
        )?;
        let updater = agent.updater.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/ApplyUpdate",
            move |request| {
                let updater = updater.clone();
                async move {
                    let updater = updater.ok_or_else(|| {
                        RpcError::new(Code::Unimplemented, "this node does not accept updates")
                    })?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let result = tokio::task::spawn_blocking(move || updater.apply(&input))
                        .await
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                    respond(&request, result)
                }
            },
        )?;
        let uninstaller = agent.uninstaller.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/Uninstall",
            move |request| {
                let uninstaller = uninstaller.clone();
                async move {
                    let uninstaller = uninstaller.ok_or_else(|| {
                        RpcError::new(
                            Code::Unimplemented,
                            "node self-uninstall is unavailable on this platform",
                        )
                    })?;
                    respond(&request, uninstaller.schedule())
                }
            },
        )?;
        let files = agent.files.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/PutFile",
            move |request| {
                let files = files.clone();
                async move {
                    let files = files.ok_or_else(|| {
                        RpcError::new(
                            Code::Unimplemented,
                            "this node does not serve file transfers",
                        )
                    })?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let value = tokio::task::spawn_blocking(move || files.put(&input))
                        .await
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                    respond(&request, value)
                }
            },
        )?;
        let files = agent.files.clone();
        server.register_streaming("/protectorate.v1.NodeAgentService/GetFile", move |request, sender| {
            let files = files.clone();
            async move {
                let files = files.ok_or_else(|| RpcError::new(Code::Unimplemented, "this node does not serve file transfers"))?;
                let input = serde_json::to_value(&request.message).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                let path = input.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
                let compress = !input.get("noCompress").and_then(Value::as_bool).unwrap_or(false);
                let (resolved, mut source) = tokio::task::spawn_blocking(move || files.open(&path, compress)).await
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))??;
                let header = source.header();
                let kind = if header.kind == Kind::File { "FILE_KIND_FILE" } else { "FILE_KIND_DIRECTORY" };
                let header_value = json!({"header": {"kind": kind, "mode": header.mode, "size": header.size.to_string(), "path": resolved.to_string_lossy()}});
                sender.send(decode(&request, header_value)?).await?;
                loop {
                    if sender.is_cancelled() { return Err(RpcError::new(Code::Canceled, "stream cancelled")); }
                    let (next, chunk) = tokio::task::spawn_blocking(move || {
                        let result = source.next_chunk(); (source, result)
                    }).await.map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    source = next;
                    let chunk = chunk.map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let Some(chunk) = chunk else { break };
                    let value = json!({"chunk": {"data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &chunk.data), "compressed": chunk.compressed, "rawSize": chunk.raw_size}});
                    sender.send(decode(&request, value)?).await?;
                }
                let trailer = tokio::task::spawn_blocking(move || source.trailer()).await
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                    .map_err(|e| RpcError::new(Code::Aborted, e.to_string()))?;
                sender.send(decode(&request, json!({"trailer": {"size": trailer.size.to_string(), "sha256": trailer.sha256, "skipped": trailer.skipped}}))?).await?;
                Ok(Default::default())
            }
        })?;
        let desk = agent.desk.clone();
        server.register_streaming("/protectorate.v1.NodeAgentService/WatchMetrics", move |request, sender| {
            let collector = Arc::clone(&agent.collector);
            async move {
                let input = serde_json::to_value(&request.message).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                let seconds = input.get("intervalSeconds").and_then(Value::as_i64).unwrap_or_default();
                let seconds = if seconds <= 0 { 10 } else { seconds.max(5) } as u64;
                let mut interval = tokio::time::interval(Duration::from_secs(seconds));
                loop {
                    tokio::select! {
                        _ = sender.cancelled() => break,
                        _ = interval.tick() => {
                            let collector = Arc::clone(&collector);
                            let metrics = tokio::task::spawn_blocking(move || collector.collect()).await
                                .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                                .map_err(|e| RpcError::new(Code::Unavailable, format!("collect metrics: {e}")))?;
                            let message = decode(&request, metrics_json(&metrics))?;
                            sender.send(message).await?;
                        }
                    }
                }
                Ok(Default::default())
            }
        })?;
        let desk_action = desk.clone();
        server.register(
            "/protectorate.v1.NodeAgentService/DeskAction",
            move |request| {
                let desk = desk_action.clone();
                async move {
                    let desk = desk.ok_or_else(no_desk)?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let result = tokio::task::spawn_blocking(move || desk.action(&input))
                        .await
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                        .map_err(desk_error)?;
                    respond(&request, result)
                }
            },
        )?;
        server.register(
            "/protectorate.v1.NodeAgentService/GetDeskState",
            move |request| {
                let desk = desk.clone();
                async move {
                    let desk = desk.ok_or_else(no_desk)?;
                    let input = serde_json::to_value(&request.message)
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                    let result = tokio::task::spawn_blocking(move || desk.get_state(&input))
                        .await
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                        .map_err(desk_error)?;
                    respond(&request, result)
                }
            },
        )?;
        Ok(server.router())
    }
}
fn no_jobs() -> RpcError {
    RpcError::new(Code::Unimplemented, "this node does not run jobs")
}
fn no_desk() -> RpcError {
    RpcError::new(
        Code::Unimplemented,
        "this node does not serve desktop commands",
    )
}
fn desk_error(error: protectorate_desk_node::Error) -> RpcError {
    use protectorate_desk_node::ErrorKind;
    let code = match error.kind {
        ErrorKind::Invalid => Code::InvalidArgument,
        ErrorKind::Unavailable => Code::FailedPrecondition,
        ErrorKind::Unsupported => Code::Unimplemented,
        ErrorKind::Internal => Code::Internal,
    };
    let mut rpc = RpcError::new(code, error.message);
    if let Some(reason) = error.reason {
        let name = protectorate_desk_node::reason_name(reason);
        if let Ok(value) = name.parse() {
            rpc.headers.insert("Protectorate-Desk-Unavailable", value);
        }
    }
    rpc
}

fn decode(request: &RpcRequest, value: Value) -> wire::Result<wire::DynamicMessage> {
    let bytes =
        serde_json::to_vec(&value).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    wire::decode_json(request.method.output(), &bytes)
}
fn respond(request: &RpcRequest, value: Value) -> wire::Result<RpcResponse> {
    Ok(decode(request, value)?.into())
}
pub use protectorate_node_metrics::metrics_json;
