//! Node binary push, verified apply and restart observation.
use super::Hub;
use protectorate_node_client::push_with_progress;
use protectorate_wire::{
    self as wire,
    v1::{self, node_agent_service, node_service, UpdatePhase},
    Call, Code, Result, RpcError, StreamSender, UnaryServer,
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const STAGE: &str = "updates/protectorate-rs-next";
const STAGE_BUNDLE: &str = "updates/protectorate-rs-bundle-next";

struct Payload {
    binary: PathBuf,
    transfer: PathBuf,
    bundle: bool,
    _staging: Option<tempfile::TempDir>,
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let apply = Arc::clone(&hub);
    server.unary(node_service::APPLY_UPDATE, move |call: Call<_>| {
        let hub = Arc::clone(&apply);
        async move {
            let name = node_name(&call.request.name)?;
            remote_node(&hub, &name)?;
            let client =
                hub.node_files
                    .client_with_timeout(&call.auth, &name, Duration::from_secs(60))?;
            tokio::task::spawn_blocking(move || {
                client.unary(node_agent_service::APPLY_UPDATE, &call.request)
            })
            .await
            .map_err(super::join_error)?
        }
    })?;
    server.stream(node_service::UPDATE_NODE, move |call: Call<_>, sender| {
        let hub = Arc::clone(&hub);
        async move { update_node(hub, call, sender).await }
    })?;
    Ok(())
}

async fn update_node(
    hub: Arc<Hub>,
    call: Call<v1::UpdateNodeRequest>,
    sender: StreamSender,
) -> Result<wire::StreamTrailers> {
    let auth = &call.auth;
    let requested = node_name(&call.request.name)?;
    let node = remote_node(&hub, &requested)?;
    let name = node.name.clone();
    let _serial = Arc::clone(&hub.node_update_lock)
        .try_lock_owned()
        .map_err(|_| {
            RpcError::new(
                Code::FailedPrecondition,
                "a node update is already in progress",
            )
        })?;
    let monitor = Arc::clone(&hub.node_monitor);
    let name_for_poll = name.clone();
    tokio::task::spawn_blocking(move || monitor.poll_node(&name_for_poll))
        .await
        .map_err(super::join_error)?;
    let status = hub.node_monitor.status(&name, &node.address, node.invited);
    let metrics = match status.metrics {
        Some(metrics) if status.freshness() == v1::NodeFreshness::Fresh => metrics,
        _ => {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("node {name} is not reachable"),
            ))
        }
    };
    let previous = metrics.agent_version;
    if previous == hub.version && !call.request.force {
        sender
            .send(&v1::UpdateNodeProgress {
                phase: UpdatePhase::Skipped as i32,
                version: previous.clone(),
                previous_version: previous,
                message: format!("already at {}", hub.version),
                ..Default::default()
            })
            .await?;
        return Ok(Default::default());
    }
    let os = if metrics.os.is_empty() {
        node.os.clone()
    } else {
        metrics.os
    };
    let arch = if metrics.arch.is_empty() {
        node.arch.clone()
    } else {
        metrics.arch
    };
    let payload = payload_path(&hub, &os, &arch).await?;
    let binary = &payload.binary;
    let (size, sha) = tokio::task::spawn_blocking({
        let binary = binary.clone();
        move || digest(&binary)
    })
    .await
    .map_err(super::join_error)?
    .map_err(|e| {
        RpcError::new(
            Code::FailedPrecondition,
            format!("read Rust node binary {}: {e}", binary.display()),
        )
    })?;
    let total_bytes = if payload.bundle { 0 } else { size };
    sender
        .send(&v1::UpdateNodeProgress {
            phase: UpdatePhase::Sending as i32,
            total_bytes,
            message: if payload.bundle {
                "verified Rust release bundle".to_owned()
            } else {
                binary
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            ..Default::default()
        })
        .await?;
    let client = hub
        .node_files
        .client_with_timeout(auth, &name, Duration::from_secs(3600))?;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(8);
    let pushed = tokio::task::spawn_blocking({
        let transfer = payload.transfer.clone();
        let bundle = payload.bundle;
        let name = name.clone();
        move || {
            push_with_progress(
                &client,
                &name,
                transfer,
                if bundle { STAGE_BUNDLE } else { STAGE },
                true,
                true,
                |sent| {
                    tx.blocking_send(sent)
                        .map_err(|_| RpcError::new(Code::Canceled, "node update stream closed"))
                },
            )
        }
    });
    while let Some(sent_bytes) = tokio::select! {
        _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "node update stream closed")),
        item = rx.recv() => item,
    } {
        sender
            .send(&v1::UpdateNodeProgress {
                phase: UpdatePhase::Sending as i32,
                sent_bytes,
                total_bytes,
                ..Default::default()
            })
            .await?;
    }
    let transfer = pushed.await.map_err(super::join_error)??;
    if (!payload.bundle && (transfer.size != size || !transfer.sha256.eq_ignore_ascii_case(&sha)))
        || (payload.bundle && transfer.skipped != 0)
    {
        return Err(RpcError::new(
            Code::DataLoss,
            "node transfer differs from the verified release payload",
        ));
    }
    sender
        .send(&v1::UpdateNodeProgress {
            phase: UpdatePhase::Verifying as i32,
            sent_bytes: transfer.size,
            total_bytes: transfer.size,
            ..Default::default()
        })
        .await?;
    let client = hub
        .node_files
        .client_with_timeout(auth, &name, Duration::from_secs(60))?;
    let apply = v1::ApplyUpdateRequest {
        name: name.clone(),
        path: transfer.path.to_string_lossy().into_owned(),
        size,
        sha256: sha,
        expected_version: hub.version.clone(),
    };
    let result =
        tokio::task::spawn_blocking(move || client.unary(node_agent_service::APPLY_UPDATE, &apply))
            .await
            .map_err(super::join_error)??;
    let base = v1::UpdateNodeProgress {
        sent_bytes: transfer.size,
        total_bytes: transfer.size,
        version: result.version.clone(),
        previous_version: result.previous_version.clone(),
        restart: result.restart,
        ..Default::default()
    };
    for phase in [UpdatePhase::Swapping, UpdatePhase::Restarting] {
        sender
            .send(&v1::UpdateNodeProgress {
                phase: phase as i32,
                ..base.clone()
            })
            .await?;
    }
    let (version, before) = (result.version, result.previous_version);
    let deadline = Instant::now() + hub.node_update_timeout;
    while Instant::now() < deadline {
        tokio::select! {
            _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "node update stream closed")),
            _ = tokio::time::sleep(hub.node_update_poll) => {},
        }
        let client = hub
            .node_files
            .client_with_timeout(auth, &name, Duration::from_secs(5))?;
        let probed = tokio::task::spawn_blocking(move || {
            client.unary(node_agent_service::GET_METRICS, &v1::GetMetricsRequest {})
        })
        .await
        .map_err(super::join_error)?;
        let Ok(metrics) = probed else { continue };
        let actual = metrics.agent_version;
        let Some(state) = metrics
            .update
            .filter(|state| state.version == version && state.previous_version == before)
        else {
            continue;
        };
        let (phase, message) = match state.phase.as_str() {
            "confirmed" if actual == version => {
                (UpdatePhase::Confirmed, format!("node reports {actual}"))
            }
            "rolledback" if actual == before => (
                UpdatePhase::RolledBack,
                format!("node came back on {actual}; it put the previous binary back"),
            ),
            _ => continue,
        };
        sender
            .send(&v1::UpdateNodeProgress {
                phase: phase as i32,
                message,
                ..base
            })
            .await?;
        return Ok(Default::default());
    }
    Err(RpcError::new(Code::DeadlineExceeded, format!(
        "node {name} did not confirm {version} within {:?}; its trial watchdog should restore the previous binary if it remains unconfirmed",
        hub.node_update_timeout
    )))
}

fn node_name(name: &str) -> Result<String> {
    if name.trim().is_empty() {
        Err(RpcError::invalid("node name is required"))
    } else {
        Ok(name.to_owned())
    }
}
fn remote_node(hub: &Hub, name: &str) -> Result<protectorate_inventory::Node> {
    if name.eq_ignore_ascii_case(&hub.hostname) {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "the hub's own node is the hub process; update the hub instead",
        ));
    }
    let node = hub.inventory.get(name).map_err(|e| match e {
        protectorate_inventory::Error::NotFound(_) => RpcError::new(Code::NotFound, e.to_string()),
        _ => RpcError::new(Code::FailedPrecondition, e.to_string()),
    })?;
    if node.invited {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "node has not joined",
        ));
    }
    Ok(node)
}
async fn payload_path(hub: &Hub, os: &str, arch: &str) -> Result<Payload> {
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let name = protectorate_release::rust_binary_name(os, arch)
        .map_err(|e| RpcError::new(Code::FailedPrecondition, e.to_string()))?;
    let staging_root = hub
        .lease_path
        .parent()
        .ok_or_else(|| RpcError::new(Code::Internal, "hub state has no parent directory"))?;
    if let Some(dir) = &hub.node_binary_dir {
        return local_payload(dir.join(name), &hub.version, os, arch, staging_root);
    }
    let mut host_binary = None;
    if os == std::env::consts::OS && arch == host_arch() {
        let binary = std::env::current_exe().map_err(RpcError::internal)?;
        let local = local_payload(binary.clone(), &hub.version, os, arch, staging_root)?;
        if local.bundle {
            return Ok(local);
        }
        host_binary = Some(binary);
    }
    if let Some(source) = &hub.releases {
        // A hub installed from a release keeps the node assets it lists.
        let staged = source.release_dir(&hub.version).join(&name);
        if staged.is_file() {
            return local_payload(staged, &hub.version, os, arch, staging_root);
        }
        let info = source.current();
        if info.version == hub.version && info.asset_url(&name).is_ok() {
            let source = Arc::clone(source);
            let fetched_os = os.to_owned();
            let fetched_arch = arch.to_owned();
            if os == "linux" {
                let dir = tokio::task::spawn_blocking(move || {
                    source
                        .fetch_rust_node_bundle(&info, &fetched_os, &fetched_arch, |_, _, _| Ok(()))
                })
                .await
                .map_err(super::join_error)?
                .map_err(|e| {
                    RpcError::new(
                        Code::FailedPrecondition,
                        format!("fetch Rust node bundle: {e}"),
                    )
                })?;
                return local_payload(dir.join(name), &hub.version, os, arch, staging_root);
            }
            let binary = tokio::task::spawn_blocking(move || {
                source.fetch_rust_binary(&info, &fetched_os, &fetched_arch)
            })
            .await
            .map_err(super::join_error)?
            .map_err(|e| {
                RpcError::new(
                    Code::FailedPrecondition,
                    format!("fetch Rust node binary: {e}"),
                )
            })?;
            return local_payload(binary, &hub.version, os, arch, staging_root);
        }
    }
    if let Some(binary) = host_binary {
        return local_payload(binary, &hub.version, os, arch, staging_root);
    }
    Err(RpcError::new(
        Code::FailedPrecondition,
        format!(
            "cross-platform Rust binary for {os}/{arch} is not configured; set --node-binary-dir"
        ),
    ))
}

fn local_payload(
    binary: PathBuf,
    version: &str,
    os: &str,
    arch: &str,
    staging_root: &Path,
) -> Result<Payload> {
    if os != "linux" {
        return Ok(Payload {
            transfer: binary.clone(),
            binary,
            bundle: false,
            _staging: None,
        });
    }
    let Some(dir) = binary.parent() else {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "node update binary has no parent directory",
        ));
    };
    if !dir.join("SHA256SUMS").exists() {
        return Ok(Payload {
            transfer: binary.clone(),
            binary,
            bundle: false,
            _staging: None,
        });
    }
    let installed = protectorate_release::verify_rust_assets_dir(dir, os, arch).map_err(|e| {
        RpcError::new(
            Code::FailedPrecondition,
            format!("verify Rust node bundle: {e}"),
        )
    })?;
    let recorded_version = std::fs::read_to_string(dir.join("VERSION")).map_err(|e| {
        RpcError::new(
            Code::FailedPrecondition,
            format!("read Rust bundle VERSION: {e}"),
        )
    })?;
    if recorded_version.trim() != version {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            format!(
                "Rust node bundle VERSION {:?} differs from hub {version}",
                recorded_version.trim()
            ),
        ));
    }
    let staging = tempfile::Builder::new()
        .prefix(".node-update-")
        .tempdir_in(staging_root)
        .map_err(|e| RpcError::new(Code::Internal, format!("stage node bundle: {e}")))?;
    for component in protectorate_release::RUST_BUNDLE {
        let name = protectorate_release::rust_bundle_name(component, os, arch).map_err(|e| {
            RpcError::new(
                Code::FailedPrecondition,
                format!("name node component: {e}"),
            )
        })?;
        std::fs::hard_link(dir.join(&name), staging.path().join(&name))
            .or_else(|error| {
                if error.raw_os_error() == Some(libc::EXDEV) {
                    std::fs::copy(dir.join(&name), staging.path().join(&name)).map(|_| ())
                } else {
                    Err(error)
                }
            })
            .map_err(|e| {
                RpcError::new(Code::Internal, format!("stage node component {name}: {e}"))
            })?;
    }
    for name in ["VERSION", "SHA256SUMS"] {
        std::fs::copy(dir.join(name), staging.path().join(name))
            .map_err(|e| RpcError::new(Code::Internal, format!("stage node bundle {name}: {e}")))?;
    }
    Ok(Payload {
        binary: installed,
        transfer: staging.path().to_path_buf(),
        bundle: true,
        _staging: Some(staging),
    })
}
fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}
fn digest(path: &Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        hash.update(&bytes[..read]);
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("binary size overflow"))?;
    }
    Ok((size, format!("{:x}", hash.finalize())))
}
