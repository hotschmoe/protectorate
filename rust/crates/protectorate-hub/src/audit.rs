use super::{input, respond, string, Hub};
use prost_reflect::Value as FieldValue;
use protectorate_audit::{Audit, Entry, Error, Query};
use protectorate_wire::{AuthContext, Code, DynamicMessage, Method, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    server.register(
        "/protectorate.v1.AuditService/ListAuditEntries",
        move |request| {
            let hub = Arc::clone(&hub);
            async move {
                let value = input(&request)?;
                let limit = value.get("limit").and_then(Value::as_i64).unwrap_or(0);
                if limit < 0 {
                    return Err(RpcError::invalid("limit must not be negative"));
                }
                let outcome = string(&value, "outcome");
                if !matches!(outcome.as_str(), "" | "ok" | "denied" | "error") {
                    return Err(RpcError::invalid("outcome must be ok, denied or error"));
                }
                let query = Query {
                    limit: if limit == 0 {
                        50
                    } else {
                        limit.min(500) as usize
                    },
                    before: string(&value, "pageToken"),
                    actor: string(&value, "actor"),
                    procedure: string(&value, "procedure"),
                    subject: string(&value, "subject"),
                    outcome,
                };
                let page = tokio::task::spawn_blocking(move || hub.audit.page(query))
                    .await
                    .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?
                    .map_err(|e| match e {
                        Error::Cursor(_) => RpcError::invalid(e.to_string()),
                        _ => RpcError::new(Code::Internal, e.to_string()),
                    })?;
                respond(
                    &request,
                    json!({
                        "entries": page.entries.iter().map(|entry| json!({
                            "time": entry.time.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                            "actor": entry.actor,
                            "tokenId": entry.token_id,
                            "procedure": entry.procedure,
                            "subject": entry.subject,
                            "outcome": entry.outcome,
                            "code": entry.code,
                            "task": entry.task,
                            "detail": entry.detail,
                        })).collect::<Vec<_>>(),
                        "nextPageToken": page.next,
                    }),
                )
            }
        },
    )
}

pub(super) fn record_completion(
    audit: &Audit,
    method: &Method,
    auth: &AuthContext,
    message: &DynamicMessage,
    error: Option<&RpcError>,
) {
    // Handlers record successful operations whose resolved subject or detail
    // is known only after they finish. Keep this observer for failures.
    if error.is_none()
        && matches!(
            method.procedure.as_str(),
            "/protectorate.v1.JobService/SubmitJob"
                | "/protectorate.v1.DeviceService/ApproveDeviceAuth"
                | "/protectorate.v1.DeviceService/DenyDeviceAuth"
                | "/protectorate.v1.TokenService/UpdateTokenScopes"
                | "/protectorate.v1.ReleaseService/UpdateHub"
                | "/protectorate.v1.SleeveService/SpawnEnvoy"
        )
    {
        return;
    }
    // A node file download is sensitive even though it does not mutate state.
    // The Go hub records this stream explicitly because generic read-only
    // calls are normally omitted from the audit log.
    if !audited_method(method, auth) {
        return;
    }
    if method.procedure == "/protectorate.v1.NodeService/PutFile" && error.is_none() {
        let continuing = matches!(
            message.get_field_by_name("offset").as_deref(),
            Some(FieldValue::U64(offset)) if *offset > 0
        );
        if continuing
            && !matches!(
                message.get_field_by_name("last").as_deref(),
                Some(FieldValue::Bool(true))
            )
        {
            return;
        }
    }
    let subject = subject(method, auth, message, error.is_none());
    // The grant task is a validated, at-most-500-byte description of why the
    // capability was minted. Never audit the response, which holds its secret.
    let detail = if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
        pairing_detail(auth)
    } else if method.procedure == "/protectorate.v1.GrantService/Admin" && error.is_none() {
        format!("task: {}", field_string(message, "task").trim())
    } else {
        String::new()
    };
    audit.write(Entry {
        actor: auth.actor.clone(),
        token_id: auth.token_id.clone(),
        task: auth.task.clone(),
        procedure: method.procedure.clone(),
        subject,
        detail,
        // Admission failures are recorded by record_denied. Once a handler
        // runs, even a permission_denied response is a failed operation; Go's
        // audit log also distinguishes this from a rejected credential/scope.
        outcome: if error.is_some() { "error" } else { "ok" }.into(),
        code: error.map(|error| code(error.code)).unwrap_or_default(),
        ..Entry::default()
    });
}

fn field_string(message: &DynamicMessage, key: &str) -> String {
    match message.get_field_by_name(key).as_deref() {
        Some(FieldValue::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn subject(method: &Method, auth: &AuthContext, request: &DynamicMessage, success: bool) -> String {
    let subject = ["name", "id", "job_id", "node", "resource", "transfer_id"]
        .iter()
        .find_map(|key| {
            request
                .get_field_by_name(key)
                .and_then(|value| match value.as_ref() {
                    FieldValue::String(value) => Some(value.clone()),
                    FieldValue::I64(value) => Some(value.to_string()),
                    FieldValue::U64(value) => Some(value.to_string()),
                    FieldValue::Message(value) => Some(field_string(value, "name")),
                    _ => None,
                })
        })
        .unwrap_or_default();
    let subject = if method
        .procedure
        .starts_with("/protectorate.v1.DeskService/")
    {
        match request.get_field_by_name("target").as_deref() {
            Some(FieldValue::Message(target)) => field_string(target, "name"),
            _ => String::new(),
        }
    } else if method.procedure == "/protectorate.v1.ReleaseService/UpdateHub" {
        "hub".to_owned()
    } else if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
        String::new()
    } else if matches!(
        method.procedure.as_str(),
        "/protectorate.v1.DeviceService/RenameDevice"
            | "/protectorate.v1.DeviceService/RevokeDevice"
    ) {
        field_string(request, "id")
    } else if method.procedure == "/protectorate.v1.GrantService/Admin" && success {
        field_string(request, "node").trim().to_ascii_lowercase()
    } else if method.procedure == "/protectorate.v1.ChatService/Report" {
        let self_name = if auth.actor == "token:orchestrator-envoy" {
            Some("envoy")
        } else {
            auth.actor.strip_prefix("token:sleeve:")
        };
        self_name
            .filter(|self_name| {
                let target = field_string(request, "sleeve");
                target.is_empty() || target == *self_name
            })
            .unwrap_or_default()
            .to_owned()
    } else {
        subject
    };
    subject
}

pub(super) fn record_denied(
    audit: &Audit,
    method: &Method,
    auth: &AuthContext,
    message: Option<&DynamicMessage>,
    error: &RpcError,
) {
    if !audited_method(method, auth) {
        return;
    }
    audit.write(Entry {
        actor: auth.actor.clone(),
        token_id: auth.token_id.clone(),
        task: auth.task.clone(),
        procedure: method.procedure.clone(),
        subject: message
            .map(|request| subject(method, auth, request, false))
            .unwrap_or_default(),
        detail: if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
            pairing_detail(auth)
        } else {
            String::new()
        },
        // Public pairing admission can reject a LAN peer or throttle a busy
        // source without authenticating a credential. Go records these as
        // operation errors; only credential and scope failures are denials.
        outcome: if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
            "error"
        } else {
            "denied"
        }
        .into(),
        code: code(error.code),
        ..Entry::default()
    });
}

pub(super) fn record_rejection(
    audit: &Audit,
    method: &Method,
    auth: Option<&AuthContext>,
    error: &RpcError,
) {
    if !audited_method(method, auth.unwrap_or(&AuthContext::default())) {
        return;
    }
    audit.write(Entry {
        actor: auth
            .map_or("unauthenticated", |auth| auth.actor.as_str())
            .into(),
        token_id: auth.map_or("", |auth| auth.token_id.as_str()).into(),
        task: auth.map_or("", |auth| auth.task.as_str()).into(),
        procedure: method.procedure.clone(),
        detail: if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
            auth.map(pairing_detail).unwrap_or_default()
        } else {
            String::new()
        },
        outcome: if matches!(error.code, Code::Unauthenticated | Code::PermissionDenied) {
            "denied"
        } else {
            "error"
        }
        .into(),
        code: code(error.code),
        ..Entry::default()
    });
}

fn pairing_detail(auth: &AuthContext) -> String {
    auth.requesting_ip
        .map(|ip| format!("requesting_ip={ip}"))
        .unwrap_or_default()
}

fn audited_method(method: &Method, auth: &AuthContext) -> bool {
    // The public poll request contains a bearer-like device code and may be
    // repeated frequently while the user approves pairing. Go only records
    // StartDeviceAuth on this public path.
    if method.procedure == "/protectorate.v1.DeviceService/PollDeviceAuth" {
        return false;
    }
    !method.read_only
        || !auth.task.is_empty()
        || method.procedure == "/protectorate.v1.NodeService/GetFile"
}

pub(super) fn record_terminal(
    audit: &Audit,
    actor: &str,
    token_id: &str,
    subject: &str,
    read_only: bool,
    result: (&str, &str),
) {
    audit.write(Entry {
        actor: actor.into(),
        token_id: token_id.into(),
        procedure: if read_only {
            "terminal.attach.readonly"
        } else {
            "terminal.attach"
        }
        .into(),
        subject: subject.into(),
        outcome: result.0.into(),
        code: result.1.into(),
        ..Entry::default()
    });
}

fn code(code: Code) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
