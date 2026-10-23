use super::{input, respond, string, Hub};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_inventory::{Error, Node, CARE, SHARED};
use protectorate_lease::Entry;
use protectorate_node_monitor::PollEvent;
use protectorate_wire::{
    self as wire, Code, Result, RpcError, RpcRequest, StreamSender, UnaryServer,
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
fn install_commands(base: &str, code: &str, name: &str, ca_sha256: &str) -> Vec<Value> {
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
    vec![
        json!({
            "platform": "INSTALL_PLATFORM_WINDOWS", "shell": "Windows PowerShell",
            "command": windows,
            "note": "Paste into Windows PowerShell (powershell.exe, not pwsh) as the signed-in user, not as administrator. Installs a logon task in that user's desktop session. x64 and ARM64.",
        }),
        json!({
            "platform": "INSTALL_PLATFORM_LINUX", "shell": "sh",
            "command": sh("install.sh"),
            "note": "Paste into a terminal as the user the agent should run as, not root. Needs curl and installs a systemd user unit. To keep it running while logged out, enable linger for that user.",
        }),
        json!({
            "platform": "INSTALL_PLATFORM_TERMUX", "shell": "Termux",
            "command": sh("install-termux.sh"),
            "note": "Paste into Termux on Android aarch64. For boot startup, install termux-services, restart Termux, and open Termux:Boot once.",
        }),
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
    server.register("/protectorate.v1.NodeService/ListNodes", move |request| {
        let hub = Arc::clone(&list);
        async move {
            let mut nodes = vec![status(&hub, local_node(&hub))];
            nodes.extend(
                hub.inventory
                    .list()
                    .iter()
                    .map(|n| status(&hub, node_json(n, false))),
            );
            respond(&request, json!({"nodes": nodes}))
        }
    })?;
    let get = Arc::clone(&hub);
    server.register("/protectorate.v1.NodeService/GetNode", move |request| {
        let hub = Arc::clone(&get);
        async move {
            let name = string(&input(&request)?, "name");
            let node = if name.eq_ignore_ascii_case(&hub.hostname) {
                local_node(&hub)
            } else {
                node_json(&hub.inventory.get(&name).map_err(node_error)?, false)
            };
            respond(&request, status(&hub, node))
        }
    })?;
    let add = Arc::clone(&hub);
    server.register("/protectorate.v1.NodeService/AddNode", move |request| {
        let hub = Arc::clone(&add);
        async move {
            let value = input(&request)?;
            let raw = value
                .get("node")
                .cloned()
                .ok_or_else(|| RpcError::new(Code::InvalidArgument, "node is required"))?;
            let node = parse_node(raw)?;
            let update = value
                .get("update")
                .and_then(Value::as_bool)
                .unwrap_or(false);
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
            let monitor = Arc::clone(&hub.node_monitor);
            let name = node.name.clone();
            tokio::task::spawn_blocking(move || monitor.poll_node(&name));
            respond(&request, node_json(&node, false))
        }
    })?;
    let invite = Arc::clone(&hub);
    server.register("/protectorate.v1.NodeService/InviteNode", move |request| {
        let hub = Arc::clone(&invite);
        async move { invite_node(hub, request).await }
    })?;
    let join = Arc::clone(&hub);
    server.register("/protectorate.v1.NodeService/JoinNode", move |request| {
        let hub = Arc::clone(&join);
        async move { join_node(hub, request).await }
    })?;
    let set_tier = Arc::clone(&hub);
    server.register("/protectorate.v1.NodeService/SetNodeTier", move |request| {
        let hub = Arc::clone(&set_tier);
        async move {
            let value = input(&request)?;
            let name = string(&value, "name").trim().to_ascii_lowercase();
            let tier = string(&value, "tier").trim().to_ascii_lowercase();
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
            respond(&request, node_json(&node, false))
        }
    })?;
    let setup = Arc::clone(&hub);
    server.register_streaming(
        "/protectorate.v1.NodeService/RemoveNode",
        move |request, sender| {
            let hub = Arc::clone(&hub);
            async move { remove_node(hub, request, sender).await }
        },
    )?;
    server.register_streaming(
        "/protectorate.v1.NodeService/WatchNodeSetup",
        move |request, sender| {
            let hub = Arc::clone(&setup);
            async move { watch_node_setup(hub, request, sender).await }
        },
    )?;
    Ok(())
}

async fn watch_node_setup(
    hub: Arc<Hub>,
    request: RpcRequest,
    sender: StreamSender,
) -> Result<wire::StreamTrailers> {
    let name = string(&input(&request)?, "name");
    if name.is_empty() {
        return Err(RpcError::invalid("node name is required"));
    }
    hub.inventory.get(&name).map_err(node_error)?;
    let mut last = None;
    // A node that just joined is only seen when the hub next polls it. Kick
    // the poll from the watch, as Go's WatchNodeSetup does, so the setup flow
    // does not sit at JOINED for a whole poll interval.
    let mut kicked = None::<Instant>;
    loop {
        let node = hub.inventory.get(&name).map_err(node_error)?;
        let status = status(&hub, node_json(&node, false));
        let progress = setup_progress(&hub, &node, status);
        let stage = string(&progress, "stage").to_owned();
        let changed = last.as_ref().is_none_or(|previous: &Value| {
            ["stage", "message", "problems", "notes"]
                .iter()
                .any(|field| previous[*field] != progress[*field])
        });
        if changed {
            let bytes = serde_json::to_vec(&progress)
                .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
            sender
                .send(wire::decode_json(request.method.output(), &bytes)?)
                .await?;
            last = Some(progress);
        }
        if stage == "NODE_SETUP_STAGE_READY" {
            return Ok(Default::default());
        }
        if stage != "NODE_SETUP_STAGE_INVITED"
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

fn setup_progress(hub: &Hub, node: &Node, status: Value) -> Value {
    let mut problems = Vec::<String>::new();
    let mut notes = Vec::<String>::new();
    let (mut stage, message) = if node.invited {
        if !node.cert_id.is_empty() {
            (
                "NODE_SETUP_STAGE_JOINED",
                format!(
                    "joined; waiting for the hub to reach the agent at {}",
                    node.address
                ),
            )
        } else {
            (
                "NODE_SETUP_STAGE_INVITED",
                "waiting for the machine to run the installer".to_owned(),
            )
        }
    } else if status["metrics"].is_null() {
        (
            "NODE_SETUP_STAGE_JOINED",
            status["error"]
                .as_str()
                .filter(|error| !error.is_empty())
                .unwrap_or("waiting for the hub to reach the agent")
                .to_owned(),
        )
    } else {
        let metrics = &status["metrics"];
        if status["freshness"] != "NODE_FRESHNESS_FRESH" {
            problems.push(format!(
                "the hub cannot reach the agent now: {}",
                status["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                    .unwrap_or("no recent poll")
            ));
        }
        let version = string(metrics, "agentVersion");
        if !version.is_empty() && version != hub.version {
            problems.push(format!(
                "the agent runs {version} and the hub {}: update the node",
                hub.version
            ));
        }
        if let Some(warnings) = metrics["warnings"].as_array() {
            notes.extend(warnings.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        (
            if !problems.is_empty() {
                "NODE_SETUP_STAGE_ATTENTION"
            } else {
                "NODE_SETUP_STAGE_READY"
            },
            if !problems.is_empty() {
                "the node answers but needs attention".to_owned()
            } else {
                format!(
                    "{}/{}, agent {version}",
                    string(metrics, "os"),
                    string(metrics, "arch")
                )
            },
        )
    };
    if let Some(alerts) = status["alerts"].as_array() {
        for alert in alerts {
            if let Some(message) = alert["message"].as_str() {
                if node.invited {
                    problems.push(message.to_owned());
                } else if alert["kind"] != "ALERT_KIND_UNREACHABLE" {
                    notes.push(message.to_owned());
                }
            }
        }
    }
    if node.invited && !problems.is_empty() {
        stage = "NODE_SETUP_STAGE_ATTENTION";
    }
    json!({"stage":stage,"message":message,"problems":problems,"notes":notes,"status":status})
}

async fn invite_node(hub: Arc<Hub>, request: RpcRequest) -> Result<wire::RpcResponse> {
    let body = input(&request)?;
    let name = string(&body, "name");
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
    let tier = body
        .get("tier")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(SHARED)
        .to_owned();
    if tier != SHARED && tier != CARE {
        return Err(RpcError::invalid("tier must be shared or care"));
    }
    let ttl = match body.get("ttl") {
        None => Duration::from_secs(3600),
        some => super::parse_duration(some)?,
    };
    if ttl.is_zero() || ttl > Duration::from_secs(24 * 3600) {
        return Err(RpcError::invalid("ttl must be positive and at most 24h"));
    }
    let base = install_base(
        &hub,
        body.get("installUrl").and_then(Value::as_str).unwrap_or(""),
    )?;
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
    let result = json!({
        "node": node_json(&node, false),
        "hubUrl": base,
        "code": code,
        "expiresAt": super::unix_timestamp(expires),
        "installUrl": base,
        "commands": install_commands(&base, &code, &name, &ca_sha256),
        "caSha256": ca_sha256,
    });
    hub.node_monitor.invalidate(&name);
    hub.emit("node.invited", &name, Value::Null);
    respond(&request, result)
}

async fn join_node(hub: Arc<Hub>, request: RpcRequest) -> Result<wire::RpcResponse> {
    let body = input(&request)?;
    let name = string(&body, "name");
    let code = string(&body, "code");
    let ca = hub.ca.clone().ok_or_else(|| {
        RpcError::new(
            Code::FailedPrecondition,
            "this hub has no CA to issue node certificates",
        )
    })?;
    let bytes = |field: &str| {
        STANDARD
            .decode(string(&body, field))
            .map_err(|_| RpcError::invalid(format!("{field} must be base64")))
    };
    let public_key = bytes("publicKey")?;
    if !protectorate_tls::verify_proof(
        &public_key,
        &protectorate_tls::join_message(&name, &code),
        &bytes("proof")?,
    ) {
        return Err(RpcError::invalid(
            "proof must sign the join message with the uncompressed P-256 key",
        ));
    }
    let address: std::net::SocketAddr = string(&body, "address")
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
    respond(&request, json!({"certificate": certificate}))
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
    request: RpcRequest,
    sender: StreamSender,
) -> Result<wire::StreamTrailers> {
    let body = input(&request)?;
    let name = string(&body, "name").trim().to_ascii_lowercase();
    if name == hub.hostname {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "the hub's own node cannot be removed",
        ));
    }
    let _drain = start_drain(&hub, &name)?;
    let info = hub.inventory.get(&name).map_err(node_error)?;
    let jobs = string(&body, "jobs");
    let wait = super::parse_duration(body.get("waitTimeout"))?;
    let wait = if wait.is_zero() {
        DEFAULT_DRAIN_WAIT
    } else {
        wait
    };
    let mut ids = hub.jobs.active_on(&name);
    match jobs.as_str() {
        "REMOVE_JOBS_CANCEL" => {
            if !ids.is_empty() {
                progress(
                    &request,
                    &sender,
                    "REMOVE_PHASE_DRAINING",
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
        "REMOVE_JOBS_WAIT" => {}
        "" | "REMOVE_JOBS_UNSPECIFIED" if !ids.is_empty() => {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!(
                    "{} job(s) running on {name} ({}): wait for them or cancel them",
                    ids.len(),
                    ids.join(", ")
                ),
            ));
        }
        "" | "REMOVE_JOBS_UNSPECIFIED" => {}
        _ => return Err(RpcError::invalid("invalid jobs mode")),
    }
    if ids.is_empty() {
        progress(
            &request,
            &sender,
            "REMOVE_PHASE_DRAINING",
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
                    &request,
                    &sender,
                    "REMOVE_PHASE_DRAINING",
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
        progress(
            &request,
            &sender,
            "REMOVE_PHASE_DRAINING",
            0,
            "no jobs running",
        )
        .await?;
    }
    if body.get("uninstall").and_then(Value::as_bool) == Some(true) {
        let message = if info.invited || !hub.node_monitor.is_fresh(&name, &info.address) {
            "not uninstalled: the node is not reachable now; run `protectorate node uninstall` on the machine".to_owned()
        } else {
            let jobs = Arc::clone(&hub.jobs);
            let node = name.clone();
            match tokio::task::spawn_blocking(move || jobs.uninstall_node(&node))
                .await.map_err(super::join_error)? {
                Ok(reply) => {
                    let actions = reply["actions"].as_array().into_iter().flatten()
                        .filter_map(Value::as_str).collect::<Vec<_>>().join("; ");
                    let left = reply["left"].as_array().into_iter().flatten()
                        .filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
                    if left.is_empty() { format!("the agent unregisters its startup and stops: {actions}") }
                    else { format!("the agent unregisters its startup and stops: {actions}. Nothing is deleted; left on the machine: {left}") }
                }
                Err(error) if error.code == Code::Unimplemented =>
                    "not uninstalled: the agent cannot uninstall itself; run `protectorate node uninstall` on the machine".to_owned(),
                Err(error) => return Err(RpcError::new(error.code, format!("uninstall on {name}: {}", error.message))),
            }
        };
        progress(&request, &sender, "REMOVE_PHASE_UNINSTALLING", 0, message).await?;
    }
    let revoke_message = if info.invited && info.cert_id.is_empty() {
        "the unused join code stops working"
    } else {
        "the node certificate is no longer trusted"
    };
    progress(
        &request,
        &sender,
        "REMOVE_PHASE_REVOKING",
        0,
        revoke_message,
    )
    .await?;
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
    progress(&request, &sender, "REMOVE_PHASE_REMOVED", 0, message).await?;
    Ok(Default::default())
}

async fn progress(
    request: &RpcRequest,
    sender: &StreamSender,
    phase: &str,
    running: usize,
    message: impl Into<String>,
) -> Result<()> {
    let value = json!({"phase":phase,"runningJobs":running,"message":message.into()});
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
    sender
        .send(wire::decode_json(request.method.output(), &bytes)?)
        .await
}

fn lease_error(error: protectorate_lease::Error) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}

fn parse_node(mut value: Value) -> Result<Node> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "node must be an object"))?;
    object.remove("local");
    serde_json::from_value(value)
        .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))
}

fn node_json(node: &Node, local: bool) -> Value {
    let mut out = serde_json::to_value(node).expect("node serialization");
    if let Some(object) = out.as_object_mut() {
        for private in [
            "codeSha256",
            "codeExpires",
            "certId",
            "certExpires",
            "publicKey",
        ] {
            object.remove(private);
        }
    }
    if local {
        out["local"] = json!(true);
    }
    out
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
        node_json(&node, false)
    };
    let status = status(hub, node);
    let payload = match event.kind {
        "node.metrics" => {
            renew_if_due(hub, &event.name);
            metrics_summary(&status)
        }
        "node.unreachable" => json!({
            "error": status["error"],
            "address": event.address,
        }),
        _ => return,
    };
    hub.emit(event.kind, &event.name, payload);
}

fn metrics_summary(status: &Value) -> Value {
    let metrics = &status["metrics"];
    let mut summary = json!({
        "freshness": status["freshness"].as_str().unwrap_or_default()
            .trim_start_matches("NODE_FRESHNESS_").to_ascii_lowercase(),
        "alerts": status["alerts"].as_array().into_iter().flatten()
            .filter_map(|alert| alert["kind"].as_str())
            .map(|kind| kind.trim_start_matches("ALERT_KIND_").to_ascii_lowercase())
            .collect::<Vec<_>>(),
        "gpu_count": metrics["gpus"].as_array().map_or(0, Vec::len),
        "containers": metrics["engine"]["containers"].as_array().map_or(0, Vec::len),
    });
    if let Some(cpu) = metrics["cpu"]["usagePercent"].as_f64() {
        summary["cpu_percent"] = json!(cpu);
    }
    if let Some(mem) = metrics["memory"]["usedPercent"].as_f64() {
        summary["mem_percent"] = json!(mem);
    }
    if let Some(worst) = metrics["filesystems"]
        .as_array()
        .into_iter()
        .flatten()
        .reduce(|worst, next| {
            if next["usedPercent"].as_f64().unwrap_or(0.0)
                > worst["usedPercent"].as_f64().unwrap_or(0.0)
            {
                next
            } else {
                worst
            }
        })
    {
        summary["worst_disk_percent"] = worst["usedPercent"].clone();
        summary["worst_disk_mount"] = worst["mount"].clone();
    }
    summary
}
fn local_node(hub: &Hub) -> Value {
    node_json(
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
fn status(hub: &Arc<Hub>, node: Value) -> Value {
    let name = node["name"].as_str().unwrap_or_default();
    let address = node["address"].as_str().unwrap_or_default();
    let mut status = hub
        .node_monitor
        .status(name, address, node["invited"] == true);
    if hub
        .node_draining
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(&name.to_ascii_lowercase())
    {
        status["draining"] = json!(true);
    }
    if status["metrics"].is_object() {
        let alerts = resource_alerts(&status["metrics"]);
        for alert in alerts {
            push_alert(&mut status, alert);
        }
    }
    if node["local"] == true && status["metrics"].is_object() {
        let sleeves = hub
            .sleeves
            .list()
            .iter()
            .filter(|row| row.state == protectorate_sleeve_registry::State::Running && !row.helper)
            .map(crate::sleeves::sleeve_json)
            .collect::<Vec<_>>();
        status["metrics"]["sleeves"] = json!(sleeves);
    }
    status["node"] = node;
    status
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
                    client.call(
                        "/protectorate.v1.NodeAgentService/RenewCertificate",
                        &json!({"certificate": certificate}),
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

fn push_alert(status: &mut Value, alert: Value) {
    match status["alerts"].as_array_mut() {
        Some(existing) => existing.push(alert),
        None => status["alerts"] = json!([alert]),
    }
}

fn percent(metrics: &Value, field: &str) -> f64 {
    metrics.get(field).and_then(Value::as_f64).unwrap_or(0.0)
}

fn resource_alerts(metrics: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for filesystem in metrics["filesystems"].as_array().into_iter().flatten() {
        let used = percent(filesystem, "usedPercent");
        if used < DISK_ALERT_PERCENT {
            continue;
        }
        let mount = string(filesystem, "mount");
        out.push(json!({
            "kind": "ALERT_KIND_DISK_PERCENT",
            "subject": mount,
            "message": format!("{mount} is {used:.0}% full"),
            "value": used,
            "threshold": DISK_ALERT_PERCENT,
        }));
    }
    let memory = &metrics["memory"];
    let used = percent(memory, "usedPercent");
    if memory.is_object() && used >= MEM_ALERT_PERCENT {
        out.push(json!({
            "kind": "ALERT_KIND_MEM_PRESSURE",
            "message": format!("memory is {used:.0}% used"),
            "value": used,
            "threshold": MEM_ALERT_PERCENT,
        }));
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
