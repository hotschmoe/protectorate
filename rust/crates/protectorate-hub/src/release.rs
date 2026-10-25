//! Release lookup, verified staging, and guarded hub restart requests.
use super::Hub;
use protectorate_audit::Entry as AuditEntry;
use protectorate_release::{self as release, Info, Source};
use protectorate_wire::{
    v1::{self, release_service, UpdatePhase},
    AuthContext, Call, Code, Result, RpcError, UnaryServer,
};
use serde_json::json;
use std::{fs, sync::Arc};

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let check = Arc::clone(&hub);
    server.unary(
        release_service::CHECK_RELEASE,
        move |call: Call<v1::CheckReleaseRequest>| {
            let hub = Arc::clone(&check);
            async move {
                let source = hub.releases.as_ref().ok_or_else(not_configured)?.clone();
                let v1::CheckReleaseRequest { channel, version } = call.request;
                validate_request(&source, &channel, &version)?;
                let result = tokio::task::spawn_blocking({
                    let source = Arc::clone(&source);
                    let channel = channel.clone();
                    let version = version.clone();
                    move || source.check(&channel, &version)
                })
                .await
                .map_err(join_error)?;
                let info = match result {
                    Ok(info) => {
                        if version.is_empty() && (channel.is_empty() || channel == source.channel())
                        {
                            publish(&hub, &info);
                        }
                        info
                    }
                    Err(error) if channel.is_empty() && version.is_empty() => {
                        let mut cached = source.available(source.channel());
                        if cached.channel.is_empty() {
                            cached.channel = source.channel().to_owned();
                        }
                        eprintln!("check hub release: {error}");
                        cached
                    }
                    Err(error) => return Err(RpcError::new(Code::Unavailable, error.to_string())),
                };
                let can_update = (call.auth.owner || call.auth.device)
                    && (info.version == hub.version
                        || (hub.release_restart.is_some()
                            && std::env::consts::OS == "linux"
                            && matches!(host_arch(), "amd64" | "arm64")
                            && release::RUST_BUNDLE.iter().all(|component| {
                                release::rust_bundle_name(component, "linux", host_arch())
                                    .ok()
                                    .is_some_and(|name| info.asset_url(&name).is_ok())
                            })));
                Ok(v1::ReleaseInfo {
                    version: info.version,
                    channel: info.channel,
                    notes_url: info.notes_url,
                    can_update,
                })
            }
        },
    )?;

    server.stream(
        release_service::UPDATE_HUB,
        move |call: Call<v1::UpdateHubRequest>, sender| {
            let hub = Arc::clone(&hub);
            async move {
                let source = hub.releases.as_ref().ok_or_else(not_configured)?.clone();
                let audit = (&call.auth, call.method.procedure.as_str());
                let v1::UpdateHubRequest { channel, version } = call.request;
                validate_request(&source, &channel, &version)?;
                let _lock = Arc::clone(&hub.release_update_lock)
                    .try_lock_owned()
                    .map_err(|_| RpcError::new(Code::FailedPrecondition, "a hub update is already in progress"))?;
                match fs::symlink_metadata(trial_path(&source)) {
                    Ok(_) => return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "a hub release trial is awaiting guardian confirmation",
                    )),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(RpcError::new(Code::Internal, format!("inspect hub release trial: {error}"))),
                }
                let info = tokio::task::spawn_blocking({
                    let source = Arc::clone(&source);
                    move || source.check(&channel, &version)
                })
                .await
                .map_err(join_error)?
                .map_err(|error| RpcError::new(Code::Unavailable, format!("resolve release: {error}")))?;
                let restarting = info.version != hub.version;
                if restarting && hub.release_restart.is_none() {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "hub restart and rollback supervisor is not configured",
                    ));
                }
                if restarting && std::env::consts::OS != "linux" {
                    return Err(RpcError::new(Code::FailedPrecondition, "no published Rust hub bundle for this platform"));
                }
                let rust_bundle = std::env::consts::OS == "linux"
                    && matches!(host_arch(), "amd64" | "arm64")
                    && release::RUST_BUNDLE.iter().all(|component| {
                        release::rust_bundle_name(component, "linux", host_arch())
                            .ok()
                            .is_some_and(|name| info.asset_url(&name).is_ok())
                    });
                if restarting && !rust_bundle {
                    return Err(RpcError::new(Code::FailedPrecondition, "release has no complete Rust hub bundle for this platform"));
                }
                let previous = source.current().version;
                if restarting && !release::valid_version(&previous) {
                    return Err(RpcError::new(Code::FailedPrecondition, "installed hub has no current release to restore"));
                }
                let progress = |phase: UpdatePhase, message: &str| v1::UpdateHubProgress {
                    phase: phase as i32,
                    message: message.into(),
                    version: info.version.clone(),
                    previous_version: hub.version.clone(),
                    ..Default::default()
                };
                if previous == info.version && !restarting {
                    sender
                        .send(&progress(UpdatePhase::Skipped, "hub already runs this release"))
                        .await?;
                    record_update(&hub, audit, &info);
                    return Ok(Default::default());
                }
                let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, u64, u64)>(8);
                let fetch = tokio::task::spawn_blocking({
                    let source = Arc::clone(&source);
                    let info = info.clone();
                    move || {
                        let progress = |name: &str, sent, total| {
                        tx.blocking_send((name.to_owned(), sent, total)).map_err(|_| {
                            release::Error::Invalid("release update stream closed".into())
                        })
                        };
                        if rust_bundle { source.fetch_rust_release(&info, "linux", host_arch(), progress) }
                        else { source.fetch(&info, progress) }
                    }
                });
                while let Some((name, sent, total)) = tokio::select! {
                    _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "release update stream closed")),
                    item = rx.recv() => item,
                } {
                    sender
                        .send(&v1::UpdateHubProgress {
                            sent_bytes: sent,
                            total_bytes: total,
                            ..progress(UpdatePhase::Sending, &name)
                        })
                        .await?;
                }
                fetch.await.map_err(join_error)?.map_err(|error| {
                    RpcError::new(Code::FailedPrecondition, format!("fetch verified release: {error}"))
                })?;
                sender
                    .send(&progress(UpdatePhase::Verifying, "all fleet assets verified"))
                    .await?;
                if sender.is_cancelled() {
                    return Err(RpcError::new(Code::Canceled, "release update stream closed"));
                }
                if restarting {
                    write_trial(&source, &previous, &info.version).map_err(|error| RpcError::new(Code::Internal, format!("prepare hub trial: {error}")))?;
                    let sent = sender
                        .send(&progress(
                            UpdatePhase::Restarting,
                            "hub will restart; check GetStatus to confirm the new version",
                        ))
                        .await;
                    if sent.is_ok() {
                        record_update(&hub, audit, &info);
                    }
                    if let Some(restart) = &hub.release_restart { restart(); }
                    sent?;
                    return Ok(Default::default());
                }
                tokio::task::spawn_blocking({
                    let source = Arc::clone(&source);
                    let version = info.version.clone();
                    move || -> release::Result<()> {
                        source.activate(&version)?;
                        source.prune(&previous)
                    }
                })
                .await
                .map_err(join_error)?
                .map_err(|error| RpcError::new(Code::FailedPrecondition, error.to_string()))?;
                sender
                    .send(&progress(
                        UpdatePhase::Confirmed,
                        "fleet assets now match the running hub",
                    ))
                    .await?;
                record_update(&hub, audit, &info);
                Ok(Default::default())
            }
        },
    )?;
    Ok(())
}

fn record_update(hub: &Hub, (auth, procedure): (&AuthContext, &str), info: &Info) {
    hub.audit.write(AuditEntry {
        actor: auth.actor.clone(),
        token_id: auth.token_id.clone(),
        task: auth.task.clone(),
        procedure: procedure.into(),
        subject: "hub".into(),
        detail: format!("channel={} version={}", info.channel, info.version),
        outcome: "ok".into(),
        ..AuditEntry::default()
    });
}

fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn trial_path(source: &Source) -> std::path::PathBuf {
    source.dir().join(".hub-trial.json")
}

fn write_trial(source: &Source, previous: &str, version: &str) -> std::io::Result<()> {
    let path = trial_path(source);
    let staged = path.with_extension("json.new");
    let body = json!({"previous":previous,"version":version}).to_string();
    fs::write(&staged, body)?;
    fs::rename(staged, path)
}

fn validate_request(source: &Source, channel: &str, version: &str) -> Result<()> {
    let effective = if channel.is_empty() {
        source.channel()
    } else {
        channel
    };
    release::validate_channel(effective)
        .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
    if !version.is_empty() && !release::valid_version(version) {
        return Err(RpcError::new(
            Code::InvalidArgument,
            format!("invalid release version {version:?}"),
        ));
    }
    Ok(())
}

fn publish(hub: &Hub, info: &Info) {
    let next = (&info.version, &info.channel, &info.notes_url);
    let mut last = hub.release_last.lock().unwrap_or_else(|e| e.into_inner());
    if last
        .as_ref()
        .is_some_and(|last| (&last.0, &last.1, &last.2) == next)
    {
        return;
    }
    *last = Some((
        info.version.clone(),
        info.channel.clone(),
        info.notes_url.clone(),
    ));
    hub.emit(
        "release.available",
        "hub",
        json!({
            "current_version":hub.version,"available_version":info.version,
            "channel":info.channel,"release_notes_url":info.notes_url
        }),
    );
}

fn not_configured() -> RpcError {
    RpcError::new(Code::FailedPrecondition, "release source is not configured")
}

fn join_error(error: tokio::task::JoinError) -> RpcError {
    RpcError::from_join(error)
}
