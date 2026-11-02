//! Grok Build over ACP: `grok agent [--always-approve] --no-leader stdio`.
//!
//! The prompt ack is Grok's own `_x.ai/queue/changed` naming the running prompt, or the
//! first session update after the prompt. Updates replayed by `session/load` arrive before its
//! reply and are dropped, and so are notices naming a prompt that already ended.

use std::collections::{HashMap, HashSet, VecDeque};

use pt_json::{Value, obj};

use crate::emit::{Out, bounded_output, content_text, id_key, opt_str};
use crate::error::DriverError;
use crate::event::{
    AckKind, Decision, Entry, ErrorClass, Harness, Stop, ToolKind, ToolStatus, TouchedPath, Usage,
    classify,
};
use crate::protocol::Protocol;
use crate::spec::{Permission, SpawnSpec};

pub(crate) struct Grok {
    cwd: String,
    resume: Option<String>,
    supervised: bool,
    effort: Option<String>,
    next_id: u64,
    pending: HashMap<u64, Rpc>,
    session_id: Option<String>,
    turn: Option<Turn>,
    /// Grok's ids of recently ended prompts.
    finished: VecDeque<String>,
    turns: u64,
    segment: u64,
    changed_tools: HashSet<String>,
    approvals: HashMap<String, Approval>,
    finished_tools: HashSet<String>,
}

enum Rpc {
    Initialize,
    Open(&'static str),
    Prompt(String),
}

struct Turn {
    request_id: String,
    acked: bool,
    /// Grok's prompt id, once a notice named it.
    native: Option<String>,
}

struct Approval {
    rpc_id: Value,
    options: Vec<(String, String)>,
}

impl Grok {
    pub(crate) fn new(spec: &SpawnSpec, cwd: String) -> Grok {
        Grok {
            cwd,
            resume: spec.resume.clone(),
            supervised: spec.permission == Permission::Supervised,
            effort: spec.effort.clone(),
            next_id: 0,
            pending: HashMap::new(),
            session_id: None,
            turn: None,
            finished: VecDeque::new(),
            turns: 0,
            segment: 0,
            changed_tools: HashSet::new(),
            approvals: HashMap::new(),
            finished_tools: HashSet::new(),
        }
    }

    pub(crate) fn argv(&self, spec: &SpawnSpec) -> Vec<String> {
        let mut a = vec!["agent".to_owned()];
        if let Some(model) = &spec.model {
            a.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &spec.effort {
            a.extend(["--reasoning-effort".into(), effort.clone()]);
        }
        if !self.supervised {
            a.push("--always-approve".into());
        }
        a.push("--no-leader".into());
        a.extend(spec.extra_args.iter().cloned());
        a.push("stdio".into());
        a
    }

    fn call(&mut self, method: &str, params: Value, rpc: Rpc, out: &mut Out) {
        self.next_id += 1;
        self.pending.insert(self.next_id, rpc);
        out.write(obj! {
            "jsonrpc" => "2.0",
            "id" => self.next_id,
            "method" => method,
            "params" => params,
        });
    }

    /// A notice names Grok prompt `id`: true when that is not the running prompt.
    fn stale(&self, id: &str) -> bool {
        self.finished.iter().any(|f| f == id)
            || self
                .turn
                .as_ref()
                .and_then(|t| t.native.as_deref())
                .is_some_and(|n| n != id)
    }

    fn ack(&mut self, native: Option<String>, out: &mut Out) {
        let Some(turn) = self.turn.as_mut().filter(|t| !t.acked) else {
            return;
        };
        turn.acked = true;
        if native.is_some() {
            turn.native.clone_from(&native);
        }
        let request_id = turn.request_id.clone();
        out.emit(Entry::PromptAck {
            request_id: request_id.clone(),
            ack: AckKind::Confirmed,
            native_turn_id: native.clone(),
            detail: None,
        });
        out.emit(Entry::TurnStarted {
            request_id: Some(request_id),
            native_turn_id: native,
        });
    }

    fn on_reply(&mut self, msg: &Value, out: &mut Out) {
        // Our ids are numbers. Grok 1.0.70 also writes the reply to a request of its own,
        // `"id": "skills-reload"`, right after it starts: noted, not a desync.
        let Some(id) = msg["id"].as_u64() else {
            return out.unmapped(&format!("reply to grok's own request {}", msg["id"]));
        };
        let Some(rpc) = self.pending.remove(&id) else {
            out.error(
                ErrorClass::Protocol,
                format!("grok replied to unknown request {}", msg["id"]),
            );
            return;
        };
        let error = &msg["error"];
        let failed = !error.is_null();
        if failed == msg.get("result").is_some() {
            out.error(
                ErrorClass::Protocol,
                format!("grok reply to request {} is malformed", msg["id"]),
            );
            return;
        }
        let message = format!(
            "{} (code {})",
            error["message"].as_str().unwrap_or("request failed"),
            error["code"]
        );
        let result = &msg["result"];
        match rpc {
            Rpc::Initialize if failed => out.refuse(
                classify(&message, None),
                format!("grok initialize: {message}"),
            ),
            Rpc::Initialize => {
                if result["protocolVersion"].as_u64() != Some(1) {
                    out.error(
                        ErrorClass::Protocol,
                        format!("grok speaks ACP version {}", result["protocolVersion"]),
                    );
                    return;
                }
                let (method, params) = match &self.resume {
                    Some(id) => (
                        "session/load",
                        obj! { "sessionId" => id.as_str(), "cwd" => self.cwd.as_str(), "mcpServers" => Value::from(Vec::new()) },
                    ),
                    None => (
                        "session/new",
                        obj! { "cwd" => self.cwd.as_str(), "mcpServers" => Value::from(Vec::new()) },
                    ),
                };
                self.call(method, params, Rpc::Open(method), out);
            }
            Rpc::Open(method) if failed => out.refuse(
                classify(&message, None),
                format!("grok {method}: {message}"),
            ),
            Rpc::Open(method) => {
                let id = opt_str(&result["sessionId"]).or_else(|| self.resume.clone());
                let Some(id) = id else {
                    out.error(
                        ErrorClass::Protocol,
                        format!("grok {method} gave no session id"),
                    );
                    return;
                };
                if let Some(want) = &self.resume
                    && *want != id
                {
                    out.error(
                        ErrorClass::Protocol,
                        format!("grok loaded session {id}, expected {want}"),
                    );
                    return;
                }
                self.session_id = Some(id.clone());
                out.emit(Entry::SessionInfo {
                    harness: Harness::Grok,
                    native_id: id,
                    model: opt_str(&result["models"]["currentModelId"]),
                    provider: Some("xai".into()),
                    effort: self.effort.clone(),
                    cwd: Some(self.cwd.clone()),
                });
            }
            Rpc::Prompt(request_id) => {
                self.on_prompt_reply(request_id, failed, &message, result, out)
            }
        }
    }

    fn on_prompt_reply(
        &mut self,
        request_id: String,
        failed: bool,
        message: &str,
        result: &Value,
        out: &mut Out,
    ) {
        let turn = self
            .turn
            .take_if(|t| t.request_id == request_id)
            .map(|t| t.acked);
        let acked = turn == Some(true);
        self.approvals.clear();
        self.finished_tools.clear();
        self.changed_tools.clear();
        let meta = &result["_meta"];
        let prompt_id = opt_str(&meta["promptId"]);
        if let Some(id) = &prompt_id {
            if self.finished.len() == 16 {
                self.finished.pop_front();
            }
            self.finished.push_back(id.clone());
        }
        let native = opt_str(&result["stopReason"]);
        let malformed = !failed && native.is_none();
        if malformed {
            out.error(
                ErrorClass::Protocol,
                "grok session/prompt reply has no stopReason",
            );
            if !acked {
                // Nothing showed the prompt ran: its effect stays open.
                return;
            }
        }
        if failed && !acked {
            out.emit(Entry::PromptAck {
                request_id,
                ack: AckKind::Rejected,
                native_turn_id: None,
                detail: Some(message.to_owned()),
            });
            out.error(
                classify(message, None),
                format!("grok session/prompt: {message}"),
            );
            return;
        }
        if !acked {
            out.emit(Entry::PromptAck {
                request_id: request_id.clone(),
                ack: AckKind::Confirmed,
                native_turn_id: prompt_id.clone(),
                detail: None,
            });
            out.emit(Entry::TurnStarted {
                request_id: Some(request_id),
                native_turn_id: prompt_id,
            });
        }
        if failed {
            out.error(
                classify(message, None),
                format!("grok session/prompt: {message}"),
            );
        }
        let stop = match native.as_deref() {
            _ if failed || malformed => Stop::Error,
            Some("end_turn") => Stop::EndTurn,
            Some("cancelled") => Stop::Cancelled,
            Some("max_tokens" | "max_turn_requests") => Stop::MaxTurns,
            _ => Stop::Other,
        };
        let meta = &result["_meta"];
        let u = if meta["usage"].as_object().is_some() {
            &meta["usage"]
        } else {
            meta
        };
        let context_tokens = out.count("totalTokens", &meta["totalTokens"]);
        let mut n = |k: &str| out.count(k, &u[k]).unwrap_or(0);
        let usage = Usage {
            input: n("inputTokens").saturating_sub(n("cachedReadTokens")),
            output: n("outputTokens"),
            cache_read: n("cachedReadTokens"),
            cache_write: n("cacheCreationTokens"),
            cost_usd: None,
            context_tokens,
            context_window: None,
        };
        out.emit(Entry::TurnSettled {
            stop,
            native_stop: native,
            usage,
            duration_ms: u["apiDurationMs"].as_u64(),
        });
    }

    fn on_request(&mut self, msg: &Value, method: &str, out: &mut Out) {
        if method != "session/request_permission" {
            out.write(obj! {
                "jsonrpc" => "2.0",
                "id" => msg["id"].clone(),
                "error" => obj! { "code" => -32601, "message" => format!("method not supported: {method}") },
            });
            out.unmapped(method);
            return;
        }
        let p = &msg["params"];
        let tool = &p["toolCall"];
        let options: Vec<(String, String)> = p["options"]
            .as_array()
            .unwrap_or_default()
            .iter()
            .filter_map(|o| Some((opt_str(&o["kind"])?, opt_str(&o["optionId"])?)))
            .collect();
        // Deny picks reject_once or else cancels; reject_always would outlive the call.
        let mut offered = Vec::new();
        for (kind, d) in [
            ("allow_once", Decision::AllowOnce),
            ("allow_always", Decision::AllowSession),
        ] {
            if options.iter().any(|(k, _)| k == kind) {
                offered.push(d);
            }
        }
        offered.extend([Decision::Deny, Decision::Cancel]);
        let Some(key) = id_key(&msg["id"]) else {
            out.error(
                ErrorClass::Protocol,
                format!("grok request {method} has id {}", msg["id"]),
            );
            return;
        };
        let id = format!("grok-{key}");
        if self.approvals.contains_key(&id) {
            out.error(
                ErrorClass::Protocol,
                format!("grok reused pending request id {}", msg["id"]),
            );
            return;
        }
        let auto = if offered.contains(&Decision::AllowOnce) {
            Decision::AllowOnce
        } else {
            Decision::Cancel
        };
        let title = tool["title"].as_str().unwrap_or("");
        let kind = acp_kind(tool);
        let name = if title.is_empty() { kind } else { title };
        let (input, _) = crate::emit::bounded_input(&tool["rawInput"]);
        out.emit(Entry::ApprovalRequest {
            id: id.clone(),
            tool: tool_kind(kind),
            name: name.to_owned(),
            summary: crate::emit::bounded(name, 300).to_owned(),
            input,
            options: offered,
        });
        self.approvals.insert(
            id.clone(),
            Approval {
                rpc_id: msg["id"].clone(),
                options,
            },
        );
        if !self.supervised {
            let _ = self.answer(&id, auto, Some("autonomous"), out);
        }
    }

    fn answer(
        &mut self,
        id: &str,
        decision: Decision,
        by: Option<&str>,
        out: &mut Out,
    ) -> Result<(), DriverError> {
        let approval = self
            .approvals
            .get(id)
            .ok_or_else(|| DriverError::UnknownApproval(id.to_owned()))?;
        let pick = |want: &str| {
            approval
                .options
                .iter()
                .find(|(k, _)| k == want)
                .map(|(_, id)| id.clone())
        };
        // Never wider than asked: allow-once is never answered with allow_always.
        let option = match decision {
            Decision::AllowOnce => Some(pick("allow_once").ok_or(DriverError::Unsupported(
                "grok offered no allow-once option",
            ))?),
            Decision::AllowSession => Some(
                pick("allow_always")
                    .or_else(|| pick("allow_once"))
                    .ok_or(DriverError::Unsupported("grok offered no allow option"))?,
            ),
            Decision::Deny => pick("reject_once"),
            Decision::Cancel => None,
        };
        let Some(approval) = self.approvals.remove(id) else {
            return Err(DriverError::UnknownApproval(id.to_owned()));
        };
        let outcome = match option {
            Some(option) => obj! { "outcome" => "selected", "optionId" => option },
            None => obj! { "outcome" => "cancelled" },
        };
        out.write(obj! {
            "jsonrpc" => "2.0",
            "id" => approval.rpc_id,
            "result" => obj! { "outcome" => outcome },
        });
        out.emit(Entry::ApprovalDecision {
            id: id.to_owned(),
            decision,
            by: by.map(str::to_owned),
            confirmed: false,
        });
        Ok(())
    }

    fn on_note(&mut self, method: &str, p: &Value, out: &mut Out) {
        let Some(ours) = self.session_id.clone() else {
            return;
        };
        if p["sessionId"].as_str().is_some_and(|id| id != ours) {
            return;
        }
        match method {
            "session/update" => {
                let prompt = opt_str(&p["_meta"]["promptId"]);
                if prompt.as_deref().is_some_and(|id| self.stale(id)) {
                    return;
                }
                if let Some(t) = self.turn.as_mut().filter(|t| t.native.is_none()) {
                    t.native.clone_from(&prompt);
                }
                self.on_update(&p["update"], prompt, out);
            }
            "_x.ai/queue/changed" => {
                if let Some(running) = opt_str(&p["runningPromptId"])
                    && !self.stale(&running)
                {
                    self.ack(Some(running), out);
                }
            }
            m if m.starts_with("_x.ai/") => {}
            m => out.unmapped(m),
        }
    }

    fn on_update(&mut self, u: &Value, prompt: Option<String>, out: &mut Out) {
        let kind = u["sessionUpdate"].as_str().unwrap_or_default();
        if matches!(
            kind,
            "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update"
        ) {
            self.ack(prompt, out);
        }
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" => {
                let id = opt_str(&u["messageId"])
                    .unwrap_or_else(|| format!("grok-{}-{}", self.turns, self.segment));
                out.delta(
                    kind == "agent_thought_chunk",
                    &id,
                    u["content"]["text"].as_str().unwrap_or(""),
                );
            }
            "tool_call" => {
                self.segment += 1;
                let id = u["toolCallId"].as_str().unwrap_or_default();
                let acp_kind = acp_kind(u);
                let title = u["title"].as_str().unwrap_or(acp_kind);
                out.tool_call(id, tool_kind(acp_kind), title, &u["rawInput"]);
                self.file_change(id, acp_kind, u, out);
                self.on_tool_status(u, out);
            }
            "tool_call_update" => {
                let id = u["toolCallId"].as_str().unwrap_or_default();
                self.file_change(id, acp_kind(u), u, out);
                self.on_tool_status(u, out);
            }
            "user_message_chunk"
            | "plan"
            | "available_commands_update"
            | "current_mode_update"
            | "config_option_update"
            | "session_info_update"
            | "usage_update" => {}
            other => out.unmapped(&format!("session/update/{other}")),
        }
    }

    /// Paths of an edit, once per tool call; Grok may name them only in a later update.
    fn file_change(&mut self, id: &str, acp_kind: &str, u: &Value, out: &mut Out) {
        if !matches!(acp_kind, "edit" | "delete" | "move") || self.changed_tools.contains(id) {
            return;
        }
        let paths: Vec<TouchedPath> = u["locations"]
            .as_array()
            .unwrap_or_default()
            .iter()
            .filter_map(|l| {
                Some(TouchedPath {
                    path: opt_str(&l["path"])?,
                    op: Some(acp_kind.to_owned()),
                })
            })
            .collect();
        if !paths.is_empty() {
            self.changed_tools.insert(id.to_owned());
            out.emit(Entry::FileChange {
                tool_id: Some(id.to_owned()),
                paths,
            });
        }
    }

    fn on_tool_status(&mut self, u: &Value, out: &mut Out) {
        let status = match u["status"].as_str() {
            Some("completed") => ToolStatus::Completed,
            Some("failed") => ToolStatus::Failed,
            _ => return,
        };
        let id = u["toolCallId"].as_str().unwrap_or_default().to_owned();
        if !self.finished_tools.insert(id.clone()) {
            return;
        }
        let mut text = content_text(&u["content"]);
        if text.is_empty() {
            text = match &u["rawOutput"] {
                Value::Null => String::new(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
        }
        let (output, truncated) = bounded_output(&text);
        out.emit(Entry::ToolResult {
            id,
            status,
            output,
            truncated,
            duration_ms: None,
        });
    }
}

/// ACP's `kind`, or Grok's own tool metadata when the first `tool_call` omits it.
fn acp_kind(u: &Value) -> &str {
    u["kind"]
        .as_str()
        .or_else(|| u["_meta"]["x.ai/tool"]["kind"].as_str())
        .unwrap_or("other")
}

fn tool_kind(acp: &str) -> ToolKind {
    match acp {
        "read" => ToolKind::Read,
        "edit" | "delete" | "move" => ToolKind::Edit,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Command,
        "fetch" => ToolKind::Web,
        _ => ToolKind::Other,
    }
}

impl Protocol for Grok {
    fn start(&mut self, out: &mut Out) {
        self.call(
            "initialize",
            obj! {
                "protocolVersion" => 1,
                "clientCapabilities" => obj! {},
                "clientInfo" => obj! { "name" => "protectorate", "version" => env!("CARGO_PKG_VERSION") },
            },
            Rpc::Initialize,
            out,
        );
    }

    fn on_message(&mut self, msg: Value, out: &mut Out) {
        let has_id = msg.get("id").is_some_and(|id| !id.is_null());
        match (msg["method"].as_str(), has_id) {
            (Some(method), true) => {
                let method = method.to_owned();
                self.on_request(&msg, &method, out);
            }
            (Some(method), false) => {
                let method = method.to_owned();
                self.on_note(&method, &msg["params"], out);
            }
            (None, true) => self.on_reply(&msg, out),
            (None, false) => out.error(
                ErrorClass::Protocol,
                "grok wrote a message with neither method nor id",
            ),
        }
    }

    fn send_user(
        &mut self,
        request_id: &str,
        text: &str,
        steer: Option<&str>,
        out: &mut Out,
    ) -> Result<(), DriverError> {
        let session = self.session_id.clone().ok_or(DriverError::NotReady)?;
        if steer.is_some() {
            return Err(DriverError::Unsupported("Grok cannot steer a running turn"));
        }
        if self.turn.is_some() {
            return Err(DriverError::Busy);
        }
        self.call(
            "session/prompt",
            obj! {
                "sessionId" => session,
                "prompt" => Value::from(vec![obj! { "type" => "text", "text" => text }]),
            },
            Rpc::Prompt(request_id.to_owned()),
            out,
        );
        self.turns += 1;
        self.turn = Some(Turn {
            request_id: request_id.to_owned(),
            acked: false,
            native: None,
        });
        Ok(())
    }

    fn respond_approval(
        &mut self,
        id: &str,
        decision: Decision,
        out: &mut Out,
    ) -> Result<(), DriverError> {
        self.answer(id, decision, None, out)
    }

    fn interrupt(&mut self, out: &mut Out) -> Result<(), DriverError> {
        let (Some(session), true) = (self.session_id.clone(), self.turn.is_some()) else {
            return Ok(());
        };
        let pending: Vec<String> = self.approvals.keys().cloned().collect();
        for id in pending {
            let _ = self.answer(&id, Decision::Cancel, Some("interrupt"), out);
        }
        out.write(obj! {
            "jsonrpc" => "2.0",
            "method" => "session/cancel",
            "params" => obj! { "sessionId" => session },
        });
        Ok(())
    }
}
