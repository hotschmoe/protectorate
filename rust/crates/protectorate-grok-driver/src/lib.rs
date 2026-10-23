//! Grok's Agent Client Protocol over `grok agent stdio`.
//!
//! The sandbox command is the absolute `grok` binary plus [`stdio_args`]:
//! `agent stdio`. `cwd` is the workspace path that process sees. The driver
//! sends `initialize`, `session/new` or `session/load`, then the session-scoped
//! `/always-approve` slash command (`off` unless `autonomous`). Updates replayed
//! by `session/load` are ignored until the first [`Session::send`]. Compaction
//! and live model selection are refused: neither ACP control is verified.

use protectorate_chatlog::{Event, Log};
use protectorate_sandbox::{
    runtime::{self, KillHandle, LaunchOptions, StdioMode},
    PlanKind, ValidatedPlan,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 8 << 20;
const MAX_TOOL_OUTPUT: usize = 16 << 10;
const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const GATE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct Pending {
    rpc_id: Value,
    options: Vec<Value>,
}

#[derive(Default)]
struct State {
    exited: bool,
    live: bool,
    busy: bool,
    next_id: u64,
    prompt_id: Option<u64>,
    per_call: bool,
    replies: BTreeMap<u64, Value>,
    approvals: BTreeMap<String, Pending>,
    session_id: String,
    context_tokens: u64,
    context_window: u64,
}

enum Stop {
    Sandbox(KillHandle),
    Pid(u32),
}

impl Stop {
    fn stop(&self) {
        match self {
            Self::Sandbox(kill) => kill.kill(),
            Self::Pid(pid) => signal(*pid),
        }
    }
}

pub struct Session {
    stdin: Mutex<ChildStdin>,
    event_gate: Mutex<()>,
    state: Mutex<State>,
    changed: Condvar,
    stop: Stop,
    log: Arc<Log>,
}

/// Arguments after the `grok` executable. `grok agent stdio` is the ACP session.
/// `--always-approve` only forces approval on, and `--permission-mode` is not a
/// stdio flag; the slash command is the verified per-session gate.
pub fn stdio_args() -> [&'static str; 2] {
    ["agent", "stdio"]
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
        let cwd = options
            .working_directory
            .to_str()
            .filter(|cwd| !cwd.is_empty())
            .ok_or("grok working directory must be a non-empty UTF-8 path")?
            .to_owned();
        let mut child = match plan.kind() {
            PlanKind::IntentionallyUnsandboxedHarness => {
                runtime::spawn_direct_harness(plan, &options)
            }
            _ => runtime::spawn(plan, &options),
        }
        .map_err(|error| error.to_string())?;
        let stdin = child.take_stdin().ok_or("sandbox did not supply stdin")?;
        let stdout = child.take_stdout().ok_or("sandbox did not supply stdout")?;
        let stderr = child.take_stderr().ok_or("sandbox did not supply stderr")?;
        let session = Arc::new(Self {
            stdin: Mutex::new(stdin),
            event_gate: Mutex::new(()),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            stop: Stop::Sandbox(child.kill_handle()),
            log,
        });
        let waiter = Arc::clone(&session);
        thread::spawn(move || {
            waiter.note_exit(&child.wait());
        });
        drive(session, stdout, stderr, cwd, resume, autonomous)
    }

    /// Drive an already started `grok agent stdio` process with piped stdio.
    /// `cwd` is the directory that process uses for `session/new` and `session/load`.
    pub fn attach(
        mut child: Child,
        log: Arc<Log>,
        cwd: &str,
        resume: Option<&str>,
        autonomous: bool,
    ) -> Result<Arc<Self>, String> {
        if cwd.is_empty() {
            return Err("grok working directory is required".into());
        }
        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or("grok process did not supply stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("grok process did not supply stdout")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("grok process did not supply stderr")?;
        let session = Arc::new(Self {
            stdin: Mutex::new(stdin),
            event_gate: Mutex::new(()),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            stop: Stop::Pid(pid),
            log,
        });
        let waiter = Arc::clone(&session);
        thread::spawn(move || {
            waiter.note_exit(&child.wait());
        });
        drive(session, stdout, stderr, cwd.to_owned(), resume, autonomous)
    }

    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        let name = if state.exited {
            "CHAT_STATE_EXITED"
        } else if !state.approvals.is_empty() {
            "CHAT_STATE_AWAITING_APPROVAL"
        } else if state.busy {
            "CHAT_STATE_WORKING"
        } else {
            "CHAT_STATE_IDLE"
        };
        json!({
            "state": name,
            "harnessSessionId": state.session_id,
            "model": "",
            "provider": "xai",
            "reasoningEffort": "",
            "contextTokens": state.context_tokens.to_string(),
            "contextWindow": state.context_window.to_string(),
            "lastSeq": self.log.last().to_string(),
            "cache": self.log.cache(),
            "settings": {
                "requested": {"model": "", "effort": ""},
                "effective": {"model": "", "effort": ""},
                "nativeDefaults": {"model": "", "effort": ""}
            }
        })
    }

    pub fn is_exited(&self) -> bool {
        self.state.lock().unwrap().exited
    }

    pub fn is_idle(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.exited && !state.busy && state.approvals.is_empty()
    }

    pub fn stop(&self) {
        self.log.note_stop();
        self.stop.stop();
    }

    pub fn wait_exited(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap();
        while !state.exited {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        true
    }

    pub fn send(&self, text: &str, from: &str, request_id: &str) -> Result<u64, String> {
        if text.is_empty() || text.len() > 1 << 20 {
            return Err("invalid: message must contain 1 to 1048576 bytes".into());
        }
        let _gate = self.event_gate.lock().unwrap();
        let (id, session_id) = {
            let mut state = self.state.lock().unwrap();
            if state.exited {
                return Err("grok session exited".into());
            }
            if state.busy || !state.approvals.is_empty() {
                return Err("precondition: Grok requires an idle chat for Send".into());
            }
            state.next_id += 1;
            (state.next_id, state.session_id.clone())
        };
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": text}]
            }
        }))?;
        {
            let mut state = self.state.lock().unwrap();
            state.live = true;
            state.busy = true;
            state.per_call = false;
            state.prompt_id = Some(id);
        }
        match self.log.append(Event::new(
            json!({"user":{"text":text,"from":from,"requestId":request_id}}),
        )) {
            Ok(seq) => Ok(seq),
            Err(error) => {
                self.stop();
                Err(error.to_string())
            }
        }
    }

    pub fn interrupt(&self) -> Result<(), String> {
        let _gate = self.event_gate.lock().unwrap();
        let (session_id, pending) = {
            let mut state = self.state.lock().unwrap();
            if state.exited || !state.busy {
                return Ok(());
            }
            (
                state.session_id.clone(),
                std::mem::take(&mut state.approvals),
            )
        };
        self.log.note_interrupt();
        for pending in pending.values() {
            let _ = self.write_json(&json!({
                "jsonrpc": "2.0",
                "id": pending.rpc_id,
                "result": {"outcome": {"outcome": "cancelled"}}
            }));
        }
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {"sessionId": session_id}
        }))
    }

    pub fn respond(&self, id: &str, allow: bool, _note: &str, by: &str) -> Result<(), String> {
        let _gate = self.event_gate.lock().unwrap();
        let pending = self
            .state
            .lock()
            .unwrap()
            .approvals
            .get(id)
            .cloned()
            .ok_or_else(|| "invalid: approval is not pending".to_owned())?;
        let outcome = pick_outcome(&pending.options, allow);
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": pending.rpc_id,
            "result": {"outcome": outcome}
        }))?;
        self.state.lock().unwrap().approvals.remove(id);
        self.log
            .append(Event::new(
                json!({"approvalResolved":{"id":id,"allow":allow,"by":by}}),
            ))
            .map_err(|error| error.to_string())?;
        self.changed.notify_all();
        Ok(())
    }

    pub fn list_models(&self) -> Result<Value, String> {
        Ok(json!({
            "supported": false,
            "reason": "Grok ACP does not expose a native model catalog"
        }))
    }

    pub fn set_selection(&self, _model: &str, _effort: &str) -> Result<Value, String> {
        Err("precondition: Grok ACP does not support live model selection".into())
    }

    pub fn compact(&self) -> Result<Value, String> {
        let state = self.state.lock().unwrap();
        if state.exited {
            return Err("grok session exited".into());
        }
        if state.busy || !state.approvals.is_empty() {
            return Err("precondition: compaction requires an idle chat".into());
        }
        Err("unimplemented: Grok ACP compaction is not verified".into())
    }

    fn handshake(&self, cwd: &str, resume: Option<&str>, autonomous: bool) -> Result<(), String> {
        let initialized = self.rpc(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": {"name": "protectorate", "version": "0"}
            }),
            RPC_TIMEOUT,
        )?;
        if initialized["protocolVersion"].as_u64() != Some(1) {
            return Err(format!(
                "initialize protocolVersion {}",
                initialized["protocolVersion"]
            ));
        }
        let (method, params) = if let Some(session_id) = resume {
            (
                "session/load",
                json!({"sessionId": session_id, "cwd": cwd, "mcpServers": []}),
            )
        } else {
            ("session/new", json!({"cwd": cwd, "mcpServers": []}))
        };
        let opened = self.rpc(method, params, RPC_TIMEOUT)?;
        let session_id = if let Some(expected) = resume {
            if opened["sessionId"]
                .as_str()
                .is_some_and(|got| got != expected)
            {
                return Err("session/load returned a different session id".into());
            }
            expected.to_owned()
        } else {
            opened["sessionId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or("session/new returned no session id")?
                .to_owned()
        };
        self.state.lock().unwrap().session_id = session_id.clone();
        let gate = if autonomous {
            "/always-approve on"
        } else {
            "/always-approve off"
        };
        self.rpc(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": gate}]
            }),
            GATE_TIMEOUT,
        )
        .map_err(|error| format!("{gate}: {error}"))?;
        self.log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn rpc(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = {
            let mut state = self.state.lock().unwrap();
            if state.exited {
                return Err("grok session exited".into());
            }
            state.next_id += 1;
            state.next_id
        };
        self.write_json(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            let mut state = self.state.lock().unwrap();
            if let Some(reply) = state.replies.remove(&id) {
                if let Some(error) = reply.get("error") {
                    let code = error["code"].as_i64().unwrap_or(0);
                    let message = error["message"].as_str().unwrap_or("RPC failed");
                    return Err(format!("rpc error {code}: {message}"));
                }
                return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
            }
            if state.exited {
                return Err("grok exited before replying".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                drop(state);
                self.stop();
                return Err(format!("{method} timed out"));
            }
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            drop(next);
        }
    }

    fn write_json(&self, value: &Value) -> Result<(), String> {
        let mut stdin = self.stdin.lock().unwrap();
        serde_json::to_writer(&mut *stdin, value).map_err(|error| error.to_string())?;
        stdin.write_all(b"\n").map_err(|error| error.to_string())?;
        stdin.flush().map_err(|error| error.to_string())
    }

    fn note_exit<E: std::fmt::Display>(&self, result: &Result<std::process::ExitStatus, E>) {
        let _gate = self.event_gate.lock().unwrap();
        let _ = self.log.append_exit("grok", result);
        let _ = self
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_EXITED"}})));
        let _ = self.log.close();
        let mut state = self.state.lock().unwrap();
        state.exited = true;
        state.busy = false;
        state.prompt_id = None;
        self.changed.notify_all();
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
                    self.log_error("grok emitted an oversized or unterminated ACP line");
                    self.stop();
                    break;
                }
                Ok(_) => match serde_json::from_slice::<Value>(trim_line(&line)) {
                    Ok(message) => self.handle(&message),
                    Err(error) => self.log_error(&format!("invalid grok ACP message: {error}")),
                },
                Err(_) => break,
            }
        }
    }

    fn handle(&self, message: &Value) {
        let _gate = self.event_gate.lock().unwrap();
        let method = message["method"].as_str().unwrap_or("");
        let has_id = message.get("id").is_some() && !message["id"].is_null();
        if !method.is_empty() && has_id {
            self.on_request(message);
        } else if !method.is_empty() {
            self.on_note(method, &message["params"]);
        } else if let Some(id) = message.get("id").and_then(rpc_id) {
            self.on_response(id, message);
        }
    }

    fn on_response(&self, id: u64, message: &Value) {
        let prompt = {
            let mut state = self.state.lock().unwrap();
            if state.live && state.prompt_id == Some(id) {
                state.prompt_id = None;
                state.busy = false;
                state.approvals.clear();
                Some(state.per_call)
            } else {
                state.replies.insert(id, message.clone());
                self.changed.notify_all();
                None
            }
        };
        let Some(per_call) = prompt else {
            return;
        };
        if let Some(error) = message.get("error") {
            let code = error["code"].as_i64().unwrap_or(0);
            let text = error["message"].as_str().unwrap_or("session/prompt failed");
            self.log_error(&format!("session/prompt: rpc error {code}: {text}"));
            self.settle("error");
            return;
        }
        let result = &message["result"];
        if !per_call {
            if let Some(usage) = result
                .get("_meta")
                .and_then(|meta| meta.get("usage"))
                .filter(|usage| usage.is_object())
            {
                let input = json_u64(&usage["inputTokens"]);
                self.emit_usage(
                    input,
                    input.saturating_sub(json_u64(&usage["cachedReadTokens"])),
                    json_u64(&usage["outputTokens"]),
                    json_u64(&usage["cachedReadTokens"]),
                    0,
                );
            }
        }
        let stop = result["stopReason"]
            .as_str()
            .filter(|stop| !stop.is_empty())
            .unwrap_or("end_turn");
        self.settle(stop);
    }

    fn on_note(&self, method: &str, params: &Value) {
        let announced = params["sessionId"].as_str().unwrap_or("");
        let (current, live) = {
            let state = self.state.lock().unwrap();
            (state.session_id.clone(), state.live)
        };
        if !live || (!announced.is_empty() && !current.is_empty() && announced != current) {
            return;
        }
        match method {
            "session/update" => self.on_update(&params["update"]),
            "_x.ai/session_notification" => self.on_provider_usage(&params["update"]["usage"]),
            _ => {}
        }
    }

    fn on_update(&self, update: &Value) {
        match update["sessionUpdate"].as_str().unwrap_or("") {
            "agent_message_chunk" | "agent_thought_chunk" => {
                let thinking = update["sessionUpdate"] == "agent_thought_chunk";
                if let Some(text) = update["content"]["text"].as_str() {
                    let id = update["messageId"].as_str().unwrap_or("");
                    let _ = self.log.append_text(id, thinking, text);
                }
            }
            "tool_call" => {
                let _ = self.log.append(Event::new(json!({"toolCall":{
                    "id": update["toolCallId"],
                    "name": update["title"],
                    "inputJson": update["rawInput"].to_string()
                }})));
            }
            "tool_call_update" => {
                let status = update["status"].as_str().unwrap_or("");
                if status != "completed" && status != "failed" {
                    return;
                }
                let mut output = tool_output(update);
                let truncated = output.len() > MAX_TOOL_OUTPUT;
                if truncated {
                    output =
                        String::from_utf8_lossy(&output.as_bytes()[..MAX_TOOL_OUTPUT]).into_owned();
                }
                let _ = self.log.append(Event::new(json!({"toolResult":{
                    "toolCallId": update["toolCallId"],
                    "output": output,
                    "isError": status == "failed",
                    "truncated": truncated
                }})));
            }
            _ => {}
        }
    }

    fn on_provider_usage(&self, usage: &Value) {
        if !usage.is_object() {
            return;
        }
        let input = json_u64(&usage["input_tokens"]);
        let output = json_u64(&usage["output_tokens"]);
        let read = json_u64(&usage["cache_read_input_tokens"]);
        let write = json_u64(&usage["cache_creation_input_tokens"]);
        if input.saturating_add(read).saturating_add(write) == 0 {
            return;
        }
        self.state.lock().unwrap().per_call = true;
        self.emit_usage(
            input.saturating_add(read).saturating_add(write),
            input,
            output,
            read,
            write,
        );
    }

    fn on_request(&self, message: &Value) {
        let method = message["method"].as_str().unwrap_or("");
        if method != "session/request_permission" {
            let _ = self.write_json(&json!({
                "jsonrpc": "2.0",
                "id": message["id"],
                "error": {"code": -32601, "message": format!("method not supported: {method}")}
            }));
            return;
        }
        if !message["params"].is_object() {
            let _ = self.write_json(&json!({
                "jsonrpc": "2.0",
                "id": message["id"],
                "error": {"code": -32602, "message": "invalid params"}
            }));
            return;
        }
        let tool = &message["params"]["toolCall"];
        let options = message["params"]["options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let id = approval_key(&message["id"]);
        let summary = {
            let title = tool["title"].as_str().unwrap_or("");
            if title.is_empty() {
                tool["kind"].as_str().unwrap_or("").to_owned()
            } else {
                truncate(title, 300)
            }
        };
        self.state.lock().unwrap().approvals.insert(
            id.clone(),
            Pending {
                rpc_id: message["id"].clone(),
                options,
            },
        );
        let _ = self.log.append(Event::new(json!({"approval":{
            "id": id,
            "kind": tool["kind"].as_str().unwrap_or(""),
            "summary": summary
        }})));
        let _ = self.log.append(Event::new(
            json!({"state":{"state":"CHAT_STATE_AWAITING_APPROVAL"}}),
        ));
        self.changed.notify_all();
    }

    fn emit_usage(&self, context: u64, input: u64, output: u64, read: u64, write: u64) {
        let window = {
            let mut state = self.state.lock().unwrap();
            state.context_tokens = context;
            state.context_window
        };
        let _ = self.log.append(Event::new(json!({"usage":{
            "contextTokens": context.to_string(),
            "contextWindow": window.to_string(),
            "inputTokens": input.to_string(),
            "outputTokens": output.to_string(),
            "cacheReadTokens": read.to_string(),
            "cacheWriteTokens": write.to_string(),
            "provider": "xai"
        }})));
    }

    fn settle(&self, stop: &str) {
        let _ = self
            .log
            .append(Event::new(json!({"settled":{"stopReason":stop}})));
        let _ = self
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
        self.changed.notify_all();
    }

    fn log_error(&self, message: &str) {
        let _ = self
            .log
            .append(Event::new(json!({"error":{"text":message}})));
    }
}

fn drive(
    session: Arc<Session>,
    stdout: impl Read + Send + 'static,
    stderr: impl Read + Send + 'static,
    cwd: String,
    resume: Option<&str>,
    autonomous: bool,
) -> Result<Arc<Session>, String> {
    let reader = Arc::clone(&session);
    thread::spawn(move || reader.read_stdout(stdout));
    thread::spawn(move || {
        let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
    });
    if let Err(error) = session.handshake(&cwd, resume, autonomous) {
        session.stop();
        let _ = session.wait_exited(Duration::from_secs(2));
        return Err(format!("open grok: {error}"));
    }
    Ok(session)
}

fn signal(pid: u32) {
    if pid == 0 {
        return;
    }
    let _ = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn trim_line(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n")
        .unwrap_or(line)
        .strip_suffix(b"\r")
        .unwrap_or(line)
}

fn rpc_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn approval_key(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn json_u64(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .unwrap_or(0)
}

fn tool_output(update: &Value) -> String {
    if let Some(items) = update["content"].as_array() {
        let parts: Vec<&str> = items
            .iter()
            .filter_map(|item| item["content"]["text"].as_str())
            .filter(|text| !text.is_empty())
            .collect();
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    match &update["rawOutput"] {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn pick_outcome(options: &[Value], allow: bool) -> Value {
    let (want, prefix) = if allow {
        ("allow_once", "allow")
    } else {
        ("reject_once", "reject")
    };
    let mut pick = None;
    for option in options {
        let kind = option["kind"].as_str().unwrap_or("");
        let id = option["optionId"].as_str().unwrap_or("");
        if id.is_empty() {
            continue;
        }
        if kind == want {
            pick = Some(id);
            break;
        }
        if pick.is_none() && kind.starts_with(prefix) {
            pick = Some(id);
        }
    }
    match pick {
        Some(id) => json!({"outcome":"selected","optionId":id}),
        None => json!({"outcome":"cancelled"}),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}
