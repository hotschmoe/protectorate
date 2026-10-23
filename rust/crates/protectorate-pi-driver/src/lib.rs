//! Pi's documented JSON-line RPC protocol, inside a caller-supplied sandbox.
//! The process remains alive across turns. Its private session store belongs to
//! Pi; the shared chat log is the durable, normalized view for subscribers.
use protectorate_chatlog::{Event, Log};
use protectorate_sandbox::{
    runtime::{self, KillHandle, LaunchOptions, StdioMode},
    PlanKind, ValidatedPlan,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    process::ChildStdin,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 4 << 20;
const MAX_TOOL_OUTPUT: usize = 16 << 10;

#[derive(Default)]
struct State {
    busy: bool,
    compacting: bool,
    selecting: bool,
    exited: bool,
    next_id: u64,
    next_message: u64,
    replies: BTreeMap<String, Value>,
    pending: BTreeSet<String>,
    session_id: String,
    model: String,
    provider: String,
    model_base_url: String,
    effort: String,
    default_model: String,
    default_provider: String,
    default_effort: String,
    requested_model: String,
    requested_effort: String,
    manual_compaction: bool,
    compaction_result: Option<Value>,
    suppress_compaction_event: bool,
    context_window: u64,
    context_tokens: u64,
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
        resume_session: Option<&str>,
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
        thread::spawn(move || drain_stderr(stderr));
        let waiter = Arc::clone(&session);
        thread::spawn(move || {
            let result = child.wait();
            let _gate = waiter.event_gate.lock().unwrap();
            let message = match result {
                Ok(status) => format!("pi exited: {status}"),
                Err(error) => format!("pi wait failed: {error}"),
            };
            let _ = waiter
                .log
                .append(Event::new(json!({"error":{"text":message}})));
            let _ = waiter
                .log
                .append(Event::new(json!({"state":{"state":"CHAT_STATE_EXITED"}})));
            let _ = waiter.log.close();
            let mut state = waiter.state.lock().unwrap();
            state.exited = true;
            state.busy = false;
            waiter.changed.notify_all();
        });
        // A real handshake proves the harness speaks the expected protocol.
        let data = session.command("get_state", json!({}), Duration::from_secs(15))?;
        let id = data["sessionId"].as_str().unwrap_or_default();
        if id.is_empty() {
            session.stop();
            return Err("pi get_state returned no session id".into());
        }
        if let Some(expected) = resume_session {
            if expected != id {
                session.stop();
                return Err("pi resumed a different session".into());
            }
        }
        {
            let mut state = session.state.lock().unwrap();
            state.session_id = id.into();
            state.model = data["model"]["id"].as_str().unwrap_or_default().into();
            state.provider = data["model"]["provider"]
                .as_str()
                .unwrap_or_default()
                .into();
            state.model_base_url = data["model"]["baseUrl"].as_str().unwrap_or_default().into();
            state.context_window = data["model"]["contextWindow"].as_u64().unwrap_or_default();
            state.effort = data["thinkingLevel"].as_str().unwrap_or_default().into();
            state.default_model = state.model.clone();
            state.default_provider = state.provider.clone();
            state.default_effort = state.effort.clone();
        }
        session
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})))
            .map_err(|e| e.to_string())?;
        Ok(session)
    }

    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        json!({
            "state": if state.exited {"CHAT_STATE_EXITED"} else if state.compacting {"CHAT_STATE_COMPACTING"} else if state.busy {"CHAT_STATE_WORKING"} else {"CHAT_STATE_IDLE"},
            "harnessSessionId": state.session_id,
            "model": state.model,
            "provider": state.provider,
            "reasoningEffort": state.effort,
            "contextWindow": state.context_window.to_string(),
            "contextTokens": state.context_tokens.to_string(),
            "lastSeq": self.log.last().to_string(),
            "cache": self.log.cache(),
            "settings": {
                "requested":{"model":state.requested_model,"effort":state.requested_effort},
                "effective":{"model":format_model(&state.provider,&state.model),"effort":state.effort},
                "nativeDefaults":{"model":format_model(&state.default_provider,&state.default_model),"effort":state.default_effort}
            },
        })
    }

    pub fn model_base_url(&self) -> String {
        self.state.lock().unwrap().model_base_url.clone()
    }

    pub fn is_exited(&self) -> bool {
        self.state.lock().unwrap().exited
    }

    pub fn is_idle(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.exited && !state.busy && !state.compacting && !state.selecting
    }

    pub fn list_models(&self) -> Result<Value, String> {
        if self.is_exited() {
            return Err("pi session exited".into());
        }
        let models = self.command("get_available_models", json!({}), Duration::from_secs(15))?;
        let levels = self.command(
            "get_available_thinking_levels",
            json!({}),
            Duration::from_secs(15),
        )?;
        let options = models["models"]
            .as_array()
            .ok_or("pi returned no model catalog")?;
        let efforts = levels["levels"]
            .as_array()
            .ok_or("pi returned no thinking levels")?;
        let efforts: Vec<_> = efforts.iter().filter_map(Value::as_str).collect();
        let state = self.state.lock().unwrap();
        let result = options
            .iter()
            .filter_map(|model| {
                let provider = model["provider"].as_str()?;
                let id = model["id"].as_str()?;
                let name = model["name"].as_str().unwrap_or(id);
                let reasoning = model["reasoning"].as_bool().unwrap_or(false);
                let option_efforts = if reasoning {
                    efforts.clone()
                } else {
                    vec!["off"]
                };
                Some(
                    json!({"model":format_model(provider,id),"name":name,"efforts":option_efforts,
                "defaultEffort":if reasoning {state.default_effort.as_str()} else {"off"},
                "isDefault":provider==state.default_provider && id==state.default_model}),
                )
            })
            .collect::<Vec<_>>();
        Ok(json!({"supported":true,"liveUpdate":true,"models":result}))
    }

    pub fn set_selection(
        &self,
        requested_model: &str,
        requested_effort: &str,
    ) -> Result<Value, String> {
        let _operation = self.operation.lock().unwrap();
        {
            let mut state = self.state.lock().unwrap();
            if state.exited {
                return Err("pi session exited".into());
            }
            if state.busy || state.compacting {
                return Err("precondition: selection requires an idle chat".into());
            }
            state.selecting = true;
        }
        let result = self.set_selection_inner(requested_model, requested_effort);
        self.state.lock().unwrap().selecting = false;
        result
    }

    fn set_selection_inner(
        &self,
        requested_model: &str,
        requested_effort: &str,
    ) -> Result<Value, String> {
        let (default_provider, default_model, default_effort) = {
            let state = self.state.lock().unwrap();
            (
                state.default_provider.clone(),
                state.default_model.clone(),
                state.default_effort.clone(),
            )
        };
        let (provider, model_id) = if requested_model.is_empty() {
            (default_provider.as_str(), default_model.as_str())
        } else {
            requested_model
                .split_once('/')
                .ok_or("invalid: model must be provider/id from ListModels")?
        };
        if provider.is_empty() || model_id.is_empty() {
            return Err("precondition: Pi has no native default model".into());
        }
        let effort = if requested_effort.is_empty() {
            default_effort.as_str()
        } else {
            requested_effort
        };
        let catalog = self.list_models()?;
        let models = catalog["models"]
            .as_array()
            .ok_or("pi returned no model catalog")?;
        let chosen = format_model(provider, model_id);
        let model = models
            .iter()
            .find(|model| model["model"] == chosen)
            .ok_or("invalid: requested model is not in Pi's catalog")?;
        if !effort.is_empty()
            && !model["efforts"]
                .as_array()
                .is_some_and(|list| list.iter().any(|e| e == effort))
        {
            return Err("invalid: requested effort is not available for this model".into());
        }
        let selected = self.command(
            "set_model",
            json!({"provider":provider,"modelId":model_id}),
            Duration::from_secs(15),
        )?;
        {
            let mut state = self.state.lock().unwrap();
            state.provider = provider.into();
            state.model = model_id.into();
            state.model_base_url = selected["baseUrl"].as_str().unwrap_or_default().into();
        }
        if !effort.is_empty() {
            self.command(
                "set_thinking_level",
                json!({"level":effort}),
                Duration::from_secs(15),
            )?;
        }
        let mut state = self.state.lock().unwrap();
        state.effort = effort.into();
        state.requested_model = requested_model.into();
        state.requested_effort = requested_effort.into();
        let settings = json!({"requested":{"model":requested_model,"effort":requested_effort},
            "effective":{"model":chosen,"effort":effort},
            "nativeDefaults":{"model":format_model(&state.default_provider,&state.default_model),"effort":state.default_effort}});
        drop(state);
        self.log
            .append(Event::new(json!({"settings":settings})))
            .map_err(|e| e.to_string())?;
        Ok(self.status())
    }

    pub fn compact(&self) -> Result<Value, String> {
        let _operation = self.operation.lock().unwrap();
        {
            let mut state = self.state.lock().unwrap();
            if state.exited {
                return Err("pi session exited".into());
            }
            if state.busy || state.selecting || state.compacting {
                return Err("precondition: compaction requires an idle chat".into());
            }
            state.compacting = true;
            state.manual_compaction = true;
            state.compaction_result = None;
            state.suppress_compaction_event = false;
        }
        let _ = self.log.append(Event::new(
            json!({"state":{"state":"CHAT_STATE_COMPACTING"}}),
        ));
        let result = self.command("compact", json!({}), Duration::from_secs(120));
        let mut state = self.state.lock().unwrap();
        let compaction = match result {
            Ok(data) => {
                if state.compaction_result.is_none() {
                    let (next, _) = self
                        .changed
                        .wait_timeout_while(state, Duration::from_millis(200), |s| {
                            s.compaction_result.is_none() && !s.exited
                        })
                        .unwrap();
                    state = next;
                }
                let observed = state.compaction_result.take();
                if observed.is_none() {
                    state.suppress_compaction_event = true;
                }
                let data = observed.unwrap_or(data);
                let result = json!({"tokensBefore":data["tokensBefore"].as_u64().unwrap_or_default().to_string(),
                    "tokensAfter":data["estimatedTokensAfter"].as_u64().unwrap_or_default().to_string(),"native":false});
                if state.suppress_compaction_event {
                    let _ = self.log.append(Event::new(json!({"compaction":result})));
                }
                Ok(result)
            }
            Err(error) => Err(if error.contains("Nothing to compact") {
                format!("precondition: {error}")
            } else {
                error
            }),
        };
        state.compacting = false;
        state.manual_compaction = false;
        drop(state);
        let _gate = self.event_gate.lock().unwrap();
        let _ = self
            .log
            .append(Event::new(json!({"state":{"state":"CHAT_STATE_IDLE"}})));
        compaction
    }

    /// Preserve the returned chat sequence in the same serialized event.
    pub fn append_report(
        &self,
        status: &str,
        summary: &str,
        brief_seq: u64,
    ) -> Result<Value, String> {
        let _gate = self.event_gate.lock().unwrap();
        let seq = self.log.last() + 1;
        let time = chrono_time();
        let result = json!({"status":status,"summary":summary,"time":time,
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

    /// Pi accepts `steer` during a running turn. A write failure never
    /// records a user message as delivered.
    pub fn send(&self, text: &str, from: &str, request_id: &str) -> Result<u64, String> {
        if text.is_empty() || text.len() > 1 << 20 {
            return Err("message must contain 1 to 1048576 bytes".into());
        }
        let _gate = self.event_gate.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        if state.exited {
            return Err("pi session exited".into());
        }
        if state.compacting || state.selecting {
            return Err("precondition: Pi is changing settings or compacting".into());
        }
        state.next_id += 1;
        let id = format!("c{}", state.next_id);
        let command = if state.busy { "steer" } else { "prompt" };
        self.write(&json!({"id":id,"type":command,"message":text}))?;
        state.busy = true;
        let result = self.log.append(Event::new(
            json!({"user":{"text":text,"from":from,"requestId":request_id}}),
        ));
        drop(state);
        if result.is_err() {
            self.stop();
        }
        result.map_err(|e| e.to_string())
    }

    pub fn interrupt(&self) -> Result<(), String> {
        let state = self.state.lock().unwrap();
        if state.exited || !state.busy {
            return Ok(());
        }
        self.write(&json!({"type":"abort"}))
    }

    pub fn stop(&self) {
        self.kill.kill();
    }

    pub fn wait_exited(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap();
        while !state.exited {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            (state, _) = self.changed.wait_timeout(state, remaining).unwrap();
        }
        true
    }

    fn write(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self.stdin.lock().unwrap();
        serde_json::to_writer(&mut *stdin, message).map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())
    }

    fn command(&self, kind: &str, fields: Value, timeout: Duration) -> Result<Value, String> {
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        let id = format!("c{}", state.next_id);
        state.pending.insert(id.clone());
        let mut command = fields.as_object().cloned().unwrap_or_default();
        command.insert("id".into(), json!(id));
        command.insert("type".into(), json!(kind));
        self.write(&Value::Object(command))?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(reply) = state.replies.remove(&id) {
                state.pending.remove(&id);
                if reply["success"] == true {
                    return Ok(reply["data"].clone());
                }
                return Err(format!(
                    "pi {kind}: {}",
                    reply["error"].as_str().unwrap_or("failed")
                ));
            }
            if state.exited {
                state.pending.remove(&id);
                return Err("pi exited before replying".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.pending.remove(&id);
                self.stop();
                return Err(format!("pi {kind} timed out"));
            }
            (state, _) = self.changed.wait_timeout(state, remaining).unwrap();
        }
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
                    let _ = self.log.append(Event::new(json!({"error":{"text":"pi emitted an oversized or unterminated RPC line"}})));
                    self.stop();
                    break;
                }
                Ok(_) => {
                    match serde_json::from_slice::<Value>(&line) {
                        Ok(message) => self.handle(&message),
                        Err(error) => {
                            let _ = self.log.append(Event::new(json!({"error":{"text":format!("invalid pi RPC message: {error}")}})));
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn handle(&self, message: &Value) {
        let _gate = self.event_gate.lock().unwrap();
        let kind = message["type"].as_str().unwrap_or_default();
        let event = match kind {
            "response" => {
                let id = message["id"].as_str().unwrap_or_default();
                if !id.is_empty() {
                    let mut state = self.state.lock().unwrap();
                    if state.pending.contains(id) {
                        state.replies.insert(id.into(), message.clone());
                        self.changed.notify_all();
                    }
                }
                if message["success"] == false && message["command"] == "prompt" {
                    let mut state = self.state.lock().unwrap();
                    state.busy = false;
                    Some(
                        json!({"error":{"text":message["error"].as_str().unwrap_or("pi prompt failed")}}),
                    )
                } else {
                    None
                }
            }
            "message_start" => {
                if message["message"]["role"] == "assistant" {
                    self.state.lock().unwrap().next_message += 1;
                }
                None
            }
            "message_update" => {
                let part = &message["assistantMessageEvent"];
                let thinking = part["type"] == "thinking_delta";
                if part["type"] == "text_delta" || thinking {
                    let id = format!("pi-{}", self.state.lock().unwrap().next_message);
                    let _ = self.log.append_text(
                        &id,
                        thinking,
                        part["delta"].as_str().unwrap_or_default(),
                    );
                }
                None
            }
            "tool_execution_start" => Some(json!({"toolCall":{
                "id":message["toolCallId"],"name":message["toolName"],
                "inputJson":message["args"].to_string()
            }})),
            "tool_execution_end" => {
                let output = message["result"]["content"]
                    .as_array()
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|part| part["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_else(|| message["result"].to_string());
                let truncated = output.len() > MAX_TOOL_OUTPUT;
                let output = if truncated {
                    String::from_utf8_lossy(&output.as_bytes()[..MAX_TOOL_OUTPUT]).into_owned()
                } else {
                    output
                };
                Some(
                    json!({"toolResult":{"toolCallId":message["toolCallId"],"output":output,"isError":message["isError"] == true,"truncated":truncated}}),
                )
            }
            "message_end" => {
                if message["message"]["role"] == "assistant" {
                    let mut state = self.state.lock().unwrap();
                    if message["message"]["model"]
                        .as_str()
                        .is_some_and(|model| model != state.model)
                        || message["message"]["provider"]
                            .as_str()
                            .is_some_and(|provider| provider != state.provider)
                    {
                        state.model_base_url.clear();
                    }
                    if let Some(model) = message["message"]["model"].as_str() {
                        state.model = model.into();
                    }
                    if let Some(provider) = message["message"]["provider"].as_str() {
                        state.provider = provider.into();
                    }
                    let usage = &message["message"]["usage"];
                    state.context_tokens = usage["totalTokens"].as_u64().unwrap_or_default();
                    Some(json!({"usage":{
                        "contextTokens":state.context_tokens.to_string(),
                        "contextWindow":state.context_window.to_string(),
                        "inputTokens":usage["input"].as_u64().unwrap_or_default().to_string(),
                        "outputTokens":usage["output"].as_u64().unwrap_or_default().to_string(),
                        "cacheReadTokens":usage["cacheRead"].as_u64().unwrap_or_default().to_string(),
                        "cacheWriteTokens":usage["cacheWrite"].as_u64().unwrap_or_default().to_string()
                    }}))
                } else {
                    None
                }
            }
            "agent_settled" => {
                self.state.lock().unwrap().busy = false;
                self.changed.notify_all();
                let stop = if message["aborted"] == true {
                    "cancelled"
                } else {
                    "end_turn"
                };
                Some(json!({"settled":{"stopReason":stop}}))
            }
            "compaction_end" => {
                let data = &message["result"];
                if data.is_null() {
                    None
                } else {
                    let mut state = self.state.lock().unwrap();
                    if state.suppress_compaction_event {
                        state.suppress_compaction_event = false;
                        None
                    } else {
                        state.compaction_result = Some(data.clone());
                        self.changed.notify_all();
                        Some(
                            json!({"compaction":{"tokensBefore":data["tokensBefore"].as_u64().unwrap_or_default().to_string(),
                            "tokensAfter":data["estimatedTokensAfter"].as_u64().unwrap_or_default().to_string(),
                            "native":!state.manual_compaction}}),
                        )
                    }
                }
            }
            _ => None,
        };
        if let Some(payload) = event {
            let _ = self.log.append(Event::new(payload));
        }
    }
}

fn format_model(provider: &str, id: &str) -> String {
    if provider.is_empty() || id.is_empty() {
        String::new()
    } else {
        format!("{provider}/{id}")
    }
}
fn chrono_time() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

fn drain_stderr(stderr: impl Read) {
    // Never let a noisy harness block on its stderr pipe. Its own session
    // store holds the full diagnostics; normalized errors go to the chat log.
    let mut reader = BufReader::new(stderr);
    let mut sink = std::io::sink();
    let _ = std::io::copy(&mut reader, &mut sink);
}
