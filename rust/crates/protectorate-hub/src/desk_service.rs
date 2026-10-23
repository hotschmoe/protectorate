//! Hub desktop RPCs. The node agent remains the authority on desktop state;
//! the hub owns lease admission and forwards one bounded action at a time.
use super::{input, respond, string, Hub};
use axum::http::{header::HeaderName, HeaderValue};
use chrono::{SecondsFormat, Utc};
use protectorate_lease::{Entry, Lease, Resource};
use protectorate_wire::{Code, Result, RpcError, RpcRequest, RpcResponse, UnaryServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const ACTION: &str = "/protectorate.v1.NodeAgentService/DeskAction";
const STATE: &str = "/protectorate.v1.NodeAgentService/GetDeskState";
const AUTO_TTL: Duration = Duration::from_secs(120);
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_COUNTS_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DailyCounts {
    day: String,
    nodes: BTreeMap<String, BTreeMap<String, i32>>,
}

pub(super) struct UseCounts {
    path: PathBuf,
    counts: Mutex<DailyCounts>,
}

impl UseCounts {
    pub(super) fn open(path: PathBuf) -> std::result::Result<Self, String> {
        let counts = match fs::File::open(&path) {
            Ok(file) => {
                let metadata = file.metadata().map_err(|e| e.to_string())?;
                if !metadata.is_file() || metadata.len() > MAX_COUNTS_BYTES {
                    return Err("desk use store must be a regular file under 1 MiB".into());
                }
                let mut bytes = Vec::new();
                file.take(MAX_COUNTS_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_COUNTS_BYTES {
                    return Err("desk use store exceeds 1 MiB".into());
                }
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DailyCounts::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            path,
            counts: Mutex::new(counts),
        })
    }

    fn persist(&self, counts: &DailyCounts) -> std::result::Result<(), String> {
        let bytes = serde_json::to_vec_pretty(counts).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_COUNTS_BYTES {
            return Err("desk use store exceeds 1 MiB".into());
        }
        let parent = self.path.parent().ok_or("desk use store has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        staged.write_all(&bytes).map_err(|e| e.to_string())?;
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        staged.persist(&self.path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn add(&self, node: &str, caller: &str) {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        if counts.day != day {
            *counts = DailyCounts {
                day,
                nodes: BTreeMap::new(),
            };
        }
        let count = counts
            .nodes
            .entry(node.to_owned())
            .or_default()
            .entry(caller.to_owned())
            .or_default();
        *count = count.saturating_add(1);
        if let Err(error) = self.persist(&counts) {
            eprintln!("hub desk use persist failed: {error}");
        }
    }

    pub(super) fn get(&self, node: &str) -> Vec<Value> {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        if counts.day != day {
            *counts = DailyCounts {
                day,
                nodes: BTreeMap::new(),
            };
        }
        counts
            .nodes
            .get(node)
            .into_iter()
            .flat_map(|by_caller| by_caller.iter())
            .map(|(caller, count)| json!({"caller":caller,"count":count}))
            .collect()
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn lease_error(error: protectorate_lease::Error) -> RpcError {
    use protectorate_lease::Error;
    let code = match error {
        Error::Invalid(_) => Code::InvalidArgument,
        Error::Busy(_) | Error::Unavailable(_) => Code::FailedPrecondition,
        Error::NotFound(_) => Code::NotFound,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn target(value: &Value) -> Result<(String, String)> {
    let name = string(&value["target"], "name").trim().to_ascii_lowercase();
    if name.is_empty() {
        return Err(RpcError::invalid("target.name is required"));
    }
    if name.len() > 63
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    {
        return Err(RpcError::invalid("invalid node name"));
    }
    Ok((name, string(&value["target"], "leaseId")))
}

async fn policy_allows(hub: &Hub, request: &RpcRequest, name: &str) -> Result<()> {
    let Some(policy) = hub.desk_policies.get(name) else {
        return Ok(());
    };
    let client = hub.node_files.client_with_timeout(request, name, TIMEOUT)?;
    let input = json!({"policy":policy.value()});
    let state = tokio::task::spawn_blocking(move || client.call(STATE, &input))
        .await
        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
        .map_err(|e| {
            RpcError::new(
                Code::FailedPrecondition,
                format!("desktop availability unavailable: {e}"),
            )
        })?;
    if state["available"] != true {
        if !matches!(
            state["reason"].as_str(),
            None | Some("" | "DESK_REASON_UNKNOWN" | "DESK_REASON_UNSPECIFIED")
        ) {
            end_desk_leases(hub, name);
        }
        return Err(RpcError::new(
            Code::FailedPrecondition,
            format!("desk:{name} is unavailable: {}", string(&state, "detail")),
        ));
    }
    Ok(())
}

fn resource(name: &str) -> Resource {
    Resource {
        node: format!("desk:{name}"),
        gpu: None,
    }
}

async fn admit(
    hub: &Hub,
    request: &RpcRequest,
    name: &str,
    lease_id: &str,
) -> Result<Option<(Lease, bool)>> {
    let now = now_ms();
    if !lease_id.is_empty() {
        let entry = hub.leases.get(lease_id, now).map_err(lease_error)?;
        let Entry::Active(lease) = entry else {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "desk lease is still queued",
            ));
        };
        if lease.resource != resource(name) {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("lease {lease_id} is not on desk:{name}"),
            ));
        }
        if lease.holder.starts_with("auto:") {
            let renewed = hub.leases.renew(lease_id, None, now).map_err(lease_error)?;
            return Ok(Some((renewed, false)));
        }
        return Ok(None);
    }
    let holder = format!("auto:{}", request.auth.actor);
    for _ in 0..2 {
        if let Some(held) = hub
            .leases
            .blocked(&resource(name), now_ms())
            .map_err(lease_error)?
        {
            if held.holder != holder {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    format!("desk:{name} is held by {}", held.holder),
                ));
            }
            if let Ok(renewed) = hub.leases.renew(&held.id, None, now_ms()) {
                return Ok(Some((renewed, false)));
            }
            continue;
        }
        match hub
            .leases
            .acquire(&holder, AUTO_TTL, &format!("desk:{name}"), now_ms())
        {
            Ok(lease) => return Ok(Some((lease, true))),
            Err(protectorate_lease::Error::Busy(_)) => continue,
            Err(error) => return Err(lease_error(error)),
        }
    }
    Err(RpcError::new(
        Code::FailedPrecondition,
        "desktop lease changed during admission",
    ))
}

fn end_desk_leases(hub: &Hub, name: &str) {
    let leases = match hub.leases.list(now_ms()) {
        Ok(leases) => leases,
        Err(error) => {
            eprintln!("hub desk lease list failed after node refusal: {error}");
            return;
        }
    };
    for entry in leases {
        let Entry::Active(lease) = entry else {
            continue;
        };
        if lease.resource == resource(name) {
            if let Err(error) = hub.leases.release(&lease.id, now_ms()) {
                eprintln!(
                    "hub desk lease {} release failed after node refusal: {error}",
                    lease.id
                );
            }
        }
    }
}

fn add_headers(response: &mut RpcResponse, lease: Option<&Lease>) -> Result<()> {
    let Some(lease) = lease else {
        return Ok(());
    };
    response.headers.insert(
        HeaderName::from_static("protectorate-desk-lease"),
        HeaderValue::from_str(&lease.id)
            .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?,
    );
    let expiry = chrono::DateTime::<Utc>::from_timestamp_millis(lease.expires_at)
        .ok_or_else(|| RpcError::new(Code::Internal, "invalid desk lease expiry"))?
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    response.headers.insert(
        HeaderName::from_static("protectorate-desk-lease-expires"),
        HeaderValue::from_str(&expiry).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?,
    );
    Ok(())
}

async fn dispatch(
    hub: Arc<Hub>,
    request: RpcRequest,
    name: &'static str,
    result: &'static str,
    mutating: bool,
) -> Result<RpcResponse> {
    let value = input(&request)?;
    let (node, lease_id) = target(&value)?;
    if mutating {
        // An explicit lease proves ownership of the resource, but the node may
        // not have received a newly saved policy yet. Refresh it before input.
        policy_allows(&hub, &request, &node).await?;
    }
    let lease = if mutating {
        admit(&hub, &request, &node, &lease_id).await?
    } else {
        None
    };
    let client = match hub.node_files.client_with_timeout(&request, &node, TIMEOUT) {
        Ok(client) => client,
        Err(error) => {
            if let Some((lease, true)) = &lease {
                let _ = hub.leases.release(&lease.id, now_ms());
            }
            return Err(error);
        }
    };
    let action = json!({name:value});
    let result_value = match tokio::task::spawn_blocking(move || client.call(ACTION, &action)).await
    {
        Ok(value) => value,
        Err(error) => Err(RpcError::new(Code::Internal, error.to_string())),
    };
    let result_value = match result_value {
        Ok(value) => value,
        Err(error) => {
            let refused = mutating && error.code == Code::FailedPrecondition;
            if refused {
                end_desk_leases(&hub, &node);
            } else if let Some((lease, true)) = &lease {
                let _ = hub.leases.release(&lease.id, now_ms());
            }
            return Err(error);
        }
    };
    let output = result_value.get(result).cloned().ok_or_else(|| {
        RpcError::new(
            Code::Internal,
            format!("node desk action omitted {result} result"),
        )
    })?;
    if mutating {
        hub.desk_use.add(&node, &request.auth.actor);
    }
    let mut response = respond(&request, output)?;
    add_headers(&mut response, lease.as_ref().map(|(lease, _)| lease))?;
    Ok(response)
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    for (method, action, result, mutating) in [
        ("Screenshot", "screenshot", "screenshot", false),
        ("ListWindows", "listWindows", "listWindows", false),
        ("Foreground", "foreground", "foreground", false),
        ("Click", "click", "input", true),
        ("Move", "move", "input", true),
        ("Type", "type", "input", true),
        ("Key", "key", "input", true),
        ("Run", "run", "run", true),
    ] {
        let hub = Arc::clone(&hub);
        server.register(
            &format!("/protectorate.v1.DeskService/{method}"),
            move |request| {
                let hub = Arc::clone(&hub);
                async move { dispatch(hub, request, action, result, mutating).await }
            },
        )?;
    }
    Ok(())
}
