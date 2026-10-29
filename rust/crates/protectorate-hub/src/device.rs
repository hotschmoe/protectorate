//! Device management. A device is a named bearer token the owner or another
//! device adds; the hub keeps only its hash.
use super::{unix_time, Hub};
use protectorate_audit::Entry as AuditEntry;
use protectorate_fsutil::unix_seconds;
use protectorate_identity::{self as identity, Device};
use protectorate_wire::{
    v1::{self, device_service},
    Call, Code, Result, RpcError, UnaryServer,
};
use serde_json::json;
use std::sync::Arc;

fn device_info(device: &Device) -> v1::DeviceInfo {
    v1::DeviceInfo {
        id: device.id.clone(),
        name: device.name.clone(),
        added_at: Some(unix_time(device.added_at)),
        added_by: device.added_by.clone(),
        last_used_at: device.last_used.map(unix_time),
    }
}

fn error(error: identity::Error) -> RpcError {
    let code = match error {
        identity::Error::Invalid(_) => Code::InvalidArgument,
        identity::Error::UnknownToken => Code::NotFound,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let add = Arc::clone(&hub);
    server.unary(
        device_service::ADD_DEVICE,
        move |call: Call<v1::AddDeviceRequest>| {
            let hub = Arc::clone(&add);
            async move {
                let (device, token) = hub
                    .identity
                    .add_device(call.request.name.trim(), &call.auth.actor, unix_seconds())
                    .map_err(error)?;
                hub.audit.write(AuditEntry {
                    actor: call.auth.actor.clone(),
                    token_id: call.auth.token_id.clone(),
                    task: call.auth.task.clone(),
                    procedure: call.method.procedure,
                    subject: device.id.clone(),
                    outcome: "ok".into(),
                    detail: format!("name={}", device.name),
                    ..AuditEntry::default()
                });
                hub.emit("devices.changed", &device.id, json!({}));
                Ok(v1::AddDeviceResponse {
                    device: Some(device_info(&device)),
                    token: token.expose().to_owned(),
                })
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
