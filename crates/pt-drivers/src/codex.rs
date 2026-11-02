//! `codex app-server` JSON-RPC over stdio (method names checked against
//! `codex app-server generate-json-schema` of codex-cli 0.164.0).
//!
//! A `turn/start` with no reply stays unconfirmed: the effect record turns that into
//! Uncertain, never into "not started".

use std::collections::{HashMap, VecDeque};

use pt_json::{Value, obj};

use crate::emit::{Out, bounded_output, content_text, id_key, opt_str};
use crate::event::{
    AckKind, Decision, Entry, ErrorClass, Harness, Stop, ToolKind, ToolStatus, TouchedPath, Usage,
    classify,
};
use crate::{DriverError, Permission, Protocol, SpawnSpec};

pub(crate) struct Codex {
    cwd: String,
    model: Option<String>,
    effort: Option<String>,
    /// Catalog pages read while checking `effort`.
    model_pages: u8,
    resume: Option<String>,
    supervised: bool,
    next_id: u64,
    pending: HashMap<u64, Rpc>,
    thread_id: Option<String>,
    turn: Option<Turn>,
    /// Recently completed turn ids, so a late or repeated notice for one is not this turn's.
    finished: VecDeque<String>,
    approvals: HashMap<String, Approval>,
    answered: HashMap<String, Decision>,
    last_total: Totals,
    total: Totals,
    context_tokens: Option<u64>,
    context_window: Option<u64>,
}

/// Pages of Codex's model catalog read before giving up on finding the model.
const MODEL_PAGES: u8 = 20;

enum Rpc {
    Initialize,
    Models,
    Open(&'static str),
    TurnStart(String),
    Steer(String),
    Interrupt,
}

struct Turn {
    native: Option<String>,
    started: bool,
    request_id: Option<String>,
    interrupt_wanted: bool,
    errored: bool,
}

struct Approval {
    rpc_id: Value,
    method: String,
    permissions: Value,
}

#[derive(Clone, Copy, Default)]
struct Totals {
    input: u64,
    cached: u64,
    cache_write: u64,
    output: u64,
}

impl Codex {
    pub(crate) fn new(spec: &SpawnSpec, cwd: String) -> Codex {
        Codex {
            cwd,
            model: spec.model.clone(),
            effort: spec.effort.clone(),
            model_pages: 0,
            resume: spec.resume.clone(),
            supervised: spec.permission == Permission::Supervised,
            next_id: 0,
            pending: HashMap::new(),
            thread_id: None,
            turn: None,
            finished: VecDeque::new(),
            approvals: HashMap::new(),
            answered: HashMap::new(),
            last_total: Totals::default(),
            total: Totals::default(),
            context_tokens: None,
            context_window: None,
        }
    }

    pub(crate) fn argv(&self, spec: &SpawnSpec) -> Vec<String> {
        let mut a = vec!["app-server".to_owned()];
        a.extend(spec.extra_args.iter().cloned());
        a
    }

    fn call(&mut self, method: &str, params: Value, rpc: Rpc, out: &mut Out) {
        self.next_id += 1;
        self.pending.insert(self.next_id, rpc);
        out.write(obj! { "id" => self.next_id, "method" => method, "params" => params });
    }

    /// Asks for a page of the model catalog, to check the requested effort against it.
    fn list_models(&mut self, cursor: Option<String>, out: &mut Out) {
        self.model_pages += 1;
        let mut params = obj! { "limit" => 100 };
        if let Some(cursor) = cursor {
            params
                .as_object_mut()
                .expect("obj! builds an object")
                .insert("cursor", cursor);
        }
        self.call("model/list", params, Rpc::Models, out);
    }

    fn drop_effort(&mut self, why: String, out: &mut Out) {
        if let Some(effort) = self.effort.take() {
            out.notice(
                "effort",
                format!("codex: effort {effort:?} {why}; using the default"),
            );
        }
    }

    /// Reads one catalog page: the requested effort stays only if the model advertises it.
    fn check_effort(&mut self, result: &Value, out: &mut Out) {
        let Some(effort) = self.effort.clone() else {
            return;
        };
        let models = result["data"].as_array().unwrap_or_default();
        let entry = models.iter().find(|m| match &self.model {
            Some(want) => m["model"].as_str() == Some(want.as_str()),
            None => m["isDefault"].as_bool() == Some(true),
        });
        match entry {
            Some(m) => {
                let advertised = m["supportedReasoningEfforts"]
                    .as_array()
                    .unwrap_or_default()
                    .iter()
                    .any(|e| e["reasoningEffort"].as_str() == Some(effort.as_str()));
                if !advertised {
                    self.drop_effort("is not advertised by its model".into(), out);
                }
            }
            None => match opt_str(&result["nextCursor"]) {
                Some(cursor) if self.model_pages < MODEL_PAGES => {
                    return self.list_models(Some(cursor), out);
                }
                // A model the catalog lacks is Codex's to refuse when the thread opens.
                _ => {}
            },
        }
        self.open_thread(out);
    }

    fn open_thread(&mut self, out: &mut Out) {
        let (policy, sandbox) = if self.supervised {
            ("untrusted", "workspace-write")
        } else {
            ("never", "danger-full-access")
        };
        let mut params = obj! {
            "cwd" => self.cwd.as_str(),
            "approvalPolicy" => policy,
            "sandbox" => sandbox,
        };
        let map = params.as_object_mut().expect("obj! builds an object");
        if let Some(model) = &self.model {
            map.insert("model", model.as_str());
        }
        let method = match &self.resume {
            Some(id) => {
                map.insert("threadId", id.as_str());
                map.insert("excludeTurns", true);
                "thread/resume"
            }
            None => "thread/start",
        };
        self.call(method, params, Rpc::Open(method), out);
    }

    fn send_interrupt(&mut self, out: &mut Out) {
        let (Some(thread), Some(turn)) = (
            self.thread_id.clone(),
            self.turn.as_ref().and_then(|t| t.native.clone()),
        ) else {
            return;
        };
        if let Some(t) = self.turn.as_mut() {
            t.interrupt_wanted = false;
        }
        self.call(
            "turn/interrupt",
            obj! { "threadId" => thread, "turnId" => turn },
            Rpc::Interrupt,
            out,
        );
    }

    /// Gives the running turn its native id (once) and reports it started.
    fn bind_turn(&mut self, native: Option<String>, out: &mut Out) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        match (&turn.native, native) {
            (None, native) => turn.native = native,
            (Some(have), Some(native)) if *have != native => {
                out.error(
                    ErrorClass::Protocol,
                    format!("codex named turn {native} for turn {have}"),
                );
                return;
            }
            _ => {}
        }
        if !turn.started && turn.native.is_some() {
            turn.started = true;
            out.emit(Entry::TurnStarted {
                request_id: turn.request_id.clone(),
                native_turn_id: turn.native.clone(),
            });
        }
        if turn.interrupt_wanted {
            self.send_interrupt(out);
        }
    }

    fn finished(&self, turn: &str) -> bool {
        self.finished.iter().any(|t| t == turn)
    }

    fn on_reply(&mut self, msg: &Value, out: &mut Out) {
        let id = msg["id"].as_u64();
        let Some(rpc) = id.and_then(|id| self.pending.remove(&id)) else {
            out.error(
                ErrorClass::Protocol,
                format!("codex replied to unknown request {}", msg["id"]),
            );
            return;
        };
        let error = &msg["error"];
        let failed = !error.is_null();
        if failed == msg.get("result").is_some() {
            // A reply is a result or an error, never both or neither: the effect stays open.
            out.error(
                ErrorClass::Protocol,
                format!("codex reply to request {} is malformed", msg["id"]),
            );
            return;
        }
        let message = error["message"]
            .as_str()
            .unwrap_or("request failed")
            .to_owned();
        let result = &msg["result"];
        match rpc {
            Rpc::Initialize if failed => out.refuse(
                classify(&message, None),
                format!("codex initialize: {message}"),
            ),
            Rpc::Initialize => {
                out.write(obj! { "method" => "initialized" });
                if self.effort.is_some() {
                    self.list_models(None, out);
                } else {
                    self.open_thread(out);
                }
            }
            Rpc::Models if failed => {
                self.drop_effort(format!("could not be checked ({message})"), out);
                self.open_thread(out);
            }
            Rpc::Models => self.check_effort(result, out),
            Rpc::Open(method) if failed => out.refuse(
                classify(&message, None),
                format!("codex {method}: {message}"),
            ),
            Rpc::Open(method) => {
                let Some(thread) = opt_str(&result["thread"]["id"]) else {
                    out.error(
                        ErrorClass::Protocol,
                        format!("codex {method} gave no thread id"),
                    );
                    return;
                };
                if let Some(want) = &self.resume
                    && *want != thread
                {
                    out.error(
                        ErrorClass::Protocol,
                        format!("codex resumed thread {thread}, expected {want}"),
                    );
                    return;
                }
                self.thread_id = Some(thread.clone());
                out.emit(Entry::SessionInfo {
                    harness: Harness::Codex,
                    native_id: thread,
                    model: opt_str(&result["model"]),
                    provider: opt_str(&result["modelProvider"]),
                    effort: self
                        .effort
                        .clone()
                        .or_else(|| opt_str(&result["reasoningEffort"])),
                    cwd: opt_str(&result["cwd"]),
                });
            }
            Rpc::TurnStart(request_id) if failed => {
                // A JSON-RPC error is definitive: no turn was started.
                if self
                    .turn
                    .as_ref()
                    .is_some_and(|t| t.request_id.as_deref() == Some(&request_id))
                {
                    self.turn = None;
                }
                out.emit(Entry::PromptAck {
                    request_id,
                    ack: AckKind::Rejected,
                    native_turn_id: None,
                    detail: Some(message),
                });
            }
            Rpc::TurnStart(request_id) => {
                let Some(native) = opt_str(&result["turn"]["id"]) else {
                    out.error(ErrorClass::Protocol, "codex turn/start gave no turn id");
                    return;
                };
                out.emit(Entry::PromptAck {
                    request_id: request_id.clone(),
                    ack: AckKind::Confirmed,
                    native_turn_id: Some(native.clone()),
                    detail: None,
                });
                // A late reply binds only its own turn, never the one running now.
                if self
                    .turn
                    .as_ref()
                    .is_some_and(|t| t.request_id.as_deref() == Some(&request_id))
                {
                    self.bind_turn(Some(native), out);
                }
            }
            Rpc::Steer(request_id) if failed => out.emit(Entry::PromptAck {
                request_id,
                ack: AckKind::Rejected,
                native_turn_id: None,
                detail: Some(message),
            }),
            Rpc::Steer(request_id) => {
                let Some(native) = opt_str(&result["turnId"]) else {
                    out.error(ErrorClass::Protocol, "codex turn/steer gave no turn id");
                    return;
                };
                out.emit(Entry::PromptAck {
                    request_id,
                    ack: AckKind::Confirmed,
                    native_turn_id: Some(native),
                    detail: None,
                });
            }
            Rpc::Interrupt if failed => {
                out.notice("interrupt", format!("codex turn/interrupt: {message}"))
            }
            Rpc::Interrupt => {}
        }
    }

    fn on_server_request(&mut self, msg: &Value, method: &str, out: &mut Out) {
        let p = &msg["params"];
        let (tool, name, summary_field) = match method {
            "item/commandExecution/requestApproval" => (ToolKind::Command, "shell", "command"),
            "item/fileChange/requestApproval" => (ToolKind::Edit, "apply_patch", "grantRoot"),
            "item/permissions/requestApproval" => (ToolKind::Other, "permissions", "reason"),
            _ => {
                out.write(obj! {
                    "id" => msg["id"].clone(),
                    "error" => obj! {
                        "code" => -32601,
                        "message" => format!("protectorate does not handle {method}"),
                    },
                });
                out.unmapped(method);
                return;
            }
        };
        let Some(key) = id_key(&msg["id"]) else {
            out.error(
                ErrorClass::Protocol,
                format!("codex request {method} has id {}", msg["id"]),
            );
            return;
        };
        let id = format!("codex-{key}");
        if self.approvals.contains_key(&id) {
            out.error(
                ErrorClass::Protocol,
                format!("codex reused pending request id {}", msg["id"]),
            );
            return;
        }
        self.answered.remove(&id);
        let detail = p[summary_field]
            .as_str()
            .or_else(|| p["reason"].as_str())
            .unwrap_or("");
        let summary = if detail.is_empty() {
            name.to_owned()
        } else {
            format!("{name}: {}", crate::emit::bounded(detail, 280))
        };
        let (input, _) = crate::emit::bounded_input(p);
        out.emit(Entry::ApprovalRequest {
            id: id.clone(),
            tool,
            name: name.to_owned(),
            summary,
            input,
            options: vec![
                Decision::AllowOnce,
                Decision::AllowSession,
                Decision::Deny,
                Decision::Cancel,
            ],
        });
        self.approvals.insert(
            id.clone(),
            Approval {
                rpc_id: msg["id"].clone(),
                method: method.to_owned(),
                permissions: p["permissions"].clone(),
            },
        );
        if !self.supervised {
            let _ = self.answer(&id, Decision::AllowOnce, Some("autonomous"), out);
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
        let result = if approval.method == "item/permissions/requestApproval" {
            match decision {
                Decision::AllowOnce => {
                    obj! { "permissions" => approval.permissions, "scope" => "turn" }
                }
                Decision::AllowSession => {
                    obj! { "permissions" => approval.permissions, "scope" => "session" }
                }
                Decision::Deny | Decision::Cancel => {
                    obj! { "permissions" => obj! {}, "scope" => "turn" }
                }
            }
        } else {
            let word = match decision {
                Decision::AllowOnce => "accept",
                Decision::AllowSession => "acceptForSession",
                Decision::Deny => "decline",
                Decision::Cancel => "cancel",
            };
            obj! { "decision" => word }
        };
        out.write(obj! { "id" => approval.rpc_id, "result" => result });
        self.answered.insert(id.to_owned(), decision);
        out.emit(Entry::ApprovalDecision {
            id: id.to_owned(),
            decision,
            by: by.map(str::to_owned),
            confirmed: false,
        });
        Ok(())
    }

    fn on_note(&mut self, method: &str, p: &Value, out: &mut Out) {
        if let (Some(thread), Some(ours)) = (p["threadId"].as_str(), &self.thread_id)
            && thread != ours
        {
            return;
        }
        match method {
            "turn/started" => {
                let native = opt_str(&p["turn"]["id"]);
                if native.as_deref().is_some_and(|t| self.finished(t)) {
                    return;
                }
                if let (Some(have), Some(native)) =
                    (self.turn.as_ref().and_then(|t| t.native.as_ref()), &native)
                    && have != native
                {
                    out.notice(
                        "turn",
                        format!("codex started turn {native} while turn {have} runs"),
                    );
                    return;
                }
                if self.turn.is_none() {
                    self.turn = Some(Turn {
                        native: None,
                        started: false,
                        request_id: None,
                        interrupt_wanted: false,
                        errored: false,
                    });
                }
                self.bind_turn(native, out);
            }
            "turn/completed" => self.on_turn_completed(p, out),
            "item/agentMessage/delta" => {
                let id = p["itemId"].as_str().unwrap_or("codex-message");
                out.delta(false, id, p["delta"].as_str().unwrap_or(""));
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                let id = p["itemId"].as_str().unwrap_or("codex-reasoning");
                out.delta(true, id, p["delta"].as_str().unwrap_or(""));
            }
            "item/started" => self.on_item_started(&p["item"], out),
            "item/completed" => self.on_item_completed(&p["item"], out),
            "thread/tokenUsage/updated" => {
                let usage = &p["tokenUsage"];
                let total = &usage["total"];
                let mut n = |k: &str| out.count(k, &total[k]).unwrap_or(0);
                self.total = Totals {
                    input: n("inputTokens"),
                    cached: n("cachedInputTokens"),
                    cache_write: n("cacheWriteInputTokens"),
                    output: n("outputTokens"),
                };
                self.context_tokens = out.count("inputTokens", &usage["last"]["inputTokens"]);
                if let Some(window) = out.count("modelContextWindow", &usage["modelContextWindow"])
                {
                    self.context_window = Some(window);
                }
            }
            "error" => {
                let error = &p["error"];
                let text = error["message"]
                    .as_str()
                    .unwrap_or("codex error")
                    .to_owned();
                if p["willRetry"].as_bool() == Some(true) {
                    out.notice("retry", text);
                } else {
                    if let Some(t) = self.turn.as_mut() {
                        t.errored = true;
                    }
                    out.error(error_class(error), text);
                }
            }
            "serverRequest/resolved" => {
                let Some(key) = id_key(&p["requestId"]) else {
                    return;
                };
                let id = format!("codex-{key}");
                if let Some(decision) = self.answered.remove(&id) {
                    out.emit(Entry::ApprovalDecision {
                        id,
                        decision,
                        by: None,
                        confirmed: true,
                    });
                } else if self.approvals.remove(&id).is_some() {
                    out.emit(Entry::ApprovalDecision {
                        id,
                        decision: Decision::Cancel,
                        by: Some("harness".into()),
                        confirmed: true,
                    });
                }
            }
            "model/rerouted" => out.notice(
                "model_rerouted",
                format!(
                    "model rerouted from {} to {}",
                    p["fromModel"].as_str().unwrap_or("?"),
                    p["toModel"].as_str().unwrap_or("?")
                ),
            ),
            "warning"
            | "configWarning"
            | "deprecationNotice"
            | "guardianWarning"
            | "modelProvider/authRecoveryStarted"
            | "modelProvider/authRecoveryCompleted" => {
                let text = p["message"]
                    .as_str()
                    .or_else(|| p["summary"].as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| p.to_string());
                out.notice(method, crate::emit::bounded(&text, 2000));
            }
            m if ignored(m) => {}
            m => out.unmapped(m),
        }
    }

    fn on_turn_completed(&mut self, p: &Value, out: &mut Out) {
        let turn = &p["turn"];
        let native = opt_str(&turn["id"]);
        if let Some(native) = &native {
            if self.finished(native) {
                return;
            }
            if let Some(have) = self.turn.as_ref().and_then(|t| t.native.as_ref())
                && have != native
            {
                out.notice(
                    "turn",
                    format!("codex completed turn {native} while turn {have} runs"),
                );
                return;
            }
            if self.finished.len() == 16 {
                self.finished.pop_front();
            }
            self.finished.push_back(native.clone());
        }
        // Completed before turn/start's reply: it did start, which the core must see first.
        if let Some(t) = self.turn.as_mut() {
            t.interrupt_wanted = false;
        }
        self.bind_turn(native, out);
        if let Some(t) = self.turn.as_mut().filter(|t| !t.started) {
            t.started = true;
            out.emit(Entry::TurnStarted {
                request_id: t.request_id.clone(),
                native_turn_id: None,
            });
        }
        let errored = self.turn.take().is_some_and(|t| t.errored);
        let status = turn["status"].as_str().unwrap_or("completed");
        let stop = match status {
            "completed" => Stop::EndTurn,
            "interrupted" => Stop::Cancelled,
            "failed" => Stop::Error,
            _ => Stop::Other,
        };
        if status == "failed" && !errored {
            let error = &turn["error"];
            let text = error["message"].as_str().unwrap_or("codex turn failed");
            out.error(error_class(error), text);
        }
        let d = Totals {
            input: self.total.input.saturating_sub(self.last_total.input),
            cached: self.total.cached.saturating_sub(self.last_total.cached),
            cache_write: self
                .total
                .cache_write
                .saturating_sub(self.last_total.cache_write),
            output: self.total.output.saturating_sub(self.last_total.output),
        };
        self.last_total = self.total;
        out.emit(Entry::TurnSettled {
            stop,
            native_stop: Some(status.to_owned()),
            usage: Usage {
                input: d.input.saturating_sub(d.cached),
                output: d.output,
                cache_read: d.cached,
                cache_write: d.cache_write,
                cost_usd: None,
                context_tokens: self.context_tokens,
                context_window: self.context_window,
            },
            duration_ms: turn["durationMs"].as_u64(),
        });
        self.approvals.clear();
    }

    fn on_item_started(&mut self, item: &Value, out: &mut Out) {
        let id = item["id"].as_str().unwrap_or_default();
        let s = |k: &str| item[k].as_str().unwrap_or_default();
        match s("type") {
            "commandExecution" => out.tool_call(
                id,
                ToolKind::Command,
                "shell",
                &obj! { "command" => item["command"].clone(), "cwd" => item["cwd"].clone() },
            ),
            "fileChange" => {
                let paths: Vec<TouchedPath> = item["changes"]
                    .as_array()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|c| {
                        Some(TouchedPath {
                            path: opt_str(&c["path"])?,
                            op: opt_str(&c["kind"]["type"]).or_else(|| opt_str(&c["kind"])),
                        })
                    })
                    .collect();
                let list: Vec<Value> = paths.iter().map(|p| Value::from(p.path.as_str())).collect();
                out.tool_call(
                    id,
                    ToolKind::Edit,
                    "apply_patch",
                    &obj! { "paths" => Value::from(list) },
                );
                if !paths.is_empty() {
                    out.emit(Entry::FileChange {
                        tool_id: Some(id.to_owned()),
                        paths,
                    });
                }
            }
            "mcpToolCall" => out.tool_call(
                id,
                ToolKind::Mcp,
                &format!("{}/{}", s("server"), s("tool")),
                &item["arguments"],
            ),
            "dynamicToolCall" => out.tool_call(id, ToolKind::Other, s("tool"), &item["arguments"]),
            "webSearch" => out.tool_call(
                id,
                ToolKind::Web,
                "web_search",
                &obj! { "query" => item["query"].clone() },
            ),
            "imageView" => out.tool_call(
                id,
                ToolKind::Read,
                "image_view",
                &obj! { "path" => item["path"].clone() },
            ),
            "collabAgentToolCall" => out.tool_call(
                id,
                ToolKind::Subagent,
                s("tool"),
                &obj! { "description" => item["prompt"].clone() },
            ),
            "subAgentActivity" => out.tool_call(
                id,
                ToolKind::Subagent,
                "subagent",
                &obj! { "description" => item["kind"].clone() },
            ),
            "imageGeneration" => {
                out.tool_call(id, ToolKind::Other, "image_generation", &Value::Null)
            }
            "userMessage" | "agentMessage" | "reasoning" | "plan" | "hookPrompt"
            | "contextCompaction" | "enteredReviewMode" | "exitedReviewMode" | "sleep"
            | "functionCallOutput" => {}
            other => out.unmapped(&format!("item/{other}")),
        }
    }

    fn on_item_completed(&mut self, item: &Value, out: &mut Out) {
        let id = item["id"].as_str().unwrap_or_default();
        let status = match item["status"].as_str() {
            Some("failed") => ToolStatus::Failed,
            Some("declined") => ToolStatus::Declined,
            _ => ToolStatus::Completed,
        };
        let result = |out: &mut Out, status: ToolStatus, text: &str| {
            let (output, truncated) = bounded_output(text);
            out.emit(Entry::ToolResult {
                id: id.to_owned(),
                status,
                output,
                truncated,
                duration_ms: item["durationMs"].as_u64(),
            });
        };
        match item["type"].as_str().unwrap_or_default() {
            "agentMessage" => out.finish(false, id, item["text"].as_str().unwrap_or("")),
            "reasoning" => {
                let summary = content_text(&item["summary"]);
                let text = if summary.is_empty() {
                    content_text(&item["content"])
                } else {
                    summary
                };
                out.finish(true, id, &text);
            }
            "contextCompaction" => out.emit(Entry::Compaction {
                trigger: None,
                tokens_before: None,
                tokens_after: None,
            }),
            "commandExecution" => {
                let status = match item["exitCode"].as_i64() {
                    Some(code) if code != 0 && status == ToolStatus::Completed => {
                        ToolStatus::Failed
                    }
                    _ => status,
                };
                result(out, status, item["aggregatedOutput"].as_str().unwrap_or(""));
            }
            "fileChange" => {
                let paths: Vec<&str> = item["changes"]
                    .as_array()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|c| c["path"].as_str())
                    .collect();
                result(out, status, &paths.join("\n"));
            }
            "mcpToolCall" => {
                let text = if item["error"].is_null() {
                    content_text(&item["result"]["content"])
                } else {
                    content_text(&item["error"]["message"])
                };
                result(out, status, &text);
            }
            "dynamicToolCall" => {
                let status = if item["success"].as_bool() == Some(false) {
                    ToolStatus::Failed
                } else {
                    status
                };
                result(out, status, &content_text(&item["contentItems"]));
            }
            "webSearch"
            | "imageView"
            | "collabAgentToolCall"
            | "subAgentActivity"
            | "imageGeneration" => result(out, status, ""),
            _ => {}
        }
    }
}

fn ignored(method: &str) -> bool {
    matches!(
        method,
        "thread/started"
            | "thread/status/changed"
            | "thread/readState/changed"
            | "thread/settings/updated"
            | "thread/name/updated"
            | "thread/compacted"
            | "thread/archived"
            | "thread/unarchived"
            | "thread/closed"
            | "thread/queue/changed"
            | "thread/attachment/updated"
            | "thread/prediction/updated"
            | "thread/project/updated"
            | "thread/environment/connected"
            | "thread/environment/disconnected"
            | "remoteControl/status/changed"
            | "mcpServer/startupStatus/updated"
            | "mcpServer/event/stream/notification"
            | "mcpServer/oauthLogin/completed"
            | "account/updated"
            | "account/rateLimits/updated"
            | "account/gatewayOAuth/changed"
            | "account/login/completed"
            | "app/list/updated"
            | "skills/changed"
            | "project/changed"
            | "fs/changed"
            | "turn/moderationMetadata"
            | "turn/plan/updated"
            | "turn/diff/updated"
            | "item/plan/delta"
            | "item/reasoning/summaryPartAdded"
            | "item/commandExecution/outputDelta"
            | "item/commandExecution/terminalInteraction"
            | "item/fileChange/outputDelta"
            | "item/fileChange/patchUpdated"
            | "item/mcpToolCall/progress"
            | "item/autoApprovalReview/started"
            | "item/autoApprovalReview/completed"
            | "hook/started"
            | "hook/completed"
            | "command/exec/outputDelta"
            | "process/outputDelta"
            | "process/exited"
            | "model/verification"
            | "model/safetyBuffering/updated"
    ) || method.starts_with("thread/goal/")
        || method.starts_with("thread/realtime/")
        || method.starts_with("fuzzyFileSearch/")
        || method.starts_with("externalAgentConfig/")
        || method.starts_with("windows")
}

/// `codexErrorInfo` first, then the message text.
fn error_class(error: &Value) -> ErrorClass {
    let info = &error["codexErrorInfo"];
    match info.as_str() {
        Some("usageLimitExceeded" | "sessionBudgetExceeded") => return ErrorClass::Quota,
        Some("rateLimitExceeded" | "serverOverloaded") => return ErrorClass::RateLimit,
        Some("unauthorized") => return ErrorClass::Auth,
        _ => {}
    }
    let status = info
        .as_object()
        .and_then(|m| m.iter().find_map(|(_, v)| v["httpStatusCode"].as_u64()));
    classify(error["message"].as_str().unwrap_or(""), status)
}

impl Protocol for Codex {
    fn start(&mut self, out: &mut Out) {
        self.call(
            "initialize",
            obj! {
                "clientInfo" => obj! {
                    "name" => "protectorate",
                    "version" => env!("CARGO_PKG_VERSION"),
                },
                "capabilities" => obj! {
                    "experimentalApi" => true,
                    "optOutNotificationMethods" => Value::from(vec![
                        Value::from("turn/diff/updated"),
                        Value::from("item/commandExecution/outputDelta"),
                        Value::from("item/fileChange/outputDelta"),
                    ]),
                },
            },
            Rpc::Initialize,
            out,
        );
    }

    fn on_message(&mut self, msg: Value, out: &mut Out) {
        match (msg["method"].as_str(), msg.get("id").is_some()) {
            (Some(method), true) => {
                let method = method.to_owned();
                self.on_server_request(&msg, &method, out);
            }
            (Some(method), false) => {
                let method = method.to_owned();
                self.on_note(&method, &msg["params"], out);
            }
            (None, true) => self.on_reply(&msg, out),
            (None, false) => out.error(
                ErrorClass::Protocol,
                "codex wrote a message with neither method nor id",
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
        let thread = self.thread_id.clone().ok_or(DriverError::NotReady)?;
        let input = Value::from(vec![obj! { "type" => "text", "text" => text }]);
        if let Some(expected) = steer {
            if self.turn.as_ref().and_then(|t| t.native.as_deref()) != Some(expected) {
                return Err(DriverError::TurnEnded);
            }
            self.call(
                "turn/steer",
                obj! { "threadId" => thread, "input" => input, "expectedTurnId" => expected },
                Rpc::Steer(request_id.to_owned()),
                out,
            );
            return Ok(());
        }
        if self.turn.is_some() {
            return Err(DriverError::Busy);
        }
        let mut params = obj! {
            "threadId" => thread,
            "input" => input,
            "clientUserMessageId" => request_id,
        };
        if let Some(effort) = &self.effort {
            params
                .as_object_mut()
                .expect("obj! builds an object")
                .insert("effort", effort.as_str());
        }
        self.call(
            "turn/start",
            params,
            Rpc::TurnStart(request_id.to_owned()),
            out,
        );
        self.turn = Some(Turn {
            native: None,
            started: false,
            request_id: Some(request_id.to_owned()),
            interrupt_wanted: false,
            errored: false,
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
        if turn.native.is_none() {
            turn.interrupt_wanted = true;
            return Ok(());
        }
        self.send_interrupt(out);
        Ok(())
    }
}
