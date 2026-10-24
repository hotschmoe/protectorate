//! Who is calling. The owner token works only on the loopback listener; an
//! approved device signs each request with its key; everything else holds a
//! capability the hub minted. Every hub entry point admits callers here.
use crate::{now_unix, unix_time, Hub};
use axum::http::{header, HeaderMap};
use prost_reflect::prost_types;
use protectorate_auth::{
    self as auth,
    capability::{self, Claims},
    proof::{self, Proof},
    Method as ScopeMethod,
};
use protectorate_identity::{self as identity, Capability, Device, Mint, Secret, Store};
use protectorate_wire::{
    self as wire,
    v1::{self, token_service},
    AuthContext, Call, Code, Denied, Method, Result, RpcError, UnaryServer,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

pub(crate) const ENVOY: &str = "orchestrator-envoy";
const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);
const RENEW: &str = "token_renew_token";

pub(crate) enum Caller {
    Owner,
    Device(Device),
    Capability {
        record: Capability,
        scopes: Vec<String>,
    },
}

pub(crate) fn scope_methods(methods: &[Method]) -> Vec<ScopeMethod<'_>> {
    methods
        .iter()
        .map(|method| ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        })
        .collect()
}

fn unauthenticated(message: impl Into<String>) -> RpcError {
    RpcError::new(Code::Unauthenticated, message)
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let text = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let prefix = text.get(..7)?;
    (prefix.eq_ignore_ascii_case("Bearer ") && text.len() > 7).then(|| &text[7..])
}

/// A browser cannot set headers on a WebSocket, so it offers the proof as a
/// subprotocol; the hub never selects it.
fn websocket_proof(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(','))
        .find_map(|entry| entry.trim().strip_prefix(proof::SUBPROTOCOL))
}

/// The scopes a capability carries after every narrowing its holders added.
fn effective(claims: &Claims, methods: &[Method]) -> Vec<String> {
    let methods = scope_methods(methods);
    claims
        .narrowed
        .iter()
        .fold(claims.scopes.clone(), |scopes, narrower| {
            auth::intersect(&scopes, narrower, &methods)
        })
}

/// Authenticates one request to `path` with HTTP `verb`. `tls` is whether it
/// arrived on a TLS listener, where the owner token is refused.
pub(crate) fn authenticate(
    owner: &str,
    store: &Store,
    methods: &[Method],
    headers: &HeaderMap,
    verb: &str,
    path: &str,
    tls: bool,
) -> Result<Caller> {
    let now = now_unix();
    let signed = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(proof::SCHEME))
        .or_else(|| (verb == "GET").then(|| websocket_proof(headers)).flatten());
    if let Some(text) = signed {
        let proof = Proof::parse(text).ok_or_else(|| unauthenticated("malformed device proof"))?;
        return store
            .verify_proof(&proof, verb, path, now)
            .map(Caller::Device)
            .map_err(|error| unauthenticated(error.to_string()));
    }
    let Some(secret) = bearer(headers) else {
        return Err(unauthenticated(
            "missing credential: sign the request with a device key or present a capability",
        ));
    };
    if secret.starts_with(capability::PREFIX) {
        let (record, claims) = store
            .verify(secret, now)
            .map_err(|error| unauthenticated(error.to_string()))?;
        let scopes = effective(&claims, methods);
        return Ok(Caller::Capability { record, scopes });
    }
    if tls {
        return Err(unauthenticated(
            "the owner token is accepted only on the hub's loopback listener",
        ));
    }
    if !owner.is_empty() && bool::from(owner.as_bytes().ct_eq(secret.as_bytes())) {
        return Ok(Caller::Owner);
    }
    Err(unauthenticated("invalid credential"))
}

/// Refuses a request that arrived on a TLS listener, for the routes only
/// the owner token opens.
pub(crate) async fn loopback_only(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if request
        .extensions()
        .get::<protectorate_wire::Peer>()
        .is_some_and(|peer| peer.tls)
    {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

pub(crate) fn context(caller: &Caller) -> AuthContext {
    match caller {
        Caller::Owner => AuthContext {
            actor: "owner".into(),
            owner: true,
            ..AuthContext::default()
        },
        Caller::Device(device) => AuthContext {
            actor: format!("token:{}", device.name),
            token_id: device.id.clone(),
            scopes: vec![auth::SCOPE_FULL.into()],
            device: true,
            ..AuthContext::default()
        },
        Caller::Capability { record, scopes } => AuthContext {
            actor: format!("token:{}", record.principal),
            token_id: record.id.clone(),
            task: record.task.clone(),
            scopes: scopes.clone(),
            ..AuthContext::default()
        },
    }
}

fn can_kill_owned_sleeve(scopes: &[&str]) -> bool {
    auth::scopes_allow(
        scopes,
        &ScopeMethod {
            tool: "sleeve_spawn_sleeve",
            service: "SleeveService",
            read_only: false,
            owner_only: false,
        },
    )
}

/// Whether an authenticated caller may call `method`.
pub(crate) fn authorize(
    caller: &Caller,
    method: &Method,
) -> std::result::Result<AuthContext, Box<Denied>> {
    let auth = context(caller);
    let allowed = match caller {
        Caller::Owner => method.tool != RENEW,
        Caller::Device(_) => method.tool != RENEW && (!method.owner_only || method.device_allowed),
        Caller::Capability { record, scopes } => {
            let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
            !method.owner_only
                && method.service != "DeviceService"
                && (method.tool == RENEW
                    || (method.tool == "sleeve_kill_sleeve" && can_kill_owned_sleeve(&scopes))
                    || (method.tool == "chat_report" && record.principal.starts_with("sleeve:"))
                    || auth::scopes_allow(
                        &scopes,
                        &ScopeMethod {
                            tool: &method.tool,
                            service: &method.service,
                            read_only: method.read_only,
                            owner_only: method.owner_only,
                        },
                    ))
        }
    };
    if !allowed {
        let who = auth.actor.strip_prefix("token:").unwrap_or(&auth.actor);
        let error = if method.tool == RENEW {
            RpcError::new(Code::PermissionDenied, "only a capability can be renewed")
        } else {
            RpcError::new(
                Code::PermissionDenied,
                format!("token {who:?} lacks the required scope: {}", method.tool),
            )
        };
        return Err(Box::new(Denied { error, auth }));
    }
    Ok(auth)
}

/// The scopes the hub may mint for a caller: what it asked for, cut down to
/// what the caller holds itself. The owner and devices hold everything.
pub(crate) fn delegate(hub: &Hub, wanted: &[String], holder: &AuthContext) -> Vec<String> {
    if holder.owner || holder.device {
        return wanted.to_vec();
    }
    auth::intersect(wanted, &holder.scopes, &scope_methods(&hub.methods))
}

/// Mints the capability `principal` holds in `path`, revoking any earlier
/// one; the hub keeps the file fresh until the lineage is revoked.
pub(crate) fn issue_held(
    hub: &Hub,
    principal: &str,
    scopes: Vec<String>,
    minted_by: &str,
    path: &Path,
) -> Result<Capability> {
    revoke_held(hub, principal)?;
    let now = now_unix();
    let (record, secret) = hub
        .identity
        .mint(
            Mint {
                principal,
                scopes,
                task: "",
                minted_by,
                ttl: hub.capability_ttl,
                file: Some(path.to_path_buf()),
            },
            now,
        )
        .map_err(|error| RpcError::new(Code::Internal, format!("mint {principal}: {error}")))?;
    if let Err(error) = write_held(path, &secret) {
        let _ = hub.identity.revoke(&record.id, now);
        return Err(error);
    }
    Ok(record)
}

/// A sleeve's scopes: the sleeve scopes, limited to what its spawner holds.
pub(crate) fn sleeve_scopes(hub: &Hub, parent: &crate::sleeves::SpawnParent) -> Vec<String> {
    let wanted = crate::sleeves::SLEEVE_SCOPES.map(str::to_owned).to_vec();
    match &parent.scopes {
        Some(held) => auth::intersect(&wanted, held, &scope_methods(&hub.methods)),
        None => wanted,
    }
}

/// The scopes of Envoy's live capability, empty when it holds none.
pub(crate) fn envoy_scopes(hub: &Hub) -> Vec<String> {
    let now = now_unix();
    hub.identity
        .capabilities()
        .into_iter()
        .find(|c| c.principal == ENVOY && c.is_active(now))
        .map(|c| c.scopes)
        .unwrap_or_default()
}

pub(crate) fn issue_sleeve(
    hub: &Hub,
    name: &str,
    parent: &crate::sleeves::SpawnParent,
    path: &Path,
) -> Result<Capability> {
    let scopes = sleeve_scopes(hub, parent);
    issue_held(hub, &format!("sleeve:{name}"), scopes, &parent.label, path)
}

/// Revokes every lineage of `principal` and removes the files the hub keeps.
pub(crate) fn revoke_held(hub: &Hub, principal: &str) -> Result<()> {
    let now = now_unix();
    for record in hub.identity.capabilities() {
        if record.principal != principal || record.revoked_at.is_some() {
            continue;
        }
        hub.identity
            .revoke(&record.id, now)
            .map_err(|error| RpcError::new(Code::Internal, error.to_string()))?;
        if let Some(file) = &record.file {
            match std::fs::remove_file(file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(RpcError::new(Code::Internal, error.to_string())),
            }
        }
    }
    Ok(())
}

fn write_held(path: &Path, secret: &Secret) -> Result<()> {
    crate::sleeves::envoy_write_if_changed(path, format!("{}\n", secret.expose()).as_bytes(), 0o600)
}

/// Rewrites each hub-held token file at half its lifetime. A lineage whose
/// file cannot be written is revoked, since nobody holds it any more.
pub(crate) fn refresh_held(hub: &Hub, now: i64) {
    for record in hub.identity.capabilities() {
        let Some(file) = &record.file else { continue };
        if record.revoked_at.is_some() || record.expires.saturating_sub(now) > record.ttl / 2 {
            continue;
        }
        let written = hub
            .identity
            .refresh(&record.id, now)
            .map_err(|error| RpcError::new(Code::Internal, error.to_string()))
            .and_then(|secret| write_held(file, &secret));
        if let Err(error) = written {
            eprintln!("refresh {} capability: {}", record.principal, error.message);
            let _ = hub.identity.revoke(&record.id, now);
        }
    }
}

/// How often hub-held capabilities are checked: often enough to refresh each
/// one well before it expires.
pub(crate) fn refresh_interval(ttl: Duration) -> Duration {
    (ttl / 8).clamp(Duration::from_millis(250), Duration::from_secs(60))
}

pub(crate) fn token_info(token: &Capability) -> v1::TokenInfo {
    v1::TokenInfo {
        id: token.id.clone(),
        name: token.principal.clone(),
        scopes: token.scopes.clone(),
        created_at: Some(unix_time(token.created)),
        expires_at: Some(unix_time(token.expires)),
        revoked_at: token.revoked_at.map(unix_time),
        task: token.task.clone(),
        minted_by: token.minted_by.clone(),
    }
}

pub(crate) fn token_error(error: identity::Error) -> RpcError {
    let code = match error {
        identity::Error::UnknownToken => Code::NotFound,
        identity::Error::Invalid(_) | identity::Error::Expired | identity::Error::Revoked => {
            Code::InvalidArgument
        }
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

/// A requested lifetime: `default` when absent, never negative.
pub(crate) fn ttl(value: Option<&prost_types::Duration>, default: Duration) -> Result<Duration> {
    value.map_or(Ok(default), |value| wire::duration(Some(value), "ttl"))
}

fn reserved(name: &str) -> bool {
    name == ENVOY || name.starts_with("sleeve:") || name.starts_with("grant:")
}

pub(super) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let create = Arc::clone(&hub);
    server.unary(
        token_service::CREATE_TOKEN,
        move |call: Call<v1::CreateTokenRequest>| {
            let hub = Arc::clone(&create);
            async move {
                let request = call.request;
                let name = request.name.trim();
                if name.is_empty() || reserved(name) {
                    return Err(RpcError::invalid(
                        "a capability needs a name that is not a sleeve, grant or Envoy identity",
                    ));
                }
                let scopes = auth::validate_scopes(
                    &request
                        .scopes
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    &scope_methods(&hub.methods),
                )
                .map_err(|error| RpcError::invalid(error.to_string()))?;
                let (record, secret) = hub
                    .identity
                    .mint(
                        Mint {
                            principal: name,
                            scopes,
                            task: "",
                            minted_by: "owner",
                            ttl: ttl(request.ttl.as_ref(), DEFAULT_TTL)?,
                            file: None,
                        },
                        now_unix(),
                    )
                    .map_err(token_error)?;
                Ok(v1::CreateTokenResponse {
                    token: Some(token_info(&record)),
                    secret: secret.expose().to_owned(),
                })
            }
        },
    )?;
    let list = Arc::clone(&hub);
    server.unary(token_service::LIST_TOKENS, move |_: Call<_>| {
        let hub = Arc::clone(&list);
        async move {
            Ok(v1::ListTokensResponse {
                tokens: hub.identity.capabilities().iter().map(token_info).collect(),
            })
        }
    })?;
    let revoke = Arc::clone(&hub);
    server.unary(
        token_service::REVOKE_TOKEN,
        move |call: Call<v1::RevokeTokenRequest>| {
            let hub = Arc::clone(&revoke);
            async move {
                let token = hub
                    .identity
                    .revoke(&call.request.id, now_unix())
                    .map_err(token_error)?;
                Ok(v1::RevokeTokenResponse {
                    token: Some(token_info(&token)),
                })
            }
        },
    )?;
    server.unary(token_service::RENEW_TOKEN, move |call: Call<_>| {
        let hub = Arc::clone(&hub);
        async move {
            let presented = bearer(&call.headers)
                .ok_or_else(|| RpcError::new(Code::Unauthenticated, "no capability presented"))?;
            let (record, secret) = hub
                .identity
                .renew(presented, now_unix())
                .map_err(|error| RpcError::new(Code::Unauthenticated, error.to_string()))?;
            Ok(v1::RenewTokenResponse {
                token: Some(token_info(&record)),
                secret: secret.expose().to_owned(),
            })
        }
    })
}

pub(crate) fn start_refresh(runtime: &tokio::runtime::Handle, hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    let every = refresh_interval(hub.capability_ttl);
    runtime.spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut hosts = Instant::now();
        loop {
            interval.tick().await;
            let Some(hub) = weak.upgrade() else { break };
            let write_hosts = hosts.elapsed() >= Duration::from_secs(60);
            if write_hosts {
                hosts = Instant::now();
            }
            let done = tokio::task::spawn_blocking(move || {
                refresh_held(&hub, now_unix());
                if write_hosts {
                    if let Err(error) = crate::envoy::write_hosts(&hub) {
                        eprintln!("Envoy hosts: {}", error.message);
                    }
                }
            })
            .await;
            if done.is_err() {
                break;
            }
        }
    });
}
