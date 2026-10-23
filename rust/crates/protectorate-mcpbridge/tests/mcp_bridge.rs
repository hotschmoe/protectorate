//! The bridge against a fake hub served by the real wire codec: protocol
//! framing, tool filtering, argument validation and error reporting.
use axum::Router;
use protectorate_mcpbridge::{Bridge, Config, Filter, Hub, PROTOCOL_VERSIONS};
use protectorate_wire::{
    decode_json, AuthContext, Authorizer, Catalog, Code, RpcError, UnaryServer,
};
use serde_json::{json, Value};
use std::io::{BufReader, Cursor};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
#[test]
fn modern_stdio_subscription_acknowledges_then_finishes_on_cancellation() {
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixStream;

    let bridge = bridge(None, &["hub_get_status"], &[]);
    let (mut input, server_input) = UnixStream::pair().unwrap();
    let (server_output, output) = UnixStream::pair().unwrap();
    output
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let output = BufReader::new(output);
    let worker = std::thread::spawn(move || {
        bridge
            .run(
                BufReader::new(server_input),
                &Arc::new(Mutex::new(server_output)),
            )
            .unwrap();
    });
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name":"test","version":"1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":6,"method":"subscriptions/listen","params":{
            "_meta":meta,"notifications":{"toolsListChanged":true}
        }})
    )
    .unwrap();
    let mut lines = output.lines();
    let ack: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(ack["method"], "notifications/subscriptions/acknowledged");
    assert_eq!(
        ack["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
        6
    );
    assert_eq!(ack["params"]["notifications"]["toolsListChanged"], true);

    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"_meta":meta}})
    )
    .unwrap();
    let listed: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(listed["id"], 7);
    assert_eq!(listed["result"]["tools"][0]["name"], "hub_get_status");

    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":6}})
    )
    .unwrap();
    let done: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(done["id"], 6);
    assert_eq!(done["result"]["resultType"], "complete");
    drop(input);
    worker.join().unwrap();
}

const TOKEN: &str = "bridge-token";

/// Serves three RPCs with the real Connect JSON codec: a read-only status, a
/// mutating note creation, and a delete that always fails.
async fn fake_hub() -> (String, tokio::task::JoinHandle<()>) {
    let catalog = Catalog::embedded().unwrap();
    let status = catalog
        .method("/protectorate.v1.HubService/GetStatus")
        .unwrap()
        .clone();
    let create = catalog
        .method("/protectorate.v1.BacklogService/CreateNote")
        .unwrap()
        .clone();
    let delete = catalog
        .method("/protectorate.v1.BacklogService/DeleteNote")
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
    server
        .register(&delete.procedure.clone(), move |_| async move {
            Err(RpcError::new(Code::NotFound, "no such note"))
        })
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router: Router = server.router();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), task)
}

fn bridge(hub: Option<Hub>, allow: &[&str], deny: &[&str]) -> Arc<Bridge> {
    Arc::new(
        Bridge::new(Config {
            version: "test".to_owned(),
            filter: Filter {
                allow: allow.iter().map(|p| (*p).to_owned()).collect(),
                deny: deny.iter().map(|p| (*p).to_owned()).collect(),
            },
            hub,
        })
        .unwrap(),
    )
}

/// Writes every line, runs the session to EOF and returns the replies.
fn exchange(bridge: &Arc<Bridge>, requests: &[Value]) -> Vec<Value> {
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<String>>()
        .join("\n")
        + "\n";
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    Arc::clone(bridge)
        .run(BufReader::new(Cursor::new(input)), &output)
        .unwrap();
    let bytes = std::mem::take(&mut *output.lock().unwrap());
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn initialize(id: i64) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
        "protocolVersion":"2025-11-25",
        "capabilities":{},
        "clientInfo":{"name":"test","version":"1"}
    }})
}

fn call(id: i64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{
        "name":name,"arguments":arguments
    }})
}

fn tool_text(response: &Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The same exchange off the async runtime: the bridge blocks on hub calls and
/// the fake hub needs a worker thread of its own.
async fn exchange_async(bridge: &Arc<Bridge>, requests: &[Value]) -> Vec<Value> {
    let bridge = Arc::clone(bridge);
    let requests = requests.to_vec();
    tokio::task::spawn_blocking(move || exchange(&bridge, &requests))
        .await
        .unwrap()
}

/// tools/call replies come back in completion order, so tests look them up by id.
fn reply_for(replies: &[Value], id: i64) -> Value {
    replies
        .iter()
        .find(|reply| reply["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no reply for id {id}"))
}

fn names(bridge: &Arc<Bridge>) -> Vec<String> {
    bridge
        .tools()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn serves_read_only_tools_and_calls_the_hub() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(Some(Hub::new(base, TOKEN)), &[], &[]);

    let replies = exchange_async(
        &bridge,
        &[
            initialize(1),
            json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}),
            call(4, "hub_get_status", json!({})),
        ],
    )
    .await;
    task.abort();

    let initialized = &replies[0];
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "protectorate");
    assert_eq!(initialized["result"]["serverInfo"]["version"], "test");
    assert_eq!(
        initialized["result"]["capabilities"]["tools"]["listChanged"],
        true
    );
    assert_eq!(replies[1]["result"], json!({}));

    let tools = replies[2]["result"]["tools"].as_array().unwrap();
    let served: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(served.contains(&"hub_get_status"));
    assert!(served.contains(&"backlog_list_notes"));
    assert!(!served.contains(&"backlog_create_note"));
    assert!(!served.contains(&"backlog_delete_note"));
    let status = tools
        .iter()
        .find(|t| t["name"] == "hub_get_status")
        .unwrap();
    assert_eq!(status["annotations"]["readOnlyHint"], true);
    assert_eq!(status["annotations"]["openWorldHint"], false);
    assert!(status["annotations"].get("destructiveHint").is_none());
    assert_eq!(
        status["inputSchema"],
        json!({"type":"object","properties":{},"additionalProperties":false})
    );
    assert!(tools
        .windows(2)
        .all(|pair| pair[0]["name"].as_str() <= pair[1]["name"].as_str()));

    let called = &replies[3];
    assert_eq!(called["result"].get("isError"), None);
    let hub_reply: Value = serde_json::from_str(&tool_text(called)).unwrap();
    assert_eq!(hub_reply["version"], "fake-hub");
    assert_eq!(hub_reply["uptimeSeconds"], "7");
}

#[tokio::test]
async fn allow_and_deny_globs_decide_what_is_callable() {
    let (base, task) = fake_hub().await;
    let allowed = bridge(
        Some(Hub::new(base.clone(), TOKEN)),
        &["backlog_create_note", "hub_*", "sleeve_browse_*"],
        &["hub_get_sleeve_*"],
    );
    let mut served = names(&allowed);
    served.sort();
    assert_eq!(
        served,
        vec![
            "backlog_create_note",
            "hub_get_status",
            "sleeve_browse_workspaces"
        ]
    );

    let replies = exchange_async(
        &allowed,
        &[
            initialize(1),
            call(2, "backlog_create_note", json!({"title":"from the bridge"})),
            call(3, "hub_get_sleeve_runtime", json!({})),
        ],
    )
    .await;
    task.abort();

    let note: Value = serde_json::from_str(&tool_text(&reply_for(&replies, 2))).unwrap();
    assert_eq!(note["note"]["id"], "n1");
    assert_eq!(note["note"]["title"], "from the bridge");
    // --deny hides the tool completely, so it is not even callable.
    let denied = reply_for(&replies, 3);
    assert_eq!(denied["error"]["code"], -32602);
    assert_eq!(
        denied["error"]["message"],
        "unknown tool \"hub_get_sleeve_runtime\""
    );

    let hidden = bridge(Some(Hub::new(base, TOKEN)), &[], &[]);
    let replies = exchange_async(
        &hidden,
        &[
            initialize(1),
            call(2, "backlog_create_note", json!({"title":"nope"})),
        ],
    )
    .await;
    assert_eq!(replies[1]["error"]["code"], -32602);
    assert_eq!(
        replies[1]["error"]["message"],
        "unknown tool \"backlog_create_note\""
    );
}

#[tokio::test]
async fn hub_and_argument_failures_come_back_as_tool_errors() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(
        Some(Hub::new(base, TOKEN)),
        &["backlog_delete_note", "hub_get_status"],
        &[],
    );
    let replies = exchange_async(
        &bridge,
        &[
            initialize(1),
            call(2, "backlog_delete_note", json!({"id":"missing"})),
            call(3, "hub_get_status", json!({"notAField":true})),
            call(4, "hub_get_status", json!("a string is not a message")),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
                "name":"hub_get_status","arguments":null
            }}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"hub_get_status"}}),
        ],
    ).await;
    task.abort();

    // tools/call replies arrive in completion order, so look them up by id.
    let replies: Vec<Value> = {
        let mut sorted = replies;
        sorted.sort_by_key(|reply| reply["id"].as_i64().unwrap());
        sorted
    };
    assert_eq!(replies[1]["result"]["isError"], true);
    assert_eq!(tool_text(&replies[1]), "not_found: no such note");

    assert_eq!(replies[2]["result"]["isError"], true);
    assert!(tool_text(&replies[2]).starts_with("invalid_argument:"));

    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(tool_text(&replies[3]).starts_with("invalid_argument:"));

    // A null, absent or empty arguments object is an empty request message.
    for reply in &replies[4..] {
        assert_eq!(reply["result"].get("isError"), None);
        let status: Value = serde_json::from_str(&tool_text(reply)).unwrap();
        assert_eq!(status["hostname"], "test");
    }
}

#[tokio::test]
async fn a_bad_frame_or_a_bad_request_gets_an_error_and_the_session_survives() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(Some(Hub::new(base, TOKEN)), &[], &[]);
    let input = "not json at all\n\
                 {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"bogus/method\"}\n\
                 \n\
                 {\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":99}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"initialize\",\"params\":{}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2099-01-01\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"ping\"}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"logging/setLevel\",\"params\":{\"level\":\"warning\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"logging/setLevel\",\"params\":{\"level\":\"vibes\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/list\",\"params\":{\"cursor\":\"abc\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"server/discover\"}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"subscriptions/listen\",\"params\":{}}\n"
        .to_owned();
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    Arc::clone(&bridge)
        .run(BufReader::new(Cursor::new(input)), &output)
        .unwrap();
    task.abort();
    let bytes = std::mem::take(&mut *output.lock().unwrap());
    let replies: Vec<Value> = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    // The blank line and the notification get no reply at all.
    assert_eq!(replies.len(), 12);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[0]["id"], Value::Null);
    assert_eq!(replies[1]["error"]["code"], -32600);
    assert_eq!(replies[1]["error"]["message"], "session is not initialized");
    assert_eq!(replies[1]["id"], 1);
    assert_eq!(replies[2]["error"]["message"], "session is not initialized");
    assert_eq!(replies[3]["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(replies[4]["error"]["code"], -32602);
    assert_eq!(
        replies[4]["error"]["message"],
        "invalid initialize parameters"
    );
    // An unknown version falls back to the one the bridge prefers.
    assert_eq!(replies[5]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[6]["result"], json!({}));
    assert_eq!(replies[7]["result"], json!({}));
    assert_eq!(replies[8]["error"]["message"], "invalid log level");
    assert_eq!(replies[9]["error"]["message"], "invalid cursor");
    assert_eq!(replies[10]["error"]["code"], -32601);
    assert_eq!(
        replies[10]["error"]["message"],
        "server/discover requires protocol version 2026-07-28"
    );
    assert_eq!(
        replies[11]["error"]["message"],
        "subscriptions/listen unavailable on this transport"
    );
}

#[tokio::test]
async fn tool_calls_run_concurrently_and_cancellation_is_reported() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(Some(Hub::new(base, TOKEN)), &["backlog_create_note"], &[]);
    let mut requests: Vec<Value> = vec![initialize(0)];
    requests.extend(
        (1..=8)
            .map(|id| {
                call(
                    id,
                    "backlog_create_note",
                    json!({"title": format!("note {id}")}),
                )
            })
            .collect::<Vec<Value>>(),
    );
    let mut replies = exchange_async(&bridge, &requests).await;
    task.abort();

    assert_eq!(replies.len(), 9);
    // Worker threads answer in completion order, so sort by id before checking.
    // Worker threads answer in completion order, so sort by id before checking.
    replies.sort_by_key(|reply| reply["id"].as_i64().unwrap());
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    for (index, reply) in replies[1..].iter().enumerate() {
        assert_eq!(reply["id"], (index + 1) as i64);
        let note: Value = serde_json::from_str(&tool_text(reply)).unwrap();
        assert_eq!(note["note"]["title"], format!("note {}", index + 1));
    }

    // A request id cancelled before it is dispatched never reaches the hub.
    let session = protectorate_mcpbridge::Session::new();
    session.mark_initialized();
    let guard = session.begin(&json!("slow")).unwrap();
    session.cancel(&json!("slow"));
    let request = protectorate_mcpbridge::Request {
        id: Some(json!("slow")),
        method: "tools/call".to_owned(),
        params: json!({"name":"backlog_create_note","arguments":{"title":"never sent"}}),
    };
    let reply = bridge.dispatch(&session, &request, &guard.cancelled);
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(tool_text(&reply), "canceled: request cancelled");
}

#[tokio::test]
async fn an_inflight_hub_call_replies_promptly_after_cancellation() {
    let catalog = Catalog::embedded().unwrap();
    let status = catalog
        .method("/protectorate.v1.HubService/GetStatus")
        .unwrap()
        .clone();
    let authorize: Authorizer = Arc::new(|_, _| Ok(AuthContext::default()));
    let mut server = UnaryServer::new(catalog, authorize);
    let (started, observed) = tokio::sync::oneshot::channel();
    let started = Arc::new(Mutex::new(Some(started)));
    server
        .register(&status.procedure.clone(), move |_| {
            let started = Arc::clone(&started);
            let status = status.clone();
            async move {
                if let Some(started) = started.lock().unwrap().take() {
                    let _ = started.send(());
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(decode_json(status.output(), br#"{"hostname":"slow"}"#)?.into())
            }
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let bridge = bridge(Some(Hub::new(base, TOKEN)), &[], &[]);
    let session = protectorate_mcpbridge::Session::new();
    session.mark_initialized();
    let guard = session.begin(&json!(7)).unwrap();
    let request = protectorate_mcpbridge::Request {
        id: Some(json!(7)),
        method: "tools/call".to_owned(),
        params: json!({"name":"hub_get_status","arguments":{}}),
    };
    let work = tokio::task::spawn_blocking({
        let bridge = Arc::clone(&bridge);
        let session = session.clone();
        move || bridge.dispatch(&session, &request, &guard.cancelled)
    });
    tokio::time::timeout(Duration::from_secs(1), observed)
        .await
        .unwrap()
        .unwrap();
    session.cancel(&json!(7));
    let reply = tokio::time::timeout(Duration::from_millis(500), work)
        .await
        .expect("cancellation should not wait for the hub timeout")
        .unwrap();
    assert_eq!(tool_text(&reply), "canceled: request cancelled");
    task.abort();
}

#[tokio::test]
async fn repeated_cancellation_releases_hub_network_capacity() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (started, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let active = Arc::new(AtomicUsize::new(0));
    let reply_now = Arc::new(AtomicBool::new(false));
    let server = tokio::spawn({
        let active = Arc::clone(&active);
        let reply_now = Arc::clone(&reply_now);
        async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let started = started.clone();
                let active = Arc::clone(&active);
                let reply_now = Arc::clone(&reply_now);
                tokio::spawn(async move {
                    let mut bytes = [0; 4096];
                    let count = socket.read(&mut bytes).await.unwrap_or(0);
                    if count == 0 {
                        return;
                    }
                    active.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    if reply_now.load(Ordering::SeqCst) {
                        // Read the complete request before closing the socket. Closing
                        // with unread body bytes can reset the connection on Linux.
                        let mut request = bytes[..count].to_vec();
                        let header_end = loop {
                            if let Some(offset) =
                                request.windows(4).position(|part| part == b"\r\n\r\n")
                            {
                                break offset + 4;
                            }
                            let count = socket.read(&mut bytes).await.unwrap_or(0);
                            assert_ne!(count, 0, "incomplete test HTTP request");
                            request.extend_from_slice(&bytes[..count]);
                        };
                        let headers = String::from_utf8_lossy(&request[..header_end]);
                        let body_len = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        while request.len() < header_end + body_len {
                            let count = socket.read(&mut bytes).await.unwrap_or(0);
                            assert_ne!(count, 0, "incomplete test HTTP body");
                            request.extend_from_slice(&bytes[..count]);
                        }
                        let body = br#"{"hostname":"ready"}"#;
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(header.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    } else {
                        while socket.read(&mut bytes).await.unwrap_or(0) != 0 {}
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        }
    });
    let mut hub = Hub::new(base, TOKEN);
    hub.timeout = Duration::from_secs(15);
    let bridge = bridge(Some(hub), &[], &[]);
    let session = protectorate_mcpbridge::Session::new();
    session.mark_initialized();

    // More than the bridge's 256-call limit must finish before the hub's
    // 15-second deadline, even though the server never sends a reply.
    for wave in 0..5 {
        let mut calls = Vec::new();
        for index in 0..64 {
            let id = json!(wave * 64 + index);
            let guard = session.begin(&id).unwrap();
            let bridge = Arc::clone(&bridge);
            let session = session.clone();
            let request = protectorate_mcpbridge::Request {
                id: Some(id.clone()),
                method: "tools/call".to_owned(),
                params: json!({"name":"hub_get_status","arguments":{}}),
            };
            calls.push((
                id,
                tokio::task::spawn_blocking(move || {
                    bridge.dispatch(&session, &request, &guard.cancelled)
                }),
            ));
        }
        for _ in 0..calls.len() {
            tokio::time::timeout(Duration::from_secs(3), observed.recv())
                .await
                .expect("hub call did not start")
                .expect("server stopped");
        }
        for (id, _) in &calls {
            session.cancel(id);
        }
        for (_, call) in calls {
            let reply = tokio::time::timeout(Duration::from_secs(2), call)
                .await
                .expect("cancelled request retained its worker")
                .unwrap();
            assert_eq!(tool_text(&reply), "canceled: request cancelled");
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("cancelled requests retained sockets");
    }

    reply_now.store(true, Ordering::SeqCst);
    let guard = session.begin(&json!("live")).unwrap();
    let request = protectorate_mcpbridge::Request {
        id: Some(json!("live")),
        method: "tools/call".to_owned(),
        params: json!({"name":"hub_get_status","arguments":{}}),
    };
    let reply = tokio::task::spawn_blocking({
        let bridge = Arc::clone(&bridge);
        let session = session.clone();
        move || bridge.dispatch(&session, &request, &guard.cancelled)
    })
    .await
    .unwrap();
    let response_text = tool_text(&reply);
    let body: Value = serde_json::from_str(&response_text).unwrap_or_else(|error| {
        panic!("live hub reply was not JSON: {error}; {response_text}; {reply}")
    });
    assert_eq!(body["hostname"], "ready");
    server.abort();
}

#[tokio::test]
async fn the_2026_07_28_meta_mode_replaces_the_handshake_and_decorates_results() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(Some(Hub::new(base, TOKEN)), &[], &[]);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name":"modern","version":"1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let replies = exchange_async(
        &bridge,
        &[
            initialize(0),
            json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":meta}}),
            json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"_meta":meta}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{
                "_meta":json!({"io.modelcontextprotocol/protocolVersion":"2025-11-25"})
            }}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{
                "_meta":json!({
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{"name":"modern","version":"1"},
                    "io.modelcontextprotocol/clientCapabilities":{}
                })
            }}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
                "name":"hub_get_status","arguments":{},
                "_meta":json!({
                    "io.modelcontextprotocol/protocolVersion":"2099-01-01",
                    "io.modelcontextprotocol/clientInfo":{},
                    "io.modelcontextprotocol/clientCapabilities":{}
                })
            }}),
        ],
    )
    .await;
    task.abort();

    let discovered = &replies[1]["result"];
    assert_eq!(discovered["supportedVersions"], json!(PROTOCOL_VERSIONS));
    assert_eq!(discovered["resultType"], "complete");
    assert_eq!(discovered["ttlMs"], 0);
    assert_eq!(discovered["cacheScope"], "public");
    assert_eq!(
        discovered["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "protectorate"
    );

    assert_eq!(replies[2]["error"]["code"], -32601);

    // A per-request version below 2026-07-28 stays in the legacy shape.
    assert!(replies[3]["result"].get("resultType").is_none());

    // A modern request needs no handshake of its own.
    let listed = &replies[4]["result"];
    assert_eq!(listed["resultType"], "complete");
    assert_eq!(listed["ttlMs"], 0);
    assert!(listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "quota_get_quota"));

    assert_eq!(replies[5]["error"]["code"], -32022);
    assert_eq!(replies[5]["error"]["data"]["requested"], "2099-01-01");
    assert_eq!(
        replies[5]["error"]["data"]["supported"],
        json!(PROTOCOL_VERSIONS)
    );
}

#[test]
fn without_a_hub_the_catalog_still_lists_and_calls_fail() {
    let bridge = bridge(None, &["session_kill_session"], &[]);
    assert!(names(&bridge)
        .iter()
        .any(|name| name == "session_kill_session"));
    assert!(bridge
        .tools()
        .iter()
        .find(|tool| tool["name"] == "session_kill_session")
        .and_then(|tool| tool["description"].as_str())
        .is_some_and(|description| !description.is_empty()));
    let replies = exchange(
        &bridge,
        &[
            initialize(1),
            call(2, "session_kill_session", json!({"sleeve":"x"})),
        ],
    );
    assert_eq!(replies[1]["result"]["isError"], true);
    assert_eq!(
        tool_text(&replies[1]),
        "unavailable: no hub endpoint is configured"
    );
}

#[test]
fn a_frame_over_the_limit_ends_the_session() {
    let bridge = bridge(None, &[], &[]);
    let huge =
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"pad":"x".repeat(17 << 20)}})
            .to_string();
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let error = Arc::clone(&bridge)
        .run(BufReader::new(Cursor::new(huge + "\n")), &output)
        .unwrap_err();
    assert!(error.to_string().contains("message exceeds 16777216 bytes"));
    assert!(output.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_hub_token_is_required_by_the_hub() {
    let (base, task) = fake_hub().await;
    let bridge = bridge(
        Some(Hub {
            url: base,
            token: "wrong".to_owned(),
            timeout: Duration::from_secs(5),
        }),
        &[],
        &[],
    );
    let replies = exchange_async(
        &bridge,
        &[initialize(1), call(2, "hub_get_status", json!({}))],
    )
    .await;
    task.abort();
    assert_eq!(replies[1]["result"]["isError"], true);
    assert_eq!(tool_text(&replies[1]), "unauthenticated: bad bearer");
}
