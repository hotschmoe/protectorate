use protectorate_chatlog::{read_all, Event, Log};
use protectorate_grok_driver::Session;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const FAKE: &str = r#"
import json, os, sys

mode = os.environ.get("GROK_FAKE", "")

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def recv():
    line = sys.stdin.readline()
    if not line:
        raise SystemExit(0)
    return json.loads(line)

def reply(msg, result):
    send({"jsonrpc": "2.0", "id": msg.get("id"), "result": result})

def fail(msg, code, message):
    send({"jsonrpc": "2.0", "id": msg.get("id"), "error": {"code": code, "message": message}})

if mode == "exit":
    raise SystemExit(3)

sid = "grok-new-1"
gated = False
gate = ""

def chunk(text, thinking=False, message_id="m1"):
    kind = "agent_thought_chunk" if thinking else "agent_message_chunk"
    send({"jsonrpc": "2.0", "method": "session/update", "params": {
        "sessionId": sid,
        "update": {"sessionUpdate": kind, "messageId": message_id, "content": {"type": "text", "text": text}},
    }})

def tool(call_id, title, raw):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {
        "sessionId": sid,
        "update": {"sessionUpdate": "tool_call", "toolCallId": call_id, "title": title, "rawInput": raw},
    }})

def tool_update(call_id, status, text=None, raw=None):
    update = {"sessionUpdate": "tool_call_update", "toolCallId": call_id, "status": status}
    if text is not None:
        update["content"] = [{"type": "content", "content": {"type": "text", "text": text}}]
    if raw is not None:
        update["rawOutput"] = raw
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": sid, "update": update}})

def ask(options):
    send({"jsonrpc": "2.0", "id": 100, "method": "session/request_permission", "params": {
        "sessionId": sid,
        "toolCall": {"toolCallId": "call-1", "kind": "edit", "title": "Write `/x/hello.txt`"},
        "options": options,
    }})
    while True:
        resp = recv()
        if resp.get("id") == 100:
            return resp

STANDARD = [
    {"optionId": "allow-edits-session", "kind": "allow_always"},
    {"optionId": "allow-once", "kind": "allow_once"},
    {"optionId": "reject-once", "kind": "reject_once"},
]

while True:
    msg = recv()
    method = msg.get("method") or ""
    params = msg.get("params") if isinstance(msg.get("params"), dict) else {}
    if method == "initialize":
        if msg.get("jsonrpc") != "2.0" or params.get("protocolVersion") != 1 or params.get("clientInfo", {}).get("name") != "protectorate":
            fail(msg, -32602, "bad initialize")
            continue
        version = 2 if mode == "bad-init" else 1
        reply(msg, {"protocolVersion": version, "agentCapabilities": {"loadSession": True}})
    elif method == "session/new":
        if mode == "no-id":
            reply(msg, {})
            continue
        if not params.get("cwd") or params.get("mcpServers") != []:
            fail(msg, -32602, "bad session/new")
            continue
        reply(msg, {"sessionId": sid})
    elif method == "session/load":
        sid = params.get("sessionId") or ""
        if not sid or not params.get("cwd") or params.get("mcpServers") != []:
            fail(msg, -32602, "bad session/load")
            continue
        chunk("REPLAY", message_id="replay")
        if mode == "load-mismatch":
            reply(msg, {"sessionId": "other"})
        else:
            reply(msg, {})
    elif method == "session/prompt":
        try:
            text = params["prompt"][0]["text"]
        except Exception:
            fail(msg, -32602, "bad prompt")
            continue
        if not gated:
            if text not in ("/always-approve off", "/always-approve on"):
                fail(msg, -32602, "expected permission gate")
                continue
            gated = True
            gate = text
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "die":
            raise SystemExit(3)
        if text == "slow":
            while recv().get("method") != "session/cancel":
                pass
            reply(msg, {"stopReason": "cancelled"})
            continue
        if text == "approve":
            resp = ask(STANDARD)
        elif text == "approve-none":
            resp = ask([])
        elif text == "approve-prefix":
            resp = ask([
                {"optionId": "reject-session", "kind": "reject_always"},
                {"optionId": "allow-session", "kind": "allow_always"},
            ])
        else:
            resp = None
        if resp is not None:
            outcome = (resp.get("result") or {}).get("outcome") or {}
            option = outcome.get("optionId") or outcome.get("outcome")
            if option == "allow-once":
                chunk("allowed gate=" + gate)
            else:
                chunk("denied " + str(option))
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "unknown":
            send({"jsonrpc": "2.0", "id": 7, "method": "session/custom", "params": {}})
            resp = recv()
            if (resp.get("error") or {}).get("code") != -32601:
                fail(msg, -32000, "driver did not reject unknown method")
                continue
            chunk("unsupported")
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "bad-permission":
            send({"jsonrpc": "2.0", "id": 8, "method": "session/request_permission", "params": []})
            resp = recv()
            if (resp.get("error") or {}).get("code") != -32602:
                fail(msg, -32000, "driver accepted invalid permission params")
                continue
            chunk("invalid-params")
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "hello":
            tool("tc1", "ls", {"command": "ls"})
            tool_update("tc1", "in_progress", text="nope")
            tool_update("tc1", "completed", text="a.txt")
            send({"jsonrpc": "2.0", "method": "_x.ai/session_notification", "params": {"sessionId": sid, "update": {"usage": {
                "input_tokens": 100, "output_tokens": 3, "cache_read_input_tokens": 28, "cache_creation_input_tokens": 0,
            }}}})
            chunk("ok id=" + sid)
            reply(msg, {"stopReason": "end_turn", "_meta": {"usage": {"inputTokens": 9, "outputTokens": 2, "cachedReadTokens": 4}}})
            continue
        if text == "fail":
            tool("f1", "write", {"path": "a"})
            tool_update("f1", "failed", text="nope")
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "meta":
            chunk("meta")
            reply(msg, {"stopReason": "end_turn", "_meta": {"usage": {"inputTokens": 9, "outputTokens": 2, "cachedReadTokens": 4}}})
            continue
        if text == "thought":
            chunk("hidden", thinking=True, message_id="think")
            chunk("visible", message_id="say")
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "huge":
            tool_update("h1", "completed", text="x" * (16384 + 32))
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "raw":
            tool_update("r1", "completed", raw="from-raw")
            reply(msg, {"stopReason": "end_turn"})
            continue
        if text == "prompt-error":
            fail(msg, -32000, "backend failed")
            continue
        chunk("echo " + text)
        reply(msg, {"stopReason": "end_turn"})
    elif method == "session/cancel":
        pass
    elif msg.get("id") is not None:
        pass
"#;

struct Running {
    session: Arc<Session>,
    path: PathBuf,
    _dir: tempfile::TempDir,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.session.stop();
        let _ = self.session.wait_exited(Duration::from_secs(2));
    }
}

fn open(mode: &str, resume: Option<&str>, autonomous: bool) -> Result<Running, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = dir.path().join("chat.jsonl");
    let log = Arc::new(Log::open(&path).map_err(|error| error.to_string())?);
    let child = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(FAKE)
        .env("GROK_FAKE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let session = Session::attach(child, log, "/workspace/demo", resume, autonomous)?;
    Ok(Running {
        session,
        path,
        _dir: dir,
    })
}

fn events(path: &Path) -> Vec<Event> {
    let start = Instant::now();
    loop {
        match read_all(path) {
            Ok(events) => return events,
            Err(_) if start.elapsed() < Duration::from_secs(2) => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("read log: {error}"),
        }
    }
}

fn wait_until(path: &Path, pred: impl Fn(&[Event]) -> bool) -> Vec<Event> {
    let start = Instant::now();
    loop {
        let found = events(path);
        if pred(&found) {
            return found;
        }
        if start.elapsed() > Duration::from_secs(5) {
            panic!("timed out; events: {found:?}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn texts(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| event.payload["text"]["delta"].as_str())
        .collect()
}

fn settled(events: &[Event], reason: &str) -> bool {
    events
        .iter()
        .any(|event| event.payload["settled"]["stopReason"] == reason)
}

fn settled_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| event.payload.get("settled").is_some())
        .count()
}

#[test]
fn hello_turn_reports_tools_and_per_call_usage() {
    let running = open("", None, false).unwrap();
    assert!(running.session.is_idle());
    assert_eq!(running.session.status()["harnessSessionId"], "grok-new-1");
    assert_eq!(running.session.status()["provider"], "xai");
    assert_eq!(running.session.status()["state"], "CHAT_STATE_IDLE");
    assert!(running.session.send("", "owner", "r").is_err());
    running.session.send("hello", "owner", "r1").unwrap();
    assert!(!running.session.is_idle());
    let found = wait_until(&running.path, |events| settled(events, "end_turn"));
    assert!(running.session.is_idle());
    let body = texts(&found);
    assert!(body.contains("ok id=grok-new-1"), "{body}");
    assert!(
        !body.contains("REPLAY") && !body.contains("always-approve"),
        "{body}"
    );
    let tools: Vec<&Value> = found
        .iter()
        .filter_map(|event| event.payload.get("toolCall"))
        .collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "ls");
    assert!(tools[0]["inputJson"].as_str().unwrap().contains("ls"));
    let results: Vec<&Value> = found
        .iter()
        .filter_map(|event| event.payload.get("toolResult"))
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["output"], "a.txt");
    assert_eq!(results[0]["isError"], false);
    let usages: Vec<&Value> = found
        .iter()
        .filter_map(|event| event.payload.get("usage"))
        .collect();
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0]["contextTokens"], "128");
    assert_eq!(usages[0]["inputTokens"], "100");
    assert_eq!(usages[0]["outputTokens"], "3");
    assert_eq!(usages[0]["cacheReadTokens"], "28");
    assert_eq!(usages[0]["cacheWriteTokens"], "0");
    assert_eq!(usages[0]["provider"], "xai");
    assert_eq!(running.session.status()["contextTokens"], "128");
    let report = running
        .session
        .append_report("REPORT_STATUS_COMPLETE", "done", 1)
        .unwrap();
    assert_eq!(report["chatSeq"], running.session.status()["lastSeq"]);
}

#[test]
fn approvals_select_once_or_cancel() {
    let running = open("", None, false).unwrap();
    running.session.send("approve", "owner", "a").unwrap();
    let pending = wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload.get("approval").is_some())
    });
    let approval = pending
        .iter()
        .find_map(|event| event.payload.get("approval"))
        .unwrap();
    assert_eq!(approval["id"], "100");
    assert_eq!(approval["kind"], "edit");
    assert!(approval["summary"].as_str().unwrap().contains("hello.txt"));
    assert_eq!(
        running.session.status()["state"],
        "CHAT_STATE_AWAITING_APPROVAL"
    );
    assert!(running.session.send("more", "owner", "b").is_err());
    running
        .session
        .respond("100", true, "ignored", "owner")
        .unwrap();
    assert!(running.session.respond("100", true, "", "owner").is_err());
    let found = wait_until(&running.path, |events| settled(events, "end_turn"));
    let body = texts(&found);
    assert!(body.contains("allowed gate=/always-approve off"), "{body}");
    assert!(found
        .iter()
        .any(|event| event.payload["approvalResolved"]["allow"] == true));
}

#[test]
fn deny_and_prefix_and_empty_options() {
    let running = open("", None, false).unwrap();
    running.session.send("approve", "owner", "a").unwrap();
    wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload.get("approval").is_some())
    });
    running
        .session
        .respond("100", false, "nope", "owner")
        .unwrap();
    let denied = wait_until(&running.path, |events| texts(events).contains("denied"));
    assert!(texts(&denied).contains("reject-once"), "{}", texts(&denied));

    running
        .session
        .send("approve-prefix", "owner", "b")
        .unwrap();
    wait_until(&running.path, |events| {
        events
            .iter()
            .filter(|event| event.payload.get("approval").is_some())
            .count()
            == 2
    });
    running.session.respond("100", true, "", "owner").unwrap();
    let prefixed = wait_until(&running.path, |events| {
        texts(events).contains("allow-session")
    });
    assert!(texts(&prefixed).contains("denied allow-session"));

    running.session.send("approve-none", "owner", "c").unwrap();
    wait_until(&running.path, |events| {
        events
            .iter()
            .filter(|event| event.payload.get("approval").is_some())
            .count()
            == 3
    });
    running.session.respond("100", true, "", "owner").unwrap();
    let cancelled = wait_until(&running.path, |events| texts(events).contains("cancelled"));
    assert!(texts(&cancelled).contains("denied cancelled"));
}

#[test]
fn autonomous_gate_is_on() {
    let running = open("", None, true).unwrap();
    running.session.send("approve", "owner", "a").unwrap();
    wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload.get("approval").is_some())
    });
    running.session.respond("100", true, "", "owner").unwrap();
    let found = wait_until(&running.path, |events| settled(events, "end_turn"));
    assert!(texts(&found).contains("allowed gate=/always-approve on"));
}

#[test]
fn interrupt_cancels_slow_turn_and_rejects_a_second_send() {
    let running = open("", None, false).unwrap();
    running.session.send("slow", "owner", "s").unwrap();
    let busy = running.session.send("more", "owner", "s2").unwrap_err();
    assert!(busy.contains("precondition"), "{busy}");
    let blocked = running.session.compact().unwrap_err();
    assert!(blocked.contains("precondition"), "{blocked}");
    running.session.interrupt().unwrap();
    let found = wait_until(&running.path, |events| settled(events, "cancelled"));
    assert!(running.session.is_idle());
    running.session.interrupt().unwrap();
    assert_eq!(
        found
            .iter()
            .filter(|event| event.payload.get("settled").is_some())
            .count(),
        1
    );
    let unsupported = running.session.compact().unwrap_err();
    assert!(unsupported.starts_with("unimplemented:"), "{unsupported}");
    let models = running.session.list_models().unwrap();
    assert_eq!(models["supported"], false);
    assert!(running.session.set_selection("grok-3", "high").is_err());
}

#[test]
fn resume_drops_replay_and_keeps_the_requested_id() {
    let running = open("", Some("sess-9"), false).unwrap();
    assert_eq!(running.session.status()["harnessSessionId"], "sess-9");
    let opened = events(&running.path);
    assert!(!texts(&opened).contains("REPLAY"), "{opened:?}");
    running.session.send("hello", "owner", "h").unwrap();
    let found = wait_until(&running.path, |events| settled(events, "end_turn"));
    assert!(texts(&found).contains("ok id=sess-9"));
    assert!(!texts(&found).contains("REPLAY"));
}

#[test]
fn meta_usage_thoughts_truncation_and_raw_output() {
    let running = open("", None, false).unwrap();
    running.session.send("meta", "owner", "m").unwrap();
    let meta = wait_until(&running.path, |events| settled(events, "end_turn"));
    let usage = meta
        .iter()
        .find_map(|event| event.payload.get("usage"))
        .unwrap();
    assert_eq!(usage["contextTokens"], "9");
    assert_eq!(usage["inputTokens"], "5");
    assert_eq!(usage["cacheReadTokens"], "4");
    assert_eq!(usage["outputTokens"], "2");

    let before = settled_count(&meta);
    running.session.send("thought", "owner", "t").unwrap();
    let thought = wait_until(&running.path, |events| {
        texts(events).contains("visible") && settled_count(events) > before
    });
    assert!(thought.iter().any(|event| {
        event.payload["text"]["thinking"] == true && event.payload["text"]["delta"] == "hidden"
    }));
    assert!(thought.iter().any(|event| {
        event.payload["text"]["thinking"] == false && event.payload["text"]["delta"] == "visible"
    }));

    let before = settled_count(&thought);
    running.session.send("huge", "owner", "u").unwrap();
    let huge = wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload["toolResult"]["truncated"] == true)
            && settled_count(events) > before
    });
    let output = huge
        .iter()
        .find_map(|event| event.payload.get("toolResult"))
        .filter(|result| result["truncated"] == true)
        .unwrap();
    assert_eq!(output["output"].as_str().unwrap().len(), 16 << 10);

    let before = settled_count(&huge);
    running.session.send("raw", "owner", "r").unwrap();
    let raw = wait_until(&running.path, |events| {
        texts_or_tools(events, "from-raw") && settled_count(events) > before
    });
    assert!(raw
        .iter()
        .any(|event| event.payload["toolResult"]["output"] == "from-raw"));

    let before = settled_count(&raw);
    running.session.send("fail", "owner", "f").unwrap();
    let failed = wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload["toolResult"]["isError"] == true)
            && settled_count(events) > before
    });
    assert!(failed
        .iter()
        .any(|event| event.payload["toolResult"]["output"] == "nope"));
}

fn texts_or_tools(events: &[Event], needle: &str) -> bool {
    events.iter().any(|event| {
        event.payload["toolResult"]["output"]
            .as_str()
            .is_some_and(|output| output.contains(needle))
    })
}

#[test]
fn unknown_requests_do_not_become_approvals() {
    let running = open("", None, false).unwrap();
    running.session.send("unknown", "owner", "u").unwrap();
    let found = wait_until(&running.path, |events| {
        texts(events).contains("unsupported")
    });
    assert!(found
        .iter()
        .all(|event| event.payload.get("approval").is_none()));
    running
        .session
        .send("bad-permission", "owner", "b")
        .unwrap();
    let invalid = wait_until(&running.path, |events| {
        texts(events).contains("invalid-params")
    });
    assert!(invalid
        .iter()
        .all(|event| event.payload.get("approval").is_none()));
    running.session.send("prompt-error", "owner", "e").unwrap();
    let failed = wait_until(&running.path, |events| settled(events, "error"));
    assert!(failed.iter().any(|event| event.payload["error"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("backend failed"))));
}

#[test]
fn open_failures_name_the_rejected_handshake() {
    let missing = must_fail(open("no-id", None, false));
    assert!(missing.contains("no session id"), "{missing}");
    let version = must_fail(open("bad-init", None, false));
    assert!(version.contains("protocolVersion"), "{version}");
    let mismatched = must_fail(open("load-mismatch", Some("sess-9"), false));
    assert!(mismatched.contains("different session"), "{mismatched}");
    let died = must_fail(open("exit", None, false));
    assert!(died.contains("open grok"), "{died}");
}

fn must_fail(result: Result<Running, String>) -> String {
    match result {
        Err(error) => error,
        Ok(_) => panic!("handshake succeeded"),
    }
}

#[test]
fn process_death_during_a_turn_marks_the_session_exited() {
    let running = open("", None, false).unwrap();
    running.session.send("die", "owner", "d").unwrap();
    assert!(running.session.wait_exited(Duration::from_secs(5)));
    let found = wait_until(&running.path, |events| {
        events
            .iter()
            .any(|event| event.payload["state"]["state"] == "CHAT_STATE_EXITED")
    });
    assert!(found.iter().any(|event| event.payload["error"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("exited"))));
    assert!(running.session.is_exited());
    assert!(running.session.send("later", "owner", "x").is_err());
}
