use super::{input, respond, string, Hub};
use protectorate_inventory::{Error, Node, CARE, SHARED};
use protectorate_lease::Entry;
use protectorate_node_monitor::PollEvent;
use protectorate_wire::{
    self as wire, Code, Result, RpcError, RpcRequest, StreamSender, UnaryServer,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_DRAIN_WAIT: Duration = Duration::from_secs(3600);
const DRAIN_POLL: Duration = Duration::from_millis(200);
// NodeStatus alerts the hub judges itself from the node's reported figures.
const DISK_ALERT_PERCENT: f64 = 90.0;
const MEM_ALERT_PERCENT: f64 = 90.0;
const INVITE_REFRESH_INTERVAL: Duration = Duration::from_secs(15);
// How often WatchNodeSetup re-reads progress, and how often it kicks the
// monitor's poll while a node is between joining and ready.
const SETUP_POLL: Duration = Duration::from_secs(1);

fn install_commands(base: &str, key: &str, name: &str) -> Result<Vec<Value>> {
    if !(8..=256).contains(&key.len())
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._~+/=-".contains(&b))
    {
        return Err(RpcError::new(
            Code::Internal,
            "the enrollment key has unexpected characters",
        ));
    }
    let sh_key = super::distribution::shell_word(key);
    let sh_name = super::distribution::shell_word(name);
    let sh_url = super::distribution::shell_word(&format!("{base}/install.sh"));
    let termux_url = super::distribution::shell_word(&format!("{base}/install-termux.sh"));
    let ps_key = super::distribution::powershell_word(key);
    let ps_name = super::distribution::powershell_word(name);
    let ps_url = super::distribution::powershell_word(&format!("{base}/install.ps1"));
    Ok(vec![
        json!({
            "platform": "INSTALL_PLATFORM_WINDOWS", "shell": "PowerShell",
            "command": format!("$ErrorActionPreference='Stop'; $env:PROTECTORATE_ENROLL_KEY={ps_key}; $env:PROTECTORATE_NODE_NAME={ps_name}; irm {ps_url} | iex"),
            "note": "Paste into a normal PowerShell as the signed-in user, not as administrator. Installs a logon task in that user's desktop session. x64 and ARM64.",
        }),
        json!({
            "platform": "INSTALL_PLATFORM_LINUX", "shell": "sh",
            "command": format!("script=$(curl -fsSL {sh_url}) && [ -n \"$script\" ] && printf '%s\\n' \"$script\" | PROTECTORATE_ENROLL_KEY={sh_key} sh -s -- --name {sh_name}"),
            "note": "Paste into a terminal as the user the agent should run as, not root. Needs curl and installs a systemd user unit. To keep it running while logged out, enable linger for that user.",
        }),
        json!({
            "platform": "INSTALL_PLATFORM_TERMUX", "shell": "Termux",
            "command": format!("script=$(curl -fsSL {termux_url}) && [ -n \"$script\" ] && printf '%s\\n' \"$script\" | PROTECTORATE_ENROLL_KEY={sh_key} sh -s -- --name {sh_name}"),
            "note": "Paste into Termux on Android aarch64. For boot startup, install termux-services, restart Termux, and open Termux:Boot once.",
        }),
    ])
}

#[derive(Default)]
pub(super) struct InviteMismatches {
    last_refresh: Option<Instant>,
    refreshing: bool,
    // Name -> (invite key, actual tailnet name).
    names: BTreeMap<String, (u64, String)>,
}

struct InviteRefresh<'a>(&'a Hub);
impl Drop for InviteRefresh<'_> {
    fn drop(&mut self) {
        self.0.invite_mismatches.lock().unwrap().refreshing = false;
    }
}

pub(super) fn start_invite_refresh(hub: &Arc<Hub>) {
    if hub.tailnet_admin.is_none() {
        return;
    }
    let weak = Arc::downgrade(hub);
    if let Err(error) = std::thread::Builder::new()
        .name("node-invite-refresh".into())
        .spawn(move || loop {
            std::thread::sleep(INVITE_REFRESH_INTERVAL);
            let Some(hub) = weak.upgrade() else {
                break;
            };
            refresh_invite_mismatches_blocking(&hub);
        })
    {
        eprintln!("node invites: start refresh: {error}");
    }
}

async fn refresh_invite_mismatches(hub: Arc<Hub>) {
    if let Err(error) =
        tokio::task::spawn_blocking(move || refresh_invite_mismatches_blocking(&hub)).await
    {
        eprintln!("node invites: refresh: {error}");
    }
}

fn refresh_invite_mismatches_blocking(hub: &Hub) {
    let Some(admin) = hub.tailnet_admin.clone() else {
        return;
    };
    let invited = hub
        .inventory
        .list()
        .into_iter()
        .filter(|node| node.invited && node.invite_key_id != 0)
        .map(|node| (node.name, node.invite_key_id))
        .collect::<BTreeMap<_, _>>();
    {
        let mut cache = hub.invite_mismatches.lock().unwrap();
        if cache.refreshing
            || cache
                .last_refresh
                .is_some_and(|last| last.elapsed() < INVITE_REFRESH_INTERVAL)
        {
            return;
        }
        cache.refreshing = true;
        cache.last_refresh = Some(Instant::now());
    }
    let _refresh = InviteRefresh(hub);
    let peers = if invited.is_empty() {
        Vec::new()
    } else {
        match admin.nodes() {
            Ok(peers) => peers,
            Err(error) => {
                eprintln!("node invites: list tailnet peers: {error}");
                return;
            }
        }
    };
    let current = hub
        .inventory
        .list()
        .into_iter()
        .filter(|node| node.invited && node.invite_key_id != 0)
        .map(|node| (node.name, node.invite_key_id))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    for (name, key) in invited {
        if current.get(&name) != Some(&key) {
            continue;
        }
        if let Some(peer) = peers.iter().find(|peer| peer.enrolled_with() == Some(key)) {
            let joined_as = &peer.given_name;
            if !joined_as.is_empty() && joined_as != &name {
                found.insert(name, (key, joined_as.clone()));
            }
        }
    }
    let changes = {
        let mut cache = hub.invite_mismatches.lock().unwrap();
        let changes = found
            .iter()
            .filter(|(name, value)| cache.names.get(*name) != Some(*value))
            .map(|(name, (_, joined_as))| (name.clone(), joined_as.clone()))
            .collect::<Vec<_>>();
        cache.names = found;
        changes
    };
    for (name, joined_as) in changes {
        hub.emit(
            "node.name_mismatch",
            &name,
            json!({"invited_as": name, "joined_as": joined_as}),
        );
    }
}

fn invalidate_invite_mismatch(hub: &Hub, name: &str) {
    let mut cache = hub.invite_mismatches.lock().unwrap();
    cache.names.remove(name);
    cache.last_refresh = None;
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
            refresh_invite_mismatches(Arc::clone(&hub)).await;
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
            refresh_invite_mismatches(Arc::clone(&hub)).await;
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
            invalidate_invite_mismatch(&hub, &node.name);
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
            let tailnet = hub.tailnet_admin.clone();
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
                let retagged = if let Some(admin) = &tailnet {
                    admin.set_tier(&name_for_store, &tier).map_err(|error| {
                        RpcError::new(Code::Unavailable, format!("retag {name_for_store} on the tailnet: {error}"))
                    })?
                } else {
                    false
                };
                match inventory.set_tier(&name_for_store, &tier) {
                    Ok(node) => Ok(node),
                    Err(error) => {
                        if retagged {
                            if let Some(admin) = tailnet {
                                if let Err(rollback) = admin.set_tier(&name_for_store, &prev.tier) {
                                    eprintln!("node tier {name_for_store}: restore tailnet tag {}: {rollback}", prev.tier);
                                }
                            }
                        }
                        Err(node_error(error))
                    }
                }
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
        refresh_invite_mismatches(Arc::clone(&hub)).await;
        let node = hub.inventory.get(&name).map_err(node_error)?;
        let status = status(&hub, node_json(&node, false));
        let progress = setup_progress(&hub, &node, status).await;
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

async fn setup_progress(hub: &Hub, node: &Node, status: Value) -> Value {
    let mut problems = Vec::<String>::new();
    let mut notes = Vec::<String>::new();
    let (mut stage, message) = if node.invited {
        let enrolled = if node.invite_key_id != 0 {
            if let Some(admin) = hub.tailnet_admin.clone() {
                let name = node.name.clone();
                let invite_key = node.invite_key_id;
                tokio::task::spawn_blocking(move || {
                    admin.nodes().is_ok_and(|nodes| {
                        nodes.iter().any(|candidate| {
                            (candidate.given_name == name || candidate.name == name)
                                && candidate.enrolled_with() == Some(invite_key)
                        })
                    })
                })
                .await
                .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        if enrolled {
            (
                "NODE_SETUP_STAGE_JOINED",
                "enrolled in the tailnet; waiting for the hub to reach the agent".to_owned(),
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
    let admin = hub.tailnet_admin.clone().ok_or_else(|| {
        RpcError::new(
            Code::FailedPrecondition,
            "this hub has no tailnet: run `protectorate hub tailnet init` and restart the hub",
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
    let pools = match body.get("pools") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| RpcError::invalid("pools must be an array"))?
            .iter()
            .map(|pool| {
                pool.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| RpcError::invalid("pool must be a string"))
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let base = install_base(
        &hub,
        &request.headers,
        body.get("installUrl").and_then(Value::as_str).unwrap_or(""),
    )?;
    if name == hub.hostname {
        return Err(RpcError::new(
            Code::AlreadyExists,
            format!("{name} is the hub's own node"),
        ));
    }
    let inventory = Arc::clone(&hub.inventory);
    let admission = Arc::clone(&hub.node_admission);
    let draining = Arc::clone(&hub.node_draining);
    let configured_pools = hub.tailnet_pools.clone();
    let hub_name = hub
        .tailnet_hub_name
        .clone()
        .unwrap_or_else(|| hub.hostname.clone());
    let control_url = admin.control_url().to_owned();
    let name_for_store = name.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _admission = admission.write().unwrap_or_else(|error| error.into_inner());
        if draining.lock().unwrap().contains(&name_for_store) {
            return Err(RpcError::new(Code::FailedPrecondition, "node is being removed"));
        }
        let prev = match inventory.get(&name_for_store) {
            Ok(node) if !node.invited => return Err(RpcError::new(Code::AlreadyExists, format!("{name_for_store} is already in the inventory: remove it first"))),
            Ok(node) => Some(node),
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(node_error(error)),
        };
        let issued = admin.invite_node(&name_for_store, &hub_name, &tier, &pools, &configured_pools, ttl)
            .map_err(|error| RpcError::new(Code::FailedPrecondition, error.to_string()))?;
        let commands = match install_commands(&base, &issued.key, &name_for_store) {
            Ok(commands) => commands,
            Err(error) => {
                if let Err(expire) = admin.expire_key(issued.id) {
                    eprintln!("invite {name_for_store}: expire unusable key {}: {expire}", issued.id);
                }
                return Err(error);
            }
        };
        let node = Node {
            name: name_for_store.clone(),
            address: name_for_store.clone(),
            tier,
            tags: pools.iter().map(|pool| format!("pool-{pool}")).collect(),
            invited: true,
            invite_key_id: issued.id,
            ..Node::default()
        };
        let node = match inventory.add(node, prev.is_some()) {
            Ok(node) => node,
            Err(error) => {
                if let Err(expire) = admin.expire_key(issued.id) { eprintln!("invite {name_for_store}: expire unused key {}: {expire}", issued.id); }
                return Err(node_error(error));
            }
        };
        if let Some(prev) = prev {
            if prev.invite_key_id != 0 && prev.invite_key_id != issued.id {
                if let Err(error) = admin.expire_key(prev.invite_key_id) {
                    eprintln!("invite {name_for_store}: expire previous key {}: {error}", prev.invite_key_id);
                }
            }
        }
        Ok(json!({
            "node": node_json(&node, false),
            "controlUrl": control_url,
            "key": issued.key,
            "expiresAt": super::timestamp(chrono::Utc::now() + chrono::Duration::from_std(ttl).expect("bounded ttl")),
            "installUrl": base,
            "commands": commands,
        }))
    }).await.map_err(super::join_error)??;
    hub.emit("node.invited", &name, Value::Null);
    invalidate_invite_mismatch(&hub, &name);
    respond(&request, result)
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

fn install_base(hub: &Hub, headers: &axum::http::HeaderMap, asked: &str) -> Result<String> {
    let from_header;
    let raw = if asked.is_empty() {
        if let Some(public_url) = &hub.public_url {
            return Ok(public_url.clone());
        }
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| RpcError::invalid("install URL is unavailable: missing Host header"))?;
        let scheme = if headers
            .get("x-forwarded-proto")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|s| s.eq_ignore_ascii_case("https"))
        {
            "https"
        } else {
            "http"
        };
        from_header = format!("{scheme}://{host}");
        &from_header
    } else {
        asked
    };
    validate_install_base(raw)
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
            let address = info.address.clone();
            match tokio::task::spawn_blocking(move || jobs.uninstall_node(&address))
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
    let revoke_message = if let Some(admin) = &hub.tailnet_admin {
        let admin = Arc::clone(admin);
        let name = name.clone();
        let key_id = info.invite_key_id;
        let invited = info.invited;
        tokio::task::spawn_blocking(move || {
            let mut actions = Vec::new();
            if invited && key_id != 0 {
                admin.expire_key(key_id).map_err(|error| {
                    RpcError::new(Code::Unavailable, format!("expire invite key: {error}"))
                })?;
                actions.push("expired the invite key".to_owned());
            }
            let revoked = admin.revoke(&name).map_err(|error| {
                RpcError::new(Code::Unavailable, format!("revoke tailnet node: {error}"))
            })?;
            actions.push(if revoked {
                "revoked the tailnet enrollment".to_owned()
            } else {
                "not enrolled in the tailnet".to_owned()
            });
            Ok::<_, RpcError>(actions.join("; "))
        })
        .await
        .map_err(super::join_error)??
    } else {
        "no tailnet revoker is configured on this hub; no tailnet enrollment was revoked".to_owned()
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
    invalidate_invite_mismatch(&hub, &name);
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
    if let Some(id) = object.get_mut("inviteKeyId") {
        if let Some(s) = id.as_str() {
            *id = json!(s
                .parse::<u64>()
                .map_err(|_| RpcError::new(Code::InvalidArgument, "invalid inviteKeyId"))?);
        }
    }
    serde_json::from_value(value)
        .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))
}

fn node_json(node: &Node, local: bool) -> Value {
    let mut out = serde_json::to_value(node).expect("node serialization");
    out["inviteKeyId"] = json!(node.invite_key_id.to_string());
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
        "node.metrics" => metrics_summary(&status),
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
    if node["invited"] == true {
        let key = node["inviteKeyId"]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok());
        let cache = hub.invite_mismatches.lock().unwrap();
        if let Some((_, joined_as)) = cache
            .names
            .get(name)
            .filter(|(cached, _)| Some(*cached) == key)
        {
            status["error"] = json!(format!(
                "joined the tailnet as {joined_as:?}, not the invited name {name:?}"
            ));
            push_alert(
                &mut status,
                json!({
                    "kind": "ALERT_KIND_NAME_MISMATCH",
                    "subject": joined_as,
                    "message": format!("invited as {name:?} but joined as {joined_as:?}: `node rm {name}`, then `node add {joined_as} --addr {joined_as}`"),
                }),
            );
        }
    }
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

#[cfg(test)]
mod public_url_tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn configured_origin_is_default_but_explicit_invite_origin_wins() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::open(dir.path(), "owner-secret", "test")
            .unwrap()
            .with_public_url("https://public.example.test:7480/".into())
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("host", "internal.example.test:7470".parse().unwrap());
        assert_eq!(
            install_base(&hub, &headers, "").unwrap(),
            "https://public.example.test:7480"
        );
        assert_eq!(
            install_base(&hub, &headers, "https://other.example.test").unwrap(),
            "https://other.example.test"
        );
        assert!(hub
            .with_public_url("https://user@bad.example".into())
            .is_err());
    }
}
