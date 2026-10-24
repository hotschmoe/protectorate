use super::Hub;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_inventory::{Error, Node, CARE, SHARED};
use protectorate_lease::Entry;
use protectorate_node_monitor::PollEvent;
use protectorate_wire::{
    self as wire,
    v1::{
        self, node_agent_service, node_service, AlertKind, InstallPlatform, NodeFreshness,
        NodeSetupStage, RemoveJobs, RemovePhase,
    },
    Call, Code, Result, RpcError, StreamSender, UnaryServer,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_DRAIN_WAIT: Duration = Duration::from_secs(3600);
const DRAIN_POLL: Duration = Duration::from_millis(200);
// NodeStatus alerts the hub judges itself from the node's reported figures.
const DISK_ALERT_PERCENT: f64 = 90.0;
const MEM_ALERT_PERCENT: f64 = 90.0;
// How often WatchNodeSetup re-reads progress, and how often it kicks the
// monitor's poll while a node is between joining and ready.
const SETUP_POLL: Duration = Duration::from_secs(1);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// One-line installers: fetch the CA, check its SHA-256 against the invite,
/// then fetch and run the installer over TLS pinned to that CA.
fn install_commands(
    base: &str,
    code: &str,
    name: &str,
    ca_sha256: &str,
) -> Vec<v1::InstallCommand> {
    let sh_code = super::distribution::shell_word(code);
    let sh_name = super::distribution::shell_word(name);
    let sh_ca = super::distribution::shell_word(&format!("{base}/ca.pem"));
    let sh_pin = format!(
        "ca=$(mktemp) && curl -fsSk --max-redirs 0 {sh_ca} -o \"$ca\" && [ \"$(sha256sum < \"$ca\" | cut -d' ' -f1)\" = {ca_sha256} ]"
    );
    let sh = |script: &str| {
        let url = super::distribution::shell_word(&format!("{base}/{script}"));
        format!("{sh_pin} && script=$(curl -fsSL --cacert \"$ca\" {url}) && [ -n \"$script\" ] && printf '%s\\n' \"$script\" | PROTECTORATE_CA_CERT=\"$ca\" PROTECTORATE_JOIN_CODE={sh_code} sh -s -- --name {sh_name}")
    };
    let ps_code = super::distribution::powershell_word(code);
    let ps_name = super::distribution::powershell_word(name);
    let ps_base = super::distribution::powershell_word(base);
    let windows = format!(
        "$ErrorActionPreference='Stop'; $h={ps_base}; [Net.ServicePointManager]::SecurityProtocol='Tls12'; [Net.ServicePointManager]::ServerCertificateValidationCallback={{$true}}; $p=Join-Path $env:TEMP ('protectorate-ca-'+[guid]::NewGuid()+'.pem'); (New-Object Net.WebClient).DownloadFile(\"$h/ca.pem\",$p); if((Get-FileHash $p -Algorithm SHA256).Hash -ne '{ca_sha256}'){{throw 'hub CA fingerprint mismatch'}}; $ca=New-Object Security.Cryptography.X509Certificates.X509Certificate2 $p; [Net.ServicePointManager]::ServerCertificateValidationCallback={{param($s,$c,$x,$e) $k=New-Object Security.Cryptography.X509Certificates.X509Chain; $k.ChainPolicy.RevocationMode='NoCheck'; $k.ChainPolicy.VerificationFlags='AllowUnknownCertificateAuthority'; [void]$k.ChainPolicy.ExtraStore.Add($ca); (($e -band 3) -eq 0) -and $k.Build([Security.Cryptography.X509Certificates.X509Certificate2]$c) -and ($k.ChainElements[$k.ChainElements.Count-1].Certificate.Thumbprint -eq $ca.Thumbprint)}}; $env:PROTECTORATE_CA_CERT=$p; $env:PROTECTORATE_JOIN_CODE={ps_code}; $env:PROTECTORATE_NODE_NAME={ps_name}; iex ((New-Object Net.WebClient).DownloadString(\"$h/install.ps1\"))"
    );
    let command =
        |platform: InstallPlatform, shell: &str, command: String, note: &str| v1::InstallCommand {
            platform: platform as i32,
            shell: shell.into(),
            command,
            note: note.into(),
        };
    vec![
        command(
            InstallPlatform::Windows,
            "Windows PowerShell",
            windows,
            "Paste into Windows PowerShell (powershell.exe, not pwsh) as the signed-in user, not as administrator. Installs a logon task in that user's desktop session. x64 and ARM64.",
        ),
        command(
            InstallPlatform::Linux,
            "sh",
            sh("install.sh"),
            "Paste into a terminal as the user the agent should run as, not root. Needs curl and installs a systemd user unit. To keep it running while logged out, enable linger for that user.",
        ),
        command(
            InstallPlatform::Termux,
            "Termux",
            sh("install-termux.sh"),
            "Paste into Termux on Android aarch64. For boot startup, install termux-services, restart Termux, and open Termux:Boot once.",
        ),
    ]
}

struct Drain<'a> {
    hub: &'a Hub,
    name: String,
}
impl Drop for Drain<'_> {
    fn drop(&mut self) {
        self.hub.node_draining.lock().unwrap().remove(&self.name);
    }
}

fn start_drain<'a>(hub: &'a Hub, name: &str) -> Result<Drain<'a>> {
    let _admission = hub
        .node_admission
        .write()
        .unwrap_or_else(|error| error.into_inner());
    let mut draining = hub.node_draining.lock().unwrap();
    if !draining.insert(name.to_owned()) {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            format!("{name} is already being removed"),
        ));
    }
    drop(draining);
    Ok(Drain {
        hub,
        name: name.to_owned(),
    })
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let list = Arc::clone(&hub);
    server.unary(node_service::LIST_NODES, move |_: Call<_>| {
        let hub = Arc::clone(&list);
        async move {
            let mut nodes = vec![status(&hub, local_node(&hub))?];
            for node in hub.inventory.list() {
                nodes.push(status(&hub, node_info(&node, false))?);
            }
            Ok(v1::ListNodesResponse { nodes })
        }
    })?;
    let get = Arc::clone(&hub);
    server.unary(
        node_service::GET_NODE,
        move |call: Call<v1::GetNodeRequest>| {
            let hub = Arc::clone(&get);
            async move {
                let name = call.request.name;
                let node = if name.eq_ignore_ascii_case(&hub.hostname) {
                    local_node(&hub)
                } else {
                    node_info(&hub.inventory.get(&name).map_err(node_error)?, false)
                };
                status(&hub, node)
            }
        },
    )?;
    let add = Arc::clone(&hub);
    server.unary(
        node_service::ADD_NODE,
        move |call: Call<v1::AddNodeRequest>| {
            let hub = Arc::clone(&add);
            async move {
                let node = call
                    .request
                    .node
                    .map(inventory_node)
                    .ok_or_else(|| RpcError::new(Code::InvalidArgument, "node is required"))?;
                let update = call.request.update;
                let inventory = Arc::clone(&hub.inventory);
                let admission = Arc::clone(&hub.node_admission);
                let draining = Arc::clone(&hub.node_draining);
                let node = tokio::task::spawn_blocking(move || {
                    let _admission = admission.read().unwrap_or_else(|error| error.into_inner());
                    if draining.lock().unwrap().contains(&node.name) {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "node is being removed",
                        ));
                    }
                    inventory.add(node, update).map_err(node_error)
                })
                .await
                .map_err(super::join_error)??;
                hub.node_monitor.invalidate(&node.name);
                // Wait for the first poll, so a lease or job asked for right
                // after AddNode returns sees the node's health.
                let monitor = Arc::clone(&hub.node_monitor);
                let name = node.name.clone();
                tokio::task::spawn_blocking(move || monitor.poll_node(&name))
                    .await
                    .map_err(super::join_error)?;
                Ok(node_info(&node, false))
            }
        },
    )?;
    let invite = Arc::clone(&hub);
    server.unary(node_service::INVITE_NODE, move |call: Call<_>| {
        let hub = Arc::clone(&invite);
        async move { invite_node(hub, call.request).await }
    })?;
    let join = Arc::clone(&hub);
    server.unary(node_service::JOIN_NODE, move |call: Call<_>| {
        let hub = Arc::clone(&join);
        async move { join_node(hub, call.request).await }
    })?;
    let set_tier = Arc::clone(&hub);
    server.unary(
        node_service::SET_NODE_TIER,
        move |call: Call<v1::SetNodeTierRequest>| {
            let hub = Arc::clone(&set_tier);
            async move {
                let name = call.request.name.trim().to_ascii_lowercase();
                let tier = call.request.tier.trim().to_ascii_lowercase();
                if tier != SHARED && tier != CARE {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "tier must be shared or care",
                    ));
                }
                if name == hub.hostname.to_ascii_lowercase() {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "the hub's own node is always shared",
                    ));
                }
                let inventory = Arc::clone(&hub.inventory);
                let admission = Arc::clone(&hub.node_admission);
                let draining = Arc::clone(&hub.node_draining);
                let name_for_store = name.clone();
                let node = tokio::task::spawn_blocking(move || {
                    let _admission = admission.read().unwrap_or_else(|error| error.into_inner());
                    if draining.lock().unwrap().contains(&name_for_store) {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "node is being removed",
                        ));
                    }
                    let prev = inventory.get(&name_for_store).map_err(node_error)?;
                    if prev.invited {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            format!("{name_for_store} is invited but has not joined"),
                        ));
                    }
                    inventory
                        .set_tier(&name_for_store, &tier)
                        .map_err(node_error)
                })
                .await
                .map_err(super::join_error)??;
                hub.emit("node.tier", &name, Value::Null);
                Ok(node_info(&node, false))
            }
        },
    )?;
    let setup = Arc::clone(&hub);
    server.stream(node_service::REMOVE_NODE, move |call: Call<_>, sender| {
        let hub = Arc::clone(&hub);
        async move { remove_node(hub, call.request, sender).await }
    })?;
    server.stream(
        node_service::WATCH_NODE_SETUP,
        move |call: Call<v1::WatchNodeSetupRequest>, sender| {
            let hub = Arc::clone(&setup);
            async move { watch_node_setup(hub, call.request.name, sender).await }
        },
    )?;
    Ok(())
}

async fn watch_node_setup(
    hub: Arc<Hub>,
    name: String,
    sender: StreamSender,
) -> Result<wire::StreamTrailers> {
    if name.is_empty() {
        return Err(RpcError::invalid("node name is required"));
    }
    hub.inventory.get(&name).map_err(node_error)?;
    let mut last = None::<v1::NodeSetupProgress>;
    // A node that just joined is only seen when the hub next polls it. Kick
    // the poll from the watch so the setup flow does not sit at JOINED for a
    // whole poll interval.
    let mut kicked = None::<Instant>;
    loop {
        let node = hub.inventory.get(&name).map_err(node_error)?;
        let status = status(&hub, node_info(&node, false))?;
        let progress = setup_progress(&hub, &node, status);
        let stage = progress.stage();
        let changed = last.as_ref().is_none_or(|previous| {
            previous.stage != progress.stage
                || previous.message != progress.message
                || previous.problems != progress.problems
                || previous.notes != progress.notes
        });
        if changed {
            sender.send(&progress).await?;
            last = Some(progress);
        }
        if stage == NodeSetupStage::Ready {
            return Ok(Default::default());
        }
        if stage != NodeSetupStage::Invited
            && kicked.is_none_or(|at| at.elapsed() >= 3 * SETUP_POLL)
        {
            kicked = Some(Instant::now());
            let monitor = Arc::clone(&hub.node_monitor);
            let node_name = name.clone();
            tokio::task::spawn_blocking(move || monitor.poll_node(&node_name));
        }
        tokio::select! {
            _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "node setup watch closed")),
            _ = tokio::time::sleep(SETUP_POLL) => {},
        }
    }
}

fn setup_progress(hub: &Hub, node: &Node, status: v1::NodeStatus) -> v1::NodeSetupProgress {
    let mut problems = Vec::<String>::new();
    let mut notes = Vec::<String>::new();
    let error = if status.error.is_empty() {
        None
    } else {
        Some(status.error.as_str())
    };
    let (mut stage, message) = if node.invited {
        if !node.cert_id.is_empty() {
            (
                NodeSetupStage::Joined,
                format!(
                    "joined; waiting for the hub to reach the agent at {}",
                    node.address
                ),
            )
        } else {
            (
                NodeSetupStage::Invited,
                "waiting for the machine to run the installer".to_owned(),
            )
        }
    } else if let Some(metrics) = &status.metrics {
        if status.freshness() != NodeFreshness::Fresh {
            problems.push(format!(
                "the hub cannot reach the agent now: {}",
                error.unwrap_or("no recent poll")
            ));
        }
        let version = &metrics.agent_version;
        if !version.is_empty() && *version != hub.version {
            problems.push(format!(
                "the agent runs {version} and the hub {}: update the node",
                hub.version
            ));
        }
        notes.extend(metrics.warnings.iter().cloned());
        if problems.is_empty() {
            (
                NodeSetupStage::Ready,
                format!("{}/{}, agent {version}", metrics.os, metrics.arch),
            )
        } else {
            (
                NodeSetupStage::Attention,
                "the node answers but needs attention".to_owned(),
            )
        }
    } else {
        (
            NodeSetupStage::Joined,
            error
                .unwrap_or("waiting for the hub to reach the agent")
                .to_owned(),
        )
    };
    for alert in &status.alerts {
        if alert.message.is_empty() {
            continue;
        }
        if node.invited {
            problems.push(alert.message.clone());
        } else if alert.kind() != AlertKind::Unreachable {
            notes.push(alert.message.clone());
        }
    }
    if node.invited && !problems.is_empty() {
        stage = NodeSetupStage::Attention;
    }
    v1::NodeSetupProgress {
        stage: stage as i32,
        problems,
        notes,
        message,
        status: Some(status),
    }
}

async fn invite_node(hub: Arc<Hub>, body: v1::InviteNodeRequest) -> Result<v1::InviteNodeResponse> {
    let name = body.name;
    if !valid_invite_name(&name) {
        return Err(RpcError::invalid(
            "name must be a lowercase DNS label, at most 63 characters",
        ));
    }
    let ca = hub.ca.clone().ok_or_else(|| {
        RpcError::new(
            Code::FailedPrecondition,
            "this hub has no CA to issue node certificates",
        )
    })?;
    let tier = if body.tier.is_empty() {
        SHARED.to_owned()
    } else {
        body.tier
    };
    if tier != SHARED && tier != CARE {
        return Err(RpcError::invalid("tier must be shared or care"));
    }
    let ttl = match &body.ttl {
        None => Duration::from_secs(3600),
        some => wire::duration(some.as_ref(), "ttl")?,
    };
    if ttl.is_zero() || ttl > Duration::from_secs(24 * 3600) {
        return Err(RpcError::invalid("ttl must be positive and at most 24h"));
    }
    let base = install_base(&hub, &body.install_url)?;
    if name == hub.hostname {
        return Err(RpcError::new(
            Code::AlreadyExists,
            format!("{name} is the hub's own node"),
        ));
    }
    let mut secret = [0_u8; 32];
    getrandom::getrandom(&mut secret).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    let code = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret);
    let ca_sha256 = hex(&Sha256::digest(ca.pem().as_bytes()));
    let expires = protectorate_tls::unix_now() + ttl.as_secs() as i64;
    let node = Node {
        name: name.clone(),
        tier,
        invited: true,
        code_sha256: hex(&Sha256::digest(code.as_bytes())),
        code_expires: expires,
        ..Node::default()
    };
    let inventory = Arc::clone(&hub.inventory);
    let admission = Arc::clone(&hub.node_admission);
    let draining = Arc::clone(&hub.node_draining);
    let node = tokio::task::spawn_blocking(move || {
        let _admission = admission.write().unwrap_or_else(|error| error.into_inner());
        if draining.lock().unwrap().contains(&node.name) {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "node is being removed",
            ));
        }
        let pending = match inventory.get(&node.name) {
            Ok(old) if !old.invited => {
                return Err(RpcError::new(
                    Code::AlreadyExists,
                    format!("{} is already in the inventory: remove it first", node.name),
                ))
            }
            Ok(_) => true,
            Err(Error::NotFound(_)) => false,
            Err(error) => return Err(node_error(error)),
        };
        inventory.add(node, pending).map_err(node_error)
    })
    .await
    .map_err(super::join_error)??;
    let result = v1::InviteNodeResponse {
        node: Some(node_info(&node, false)),
        hub_url: base.clone(),
        commands: install_commands(&base, &code, &name, &ca_sha256),
        code,
        expires_at: Some(wire::timestamp_millis(expires.saturating_mul(1000))),
        install_url: base,
        ca_sha256,
    };
    hub.node_monitor.invalidate(&name);
    hub.emit("node.invited", &name, Value::Null);
    Ok(result)
}

async fn join_node(hub: Arc<Hub>, body: v1::JoinNodeRequest) -> Result<v1::JoinNodeResponse> {
    let v1::JoinNodeRequest {
        name,
        code,
        public_key,
        proof,
        address,
    } = body;
    let ca = hub.ca.clone().ok_or_else(|| {
        RpcError::new(
            Code::FailedPrecondition,
            "this hub has no CA to issue node certificates",
        )
    })?;
    if !protectorate_tls::verify_proof(
        &public_key,
        &protectorate_tls::join_message(&name, &code),
        &proof,
    ) {
        return Err(RpcError::invalid(
            "proof must sign the join message with the uncompressed P-256 key",
        ));
    }
    let address: std::net::SocketAddr = address
        .parse()
        .map_err(|_| RpcError::invalid("address must be IP:PORT"))?;
    protectorate_tls::check_listen(address)
        .map_err(|error| RpcError::invalid(error.to_string()))?;
    let mut id = [0_u8; 8];
    getrandom::getrandom(&mut id).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    let cert_id = hex(&id);
    let (certificate, cert_expires) = ca
        .issue_node(&name, &cert_id, &public_key)
        .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
    let inventory = Arc::clone(&hub.inventory);
    let code_sha256 = hex(&Sha256::digest(code.as_bytes()));
    let joined = Node {
        address: address.to_string(),
        cert_id,
        cert_expires,
        public_key: STANDARD.encode(&public_key),
        ..Node::default()
    };
    let node_name = name.clone();
    tokio::task::spawn_blocking(move || {
        inventory.redeem(
            &node_name,
            &code_sha256,
            protectorate_tls::unix_now(),
            joined,
        )
    })
    .await
    .map_err(super::join_error)?
    .map_err(|error| match error {
        Error::Io(_) | Error::Json(_) => node_error(error),
        _ => RpcError::new(
            Code::PermissionDenied,
            "the join code is unknown, used or expired: invite the node again",
        ),
    })?;
    hub.node_monitor.invalidate(&name);
    let monitor = Arc::clone(&hub.node_monitor);
    tokio::task::spawn_blocking(move || monitor.poll_node(&name));
    Ok(v1::JoinNodeResponse {
        certificate,
        ca: ca.pem().to_owned(),
    })
}

fn valid_invite_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && label_byte(bytes[0])
        && label_byte(bytes[bytes.len() - 1])
        && bytes.iter().all(|b| label_byte(*b) || *b == b'-')
}

fn label_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn install_base(hub: &Hub, asked: &str) -> Result<String> {
    let base = if asked.is_empty() {
        hub.public_url.clone().ok_or_else(|| {
            RpcError::new(
                Code::FailedPrecondition,
                "the hub has no TLS listener: start it with --tls-listen or --public-url",
            )
        })?
    } else {
        validate_install_base(asked)?
    };
    if !base.starts_with("https://") {
        return Err(RpcError::invalid(
            "the install URL must be the hub's HTTPS URL",
        ));
    }
    Ok(base)
}

pub(super) fn validate_install_base(raw: &str) -> Result<String> {
    let url = raw.trim().trim_end_matches('/');
    let (scheme, authority) = url
        .split_once("://")
        .ok_or_else(|| RpcError::invalid("install URL must be http(s)://host[:port]"))?;
    if !matches!(scheme, "http" | "https")
        || authority.is_empty()
        || authority.contains(['/', '?', '#', '@', '\\'])
    {
        return Err(RpcError::invalid(
            "install URL must be http(s)://host[:port]",
        ));
    }
    let parsed: axum::http::uri::Authority = authority
        .parse()
        .map_err(|_| RpcError::invalid("invalid install URL host"))?;
    if parsed
        .port()
        .is_some_and(|_| parsed.port_u16().is_none_or(|port| port == 0))
    {
        return Err(RpcError::invalid("invalid install URL port"));
    }
    let host = parsed.host();
    if host.is_empty()
        || host.contains("..")
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'[' | b']'))
    {
        return Err(RpcError::invalid("invalid install URL host"));
    }
    if authority.ends_with(':') || authority != parsed.as_str() {
        return Err(RpcError::invalid("invalid install URL port"));
    }
    Ok(format!("{scheme}://{authority}"))
}

async fn remove_node(
    hub: Arc<Hub>,
    body: v1::RemoveNodeRequest,
    sender: StreamSender,
) -> Result<wire::StreamTrailers> {
    let name = body.name.trim().to_ascii_lowercase();
    if name == hub.hostname {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "the hub's own node cannot be removed",
        ));
    }
    let _drain = start_drain(&hub, &name)?;
    let info = hub.inventory.get(&name).map_err(node_error)?;
    let jobs =
        RemoveJobs::try_from(body.jobs).map_err(|_| RpcError::invalid("invalid jobs mode"))?;
    let wait = wire::duration(body.wait_timeout.as_ref(), "wait_timeout")?;
    let wait = if wait.is_zero() {
        DEFAULT_DRAIN_WAIT
    } else {
        wait
    };
    let mut ids = hub.jobs.active_on(&name);
    match jobs {
        RemoveJobs::Cancel => {
            if !ids.is_empty() {
                progress(
                    &sender,
                    RemovePhase::Draining,
                    ids.len(),
                    format!("no new jobs or leases; canceling {}", ids.join(", ")),
                )
                .await?;
            }
            for id in &ids {
                let jobs = Arc::clone(&hub.jobs);
                let id = id.clone();
                tokio::task::spawn_blocking(move || jobs.cancel(&id))
                    .await
                    .map_err(super::join_error)??;
            }
        }
        RemoveJobs::Wait => {}
        RemoveJobs::Unspecified if !ids.is_empty() => {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!(
                    "{} job(s) running on {name} ({}): wait for them or cancel them",
                    ids.len(),
                    ids.join(", ")
                ),
            ));
        }
        RemoveJobs::Unspecified => {}
    }
    if ids.is_empty() {
        progress(
            &sender,
            RemovePhase::Draining,
            0,
            "no new jobs or leases; no jobs running",
        )
        .await?;
    } else {
        let deadline = Instant::now() + wait;
        let mut previous = usize::MAX;
        loop {
            ids = hub.jobs.active_on(&name);
            if ids.is_empty() {
                break;
            }
            if ids.len() != previous {
                progress(
                    &sender,
                    RemovePhase::Draining,
                    ids.len(),
                    format!("waiting for {} job(s) to finish", ids.len()),
                )
                .await?;
                previous = ids.len();
            }
            if Instant::now() >= deadline {
                return Err(RpcError::new(
                    Code::DeadlineExceeded,
                    format!(
                        "{} job(s) still running on {name}: cancel them or wait longer",
                        ids.len()
                    ),
                ));
            }
            tokio::select! {
                _ = tokio::time::sleep(DRAIN_POLL) => {},
                _ = sender.cancelled() => return Err(RpcError::new(Code::Canceled, "node removal stream closed")),
            }
        }
        progress(&sender, RemovePhase::Draining, 0, "no jobs running").await?;
    }
    if body.uninstall {
        let message = if info.invited || !hub.node_monitor.is_fresh(&name, &info.address) {
            "not uninstalled: the node is not reachable now; run `protectorate node uninstall` on the machine".to_owned()
        } else {
            let jobs = Arc::clone(&hub.jobs);
            let node = name.clone();
            match tokio::task::spawn_blocking(move || jobs.uninstall_node(&node))
                .await.map_err(super::join_error)? {
                Ok(reply) => {
                    let actions = reply.actions.join("; ");
                    let left = reply.left.join(", ");
                    if left.is_empty() { format!("the agent unregisters its startup and stops: {actions}") }
                    else { format!("the agent unregisters its startup and stops: {actions}. Nothing is deleted; left on the machine: {left}") }
                }
                Err(error) if error.code == Code::Unimplemented =>
                    "not uninstalled: the agent cannot uninstall itself; run `protectorate node uninstall` on the machine".to_owned(),
                Err(error) => return Err(RpcError::new(error.code, format!("uninstall on {name}: {}", error.message))),
            }
        };
        progress(&sender, RemovePhase::Uninstalling, 0, message).await?;
    }
    let revoke_message = if info.invited && info.cert_id.is_empty() {
        "the unused join code stops working"
    } else {
        "the node certificate is no longer trusted"
    };
    progress(&sender, RemovePhase::Revoking, 0, revoke_message).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let entries = hub.leases.list(now).map_err(lease_error)?;
    let mut released = 0;
    for entry in entries {
        let (id, resource) = match entry {
            Entry::Active(lease) => (lease.id, lease.resource.to_string()),
            Entry::Queued { ticket, .. } => (ticket.id, ticket.resource),
        };
        if resource == name || resource.starts_with(&format!("{name}:")) {
            hub.leases.release(&id, now).map_err(lease_error)?;
            released += 1;
        }
    }
    let inventory = Arc::clone(&hub.inventory);
    let removed_name = name.clone();
    tokio::task::spawn_blocking(move || inventory.remove(&removed_name))
        .await
        .map_err(super::join_error)?
        .map_err(node_error)?;
    hub.node_monitor.invalidate(&name);
    hub.emit("node.removed", &name, Value::Null);
    let mut message = format!("removed {name} from the inventory");
    if released > 0 {
        message.push_str(&format!("; released {released} lease(s) on it"));
    }
    progress(&sender, RemovePhase::Removed, 0, message).await?;
    Ok(Default::default())
}

async fn progress(
    sender: &StreamSender,
    phase: RemovePhase,
    running: usize,
    message: impl Into<String>,
) -> Result<()> {
    sender
        .send(&v1::RemoveNodeProgress {
            phase: phase as i32,
            running_jobs: u32::try_from(running).unwrap_or(u32::MAX),
            message: message.into(),
        })
        .await
}

fn lease_error(error: protectorate_lease::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}

/// The inventory record an AddNode request describes; `local` is not stored.
fn inventory_node(info: v1::NodeInfo) -> Node {
    Node {
        name: info.name,
        address: info.address,
        os: info.os,
        arch: info.arch,
        tags: info.tags,
        ssh_alias: info.ssh_alias,
        tier: info.tier,
        notes: info.notes,
        invited: info.invited,
        ..Node::default()
    }
}

fn node_info(node: &Node, local: bool) -> v1::NodeInfo {
    v1::NodeInfo {
        name: node.name.clone(),
        address: node.address.clone(),
        os: node.os.clone(),
        arch: node.arch.clone(),
        tags: node.tags.clone(),
        ssh_alias: node.ssh_alias.clone(),
        tier: node.tier.clone(),
        notes: node.notes.clone(),
        local,
        invited: node.invited,
    }
}

pub(super) fn publish_poll_event(hub: &Arc<Hub>, event: PollEvent) {
    if event.kind == "node.joined" {
        hub.emit(event.kind, &event.name, json!({"address": event.address}));
        return;
    }
    let node = if event.name == hub.hostname && event.address == "local" {
        local_node(hub)
    } else {
        let Ok(node) = hub.inventory.get(&event.name) else {
            return;
        };
        if node.address != event.address {
            return;
        }
        node_info(&node, false)
    };
    let status = match status(hub, node) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("node {}: status: {}", event.name, error.message);
            return;
        }
    };
    let payload = match event.kind {
        "node.metrics" => {
            renew_if_due(hub, &event.name);
            metrics_summary(&status)
        }
        "node.unreachable" => json!({
            "error": status.error,
            "address": event.address,
        }),
        _ => return,
    };
    hub.emit(event.kind, &event.name, payload);
}

/// The "node.metrics" event payload.
fn metrics_summary(status: &v1::NodeStatus) -> Value {
    let freshness = match status.freshness() {
        NodeFreshness::Unspecified => String::new(),
        freshness => freshness
            .as_str_name()
            .trim_start_matches("NODE_FRESHNESS_")
            .to_ascii_lowercase(),
    };
    let alerts = status
        .alerts
        .iter()
        .map(|alert| {
            alert
                .kind()
                .as_str_name()
                .trim_start_matches("ALERT_KIND_")
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let default = v1::NodeMetrics::default();
    let metrics = status.metrics.as_ref().unwrap_or(&default);
    let mut summary = json!({
        "freshness": freshness,
        "alerts": alerts,
        "gpu_count": metrics.gpus.len(),
        "containers": metrics.engine.as_ref().map_or(0, |engine| engine.containers.len()),
    });
    if let Some(cpu) = &metrics.cpu {
        summary["cpu_percent"] = json!(cpu.usage_percent);
    }
    if let Some(memory) = &metrics.memory {
        summary["mem_percent"] = json!(memory.used_percent);
    }
    if let Some(worst) = metrics.filesystems.iter().reduce(|worst, next| {
        if next.used_percent > worst.used_percent {
            next
        } else {
            worst
        }
    }) {
        summary["worst_disk_percent"] = json!(worst.used_percent);
        summary["worst_disk_mount"] = json!(worst.mount);
    }
    summary
}
fn local_node(hub: &Hub) -> v1::NodeInfo {
    node_info(
        &Node {
            name: hub.hostname.clone(),
            address: "local".into(),
            os: super::platform_os().into(),
            arch: super::platform_arch().into(),
            tier: SHARED.into(),
            ..Node::default()
        },
        true,
    )
}
fn status(hub: &Arc<Hub>, node: v1::NodeInfo) -> Result<v1::NodeStatus> {
    let mut status = hub
        .node_monitor
        .status(&node.name, &node.address, node.invited);
    status.draining = hub
        .node_draining
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(&node.name.to_ascii_lowercase());
    if let Some(metrics) = &mut status.metrics {
        status.alerts.extend(resource_alerts(metrics));
        if node.local {
            metrics.sleeves = hub
                .sleeves
                .list()
                .iter()
                .filter(|row| {
                    row.state == protectorate_sleeve_registry::State::Running && !row.helper
                })
                .map(crate::sleeves::sleeve)
                .collect::<Result<_>>()?;
        }
    }
    status.node = Some(node);
    Ok(status)
}

/// Pushes a fresh certificate for the node's own key once its current one
/// is within RENEW_BEFORE of expiry. The node swaps it in without a restart.
fn renew_if_due(hub: &Arc<Hub>, name: &str) {
    let Ok(node) = hub.inventory.get(name) else {
        return;
    };
    let Some(ca) = hub.ca.clone() else {
        return;
    };
    if node.cert_id.is_empty()
        || node.cert_expires - protectorate_tls::unix_now()
            > protectorate_tls::RENEW_BEFORE.as_secs() as i64
        || !hub.renewing.lock().unwrap().insert(node.name.clone())
    {
        return;
    }
    let hub = Arc::clone(hub);
    std::thread::spawn(move || {
        let result = (|| {
            let key = STANDARD
                .decode(&node.public_key)
                .map_err(|e| e.to_string())?;
            let (certificate, expires) = ca
                .issue_node(&node.name, &node.cert_id, &key)
                .map_err(|e| e.to_string())?;
            hub.node_files
                .internal_client_with_timeout(&node.name, Duration::from_secs(30))
                .and_then(|client| {
                    client.unary(
                        node_agent_service::RENEW_CERTIFICATE,
                        &v1::RenewCertificateRequest { certificate },
                    )
                })
                .map_err(|e| e.message)?;
            hub.inventory
                .renewed(&node.name, &node.cert_id, expires)
                .map_err(|e| e.to_string())
        })();
        if let Err(error) = result {
            eprintln!("renew certificate for {}: {error}", node.name);
        }
        hub.renewing.lock().unwrap().remove(&node.name);
    });
}

fn resource_alerts(metrics: &v1::NodeMetrics) -> Vec<v1::NodeAlert> {
    let mut out = Vec::new();
    for filesystem in &metrics.filesystems {
        let used = filesystem.used_percent;
        if used < DISK_ALERT_PERCENT {
            continue;
        }
        let mount = &filesystem.mount;
        out.push(v1::NodeAlert {
            kind: AlertKind::DiskPercent as i32,
            subject: mount.clone(),
            message: format!("{mount} is {used:.0}% full"),
            value: used,
            threshold: DISK_ALERT_PERCENT,
        });
    }
    if let Some(memory) = &metrics.memory {
        let used = memory.used_percent;
        if used >= MEM_ALERT_PERCENT {
            out.push(v1::NodeAlert {
                kind: AlertKind::MemPressure as i32,
                message: format!("memory is {used:.0}% used"),
                value: used,
                threshold: MEM_ALERT_PERCENT,
                ..Default::default()
            });
        }
    }
    out
}

fn node_error(error: Error) -> RpcError {
    let code = match error {
        Error::Invalid(_) => Code::InvalidArgument,
        Error::Exists(_) => Code::AlreadyExists,
        Error::NotFound(_) => Code::NotFound,
        Error::Invited(_) => Code::FailedPrecondition,
        Error::Io(_) | Error::Json(_) => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}
