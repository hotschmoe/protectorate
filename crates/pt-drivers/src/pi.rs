//! `pi --mode rpc` JSONL. Provider and model come from Pi's own `--provider`/`--model`
//! flags (a local endpoint is just a provider in the owner's models.json). The prompt ack is
//! the `prompt` response; a turn ends at `agent_settled`, not `agent_end`.

use std::collections::HashMap;

use pt_json::{Value, obj};

use crate::emit::{Out, bounded_output, content_text, opt_str};
use crate::event::{
    AckKind, Decision, Entry, ErrorClass, Harness, Stop, ToolKind, ToolStatus, TouchedPath, Usage,
    classify,
};
use crate::{DriverError, Protocol, SpawnSpec};

pub(crate) struct Pi {
    resume: Option<String>,
    next_id: u64,
    pending: HashMap<String, Rpc>,
    ready: bool,
    turn: Option<Turn>,
    msg_n: u64,
    context_window: Option<u64>,
}

enum Rpc {
    State,
    Prompt(String),
    Steer(String),
    Abort,
}

#[derive(Default)]
struct Turn {
    request_id: Option<String>,
    started: bool,
    errored: bool,
    usage: Usage,
    cost: f64,
}

impl Pi {
    pub(crate) fn new(spec: &SpawnSpec) -> Pi {
        Pi {
            resume: spec.resume.clone(),
            next_id: 0,
            pending: HashMap::new(),
            ready: false,
            turn: None,
            msg_n: 0,
            context_window: None,
        }
    }

    pub(crate) fn argv(&self, spec: &SpawnSpec, new_id: Option<&str>) -> Vec<String> {
        let mut a = vec!["--mode".to_owned(), "rpc".to_owned()];
        if let Some(provider) = &spec.provider {
            a.extend(["--provider".into(), provider.clone()]);
        }
        if let Some(model) = &spec.model {
            a.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &spec.effort {
            a.extend(["--thinking".into(), effort.clone()]);
        }
        match (&self.resume, new_id) {
            (Some(id), _) => a.extend(["--session".into(), id.clone()]),
            (None, Some(id)) => a.extend(["--session-id".into(), id.to_owned()]),
            (None, None) => {}
        }
        a.extend(spec.extra_args.iter().cloned());
        a
    }

    fn command(&mut self, kind: &str, mut fields: Value, rpc: Rpc, out: &mut Out) {
        self.next_id += 1;
        let id = format!("pt-{}", self.next_id);
        if let Some(map) = fields.as_object_mut() {
            map.insert("id", id.as_str());
            map.insert("type", kind);
        }
        self.pending.insert(id, rpc);
        out.write(fields);
    }

    fn on_response(&mut self, msg: &Value, out: &mut Out) {
        let command = msg["command"].as_str().unwrap_or_default();
        let ok = msg["success"].as_bool() == Some(true);
        let error = msg["error"].as_str().unwrap_or("failed").to_owned();
        let Some(rpc) = msg["id"].as_str().and_then(|id| self.pending.remove(id)) else {
            if command == "parse" {
                out.error(
                    ErrorClass::Protocol,
                    format!("pi could not parse a command: {error}"),
                );
            } else {
                out.error(
                    ErrorClass::Protocol,
                    format!("pi responded to unknown command {} ({command})", msg["id"]),
                );
            }
            return;
        };
        let data = &msg["data"];
        match rpc {
            Rpc::State if !ok => {
                out.error(classify(&error, None), format!("pi get_state: {error}"))
            }
            Rpc::State => {
                let Some(id) = opt_str(&data["sessionId"]) else {
                    out.error(ErrorClass::Protocol, "pi get_state gave no session id");
                    return;
                };
                if let Some(want) = &self.resume
                    && !id.starts_with(want.as_str())
                {
                    out.error(
                        ErrorClass::Protocol,
                        format!("pi opened session {id}, expected {want}"),
                    );
                    return;
                }
                self.ready = true;
                let model = &data["model"];
                self.context_window = model["contextWindow"].as_u64();
                out.emit(Entry::SessionInfo {
                    harness: Harness::Pi,
                    native_id: id,
                    model: opt_str(&model["id"]),
                    provider: opt_str(&model["provider"]),
                    effort: opt_str(&data["thinkingLevel"]),
                    cwd: None,
                });
            }
            Rpc::Prompt(request_id) | Rpc::Steer(request_id) => {
                let disposition = opt_str(&data["disposition"]);
                out.emit(Entry::PromptAck {
                    request_id: request_id.clone(),
                    ack: if ok {
                        AckKind::Confirmed
                    } else {
                        AckKind::Rejected
                    },
                    native_turn_id: None,
                    detail: if ok { disposition.clone() } else { Some(error) },
                });
                let ours = self
                    .turn
                    .as_ref()
                    .is_some_and(|t| t.request_id.as_deref() == Some(&request_id) && !t.started);
                if ours && (!ok || disposition.as_deref() == Some("handled")) {
                    // Rejected before acceptance, or consumed without a run: no agent_settled follows.
                    self.turn = None;
                    if ok {
                        out.emit(Entry::TurnSettled {
                            stop: Stop::Other,
                            native_stop: disposition,
                            usage: Usage::default(),
                            duration_ms: None,
                        });
                    }
                }
            }
            Rpc::Abort if !ok => out.notice("interrupt", format!("pi abort: {error}")),
            Rpc::Abort => {}
        }
    }

    fn turn(&mut self) -> &mut Turn {
        self.turn.get_or_insert_with(Turn::default)
    }

    fn on_message_update(&mut self, msg: &Value, out: &mut Out) {
        let e = &msg["assistantMessageEvent"];
        let id = format!("pi-{}:{}", self.msg_n, e["contentIndex"]);
        match e["type"].as_str().unwrap_or_default() {
            "text_delta" => out.delta(false, &id, e["delta"].as_str().unwrap_or("")),
            "thinking_delta" => out.delta(true, &id, e["delta"].as_str().unwrap_or("")),
            "text_end" => out.finish(false, &id, e["content"].as_str().unwrap_or("")),
            "thinking_end" => out.finish(true, &id, e["content"].as_str().unwrap_or("")),
            _ => {}
        }
    }

    fn on_message_end(&mut self, msg: &Value, out: &mut Out) {
        let m = &msg["message"];
        if m["role"].as_str() != Some("assistant") {
            return;
        }
        out.flush();
        let u = &m["usage"];
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        let context = n("input") + n("cacheRead") + n("cacheWrite");
        let window = self.context_window;
        let turn = self.turn();
        turn.usage.input += n("input");
        turn.usage.output += n("output");
        turn.usage.cache_read += n("cacheRead");
        turn.usage.cache_write += n("cacheWrite");
        turn.cost += u["cost"]["total"].as_f64().unwrap_or(0.0);
        if context > 0 {
            turn.usage.context_tokens = Some(context);
        }
        turn.usage.context_window = window;
        if m["stopReason"].as_str() == Some("error") {
            turn.errored = true;
            let text = m["errorMessage"].as_str().unwrap_or("pi model call failed");
            out.error(classify(text, None), text);
        }
    }

    fn on_settled(&mut self, msg: &Value, out: &mut Out) {
        let turn = self.turn.take().unwrap_or_default();
        let aborted = msg["aborted"].as_bool() == Some(true);
        let stop = if aborted {
            Stop::Cancelled
        } else if turn.errored {
            Stop::Error
        } else {
            Stop::EndTurn
        };
        let mut usage = turn.usage;
        if turn.cost > 0.0 {
            usage.cost_usd = Some(turn.cost);
        }
        out.emit(Entry::TurnSettled {
            stop,
            native_stop: Some(if aborted { "aborted" } else { "settled" }.into()),
            usage,
            duration_ms: None,
        });
    }

    fn on_extension_ui(&mut self, msg: &Value, out: &mut Out) {
        let method = msg["method"].as_str().unwrap_or_default();
        let title = msg["title"].as_str().unwrap_or_default();
        match method {
            "select" | "confirm" | "input" | "editor" => {
                // No hub UI answers extension dialogs yet; cancel so Pi does not wait.
                out.write(obj! {
                    "type" => "extension_ui_response",
                    "id" => msg["id"].clone(),
                    "cancelled" => true,
                });
                out.notice(
                    "extension_ui",
                    format!("cancelled pi extension {method} dialog: {title}"),
                );
            }
            "notify" => out.notice(
                "extension_notify",
                msg["message"].as_str().unwrap_or(title).to_owned(),
            ),
            _ => {}
        }
    }
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "bash" => ToolKind::Command,
        "read" => ToolKind::Read,
        "edit" | "write" => ToolKind::Edit,
        "grep" | "find" | "ls" => ToolKind::Search,
        n if n.starts_with("mcp") => ToolKind::Mcp,
        _ => ToolKind::Other,
    }
}

impl Protocol for Pi {
    fn start(&mut self, out: &mut Out) {
        self.command("get_state", obj! {}, Rpc::State, out);
    }

    fn on_message(&mut self, msg: Value, out: &mut Out) {
        match msg["type"].as_str().unwrap_or_default() {
            "response" => self.on_response(&msg, out),
            "agent_start" => {
                let turn = self.turn();
                if !turn.started {
                    turn.started = true;
                    let request_id = turn.request_id.clone();
                    out.emit(Entry::TurnStarted {
                        request_id,
                        native_turn_id: None,
                    });
                }
            }
            "message_start" => {
                if msg["message"]["role"].as_str() == Some("assistant") {
                    self.msg_n += 1;
                }
            }
            "message_update" => self.on_message_update(&msg, out),
            "message_end" => self.on_message_end(&msg, out),
            "tool_execution_start" => {
                let id = msg["toolCallId"].as_str().unwrap_or_default();
                let name = msg["toolName"].as_str().unwrap_or_default();
                let args = &msg["args"];
                out.tool_call(id, tool_kind(name), name, args);
                if matches!(name, "edit" | "write")
                    && let Some(path) = opt_str(&args["path"])
                {
                    out.emit(Entry::FileChange {
                        tool_id: Some(id.to_owned()),
                        paths: vec![TouchedPath {
                            path,
                            op: Some(name.to_owned()),
                        }],
                    });
                }
            }
            "tool_execution_end" => {
                let (output, truncated) = bounded_output(&content_text(&msg["result"]["content"]));
                out.emit(Entry::ToolResult {
                    id: msg["toolCallId"].as_str().unwrap_or_default().to_owned(),
                    status: if msg["isError"].as_bool() == Some(true) {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    },
                    output,
                    truncated,
                    duration_ms: msg["durationMs"].as_u64(),
                });
            }
            "agent_settled" => self.on_settled(&msg, out),
            "compaction_end" => {
                let r = &msg["result"];
                if r.as_object().is_some() {
                    out.emit(Entry::Compaction {
                        trigger: opt_str(&msg["reason"]),
                        tokens_before: r["tokensBefore"].as_u64(),
                        tokens_after: r["estimatedTokensAfter"].as_u64(),
                    });
                } else if let Some(error) = msg["errorMessage"].as_str() {
                    out.notice("compaction", format!("pi compaction failed: {error}"));
                }
            }
            "auto_retry_start" => out.notice(
                "retry",
                msg["errorMessage"]
                    .as_str()
                    .unwrap_or("retrying")
                    .to_owned(),
            ),
            "auto_retry_end" => {
                if msg["success"].as_bool() == Some(false) {
                    let text = msg["finalError"].as_str().unwrap_or("pi retries exhausted");
                    self.turn().errored = true;
                    out.error(classify(text, None), text);
                }
            }
            "extension_error" => out.notice(
                "extension_error",
                format!(
                    "{}: {}",
                    msg["extensionPath"].as_str().unwrap_or("extension"),
                    msg["error"].as_str().unwrap_or("")
                ),
            ),
            "extension_ui_request" => self.on_extension_ui(&msg, out),
            "agent_end"
            | "turn_start"
            | "turn_end"
            | "tool_execution_update"
            | "queue_update"
            | "entry_appended"
            | "session_info_changed"
            | "thinking_level_changed"
            | "compaction_start"
            | "summarization_retry_scheduled"
            | "summarization_retry_attempt_start"
            | "summarization_retry_finished"
            | "bash_execution_update" => {}
            other => out.unmapped(other),
        }
    }

    fn send_user(
        &mut self,
        request_id: &str,
        text: &str,
        steer: bool,
        out: &mut Out,
    ) -> Result<(), DriverError> {
        if !self.ready {
            return Err(DriverError::NotReady);
        }
        if steer && self.turn.is_some() {
            self.command(
                "steer",
                obj! { "message" => text },
                Rpc::Steer(request_id.to_owned()),
                out,
            );
            return Ok(());
        }
        if self.turn.is_some() {
            return Err(DriverError::Busy);
        }
        self.command(
            "prompt",
            obj! { "message" => text },
            Rpc::Prompt(request_id.to_owned()),
            out,
        );
        self.turn = Some(Turn {
            request_id: Some(request_id.to_owned()),
            ..Turn::default()
        });
        Ok(())
    }

    fn respond_approval(
        &mut self,
        id: &str,
        _decision: Decision,
        _out: &mut Out,
    ) -> Result<(), DriverError> {
        Err(DriverError::UnknownApproval(id.to_owned()))
    }

    fn interrupt(&mut self, out: &mut Out) -> Result<(), DriverError> {
        if self.turn.is_some() {
            self.command("abort", obj! {}, Rpc::Abort, out);
        }
        Ok(())
    }
}
