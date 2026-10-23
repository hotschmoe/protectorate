//! Device pairing and trusted-device management. A device registers its
//! public key when it starts pairing and signs every later request.
use super::{input, now_unix, respond, string, unix_timestamp, Hub};
use base64::{engine::general_purpose::STANDARD, Engine};
use protectorate_audit::Entry as AuditEntry;
use protectorate_identity::{self as identity, Device, PollStatus};
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::json;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const START: &str = "/protectorate.v1.DeviceService/StartDeviceAuth";
const POLL: &str = "/protectorate.v1.DeviceService/PollDeviceAuth";
const JOIN: &str = "/protectorate.v1.NodeService/JoinNode";
const EXPIRY_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub(super) fn start_expiry_sweep(hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    if let Err(error) = std::thread::Builder::new()
        .name("device-auth-expiry".into())
        .spawn(move || loop {
            let Some(hub) = weak.upgrade() else {
                break;
            };
            sweep_expired(&hub, now_unix());
            drop(hub);
            std::thread::sleep(EXPIRY_SWEEP_INTERVAL);
        })
    {
        eprintln!("device authorization: start expiry sweep: {error}");
    }
}

fn sweep_expired(hub: &Hub, now: i64) {
    if let Err(error) = hub.identity.sweep(now) {
        hub.audit.write(AuditEntry {
            actor: "hub".into(),
            procedure: "device_auth.sweep".into(),
            outcome: "error".into(),
            detail: error.to_string(),
            ..AuditEntry::default()
        });
    }
}

/// Procedures callable without a token: device pairing, and a new node
/// redeeming its single-use join code.
pub(super) fn public_procedure(procedure: &str) -> bool {
    procedure == START || procedure == POLL || procedure == JOIN
}

#[derive(Clone)]
struct Window {
    since: Instant,
    count: u32,
}
impl Default for Window {
    fn default() -> Self {
        Self {
            since: Instant::now(),
            count: 0,
        }
    }
}
#[derive(Default)]
pub(super) struct Rates(Mutex<HashMap<String, Window>>);
impl Rates {
    pub(super) fn admit(&self, procedure: &str, ip: IpAddr) -> Result<()> {
        let ip = normalize_ip(ip);
        if !protectorate_tls::private_ip(ip) {
            return Err(RpcError::new(
                Code::PermissionDenied,
                "pairing and joining are available on the private network only",
            ));
        }
        let (kind, local_limit, global_limit) = match procedure {
            START => ("start", 10, 60),
            JOIN => ("join", 10, 60),
            _ => ("poll", 120, 1200),
        };
        let mut windows = self.0.lock().unwrap_or_else(|e| e.into_inner());
        windows.retain(|_, window| window.since.elapsed() < Duration::from_secs(60));
        let global = format!("{kind}:global");
        let local = format!("{kind}:{ip}");
        if windows
            .get(&global)
            .is_some_and(|w| w.count >= global_limit)
            || windows.get(&local).is_some_and(|w| w.count >= local_limit)
        {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "pairing and join rate limit; retry after one minute",
            ));
        }
        // Every accepted request is bounded by the global window, so at most
        // 1,260 active entries can be created within one minute.
        windows.entry(global).or_default().count += 1;
        windows.entry(local).or_default().count += 1;
        Ok(())
    }
}

pub(super) fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

pub(super) fn source_ip(
    peer: IpAddr,
    headers: &axum::http::HeaderMap,
    trusted_proxies: &[IpAddr],
) -> IpAddr {
    let peer = normalize_ip(peer);
    if !trusted_proxies.contains(&peer) {
        return peer;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .map(normalize_ip)
        .unwrap_or(peer)
}

fn device_json(device: &Device) -> serde_json::Value {
    let mut out = json!({
        "id": device.id, "name": device.name, "platform": device.platform,
        "approvedAt": unix_timestamp(device.approved_at), "approvedBy": device.approved_by,
    });
    if let Some(used) = device.last_used {
        out["lastUsedAt"] = json!(unix_timestamp(used));
    }
    out
}

fn error(error: identity::Error) -> RpcError {
    let code = match error {
        identity::Error::Invalid(_) => Code::InvalidArgument,
        identity::Error::DeviceCapacity => Code::ResourceExhausted,
        identity::Error::DeviceRequest | identity::Error::UnknownToken => Code::NotFound,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let start = Arc::clone(&hub);
    server.register(START, move |request| {
        let hub = Arc::clone(&start);
        async move {
            let body = input(&request)?;
            let key = STANDARD
                .decode(string(&body, "publicKey"))
                .map_err(|_| RpcError::invalid("publicKey must be base64"))?;
            let started = hub
                .identity
                .start_device(
                    string(&body, "name").trim(),
                    string(&body, "platform").trim(),
                    &key,
                    &request
                        .auth
                        .requesting_ip
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| "127.0.0.1".into()),
                    now_unix(),
                )
                .map_err(error)?;
            hub.emit("device_auth.pending", "", json!({}));
            respond(
                &request,
                json!({
                    "deviceCode": started.device_code.expose(),
                    "userCode": started.request.user_code,
                    "expiresAt": unix_timestamp(started.request.expires),
                    "interval": started.request.interval,
                }),
            )
        }
    })?;
    let poll = Arc::clone(&hub);
    server.register(POLL, move |request| {
        let hub = Arc::clone(&poll);
        async move {
            let code = string(&input(&request)?, "deviceCode");
            if code.len() > 128 {
                return Err(RpcError::new(
                    Code::InvalidArgument,
                    "device code is too long",
                ));
            }
            let outcome = hub.identity.poll_device(&code, now_unix()).map_err(error)?;
            let status = match outcome.status {
                PollStatus::Expired => "expired",
                PollStatus::Pending => "pending",
                PollStatus::SlowDown => "slow_down",
                PollStatus::Denied => "denied",
                PollStatus::Consumed => "consumed",
                PollStatus::Approved => "approved",
            };
            let mut body = json!({"status": status, "interval": outcome.interval});
            if let Some(device) = outcome.device {
                hub.emit("devices.changed", &device.id, json!({}));
                body["device"] = device_json(&device);
            }
            respond(&request, body)
        }
    })?;
    let pending = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.DeviceService/ListDeviceAuth",
        move |request| {
            let hub = Arc::clone(&pending);
            async move {
                let requests = hub
                    .identity
                    .pending_devices(now_unix())
                    .into_iter()
                    .map(|r| {
                        json!({
                            "userCode": r.user_code, "name": r.name, "platform": r.platform,
                            "requestingIp": r.ip, "expiresAt": unix_timestamp(r.expires),
                        })
                    })
                    .collect::<Vec<_>>();
                respond(&request, json!({"requests": requests}))
            }
        },
    )?;
    for (procedure, approve) in [
        ("/protectorate.v1.DeviceService/ApproveDeviceAuth", true),
        ("/protectorate.v1.DeviceService/DenyDeviceAuth", false),
    ] {
        let decide = Arc::clone(&hub);
        server.register(procedure, move |request| {
            let hub = Arc::clone(&decide);
            async move {
                if !request.auth.owner && !request.auth.device {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "approval requires the owner or an approved device",
                    ));
                }
                let code = string(&input(&request)?, "userCode");
                let decided = hub
                    .identity
                    .decide_device(&code, &request.auth.actor, approve, now_unix())
                    .map_err(error)?;
                hub.audit.write(AuditEntry {
                    actor: request.auth.actor.clone(),
                    token_id: request.auth.token_id.clone(),
                    task: request.auth.task.clone(),
                    procedure: procedure.into(),
                    subject: decided.name,
                    outcome: "ok".into(),
                    detail: format!("platform={} requesting_ip={}", decided.platform, decided.ip),
                    ..AuditEntry::default()
                });
                hub.emit("device_auth.changed", "", json!({}));
                respond(&request, json!({}))
            }
        })?;
    }
    let list = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.DeviceService/ListDevices",
        move |request| {
            let hub = Arc::clone(&list);
            async move {
                let devices = hub
                    .identity
                    .devices()
                    .iter()
                    .map(device_json)
                    .collect::<Vec<_>>();
                respond(&request, json!({"devices": devices}))
            }
        },
    )?;
    let revoke = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.DeviceService/RevokeDevice",
        move |request| {
            let hub = Arc::clone(&revoke);
            async move {
                let id = string(&input(&request)?, "id");
                let device = hub
                    .identity
                    .remove_device(&id)
                    .map_err(|_| RpcError::new(Code::NotFound, "no such device"))?;
                hub.emit("devices.changed", &device.id, json!({}));
                respond(&request, json!({"device": device_json(&device)}))
            }
        },
    )?;
    server.register(
        "/protectorate.v1.DeviceService/RenameDevice",
        move |request| {
            let hub = Arc::clone(&hub);
            async move {
                let body = input(&request)?;
                let device = hub
                    .identity
                    .rename_device(&string(&body, "id"), string(&body, "name").trim())
                    .map_err(error)?;
                hub.emit("devices.changed", &device.id, json!({}));
                respond(&request, json!({"device": device_json(&device)}))
            }
        },
    )?;
    Ok(())
}
