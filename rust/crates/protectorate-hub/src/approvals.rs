//! Owner approval for high-impact calls by agents: Envoy, sleeves, and grants
//! Envoy minted. A gated call opens a request and fails with an aborted
//! error carrying its id; the same call repeated with that id in the
//! `protectorate-approval` header runs once the owner approves it, exactly
//! once. Requests bind to the caller and the exact request (or launch
//! inputs), live in hub memory and expire. Launch pins (rug-pull detection)
//! use the same requests.
use super::{input, respond, string, Hub};
use chrono::{DateTime, Utc};
use protectorate_audit::Entry as AuditEntry;
use protectorate_sleeve_registry::Pin;
use protectorate_wire::{encode_json, Code, Result, RpcError, RpcRequest, UnaryServer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) const HEADER: &str = "protectorate-approval";
pub(crate) const DEFAULT_TTL: Duration = Duration::from_secs(10 * 60);
const DECIDE: &str = "/protectorate.v1.ApprovalService/DecideApproval";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Pending,
    Approved,
    Denied,
    Expired,
    Used,
}

impl State {
    fn proto(self) -> &'static str {
        match self {
            Self::Pending => "APPROVAL_STATE_PENDING",
            Self::Approved => "APPROVAL_STATE_APPROVED",
            Self::Denied => "APPROVAL_STATE_DENIED",
            Self::Expired => "APPROVAL_STATE_EXPIRED",
            Self::Used => "APPROVAL_STATE_USED",
        }
    }
}

#[derive(Clone)]
struct Request {
    ask: Ask,
    id: String,
    fingerprint: String,
    state: State,
    created: DateTime<Utc>,
    expires: DateTime<Utc>,
    decided_by: String,
}

/// One gated action as the owner sees it. `binding` is what an approval
/// admits: the encoded request, or a launch's hashes.
#[derive(Clone, Default)]
pub(crate) struct Ask {
    pub actor: String,
    pub token_id: String,
    pub run_id: String,
    pub procedure: String,
    pub subject: String,
    pub reason: String,
    pub detail: String,
    pub binding: Vec<u8>,
    /// The request id the caller repeats, from the approval header.
    pub hint: String,
}

pub(crate) struct Approvals {
    requests: Mutex<BTreeMap<String, Request>>,
    ttl: Duration,
}

impl Approvals {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            requests: Mutex::new(BTreeMap::new()),
            ttl,
        }
    }

    /// Ok when the owner approved this exact ask and it was not used yet.
    /// Otherwise the error names the open request, a denial or an expiry.
    pub(crate) fn check(&self, hub: &Hub, ask: Ask) -> Result<()> {
        let fingerprint = format!(
            "{:x}",
            Sha256::new()
                .chain_update(ask.actor.as_bytes())
                .chain_update([0])
                .chain_update(ask.token_id.as_bytes())
                .chain_update([0])
                .chain_update(ask.procedure.as_bytes())
                .chain_update([0])
                .chain_update(&ask.binding)
                .finalize()
        );
        let now = Utc::now();
        let mut requests = self.requests.lock().unwrap();
        self.sweep(hub, &mut requests, now);
        let found = requests
            .get(&ask.hint)
            .filter(|r| r.fingerprint == fingerprint && r.state != State::Used)
            .or_else(|| {
                requests.values().find(|r| {
                    r.fingerprint == fingerprint
                        && matches!(r.state, State::Pending | State::Approved | State::Denied)
                })
            })
            .map(|r| r.id.clone());
        let request = match found {
            Some(id) => requests.get_mut(&id).unwrap(),
            None => {
                let id = new_id()?;
                let request = Request {
                    ask,
                    id: id.clone(),
                    fingerprint,
                    state: State::Pending,
                    created: now,
                    expires: now
                        + chrono::Duration::from_std(self.ttl).unwrap_or(chrono::Duration::MAX),
                    decided_by: String::new(),
                };
                hub.audit.write(AuditEntry {
                    actor: request.ask.actor.clone(),
                    token_id: request.ask.token_id.clone(),
                    run_id: request.ask.run_id.clone(),
                    procedure: "approval.request".into(),
                    subject: id.clone(),
                    outcome: "ok".into(),
                    detail: summary(&request.ask),
                    ..AuditEntry::default()
                });
                hub.emit("approval.requested", &id, event(&request));
                requests.entry(id).or_insert(request)
            }
        };
        match request.state {
            State::Approved => {
                request.state = State::Used;
                Ok(())
            }
            State::Pending => {
                let mut error = RpcError::new(
                    Code::Aborted,
                    format!(
                        "waiting for owner approval {} ({}); repeat the call with header {HEADER}: {} to continue",
                        request.id, request.ask.reason, request.id
                    ),
                );
                error.details.push(json!({
                    "type": "protectorate.v1.Approval",
                    "debug": {"id": request.id, "expiresAt": super::timestamp(request.expires)},
                }));
                Err(error)
            }
            State::Denied => Err(RpcError::new(
                Code::PermissionDenied,
                format!(
                    "{} denied approval {} ({})",
                    request.decided_by, request.id, request.ask.reason
                ),
            )),
            State::Expired | State::Used => Err(RpcError::new(
                Code::PermissionDenied,
                format!("approval {} expired before the owner decided", request.id),
            )),
        }
    }

    fn decide(&self, hub: &Hub, id: &str, approve: bool, decider: &str) -> Result<Request> {
        let mut requests = self.requests.lock().unwrap();
        self.sweep(hub, &mut requests, Utc::now());
        let request = requests
            .get_mut(id)
            .ok_or_else(|| RpcError::new(Code::NotFound, format!("no approval request {id:?}")))?;
        if request.state != State::Pending {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("approval {id} is not pending"),
            ));
        }
        request.state = if approve {
            State::Approved
        } else {
            State::Denied
        };
        request.decided_by = decider.into();
        hub.emit("approval.decided", id, event(request));
        Ok(request.clone())
    }

    /// Expires overdue requests, auditing each, and forgets requests ten
    /// minutes after they expired; until then a denied call stays denied.
    fn sweep(&self, hub: &Hub, requests: &mut BTreeMap<String, Request>, now: DateTime<Utc>) {
        let keep = chrono::Duration::from_std(DEFAULT_TTL).unwrap_or(chrono::Duration::MAX);
        requests.retain(|_, r| r.expires + keep > now);
        for request in requests.values_mut() {
            if request.state == State::Pending && request.expires <= now {
                request.state = State::Expired;
                hub.audit.write(AuditEntry {
                    actor: "hub".into(),
                    run_id: request.ask.run_id.clone(),
                    procedure: "approval.expired".into(),
                    subject: request.id.clone(),
                    outcome: "ok".into(),
                    detail: summary(&request.ask),
                    ..AuditEntry::default()
                });
                hub.emit("approval.decided", &request.id, event(request));
            }
        }
    }
}

fn new_id() -> Result<String> {
    let mut bytes = [0u8; 6];
    getrandom::getrandom(&mut bytes).map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn summary(ask: &Ask) -> String {
    let mut text = format!("{}: {} {}", ask.reason, ask.procedure, ask.subject);
    if !ask.detail.is_empty() {
        text.push_str("; ");
        text.push_str(&ask.detail);
    }
    text
}

fn event(request: &Request) -> Value {
    json!({
        "actor": request.ask.actor,
        "procedure": request.ask.procedure,
        "subject": request.ask.subject,
        "reason": request.ask.reason,
        "state": request.state.proto(),
    })
}

fn approval_json(request: &Request) -> Value {
    json!({
        "id": request.id,
        "actor": request.ask.actor,
        "procedure": request.ask.procedure,
        "subject": request.ask.subject,
        "reason": request.ask.reason,
        "detail": request.ask.detail,
        "runId": request.ask.run_id,
        "state": request.state.proto(),
        "createdAt": super::timestamp(request.created),
        "expiresAt": super::timestamp(request.expires),
        "decidedBy": request.decided_by,
    })
}

/// Envoy, a sleeve, or a token Envoy minted. The owner, approved devices and
/// tokens the owner issued are never gated.
pub(crate) fn is_agent(hub: &Hub, actor: &str, token_id: &str) -> bool {
    super::actor_sleeve(actor).is_some()
        || (!token_id.is_empty()
            && hub
                .identity
                .list()
                .iter()
                .any(|t| t.id == token_id && t.minted_by == "orchestrator-envoy"))
}

/// The wire gate: destructive calls and admin jobs by agents need an
/// approval first. Agents never reach care nodes: placement refuses them.
/// Grant minting asks from its handler, after validation.
pub(crate) fn gate(hub: &Hub, request: &RpcRequest) -> Result<()> {
    let method = &request.method;
    let reason = if method.destructive {
        "destructive"
    } else if method.procedure == "/protectorate.v1.JobService/SubmitJob"
        && input(request)?["admin"] == true
    {
        "node admin job"
    } else {
        return Ok(());
    };
    let auth = &request.auth;
    if auth.owner || auth.device || !is_agent(hub, &auth.actor, &auth.token_id) {
        return Ok(());
    }
    require(hub, request, reason)
}

/// Ok once the owner approved this exact call by this caller.
pub(crate) fn require(hub: &Hub, request: &RpcRequest, reason: &str) -> Result<()> {
    let (method, auth) = (&request.method, &request.auth);
    hub.approvals.check(
        hub,
        Ask {
            actor: auth.actor.clone(),
            token_id: auth.token_id.clone(),
            run_id: super::actor_run(hub, &auth.actor),
            procedure: method.procedure.clone(),
            subject: crate::audit::subject(method, auth, &request.message, false),
            reason: reason.into(),
            detail: String::new(),
            binding: encode_json(&request.message)?,
            hint: hint(&request.headers),
        },
    )
}

/// Who asked for a launch, carried in the launch request as `requester`. A
/// launch without one is the hub's own, such as recovery after a restart,
/// and is gated like an agent's.
pub(crate) fn requester(hub: &Hub, request: &RpcRequest) -> Value {
    let auth = &request.auth;
    json!({
        "actor": auth.actor,
        "tokenId": auth.token_id,
        "gated": !(auth.owner || auth.device) && is_agent(hub, &auth.actor, &auth.token_id),
        "approval": hint(&request.headers),
    })
}

pub(crate) fn file_sha256(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    fs::File::open(path)
        .and_then(|mut file| io::copy(&mut file, &mut hasher))
        .map_err(|e| RpcError::new(Code::Unavailable, format!("hash {}: {e}", path.display())))?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn json_sha256(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string()))
}

/// Compares a launch with the pin of the harness's last launch. A change the
/// owner asked for is recorded; one an agent or the hub itself asked for
/// needs the owner's approval of exactly the new hashes.
pub(crate) fn check_pin(
    hub: &Hub,
    value: &Value,
    name: &str,
    run_id: &str,
    harness: &str,
    pinned: Option<&Pin>,
    pin: &Pin,
) -> Result<()> {
    let Some(old) = pinned.filter(|old| *old != pin) else {
        return Ok(());
    };
    let short = |hash: &str| hash.get(..12).unwrap_or(hash).to_owned();
    let mut changes = Vec::new();
    if old.executable_sha256 != pin.executable_sha256 {
        changes.push(format!(
            "{harness} executable {} -> {}",
            short(&old.executable_sha256),
            short(&pin.executable_sha256)
        ));
    }
    if old.profile_sha256 != pin.profile_sha256 {
        changes.push(format!(
            "{harness} profile {} -> {}",
            short(&old.profile_sha256),
            short(&pin.profile_sha256)
        ));
    }
    let detail = changes.join(", ");
    let requester = &value["requester"];
    let actor = requester["actor"].as_str().unwrap_or("hub");
    let token_id = string(requester, "tokenId");
    if requester["gated"] == false {
        hub.audit.write(AuditEntry {
            actor: actor.into(),
            token_id,
            run_id: run_id.into(),
            procedure: "harness.pin_changed".into(),
            subject: name.into(),
            outcome: "ok".into(),
            detail: detail.clone(),
            ..AuditEntry::default()
        });
        hub.emit(
            "sleeve.pin_changed",
            name,
            json!({"harness": harness, "detail": detail}),
        );
        return Ok(());
    }
    hub.approvals.check(
        hub,
        Ask {
            actor: actor.into(),
            token_id,
            run_id: run_id.into(),
            procedure: "harness.pin".into(),
            subject: name.into(),
            reason: "harness or profile changed".into(),
            detail,
            binding: [name, harness, &pin.executable_sha256, &pin.profile_sha256]
                .join("\0")
                .into_bytes(),
            hint: string(requester, "approval"),
        },
    )
}

pub(crate) fn hint(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let list = Arc::clone(&hub);
    server.register(
        "/protectorate.v1.ApprovalService/ListApprovals",
        move |request| {
            let hub = Arc::clone(&list);
            async move {
                let all = input(&request)?["all"] == true;
                let mut requests = hub.approvals.requests.lock().unwrap();
                hub.approvals.sweep(&hub, &mut requests, Utc::now());
                let mut rows = requests
                    .values()
                    .filter(|r| all || r.state == State::Pending)
                    .collect::<Vec<_>>();
                rows.sort_by_key(|r| r.created);
                let approvals = rows.into_iter().map(approval_json).collect::<Vec<_>>();
                drop(requests);
                respond(&request, json!({"approvals": approvals}))
            }
        },
    )?;
    server.register(DECIDE, move |request| {
        let hub = Arc::clone(&hub);
        async move {
            let value = input(&request)?;
            let approve = value["approve"] == true;
            let decided =
                hub.approvals
                    .decide(&hub, &string(&value, "id"), approve, &request.auth.actor)?;
            hub.audit.write(AuditEntry {
                actor: request.auth.actor.clone(),
                token_id: request.auth.token_id.clone(),
                run_id: decided.ask.run_id.clone(),
                procedure: DECIDE.into(),
                subject: decided.id.clone(),
                outcome: "ok".into(),
                detail: format!(
                    "{} {} for {}",
                    if approve { "approved" } else { "denied" },
                    summary(&decided.ask),
                    decided.ask.actor
                ),
                ..AuditEntry::default()
            });
            respond(&request, json!({"approval": approval_json(&decided)}))
        }
    })
}
