//! `protectorate mcp --envoy`: a stdio MCP server (JSON-RPC 2.0, one message per line) for
//! vendor general agents that prefer MCP over the CLI (D9). Three tools, each one envoy-only
//! API call with the caller's token: `envoy_send`, `envoy_wait` and `envoy_read`.
//!
//! Tool calls run on their own threads, so a long `envoy_wait` does not hold up `ping`; every
//! other message is handled in order on the reader. Responses are written whole, one per line,
//! under a lock.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use pt_api::{self as api, method as m};
use pt_json::{Value, obj};

use crate::cli::entry_line;
use crate::client::Client;
use crate::fail::Fail;

/// Newest first; an unknown version requested by the client gets the first.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const WAIT_DEFAULT_SECS: u64 = 60;
const WAIT_MAX_SECS: u64 = 3600;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct Server {
    client: Client,
    /// The `agent:<name>` tag on messages; from `--as`, else the client's own name.
    agent: Mutex<String>,
    initialized: Mutex<bool>,
    out: Mutex<io::Stdout>,
}

fn tools() -> Value {
    let tool = |name: &str, description: &str, props: Value, required: Vec<Value>| {
        obj! {
            "name" => name,
            "description" => description,
            "inputSchema" => obj! {
                "type" => "object",
                "properties" => props,
                "required" => required,
                "additionalProperties" => false,
            },
        }
    };
    Value::Array(vec![
        tool(
            "envoy_send",
            "Send a message to Envoy, the owner's orchestrator for coding work on this machine. \
             Returns a request_id; call envoy_wait with it for Envoy's reply.",
            obj! { "text" => obj! { "type" => "string", "description" => "The message." } },
            vec!["text".into()],
        ),
        tool(
            "envoy_wait",
            "Wait for Envoy's reply to a message. Without request_id, waits on your latest \
             message. If the reply is not settled when the timeout passes, call again.",
            obj! {
                "request_id" => obj! { "type" => "string" },
                "timeout_secs" => obj! {
                    "type" => "integer",
                    "minimum" => 1,
                    "maximum" => WAIT_MAX_SECS,
                    "description" => "Default 60.",
                },
            },
            Vec::new(),
        ),
        tool(
            "envoy_read",
            "Read Envoy's transcript (messages, replies, tool calls) after a seq.",
            obj! {
                "after_seq" => obj! {
                    "type" => "integer",
                    "minimum" => 0,
                    "description" => "Entries with a greater seq; default 0, the start.",
                },
            },
            Vec::new(),
        ),
    ])
}

fn agent_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    if n.is_empty() { "agent".into() } else { n }
}

fn fail_text(e: Fail) -> String {
    match e {
        Fail::Api(e) => e.to_string(),
        Fail::Usage(s) | Fail::Other(s) => s,
        Fail::Exit(code) => format!("job exited with code {code}"),
    }
}

impl Server {
    fn write(&self, msg: &Value) {
        let mut out = lock(&self.out);
        let _ = writeln!(out, "{msg}");
        let _ = out.flush();
    }

    fn reply(&self, id: &Value, result: Result<Value, (i64, String)>) {
        let msg = match result {
            Ok(r) => obj! { "jsonrpc" => "2.0", "id" => id.clone(), "result" => r },
            Err((code, message)) => obj! {
                "jsonrpc" => "2.0",
                "id" => id.clone(),
                "error" => obj! { "code" => code, "message" => message },
            },
        };
        self.write(&msg);
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params["protocolVersion"].as_str().unwrap_or("");
        let version = VERSIONS
            .iter()
            .find(|v| **v == asked)
            .copied()
            .unwrap_or(VERSIONS[0]);
        if let Some(name) = params["clientInfo"]["name"].as_str() {
            let mut agent = lock(&self.agent);
            if agent.is_empty() {
                *agent = agent_name(name);
            }
        }
        *lock(&self.initialized) = true;
        obj! {
            "protocolVersion" => version,
            "capabilities" => obj! { "tools" => obj! { "listChanged" => false } },
            "serverInfo" => obj! {
                "name" => "protectorate-envoy",
                "version" => option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or("dev"),
            },
            "instructions" => "Talk to Envoy, the owner's orchestrator: envoy_send a request, \
                envoy_wait for the reply, envoy_read for the whole conversation. Envoy asks the \
                owner before big, heavy or dangerous work.",
        }
    }

    /// A tool's text, or the hub's error as a tool error the model can read.
    fn call_tool(&self, name: &str, args: &Value) -> Result<(String, bool), (i64, String)> {
        let text = |r: Result<String, Fail>| match r {
            Ok(t) => (t, false),
            Err(e) => (fail_text(e), true),
        };
        match name {
            "envoy_send" => {
                let Some(t) = args["text"].as_str().filter(|t| !t.is_empty()) else {
                    return Err((INVALID_PARAMS, "envoy_send needs text".into()));
                };
                let agent = lock(&self.agent).clone();
                let req = api::EnvoySendRequest {
                    text: t.to_string(),
                    agent: Some(if agent.is_empty() {
                        "agent".into()
                    } else {
                        agent
                    }),
                };
                Ok(text(self.client.call::<m::EnvoySend>(&req).map(|v| {
                    format!(
                        "request_id: {}\nCall envoy_wait for Envoy's reply.",
                        v.request_id
                    )
                })))
            }
            "envoy_wait" => {
                let secs = args["timeout_secs"]
                    .as_u64()
                    .unwrap_or(WAIT_DEFAULT_SECS)
                    .clamp(1, WAIT_MAX_SECS);
                let req = api::EnvoyWaitRequest {
                    request_id: args["request_id"].as_str().map(str::to_string),
                    timeout_ms: secs * 1000,
                };
                let r = self
                    .client
                    .call_for::<m::EnvoyWait>(&req, Duration::from_secs(secs));
                Ok(text(r.map(|v| {
                    if v.settled {
                        v.text
                    } else {
                        format!(
                            "Envoy has not finished replying to {} yet; call envoy_wait again.\n{}",
                            v.request_id, v.text
                        )
                    }
                })))
            }
            "envoy_read" => {
                let req = api::EnvoyReadRequest {
                    from_seq: args["after_seq"].as_u64().map_or(0, |s| s + 1),
                    limit: None,
                };
                Ok(text(self.client.call::<m::EnvoyRead>(&req).map(|t| {
                    let lines: Vec<String> = t.entries.iter().map(entry_line).collect();
                    if lines.is_empty() {
                        "(no entries)".into()
                    } else {
                        lines.join("\n")
                    }
                })))
            }
            other => Err((INVALID_PARAMS, format!("no tool {other:?}"))),
        }
    }

    fn handle(&self, msg: Value) {
        let id = msg["id"].clone();
        let method = msg["method"].as_str().unwrap_or("").to_string();
        let params = &msg["params"];
        let is_request = msg.as_object().is_some_and(|o| o.get("id").is_some());
        if !is_request {
            // Notifications: initialized, cancelled. A cancelled call still answers when done.
            return;
        }
        if msg["jsonrpc"].as_str() != Some("2.0") || method.is_empty() {
            return self.reply(
                &id,
                Err((INVALID_REQUEST, "not a JSON-RPC 2.0 request".into())),
            );
        }
        let ready = *lock(&self.initialized);
        let result = match method.as_str() {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(obj! {}),
            _ if !ready => Err((INVALID_REQUEST, "initialize first".into())),
            "tools/list" => Ok(obj! { "tools" => tools() }),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = match &params["arguments"] {
                    Value::Null => obj! {},
                    v => v.clone(),
                };
                self.call_tool(name, &args).map(|(text, is_error)| {
                    obj! {
                        "content" => vec![obj! { "type" => "text", "text" => text }],
                        "isError" => is_error,
                    }
                })
            }
            other => Err((METHOD_NOT_FOUND, format!("no method {other}"))),
        };
        self.reply(&id, result);
    }
}

pub fn serve(client: &Client, agent: Option<&str>) -> Result<(), String> {
    let server = Arc::new(Server {
        client: Client {
            socket: client.socket.clone(),
            token: client.token.clone(),
        },
        agent: Mutex::new(agent.map(agent_name).unwrap_or_default()),
        initialized: Mutex::new(false),
        out: Mutex::new(io::stdout()),
    });
    let mut workers = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| format!("read stdin: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let msg = match pt_json::parse(&line) {
            Ok(v) if v.as_object().is_some() => v,
            Ok(_) => {
                server.reply(
                    &Value::Null,
                    Err((
                        INVALID_REQUEST,
                        "one JSON-RPC object per line; no batches".into(),
                    )),
                );
                continue;
            }
            Err(e) => {
                server.reply(
                    &Value::Null,
                    Err((PARSE_ERROR, format!("parse error: {e}"))),
                );
                continue;
            }
        };
        if msg["method"].as_str() == Some("tools/call") {
            let s = server.clone();
            workers.push(thread::spawn(move || s.handle(msg)));
            workers.retain(|w| !w.is_finished());
        } else {
            server.handle(msg);
        }
    }
    // stdin closed: answer what is in flight, then exit.
    for w in workers {
        let _ = w.join();
    }
    Ok(())
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
