//! `pi --mode rpc` JSONL. Provider and model come from Pi's own `--provider`/`--model`
//! flags (a local endpoint is just a provider in the owner's models.json). The prompt ack is
//! the `prompt` response; a turn ends at `agent_settled`, not `agent_end`. Pi has no turn
//! ids, so the driver numbers its turns (`pi-turn-<n>`) for acks and steering.

use std::collections::HashMap;

use pt_json::{Value, obj};

use crate::emit::{Out, add_count, bounded_output, content_text, opt_str};
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
    turns: u64,
    msg_n: u64,
    context_window: Option<u64>,
}

enum Rpc {
    State,
    Prompt(String),
    /// The request and the turn it steers.
    Steer(String, String),
    Abort,
}

impl Rpc {
    fn command(&self) -> &'static str {
        match self {
            Rpc::State => "get_state",
            Rpc::Prompt(_) => "prompt",
            Rpc::Steer(..) => "steer",
            Rpc::Abort => "abort",
        }
    }
}

struct Turn {
    native: String,
    request_id: Option<String>,
    /// Pi answered our prompt, which it does before the run starts.
    answered: bool,
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
            turns: 0,
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

    fn new_turn(&mut self, request_id: Option<String>) -> Turn {
        self.turns += 1;
        Turn {
            native: format!("pi-turn-{}", self.turns),
            request_id,
            answered: false,
            started: false,
            errored: false,
            usage: Usage::default(),
            cost: 0.0,
        }
    }

    fn on_response(&mut self, msg: &Value, out: &mut Out) {
        let command = msg["command"].as_str().unwrap_or_default();
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
        // Only a well-formed answer to this very command decides it; anything else leaves the
        // effect open.
        let Some(ok) = msg["success"]
            .as_bool()
            .filter(|_| command == rpc.command())
        else {
            out.error(
                ErrorClass::Protocol,
                format!(
                    "pi answered {} {} with a malformed {command:?} response",
                    rpc.command(),
                    msg["id"]
                ),
            );
            return;
        };
        let data = &msg["data"];
        let disposition = opt_str(&data["disposition"]);
        let known = match rpc {
            Rpc::Prompt(_) => ["started", "queued", "handled"].as_slice(),
            Rpc::Steer(..) => &["queued", "handled"],
            _ => &[],
        };
        if ok && !known.is_empty() && !disposition.as_deref().is_some_and(|d| known.contains(&d)) {
            out.error(
                ErrorClass::Protocol,
                format!(
                    "pi {command} response has disposition {}",
                    data["disposition"]
                ),
            );
            return;
        }
        match rpc {
            Rpc::State if !ok => {
                out.refuse(classify(&error, None), format!("pi get_state: {error}"))
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
                self.context_window = out.count("contextWindow", &model["contextWindow"]);
                out.emit(Entry::SessionInfo {
                    harness: Harness::Pi,
                    native_id: id,
                    model: opt_str(&model["id"]),
                    provider: opt_str(&model["provider"]),
                    effort: opt_str(&data["thinkingLevel"]),
                    cwd: None,
                });
            }
            Rpc::Steer(request_id, native) => out.emit(Entry::PromptAck {
                request_id,
                ack: if ok {
                    AckKind::Confirmed
                } else {
                    AckKind::Rejected
                },
                native_turn_id: ok.then_some(native),
                detail: if ok { disposition } else { Some(error) },
            }),
            Rpc::Prompt(request_id) => {
                let turn = self
                    .turn
                    .as_mut()
                    .filter(|t| t.request_id.as_deref() == Some(&request_id));
                let native = turn.as_ref().map(|t| t.native.clone());
                let unstarted = turn.as_ref().is_some_and(|t| !t.started);
                if let Some(t) = turn {
                    t.answered = true;
                }
                out.emit(Entry::PromptAck {
                    request_id: request_id.clone(),
                    ack: if ok {
                        AckKind::Confirmed
                    } else {
                        AckKind::Rejected
                    },
                    native_turn_id: native.clone().filter(|_| ok),
                    detail: if ok { disposition.clone() } else { Some(error) },
                });
                if unstarted && (!ok || disposition.as_deref() == Some("handled")) {
                    // Rejected before acceptance, or consumed without a run: no agent_settled
                    // follows. A handled prompt still reports a turn, so it starts and ends.
                    self.turn = None;
                    if ok {
                        out.emit(Entry::TurnStarted {
                            request_id: Some(request_id),
                            native_turn_id: native,
                        });
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
        if self.turn.is_none() {
            self.turn = Some(self.new_turn(None));
        }
        self.turn.as_mut().expect("a turn was just set")
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
        let mut n = |k: &str| out.count(k, &u[k]).unwrap_or(0);
        let (input, output, cache_read, cache_write) =
            (n("input"), n("output"), n("cacheRead"), n("cacheWrite"));
        let cost = out.cost("cost", &u["cost"]["total"]).unwrap_or(0.0);
        let context = input + cache_read + cache_write;
        let window = self.context_window;
        let turn = self.turn();
        let u = &mut turn.usage;
        u.input = add_count(u.input, input);
        u.output = add_count(u.output, output);
        u.cache_read = add_count(u.cache_read, cache_read);
        u.cache_write = add_count(u.cache_write, cache_write);
        turn.cost = (turn.cost + cost).min(1e6);
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
        // Pi answers a prompt before its run starts, so a settle before that answer is not
        // this prompt's.
        if self
            .turn
            .as_ref()
            .is_some_and(|t| t.request_id.is_some() && !t.answered)
        {
            out.notice("stale_settle", "pi settled before answering the prompt");
            return;
        }
        let Some(turn) = self.turn.take() else {
            out.emit(Entry::TurnSettled {
                stop: Stop::Other,
                native_stop: Some("settled".into()),
                usage: Usage::default(),
                duration_ms: None,
            });
            return;
        };
        if !turn.started {
            out.emit(Entry::TurnStarted {
                request_id: turn.request_id.clone(),
                native_turn_id: Some(turn.native.clone()),
            });
        }
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
                    let native = turn.native.clone();
                    out.emit(Entry::TurnStarted {
                        request_id,
                        native_turn_id: Some(native),
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
                    let tokens_before = out.count("tokensBefore", &r["tokensBefore"]);
                    let tokens_after =
                        out.count("estimatedTokensAfter", &r["estimatedTokensAfter"]);
                    out.emit(Entry::Compaction {
                        trigger: opt_str(&msg["reason"]),
                        tokens_before,
                        tokens_after,
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
        steer: Option<&str>,
        out: &mut Out,
    ) -> Result<(), DriverError> {
        if !self.ready {
            return Err(DriverError::NotReady);
        }
        if let Some(expected) = steer {
            // A steer that lost the race with settlement must not become a new prompt.
            if self.turn.as_ref().map(|t| t.native.as_str()) != Some(expected) {
                return Err(DriverError::TurnEnded);
            }
            self.command(
                "steer",
                obj! { "message" => text },
                Rpc::Steer(request_id.to_owned(), expected.to_owned()),
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
        self.turn = Some(self.new_turn(Some(request_id.to_owned())));
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
