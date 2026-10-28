//! Device pairing and trusted-device management. A device registers its
//! public key when it starts pairing and signs every later request.
use super::{unix_time, Hub};
use protectorate_audit::Entry as AuditEntry;
use protectorate_fsutil::unix_seconds;
use protectorate_identity::{self as identity, Device, PollStatus};
use protectorate_wire::{
    v1::{self, device_service},
    AuthContext, Call, Code, Result, RpcError, UnaryServer,
};
use serde_json::json;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const START: &str = device_service::START_DEVICE_AUTH.procedure;
const POLL: &str = device_service::POLL_DEVICE_AUTH.procedure;
const EXPIRY_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub(super) fn start_expiry_sweep(hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    if let Err(error) = std::thread::Builder::new()
        .name("device-auth-expiry".into())
        .spawn(move || loop {
            let Some(hub) = weak.upgrade() else {
                break;
            };
            sweep_expired(&hub, unix_seconds());
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

/// Procedures callable without a token: device pairing.
pub(super) fn public_procedure(procedure: &str) -> bool {
    procedure == START || procedure == POLL
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
                "pairing is available on the private network only",
            ));
        }
        let (kind, local_limit, global_limit) = match procedure {
            START => ("start", 10, 60),
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

fn device_info(device: &Device) -> v1::DeviceInfo {
    v1::DeviceInfo {
        id: device.id.clone(),
        name: device.name.clone(),
        platform: device.platform.clone(),
        approved_at: Some(unix_time(device.approved_at)),
        approved_by: device.approved_by.clone(),
        last_used_at: device.last_used.map(unix_time),
    }
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
    server.unary(
        device_service::START_DEVICE_AUTH,
        move |call: Call<v1::StartDeviceAuthRequest>| {
            let hub = Arc::clone(&start);
            async move {
                let request = call.request;
                let started = hub
                    .identity
                    .start_device(
                        request.name.trim(),
                        request.platform.trim(),
                        &request.public_key,
                        &call
                            .auth
                            .requesting_ip
                            .map(|ip| ip.to_string())
                            .unwrap_or_else(|| "127.0.0.1".into()),
                        unix_seconds(),
                    )
                    .map_err(error)?;
                hub.emit("device_auth.pending", "", json!({}));
                Ok(v1::StartDeviceAuthResponse {
                    device_code: started.device_code.expose().to_owned(),
                    user_code: started.request.user_code,
                    expires_at: Some(unix_time(started.request.expires)),
                    interval: started.request.interval.try_into().unwrap_or(i32::MAX),
                })
            }
        },
    )?;
    let poll = Arc::clone(&hub);
    server.unary(
        device_service::POLL_DEVICE_AUTH,
        move |call: Call<v1::PollDeviceAuthRequest>| {
            let hub = Arc::clone(&poll);
            async move {
                let code = call.request.device_code;
                if code.len() > 128 {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "device code is too long",
                    ));
                }
                let outcome = hub
                    .identity
                    .poll_device(&code, unix_seconds())
                    .map_err(error)?;
                let status = match outcome.status {
                    PollStatus::Expired => "expired",
                    PollStatus::Pending => "pending",
                    PollStatus::SlowDown => "slow_down",
                    PollStatus::Denied => "denied",
                    PollStatus::Consumed => "consumed",
                    PollStatus::Approved => "approved",
                };
                if let Some(device) = &outcome.device {
                    hub.emit("devices.changed", &device.id, json!({}));
                }
                Ok(v1::PollDeviceAuthResponse {
                    status: status.into(),
                    interval: outcome.interval.try_into().unwrap_or(i32::MAX),
                    device: outcome.device.as_ref().map(device_info),
                })
            }
        },
    )?;
    let pending = Arc::clone(&hub);
    server.unary(device_service::LIST_DEVICE_AUTH, move |_: Call<_>| {
        let hub = Arc::clone(&pending);
        async move {
            Ok(v1::ListDeviceAuthResponse {
                requests: hub
                    .identity
                    .pending_devices(unix_seconds())
                    .into_iter()
                    .map(|r| v1::DeviceAuthInfo {
                        user_code: r.user_code,
                        name: r.name,
                        platform: r.platform,
                        requesting_ip: r.ip,
                        expires_at: Some(unix_time(r.expires)),
                    })
                    .collect(),
            })
        }
    })?;
    let approve = Arc::clone(&hub);
    server.unary(
        device_service::APPROVE_DEVICE_AUTH,
        move |call: Call<v1::ApproveDeviceAuthRequest>| {
            let hub = Arc::clone(&approve);
            async move {
                decide(
                    &hub,
                    &call.auth,
                    call.method.procedure,
                    &call.request.user_code,
                    true,
                )?;
                Ok(v1::ApproveDeviceAuthResponse {})
            }
        },
    )?;
    let deny = Arc::clone(&hub);
    server.unary(
        device_service::DENY_DEVICE_AUTH,
        move |call: Call<v1::DenyDeviceAuthRequest>| {
            let hub = Arc::clone(&deny);
            async move {
                decide(
                    &hub,
                    &call.auth,
                    call.method.procedure,
                    &call.request.user_code,
                    false,
                )?;
                Ok(v1::DenyDeviceAuthResponse {})
            }
        },
    )?;
    let list = Arc::clone(&hub);
    server.unary(device_service::LIST_DEVICES, move |_: Call<_>| {
        let hub = Arc::clone(&list);
        async move {
            Ok(v1::ListDevicesResponse {
                devices: hub.identity.devices().iter().map(device_info).collect(),
            })
        }
    })?;
    let revoke = Arc::clone(&hub);
    server.unary(
        device_service::REVOKE_DEVICE,
        move |call: Call<v1::RevokeDeviceRequest>| {
            let hub = Arc::clone(&revoke);
            async move {
                let device = hub
                    .identity
                    .remove_device(&call.request.id)
                    .map_err(|_| RpcError::new(Code::NotFound, "no such device"))?;
                hub.emit("devices.changed", &device.id, json!({}));
                Ok(v1::RevokeDeviceResponse {
                    device: Some(device_info(&device)),
                })
            }
        },
    )?;
    server.unary(
        device_service::RENAME_DEVICE,
        move |call: Call<v1::RenameDeviceRequest>| {
            let hub = Arc::clone(&hub);
            async move {
                let device = hub
                    .identity
                    .rename_device(&call.request.id, call.request.name.trim())
                    .map_err(error)?;
                hub.emit("devices.changed", &device.id, json!({}));
                Ok(v1::RenameDeviceResponse {
                    device: Some(device_info(&device)),
                })
            }
        },
    )?;
    Ok(())
}

/// Approves or denies a pairing request; only the owner or an approved
/// device decides.
fn decide(
    hub: &Hub,
    auth: &AuthContext,
    procedure: String,
    code: &str,
    approve: bool,
) -> Result<()> {
    if !auth.owner && !auth.device {
        return Err(RpcError::new(
            Code::PermissionDenied,
            "approval requires the owner or an approved device",
        ));
    }
    let decided = hub
        .identity
        .decide_device(code, &auth.actor, approve, unix_seconds())
        .map_err(error)?;
    hub.audit.write(AuditEntry {
        actor: auth.actor.clone(),
        token_id: auth.token_id.clone(),
        task: auth.task.clone(),
        procedure,
        subject: decided.name,
        outcome: "ok".into(),
        detail: format!("platform={} requesting_ip={}", decided.platform, decided.ip),
        ..AuditEntry::default()
    });
    hub.emit("device_auth.changed", "", json!({}));
    Ok(())
}
