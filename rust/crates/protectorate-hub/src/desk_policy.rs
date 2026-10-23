//! Durable hub desk policy and bounded desktop-state cache for node status.
use super::{input, parse_duration, respond, string, Hub};
use protectorate_desk_policy::Policy;
use protectorate_lease::Resource;
use protectorate_mailbox::{Kind, NewMessage};
use protectorate_sleeve_registry::Mode;
use protectorate_wire::{Code, Result, RpcError, RpcRequest, UnaryServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_STORE_BYTES: u64 = 1024 * 1024;
const STATE_TIMEOUT: Duration = Duration::from_secs(10);
const STATE: &str = "/protectorate.v1.NodeAgentService/GetDeskState";
const POLICY_REFRESH: Duration = Duration::from_secs(3);
const IDLE_REFRESH: Duration = Duration::from_secs(30);
const MAX_CACHED_NODES: usize = 256;
const MAX_CONCURRENT_REFRESHES: usize = 8;
const MAX_REVOKES_PER_PASS: usize = 64;
const LEASE_STATE_MAX_AGE: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    window: String,
    idle: String,
    timezone: String,
    lock_after: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disk {
    nodes: BTreeMap<String, Record>,
}

pub(super) struct Store {
    path: PathBuf,
    disk: Mutex<Disk>,
    cache: Mutex<Cache>,
    refresh_slots: Arc<tokio::sync::Semaphore>,
    runtime: Mutex<Option<tokio::runtime::Handle>>,
}

#[derive(Default)]
struct Cache {
    states: BTreeMap<String, CachedState>,
    in_flight: BTreeMap<String, u64>,
    next_token: u64,
}

struct CachedState {
    address: String,
    state: Value,
    at: Instant,
}

impl Store {
    pub(super) fn open(path: PathBuf) -> std::result::Result<Self, String> {
        let disk = match fs::File::open(&path) {
            Ok(file) => {
                let meta = file.metadata().map_err(|e| e.to_string())?;
                if !meta.is_file() || meta.len() > MAX_STORE_BYTES {
                    return Err("desk policy store must be a regular file under 1 MiB".into());
                }
                let mut bytes = Vec::new();
                file.take(MAX_STORE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_STORE_BYTES {
                    return Err("desk policy store exceeds 1 MiB".into());
                }
                let disk: Disk = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                for (node, policy) in &disk.nodes {
                    validate(policy).map_err(|e| format!("node {node}: {e}"))?;
                }
                disk
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Disk::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            path,
            disk: Mutex::new(disk),
            cache: Mutex::new(Cache::default()),
            refresh_slots: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_REFRESHES)),
            runtime: Mutex::new(None),
        })
    }

    pub(super) fn get(&self, name: &str) -> Option<Record> {
        self.disk
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .nodes
            .get(name)
            .cloned()
    }

    fn set(&self, name: &str, policy: Option<Record>) -> Result<()> {
        let mut guard = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = Disk {
            nodes: guard.nodes.clone(),
        };
        if let Some(policy) = policy {
            next.nodes.insert(name.to_owned(), policy);
        } else {
            next.nodes.remove(name);
        }
        let bytes = serde_json::to_vec_pretty(&next)
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(RpcError::new(
                Code::ResourceExhausted,
                "desk policy store exceeds 1 MiB",
            ));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| RpcError::new(Code::Internal, "desk policy path has no parent"))?;
        fs::create_dir_all(parent).map_err(internal)?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(internal)?;
        staged.write_all(&bytes).map_err(internal)?;
        staged.as_file().sync_all().map_err(internal)?;
        staged.persist(&self.path).map_err(|e| internal(e.error))?;
        *guard = next;
        self.invalidate(name);
        Ok(())
    }

    fn invalidate(&self, name: &str) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.in_flight.remove(name);
        cache.states.remove(name);
    }

    pub(super) fn cached(&self, name: &str, address: &str) -> Option<Value> {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .states
            .get(name)
            .filter(|entry| entry.address == address)
            .map(|entry| entry.state.clone())
    }

    pub(super) fn fresh_available(&self, name: &str) -> bool {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .states
            .get(name)
            .is_some_and(|entry| {
                entry.at.elapsed() <= LEASE_STATE_MAX_AGE && entry.state["available"] == true
            })
    }

    fn put(&self, name: &str, address: &str, state: Value, token: Option<u64>) -> bool {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if token.is_some_and(|token| cache.in_flight.get(name) != Some(&token)) {
            return false;
        }
        cache.in_flight.remove(name);
        if !cache.states.contains_key(name) && cache.states.len() >= MAX_CACHED_NODES {
            if let Some(oldest) = cache
                .states
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(name, _)| name.clone())
            {
                cache.states.remove(&oldest);
            }
        }
        cache.states.insert(
            name.to_owned(),
            CachedState {
                address: address.to_owned(),
                state,
                at: Instant::now(),
            },
        );
        true
    }

    pub(super) fn refresh_if_due(
        hub: &Arc<Hub>,
        name: &str,
        address: &str,
        os: &str,
        invited: bool,
    ) {
        if invited || name == hub.hostname {
            return;
        }
        // Poll events can arrive on NodeMonitor's scoped OS threads. Use the
        // server runtime rather than assuming this caller is a Tokio worker.
        let Some(runtime) = hub.desk_policies.runtime.lock().unwrap().clone() else {
            return;
        };
        let has_policy = hub.desk_policies.get(name).is_some();
        if !has_policy && !os.eq_ignore_ascii_case("windows") {
            return;
        }
        let interval = if has_policy {
            POLICY_REFRESH
        } else {
            IDLE_REFRESH
        };
        let Ok(permit) = Arc::clone(&hub.desk_policies.refresh_slots).try_acquire_owned() else {
            return;
        };
        let token = {
            let mut cache = hub
                .desk_policies
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if cache.in_flight.contains_key(name)
                || cache
                    .states
                    .get(name)
                    .is_some_and(|entry| entry.address == address && entry.at.elapsed() < interval)
            {
                return;
            }
            cache.next_token = cache.next_token.wrapping_add(1);
            let token = cache.next_token;
            cache.in_flight.insert(name.to_owned(), token);
            token
        };
        let hub = Arc::clone(hub);
        let name = name.to_owned();
        let address = address.to_owned();
        let os = os.to_owned();
        runtime.spawn(async move {
            let policy = hub.desk_policies.get(&name);
            let request =
                policy.map_or_else(|| json!({}), |policy| json!({"policy":policy.value()}));
            let state = if os.eq_ignore_ascii_case("linux") {
                json!({"reason":"DESK_REASON_UNSUPPORTED","detail":"desktop control is unsupported on this node"})
            } else if !os.eq_ignore_ascii_case("windows") {
                json!({"reason":"DESK_REASON_UNKNOWN","detail":"desktop state has not been reported by the node"})
            } else {
                match hub
                    .node_files
                    .internal_client_with_timeout(&name, STATE_TIMEOUT)
                {
                    Ok(client) => {
                        match tokio::task::spawn_blocking(move || client.call(STATE, &request))
                            .await
                        {
                            Ok(Ok(state)) => state,
                            Ok(Err(error)) => {
                                json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("node did not report desktop state: {error}")})
                            }
                            Err(error) => {
                                json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("desktop state task failed: {error}")})
                            }
                        }
                    }
                    Err(error) => {
                        json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("node did not report desktop state: {error}")})
                    }
                }
            };
            if hub
                .desk_policies
                .put(&name, &address, state.clone(), Some(token))
                && should_revoke(&hub, &name, &state)
            {
                let hub = Arc::clone(&hub);
                tokio::task::spawn_blocking(move || revoke_desk_leases(&hub, &name, &state));
            }
            drop(permit);
        });
    }
}

impl Record {
    pub(super) fn value(&self) -> Value {
        json!({"window": self.window, "idle": self.idle, "timezone": self.timezone, "lockAfter": self.lock_after})
    }

    pub(super) fn lock_after(&self) -> bool {
        self.lock_after
    }
}

fn internal(error: std::io::Error) -> RpcError {
    RpcError::new(Code::Internal, format!("persist desk policy: {error}"))
}

fn validate(record: &Record) -> Result<Policy> {
    let idle = parse_duration(Some(&Value::String(record.idle.clone())))?;
    if idle > Duration::from_secs(24 * 60 * 60) {
        return Err(RpcError::new(
            Code::InvalidArgument,
            "desk idle exceeds 24 hours",
        ));
    }
    Policy::new(&record.window, idle, &record.timezone, record.lock_after)
        .map_err(|e| RpcError::new(Code::InvalidArgument, e.to_string()))
}

fn parse_policy(value: &Value) -> Result<Record> {
    let record = Record {
        window: string(value, "window"),
        idle: string(value, "idle"),
        timezone: string(value, "timezone"),
        lock_after: value["lockAfter"] == true,
    };
    let parsed = validate(&record)?;
    Ok(Record {
        window: parsed.window(),
        ..record
    })
}

fn node(hub: &Hub, name: &str) -> Result<protectorate_inventory::Node> {
    let name = name.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    {
        return Err(RpcError::new(Code::InvalidArgument, "invalid node name"));
    }
    if name == hub.hostname {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "the hub's own node does not take a desk policy",
        ));
    }
    let node = hub
        .inventory
        .get(&name)
        .map_err(|e| RpcError::new(Code::NotFound, e.to_string()))?;
    if node.invited {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "node has not joined",
        ));
    }
    Ok(node)
}

async fn status(
    hub: &Hub,
    request: &RpcRequest,
    node: &protectorate_inventory::Node,
) -> Result<Value> {
    let name = node.name.to_ascii_lowercase();
    let mut result = json!({});
    result["inputs"] = json!(hub.desk_use.get(&name));
    if let Some(policy) = hub.desk_policies.get(&name) {
        result["policy"] = json!({
            "window": policy.window, "idle": policy.idle,
            "timezone": policy.timezone, "lockAfter": policy.lock_after,
        });
    }
    result["state"] = if node.os.eq_ignore_ascii_case("linux") {
        json!({"reason":"DESK_REASON_UNSUPPORTED","detail":"desktop control is unsupported on this node"})
    } else if node.os.eq_ignore_ascii_case("windows") {
        let state_request = result
            .get("policy")
            .map_or_else(|| json!({}), |policy| json!({"policy":policy}));
        match hub
            .node_files
            .client_with_timeout(request, &name, STATE_TIMEOUT)
        {
            Ok(client) => {
                match tokio::task::spawn_blocking(move || client.call(STATE, &state_request)).await
                {
                    Ok(Ok(state)) => state,
                    Ok(Err(error)) => {
                        json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("node did not report desktop state: {error}")})
                    }
                    Err(error) => {
                        json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("desktop state task failed: {error}")})
                    }
                }
            }
            Err(error) => {
                json!({"reason":"DESK_REASON_UNKNOWN","detail":format!("node did not report desktop state: {error}")})
            }
        }
    } else {
        json!({"reason":"DESK_REASON_UNKNOWN","detail":"desktop state has not been reported by the node"})
    };
    hub.desk_policies
        .put(&name, &node.address, result["state"].clone(), None);
    let resource = Resource {
        node: format!("desk:{name}"),
        gpu: None,
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    if let Some(lease) = hub
        .leases
        .blocked(&resource, now_ms)
        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
    {
        result["holder"] = json!(lease.holder);
    }
    Ok(result)
}

pub(super) async fn gate_live(hub: &Arc<Hub>, request: &RpcRequest, name: &str) -> Result<()> {
    if hub.desk_policies.get(name).is_none() {
        return Ok(());
    }
    let node = node(hub, name)?;
    let result = status(hub, request, &node).await?;
    if result["state"]["available"] == true {
        return Ok(());
    }
    if should_revoke(hub, name, &result["state"]) {
        let hub = Arc::clone(hub);
        let name = name.to_owned();
        let state = result["state"].clone();
        tokio::task::spawn_blocking(move || revoke_desk_leases(&hub, &name, &state))
            .await
            .map_err(super::join_error)?;
    }
    Err(RpcError::new(
        Code::FailedPrecondition,
        format!(
            "desk:{name} is unavailable: {}",
            string(&result["state"], "detail")
        ),
    ))
}

fn should_revoke(hub: &Hub, name: &str, state: &Value) -> bool {
    hub.desk_policies.get(name).is_some()
        && state["available"] != true
        && state["reason"] != "DESK_REASON_UNKNOWN"
}

pub(super) fn revoke_desk_leases(hub: &Hub, name: &str, state: &Value) {
    if !should_revoke(hub, name, state) {
        return;
    }
    let resource = Resource {
        node: format!("desk:{name}"),
        gpu: None,
    };
    let detail = string(state, "detail");
    // Releasing an active lease may grant the next queued lease, so keep
    // draining a bounded number and revisit on the next watcher pass.
    for _ in 0..MAX_REVOKES_PER_PASS {
        let now = chrono::Utc::now().timestamp_millis();
        let lease = match hub.leases.blocked(&resource, now) {
            Ok(Some(lease)) => lease,
            Ok(None) => return,
            Err(error) => {
                eprintln!("desk:{name}: inspect active lease: {error}");
                return;
            }
        };
        if let Err(error) = hub.leases.release(&lease.id, now) {
            eprintln!(
                "desk:{name}: release unavailable lease {}: {error}",
                lease.id
            );
            return;
        }
        let who = lease.holder.strip_prefix("auto:").unwrap_or(&lease.holder);
        let who = who.strip_prefix("token:").unwrap_or(who);
        let who = who.strip_prefix("sleeve:").unwrap_or(who);
        let Ok(sleeve) = hub.sleeves.get(who) else {
            continue;
        };
        let body = format!(
            "Your desktop lease {} on {} was ended by the hub: the desktop is unavailable ({}). Stop driving the desktop. Check `protectorate desk policy {}` before trying again.",
            lease.id, name, detail, name
        );
        match hub.mail.send(NewMessage {
            from: "hub".into(),
            to: who.into(),
            kind: Kind::NOTICE,
            body,
            request_id: format!("desk-away-{}", lease.id),
            recipients: vec![who.into()],
            ..Default::default()
        }) {
            Ok(outcome) if outcome.created => {
                hub.emit("msg.sent", &outcome.message.id, json!({"recipients": who}));
                if sleeve.mode != Mode::Chat {
                    if let Err(error) = hub.mail.arm(&outcome.message.id, who) {
                        eprintln!("desk:{name}: arm notice for {who}: {error}");
                    }
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("desk:{name}: notify {who}: {error}"),
        }
    }
    eprintln!("desk:{name}: deferred remaining lease revocations to next watcher pass");
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    // A weak reference lets the worker stop when the server and handlers stop.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        *hub.desk_policies.runtime.lock().unwrap() = Some(runtime.clone());
        let worker = Arc::downgrade(&hub);
        runtime.spawn(async move {
            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + POLICY_REFRESH,
                POLICY_REFRESH,
            );
            let mut pass = 0u64;
            loop {
                ticker.tick().await;
                let Some(hub) = worker.upgrade() else { break };
                pass = pass.wrapping_add(1);
                let all = pass.is_multiple_of(10);
                for node in hub.inventory.list() {
                    if node.invited
                        || (hub.desk_policies.get(&node.name).is_none()
                            && (!all || !node.os.eq_ignore_ascii_case("windows")))
                    {
                        continue;
                    }
                    Store::refresh_if_due(&hub, &node.name, &node.address, &node.os, false);
                }
            }
        });
    }
    let get_hub = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.NodeService/GetDeskPolicy",
        move |request| {
            let hub = Arc::clone(&get_hub);
            async move {
                let value = input(&request)?;
                let node = node(&hub, &string(&value, "name"))?;
                respond(&request, status(&hub, &request, &node).await?)
            }
        },
    )?;
    server.register(
        "/protectorate.v1.NodeService/SetDeskPolicy",
        move |request| {
            let hub = Arc::clone(&hub);
            async move {
                if !request.auth.owner {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "desk policy changes require the owner token",
                    ));
                }
                let value = input(&request)?;
                let node = node(&hub, &string(&value, "name"))?;
                let policy = value
                    .get("policy")
                    .filter(|v| !v.is_null())
                    .map(parse_policy)
                    .transpose()?;
                hub.desk_policies
                    .set(&node.name.to_ascii_lowercase(), policy)?;
                let result = status(&hub, &request, &node).await?;
                if should_revoke(&hub, &node.name, &result["state"]) {
                    let hub = Arc::clone(&hub);
                    let name = node.name.clone();
                    let state = result["state"].clone();
                    tokio::task::spawn_blocking(move || revoke_desk_leases(&hub, &name, &state))
                        .await
                        .map_err(super::join_error)?;
                }
                respond(&request, result)
            }
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_inventory::Node;

    #[test]
    fn direct_update_invalidates_only_the_same_nodes_in_flight_reply() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("policies.json")).unwrap();
        {
            let mut cache = store.cache.lock().unwrap();
            cache.in_flight.insert("a".into(), 1);
            cache.in_flight.insert("b".into(), 2);
        }
        store.put("a", "a:123", json!({"detail":"new"}), None);
        store.put("a", "a:123", json!({"detail":"old"}), Some(1));
        store.put("b", "b:123", json!({"detail":"peer"}), Some(2));
        assert_eq!(store.cached("a", "a:123").unwrap()["detail"], "new");
        assert_eq!(store.cached("b", "b:123").unwrap()["detail"], "peer");
        assert!(store.cached("a", "changed:123").is_none());
    }

    #[tokio::test]
    async fn scoped_monitor_poll_can_refresh_windows_desk_state() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Arc::new(Hub::open(dir.path(), "owner-secret", "test").unwrap());
        hub.inventory
            .add(
                Node {
                    name: "windows-peer".into(),
                    address: "127.0.0.1:17484".into(),
                    os: "windows".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        *hub.desk_policies.runtime.lock().unwrap() = Some(tokio::runtime::Handle::current());
        hub.node_monitor
            .set_route(Arc::new(|_, _| Err("fixture offline".into())));
        let weak = Arc::downgrade(&hub);
        hub.node_monitor.set_event_sink(Arc::new(move |event| {
            if let Some(hub) = weak.upgrade() {
                crate::nodes::publish_poll_event(&hub, event);
            }
        }));

        // poll_all launches poll_remote on a scoped OS thread, as in serve.
        let monitor = Arc::clone(&hub.node_monitor);
        tokio::task::spawn_blocking(move || monitor.poll_all())
            .await
            .unwrap();
        assert_eq!(
            hub.node_monitor
                .status("windows-peer", "127.0.0.1:17484", false)["error"],
            "fixture offline"
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(state) = hub.desk_policies.cached("windows-peer", "127.0.0.1:17484") {
                    assert_eq!(state["reason"], "DESK_REASON_UNKNOWN");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("desk refresh from scoped poll thread should finish");
    }
}
