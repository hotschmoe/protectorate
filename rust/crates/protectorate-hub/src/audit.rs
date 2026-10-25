use super::{proto_time, Hub};
use prost_reflect::Value as FieldValue;
use protectorate_audit::{Audit, Entry, Error, Head, Query};
use protectorate_wire::{
    v1::{self, audit_service},
    AuthContext, Call, Code, DynamicMessage, Method, Result, RpcError, UnaryServer,
};
use std::sync::Arc;

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let list = Arc::clone(&hub);
    server.unary(
        audit_service::LIST_AUDIT_ENTRIES,
        move |call: Call<v1::ListAuditEntriesRequest>| {
            let hub = Arc::clone(&list);
            async move {
                let request = call.request;
                if request.limit < 0 {
                    return Err(RpcError::invalid("limit must not be negative"));
                }
                if !matches!(request.outcome.as_str(), "" | "ok" | "denied" | "error") {
                    return Err(RpcError::invalid("outcome must be ok, denied or error"));
                }
                let query = Query {
                    limit: if request.limit == 0 {
                        50
                    } else {
                        request.limit.min(500) as usize
                    },
                    before: request.page_token,
                    actor: request.actor,
                    procedure: request.procedure,
                    subject: request.subject,
                    outcome: request.outcome,
                    run_id: request.run_id,
                };
                let (page, current) = tokio::task::spawn_blocking(move || {
                    Ok::<_, Error>((hub.audit.page(query)?, hub.audit.head()?))
                })
                .await
                .map_err(super::join_error)?
                .map_err(|e| match e {
                    Error::Cursor(_) => RpcError::invalid(e.to_string()),
                    _ => RpcError::new(Code::Internal, e.to_string()),
                })?;
                Ok(v1::ListAuditEntriesResponse {
                    entries: page
                        .entries
                        .into_iter()
                        .map(|entry| v1::AuditEntry {
                            time: Some(proto_time(entry.time)),
                            actor: entry.actor,
                            token_id: entry.token_id,
                            procedure: entry.procedure,
                            subject: entry.subject,
                            outcome: entry.outcome,
                            code: entry.code,
                            task: entry.task,
                            detail: entry.detail,
                            run_id: entry.run_id,
                            seq: entry.seq,
                            prev_hash: entry.prev,
                            hash: entry.hash,
                        })
                        .collect(),
                    next_page_token: page.next,
                    head: Some(head(current.unwrap_or_default())),
                })
            }
        },
    )?;
    let verify = Arc::clone(&hub);
    server.unary(audit_service::VERIFY_AUDIT, move |_: Call<_>| {
        let hub = Arc::clone(&verify);
        async move {
            let result = tokio::task::spawn_blocking(move || hub.audit.verify())
                .await
                .map_err(super::join_error)?;
            match result {
                Ok(report) => Ok(v1::VerifyAuditResponse {
                    ok: true,
                    entries: report.entries,
                    head: Some(head(report.head)),
                    beyond_head: report.beyond_head,
                    ..Default::default()
                }),
                Err(Error::Broken(reason)) => Ok(v1::VerifyAuditResponse {
                    error: reason,
                    ..Default::default()
                }),
                Err(error) => Err(RpcError::new(Code::Internal, error.to_string())),
            }
        }
    })
}

pub(super) fn head(head: Head) -> v1::AuditHead {
    v1::AuditHead {
        seq: head.seq,
        hash: head.hash,
    }
}

pub(super) fn record_completion(
    audit: &Audit,
    method: &Method,
    auth: &AuthContext,
    message: &DynamicMessage,
    error: Option<&RpcError>,
) {
    // A panicking handler is always recorded, read or write, so a recovered
    // panic is visible in the audit chain and the log.
    let panicked = error.is_some_and(RpcError::is_panic);
    if let Some(error) = error.filter(|_| panicked) {
        eprintln!(
            "hub: {} by {} panicked: {}",
            method.procedure, auth.actor, error.message
        );
    }
    // Handlers record successful operations whose resolved subject or detail
    // is known only after they finish. Keep this observer for failures.
    if error.is_none()
        && matches!(
            method.procedure.as_str(),
            "/protectorate.v1.JobService/SubmitJob"
                | "/protectorate.v1.DeviceService/ApproveDeviceAuth"
                | "/protectorate.v1.DeviceService/DenyDeviceAuth"
                | "/protectorate.v1.ReleaseService/UpdateHub"
                | "/protectorate.v1.SleeveService/SpawnEnvoy"
                | "/protectorate.v1.ApprovalService/DecideApproval"
        )
    {
        return;
    }
    // A node file download is sensitive even though it does not mutate
    // state, so it is recorded though reads normally are not.
    if !panicked && !audited_method(method, auth) {
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
    let detail = if let Some(error) = error.filter(|_| panicked) {
        error.message.clone()
    } else if method.procedure == "/protectorate.v1.DeviceService/StartDeviceAuth" {
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
        // runs, even a permission_denied response is a failed operation.
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

pub(super) fn subject(
    method: &Method,
    auth: &AuthContext,
    request: &DynamicMessage,
    success: bool,
) -> String {
    let subject = [
        "name",
        "sleeve",
        "id",
        "job_id",
        "node",
        "resource",
        "transfer_id",
    ]
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
    let subject = if method.procedure == "/protectorate.v1.ReleaseService/UpdateHub" {
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
        super::actor_sleeve(&auth.actor)
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
        // source without authenticating a credential: an operation error.
        // Only credential and scope failures are denials.
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
    // repeated frequently while the user approves pairing; only
    // StartDeviceAuth is recorded on this public path.
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
