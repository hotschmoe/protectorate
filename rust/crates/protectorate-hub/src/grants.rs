//! Task-scoped admin grants. GrantService is explicit about who may mint them.
use super::{
    input, now_unix, parse_duration, respond, scope_methods, string, token_error, token_json, Hub,
};
use protectorate_inventory::SHARED;
use protectorate_wire::{Code, Method, Result, RpcError, UnaryServer};
use serde_json::json;
use std::{sync::Arc, time::Duration};

const DEFAULT_TTL: Duration = Duration::from_secs(4 * 60 * 60);

pub(super) fn register(
    server: &mut UnaryServer,
    hub: Arc<Hub>,
    all_methods: Arc<Vec<Method>>,
) -> Result<()> {
    server.register("/protectorate.v1.GrantService/Admin", move |request| {
        let hub = Arc::clone(&hub);
        let methods = Arc::clone(&all_methods);
        async move {
            let body = input(&request)?;
            let node = string(&body, "node").trim().to_ascii_lowercase();
            let task = string(&body, "task");
            let minted_by = if request.auth.owner {
                "owner"
            } else if request.auth.actor == "token:orchestrator-envoy" {
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
                crate::approvals::require(&hub, &request, "admin grant")?;
                "orchestrator-envoy"
            } else {
                return Err(RpcError::new(
                    Code::PermissionDenied,
                    "only the owner or enabled orchestrator may mint admin grants",
                ));
            };
            let ttl = if body.get("ttl").is_some() {
                parse_duration(body.get("ttl"))?
            } else {
                DEFAULT_TTL
            };
            let methods = scope_methods(&methods);
            let issued = hub
                .identity
                .create_grant(&node, &task, minted_by, ttl, now_unix(), &methods)
                .map_err(token_error)?;
            hub.emit("grant.minted", &issued.token.id, json!({"node": node}));
            respond(
                &request,
                json!({"token": token_json(&issued.token), "secret": issued.secret.expose()}),
            )
        }
    })
}
