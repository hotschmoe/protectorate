//! Lease RPCs backed by the persistent lease table. Node admission is supplied
//! by a chooser once the node monitor is connected to this hub.
use super::{input, parse_duration, respond, string, EventBus, Hub};
use chrono::{DateTime, SecondsFormat, Utc};
use protectorate_inventory::{Inventory, SHARED};
use protectorate_lease::{self as leases, Chooser, Entry, Event, EventKind, LeaseTable, Wait};
use protectorate_mailbox::{Kind, NewMessage, Store as MailStore};
use protectorate_node_monitor::Monitor as NodeMonitor;
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) fn default_chooser(
    inventory: Arc<Inventory>,
    monitor: Arc<NodeMonitor>,
    local_name: String,
) -> Chooser {
    Arc::new(move |request, blocked| {
        if request.starts_with('{') {
            return Err(leases::Error::Unavailable(
                "node selectors require a connected node inventory".into(),
            ));
        }
        let resource = leases::parse_resource(request)?;
        let name = resource.node.as_str();
        let address = if name != local_name {
            let node = inventory.get(name).map_err(|_| {
                leases::Error::Unavailable(format!("node {name} is not in the inventory"))
            })?;
            if node.invited || node.tier != SHARED {
                return Err(leases::Error::Unavailable(format!(
                    "node {name} is not available for leases"
                )));
            }
            node.address
        } else {
            "local".into()
        };
        if !monitor.is_fresh(name, &address) {
            return Err(leases::Error::Unavailable(format!(
                "node {name} has no fresh successful metrics poll"
            )));
        }
        monitor
            .gpu_free(name, &address, resource.gpu)
            .map_err(leases::Error::Unavailable)?;
        if let Some(held) = blocked(&resource) {
            return Err(leases::Error::Busy(format!(
                "{} held by {}",
                resource, held.holder
            )));
        }
        Ok(resource)
    })
}

pub(super) fn open_table(
    path: &Path,
    events: Arc<EventBus>,
    mail: Arc<MailStore>,
    chooser: Chooser,
    admission: Arc<RwLock<()>>,
    draining: Arc<Mutex<BTreeSet<String>>>,
) -> leases::Result<LeaseTable> {
    let grant_events = Arc::clone(&events);
    let guarded: Chooser = Arc::new(move |request, blocked| {
        let resource = chooser(request, blocked)?;
        let name = &resource.node;
        if draining.lock().unwrap().contains(name) {
            return Err(leases::Error::Unavailable(format!(
                "node {name} is being removed"
            )));
        }
        Ok(resource)
    });
    Ok(LeaseTable::open(path)?
        .with_chooser(guarded)
        .with_admission(admission)
        .with_on_event(move |event| {
            publish_event(&events, event);
        })
        .with_on_grant(move |ticket, lease| {
            if ticket.notify.is_empty() {
                return;
            }
            let seconds = lease
                .expires_at
                .saturating_sub(now_millis())
                .max(0)
                / 1000;
            let body = format!(
                "Your queued lease on {} is granted as {}. It is held for you for {}s; unclaimed, it passes to the next waiter.\n\nReply: claim it with `protectorate lease renew {}` (keeps it for {}), or give it up with `protectorate lease release {}`.",
                lease.resource,
                lease.id,
                seconds,
                lease.id,
                duration(ticket.ttl),
                lease.id,
            );
            match mail.send(NewMessage {
                from: "hub".into(),
                to: ticket.notify.clone(),
                kind: Kind::NOTICE,
                body,
                request_id: format!("lease-grant-{}", lease.id),
                recipients: vec![ticket.notify.clone()],
                ..Default::default()
            }) {
                Ok(outcome) if outcome.created => grant_events.publish(
                    "msg.sent",
                    &outcome.message.id,
                    json!({"recipients": ticket.notify}),
                ),
                Ok(_) => {}
                Err(error) => eprintln!(
                    "lease {}: notify {}: {error}",
                    lease.id, ticket.notify
                ),
            }
        }))
}

fn publish_event(bus: &EventBus, event: &Event) {
    let (kind, subject, payload) = match event.kind {
        EventKind::Acquired | EventKind::Renewed | EventKind::Released | EventKind::Expired => {
            let Some(lease) = &event.lease else { return };
            let kind = match event.kind {
                EventKind::Acquired => "lease.acquired",
                EventKind::Renewed => "lease.renewed",
                EventKind::Released => "lease.released",
                _ => "lease.expired",
            };
            (
                kind,
                lease.id.as_str(),
                json!({
                    "resource": lease.resource.to_string(), "node": lease.resource.node,
                    "holder": lease.holder, "expires_at": at(lease.expires_at),
                }),
            )
        }
        EventKind::Queued | EventKind::Granted | EventKind::Dropped => {
            let Some(ticket) = &event.ticket else { return };
            let kind = match event.kind {
                EventKind::Queued => "lease.queued",
                EventKind::Granted => "lease.granted",
                _ => "lease.dropped",
            };
            let mut payload = json!({"resource": ticket.resource, "holder": ticket.holder});
            if let Some(position) = event.position {
                payload["position"] = json!(position);
            }
            if !event.reason.is_empty() {
                payload["reason"] = json!(event.reason);
            }
            if let Some(lease) = &event.lease {
                payload["lease_id"] = json!(lease.id);
            }
            (kind, ticket.id.as_str(), payload)
        }
    };
    bus.publish(kind, subject, payload);
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn at(millis: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn duration(ttl: Duration) -> String {
    let nanos = ttl.subsec_nanos();
    if nanos == 0 {
        format!("{}s", ttl.as_secs())
    } else {
        format!(
            "{}.{}s",
            ttl.as_secs(),
            format!("{nanos:09}").trim_end_matches('0')
        )
    }
}

fn entry_json(entry: &Entry) -> Value {
    match entry {
        Entry::Active(lease) => {
            let mut value = json!({
                "id": lease.id, "resource": lease.resource.to_string(),
                "node": lease.resource.node, "holder": lease.holder,
                "ttl": duration(lease.ttl), "acquiredAt": at(lease.acquired_at),
                "expiresAt": at(lease.expires_at),
            });
            if let Some(gpu) = lease.resource.gpu {
                value["gpuIndex"] = json!(gpu);
            }
            value
        }
        Entry::Queued { ticket, position } => json!({
            "id": ticket.id, "resource": ticket.resource, "holder": ticket.holder,
            "ttl": duration(ticket.ttl), "acquiredAt": at(ticket.enqueued_at),
            "expiresAt": at(ticket.enqueued_at.saturating_add(leases::MAX_QUEUE_WAIT.as_millis() as i64)),
            "state": "LEASE_STATE_QUEUED", "queuePosition": position,
        }),
    }
}

fn error(error: leases::Error) -> RpcError {
    let code = match error {
        leases::Error::Invalid(_) => Code::InvalidArgument,
        leases::Error::Busy(_) | leases::Error::Unavailable(_) => Code::FailedPrecondition,
        leases::Error::NotFound(_) => Code::NotFound,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn holder(actor: &str, requested: String) -> String {
    actor
        .strip_prefix("token:sleeve:")
        .map(str::to_owned)
        .unwrap_or(requested)
}

fn request_key(input: &Value) -> Result<(String, String)> {
    if let Some(resource) = input.get("resource").and_then(Value::as_str) {
        if resource.trim().is_empty() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "set resource or selector",
            ));
        }
        return Ok((resource.to_owned(), resource.to_owned()));
    }
    if let Some(selector) = input.get("selector") {
        return Ok((
            "(selector)".into(),
            json!({"selector": selector}).to_string(),
        ));
    }
    Err(RpcError::new(
        Code::InvalidArgument,
        "set resource or selector",
    ))
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let acquire = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.LeaseService/AcquireLease",
        move |request| {
            let hub = Arc::clone(&acquire);
            async move {
                let body = input(&request)?;
                let (resource, key) = request_key(&body)?;
                let holder = holder(&request.auth.actor, string(&body, "holder"));
                let ttl = parse_duration(body.get("ttl"))?;
                let wait = body.get("wait").and_then(Value::as_bool).unwrap_or(false);
                let priority = body.get("priority").and_then(Value::as_i64).unwrap_or(0) as i32;
                let notify = request
                    .auth
                    .actor
                    .strip_prefix("token:sleeve:")
                    .map(str::to_owned)
                    .unwrap_or_default();
                let table = Arc::clone(&hub.leases);
                let entry = tokio::task::spawn_blocking(move || {
                    let now = now_millis();
                    if wait {
                        table.acquire_wait(
                            Wait {
                                holder,
                                notify,
                                resource,
                                request: key,
                                ttl,
                                priority,
                            },
                            now,
                        )
                    } else {
                        table.acquire(&holder, ttl, &key, now).map(Entry::Active)
                    }
                })
                .await
                .map_err(super::join_error)?
                .map_err(error)?;
                respond(&request, entry_json(&entry))
            }
        },
    )?;
    let renew = Arc::clone(&hub);
    server.register("/protectorate.v1.LeaseService/RenewLease", move |request| {
        let table = Arc::clone(&renew.leases);
        async move {
            let body = input(&request)?;
            let id = string(&body, "id");
            let ttl = body
                .get("ttl")
                .map(|value| parse_duration(Some(value)))
                .transpose()?;
            let lease = tokio::task::spawn_blocking(move || table.renew(&id, ttl, now_millis()))
                .await
                .map_err(super::join_error)?
                .map_err(error)?;
            respond(&request, entry_json(&Entry::Active(lease)))
        }
    })?;
    let release = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.LeaseService/ReleaseLease",
        move |request| {
            let table = Arc::clone(&release.leases);
            async move {
                let id = string(&input(&request)?, "id");
                tokio::task::spawn_blocking(move || table.release(&id, now_millis()))
                    .await
                    .map_err(super::join_error)?
                    .map_err(error)?;
                respond(&request, json!({}))
            }
        },
    )?;
    server.register("/protectorate.v1.LeaseService/ListLeases", move |request| {
        let table = Arc::clone(&hub.leases);
        async move {
            let entries = tokio::task::spawn_blocking(move || table.list(now_millis()))
                .await
                .map_err(super::join_error)?
                .map_err(error)?;
            respond(
                &request,
                json!({"leases": entries.iter().map(entry_json).collect::<Vec<_>>() }),
            )
        }
    })?;
    Ok(())
}
