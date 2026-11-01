//! Claude Code `-p` stream-json over stdio. The session id is assigned at launch
//! (`--session-id`) or resumed (`--resume`); `--replay-user-messages` echoes each stdin
//! message with the `uuid` we gave it, which is the prompt ack.

use std::collections::{HashMap, HashSet, VecDeque};

use pt_json::{Value, obj};

use crate::emit::{Out, bounded_output, content_text, opt_str};
use crate::event::{
    AckKind, Decision, Entry, ErrorClass, Harness, Stop, ToolKind, ToolStatus, TouchedPath, Usage,
    classify,
};
use crate::{DriverError, Permission, Protocol, SpawnSpec};

pub(crate) struct Claude {
    session_id: String,
    resume: bool,
    supervised: bool,
    model: Option<String>,
    effort: Option<String>,
    cwd: String,
    /// The random prefix of our message uuids; the last group counts messages.
    uuid_base: String,
    sent: u64,
    /// (request id, message uuid) written but not yet echoed back.
    unechoed: VecDeque<(String, String)>,
    /// Uuids echoed or given up on, so a late echo of one is told from a foreign one.
    done: VecDeque<String>,
    turn: Option<Turn>,
    approvals: HashMap<String, Approval>,
    denied: HashSet<String>,
    msg_id: String,
    block: Option<usize>,
    blocks_in_msg: usize,
    context_tokens: Option<u64>,
    next_control: u64,
}

struct Turn {
    /// Our interrupt's control request id, once sent.
    interrupt: Option<String>,
    /// Claude answered that interrupt with success: the result that follows is its result.
    interrupt_accepted: bool,
}

struct Approval {
    input: Value,
    suggestions: Value,
    tool_use_id: Option<String>,
}

impl Claude {
    pub(crate) fn new(session_id: String, uuid: String, spec: &SpawnSpec, cwd: String) -> Claude {
        Claude {
            session_id,
            resume: spec.resume.is_some(),
            supervised: spec.permission == Permission::Supervised,
            model: spec.model.clone(),
            effort: spec.effort.clone(),
            cwd,
            uuid_base: crate::emit::bounded(&uuid, 24).to_owned(),
            sent: 0,
            unechoed: VecDeque::new(),
            done: VecDeque::new(),
            turn: None,
            approvals: HashMap::new(),
            denied: HashSet::new(),
            msg_id: String::new(),
            block: None,
            blocks_in_msg: 0,
            context_tokens: None,
            next_control: 0,
        }
    }

    pub(crate) fn argv(&self, spec: &SpawnSpec) -> Vec<String> {
        let mut a: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--replay-user-messages",
            "--include-partial-messages",
        ]
        .map(String::from)
        .to_vec();
        if let Some(model) = &spec.model {
            a.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &spec.effort {
            a.extend(["--effort".into(), effort.clone()]);
        }
        if self.resume {
            a.extend(["--resume".into(), self.session_id.clone()]);
        } else {
            a.extend(["--session-id".into(), self.session_id.clone()]);
        }
        if self.supervised {
            a.extend(
                [
                    "--permission-prompt-tool",
                    "stdio",
                    "--permission-mode",
                    "manual",
                ]
                .map(String::from),
            );
        } else {
            a.push("--dangerously-skip-permissions".into());
        }
        a.extend(spec.extra_args.iter().cloned());
        a
    }

    fn session_info(&self, out: &mut Out) {
        out.emit(Entry::SessionInfo {
            harness: Harness::Claude,
            native_id: self.session_id.clone(),
            model: self.model.clone(),
            provider: Some("anthropic".into()),
            effort: self.effort.clone(),
            cwd: Some(self.cwd.clone()),
        });
    }

    fn check_session(&self, msg: &Value, out: &mut Out) {
        if let Some(id) = msg["session_id"].as_str()
            && id != self.session_id
        {
            out.error(
                ErrorClass::Protocol,
                format!("claude reported session {id}, expected {}", self.session_id),
            );
        }
    }

    fn on_system(&mut self, msg: &Value, out: &mut Out) {
        let subtype = msg["subtype"].as_str().unwrap_or_default();
        match subtype {
            "init" => {
                self.check_session(msg, out);
                let model = opt_str(&msg["model"]);
                if model.is_some() && model != self.model {
                    self.model = model;
                    self.session_info(out);
                }
            }
            "compact_boundary" => {
                let meta = &msg["compact_metadata"];
                let tokens_before = out.count("pre_tokens", &meta["pre_tokens"]);
                let tokens_after = out.count("post_tokens", &meta["post_tokens"]);
                out.emit(Entry::Compaction {
                    trigger: opt_str(&meta["trigger"]),
                    tokens_before,
                    tokens_after,
                });
            }
            "api_retry" => out.notice(
                "retry",
                format!(
                    "api retry {} of {}: {}",
                    msg["attempt"], msg["max_retries"], msg["error"]
                ),
            ),
            "status"
            | "thinking_tokens"
            | "hook_started"
            | "hook_response"
            | "hook_progress"
            | "task_started"
            | "task_progress"
            | "task_notification"
            | "files_persisted"
            | "local_command_output"
            | "informational" => {}
            other => out.unmapped(&format!("system/{other}")),
        }
    }

    fn on_stream_event(&mut self, msg: &Value, out: &mut Out) {
        let event = &msg["event"];
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let id = event["message"]["id"].as_str().unwrap_or_default();
                if id != self.msg_id {
                    self.msg_id = id.to_owned();
                    self.blocks_in_msg = 0;
                }
            }
            "content_block_start" => self.block = event["index"].as_u64().map(|i| i as usize),
            "content_block_delta" => {
                let id = format!("{}:{}", self.msg_id, event["index"]);
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => out.delta(false, &id, delta["text"].as_str().unwrap_or("")),
                    "thinking_delta" => {
                        out.delta(true, &id, delta["thinking"].as_str().unwrap_or(""))
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn block_id(&mut self, msg_id: &str) -> String {
        if msg_id != self.msg_id {
            self.msg_id = msg_id.to_owned();
            self.blocks_in_msg = 0;
            self.block = None;
        }
        let index = self.block.take().unwrap_or(self.blocks_in_msg);
        self.blocks_in_msg = index.saturating_add(1);
        format!("{msg_id}:{index}")
    }

    fn on_assistant(&mut self, msg: &Value, out: &mut Out) {
        let message = &msg["message"];
        let msg_id = message["id"].as_str().unwrap_or_default().to_owned();
        let usage = &message["usage"];
        if usage.as_object().is_some() {
            let mut n = |k: &str| out.count(k, &usage[k]).unwrap_or(0);
            self.context_tokens = Some(
                n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"),
            );
        }
        let Some(blocks) = message["content"].as_array() else {
            return;
        };
        for block in blocks {
            match block["type"].as_str().unwrap_or_default() {
                "text" => {
                    let id = self.block_id(&msg_id);
                    out.finish(false, &id, block["text"].as_str().unwrap_or(""));
                }
                "thinking" => {
                    let id = self.block_id(&msg_id);
                    out.finish(true, &id, block["thinking"].as_str().unwrap_or(""));
                }
                "tool_use" => {
                    self.block_id(&msg_id);
                    let id = block["id"].as_str().unwrap_or_default();
                    let name = block["name"].as_str().unwrap_or_default();
                    let input = &block["input"];
                    out.tool_call(id, tool_kind(name), name, input);
                    if let Some(path) = edit_path(name, input) {
                        out.emit(Entry::FileChange {
                            tool_id: Some(id.to_owned()),
                            paths: vec![TouchedPath {
                                path,
                                op: Some(name.to_ascii_lowercase()),
                            }],
                        });
                    }
                }
                _ => {
                    self.block_id(&msg_id);
                }
            }
        }
        if let Some(error) = msg["error"].as_str() {
            let text = content_text(&message["content"]);
            out.error(classify(&format!("{error} {text}"), None), text);
        }
    }

    fn on_user(&mut self, msg: &Value, out: &mut Out) {
        let content = &msg["message"]["content"];
        if msg["isReplay"].as_bool() == Some(true) {
            let is_prompt = content.as_str().is_some()
                || content
                    .as_array()
                    .is_some_and(|parts| parts.iter().all(|p| p["type"].as_str() == Some("text")));
            if is_prompt {
                self.on_echo(msg["uuid"].as_str().unwrap_or_default(), out);
            }
            return;
        }
        let Some(parts) = content.as_array() else {
            return;
        };
        for part in parts
            .iter()
            .filter(|p| p["type"].as_str() == Some("tool_result"))
        {
            let id = part["tool_use_id"].as_str().unwrap_or_default();
            let (output, truncated) = bounded_output(&content_text(&part["content"]));
            let status = if part["is_error"].as_bool() != Some(true) {
                ToolStatus::Completed
            } else if self.denied.remove(id) {
                ToolStatus::Declined
            } else {
                ToolStatus::Failed
            };
            out.emit(Entry::ToolResult {
                id: id.to_owned(),
                status,
                output,
                truncated,
                duration_ms: None,
            });
        }
    }

    /// Our message `uuid` came back: that prompt is confirmed, wherever it is in the queue.
    fn on_echo(&mut self, uuid: &str, out: &mut Out) {
        let Some(i) = self.unechoed.iter().position(|(_, u)| u == uuid) else {
            if self.done.iter().any(|u| u == uuid) {
                out.notice("stale_echo", format!("claude echoed message {uuid} late"));
            } else {
                out.error(
                    ErrorClass::Protocol,
                    format!("claude echoed a user message nobody sent ({uuid:?})"),
                );
            }
            return;
        };
        let Some((request_id, uuid)) = self.unechoed.remove(i) else {
            return;
        };
        self.retire(uuid.clone());
        out.emit(Entry::PromptAck {
            request_id: request_id.clone(),
            ack: AckKind::Confirmed,
            native_turn_id: Some(uuid.clone()),
            detail: None,
        });
        out.emit(Entry::TurnStarted {
            request_id: Some(request_id),
            native_turn_id: Some(uuid),
        });
    }

    fn retire(&mut self, uuid: String) {
        if self.done.len() == 32 {
            self.done.pop_front();
        }
        self.done.push_back(uuid);
    }

    fn on_control_response(&mut self, msg: &Value) {
        let response = &msg["response"];
        if let Some(turn) = self.turn.as_mut()
            && turn.interrupt.is_some()
            && response["request_id"].as_str() == turn.interrupt.as_deref()
            && response["subtype"].as_str() == Some("success")
        {
            turn.interrupt_accepted = true;
        }
    }

    fn on_control_request(&mut self, msg: &Value, out: &mut Out) {
        let request = &msg["request"];
        let Some(id) = msg["request_id"].as_str().filter(|id| !id.is_empty()) else {
            out.error(
                ErrorClass::Protocol,
                "claude control_request without request_id",
            );
            return;
        };
        let subtype = request["subtype"].as_str().unwrap_or_default();
        if subtype != "can_use_tool" {
            out.write(obj! {
                "type" => "control_response",
                "response" => obj! {
                    "subtype" => "error",
                    "request_id" => id,
                    "error" => format!("protectorate does not handle {subtype}"),
                },
            });
            out.unmapped(&format!("control_request/{subtype}"));
            return;
        }
        if self.approvals.contains_key(id) {
            out.error(
                ErrorClass::Protocol,
                format!("claude reused pending control request id {id:?}"),
            );
            return;
        }
        let name = request["tool_name"].as_str().unwrap_or_default();
        let input = request["input"].clone();
        let suggestions = request["permission_suggestions"].clone();
        let mut options = vec![Decision::AllowOnce];
        if suggestions.as_array().is_some_and(|s| !s.is_empty()) {
            options.push(Decision::AllowSession);
        }
        options.extend([Decision::Deny, Decision::Cancel]);
        let summary = crate::emit::summarize(name, Some(&input));
        let (bounded, _) = crate::emit::bounded_input(&input);
        out.emit(Entry::ApprovalRequest {
            id: id.to_owned(),
            tool: tool_kind(name),
            name: name.to_owned(),
            summary,
            input: bounded,
            options,
        });
        self.approvals.insert(
            id.to_owned(),
            Approval {
                input,
                suggestions,
                tool_use_id: opt_str(&request["tool_use_id"]),
            },
        );
        if !self.supervised {
            let _ = self.answer(id, Decision::AllowOnce, Some("autonomous"), out);
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
            .remove(id)
            .ok_or_else(|| DriverError::UnknownApproval(id.to_owned()))?;
        if matches!(decision, Decision::Deny | Decision::Cancel)
            && let Some(tool_use_id) = &approval.tool_use_id
        {
            self.denied.insert(tool_use_id.clone());
        }
        let response = match decision {
            Decision::AllowOnce => obj! { "behavior" => "allow", "updatedInput" => approval.input },
            Decision::AllowSession => obj! {
                "behavior" => "allow",
                "updatedInput" => approval.input,
                "updatedPermissions" => approval.suggestions,
            },
            Decision::Deny => obj! { "behavior" => "deny", "message" => "Denied by the owner." },
            Decision::Cancel => obj! {
                "behavior" => "deny",
                "message" => "Cancelled by the owner.",
                "interrupt" => true,
            },
        };
        out.write(obj! {
            "type" => "control_response",
            "response" => obj! { "subtype" => "success", "request_id" => id, "response" => response },
        });
        out.emit(Entry::ApprovalDecision {
            id: id.to_owned(),
            decision,
            by: by.map(str::to_owned),
            confirmed: false,
        });
        Ok(())
    }

    fn on_result(&mut self, msg: &Value, out: &mut Out) {
        self.check_session(msg, out);
        let turn = self.turn.take();
        let is_error = msg["is_error"].as_bool() == Some(true);
        let subtype = msg["subtype"].as_str().unwrap_or_default();
        // A result written before Claude took the interrupt is that result, success or error.
        let interrupted = is_error && turn.as_ref().is_some_and(|t| t.interrupt_accepted)
            || msg["terminal_reason"]
                .as_str()
                .is_some_and(|r| r.starts_with("aborted"));
        let native = opt_str(&msg["stop_reason"])
            .or_else(|| opt_str(&msg["terminal_reason"]))
            .or_else(|| opt_str(&msg["subtype"]));
        let stop = match (subtype, native.as_deref()) {
            ("error_max_turns", _) => Stop::MaxTurns,
            _ if interrupted => Stop::Cancelled,
            (_, _) if is_error => Stop::Error,
            ("success", Some("end_turn") | None) => Stop::EndTurn,
            ("success", _) => Stop::Other,
            _ => Stop::Error,
        };
        if is_error && !interrupted {
            let text = opt_str(&msg["result"])
                .or_else(|| {
                    msg["errors"]
                        .as_array()
                        .map(|e| e.iter().map(content_text).collect::<Vec<_>>().join("; "))
                })
                .unwrap_or_else(|| format!("claude turn failed ({subtype})"));
            out.error(classify(&text, msg["api_error_status"].as_u64()), text);
        }
        let mut window = None;
        for (_, m) in msg["modelUsage"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.iter())
        {
            window = window.max(out.count("contextWindow", &m["contextWindow"]));
        }
        let cost_usd = out.cost("total_cost_usd", &msg["total_cost_usd"]);
        let usage = &msg["usage"];
        let mut n = |k: &str| out.count(k, &usage[k]).unwrap_or(0);
        let usage = Usage {
            input: n("input_tokens"),
            output: n("output_tokens"),
            cache_read: n("cache_read_input_tokens"),
            cache_write: n("cache_creation_input_tokens"),
            cost_usd,
            context_tokens: self.context_tokens,
            context_window: window,
        };
        out.emit(Entry::TurnSettled {
            stop,
            native_stop: native,
            usage,
            duration_ms: msg["duration_ms"].as_u64(),
        });
        if !self.unechoed.is_empty() {
            out.notice(
                "unechoed",
                format!(
                    "claude settled without echoing {} sent message(s)",
                    self.unechoed.len()
                ),
            );
            for (_, uuid) in std::mem::take(&mut self.unechoed) {
                self.retire(uuid);
            }
        }
        self.approvals.clear();
        self.denied.clear();
    }
}

impl Protocol for Claude {
    fn start(&mut self, out: &mut Out) {
        self.session_info(out);
    }

    fn on_message(&mut self, msg: Value, out: &mut Out) {
        if !msg["parent_tool_use_id"].is_null() {
            return;
        }
        match msg["type"].as_str().unwrap_or_default() {
            "system" => self.on_system(&msg, out),
            "stream_event" => self.on_stream_event(&msg, out),
            "assistant" => self.on_assistant(&msg, out),
            "user" => self.on_user(&msg, out),
            "control_request" => self.on_control_request(&msg, out),
            "result" => self.on_result(&msg, out),
            "control_response" => self.on_control_response(&msg),
            "control_cancel_request"
            | "rate_limit_event"
            | "keep_alive"
            | "tool_progress"
            | "auth_status" => {}
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
        if steer.is_some() {
            return Err(DriverError::Unsupported(
                "Claude cannot steer a running turn",
            ));
        }
        if self.turn.is_some() {
            return Err(DriverError::Busy);
        }
        self.sent += 1;
        let uuid = format!("{}{:012x}", self.uuid_base, self.sent);
        out.write(obj! {
            "type" => "user",
            "message" => obj! { "role" => "user", "content" => text },
            "uuid" => uuid.as_str(),
        });
        self.unechoed.push_back((request_id.to_owned(), uuid));
        self.turn = Some(Turn {
            interrupt: None,
            interrupt_accepted: false,
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
        let Some(turn) = self.turn.as_mut() else {
            return Ok(());
        };
        self.next_control += 1;
        let id = format!("pt-interrupt-{}", self.next_control);
        out.write(obj! {
            "type" => "control_request",
            "request_id" => id.as_str(),
            "request" => obj! { "subtype" => "interrupt" },
        });
        turn.interrupt = Some(id);
        Ok(())
    }
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "Bash" | "BashOutput" | "KillShell" | "PowerShell" | "Monitor" => ToolKind::Command,
        "Read" | "NotebookRead" | "LS" => ToolKind::Read,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolKind::Edit,
        "Grep" | "Glob" | "ToolSearch" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Web,
        "Task" | "Agent" => ToolKind::Subagent,
        n if n.starts_with("mcp__") => ToolKind::Mcp,
        _ => ToolKind::Other,
    }
}

fn edit_path(name: &str, input: &Value) -> Option<String> {
    match name {
        "Edit" | "MultiEdit" | "Write" => opt_str(&input["file_path"]),
        "NotebookEdit" => opt_str(&input["notebook_path"]),
        _ => None,
    }
}
