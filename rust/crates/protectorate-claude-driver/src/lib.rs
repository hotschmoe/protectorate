//! Claude Code's stream JSON protocol inside a caller supplied sandbox.
//! Claude owns its private transcript; this crate writes the durable public
//! chat view and handles the SDK host permission exchange.
use protectorate_chatlog::{Event, Log};
use protectorate_sandbox::{
    runtime::{self, KillHandle, LaunchOptions, StdioMode},
    LineInput, PlanKind, ValidatedPlan,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    io::{BufRead, BufReader, Read},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread,
    time::{Duration, Instant},
};

/// The session state is independent flags, counters and maps for one chat; the
/// hub must still be able to read, interrupt and stop the session after a
/// panic in the reader thread, so a poisoned guard is reused.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

const MAX_LINE: usize = 8 << 20;
const MAX_TOOL_OUTPUT: usize = 16 << 10;

#[derive(Default)]
struct State {
    busy: bool,
    compacting: bool,
    exited: bool,
    session_id: String,
    model: String,
    context_window: u64,
    context_tokens: u64,
    next_control: u64,
    approvals: BTreeMap<String, Value>,
    seen_messages: HashSet<String>,
    compact_result: Option<Value>,
    compact_error: Option<String>,
}

pub struct Session {
    input: LineInput,
    event_gate: Mutex<()>,
    state: Mutex<State>,
    changed: Condvar,
    kill: KillHandle,
    log: Arc<Log>,
    autonomous: bool,
}

impl Session {
    pub fn start(
        plan: &ValidatedPlan,
        mut options: LaunchOptions,
        log: Arc<Log>,
        session_id: String,
        autonomous: bool,
    ) -> Result<Arc<Self>, String> {
        options.stdio = StdioMode::Duplex;
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
            input: LineInput::start(stdin, "claude", {
                let log = Arc::clone(&log);
                let kill = child.kill_handle();
                move |error| {
                    let _ = log.append(Event::new(
                        json!({"error":{"text":format!("write claude stdin: {error}")}}),
                    ));
                    kill.kill();
                }
            }),
            event_gate: Mutex::new(()),
            state: Mutex::new(State {
                session_id,
                ..State::default()
            }),
            changed: Condvar::new(),
            kill: child.kill_handle(),
            log,
            autonomous,
        });
        let reader = Arc::clone(&session);
        thread::spawn(move || reader.read_stdout(stdout));
        thread::spawn(move || {
            let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
        });
        let waiter = Arc::clone(&session);
        thread::spawn(move || {
            let result = child.wait();
            let _gate = lock(&waiter.event_gate);
            let _ = waiter.log.end_session("claude", &result);
            let mut state = lock(&waiter.state);
            state.exited = true;
            state.busy = false;
            waiter.changed.notify_all();
        });
        session
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})))
            .map_err(|e| e.to_string())?;
        Ok(session)
    }

    pub fn status(&self) -> Value {
        let s = lock(&self.state);
        json!({"state": if s.exited {"CHAT_STATE_EXITED"} else if !s.approvals.is_empty() {"CHAT_STATE_AWAITING_APPROVAL"}
            else if s.compacting {"CHAT_STATE_COMPACTING"} else if s.busy {"CHAT_STATE_WORKING"} else {"CHAT_STATE_IDLE"},
            "harnessSessionId":s.session_id,"model":s.model,"provider":"anthropic",
            "contextTokens":s.context_tokens.to_string(),"contextWindow":s.context_window.to_string(),
            "lastSeq":self.log.last().to_string(),"cache":self.log.cache()})
    }
    pub fn is_exited(&self) -> bool {
        lock(&self.state).exited
    }
    pub fn is_idle(&self) -> bool {
        let s = lock(&self.state);
        !s.exited && !s.busy && !s.compacting && s.approvals.is_empty()
    }
    pub fn stop(&self) {
        self.log.note_stop();
        self.kill.kill();
    }
    pub fn wait_exited(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut s = lock(&self.state);
        while !s.exited {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            (s, _) = self
                .changed
                .wait_timeout(s, remaining)
                .unwrap_or_else(PoisonError::into_inner);
        }
        true
    }
    pub fn send(&self, text: &str, from: &str, request_id: &str) -> Result<u64, String> {
        if text.is_empty() || text.len() > 1 << 20 {
            return Err("invalid: message must contain 1 to 1048576 bytes".into());
        }
        let _gate = lock(&self.event_gate);
        let mut s = lock(&self.state);
        if s.exited {
            return Err("claude session exited".into());
        }
        if s.busy || s.compacting || !s.approvals.is_empty() {
            return Err("precondition: Claude requires an idle chat for Send".into());
        }
        self.input
            .write(&json!({"type":"user","message":{"role":"user","content":text}}))?;
        s.busy = true;
        let seq = self.log.append(Event::new(
            json!({"user":{"text":text,"from":from,"requestId":request_id}}),
        ));
        drop(s);
        if seq.is_err() {
            self.stop();
        }
        seq.map_err(|e| e.to_string())
    }
    pub fn interrupt(&self) -> Result<(), String> {
        let _gate = lock(&self.event_gate);
        let mut s = lock(&self.state);
        if s.exited || !s.busy {
            return Ok(());
        }
        self.log.note_interrupt();
        s.next_control += 1;
        self.input.write(
            &json!({"type":"control_request","request_id":format!("int-{}",s.next_control),
            "request":{"subtype":"interrupt"}}),
        )
    }
    pub fn respond(&self, id: &str, allow: bool, note: &str, by: &str) -> Result<(), String> {
        let _gate = lock(&self.event_gate);
        let mut s = lock(&self.state);
        let input = s
            .approvals
            .get(id)
            .ok_or_else(|| "invalid: approval is not pending".to_owned())?;
        let response = if allow {
            json!({"behavior":"allow","updatedInput":input})
        } else {
            json!({"behavior":"deny","message":if note.is_empty() {"denied by user"} else {note}})
        };
        self.input.write(&json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":response}}))?;
        s.approvals.remove(id);
        self.log
            .append(Event::new(
                json!({"approvalResolved":{"id":id,"allow":allow,"by":by}}),
            ))
            .map_err(|e| e.to_string())?;
        self.changed.notify_all();
        Ok(())
    }
    pub fn compact(&self) -> Result<Value, String> {
        let _gate = lock(&self.event_gate);
        {
            let mut s = lock(&self.state);
            if s.exited {
                return Err("claude session exited".into());
            }
            if s.busy || s.compacting || !s.approvals.is_empty() {
                return Err("precondition: compaction requires an idle chat".into());
            }
            s.compacting = true;
            s.compact_result = None;
            s.compact_error = None;
        }
        if let Err(e) = self
            .input
            .write(&json!({"type":"user","message":{"role":"user","content":"/compact"}}))
        {
            lock(&self.state).compacting = false;
            return Err(e);
        }
        drop(_gate);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut s = lock(&self.state);
        loop {
            if let Some(error) = s.compact_error.take() {
                return Err(error);
            }
            if !s.compacting {
                if let Some(result) = s.compact_result.take() {
                    return Ok(result);
                }
                return Err("precondition: Claude did not confirm compaction".into());
            }
            if s.exited {
                return Err("claude exited during compaction".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.stop();
                return Err("claude compaction timed out".into());
            }
            (s, _) = self
                .changed
                .wait_timeout(s, remaining)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
    pub fn list_models(&self) -> Result<Value, String> {
        Ok(
            json!({"supported":false,"reason":"Claude Code stream JSON does not expose a native model catalog"}),
        )
    }
    pub fn set_selection(&self, _model: &str, _effort: &str) -> Result<Value, String> {
        Err("precondition: Claude Code stream JSON does not support live model selection".into())
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
                    let _ = self.log.append(Event::new(json!({"error":{"text":"Claude emitted an oversized or unterminated stream line"}})));
                    self.stop();
                    break;
                }
                Ok(_) => {
                    match serde_json::from_slice::<Value>(&line) {
                        Ok(value) => self.handle(&value),
                        Err(e) => {
                            let _ = self.log.append(Event::new(json!({"error":{"text":format!("invalid Claude stream JSON: {e}")}})));
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }
    fn handle(&self, value: &Value) {
        let _gate = lock(&self.event_gate);
        let kind = value["type"].as_str().unwrap_or_default();
        match kind {
            "system" => match value["subtype"].as_str().unwrap_or_default() {
                "init" => {
                    if let Some(id) = value["session_id"].as_str() {
                        let mut s = lock(&self.state);
                        if s.session_id != id {
                            drop(s);
                            self.log_error("Claude resumed a different session");
                            self.stop();
                        } else {
                            s.session_id = id.into();
                        }
                    }
                }
                "compact_boundary" => {
                    let meta = &value["compact_metadata"];
                    let event = json!({"tokensBefore":meta["pre_tokens"].as_u64().unwrap_or_default().to_string(),
                        "tokensAfter":meta["post_tokens"].as_u64().unwrap_or_default().to_string(),
                        "native":meta["trigger"] == "auto"});
                    let mut s = lock(&self.state);
                    if s.compacting {
                        s.compact_result = Some(event.clone());
                        self.changed.notify_all();
                    }
                    drop(s);
                    let _ = self.log.append(Event::new(json!({"compaction":event})));
                }
                _ => {}
            },
            "assistant" => self.on_assistant(value),
            "user" => self.on_user(value),
            "control_request" => self.on_control_request(value),
            "result" => self.on_result(value),
            _ => {}
        }
    }
    fn on_assistant(&self, value: &Value) {
        if !value["parent_tool_use_id"].is_null() {
            return;
        }
        let message = &value["message"];
        let id = message["id"].as_str().unwrap_or_default();
        if let Some(model) = message["model"].as_str() {
            lock(&self.state).model = model.into();
        }
        let usage = &message["usage"];
        if !id.is_empty() && usage.is_object() {
            let mut s = lock(&self.state);
            if s.seen_messages.insert(format!("usage:{id}")) {
                let input = usage["input_tokens"].as_u64().unwrap_or_default();
                let read = usage["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                let write = usage["cache_creation_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                s.context_tokens = input.saturating_add(read).saturating_add(write);
                let _ = self.log.append(Event::new(json!({"usage":{
                    "contextTokens":s.context_tokens.to_string(),"contextWindow":s.context_window.to_string(),
                    "inputTokens":input.to_string(),"outputTokens":usage["output_tokens"].as_u64().unwrap_or_default().to_string(),
                    "cacheReadTokens":read.to_string(),"cacheWriteTokens":write.to_string()}})));
            }
        }
        if let Some(blocks) = message["content"].as_array() {
            for (index, block) in blocks.iter().enumerate() {
                match block["type"].as_str().unwrap_or_default() {
                    "text" | "thinking" => {
                        let thinking = block["type"] == "thinking";
                        if let Some(text) =
                            block[if thinking { "thinking" } else { "text" }].as_str()
                        {
                            let _ = self.log.append_text(id, thinking, text);
                        }
                    }
                    "tool_use" => {
                        let key = format!("block:{id}:{index}:tool_use");
                        if !lock(&self.state).seen_messages.insert(key) {
                            continue;
                        }
                        let _ = self.log.append(Event::new(json!({"toolCall":{
                            "id":block["id"],"name":block["name"],"inputJson":block["input"].to_string()}})));
                    }
                    _ => {}
                }
            }
        }
    }
    fn on_user(&self, value: &Value) {
        if !value["parent_tool_use_id"].is_null() {
            return;
        }
        if let Some(blocks) = value["message"]["content"].as_array() {
            for block in blocks {
                if block["type"] != "tool_result" {
                    continue;
                }
                let output = match &block["content"] {
                    Value::String(s) => s.clone(),
                    Value::Array(parts) => parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    other => other.to_string(),
                };
                let truncated = output.len() > MAX_TOOL_OUTPUT;
                let output = if truncated {
                    String::from_utf8_lossy(&output.as_bytes()[..MAX_TOOL_OUTPUT]).into_owned()
                } else {
                    output
                };
                let _ = self.log.append(Event::new(json!({"toolResult":{
                    "toolCallId":block["tool_use_id"],"output":output,"isError":block["is_error"] == true,"truncated":truncated}})));
            }
        }
    }
    fn on_control_request(&self, value: &Value) {
        if value["request"]["subtype"] != "can_use_tool" {
            return;
        }
        let Some(id) = value["request_id"].as_str() else {
            return;
        };
        let input = value["request"]["input"].clone();
        let name = value["request"]["tool_name"].as_str().unwrap_or_default();
        let summary = summarize_tool(name, &input);
        if self.autonomous {
            let response = json!({"type":"control_response","response":{
                "subtype":"success","request_id":id,
                "response":{"behavior":"allow","updatedInput":input}}});
            if let Err(error) = self.input.write(&response) {
                self.log_error(&format!("Claude auto approval: {error}"));
                self.stop();
                return;
            }
            let _ = self.log.append(Event::new(
                json!({"approval":{"id":id,"kind":name,"summary":summary}}),
            ));
            let _ = self.log.append(Event::new(
                json!({"approvalResolved":{"id":id,"allow":true,"by":"autonomous"}}),
            ));
            return;
        }
        lock(&self.state).approvals.insert(id.into(), input.clone());
        let _ = self.log.append(Event::new(
            json!({"approval":{"id":id,"kind":name,"summary":summary}}),
        ));
        let _ = self.log.append(Event::new(
            json!({"state":{"state":"CHAT_STATE_AWAITING_APPROVAL"}}),
        ));
        self.changed.notify_all();
    }
    fn on_result(&self, value: &Value) {
        let mut s = lock(&self.state);
        if let Some(id) = value["session_id"].as_str() {
            if id != s.session_id {
                drop(s);
                self.log_error("Claude returned a different session");
                self.stop();
                return;
            }
        }
        if let Some(models) = value["modelUsage"].as_object() {
            s.context_window = models
                .values()
                .filter_map(|v| v["contextWindow"].as_u64())
                .max()
                .unwrap_or(s.context_window);
        }
        if s.compacting {
            s.compacting = false;
            if value["is_error"] == true {
                s.compact_error = Some(format!("claude compact: {}", value["result"]));
            }
            self.changed.notify_all();
            drop(s);
            let _ = self
                .log
                .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
            return;
        }
        s.busy = false;
        s.approvals.clear();
        self.changed.notify_all();
        drop(s);
        if value["is_error"] == true {
            self.log_error(value["result"].as_str().unwrap_or("Claude turn failed"));
        }
        let stop = value["stop_reason"]
            .as_str()
            .or_else(|| value["subtype"].as_str())
            .unwrap_or("end_turn");
        let _ = self
            .log
            .append(Event::new(json!({"settled":{"stopReason":stop}})));
        let _ = self
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
    }
    fn log_error(&self, message: &str) {
        let _ = self
            .log
            .append(Event::new(json!({"error":{"text":message}})));
    }
}

fn summarize_tool(name: &str, input: &Value) -> String {
    let detail = ["command", "file_path", "path", "url", "pattern"]
        .iter()
        .find_map(|key| input[*key].as_str())
        .unwrap_or_default();
    let text = if detail.is_empty() {
        format!("{name} {input}")
    } else {
        format!("{name}: {detail}")
    };
    if text.len() <= 300 {
        text
    } else {
        format!("{}...", String::from_utf8_lossy(&text.as_bytes()[..300]))
    }
}

/// UUID v4 for Claude's `--session-id`; no external UUID dependency is needed.
pub fn new_session_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        u16::from_be_bytes(bytes[4..6].try_into().unwrap()),
        u16::from_be_bytes(bytes[6..8].try_into().unwrap()),
        u16::from_be_bytes(bytes[8..10].try_into().unwrap()),
        u64::from_be_bytes([
            0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        ])
    ))
}
