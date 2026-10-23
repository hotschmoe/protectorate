//! Streamable HTTP transport: one bearer-authenticated endpoint that answers
//! JSON-RPC over POST, streams server-to-client messages over GET, and ends a
//! session on DELETE.
//!
//! A session is created by a successful `initialize` POST, which returns its
//! id in `Mcp-Session-Id`; every later request must carry that id. The
//! stateless 2026-07-28 shape needs no session: those requests carry their
//! client info in `_meta` and are answered on their own connection.
use crate::jsonrpc::{self, Failure, Request};
use crate::session::{request_key, Guard, Session};
use crate::{is_modern, notify, Bridge, MAX_MESSAGE_BYTES, MAX_PENDING_REQUESTS};
use axum::body::{Body, Bytes};
use axum::extract::{Request as HttpRequest, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use base64::Engine;
use futures_core::Stream;
use protectorate_wire::{Code, Result, RpcError};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{interval_at, MissedTickBehavior};
/// Sessions a bridge keeps at once; a full bridge refuses new ones with 503.
pub const MAX_HTTP_SESSIONS: usize = 128;
/// How long a session with no request in progress survives.
pub const DEFAULT_SESSION_IDLE: Duration = Duration::from_secs(60 * 60);
/// Session ids are this many random bytes, hex encoded.
pub const SESSION_ID_BYTES: usize = 24;
/// Comment frames on an idle event stream, which is also how a bridge notices
/// that its client is gone.
pub const KEEPALIVE: Duration = Duration::from_secs(15);

const SESSION_ID: &str = "mcp-session-id";
const MODERN_VERSION: &str = "2026-07-28";
const MAX_SESSION_ID_CHARS: usize = 128;
const EVENT_BUFFER: usize = 32;

/// How the HTTP endpoint is guarded and bounded. The bearer is the bridge's
/// own hub token: the endpoint acts with it, so it is worth as much as the
/// token file it came from.
#[derive(Clone)]
pub struct HttpConfig {
    pub token: String,
    pub max_sessions: usize,
    pub max_pending_requests: usize,
    pub session_idle: Duration,
    pub max_body_bytes: usize,
}

impl HttpConfig {
    pub fn new(token: impl Into<String>) -> Result<HttpConfig> {
        let token = token.into();
        if token.is_empty() {
            return Err(RpcError::invalid("the HTTP bridge needs a bearer token"));
        }
        if token.contains(['\r', '\n']) {
            return Err(RpcError::invalid("bearer token must not contain newlines"));
        }
        Ok(HttpConfig {
            token,
            max_sessions: MAX_HTTP_SESSIONS,
            max_pending_requests: MAX_PENDING_REQUESTS,
            session_idle: DEFAULT_SESSION_IDLE,
            max_body_bytes: MAX_MESSAGE_BYTES,
        })
    }
}

/// The bearer never appears in a `Debug` rendering of the config.
impl std::fmt::Debug for HttpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpConfig")
            .field("token", &"[redacted]")
            .field("max_sessions", &self.max_sessions)
            .field("max_pending_requests", &self.max_pending_requests)
            .field("session_idle", &self.session_idle)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

/// The router for one bridge. Binding is the caller's choice; `validate_listen`
/// says which binds are safe for a cleartext bearer.
pub fn router(bridge: Arc<Bridge>, config: HttpConfig) -> Router {
    Router::new().fallback(handle).with_state(Arc::new(App {
        bridge,
        registry: Registry::new(config.max_sessions, config.session_idle),
        config,
    }))
}

pub async fn bind(listen: &str) -> Result<TcpListener> {
    validate_listen(listen)?;
    TcpListener::bind(listen)
        .await
        .map_err(|error| RpcError::new(Code::Unavailable, format!("bind {listen}: {error}")))
}

/// Serves until the listener fails or `shutdown` resolves, which lets active
/// requests finish.
pub async fn serve(
    listener: TcpListener,
    bridge: Arc<Bridge>,
    config: HttpConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    axum::serve(listener, router(bridge, config))
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| RpcError::new(Code::Internal, format!("serve MCP HTTP: {error}")))
}

/// Plain HTTP carries the bearer in the clear, so it stays on loopback; a
/// private address does not prove the path to it is encrypted.
pub fn validate_listen(listen: &str) -> Result<()> {
    let refused = |reason: &str| {
        RpcError::invalid(format!(
            "--http {listen:?}: {reason}; serve MCP over plain HTTP only on loopback"
        ))
    };
    let Some((host, port)) = listen.rsplit_once(':') else {
        return Err(refused("a host and a port are needed"));
    };
    if port.is_empty()
        || port.len() > 5
        || !port.bytes().all(|byte| byte.is_ascii_digit())
        || port.parse::<u16>().is_err()
    {
        return Err(refused("the port is not a number"));
    }
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host == "localhost" {
        return Ok(());
    }
    let Ok(address) = host.parse::<IpAddr>() else {
        return Err(refused("the host must be an IP address or localhost"));
    };
    if address.is_loopback() {
        return Ok(());
    }
    Err(refused("the address is not loopback"))
}

struct App {
    bridge: Arc<Bridge>,
    registry: Registry,
    config: HttpConfig,
}

/// One MCP session on this transport.
struct Slot {
    id: String,
    session: Session,
    version: Mutex<String>,
    last_used: Mutex<Instant>,
    active: AtomicUsize,
    closed: watch::Sender<bool>,
    events: broadcast::Sender<Value>,
    subscriptions: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl Slot {
    fn new(id: String, max_pending_requests: usize) -> Arc<Slot> {
        let (closed, _) = watch::channel(false);
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Arc::new(Slot {
            id,
            session: Session::with_max_pending(max_pending_requests),
            version: Mutex::new(String::new()),
            last_used: Mutex::new(Instant::now()),
            active: AtomicUsize::new(0),
            closed,
            events,
            subscriptions: Mutex::new(HashMap::new()),
        })
    }

    fn close(&self) {
        self.session.cancel_all();
        self.closed.send_replace(true);
        let mut subscriptions = self
            .subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for (_, cancel) in subscriptions.drain() {
            cancel.send_replace(true);
        }
    }
}

/// Counts one request against its session while it is being answered.
struct Active {
    slot: Arc<Slot>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.slot.active.fetch_sub(1, Ordering::SeqCst);
        *self
            .slot
            .last_used
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }
}

/// Keeps a subscription cancellable while its request is open.
struct Subscription {
    slot: Arc<Slot>,
    key: String,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.slot
            .subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
    }
}

struct Registry {
    max_sessions: usize,
    session_idle: Duration,
    sessions: Mutex<HashMap<String, Arc<Slot>>>,
}

enum Acquired {
    None,
    Live(Active),
    Unknown,
}

impl Registry {
    fn new(max_sessions: usize, session_idle: Duration) -> Registry {
        Registry {
            max_sessions: max_sessions.max(1),
            session_idle,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Looks up `id`, counts the request against it, and expires idle sessions
    /// on the way in.
    fn acquire(&self, id: Option<&str>) -> Acquired {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, slot)| {
                slot.active.load(Ordering::SeqCst) == 0
                    && now.duration_since(
                        *slot
                            .last_used
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner),
                    ) >= self.session_idle
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(slot) = sessions.remove(&id) {
                slot.close();
            }
        }
        let Some(id) = id else {
            return Acquired::None;
        };
        match sessions.get(id) {
            Some(slot) => {
                slot.active.fetch_add(1, Ordering::SeqCst);
                *slot
                    .last_used
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = now;
                Acquired::Live(Active {
                    slot: Arc::clone(slot),
                })
            }
            None => Acquired::Unknown,
        }
    }

    /// Inserts a session, or refuses it when the bridge is full.
    fn open(&self, slot: &Arc<Slot>) -> bool {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        if sessions.len() >= self.max_sessions {
            return false;
        }
        sessions.insert(slot.id.clone(), Arc::clone(slot));
        true
    }
}

async fn handle(State(app): State<Arc<App>>, request: HttpRequest) -> Response {
    if !valid_origin(request.headers()) {
        return plain(StatusCode::FORBIDDEN, "invalid Origin");
    }
    if !bearer_ok(app.config.token.as_str(), request.headers()) {
        let mut response = plain(StatusCode::UNAUTHORIZED, "missing or invalid bearer token");
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }
    match *request.method() {
        Method::POST => post(app, request).await,
        Method::GET | Method::DELETE => {
            let id = match session_id(request.headers()) {
                Ok(id) => id,
                Err(reason) => return plain(StatusCode::BAD_REQUEST, reason),
            };
            if request.method() == Method::GET {
                get(app, id, request.headers())
            } else {
                delete(&app, id, request.headers())
            }
        }
        _ => {
            let mut response = plain(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
            response
                .headers_mut()
                .insert(header::ALLOW, HeaderValue::from_static("GET, POST, DELETE"));
            response
        }
    }
}

async fn post(app: Arc<App>, request: HttpRequest) -> Response {
    if !is_json(request.headers()) {
        return plain(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected application/json",
        );
    }
    let headers = request.headers().clone();
    let body = match read_body(request.into_body(), app.config.max_body_bytes).await {
        Ok(body) => body,
        Err(BodyFailure::TooLarge) => {
            return plain(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("request exceeds {} bytes", app.config.max_body_bytes),
            )
        }
        Err(BodyFailure::Read) => return plain(StatusCode::BAD_REQUEST, "read MCP request"),
    };
    let request = match jsonrpc::parse_request(&body) {
        Ok(request) => request,
        Err(failure) => return json_reply(failure.into_response(), None),
    };
    let body_version = request
        .params
        .get("_meta")
        .and_then(|meta| meta.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str);
    let header_version = headers
        .get("mcp-protocol-version")
        .and_then(|h| h.to_str().ok());
    let modern = is_modern(&request)
        || header_version.is_some_and(|version| {
            !matches!(
                version,
                "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05"
            )
        });
    if modern {
        if let Some(error) = validate_modern_headers(&headers, &request, body_version) {
            return error;
        }
        if request.is_notification() {
            return StatusCode::ACCEPTED.into_response();
        }
        let slot = Slot::new(String::new(), app.config.max_pending_requests);
        return answer(&app, &slot, request, None).await;
    }
    let id = match session_id(&headers) {
        Ok(id) => id,
        Err(reason) => return plain(StatusCode::BAD_REQUEST, reason),
    };
    match app.registry.acquire(id.as_deref()) {
        Acquired::Unknown => plain(StatusCode::NOT_FOUND, "session not found"),
        Acquired::Live(active) => {
            if !valid_legacy_version(&headers, &active.slot) {
                return plain(
                    StatusCode::BAD_REQUEST,
                    "invalid MCP-Protocol-Version for session",
                );
            }
            if request.is_notification() {
                notify(&active.slot.session, &request);
                cancel_subscription(&active.slot, &request);
                return StatusCode::ACCEPTED.into_response();
            }
            answer(&app, &Arc::clone(&active.slot), request, Some(active)).await
        }
        Acquired::None => {
            // Without a session id only the handshake and the stateless
            // per-request shape may run, and neither is retained.
            let handshake = matches!(request.method.as_str(), "initialize" | "server/discover");
            if request.is_notification() || !(handshake || modern) {
                return plain(StatusCode::BAD_REQUEST, "session id required");
            }
            let id = match new_session_id() {
                Ok(id) => id,
                Err(error) => return plain(StatusCode::INTERNAL_SERVER_ERROR, &error.message),
            };
            let slot = Slot::new(id, app.config.max_pending_requests);
            answer(&app, &slot, request, None).await
        }
    }
}

/// Answers one request on `slot`. `active` is the session count held for it;
/// `None` means the slot is a one-shot stateless session.
async fn answer(
    app: &Arc<App>,
    slot: &Arc<Slot>,
    request: Request,
    active: Option<Active>,
) -> Response {
    let guard = match slot
        .session
        .begin(&request.id.clone().unwrap_or(Value::Null))
    {
        Ok(guard) => guard,
        Err(failure) => return json_reply(failure.into_response(), None),
    };
    if request.method == "subscriptions/listen" {
        return subscription(app, slot, request, guard, active).await;
    }
    let bridge = Arc::clone(&app.bridge);
    let session = slot.session.clone();
    let cancelled = Arc::clone(&guard.cancelled);
    let id = request.id.clone();
    let initialized = request.method == "initialize";
    let modern = is_modern(&request);
    let outcome =
        tokio::task::spawn_blocking(move || bridge.dispatch(&session, &request, &cancelled)).await;
    let reply = match outcome {
        Ok(reply) => reply,
        Err(_) => {
            Failure::new(id, jsonrpc::INTERNAL_ERROR, "tool call worker panicked").into_response()
        }
    };
    let mut retained = None;
    if initialized && reply.get("error").is_none() {
        if let Some(version) = reply["result"]["protocolVersion"].as_str() {
            *slot.version.lock().unwrap_or_else(PoisonError::into_inner) = version.to_owned();
        }
        if active.is_none() && !app.registry.open(slot) {
            return plain(
                StatusCode::SERVICE_UNAVAILABLE,
                "too many MCP sessions; close an existing session and retry",
            );
        }
        retained = Some(slot.id.as_str());
    }
    let not_found = modern && reply["error"]["code"] == jsonrpc::METHOD_NOT_FOUND;
    let mut response = json_reply(reply, retained);
    if not_found {
        *response.status_mut() = StatusCode::NOT_FOUND;
    }
    response
}

/// `subscriptions/listen` keeps its request open for as long as it is
/// subscribed: the acknowledgement and every later message go to the session
/// event stream, and the JSON-RPC result closes this request's own stream when
/// the subscription ends. Only a real session can carry those messages, so
/// `active` also says whether this slot is one.
async fn subscription(
    app: &Arc<App>,
    slot: &Arc<Slot>,
    request: Request,
    guard: Guard,
    active: Option<Active>,
) -> Response {
    let failure = |code: i64, message: &str| {
        json_reply(
            Failure::new(request.id.clone(), code, message).into_response(),
            None,
        )
    };
    if !is_modern(&request) {
        return failure(
            jsonrpc::METHOD_NOT_FOUND,
            "subscriptions/listen unavailable on this transport",
        );
    }
    let Some(notifications) = request
        .params
        .get("notifications")
        .and_then(Value::as_object)
    else {
        return failure(jsonrpc::INVALID_PARAMS, "missing notifications");
    };
    let mut agreed = Map::new();
    if notifications
        .get("toolsListChanged")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && app.bridge.tool_count() > 0
    {
        agreed.insert("toolsListChanged".to_owned(), json!(true));
    }
    let id = request.id.clone().unwrap_or(Value::Null);
    let key = request_key(&id);
    let (cancel, waiter) = watch::channel(false);
    slot.subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key.clone(), cancel);
    let subscription = Arc::new(Subscription {
        slot: Arc::clone(slot),
        key,
    });
    let meta = json!({"io.modelcontextprotocol/subscriptionId": id});
    let events = slot.events.subscribe();
    let ack = json!({
        "jsonrpc": "2.0",
        "method": "notifications/subscriptions/acknowledged",
        "params": {"_meta": meta, "notifications": agreed},
    });
    let result = jsonrpc::success(&id, json!({"_meta": meta}));
    let cancelled = Arc::clone(&guard.cancelled);
    let body = event_stream(
        slot,
        events,
        Some(waiter),
        Hold {
            _pending: Some(guard),
            _subscription: Some(subscription),
            _active: active,
        },
        Some(result),
        Some(ack),
        Some(cancelled),
    );
    sse(body)
}

fn get(app: Arc<App>, id: Option<String>, headers: &HeaderMap) -> Response {
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|value| value == MODERN_VERSION)
    {
        return plain(StatusCode::METHOD_NOT_ALLOWED, "modern MCP uses POST only");
    }
    let Some(id) = id else {
        return plain(StatusCode::BAD_REQUEST, "session id required");
    };
    match app.registry.acquire(Some(&id)) {
        Acquired::Live(active) => {
            if !valid_legacy_version(headers, &active.slot) {
                return plain(
                    StatusCode::BAD_REQUEST,
                    "invalid MCP-Protocol-Version for session",
                );
            }
            let slot = Arc::clone(&active.slot);
            let events = slot.events.subscribe();
            let body = event_stream(
                &slot,
                events,
                None,
                Hold {
                    _pending: None,
                    _subscription: None,
                    _active: Some(active),
                },
                None,
                None,
                None,
            );
            sse(body)
        }
        Acquired::Unknown => plain(StatusCode::NOT_FOUND, "session not found"),
        Acquired::None => plain(StatusCode::BAD_REQUEST, "session id required"),
    }
}

fn delete(app: &Arc<App>, id: Option<String>, headers: &HeaderMap) -> Response {
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|value| value == MODERN_VERSION)
    {
        return plain(StatusCode::METHOD_NOT_ALLOWED, "modern MCP uses POST only");
    }
    let Some(id) = id else {
        return plain(StatusCode::BAD_REQUEST, "session id required");
    };
    let mut sessions = app
        .registry
        .sessions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if sessions
        .get(&id)
        .is_some_and(|slot| !valid_legacy_version(headers, slot))
    {
        return plain(
            StatusCode::BAD_REQUEST,
            "invalid MCP-Protocol-Version for session",
        );
    }
    let removed = sessions.remove(&id);
    drop(sessions);
    match removed {
        Some(slot) => {
            slot.close();
            StatusCode::NO_CONTENT.into_response()
        }
        None => plain(StatusCode::NOT_FOUND, "session not found"),
    }
}

fn cancel_subscription(slot: &Arc<Slot>, request: &Request) {
    if request.method != "notifications/cancelled" {
        return;
    }
    let Some(id) = request.params.get("requestId") else {
        return;
    };
    let subscriptions = slot
        .subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(cancel) = subscriptions.get(&request_key(id)) {
        cancel.send_replace(true);
    }
}

/// Everything that has to stay alive while an event stream is open.
/// Owned only for the `Drop` of each part: the pending request slot, the
/// subscription's cancellation entry, and the session request count.
struct Hold {
    _pending: Option<Guard>,
    _subscription: Option<Arc<Subscription>>,
    _active: Option<Active>,
}

/// The SSE body of one stream: session messages, keepalive comments while idle,
/// and a final frame. The producer ends when the session closes, the request is
/// cancelled, or the client stops reading.
fn event_stream(
    slot: &Arc<Slot>,
    mut events: broadcast::Receiver<Value>,
    cancel: Option<watch::Receiver<bool>>,
    hold: Hold,
    final_event: Option<Value>,
    first_event: Option<Value>,
    cancelled: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> Body {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut closed = slot.closed.subscribe();
    let slot = Arc::clone(slot);
    tokio::spawn(async move {
        let _hold = hold;
        let _slot = slot;
        let mut keepalive = interval_at(tokio::time::Instant::now() + KEEPALIVE, KEEPALIVE);
        keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut cancel = cancel;
        // The first frame is what flushes the response head, so a client sees
        // the stream open before there is anything to say.
        if tx.send(Bytes::from_static(b": open\n\n")).is_err() {
            return;
        }
        if let Some(first) = first_event {
            if tx.send(sse_frame(&first)).is_err() {
                return;
            }
        }
        loop {
            tokio::select! {
                biased;
                _ = tx.closed() => break,
                _ = wait_for_end(&mut closed) => break,
                _ = async {
                    match &mut cancel {
                        Some(cancel) => wait_for_end(cancel).await,
                        None => std::future::pending().await,
                    }
                } => break,
                message = events.recv() => match message {
                    Ok(message) => {
                        if tx.send(sse_frame(&message)).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                },
                _ = keepalive.tick() => {
                    if tx.send(Bytes::from_static(b": ping\n\n")).is_err() {
                        break;
                    }
                },
            }
        }
        if let Some(message) = final_event {
            let _ = tx.send(sse_frame(&message));
        }
    });
    Body::from_stream(Events { rx, cancelled })
}

async fn wait_for_end(end: &mut watch::Receiver<bool>) {
    while !*end.borrow_and_update() {
        if end.changed().await.is_err() {
            return;
        }
    }
}

struct Events {
    rx: mpsc::UnboundedReceiver<Bytes>,
    cancelled: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl Drop for Events {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancelled {
            cancelled.store(true, Ordering::SeqCst);
        }
    }
}

impl Stream for Events {
    type Item = std::result::Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|frame| frame.map(Ok))
    }
}

fn sse_frame(message: &Value) -> Bytes {
    let text = serde_json::to_string(message).unwrap_or_else(|_| "{}".to_owned());
    Bytes::from(format!("event: message\ndata: {text}\n\n"))
}

fn sse(body: Body) -> Response {
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

fn json_reply(body: Value, session_id: Option<&str>) -> Response {
    let mut response = Response::new(Body::from(body.to_string()));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(id) = session_id {
        if let Ok(value) = HeaderValue::from_str(id) {
            headers.insert(SESSION_ID, value);
        }
    }
    response
}

fn plain(status: StatusCode, message: &str) -> Response {
    let mut response = Response::new(Body::from(format!("{message}\n")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn bearer_ok(token: &str, headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(got) = value.strip_prefix("Bearer ") else {
        return false;
    };
    got.as_bytes().ct_eq(token.as_bytes()).into()
}

fn valid_origin(headers: &HeaderMap) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let Some(origin) = origins.next() else {
        return true;
    };
    if origins.next().is_some() {
        return false;
    }
    let (Some(origin), Some(host)) = (
        origin.to_str().ok(),
        headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok()),
    ) else {
        return false;
    };
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri
            .authority()
            .is_some_and(|authority| authority.as_str().eq_ignore_ascii_case(host))
        && matches!(uri.path(), "" | "/")
        && uri.query().is_none()
}

fn header_mismatch(id: Option<Value>, reason: &str) -> Response {
    let mut response = json_reply(
        Failure::new(id, -32020, format!("Header mismatch: {reason}")).into_response(),
        None,
    );
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

fn validate_modern_headers(
    headers: &HeaderMap,
    request: &Request,
    body_version: Option<&str>,
) -> Option<Response> {
    let id = request.id.clone();
    let header = |name: &str| {
        let mut values = headers.get_all(name).iter();
        let value = values.next()?.to_str().ok()?;
        values.next().is_none().then_some(value)
    };
    let Some(version) = header("mcp-protocol-version") else {
        return Some(header_mismatch(id, "MCP-Protocol-Version is required"));
    };
    if Some(version) != body_version {
        return Some(header_mismatch(
            id,
            "MCP-Protocol-Version differs from body",
        ));
    }
    if version != MODERN_VERSION {
        let mut response = json_reply(
            Failure::new(
                id,
                jsonrpc::UNSUPPORTED_PROTOCOL_VERSION,
                "unsupported protocol version",
            )
            .with_data(json!({"requested":version,"supported":crate::PROTOCOL_VERSIONS}))
            .into_response(),
            None,
        );
        *response.status_mut() = StatusCode::BAD_REQUEST;
        return Some(response);
    }
    let meta = &request.params["_meta"];
    if !meta["io.modelcontextprotocol/clientCapabilities"].is_object()
        || meta
            .get("io.modelcontextprotocol/clientInfo")
            .is_some_and(|info| !info.is_object())
    {
        let mut response = json_reply(
            Failure::new(
                id,
                jsonrpc::INVALID_PARAMS,
                "invalid per-request client metadata",
            )
            .into_response(),
            None,
        );
        *response.status_mut() = StatusCode::BAD_REQUEST;
        return Some(response);
    }
    if header("mcp-method") != Some(request.method.as_str()) {
        return Some(header_mismatch(
            id,
            "Mcp-Method is missing or differs from body",
        ));
    }
    let body_name = match request.method.as_str() {
        "tools/call" | "prompts/get" => request.params.get("name").and_then(Value::as_str),
        "resources/read" => request.params.get("uri").and_then(Value::as_str),
        _ => None,
    };
    if matches!(
        request.method.as_str(),
        "tools/call" | "prompts/get" | "resources/read"
    ) {
        let got = header("mcp-name").and_then(decode_mcp_name);
        if got.as_deref() != body_name {
            return Some(header_mismatch(
                id,
                "Mcp-Name is missing, malformed, or differs from body",
            ));
        }
    } else if headers.contains_key("mcp-name") {
        return Some(header_mismatch(id, "Mcp-Name has no source field in body"));
    }
    None
}

fn decode_mcp_name(raw: &str) -> Option<String> {
    if let Some(body) = raw.strip_prefix("=?base64?") {
        let body = body.strip_suffix("?=")?;
        return String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(body)
                .ok()?,
        )
        .ok();
    }
    if raw.trim_matches([' ', '\t']) != raw
        || !raw.bytes().all(|byte| matches!(byte, 0x20..=0x7e | b'\t'))
    {
        return None;
    }
    Some(raw.to_owned())
}

fn valid_legacy_version(headers: &HeaderMap, slot: &Slot) -> bool {
    let version = slot.version.lock().unwrap_or_else(PoisonError::into_inner);
    let header = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok());
    header.is_none_or(|header| header == version.as_str())
}

fn session_id(headers: &HeaderMap) -> std::result::Result<Option<String>, &'static str> {
    let Some(value) = headers.get(SESSION_ID) else {
        return Ok(None);
    };
    let Ok(text) = value.to_str() else {
        return Err("invalid Mcp-Session-Id");
    };
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > MAX_SESSION_ID_CHARS {
        return Err("Mcp-Session-Id is too long");
    }
    Ok(Some(text.to_owned()))
}

fn is_json(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/json")
}

enum BodyFailure {
    TooLarge,
    Read,
}

async fn read_body(body: Body, limit: usize) -> std::result::Result<Bytes, BodyFailure> {
    let mut frames = body.into_data_stream();
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let frame = std::future::poll_fn(|cx| Pin::new(&mut frames).poll_next(cx)).await;
        let Some(frame) = frame else {
            break;
        };
        let Ok(frame) = frame else {
            return Err(BodyFailure::Read);
        };
        if bytes.len() + frame.len() > limit {
            return Err(BodyFailure::TooLarge);
        }
        bytes.extend_from_slice(&frame);
    }
    Ok(Bytes::from(bytes))
}

fn new_session_id() -> Result<String> {
    let mut bytes = [0_u8; SESSION_ID_BYTES];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| RpcError::new(Code::Internal, format!("create session id: {error}")))?;
    Ok(bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}
