//! Lease RPCs backed by the persistent lease table. Node admission is supplied
//! by a chooser once the node monitor is connected to this hub.
use super::{desk_policy, input, node_files, parse_duration, respond, string, EventBus, Hub};
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
    sync::{mpsc, Arc, Mutex, RwLock},
    thread,
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
        let desk = resource.node.starts_with("desk:");
        let name = resource
            .node
            .strip_prefix("desk:")
            .unwrap_or(&resource.node);
        let address = if name != local_name {
            let node = inventory.get(name).map_err(|_| {
                leases::Error::Unavailable(format!("node {name} is not in the inventory"))
            })?;
            if node.invited || (!desk && node.tier != SHARED) {
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
        if !desk {
            monitor
                .gpu_free(name, &address, resource.gpu)
                .map_err(leases::Error::Unavailable)?;
        }
        if let Some(held) = blocked(&resource) {
            return Err(leases::Error::Busy(format!(
                "{} held by {}",
                resource, held.holder
            )));
        }
        Ok(resource)
    })
}

pub(super) struct DeskNotifications {
    policies: Arc<desk_policy::Store>,
    files: Arc<node_files::FileRelay>,
}

impl DeskNotifications {
    pub(super) fn new(
        policies: Arc<desk_policy::Store>,
        files: Arc<node_files::FileRelay>,
    ) -> Self {
        Self { policies, files }
    }
}

pub(super) fn open_table(
    path: &Path,
    events: Arc<EventBus>,
    mail: Arc<MailStore>,
    desk: DeskNotifications,
    chooser: Chooser,
    admission: Arc<RwLock<()>>,
    draining: Arc<Mutex<BTreeSet<String>>>,
) -> leases::Result<LeaseTable> {
    let grant_events = Arc::clone(&events);
    let DeskNotifications { policies, files } = desk;
    let chooser_policies = Arc::clone(&policies);
    let (desk_end_tx, desk_end_rx) = mpsc::sync_channel::<String>(64);
    let worker_policies = Arc::clone(&policies);
    thread::Builder::new()
        .name("protectorate-desk-lease".into())
        .spawn(move || {
            for name in desk_end_rx {
                let Some(policy) = worker_policies
                    .get(&name)
                    .filter(|policy| policy.lock_after())
                else {
                    continue;
                };
                let result = files
                    .internal_client_with_timeout(&name, Duration::from_secs(10))
                    .and_then(|client| {
                        client.call(
                            "/protectorate.v1.NodeAgentService/GetDeskState",
                            &json!({"policy": policy.value(), "leaseEnded": true}),
                        )
                    });
                if let Err(error) = result {
                    eprintln!("desk:{name} lease end notification failed: {error}");
                }
            }
        })?;
    let guarded: Chooser = Arc::new(move |request, blocked| {
        let resource = chooser(request, blocked)?;
        if let Some(name) = resource.node.strip_prefix("desk:") {
            if chooser_policies.get(name).is_some() && !chooser_policies.fresh_available(name) {
                return Err(leases::Error::Unavailable(format!(
                    "desk:{name} has no fresh available desktop state"
                )));
            }
        }
        let name = resource
            .node
            .strip_prefix("desk:")
            .unwrap_or(&resource.node);
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
            if !matches!(event.kind, EventKind::Released | EventKind::Expired) {
                return;
            }
            let Some(name) = event.lease.as_ref().and_then(|lease| lease.resource.node.strip_prefix("desk:")) else {
                return;
            };
            if !policies.get(name).is_some_and(|policy| policy.lock_after()) {
                return;
            }
            if let Err(error) = desk_end_tx.try_send(name.to_owned()) {
                eprintln!("desk:{name} lease end notification queue unavailable: {error}");
            }
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
                if let Some(name) = resource.strip_prefix("desk:") {
                    desk_policy::gate_live(&hub, &request, name).await?;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_grant_sends_a_single_claim_notice_to_the_waiting_sleeve() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Arc::new(MailStore::open(dir.path().join("mail.jsonl")).unwrap());
        let chooser: Chooser = Arc::new(|request, blocked| {
            let resource = leases::parse_resource(request)?;
            if blocked(&resource).is_some() {
                return Err(leases::Error::Busy("held".into()));
            }
            Ok(resource)
        });
        let table = open_table(
            &dir.path().join("leases.json"),
            Arc::new(EventBus::new()),
            Arc::clone(&mail),
            DeskNotifications::new(
                Arc::new(desk_policy::Store::open(dir.path().join("desk-policies.json")).unwrap()),
                Arc::new(node_files::FileRelay::new(
                    Arc::new(Inventory::open(dir.path().join("nodes.json"), "hub").unwrap()),
                    "hub".into(),
                )),
            ),
            chooser,
            Arc::new(RwLock::new(())),
            Arc::new(Mutex::new(BTreeSet::new())),
        )
        .unwrap();
        let now = now_millis();
        let held = table
            .acquire("first", Duration::from_secs(20), "peer", now)
            .unwrap();
        let queued = table
            .acquire_wait(
                Wait {
                    holder: "waiting".into(),
                    notify: "worker".into(),
                    resource: "peer".into(),
                    request: "peer".into(),
                    ttl: Duration::from_secs(60),
                    priority: 0,
                },
                now,
            )
            .unwrap();
        assert!(matches!(queued, Entry::Queued { .. }));
        table.release(&held.id, now).unwrap();
        let pending = mail.pending();
        let notices = &pending["worker"];
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].from, "hub");
        assert_eq!(notices[0].kind, Kind::NOTICE);
        assert!(notices[0].body.contains("protectorate lease renew"));
        assert!(notices[0].body.contains("protectorate lease release"));
        assert!(notices[0].request_id.starts_with("lease-grant-"));
        table.sweep(now).unwrap();
        assert_eq!(mail.pending()["worker"].len(), 1);
    }
}
