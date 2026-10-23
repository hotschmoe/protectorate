//! Task-scoped admin grants. GrantService is explicit about who may mint them.
use super::{
    access::{self, scope_methods, token_json},
    input, now_unix, respond, string, Hub,
};
use protectorate_identity::Mint;
use protectorate_inventory::SHARED;
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::json;
use std::{sync::Arc, time::Duration};

const DEFAULT_TTL: Duration = Duration::from_secs(4 * 60 * 60);

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    server.register("/protectorate.v1.GrantService/Admin", move |request| {
        let hub = Arc::clone(&hub);
        async move {
            let body = input(&request)?;
            let node = string(&body, "node").trim().to_ascii_lowercase();
            let task = string(&body, "task");
            if task.trim().is_empty() || task.len() > 500 {
                return Err(RpcError::invalid("a grant needs a task of 1-500 bytes"));
            }
            let ttl = access::ttl(&body, DEFAULT_TTL)?;
            let methods = scope_methods(&hub.methods);
            let mut scopes = protectorate_auth::grant_scopes(&node, &methods)
                .map_err(|error| RpcError::invalid(error.to_string()))?;
            let minted_by = if request.auth.owner {
                "owner"
            } else if request.auth.actor == format!("token:{}", access::ENVOY) {
                if !hub.orchestrator_admin_grants {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "the hub does not let the orchestrator mint admin grants",
                    ));
                }
                let bytes = node.as_bytes();
                if bytes.is_empty()
                    || bytes.len() > 63
                    || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit()
                    || !bytes.iter().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(byte)
                    })
                {
                    return Err(RpcError::new(
                        Code::InvalidArgument,
                        "the orchestrator must name one node, not a pattern",
                    ));
                }
                let info = hub
                    .inventory
                    .get(&node)
                    .map_err(|_| RpcError::new(Code::NotFound, format!("no node {node:?}")))?;
                if info.tier != SHARED {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        format!(
                            "node {node} is tier {}; only the owner grants admin on it",
                            info.tier
                        ),
                    ));
                }
                // The node_admin scope is this rule's to give; the job
                // scopes must be ones the orchestrator holds itself.
                let (admin, jobs): (Vec<String>, Vec<String>) =
                    scopes.into_iter().partition(|scope| {
                        scope.starts_with(protectorate_auth::SCOPE_NODE_ADMIN_PREFIX)
                    });
                let jobs = access::delegate(&hub, &jobs, &request.auth);
                if jobs.is_empty() {
                    return Err(RpcError::new(
                        Code::PermissionDenied,
                        "the orchestrator holds no job scopes to delegate",
                    ));
                }
                scopes = jobs.into_iter().chain(admin).collect();
                if ttl.is_zero() || ttl > protectorate_identity::MAX_TTL {
                    return Err(RpcError::invalid(
                        "a grant lifetime must be positive and at most 24 hours",
                    ));
                }
                crate::approvals::require(&hub, &request, "admin grant")?;
                access::ENVOY
            } else {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "only the owner or enabled orchestrator may mint admin grants",
                ));
            };
            let (record, secret) = hub
                .identity
                .mint(
                    Mint {
                        principal: &format!("grant:{node}"),
                        scopes,
                        task: &task,
                        minted_by,
                        ttl,
                        file: None,
                    },
                    now_unix(),
                )
                .map_err(access::token_error)?;
            hub.emit("grant.minted", &record.id, json!({"node": node}));
            respond(
                &request,
                json!({"token": token_json(&record), "secret": secret.expose()}),
            )
        }
    })
}
