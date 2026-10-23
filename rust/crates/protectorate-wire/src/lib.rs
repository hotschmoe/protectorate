//! Descriptor-driven protobuf JSON and Connect unary HTTP endpoints.
//! The embedded descriptor is generated from the repository's proto sources.
mod streaming;
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method as HttpMethod, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
pub use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use prost_reflect::{DeserializeOptions, MethodDescriptor};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
pub use streaming::{
    decode_stream_request, encode_envelope, StreamSender, StreamTrailers,
    DEFAULT_MAX_STREAM_MESSAGE_BYTES, DEFAULT_STREAM_CAPACITY, FLAG_COMPRESSED, FLAG_END_STREAM,
};
pub const DESCRIPTOR_SET: &[u8] = include_bytes!("../assets/protectorate.binpb");
/// The same API image with proto comments, for tool descriptions and schemas.
pub const SOURCE_DESCRIPTOR_SET: &[u8] = include_bytes!("../assets/protectorate-source.binpb");
pub const DEFAULT_MAX_REQUEST_BYTES: usize = 4 << 20;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Canceled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}
impl Code {
    pub fn http_status(self) -> StatusCode {
        match self {
            Self::Canceled => StatusCode::from_u16(499).unwrap(),
            Self::Unknown | Self::Internal | Self::DataLoss => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InvalidArgument | Self::FailedPrecondition | Self::OutOfRange => {
                StatusCode::BAD_REQUEST
            }
            Self::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::AlreadyExists | Self::Aborted => StatusCode::CONFLICT,
            Self::PermissionDenied => StatusCode::FORBIDDEN,
            Self::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
            Self::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: Code,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<serde_json::Value>,
    #[serde(skip)]
    pub headers: Box<HeaderMap>,
    #[serde(skip)]
    pub status: Option<StatusCode>,
}
impl RpcError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
            headers: Box::default(),
            status: None,
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(Code::InvalidArgument, message)
    }
    fn status(mut self, status: StatusCode) -> Self {
        self.status = Some(status);
        self
    }
}
impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for RpcError {}
pub type Result<T> = std::result::Result<T, RpcError>;
impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let status = self.status.unwrap_or_else(|| self.code.http_status());
        let bytes = serde_json::to_vec(&self).unwrap_or_else(|_| {
            b"{\"code\":\"internal\",\"message\":\"error encoding response\"}".to_vec()
        });
        let mut r = Response::new(Body::from(bytes));
        *r.status_mut() = status;
        *r.headers_mut() = *self.headers;
        r.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        r.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        r.headers_mut()
            .insert("connect-protocol-version", HeaderValue::from_static("1"));
        r
    }
}
/// Rejects unknown fields at every message level, trailing JSON and empty
/// payloads, while accepting protobuf field names and JSON lowerCamel names.
pub fn decode_json(descriptor: MessageDescriptor, data: &[u8]) -> Result<DynamicMessage> {
    if data.is_empty() {
        return Err(RpcError::invalid(
            "zero-length payload is not valid protobuf JSON",
        ));
    }
    let text =
        std::str::from_utf8(data).map_err(|_| RpcError::invalid("protobuf JSON must be UTF-8"))?;
    validate_duplicates(&descriptor, text).map_err(RpcError::invalid)?;
    let mut d = serde_json::Deserializer::from_slice(data);
    let message = DynamicMessage::deserialize_with_options(
        descriptor,
        &mut d,
        &DeserializeOptions::new().deny_unknown_fields(true),
    )
    .map_err(|e| RpcError::invalid(format!("invalid protobuf JSON: {e}")))?;
    d.end()
        .map_err(|e| RpcError::invalid(format!("trailing protobuf JSON: {e}")))?;
    Ok(message)
}
// prost-reflect accepts duplicate fields and multiple oneof members. Validate
// those before decoding, retaining raw nested JSON so aliases are not lost.
struct ObjectFields(Vec<(String, Box<serde_json::value::RawValue>)>);
impl<'de> Deserialize<'de> for ObjectFields {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ObjectFields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a protobuf JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut fields = Vec::new();
                while let Some(pair) = map.next_entry()? {
                    fields.push(pair);
                }
                Ok(ObjectFields(fields))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
fn validate_duplicates(
    descriptor: &MessageDescriptor,
    text: &str,
) -> std::result::Result<(), String> {
    if descriptor.full_name().starts_with("google.protobuf.") || !text.trim_start().starts_with('{')
    {
        return Ok(());
    }
    let ObjectFields(fields) =
        serde_json::from_str(text).map_err(|e| format!("invalid protobuf JSON: {e}"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut oneofs = std::collections::BTreeSet::new();
    for (key, value) in fields {
        let Some(field) = descriptor
            .get_field_by_name(&key)
            .or_else(|| descriptor.get_field_by_json_name(&key))
        else {
            continue;
        };
        if !seen.insert(field.number()) {
            return Err(format!("duplicate protobuf field {key}"));
        }
        if value.get().trim() == "null" {
            continue;
        }
        if let Some(oneof) = field.containing_oneof() {
            if !oneofs.insert(oneof.full_name().to_owned()) {
                return Err(format!("multiple members of oneof {}", oneof.name()));
            }
        }
        if let prost_reflect::Kind::Message(nested) = field.kind() {
            if field.is_map() {
                if let prost_reflect::Kind::Message(nested) = nested.map_entry_value_field().kind()
                {
                    if let Ok(ObjectFields(entries)) = serde_json::from_str(value.get()) {
                        for (_, v) in entries {
                            validate_duplicates(&nested, v.get())?;
                        }
                    }
                }
            } else if field.is_list() {
                if let Ok(entries) =
                    serde_json::from_str::<Vec<Box<serde_json::value::RawValue>>>(value.get())
                {
                    for v in entries {
                        validate_duplicates(&nested, v.get())?;
                    }
                }
            } else {
                validate_duplicates(&nested, value.get())?;
            }
        }
    }
    Ok(())
}
/// Emits protobuf JSON: int64/uint64 as strings, symbolic enums, RFC3339
/// timestamps, duration strings, bytes as base64, and absent defaults omitted.
pub fn encode_json(message: &DynamicMessage) -> Result<Vec<u8>> {
    serde_json::to_vec(message)
        .map_err(|_| RpcError::new(Code::Internal, "failed to encode protobuf JSON"))
}
#[derive(Clone, Debug)]
pub struct Method {
    pub procedure: String,
    pub service: String,
    pub tool: String,
    pub read_only: bool,
    pub owner_only: bool,
    pub device_allowed: bool,
    pub node_local: bool,
    pub destructive: bool,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub descriptor: MethodDescriptor,
}
impl Method {
    pub fn input(&self) -> MessageDescriptor {
        self.descriptor.input()
    }
    pub fn output(&self) -> MessageDescriptor {
        self.descriptor.output()
    }
    pub fn is_unary(&self) -> bool {
        !self.client_streaming && !self.server_streaming
    }
    /// Leading proto comment, collapsed to one line when source info exists.
    pub fn description(&self) -> String {
        self.descriptor
            .parent_file()
            .file_descriptor_proto()
            .source_code_info
            .as_ref()
            .and_then(|info| {
                info.location
                    .iter()
                    .find(|location| location.path.as_slice() == self.descriptor.path())
            })
            .and_then(|location| location.leading_comments.as_deref())
            .map(|comment| comment.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    }
}
#[derive(Clone, Debug)]
pub struct Catalog {
    pool: DescriptorPool,
    methods: BTreeMap<String, Method>,
}
fn snake(s: &str) -> String {
    let chars: Vec<_> = s.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|c| c.is_lowercase())
                        && chars[i - 1].is_uppercase())
            {
                out.push('_')
            }
            out.extend(c.to_lowercase())
        } else {
            out.push(*c)
        }
    }
    out
}
impl Catalog {
    pub fn embedded() -> Result<Self> {
        Self::from_descriptor_set(DESCRIPTOR_SET)
    }
    pub fn with_source_info() -> Result<Self> {
        Self::from_descriptor_set(SOURCE_DESCRIPTOR_SET)
    }
    pub fn from_descriptor_set(bytes: &[u8]) -> Result<Self> {
        let pool = DescriptorPool::decode(bytes)
            .map_err(|e| RpcError::new(Code::Internal, format!("invalid API descriptor: {e}")))?;
        let extension = |options: &DynamicMessage, name: &str| {
            pool.get_extension_by_name(&format!("protectorate.v1.{name}"))
                .is_some_and(|e| options.get_extension(&e).as_bool() == Some(true))
        };
        let mut methods = BTreeMap::new();
        let mut tools = BTreeMap::new();
        for service in pool
            .services()
            .filter(|s| s.full_name().starts_with("protectorate.v1."))
        {
            let service_options = service.options();
            for descriptor in service.methods() {
                let options = descriptor.options();
                let procedure = format!("/{}/{}", service.full_name(), descriptor.name());
                let tool = format!(
                    "{}_{}",
                    snake(service.name().trim_end_matches("Service")),
                    snake(descriptor.name())
                );
                if tools.insert(tool.clone(), procedure.clone()).is_some() {
                    return Err(RpcError::new(
                        Code::Internal,
                        format!("duplicate tool name {tool}"),
                    ));
                }
                let m = Method {
                    procedure: procedure.clone(),
                    service: service.name().into(),
                    tool,
                    read_only: options
                        .get_field_by_name("idempotency_level")
                        .is_some_and(|v| v.as_enum_number() == Some(1)),
                    owner_only: extension(&service_options, "owner_only")
                        || extension(&options, "owner_only_rpc"),
                    device_allowed: extension(&options, "device_allowed"),
                    node_local: extension(&service_options, "node_local"),
                    destructive: extension(&options, "destructive"),
                    client_streaming: descriptor.is_client_streaming(),
                    server_streaming: descriptor.is_server_streaming(),
                    descriptor,
                };
                methods.insert(procedure, m);
            }
        }
        Ok(Self { pool, methods })
    }
    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }
    pub fn method(&self, procedure: &str) -> Option<&Method> {
        self.methods.get(procedure)
    }
    pub fn methods(&self) -> impl Iterator<Item = &Method> {
        self.methods.values()
    }
    pub fn message(&self, name: &str) -> Result<MessageDescriptor> {
        self.pool
            .get_message_by_name(name)
            .ok_or_else(|| RpcError::new(Code::NotFound, format!("unknown message {name}")))
    }
}
#[derive(Clone, Debug, Default)]
pub struct AuthContext {
    pub actor: String,
    pub token_id: String,
    pub task: String,
    pub scopes: Vec<String>,
    pub owner: bool,
    pub device: bool,
    /// Direct transport peer for public device pairing audit entries.
    pub requesting_ip: Option<std::net::IpAddr>,
}
/// The transport peer, inserted as a request extension by the listener and
/// never taken from a header. `tls` is false only on the hub's loopback
/// listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    pub addr: SocketAddr,
    pub tls: bool,
}
/// An admission failure, with whatever identity was established before it.
#[derive(Clone, Debug)]
pub struct Denied {
    pub error: RpcError,
    pub auth: AuthContext,
}
impl From<RpcError> for Box<Denied> {
    fn from(error: RpcError) -> Self {
        Box::new(Denied {
            error,
            auth: AuthContext::default(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct RpcRequest {
    pub method: Method,
    pub headers: HeaderMap,
    pub message: DynamicMessage,
    pub auth: AuthContext,
    pub peer: Option<Peer>,
}
#[derive(Clone, Debug)]
pub struct RpcResponse {
    pub message: DynamicMessage,
    pub headers: HeaderMap,
}
impl From<DynamicMessage> for RpcResponse {
    fn from(message: DynamicMessage) -> Self {
        Self {
            message,
            headers: HeaderMap::new(),
        }
    }
}
type HandlerFuture = Pin<Box<dyn Future<Output = Result<RpcResponse>> + Send>>;
type Handler = Arc<dyn Fn(RpcRequest) -> HandlerFuture + Send + Sync>;
type StreamingFuture = Pin<Box<dyn Future<Output = Result<StreamTrailers>> + Send>>;
type StreamingHandler = Arc<dyn Fn(RpcRequest, StreamSender) -> StreamingFuture + Send + Sync>;
/// The hook decides public, owner, device, scope and node-local policies.
/// It runs before JSON decoding and handlers, and must not block on network IO.
pub type Authorizer = Arc<dyn Fn(&Method, &HeaderMap) -> Result<AuthContext> + Send + Sync>;
pub type PeerAuthorizer = Arc<
    dyn Fn(&Method, &HeaderMap, Option<Peer>) -> std::result::Result<AuthContext, Box<Denied>>
        + Send
        + Sync,
>;
enum Admission {
    Headers(Authorizer),
    Peer(PeerAuthorizer),
}
/// Called after a registered handler finishes. The observer sees the validated
/// request and final handler error, but never raw headers or bearer tokens.
pub type CompletionObserver =
    Arc<dyn Fn(&Method, &AuthContext, &DynamicMessage, Option<&RpcError>) + Send + Sync>;
/// Runs on every decoded request just before its handler. An error is the
/// call's result, with no completion or rejection observer: the gate records
/// its own decisions.
pub type Gate = Arc<dyn Fn(&RpcRequest) -> Result<()> + Send + Sync>;
/// Called when a known procedure fails validation before its handler starts.
/// No request body or credential headers are exposed to this hook.
pub type RejectionObserver = Arc<dyn Fn(&Method, Option<&AuthContext>, &RpcError) + Send + Sync>;
/// Called once for an admission denial with the identity the authorizer
/// established. The optional request is decoded only when its body fits the
/// small audit bound and arrives promptly.
pub type AdmissionDenialObserver =
    Arc<dyn Fn(&Method, &AuthContext, Option<&DynamicMessage>, &RpcError) + Send + Sync>;
struct AdmissionDenialGuard {
    observer: AdmissionDenialObserver,
    method: Method,
    denied: Box<Denied>,
    finished: bool,
}
impl AdmissionDenialGuard {
    fn finish(&mut self, message: Option<&DynamicMessage>) {
        self.finished = true;
        (self.observer)(&self.method, &self.denied.auth, message, &self.denied.error);
    }
}
impl Drop for AdmissionDenialGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(None);
        }
    }
}
struct BodyReadGuard<'a> {
    observer: Option<&'a RejectionObserver>,
    method: &'a Method,
    auth: &'a AuthContext,
    deadline: Option<tokio::time::Instant>,
    active: bool,
}
impl BodyReadGuard<'_> {
    fn disarm(&mut self) {
        self.active = false;
    }
}
impl Drop for BodyReadGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(observer) = self.observer {
            let error = if self
                .deadline
                .is_some_and(|at| tokio::time::Instant::now() >= at)
            {
                RpcError::new(
                    Code::DeadlineExceeded,
                    "request deadline expired before handler start",
                )
            } else {
                RpcError::new(Code::Canceled, "request was canceled before handler start")
            };
            observer(self.method, Some(self.auth), &error);
        }
    }
}
struct CompletionGuard {
    observer: Option<CompletionObserver>,
    method: Method,
    auth: AuthContext,
    message: DynamicMessage,
    finished: bool,
    deadline_expired: Option<Arc<AtomicBool>>,
}
impl CompletionGuard {
    fn complete(&mut self, error: Option<&RpcError>) {
        self.finished = true;
        if let Some(observer) = &self.observer {
            observer(&self.method, &self.auth, &self.message, error);
        }
    }
}
impl Drop for CompletionGuard {
    fn drop(&mut self) {
        if !self.finished {
            let error = if self
                .deadline_expired
                .as_ref()
                .is_some_and(|expired| expired.load(Ordering::SeqCst))
            {
                RpcError::new(Code::DeadlineExceeded, "stream deadline exceeded")
            } else {
                RpcError::new(
                    Code::Canceled,
                    "handler future was dropped before completion",
                )
            };
            self.complete(Some(&error));
        }
    }
}
pub struct UnaryServer {
    catalog: Catalog,
    handlers: BTreeMap<String, Handler>,
    streaming_handlers: BTreeMap<String, StreamingHandler>,
    max_stream_message_bytes: usize,
    authorize: Admission,
    completion_observer: Option<CompletionObserver>,
    rejection_observer: Option<RejectionObserver>,
    admission_denial_observer: Option<AdmissionDenialObserver>,
    gate: Option<Gate>,
    max_request_bytes: usize,
}
impl UnaryServer {
    /// The authorization hook is mandatory; accidental anonymous serving is avoided.
    pub fn new(catalog: Catalog, authorize: Authorizer) -> Self {
        Self::with_admission(catalog, Admission::Headers(authorize))
    }
    pub fn new_with_peer(catalog: Catalog, authorize: PeerAuthorizer) -> Self {
        Self::with_admission(catalog, Admission::Peer(authorize))
    }
    fn with_admission(catalog: Catalog, authorize: Admission) -> Self {
        Self {
            catalog,
            handlers: BTreeMap::new(),
            streaming_handlers: BTreeMap::new(),
            max_stream_message_bytes: DEFAULT_MAX_STREAM_MESSAGE_BYTES,
            authorize,
            completion_observer: None,
            rejection_observer: None,
            admission_denial_observer: None,
            gate: None,
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
        }
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn with_max_request_bytes(mut self, limit: usize) -> Self {
        self.max_request_bytes = limit;
        self
    }
    pub fn with_completion_observer(mut self, observer: CompletionObserver) -> Self {
        self.completion_observer = Some(observer);
        self
    }
    pub fn with_rejection_observer(mut self, observer: RejectionObserver) -> Self {
        self.rejection_observer = Some(observer);
        self
    }
    pub fn with_admission_denial_observer(mut self, observer: AdmissionDenialObserver) -> Self {
        self.admission_denial_observer = Some(observer);
        self
    }
    pub fn with_gate(mut self, gate: Gate) -> Self {
        self.gate = Some(gate);
        self
    }
    fn reject(&self, method: &Method, auth: Option<&AuthContext>, error: RpcError) -> RpcError {
        if let Some(observer) = &self.rejection_observer {
            observer(method, auth, &error);
        }
        error
    }
    pub fn register<F, Fut>(&mut self, procedure: &str, handler: F) -> Result<()>
    where
        F: Fn(RpcRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<RpcResponse>> + Send + 'static,
    {
        let method = self.catalog.method(procedure).ok_or_else(|| {
            RpcError::new(Code::NotFound, format!("unknown procedure {procedure}"))
        })?;
        if !method.is_unary() {
            return Err(RpcError::new(
                Code::Unimplemented,
                "streaming RPCs need a streaming transport",
            ));
        }
        if self.handlers.contains_key(procedure) {
            return Err(RpcError::new(
                Code::AlreadyExists,
                "handler is already registered",
            ));
        }
        self.handlers.insert(
            procedure.into(),
            Arc::new({
                let observer = self.completion_observer.clone();
                move |request| {
                    let mut guard = CompletionGuard {
                        observer: observer.clone(),
                        method: request.method.clone(),
                        auth: request.auth.clone(),
                        message: request.message.clone(),
                        finished: false,
                        deadline_expired: None,
                    };
                    let future = handler(request);
                    Box::pin(async move {
                        let result = future.await.and_then(|response| {
                            if response.message.descriptor() != guard.method.output() {
                                return Err(RpcError::new(
                                    Code::Internal,
                                    "handler returned the wrong protobuf message type",
                                ));
                            }
                            Ok(response)
                        });
                        guard.complete(result.as_ref().err());
                        result
                    })
                }
            }),
        );
        Ok(())
    }
    /// Registers a producer for a schema-declared server stream. Return trailer
    /// metadata on completion or a Connect error; send messages through sender.
    pub fn register_streaming<F, Fut>(&mut self, procedure: &str, handler: F) -> Result<()>
    where
        F: Fn(RpcRequest, StreamSender) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<StreamTrailers>> + Send + 'static,
    {
        let method = self
            .catalog
            .method(procedure)
            .ok_or_else(|| RpcError::new(Code::NotFound, "unknown streaming procedure"))?;
        if !method.server_streaming || method.client_streaming {
            return Err(RpcError::new(
                Code::Unimplemented,
                "method is not a server-only stream",
            ));
        }
        if self.streaming_handlers.contains_key(procedure) {
            return Err(RpcError::new(
                Code::AlreadyExists,
                "stream handler already registered",
            ));
        }
        self.streaming_handlers.insert(
            procedure.into(),
            Arc::new({
                let observer = self.completion_observer.clone();
                move |request, sender| {
                    let mut guard = CompletionGuard {
                        observer: observer.clone(),
                        method: request.method.clone(),
                        auth: request.auth.clone(),
                        message: request.message.clone(),
                        finished: false,
                        deadline_expired: Some(sender.deadline_expired()),
                    };
                    let future = handler(request, sender);
                    Box::pin(async move {
                        let result = future.await;
                        guard.complete(result.as_ref().err());
                        result
                    })
                }
            }),
        );
        Ok(())
    }
    pub fn with_max_stream_message_bytes(mut self, limit: usize) -> Self {
        self.max_stream_message_bytes = limit;
        self
    }
    pub fn router(self) -> Router {
        Router::new().fallback(dispatch).with_state(Arc::new(self))
    }
}
fn timeout(headers: &HeaderMap) -> Result<Option<Duration>> {
    let Some(v) = headers.get("connect-timeout-ms") else {
        return Ok(None);
    };
    let s = v
        .to_str()
        .map_err(|_| RpcError::invalid("invalid connect-timeout-ms"))?;
    if s.is_empty() || s.len() > 10 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RpcError::invalid("invalid connect-timeout-ms"));
    }
    Ok(Some(Duration::from_millis(s.parse().map_err(|_| {
        RpcError::invalid("invalid connect-timeout-ms")
    })?)))
}
async fn dispatch(State(server): State<Arc<UnaryServer>>, request: Request) -> Response {
    let streaming_json =
        content_type(request.headers()).eq_ignore_ascii_case("application/connect+json");
    let deadline = match timeout(request.headers()) {
        Ok(d) => d,
        Err(e) => {
            let e = if let Some(method) = server.catalog.method(request.uri().path()) {
                server.reject(method, None, e)
            } else {
                e
            };
            return if streaming_json {
                streaming::error_response(e)
            } else {
                e.into_response()
            };
        }
    };
    let deadline_at = deadline.map(|d| tokio::time::Instant::now() + d);
    let operation = dispatch_inner(server, request, deadline_at);
    let result = match deadline {
        Some(deadline) => match tokio::time::timeout(deadline, operation).await {
            Ok(result) => result,
            Err(_) => Err(RpcError::new(
                Code::DeadlineExceeded,
                "request deadline exceeded",
            )),
        },
        None => operation.await,
    };
    match result {
        Ok(response) => response,
        Err(e)
            if streaming_json
                && e.status != Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                && e.status != Some(StatusCode::METHOD_NOT_ALLOWED) =>
        {
            streaming::error_response(e)
        }
        Err(e) => e.into_response(),
    }
}
fn content_type(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
}
async fn dispatch_inner(
    server: Arc<UnaryServer>,
    request: Request,
    deadline: Option<tokio::time::Instant>,
) -> Result<Response> {
    if request.method() != HttpMethod::POST {
        let mut e = RpcError::new(Code::Unimplemented, "Connect RPCs require POST")
            .status(StatusCode::METHOD_NOT_ALLOWED);
        e.headers
            .insert(header::ALLOW, HeaderValue::from_static("POST"));
        return Err(
            if let Some(method) = server.catalog.method(request.uri().path()) {
                server.reject(method, None, e)
            } else {
                e
            },
        );
    }
    let path = request.uri().path();
    let method = server
        .catalog
        .method(path)
        .ok_or_else(|| {
            RpcError::new(Code::Unimplemented, "unknown RPC procedure")
                .status(StatusCode::NOT_FOUND)
        })?
        .clone();
    if method.client_streaming {
        return Err(server.reject(
            &method,
            None,
            RpcError::new(
                Code::Unimplemented,
                "client and bidirectional streaming are not implemented",
            ),
        ));
    }
    if let Some(version) = request.headers().get("connect-protocol-version") {
        if version != "1" {
            return Err(server.reject(
                &method,
                None,
                RpcError::invalid("unsupported Connect protocol version"),
            ));
        }
    }
    let expected = if method.server_streaming {
        "application/connect+json"
    } else {
        "application/json"
    };
    if !content_type(request.headers()).eq_ignore_ascii_case(expected) {
        return Err(server.reject(
            &method,
            None,
            RpcError::invalid(format!("expected {expected}"))
                .status(StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ));
    }
    if method.server_streaming
        && request
            .headers()
            .get("connect-content-encoding")
            .is_some_and(|v| v != "identity")
    {
        return Err(server.reject(
            &method,
            None,
            RpcError::new(
                Code::Unimplemented,
                "stream compression is unsupported; supported encoding: identity",
            ),
        ));
    }
    if request
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|v| v != "identity")
    {
        return Err(server.reject(
            &method,
            None,
            RpcError::new(
                Code::Unimplemented,
                "compressed request bodies are not supported",
            )
            .status(StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ));
    }
    let peer = request.extensions().get::<Peer>().copied();
    let admission = match &server.authorize {
        Admission::Headers(authorize) => {
            authorize(&method, request.headers()).map_err(Box::<Denied>::from)
        }
        Admission::Peer(authorize) => authorize(&method, request.headers(), peer),
    };
    let auth = match admission {
        Ok(auth) => auth,
        Err(denied) => {
            let error = denied.error.clone();
            if let Some(observer) = &server.admission_denial_observer {
                let mut guard = AdmissionDenialGuard {
                    observer: Arc::clone(observer),
                    method: method.clone(),
                    denied,
                    finished: false,
                };
                const AUDIT_BODY_LIMIT: usize = 16 << 10;
                let message = if request
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|length| length.to_str().ok())
                    .and_then(|length| length.parse::<usize>().ok())
                    .is_some_and(|length| length > AUDIT_BODY_LIMIT)
                {
                    None
                } else {
                    let body = request.into_body();
                    match tokio::time::timeout(
                        Duration::from_millis(50),
                        to_bytes(
                            body,
                            AUDIT_BODY_LIMIT + usize::from(method.server_streaming) * 5,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(bytes)) if method.server_streaming => {
                            decode_stream_request(method.input(), &bytes, AUDIT_BODY_LIMIT).ok()
                        }
                        Ok(Ok(bytes)) => decode_json(method.input(), &bytes).ok(),
                        _ => None,
                    }
                };
                guard.finish(message.as_ref());
            }
            return Err(error);
        }
    };
    let (headers, body) = {
        let (parts, body) = request.into_parts();
        (parts.headers, body)
    };
    let body_limit = server
        .max_request_bytes
        .saturating_add(if method.server_streaming { 5 } else { 0 });
    let mut body_guard = BodyReadGuard {
        observer: server.rejection_observer.as_ref(),
        method: &method,
        auth: &auth,
        deadline,
        active: true,
    };
    let bytes = to_bytes(body, body_limit).await.map_err(|_| {
        body_guard.disarm();
        server.reject(
            &method,
            Some(&auth),
            RpcError::new(
                Code::ResourceExhausted,
                "request body exceeds limit or could not be read",
            ),
        )
    })?;
    let message = if method.server_streaming {
        decode_stream_request(method.input(), &bytes, server.max_request_bytes)
    } else {
        decode_json(method.input(), &bytes)
    }
    .map_err(|error| {
        body_guard.disarm();
        server.reject(&method, Some(&auth), error)
    })?;
    body_guard.disarm();
    drop(body_guard);
    if method.server_streaming {
        let handler = server
            .streaming_handlers
            .get(&method.procedure)
            .ok_or_else(|| {
                server.reject(
                    &method,
                    Some(&auth),
                    RpcError::new(Code::Unimplemented, "streaming method is not implemented"),
                )
            })?
            .clone();
        let output = method.output();
        let req = RpcRequest {
            method,
            headers,
            message,
            auth,
            peer,
        };
        if let Some(gate) = &server.gate {
            gate(&req)?;
        }
        return Ok(streaming::stream_response(
            output,
            DEFAULT_STREAM_CAPACITY,
            server.max_stream_message_bytes,
            deadline,
            move |sender| handler(req, sender),
        ));
    }
    let handler = server
        .handlers
        .get(&method.procedure)
        .ok_or_else(|| {
            server.reject(
                &method,
                Some(&auth),
                RpcError::new(Code::Unimplemented, "RPC method is not implemented"),
            )
        })?
        .clone();
    let request = RpcRequest {
        method: method.clone(),
        headers,
        message,
        auth,
        peer,
    };
    if let Some(gate) = &server.gate {
        gate(&request)?;
    }
    let response = handler(request).await?;
    if response.message.descriptor() != method.output() {
        return Err(RpcError::new(
            Code::Internal,
            "handler returned the wrong protobuf message type",
        ));
    }
    let bytes = encode_json(&response.message)?;
    let mut out = Response::new(Body::from(bytes));
    *out.headers_mut() = response.headers;
    out.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    out.headers_mut()
        .insert("connect-protocol-version", HeaderValue::from_static("1"));
    out.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(out)
}
