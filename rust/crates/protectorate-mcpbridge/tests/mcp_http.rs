//! The HTTP transport on a real loopback listener: bearer auth, the session
//! lifecycle, event streams, and the transport's own limits. The hub behind it
//! is served by the real wire codec.
use axum::Router;
use protectorate_mcpbridge::{serve, Bridge, Config, Filter, HttpConfig, Hub, PROTOCOL_VERSIONS};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

const TOKEN: &str = "bridge-token";
const MODERN: &str = "2026-07-28";

/// Serves the three RPCs the tests need: a read-only status, a delete that
/// always fails, and a mutating note creation that the default filter hides.
async fn fake_hub() -> (String, tokio::task::JoinHandle<()>) {
    let catalog = Catalog::embedded().unwrap();
    let status = catalog
        .method("/protectorate.v1.HubService/GetStatus")
        .unwrap()
        .clone();
    let delete = catalog
        .method("/protectorate.v1.BacklogService/DeleteNote")
        .unwrap()
        .clone();
    let create = catalog
        .method("/protectorate.v1.BacklogService/CreateNote")
        .unwrap()
        .clone();
    let authorize: Authorizer =
        Arc::new(
            |_, headers| match headers.get("authorization").and_then(|v| v.to_str().ok()) {
                Some(value) if value == format!("Bearer {TOKEN}") => Ok(AuthContext::default()),
                _ => Err(RpcError::new(Code::Unauthenticated, "bad bearer")),
            },
        );
    let mut server = UnaryServer::new(catalog, authorize);
    server
        .register(&status.procedure.clone(), move |_| {
            let output = status.output();
            async move {
                Ok(decode_json(
                    output,
                    br#"{"version":"fake-hub","hostname":"test","uptimeSeconds":"7"}"#,
                )?
                .into())
            }
        })
        .unwrap();
    server
        .register(&delete.procedure.clone(), move |_| async move {
            Err(RpcError::new(Code::NotFound, "no such note"))
        })
        .unwrap();
    server
        .register(&create.procedure.clone(), move |request| {
            let output = create.output();
            let title = request
                .message
                .get_field_by_name("title")
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            async move {
                Ok(decode_json(
                    output,
                    format!(r#"{{"note":{{"id":"n1","title":{title:?},"state":"open"}}}}"#)
                        .as_bytes(),
                )?
                .into())
            }
        })
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router: Router = server.router();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), task)
}

struct Server {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<protectorate_mcpbridge::Result<()>>,
}

impl Server {
    async fn start(bridge: Arc<Bridge>, config: HttpConfig) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(serve(listener, bridge, config, async {
            let _ = stopped.await;
        }));
        Server {
            addr,
            shutdown: Some(shutdown),
            task,
        }
    }

    async fn stop(self) {
        let _ = self.shutdown.unwrap().send(());
        self.task.await.unwrap().unwrap();
    }
}

async fn bridge_to(hub: &str) -> Arc<Bridge> {
    Arc::new(
        Bridge::new(Config {
            version: "test".to_owned(),
            filter: Filter::default(),
            hub: Some(Hub::new(hub, TOKEN)),
        })
        .unwrap(),
    )
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("JSON body")
    }

    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(got, _)| got == name)
            .map(|(_, value)| value.clone())
    }

    fn session(&self) -> String {
        self.header("mcp-session-id").expect("Mcp-Session-Id")
    }

    fn tool_text(&self) -> String {
        self.json()["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

async fn read_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    loop {
        let mut byte = [0_u8];
        stream.read_exact(&mut byte).await.unwrap();
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            return String::from_utf8(line).unwrap();
        }
    }
}

async fn read_head(stream: &mut TcpStream) -> (u16, Vec<(String, String)>) {
    let start = read_line(stream).await;
    let status = start
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .expect("status line");
    let mut headers = Vec::new();
    loop {
        let line = read_line(stream).await;
        if line == "\r\n" {
            return (status, headers);
        }
        let (name, value) = line.split_once(':').expect("header line");
        headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
    }
}

/// One chunk of a streamed response. `None` is the terminating chunk.
async fn read_chunk(stream: &mut TcpStream) -> Option<String> {
    let line = read_line(stream).await;
    let size =
        usize::from_str_radix(line.trim().split(';').next().unwrap(), 16).expect("chunk size");
    if size == 0 {
        return None;
    }
    let mut bytes = vec![0_u8; size];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(read_line(stream).await, "\r\n");
    Some(String::from_utf8(bytes).unwrap())
}

async fn send(stream: &mut TcpStream, method: &str, headers: &[String], body: Option<&str>) {
    let mut text = format!("{method} / HTTP/1.1\r\nHost: localhost\r\n");
    for header in headers {
        text.push_str(header);
        text.push_str("\r\n");
    }
    text.push_str(&format!(
        "Content-Length: {}\r\n\r\n",
        body.map_or(0, |body| body.len())
    ));
    if let Some(body) = body {
        text.push_str(body);
    }
    stream.write_all(text.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

/// A complete request: the connection is closed at the end of the reply.
async fn request(addr: SocketAddr, method: &str, headers: &[String], body: Option<&str>) -> Reply {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let mut headers: Vec<String> = headers.to_vec();
    headers.push("Connection: close".to_owned());
    send(&mut stream, method, &headers, body).await;
    let (status, headers) = read_head(&mut stream).await;
    let mut body = Vec::new();
    stream.read_to_end(&mut body).await.unwrap();
    Reply {
        status,
        headers,
        body,
    }
}

async fn post(addr: SocketAddr, token: &str, session: Option<&str>, body: Value) -> Reply {
    let mut headers = vec![
        format!("Authorization: Bearer {token}"),
        "Content-Type: application/json".to_owned(),
        "Accept: application/json, text/event-stream".to_owned(),
    ];
    if let Some(session) = session {
        headers.push(format!("Mcp-Session-Id: {session}"));
        headers.push("MCP-Protocol-Version: 2025-11-25".to_owned());
    }
    if body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] == MODERN {
        headers.retain(|header| !header.starts_with("MCP-Protocol-Version:"));
        headers.push(format!("MCP-Protocol-Version: {MODERN}"));
        headers.push(format!("Mcp-Method: {}", body["method"].as_str().unwrap()));
        if let Some(name) = body["params"]["name"]
            .as_str()
            .or_else(|| body["params"]["uri"].as_str())
        {
            headers.push(format!("Mcp-Name: {name}"));
        }
    }
    request(addr, "POST", &headers, Some(&body.to_string())).await
}

async fn legacy_post_without_version(addr: SocketAddr, session: &str, body: Value) -> Reply {
    request(
        addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            "Content-Type: application/json".to_owned(),
            format!("Mcp-Session-Id: {session}"),
        ],
        Some(&body.to_string()),
    )
    .await
}

#[tokio::test]
async fn legacy_clients_can_omit_version_after_initializing() {
    let (hub, hub_task) = fake_hub().await;
    let mut config = HttpConfig::new(TOKEN).unwrap();
    config.max_sessions = 1;
    let server = Server::start(bridge_to(&hub).await, config).await;
    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();

    let reply = legacy_post_without_version(
        server.addr,
        &session,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert!(reply.json()["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "hub_get_status"));

    let rejected = legacy_post_without_version(
        server.addr,
        &session,
        json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(rejected.status, 200);
    assert_eq!(rejected.json()["error"]["code"], -32602);
    let after = legacy_post_without_version(
        server.addr,
        &session,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"hub_get_status","arguments":{}
        }}),
    )
    .await;
    assert_eq!(after.status, 200);
    assert_eq!(after.json()["result"]["isError"], Value::Null);

    let wrong = request(
        server.addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            "Content-Type: application/json".to_owned(),
            format!("Mcp-Session-Id: {session}"),
            "MCP-Protocol-Version: 2025-06-18".to_owned(),
        ],
        Some(&json!({"jsonrpc":"2.0","id":5,"method":"ping"}).to_string()),
    )
    .await;
    assert_eq!(wrong.status, 400);
    hub_task.abort();
    server.stop().await;
}

/// A request whose response is a stream the test reads frame by frame.
async fn open(
    addr: SocketAddr,
    method: &str,
    token: &str,
    session: &str,
    body: Option<Value>,
) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let mut headers = vec![
        format!("Authorization: Bearer {token}"),
        format!("Mcp-Session-Id: {session}"),
        "Content-Type: application/json".to_owned(),
        "Accept: application/json, text/event-stream".to_owned(),
        if body.as_ref().is_some_and(|body| {
            body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] == MODERN
        }) {
            format!("MCP-Protocol-Version: {MODERN}")
        } else {
            "MCP-Protocol-Version: 2025-11-25".to_owned()
        },
    ];
    if let Some(method) = body.as_ref().and_then(|body| body["method"].as_str()) {
        headers.push(format!("Mcp-Method: {method}"));
    }
    send(
        &mut stream,
        method,
        &headers,
        body.map(|body| body.to_string()).as_deref(),
    )
    .await;
    stream
}

/// The next SSE message, skipping keepalive comments.
async fn next_event(stream: &mut TcpStream) -> Value {
    loop {
        let Some(chunk) = read_chunk(stream).await else {
            panic!("the event stream ended");
        };
        for frame in chunk.split("\n\n").filter(|frame| !frame.trim().is_empty()) {
            if frame.trim_start().starts_with(':') {
                continue;
            }
            let data = frame
                .lines()
                .find(|line| line.starts_with("data: "))
                .expect("data frame")
                .trim()
                .trim_start_matches("data: ")
                .to_owned();
            return serde_json::from_str(&data).expect("SSE data");
        }
    }
}

async fn expect_end(stream: &mut TcpStream) {
    assert!(read_chunk(stream).await.is_none());
}

fn initialize(id: i64) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
        "protocolVersion":"2025-11-25",
        "capabilities":{},
        "clientInfo":{"name":"test","version":"1"}
    }})
}

fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": MODERN,
        "io.modelcontextprotocol/clientInfo": {"name":"modern","version":"1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

#[tokio::test]
async fn every_request_needs_the_bearer() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;

    let reply = request(
        server.addr,
        "POST",
        &["Content-Type: application/json".to_owned()],
        Some(&initialize(1).to_string()),
    )
    .await;
    assert_eq!(reply.status, 401);
    assert_eq!(reply.header("www-authenticate").as_deref(), Some("Bearer"));

    let reply = post(server.addr, "wrong", None, initialize(1)).await;
    assert_eq!(reply.status, 401);

    let reply = request(
        server.addr,
        "GET",
        &["Mcp-Session-Id: abc".to_owned()],
        None,
    )
    .await;
    assert_eq!(reply.status, 401);
    let reply = request(
        server.addr,
        "DELETE",
        &["Mcp-Session-Id: abc".to_owned()],
        None,
    )
    .await;
    assert_eq!(reply.status, 401);

    let reply = post(server.addr, TOKEN, None, initialize(1)).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["result"]["serverInfo"]["name"], "protectorate");
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn a_session_is_opened_by_initialize_and_closed_by_delete() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;

    let reply = post(server.addr, TOKEN, None, initialize(1)).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["result"]["protocolVersion"], "2025-11-25");
    let session = reply.session();
    assert_eq!(session.len(), 48);

    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(reply.status, 200);
    let listed = reply.json();
    let served: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(served.contains(&"hub_get_status"));
    assert!(!served.contains(&"backlog_create_note"));

    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"hub_get_status","arguments":{}
        }}),
    )
    .await;
    assert_eq!(reply.status, 200);
    let status: Value = serde_json::from_str(&reply.tool_text()).unwrap();
    assert_eq!(status["version"], "fake-hub");
    let ping = json!({"jsonrpc":"2.0","id":31,"method":"ping"}).to_string();
    let missing_version = request(
        server.addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            format!("Mcp-Session-Id: {session}"),
            "Content-Type: application/json".to_owned(),
        ],
        Some(&ping),
    )
    .await;
    assert_eq!(missing_version.status, 200);
    let wrong_version = request(
        server.addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            format!("Mcp-Session-Id: {session}"),
            "MCP-Protocol-Version: 2025-03-26".to_owned(),
            "Content-Type: application/json".to_owned(),
        ],
        Some(&ping),
    )
    .await;
    assert_eq!(wrong_version.status, 400);

    // A session id the bridge never handed out is refused, so the client
    // re-initializes instead of talking to a stale session.
    let reply = post(
        server.addr,
        TOKEN,
        Some("0123456789abcdef"),
        json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 404);

    // And a request that needs a session says so instead of starting one.
    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":5,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 400);

    let reply = request(
        server.addr,
        "DELETE",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            format!("Mcp-Session-Id: {session}"),
            "MCP-Protocol-Version: 2025-11-25".to_owned(),
        ],
        None,
    )
    .await;
    assert_eq!(reply.status, 204);
    assert!(reply.body.is_empty());

    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":6,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 404);

    let reply = request(
        server.addr,
        "DELETE",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            "Mcp-Session-Id: missing".to_owned(),
        ],
        None,
    )
    .await;
    assert_eq!(reply.status, 404);
    let reply = request(
        server.addr,
        "DELETE",
        &[format!("Authorization: Bearer {TOKEN}")],
        None,
    )
    .await;
    assert_eq!(reply.status, 400);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn modern_headers_and_origin_are_validated_before_dispatch() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;
    let body =
        json!({"jsonrpc":"2.0","id":7,"method":"server/discover","params":{"_meta":modern_meta()}})
            .to_string();
    let valid = vec![
        format!("Authorization: Bearer {TOKEN}"),
        "Content-Type: application/json".to_owned(),
        "Accept: application/json, text/event-stream".to_owned(),
        format!("MCP-Protocol-Version: {MODERN}"),
        "Mcp-Method: server/discover".to_owned(),
    ];
    let reply = request(server.addr, "POST", &valid, Some(&body)).await;
    assert_eq!(reply.status, 200);
    let mut stale_session = valid.clone();
    stale_session.push(format!("Mcp-Session-Id: {}", "x".repeat(200)));
    assert_eq!(
        request(server.addr, "POST", &stale_session, Some(&body))
            .await
            .status,
        200
    );
    let unknown_method = body.replace("server/discover", "unknown/operation");
    let mut unknown_headers = valid.clone();
    unknown_headers.retain(|header| !header.starts_with("Mcp-Method:"));
    unknown_headers.push("Mcp-Method: unknown/operation".to_owned());
    let reply = request(server.addr, "POST", &unknown_headers, Some(&unknown_method)).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.json()["error"]["code"], -32601);
    for broken in [
        valid[..4].to_vec(),
        valid
            .iter()
            .filter(|header| !header.starts_with("MCP-Protocol-Version:"))
            .cloned()
            .collect(),
        valid
            .iter()
            .map(|header| {
                if header.starts_with("Mcp-Method:") {
                    "Mcp-Method: tools/call".to_owned()
                } else {
                    header.clone()
                }
            })
            .collect(),
    ] {
        let reply = request(server.addr, "POST", &broken, Some(&body)).await;
        assert_eq!(reply.status, 400);
        assert_eq!(reply.json()["error"]["code"], -32020);
    }
    let mut wrong_origin = valid.clone();
    wrong_origin.push("Origin: http://evil.example".to_owned());
    assert_eq!(
        request(server.addr, "POST", &wrong_origin, Some(&body))
            .await
            .status,
        403
    );
    let mut same_origin = valid.clone();
    same_origin.push("Origin: http://localhost".to_owned());
    assert_eq!(
        request(server.addr, "POST", &same_origin, Some(&body))
            .await
            .status,
        200
    );
    same_origin.push("Origin: http://evil.example".to_owned());
    assert_eq!(
        request(server.addr, "POST", &same_origin, Some(&body))
            .await
            .status,
        403
    );
    let mut no_info: Value = serde_json::from_str(&body).unwrap();
    no_info["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientInfo");
    // Client identity is optional in the finalized 2026 protocol.
    assert_eq!(
        request(server.addr, "POST", &valid, Some(&no_info.to_string()))
            .await
            .status,
        200
    );
    let mut no_capabilities: Value = serde_json::from_str(&body).unwrap();
    no_capabilities["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientCapabilities");
    let reply = request(
        server.addr,
        "POST",
        &valid,
        Some(&no_capabilities.to_string()),
    )
    .await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.json()["error"]["code"], -32602);

    let named = json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{
        "name":"Hello 世界","arguments":{},"_meta":modern_meta()
    }})
    .to_string();
    let mut headers = valid.clone();
    headers.retain(|header| !header.starts_with("Mcp-Method:"));
    headers.push("Mcp-Method: tools/call".to_owned());
    let missing = request(server.addr, "POST", &headers, Some(&named)).await;
    assert_eq!(missing.status, 400);
    assert_eq!(missing.json()["error"]["code"], -32020);
    headers.push("Mcp-Name: =?base64?SGVsbG8g5LiW55WM?=".to_owned());
    let valid_name = request(server.addr, "POST", &headers, Some(&named)).await;
    assert_eq!(valid_name.status, 200);
    assert_eq!(valid_name.json()["error"]["code"], -32602); // unknown tool, valid routing header
    headers.pop();
    headers.push("Mcp-Name: =?base64?%%%?=".to_owned());
    let malformed = request(server.addr, "POST", &headers, Some(&named)).await;
    assert_eq!(malformed.status, 400);
    assert_eq!(malformed.json()["error"]["code"], -32020);

    let unknown = body.replace(MODERN, "2099-01-01");
    let mut unsupported = valid.clone();
    unsupported.retain(|header| !header.starts_with("MCP-Protocol-Version:"));
    unsupported.push("MCP-Protocol-Version: 2099-01-01".to_owned());
    let reply = request(server.addr, "POST", &unsupported, Some(&unknown)).await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.json()["error"]["code"], -32022);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn transport_mistakes_get_http_status_codes() {
    let (hub, hub_task) = fake_hub().await;
    let mut config = HttpConfig::new(TOKEN).unwrap();
    config.max_body_bytes = 64;
    let server = Server::start(bridge_to(&hub).await, config).await;

    let reply = request(
        server.addr,
        "PUT",
        &[format!("Authorization: Bearer {TOKEN}")],
        None,
    )
    .await;
    assert_eq!(reply.status, 405);
    assert_eq!(reply.header("allow").as_deref(), Some("GET, POST, DELETE"));

    let reply = request(
        server.addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            "Content-Type: text/plain".to_owned(),
        ],
        Some("{}"),
    )
    .await;
    assert_eq!(reply.status, 415);

    // A frame that is not JSON at all is still answered as JSON-RPC.
    let reply = request(
        server.addr,
        "POST",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            "Content-Type: application/json".to_owned(),
        ],
        Some("not json at all"),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["error"]["code"], -32700);
    assert_eq!(reply.json()["id"], Value::Null);

    // Valid JSON that is not a request object is refused the same way.
    let reply = post(server.addr, TOKEN, None, json!("not an object")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["error"]["code"], -32600);

    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"pad":"x".repeat(128)}}),
    )
    .await;
    assert_eq!(reply.status, 413);

    let reply = request(
        server.addr,
        "GET",
        &[format!("Authorization: Bearer {TOKEN}")],
        None,
    )
    .await;
    assert_eq!(reply.status, 400);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn stateless_requests_are_limited_to_the_handshake_and_the_new_shape() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;

    // The 2026-07-28 shape carries its client info per request, so it needs no
    // session and gets none.
    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":modern_meta()}}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.json()["result"]["supportedVersions"],
        json!(PROTOCOL_VERSIONS)
    );
    assert_eq!(reply.json()["result"]["resultType"], "complete");
    assert!(reply.header("mcp-session-id").is_none());

    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{
            "_meta":modern_meta()
        }}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["result"]["resultType"], "complete");
    assert!(reply.json()["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "quota_get_quota"));

    // A legacy request without a session cannot be answered statelessly.
    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(reply.status, 400);
    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}),
    )
    .await;
    assert_eq!(reply.status, 400);

    // A failed handshake does not create a session.
    let reply = post(
        server.addr,
        TOKEN,
        None,
        json!({"jsonrpc":"2.0","id":4,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["error"]["code"], -32602);
    assert!(reply.header("mcp-session-id").is_none());
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn hub_failures_arrive_as_tool_errors() {
    let (hub, hub_task) = fake_hub().await;
    let bridge = Arc::new(
        Bridge::new(Config {
            version: "test".to_owned(),
            filter: Filter {
                allow: vec!["backlog_delete_note".to_owned()],
                deny: vec![],
            },
            hub: Some(Hub::new(&hub, TOKEN)),
        })
        .unwrap(),
    );
    let server = Server::start(bridge, HttpConfig::new(TOKEN).unwrap()).await;

    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"backlog_delete_note","arguments":{"id":"missing"}
        }}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["result"]["isError"], true);
    assert_eq!(reply.tool_text(), "not_found: no such note");

    // The hub token the bridge was started with is what the hub sees.
    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"backlog_create_note","arguments":{"title":"hidden"}
        }}),
    )
    .await;
    assert_eq!(
        reply.json()["error"]["message"],
        "unknown tool \"backlog_create_note\""
    );
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn modern_listen_acknowledges_on_its_own_post_stream() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;
    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let mut legacy_stream = open(server.addr, "GET", TOKEN, &session, None).await;
    assert_eq!(read_head(&mut legacy_stream).await.0, 200);
    assert!(read_chunk(&mut legacy_stream)
        .await
        .unwrap()
        .contains(": open"));
    let mut subscription = open(
        server.addr,
        "POST",
        TOKEN,
        "",
        Some(
            json!({"jsonrpc":"2.0","id":7,"method":"subscriptions/listen","params":{
                "_meta":modern_meta(),
                "notifications":{"toolsListChanged":true}
            }}),
        ),
    )
    .await;
    let (status, headers) = read_head(&mut subscription).await;
    assert_eq!(status, 200);
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "content-type")
            .map(|(_, value)| value.clone())
            .as_deref(),
        Some("text/event-stream")
    );

    let acknowledged = next_event(&mut subscription).await;
    assert_eq!(
        acknowledged["method"],
        "notifications/subscriptions/acknowledged"
    );
    assert_eq!(
        acknowledged["params"]["notifications"]["toolsListChanged"],
        true
    );
    assert_eq!(
        acknowledged["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
        7
    );
    // The legacy GET stream must not receive the modern acknowledgement.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), read_chunk(&mut legacy_stream))
            .await
            .is_err()
    );
    drop(subscription);
    drop(legacy_stream);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn closing_a_legacy_session_ends_its_get_stream() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;

    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let mut stream = open(server.addr, "GET", TOKEN, &session, None).await;
    assert_eq!(read_head(&mut stream).await.0, 200);

    let reply = request(
        server.addr,
        "DELETE",
        &[
            format!("Authorization: Bearer {TOKEN}"),
            format!("Mcp-Session-Id: {session}"),
            "MCP-Protocol-Version: 2025-11-25".to_owned(),
        ],
        None,
    )
    .await;
    assert_eq!(reply.status, 204);
    // The stream's opening comment arrives before the terminating chunk.
    assert!(read_chunk(&mut stream).await.unwrap().contains(": open"));
    expect_end(&mut stream).await;
    drop(stream);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn subscriptions_need_the_new_shape_and_no_legacy_session() {
    let (hub, hub_task) = fake_hub().await;
    let server = Server::start(bridge_to(&hub).await, HttpConfig::new(TOKEN).unwrap()).await;

    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":1,"method":"subscriptions/listen","params":{
            "notifications":{"toolsListChanged":true}
        }}),
    )
    .await;
    assert_eq!(reply.json()["error"]["code"], -32601);

    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"subscriptions/listen","params":{
            "_meta":modern_meta()
        }}),
    )
    .await;
    assert_eq!(reply.json()["error"]["code"], -32602);
    assert_eq!(reply.json()["error"]["message"], "missing notifications");

    // A modern subscription is independent of the legacy session.
    let mut subscription = open(
        server.addr,
        "POST",
        TOKEN,
        "",
        Some(
            json!({"jsonrpc":"2.0","id":3,"method":"subscriptions/listen","params":{
                "_meta":modern_meta(),
                "notifications":{"toolsListChanged":true}
            }}),
        ),
    )
    .await;
    assert_eq!(read_head(&mut subscription).await.0, 200);
    assert_eq!(
        next_event(&mut subscription).await["params"]["_meta"]
            ["io.modelcontextprotocol/subscriptionId"],
        3
    );
    drop(subscription);

    // An empty filter still gets the required acknowledgement as the first
    // JSON-RPC message on its own SSE response.
    let mut empty = open(
        server.addr,
        "POST",
        TOKEN,
        "",
        Some(
            json!({"jsonrpc":"2.0","id":4,"method":"subscriptions/listen","params":{
                "_meta":modern_meta(),
                "notifications":{}
            }}),
        ),
    )
    .await;
    assert_eq!(read_head(&mut empty).await.0, 200);
    assert_eq!(
        next_event(&mut empty).await["params"]["notifications"],
        json!({})
    );
    drop(empty);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn a_full_bridge_refuses_new_sessions_and_expired_ones_are_gone() {
    let (hub, hub_task) = fake_hub().await;
    let mut config = HttpConfig::new(TOKEN).unwrap();
    config.max_sessions = 2;
    config.session_idle = Duration::from_millis(200);
    let server = Server::start(bridge_to(&hub).await, config).await;

    let first = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let _second = post(server.addr, TOKEN, None, initialize(2))
        .await
        .session();
    let reply = post(server.addr, TOKEN, None, initialize(3)).await;
    assert_eq!(reply.status, 503);

    // Existing clients keep working while the bridge is full.
    let reply = post(
        server.addr,
        TOKEN,
        Some(&first),
        json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 200);

    // An idle session is expired on the next request that comes through.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let reply = post(
        server.addr,
        TOKEN,
        Some(&first),
        json!({"jsonrpc":"2.0","id":5,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 404);
    hub_task.abort();
    server.stop().await;
}

#[tokio::test]
async fn modern_streams_do_not_consume_legacy_session_capacity() {
    let (hub, hub_task) = fake_hub().await;
    let mut config = HttpConfig::new(TOKEN).unwrap();
    config.max_pending_requests = 1;
    let server = Server::start(bridge_to(&hub).await, config).await;

    let session = post(server.addr, TOKEN, None, initialize(1))
        .await
        .session();
    let mut held: Vec<TcpStream> = Vec::new();
    for id in 1..=2 {
        let mut stream = open(
            server.addr,
            "POST",
            TOKEN,
            "",
            Some(
                json!({"jsonrpc":"2.0","id":id,"method":"subscriptions/listen","params":{
                    "_meta":modern_meta(),
                    "notifications":{"toolsListChanged":true}
                }}),
            ),
        )
        .await;
        read_head(&mut stream).await;
        next_event(&mut stream).await;
        held.push(stream);
    }

    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.status, 200);
    assert!(reply.json().get("error").is_none());
    drop(held);
    let reply = post(
        server.addr,
        TOKEN,
        Some(&session),
        json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
    )
    .await;
    assert_eq!(reply.json().get("error"), None);
    hub_task.abort();
    server.stop().await;
}
