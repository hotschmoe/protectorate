//! An MCP server that serves the hub API as tools, over newline-delimited
//! JSON-RPC on stdin/stdout or over a streamable HTTP endpoint.
//!
//! Every unary RPC in the embedded API descriptor becomes a tool named
//! `<service>_<method>`. Arguments are validated against the request message
//! and sent to the hub through `protectorate-connect-client`; the reply comes
//! back as proto-JSON text. With no `--allow` patterns only read-only tools
//! are offered, and the hub token decides what the bridge may really do.
//!
//! ```no_run
//! use protectorate_mcpbridge::{Bridge, Config};
//! use std::io;
//! use std::sync::{Arc, Mutex};
//!
//! let bridge = Arc::new(Bridge::new(Config::without_hub("0.1.0")).unwrap());
//! let out = Arc::new(Mutex::new(io::stdout()));
//! bridge.run(io::stdin().lock(), &out).unwrap();
//! ```
//!
//! The HTTP transport is one bearer-authenticated endpoint: POST carries
//! JSON-RPC, GET streams server-to-client messages, DELETE ends the session.
//!
//! ```no_run
//! use protectorate_mcpbridge::{bind, serve, Bridge, Config, HttpConfig};
//! use std::sync::Arc;
//!
//! # async fn run() -> protectorate_mcpbridge::Result<()> {
//! let bridge = Arc::new(Bridge::new(Config::without_hub("0.1.0")).unwrap());
//! let listener = bind("127.0.0.1:7471").await?;
//! serve(listener, bridge, HttpConfig::new("token")?, async {
//!     let _ = tokio::signal::ctrl_c().await;
//! })
//! .await
//! # }
//! ```
mod catalog;
mod http;
mod jsonrpc;
mod schema;
mod session;
mod tls;

pub use catalog::{tool_defs, Filter, ToolDef};
pub use http::{
    bind, router, serve, validate_listen, HttpConfig, DEFAULT_SESSION_IDLE, KEEPALIVE,
    MAX_HTTP_SESSIONS, SESSION_ID_BYTES,
};
pub use jsonrpc::{Failure, Request};
pub use protectorate_connect_client::{Client, Credential};
pub use protectorate_wire::{Code, Result, RpcError};
pub use schema::input_schema;
pub use session::{Guard, Session, MAX_PENDING_REQUESTS};
pub use tls::{bind_tls, serve_tls, tls_config, TlsListener};

use protectorate_wire::{Catalog, Method};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The hub URL used when the caller does not set one.
pub const DEFAULT_HUB: &str = "http://127.0.0.1:7470";
/// The hub request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
// Bounds calls that have not yet left DNS or TCP connection setup when canceled.
const MAX_HUB_CALLS: usize = MAX_PENDING_REQUESTS * 2;
/// The largest JSON-RPC frame accepted on a single line.
pub const MAX_MESSAGE_BYTES: usize = 16 << 20;
/// The server name reported in `serverInfo`.
pub const SERVER_NAME: &str = "protectorate";
/// Protocol versions served, newest first.
pub const PROTOCOL_VERSIONS: [&str; 5] = [
    "2026-07-28",
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];

pub(crate) const MODERN: &str = "2026-07-28";
const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// Where the bridge sends tool calls, and how it authenticates there.
pub struct Hub {
    pub url: String,
    pub credential: Credential,
    pub timeout: Duration,
}

/// What to serve, and where to call it. A config without a hub still lists
/// tools, so a `--list` style caller never contacts the hub.
pub struct Config {
    pub version: String,
    pub filter: Filter,
    pub hub: Option<Hub>,
}

impl Config {
    /// Serves tools but every `tools/call` fails as unavailable.
    pub fn without_hub(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            filter: Filter::default(),
            hub: None,
        }
    }
}

/// The tools this bridge offers, and the client that calls them.
pub struct Bridge {
    version: String,
    tools: Vec<Value>,
    methods: HashMap<String, Method>,
    client: Option<Client>,
    hub_calls: Arc<AtomicUsize>,
}

impl Bridge {
    pub fn new(config: Config) -> Result<Bridge> {
        config.filter.validate()?;
        let catalog = Catalog::with_source_info()?;
        let mut tools = Vec::new();
        let mut methods = HashMap::new();
        for def in tool_defs(&catalog)? {
            if !config.filter.permits(&def)? {
                continue;
            }
            let mut annotations = json!({
                "readOnlyHint": def.read_only,
                "idempotentHint": def.read_only,
                "openWorldHint": false,
            });
            if !def.read_only {
                annotations["destructiveHint"] = json!(def.destructive);
            }
            tools.push(json!({
                "name": def.name,
                "description": def.method.description(),
                "inputSchema": input_schema(def.method.input()),
                "annotations": annotations,
            }));
            methods.insert(def.name, def.method);
        }
        let client = match config.hub {
            Some(hub) => {
                Some(Client::new(&hub.url, "", hub.timeout)?.with_credential(hub.credential))
            }
            None => None,
        };
        Ok(Bridge {
            version: config.version,
            tools,
            methods,
            client,
            hub_calls: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// The catalog served by `tools/list`.
    pub fn tools(&self) -> &[Value] {
        &self.tools
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    fn capabilities(&self) -> Value {
        let mut caps = json!({"logging": {}});
        if !self.tools.is_empty() {
            caps["tools"] = json!({"listChanged": true});
        }
        caps
    }

    /// Answers one request. `cancelled` is the session slot's flag, set by a
    /// `notifications/cancelled` for this request id.
    pub fn dispatch(&self, session: &Session, request: &Request, cancelled: &AtomicBool) -> Value {
        let id = request.id.clone().unwrap_or(Value::Null);
        if !request.params.is_null() && !request.params.is_object() {
            return Failure::new(Some(id), jsonrpc::INVALID_PARAMS, "invalid parameters")
                .into_response();
        }
        let meta = meta_of(request);
        let requested = meta
            .and_then(|meta| meta.get(META_PROTOCOL_VERSION))
            .and_then(Value::as_str);
        let modern = requested.is_some_and(|version| !legacy_version(version));
        if modern {
            let complete = |key: &str| {
                meta.and_then(|meta| meta.get(key))
                    .is_some_and(|value| value.is_object())
            };
            if !complete(META_CLIENT_CAPABILITIES)
                || meta
                    .and_then(|meta| meta.get(META_CLIENT_INFO))
                    .is_some_and(|value| !value.is_object())
            {
                return Failure::new(
                    Some(id),
                    jsonrpc::INVALID_PARAMS,
                    "invalid per-request client metadata",
                )
                .into_response();
            }
            if requested != Some(MODERN) {
                return Failure::new(
                    Some(id),
                    jsonrpc::UNSUPPORTED_PROTOCOL_VERSION,
                    "unsupported protocol version",
                )
                .with_data(json!({
                    "requested": requested,
                    "supported": PROTOCOL_VERSIONS,
                }))
                .into_response();
            }
            if matches!(
                request.method.as_str(),
                "initialize" | "ping" | "logging/setLevel"
            ) {
                return Failure::new(
                    Some(id),
                    jsonrpc::METHOD_NOT_FOUND,
                    format!("method {:?} not found", request.method),
                )
                .into_response();
            }
        }

        let outcome = self
            .dispatch_inner(session, request, cancelled, modern, &id)
            .and_then(|result| {
                if modern {
                    decorate(result, &request.method, &self.version)
                } else {
                    Ok(result)
                }
            });
        match outcome {
            Ok(result) => jsonrpc::success(&id, result),
            Err(failure) => failure.with_id(&id).into_response(),
        }
    }

    fn dispatch_inner(
        &self,
        session: &Session,
        request: &Request,
        cancelled: &AtomicBool,
        modern: bool,
        id: &Value,
    ) -> std::result::Result<Value, Failure> {
        let failure = |code: i64, message: String| Failure::new(Some(id.clone()), code, message);
        match request.method.as_str() {
            "server/discover" => {
                if !modern {
                    return Err(failure(
                        jsonrpc::METHOD_NOT_FOUND,
                        format!("server/discover requires protocol version {MODERN}"),
                    ));
                }
                return Ok(json!({
                    "supportedVersions": PROTOCOL_VERSIONS,
                    "capabilities": self.capabilities(),
                }));
            }
            "initialize" => {
                let requested = request
                    .params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .filter(|version| !version.is_empty())
                    .ok_or_else(|| {
                        failure(
                            jsonrpc::INVALID_PARAMS,
                            "invalid initialize parameters".to_owned(),
                        )
                    })?;
                let agreed = if PROTOCOL_VERSIONS.contains(&requested) && requested != MODERN {
                    requested
                } else {
                    "2025-11-25"
                };
                session.mark_initialized();
                return Ok(json!({
                    "protocolVersion": agreed,
                    "capabilities": self.capabilities(),
                    "serverInfo": {"name": SERVER_NAME, "version": self.version},
                }));
            }
            "ping" => return Ok(json!({})),
            _ => {}
        }
        if !session.is_initialized() && !modern {
            return Err(failure(
                jsonrpc::INVALID_REQUEST,
                "session is not initialized".to_owned(),
            ));
        }
        match request.method.as_str() {
            "tools/list" => {
                match request.params.get("cursor") {
                    None | Some(Value::Null) => {}
                    Some(Value::String(cursor)) if cursor.is_empty() => {}
                    _ => {
                        return Err(failure(
                            jsonrpc::INVALID_PARAMS,
                            "invalid cursor".to_owned(),
                        ))
                    }
                }
                Ok(json!({"tools": self.tools}))
            }
            "tools/call" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        failure(
                            jsonrpc::INVALID_PARAMS,
                            "invalid tools/call parameters".to_owned(),
                        )
                    })?;
                let method = self.methods.get(name).ok_or_else(|| {
                    failure(jsonrpc::INVALID_PARAMS, format!("unknown tool {:?}", name))
                })?;
                let arguments = request
                    .params
                    .get("arguments")
                    .filter(|arguments| !arguments.is_null())
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Ok(self.call_tool(method, &arguments, cancelled))
            }
            "logging/setLevel" => {
                const LEVELS: [&str; 8] = [
                    "debug",
                    "info",
                    "notice",
                    "warning",
                    "error",
                    "critical",
                    "alert",
                    "emergency",
                ];
                match request.params.get("level").and_then(Value::as_str) {
                    Some(level) if LEVELS.contains(&level) => Ok(json!({})),
                    _ => Err(failure(
                        jsonrpc::INVALID_PARAMS,
                        "invalid log level".to_owned(),
                    )),
                }
            }
            "subscriptions/listen" => Err(failure(
                jsonrpc::METHOD_NOT_FOUND,
                "subscriptions/listen unavailable on this transport".to_owned(),
            )),
            _ => Err(failure(
                jsonrpc::METHOD_NOT_FOUND,
                format!("method {:?} not found", request.method),
            )),
        }
    }

    /// Runs one tool. Hub and argument failures come back as tool results with
    /// `isError`, not as JSON-RPC errors, which is what MCP clients report.
    fn call_tool(&self, method: &Method, arguments: &Value, cancelled: &AtomicBool) -> Value {
        if cancelled.load(Ordering::SeqCst) {
            return tool_error(Code::Canceled, "request cancelled");
        }
        let Some(client) = &self.client else {
            return tool_error(Code::Unavailable, "no hub endpoint is configured");
        };
        if self
            .hub_calls
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < MAX_HUB_CALLS).then_some(count + 1)
            })
            .is_err()
        {
            return tool_error(Code::ResourceExhausted, "too many hub calls in progress");
        }
        let client = client.isolated();
        let abort = client.clone();
        let procedure = method.procedure.clone();
        let arguments = arguments.clone();
        let hub_calls = Arc::clone(&self.hub_calls);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let outcome = client.call(&procedure, &arguments);
            hub_calls.fetch_sub(1, Ordering::SeqCst);
            let _ = sender.send(outcome);
        });
        let outcome = loop {
            if cancelled.load(Ordering::SeqCst) {
                abort.shutdown();
                return tool_error(Code::Canceled, "request cancelled");
            }
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(outcome) => break outcome,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return tool_error(Code::Internal, "hub call worker stopped unexpectedly");
                }
            }
        };
        if cancelled.load(Ordering::SeqCst) {
            abort.shutdown();
            return tool_error(Code::Canceled, "request cancelled");
        }
        match outcome {
            Ok(value) => tool_text(&value.to_string()),
            Err(error) => tool_error(error.code, &error.message),
        }
    }

    /// Exchanges JSON-RPC lines until stdin closes. `tools/call` runs on its
    /// own thread so a slow hub call cannot block the rest of the session;
    /// every reply is written under one lock, in completion order.
    pub fn run<W: Write + Send + 'static>(
        self: Arc<Self>,
        mut input: impl BufRead,
        output: &Arc<Mutex<W>>,
    ) -> io::Result<()> {
        let session = Session::new();
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut subscriptions = Vec::new();
        let failures: Arc<Mutex<Option<io::Error>>> = Arc::new(Mutex::new(None));
        let mut line: Vec<u8> = Vec::new();
        let mut read_error: Option<io::Error> = None;
        while failures.lock().map(|slot| slot.is_none()).unwrap_or(false) {
            line.clear();
            match read_line(&mut input, &mut line, MAX_MESSAGE_BYTES) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) => {
                    read_error = Some(error);
                    break;
                }
            }
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let request = match jsonrpc::parse_request(&line) {
                Ok(request) => request,
                Err(failure) => {
                    record(&failures, send(output, &failure.into_response()));
                    continue;
                }
            };
            if request.is_notification() {
                notify(&session, &request);
                continue;
            }
            let id = request.id.clone().unwrap_or(Value::Null);
            let guard = match session.begin(&id) {
                Ok(guard) => guard,
                Err(failure) => {
                    record(&failures, send(output, &failure.into_response()));
                    continue;
                }
            };
            if request.method != "tools/call" && request.method != "subscriptions/listen" {
                let response = self.dispatch(&session, &request, &guard.cancelled);
                record(&failures, send(output, &response));
                drop(guard);
                continue;
            }
            let bridge = Arc::clone(&self);
            if request.method == "subscriptions/listen" {
                subscriptions.push(Arc::clone(&guard.cancelled));
            }
            let session = session.clone();
            let output = Arc::clone(output);
            let failures = Arc::clone(&failures);
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    if workers.swap_remove(index).join().is_err() {
                        return Err(io::Error::other("a tool call worker panicked"));
                    }
                } else {
                    index += 1;
                }
            }
            workers.push(std::thread::spawn(move || {
                let response = if request.method == "subscriptions/listen" {
                    bridge.listen_stdio(&request, &guard.cancelled, &output, &failures)
                } else {
                    bridge.dispatch(&session, &request, &guard.cancelled)
                };
                record(&failures, send(&output, &response));
            }));
        }
        for cancelled in subscriptions {
            cancelled.store(true, Ordering::SeqCst);
        }
        for worker in workers {
            if worker.join().is_err() {
                return Err(io::Error::other("a tool call worker panicked"));
            }
        }
        if let Some(error) = read_error {
            return Err(error);
        }
        match Arc::try_unwrap(failures) {
            Ok(slot) => match slot
                .into_inner()
                .map_err(|_| io::Error::other("lock poisoned"))?
            {
                Some(error) => Err(error),
                None => Ok(()),
            },
            Err(_) => Ok(()),
        }
    }

    /// A stdio subscription sends its acknowledgement before the final reply.
    /// Its guard stays in the session so notifications/cancelled can end it.
    fn listen_stdio<W: Write + Send + 'static>(
        &self,
        request: &Request,
        cancelled: &AtomicBool,
        output: &Arc<Mutex<W>>,
        failures: &Arc<Mutex<Option<io::Error>>>,
    ) -> Value {
        let id = request.id.clone().unwrap_or(Value::Null);
        let fail = |code, message| Failure::new(Some(id.clone()), code, message).into_response();
        if !request.params.is_object() {
            return fail(jsonrpc::INVALID_PARAMS, "invalid parameters");
        }
        let Some(meta) = meta_of(request) else {
            return fail(
                jsonrpc::METHOD_NOT_FOUND,
                "subscriptions/listen unavailable on this transport",
            );
        };
        let version = meta.get(META_PROTOCOL_VERSION).and_then(Value::as_str);
        if version.is_none_or(legacy_version) {
            return fail(
                jsonrpc::METHOD_NOT_FOUND,
                "subscriptions/listen unavailable on this transport",
            );
        }
        if !meta
            .get(META_CLIENT_CAPABILITIES)
            .is_some_and(Value::is_object)
            || meta
                .get(META_CLIENT_INFO)
                .is_some_and(|value| !value.is_object())
        {
            return fail(
                jsonrpc::INVALID_PARAMS,
                "invalid per-request client metadata",
            );
        }
        if version != Some(MODERN) {
            return Failure::new(
                Some(id.clone()),
                jsonrpc::UNSUPPORTED_PROTOCOL_VERSION,
                "unsupported protocol version",
            )
            .with_data(json!({"requested": version, "supported": PROTOCOL_VERSIONS}))
            .into_response();
        }
        let Some(notifications) = request
            .params
            .get("notifications")
            .and_then(Value::as_object)
        else {
            return fail(jsonrpc::INVALID_PARAMS, "missing notifications");
        };
        let mut agreed = Map::new();
        if notifications.get("toolsListChanged") == Some(&Value::Bool(true))
            && !self.tools.is_empty()
        {
            agreed.insert("toolsListChanged".to_owned(), Value::Bool(true));
        }
        let subscription_meta = json!({"io.modelcontextprotocol/subscriptionId": id});
        let ack = json!({
            "jsonrpc": "2.0",
            "method": "notifications/subscriptions/acknowledged",
            "params": {"_meta": subscription_meta, "notifications": agreed},
        });
        record(failures, send(output, &ack));
        if !agreed.is_empty() {
            while !cancelled.load(Ordering::SeqCst)
                && failures.lock().is_ok_and(|error| error.is_none())
            {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let result = json!({"_meta": subscription_meta});
        match decorate(result, &request.method, &self.version) {
            Ok(result) => jsonrpc::success(&id, result),
            Err(failure) => failure.with_id(&id).into_response(),
        }
    }
}

/// The `_meta` object of a request, if it has one.
pub(crate) fn meta_of(request: &Request) -> Option<&Map<String, Value>> {
    request
        .params
        .as_object()
        .and_then(|params| params.get("_meta"))
        .and_then(Value::as_object)
}

/// Whether this request asks for the stateless 2026-07-28 shape, which needs
/// no handshake and therefore no session of its own.
pub(crate) fn is_modern(request: &Request) -> bool {
    meta_of(request)
        .and_then(|meta| meta.get(META_PROTOCOL_VERSION))
        .and_then(Value::as_str)
        .is_some_and(|version| !legacy_version(version))
}

fn legacy_version(version: &str) -> bool {
    matches!(
        version,
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05"
    )
}

/// Applies `notifications/cancelled`; every other notification is ignored.
fn notify(session: &Session, request: &Request) {
    if request.method != "notifications/cancelled" {
        return;
    }
    if let Some(id) = request.params.get("requestId") {
        session.cancel(id);
    }
}

fn tool_text(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}]})
}

fn tool_error(code: Code, message: &str) -> Value {
    let name = serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    json!({
        "content": [{"type": "text", "text": format!("{name}: {message}")}],
        "isError": true,
    })
}

/// The 2026-07-28 result envelope: every result says it is complete and names
/// the server, and discoverable lists say how long they may be cached.
fn decorate(mut result: Value, method: &str, version: &str) -> std::result::Result<Value, Failure> {
    let Value::Object(map) = &mut result else {
        return Err(Failure::new(None, jsonrpc::INTERNAL_ERROR, "encode result"));
    };
    map.insert("resultType".to_owned(), json!("complete"));
    let meta = map
        .entry("_meta".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(meta) = meta else {
        return Err(Failure::new(None, jsonrpc::INTERNAL_ERROR, "encode result"));
    };
    meta.insert(
        META_SERVER_INFO.to_owned(),
        json!({"name": SERVER_NAME, "version": version}),
    );
    if method == "tools/list" || method == "server/discover" {
        map.insert("ttlMs".to_owned(), json!(0));
        map.insert("cacheScope".to_owned(), json!("public"));
    }
    Ok(result)
}

fn send<W: Write>(output: &Arc<Mutex<W>>, response: &Value) -> io::Result<()> {
    let mut writer = output
        .lock()
        .map_err(|_| io::Error::other("output lock poisoned"))?;
    serde_json::to_writer(&mut *writer, response)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn record(slot: &Arc<Mutex<Option<io::Error>>>, result: io::Result<()>) {
    if let Ok(mut slot) = slot.lock() {
        if let Err(error) = result {
            slot.get_or_insert(error);
        }
    }
}

/// One newline-delimited frame, refusing to buffer more than `limit` bytes.
fn read_line(reader: &mut impl BufRead, buffer: &mut Vec<u8>, limit: usize) -> io::Result<usize> {
    loop {
        let filled = reader.fill_buf()?;
        if filled.is_empty() {
            return Ok(buffer.len());
        }
        let room = limit + 1 - buffer.len();
        let window = filled.len().min(room);
        let newline = filled[..window].iter().position(|byte| *byte == b'\n');
        let end = newline.map_or(window, |newline| newline + 1);
        buffer.extend_from_slice(&filled[..end]);
        reader.consume(end);
        if buffer.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("message exceeds {limit} bytes"),
            ));
        }
        if newline.is_some() {
            return Ok(buffer.len());
        }
    }
}
