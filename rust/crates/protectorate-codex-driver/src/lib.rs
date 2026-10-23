//! Codex app-server JSON-RPC over stdio, running inside a caller supplied sandbox.
//! Codex keeps its native thread; Protectorate keeps an append-only public event log.
use protectorate_chatlog::{Event, Log};
use protectorate_sandbox::{
    runtime::{self, KillHandle, LaunchOptions, StdioMode},
    PlanKind, ValidatedPlan,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::ChildStdin,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 8 << 20;
const MAX_TOOL_OUTPUT: usize = 16 << 10;
const RPC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct Approval {
    rpc_id: Value,
    method: String,
    params: Value,
}

#[derive(Default)]
struct State {
    exited: bool,
    busy: bool,
    compacting: bool,
    selecting: bool,
    next_id: u64,
    replies: BTreeMap<u64, Value>,
    thread_id: String,
    turn_id: String,
    model: String,
    effort: String,
    default_model: String,
    default_effort: String,
    requested_model: String,
    requested_effort: String,
    context_tokens: u64,
    context_window: u64,
    compact_post: u64,
    compaction: Option<Result<Value, String>>,
    compact_candidate: Option<Value>,
    settings_confirmed: bool,
    approvals: BTreeMap<String, Approval>,
}

pub struct Session {
    stdin: Mutex<ChildStdin>,
    event_gate: Mutex<()>,
    operation: Mutex<()>,
    state: Mutex<State>,
    changed: Condvar,
    kill: KillHandle,
    log: Arc<Log>,
}

impl Session {
    pub fn start(
        plan: &ValidatedPlan,
        mut options: LaunchOptions,
        log: Arc<Log>,
        resume: Option<&str>,
        autonomous: bool,
    ) -> Result<Arc<Self>, String> {
        options.stdio = StdioMode::Duplex;
        let cwd = options.working_directory.to_string_lossy().into_owned();
        let mut child = match plan.kind() {
            PlanKind::IntentionallyUnsandboxedHarness => {
                runtime::spawn_direct_harness(plan, &options)
            }
            _ => runtime::spawn(plan, &options),
        }
        .map_err(|e| e.to_string())?;
        let stdin = child.take_stdin().ok_or("sandbox did not supply stdin")?;
        let stdout = child.take_stdout().ok_or("sandbox did not supply stdout")?;
        let stderr = child.take_stderr().ok_or("sandbox did not supply stderr")?;
        let session = Arc::new(Self {
            stdin: Mutex::new(stdin),
            event_gate: Mutex::new(()),
            operation: Mutex::new(()),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            kill: child.kill_handle(),
            log,
        });
        let reader = Arc::clone(&session);
        thread::spawn(move || reader.read_stdout(stdout));
        thread::spawn(move || {
            let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
        });
        let waiter = Arc::clone(&session);
        thread::spawn(move || {
            let result = child.wait();
            let _gate = waiter.event_gate.lock().unwrap();
            let mut state = waiter.state.lock().unwrap();
            state.exited = true;
            state.busy = false;
            waiter.changed.notify_all();
            drop(state);
            let _ = waiter.log.append(Event::new(
                json!({"error":{"text":format!("codex exited: {result:?}")}}),
            ));
            let _ = waiter
                .log
                .append(Event::new(json!({"state":{"state":"CHAT_STATE_EXITED"}})));
            let _ = waiter.log.close();
        });
        let init = session.rpc(
            "initialize",
            json!({"clientInfo":{"name":"protectorate","version":"0"},
            "capabilities":{"experimentalApi":true}}),
            RPC_TIMEOUT,
        );
        if let Err(e) = init {
            session.stop();
            return Err(format!("codex initialize: {e}"));
        }
        if let Err(error) = session.write(&json!({"method":"initialized","params":{}})) {
            session.stop();
            return Err(error);
        }
        let mut params = json!({"cwd":cwd});
        if !autonomous {
            params["approvalPolicy"] = json!("untrusted");
            params["sandbox"] = json!("workspace-write");
        }
        let method = if let Some(id) = resume {
            params["threadId"] = json!(id);
            params["excludeTurns"] = json!(true);
            "thread/resume"
        } else {
            "thread/start"
        };
        let result = session.rpc(method, params, RPC_TIMEOUT);
        let result = match result {
            Ok(v) => v,
            Err(e) => {
                session.stop();
                return Err(format!("codex {method}: {e}"));
            }
        };
        let id = result["thread"]["id"].as_str().unwrap_or_default();
        if id.is_empty() || resume.is_some_and(|expected| expected != id) {
            session.stop();
            return Err(format!("codex {method} returned an invalid thread id"));
        }
        {
            let mut state = session.state.lock().unwrap();
            state.thread_id = id.into();
            state.model = result["model"].as_str().unwrap_or_default().into();
            state.effort = result["reasoningEffort"]
                .as_str()
                .unwrap_or_default()
                .into();
            state.default_model = state.model.clone();
            state.default_effort = state.effort.clone();
        }
        if let Err(error) = session
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})))
        {
            session.stop();
            return Err(error.to_string());
        }
        Ok(session)
    }

    pub fn status(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"state":if s.exited {"CHAT_STATE_EXITED"} else if !s.approvals.is_empty() {"CHAT_STATE_AWAITING_APPROVAL"}
            else if s.compacting {"CHAT_STATE_COMPACTING"} else if s.busy {"CHAT_STATE_WORKING"} else {"CHAT_STATE_IDLE"},
            "harnessSessionId":s.thread_id,"model":s.model,"provider":"openai","reasoningEffort":s.effort,
            "contextTokens":s.context_tokens.to_string(),"contextWindow":s.context_window.to_string(),"lastSeq":self.log.last().to_string(),
            "settings":{"requested":{"model":s.requested_model,"effort":s.requested_effort},
                "effective":{"model":s.model,"effort":s.effort},
                "nativeDefaults":{"model":s.default_model,"effort":s.default_effort}}})
    }
    pub fn is_exited(&self) -> bool {
        self.state.lock().unwrap().exited
    }
    pub fn is_idle(&self) -> bool {
        let s = self.state.lock().unwrap();
        !s.exited && !s.busy && !s.compacting && !s.selecting && s.approvals.is_empty()
    }
    pub fn stop(&self) {
        self.kill.kill();
    }
    pub fn wait_exited(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().unwrap();
        while !s.exited {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return false;
            }
            (s, _) = self.changed.wait_timeout(s, remain).unwrap();
        }
        true
    }
    fn write(&self, value: &Value) -> Result<(), String> {
        let mut stdin = self.stdin.lock().unwrap();
        serde_json::to_writer(&mut *stdin, value).map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())
    }
    fn begin_rpc(&self, method: &str, params: Value) -> Result<u64, String> {
        let id = {
            let mut s = self.state.lock().unwrap();
            s.next_id += 1;
            s.next_id
        };
        self.write(&json!({"id":id,"method":method,"params":params}))?;
        Ok(id)
    }
    fn await_rpc(&self, id: u64, timeout: Duration) -> Result<Value, String> {
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().unwrap();
        loop {
            if let Some(reply) = s.replies.remove(&id) {
                if reply.get("error").is_some() {
                    return Err(reply["error"]["message"]
                        .as_str()
                        .unwrap_or("RPC failed")
                        .into());
                }
                return Ok(reply["result"].clone());
            }
            if s.exited {
                return Err("codex app-server exited before replying".into());
            }
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                self.stop();
                return Err("codex RPC timed out".into());
            }
            (s, _) = self.changed.wait_timeout(s, remain).unwrap();
        }
    }
    fn rpc(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.begin_rpc(method, params)?;
        self.await_rpc(id, timeout)
    }

    pub fn send(&self, text: &str, from: &str, request_id: &str) -> Result<u64, String> {
        if text.is_empty() || text.len() > 1 << 20 {
            return Err("invalid: message must contain 1 to 1048576 bytes".into());
        }
        let _op = self.operation.lock().unwrap();
        let gate = self.event_gate.lock().unwrap();
        let (thread, turn) = {
            let s = self.state.lock().unwrap();
            if s.exited {
                return Err("codex session exited".into());
            }
            if s.compacting || s.selecting || !s.approvals.is_empty() {
                return Err(
                    "precondition: Codex is compacting, changing settings, or awaiting approval"
                        .into(),
                );
            }
            (
                s.thread_id.clone(),
                if s.busy {
                    Some(s.turn_id.clone())
                } else {
                    None
                },
            )
        };
        if turn.as_deref() == Some("") {
            return Err("precondition: Codex turn has not started yet".into());
        }
        let was_steer = turn.is_some();
        let input = json!([{"type":"text","text":text}]);
        let (method, params) = if let Some(id) = turn {
            (
                "turn/steer",
                json!({"threadId":thread,"input":input,"expectedTurnId":id}),
            )
        } else {
            ("turn/start", json!({"threadId":thread,"input":input}))
        };
        let rpc_id = self.begin_rpc(method, params)?;
        self.state.lock().unwrap().busy = true;
        let seq = self
            .log
            .append(Event::new(
                json!({"user":{"text":text,"from":from,"requestId":request_id}}),
            ))
            .map_err(|e| {
                self.stop();
                e.to_string()
            })?;
        let _ = self
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_WORKING"}})));
        drop(gate);
        let result = self.await_rpc(rpc_id, RPC_TIMEOUT);
        match result {
            Ok(result) => {
                if let Some(id) = result["turn"]["id"].as_str() {
                    self.state.lock().unwrap().turn_id = id.into();
                }
                Ok(seq)
            }
            Err(e) => {
                if !was_steer {
                    self.state.lock().unwrap().busy = false;
                    let _ = self
                        .log
                        .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
                }
                self.log_error(&format!("codex {method}: {e}"));
                Err(format!("codex {method}: {e}"))
            }
        }
    }
    pub fn interrupt(&self) -> Result<(), String> {
        let (thread, turn) = {
            let s = self.state.lock().unwrap();
            if !s.busy || s.turn_id.is_empty() || s.exited {
                return Ok(());
            }
            (s.thread_id.clone(), s.turn_id.clone())
        };
        self.rpc(
            "turn/interrupt",
            json!({"threadId":thread,"turnId":turn}),
            RPC_TIMEOUT,
        )
        .map(|_| ())
    }
    pub fn respond(&self, id: &str, allow: bool, _note: &str, by: &str) -> Result<(), String> {
        let _gate = self.event_gate.lock().unwrap();
        let approval = { self.state.lock().unwrap().approvals.get(id).cloned() }
            .ok_or("invalid: approval is not pending")?;
        let result = if approval.method == "item/permissions/requestApproval" {
            json!({"permissions":if allow {approval.params["permissions"].clone()} else {json!({})},"scope":"turn"})
        } else {
            json!({"decision":if allow {"accept"} else {"decline"}})
        };
        self.write(&json!({"id":approval.rpc_id,"result":result}))?;
        self.state.lock().unwrap().approvals.remove(id);
        self.log
            .append(Event::new(
                json!({"approvalResolved":{"id":id,"allow":allow,"by":by}}),
            ))
            .map_err(|e| e.to_string())?;
        if self.state.lock().unwrap().approvals.is_empty() {
            let _ = self
                .log
                .append(Event::new(json!({"state":{"state":"CHAT_STATE_WORKING"}})));
        }
        self.changed.notify_all();
        Ok(())
    }
    pub fn list_models(&self) -> Result<Value, String> {
        let mut cursor = String::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut models = Vec::new();
        loop {
            let params = if cursor.is_empty() {
                json!({"limit":100})
            } else {
                json!({"limit":100,"cursor":cursor})
            };
            let page = self.rpc("model/list", params, RPC_TIMEOUT)?;
            let entries = page["data"]
                .as_array()
                .ok_or("codex model/list returned no data")?;
            models.extend(entries.iter().map(|m| json!({"model":m["model"],"name":m["displayName"],
                "isDefault":m["isDefault"],"defaultEffort":m["defaultReasoningEffort"],
                "efforts":m["supportedReasoningEfforts"].as_array().map(|v|v.iter().map(|e|e["reasoningEffort"].clone()).collect::<Vec<_>>()).unwrap_or_default()})));
            cursor = page["nextCursor"].as_str().unwrap_or_default().into();
            if cursor.is_empty() {
                break;
            }
            if !seen.insert(cursor.clone()) || models.len() > 10_000 {
                return Err("codex model catalog pagination did not finish".into());
            }
        }
        Ok(json!({"supported":true,"liveUpdate":true,"models":models}))
    }
    pub fn set_selection(
        &self,
        requested_model: &str,
        requested_effort: &str,
    ) -> Result<Value, String> {
        let _op = self.operation.lock().unwrap();
        {
            let mut s = self.state.lock().unwrap();
            if s.exited {
                return Err("codex session exited".into());
            }
            if s.busy || s.compacting || !s.approvals.is_empty() {
                return Err("precondition: selection requires an idle chat".into());
            }
            s.selecting = true;
            s.settings_confirmed = false;
        }
        let result = self.set_selection_inner(requested_model, requested_effort);
        self.state.lock().unwrap().selecting = false;
        self.changed.notify_all();
        result
    }
    fn set_selection_inner(&self, model: &str, effort: &str) -> Result<Value, String> {
        let catalog = self.list_models()?;
        let models = catalog["models"].as_array().unwrap();
        let (default_model, default_effort, thread) = {
            let s = self.state.lock().unwrap();
            (
                s.default_model.clone(),
                s.default_effort.clone(),
                s.thread_id.clone(),
            )
        };
        let chosen = if model.is_empty() {
            &default_model
        } else {
            model
        };
        let entry = models
            .iter()
            .find(|m| m["model"] == chosen)
            .ok_or_else(|| format!("invalid: model {chosen:?} is not in Codex's catalog"))?;
        let chosen_effort = if effort.is_empty() {
            if model.is_empty() {
                default_effort.as_str()
            } else {
                entry["defaultEffort"].as_str().unwrap_or_default()
            }
        } else {
            effort
        };
        if !chosen_effort.is_empty()
            && !entry["efforts"]
                .as_array()
                .is_some_and(|list| list.iter().any(|e| e == chosen_effort))
        {
            return Err(format!(
                "invalid: model {chosen:?} does not advertise effort {chosen_effort:?}"
            ));
        }
        let mut params = json!({"threadId":thread,"model":chosen});
        if !chosen_effort.is_empty() {
            params["effort"] = json!(chosen_effort);
        }
        self.rpc("thread/settings/update", params, RPC_TIMEOUT)?;
        let deadline = Instant::now() + RPC_TIMEOUT;
        let mut s = self.state.lock().unwrap();
        while !s.settings_confirmed {
            if s.exited {
                return Err("codex exited before confirming settings".into());
            }
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return Err(
                    "codex settings acknowledged but effective values were not confirmed".into(),
                );
            }
            (s, _) = self.changed.wait_timeout(s, remain).unwrap();
        }
        s.requested_model = model.into();
        s.requested_effort = effort.into();
        drop(s);
        let status = self.status();
        self.log
            .append(Event::new(json!({"selection":status["settings"]})))
            .map_err(|e| e.to_string())?;
        Ok(status)
    }
    pub fn compact(&self) -> Result<Value, String> {
        let _op = self.operation.lock().unwrap();
        let thread = {
            let mut s = self.state.lock().unwrap();
            if s.exited {
                return Err("codex session exited".into());
            }
            if s.busy || s.compacting || s.selecting || !s.approvals.is_empty() {
                return Err("precondition: compaction requires an idle chat".into());
            }
            s.compacting = true;
            s.compaction = None;
            s.compact_candidate = None;
            s.compact_post = 0;
            s.thread_id.clone()
        };
        if let Err(error) = self.rpc(
            "thread/compact/start",
            json!({"threadId":thread}),
            RPC_TIMEOUT,
        ) {
            self.state.lock().unwrap().compacting = false;
            return Err(format!("codex compact: {error}"));
        }
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut s = self.state.lock().unwrap();
        loop {
            if let Some(result) = s.compaction.take() {
                return result;
            }
            if s.exited {
                return Err("codex exited during compaction".into());
            }
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return Err("codex compaction timed out".into());
            }
            (s, _) = self.changed.wait_timeout(s, remain).unwrap();
        }
    }
    pub fn append_report(
        &self,
        status: &str,
        summary: &str,
        brief_seq: u64,
    ) -> Result<Value, String> {
        let _gate = self.event_gate.lock().unwrap();
        let seq = self.log.last() + 1;
        let result = json!({"status":status,"summary":summary,"time":chrono::Utc::now().to_rfc3339(),
            "chatSeq":seq.to_string(),"briefSeq":brief_seq.to_string()});
        let stored = self
            .log
            .append(Event::new(json!({"report":result})))
            .map_err(|e| e.to_string())?;
        if stored != seq {
            return Err("chat log sequence changed while reporting".into());
        }
        Ok(result)
    }
    fn read_stdout(&self, output: impl Read) {
        let mut reader = BufReader::new(output);
        loop {
            let mut line = Vec::new();
            match reader
                .by_ref()
                .take((MAX_LINE + 1) as u64)
                .read_until(b'\n', &mut line)
            {
                Ok(0) => break,
                Ok(_) if line.len() > MAX_LINE || !line.ends_with(b"\n") => {
                    self.log_error("codex emitted an oversized or unterminated RPC line");
                    self.stop();
                    break;
                }
                Ok(_) => match serde_json::from_slice::<Value>(&line) {
                    Ok(message) => self.handle(&message),
                    Err(e) => self.log_error(&format!("invalid codex RPC message: {e}")),
                },
                Err(_) => break,
            }
        }
    }
    fn handle(&self, message: &Value) {
        let _gate = self.event_gate.lock().unwrap();
        if let Some(method) = message["method"].as_str() {
            if message.get("id").is_some() {
                self.handle_request(message, method);
            } else {
                self.handle_note(method, &message["params"]);
            }
        } else if let Some(id) = message["id"].as_u64() {
            let mut s = self.state.lock().unwrap();
            s.replies.insert(id, message.clone());
            self.changed.notify_all();
        }
    }
    fn handle_request(&self, message: &Value, method: &str) {
        let (kind, summary) = match method {
            "item/commandExecution/requestApproval" => (
                "command",
                message["params"]["command"]
                    .as_str()
                    .or_else(|| message["params"]["reason"].as_str())
                    .unwrap_or_default(),
            ),
            "item/fileChange/requestApproval" => (
                "file_change",
                message["params"]["reason"]
                    .as_str()
                    .or_else(|| message["params"]["grantRoot"].as_str())
                    .unwrap_or_default(),
            ),
            "item/permissions/requestApproval" => (
                "permissions",
                message["params"]["reason"].as_str().unwrap_or_default(),
            ),
            _ => {
                let _ = self.write(&json!({"id":message["id"],"error":{"code":-32601,"message":format!("request not supported by driver: {method}")}}));
                self.log_error(&format!("unsupported codex server request: {method}"));
                return;
            }
        };
        let id = match &message["id"] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let summary = summary.chars().take(300).collect::<String>();
        self.state.lock().unwrap().approvals.insert(
            id.clone(),
            Approval {
                rpc_id: message["id"].clone(),
                method: method.into(),
                params: message["params"].clone(),
            },
        );
        let _ = self.log.append(Event::new(
            json!({"approval":{"id":id,"kind":kind,"summary":summary}}),
        ));
        let _ = self.log.append(Event::new(
            json!({"state":{"state":"CHAT_STATE_AWAITING_APPROVAL"}}),
        ));
        self.changed.notify_all();
    }
    fn handle_note(&self, method: &str, p: &Value) {
        let thread = p["threadId"].as_str().unwrap_or_default();
        if !thread.is_empty() && thread != self.state.lock().unwrap().thread_id {
            return;
        }
        match method {
            "thread/settings/updated" => {
                if let Some(model) = p["threadSettings"]["model"].as_str() {
                    let mut s = self.state.lock().unwrap();
                    s.model = model.into();
                    s.effort = p["threadSettings"]["effort"]
                        .as_str()
                        .unwrap_or_default()
                        .into();
                    s.settings_confirmed = true;
                    self.changed.notify_all();
                }
            }
            "model/rerouted" => {
                if let Some(model) = p["toModel"].as_str() {
                    let mut s = self.state.lock().unwrap();
                    s.model = model.into();
                    s.effort.clear();
                    let settings = self.settings_value(&s);
                    drop(s);
                    let _ = self.log.append(Event::new(json!({"selection":settings})));
                }
            }
            "turn/started" => {
                if let Some(id) = p["turn"]["id"].as_str() {
                    self.state.lock().unwrap().turn_id = id.into();
                }
            }
            "item/agentMessage/delta"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta" => {
                let thinking = method != "item/agentMessage/delta";
                let id = p["itemId"].as_str().unwrap_or(if thinking {
                    "codex-reasoning"
                } else {
                    "codex-message"
                });
                if let Some(delta) = p["delta"].as_str() {
                    let _ = self.log.append_text(id, thinking, delta);
                }
            }
            "item/started" => {
                let item = &p["item"];
                let kind = item["type"].as_str().unwrap_or_default();
                if !matches!(
                    kind,
                    "userMessage" | "agentMessage" | "reasoning" | "contextCompaction"
                ) {
                    let name = item["tool"].as_str().unwrap_or(kind);
                    let input = if let Some(command) = item["command"].as_str() {
                        json!({"command":command})
                    } else {
                        json!({})
                    };
                    let _ = self.log.append(Event::new(json!({"toolCall":{"id":item["id"],"name":name,"inputJson":input.to_string()}})));
                }
            }
            "item/completed" => {
                let item = &p["item"];
                match item["type"].as_str().unwrap_or_default() {
                    "commandExecution" => {
                        let output = item["aggregatedOutput"].as_str().unwrap_or_default();
                        let truncated = output.len() > MAX_TOOL_OUTPUT;
                        let output = String::from_utf8_lossy(
                            &output.as_bytes()[..output.len().min(MAX_TOOL_OUTPUT)],
                        );
                        let failed = item["status"] == "failed"
                            || item["exitCode"].as_i64().is_some_and(|c| c != 0);
                        let _ = self.log.append(Event::new(json!({"toolResult":{"toolCallId":item["id"],"output":output,"isError":failed,"truncated":truncated}})));
                    }
                    "contextCompaction" => {
                        let mut s = self.state.lock().unwrap();
                        let before = s.context_tokens;
                        let after = s.compact_post;
                        s.context_tokens = after;
                        let native = !s.compacting;
                        let result = json!({"tokensBefore":before.to_string(),"tokensAfter":after.to_string(),"native":native});
                        if s.compacting {
                            s.compact_candidate = Some(result.clone());
                        }
                        drop(s);
                        let _ = self.log.append(Event::new(json!({"compaction":result})));
                    }
                    _ => {}
                }
            }
            "thread/tokenUsage/updated" => {
                let last = &p["tokenUsage"]["last"];
                let input = last["inputTokens"].as_u64().unwrap_or_default();
                let output = last["outputTokens"].as_u64().unwrap_or_default();
                let cached = last["cachedInputTokens"].as_u64().unwrap_or_default();
                let mut s = self.state.lock().unwrap();
                s.compact_post = last["totalTokens"].as_u64().unwrap_or_default();
                if input > 0 {
                    s.context_tokens = input;
                    s.context_window = p["tokenUsage"]["modelContextWindow"]
                        .as_u64()
                        .unwrap_or(s.context_window);
                    let model = s.model.clone();
                    let window = s.context_window;
                    drop(s);
                    let _ = self.log.append(Event::new(json!({"usage":{"contextTokens":input.to_string(),"contextWindow":window.to_string(),
                        "inputTokens":input.saturating_sub(cached).to_string(),"outputTokens":output.to_string(),
                        "cacheReadTokens":cached.to_string(),"cacheWriteTokens":last["cacheWriteInputTokens"].as_u64().unwrap_or_default().to_string(),
                        "model":model,"provider":"openai"}})));
                }
            }
            "turn/completed" => {
                let mut s = self.state.lock().unwrap();
                s.turn_id.clear();
                if s.compacting {
                    if p["turn"]["status"] == "failed" {
                        s.compaction = Some(Err(p["turn"]["error"]["message"]
                            .as_str()
                            .unwrap_or("codex compaction failed")
                            .into()));
                    } else if s.compaction.is_none() {
                        let result = s.compact_candidate.take().unwrap_or_else(||json!({"tokensBefore":s.context_tokens.to_string(),"tokensAfter":s.compact_post.to_string(),"native":false}));
                        s.compaction = Some(Ok(result));
                    }
                    s.compacting = false;
                    self.changed.notify_all();
                    return;
                }
                s.busy = false;
                s.approvals.clear();
                self.changed.notify_all();
                drop(s);
                let status = p["turn"]["status"].as_str().unwrap_or("completed");
                if status == "failed" {
                    self.log_error(
                        p["turn"]["error"]["message"]
                            .as_str()
                            .unwrap_or("codex turn failed"),
                    );
                }
                let stop = match status {
                    "interrupted" => "cancelled",
                    "failed" => "error",
                    _ => "end_turn",
                };
                let _ = self
                    .log
                    .append(Event::new(json!({"settled":{"stopReason":stop}})));
                let _ = self
                    .log
                    .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
            }
            _ => {}
        }
    }
    fn settings_value(&self, s: &State) -> Value {
        json!({"requested":{"model":s.requested_model,"effort":s.requested_effort},
            "effective":{"model":s.model,"effort":s.effort},
            "nativeDefaults":{"model":s.default_model,"effort":s.default_effort}})
    }
    fn log_error(&self, message: &str) {
        let _ = self
            .log
            .append(Event::new(json!({"error":{"text":message}})));
    }
}
