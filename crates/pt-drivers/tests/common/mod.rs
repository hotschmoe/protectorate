#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pt_drivers::{
    Ack, Entry, ErrorClass, Event, Harness, Session, SpawnSpec, Stop, ToolKind, ToolStatus,
};

pub const READY: &str = "Reply with exactly the word READY and nothing else.";
pub const RECALL: &str = "What word did you reply with before? Answer in one word.";
pub const READ_FILE: &str = "Use your file reading tool to read notes.txt in the current directory, then reply with only the secret word it contains.";
pub const LONG: &str =
    "Write the numbers from 1 to 400 as words, one per line, with no other text.";

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh git repo under the target tmp dir (left in place; cargo clean removes it).
pub fn temp_repo(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pt-drivers-{name}-{}-{n}-{}",
        std::process::id(),
        pt_now()
    ));
    std::fs::create_dir_all(&dir).expect("create temp repo");
    std::fs::write(dir.join("notes.txt"), "The secret word is PELICAN.\n").expect("write notes");
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&dir)
            .env("GIT_AUTHOR_NAME", "e2e")
            .env("GIT_AUTHOR_EMAIL", "e2e@localhost")
            .env("GIT_COMMITTER_NAME", "e2e")
            .env("GIT_COMMITTER_EMAIL", "e2e@localhost")
            .output()
            .expect("run git")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    dir
}

fn pt_now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

pub fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// `PT_E2E_<HARNESS>_BIN`, else the standalone binary this machine has.
pub fn program(harness: Harness) -> Option<PathBuf> {
    let var = format!("PT_E2E_{}_BIN", harness.as_str().to_ascii_uppercase());
    if let Some(p) = std::env::var_os(var) {
        return Some(PathBuf::from(p));
    }
    match harness {
        Harness::Codex => codex_native(),
        h => on_path(h.as_str()),
    }
}

/// The native binary inside the npm package, so no Node runs.
fn codex_native() -> Option<PathBuf> {
    let wrapper = std::fs::canonicalize(on_path("codex")?).ok()?;
    let pkg = wrapper.parent()?.parent()?;
    let (platform, triple) = match std::env::consts::ARCH {
        "aarch64" => ("codex-linux-arm64", "aarch64-unknown-linux-musl"),
        "x86_64" => ("codex-linux-x64", "x86_64-unknown-linux-musl"),
        _ => return None,
    };
    let bin = pkg
        .join("node_modules/@openai")
        .join(platform)
        .join("vendor")
        .join(triple)
        .join("bin/codex");
    bin.is_file().then_some(bin)
}

pub fn skip(harness: Harness, why: &str) {
    eprintln!("SKIP {harness}: {why}");
}

pub fn spec(harness: Harness, program: &Path, cwd: &Path) -> SpawnSpec {
    let mut spec = SpawnSpec::new(harness, program, cwd);
    spec.g = 7;
    // This suite may itself run under Claude Code; the child must not see its session.
    spec.env_remove = std::env::vars()
        .map(|(k, _)| k)
        .filter(|k| k.starts_with("CLAUDE"))
        .collect();
    if let Some(dir) = std::env::var_os("PT_E2E_CAPTURE") {
        let file = Path::new(&dir).join(format!("{harness}.jsonl"));
        let mut tee = Command::new("sh");
        tee.args(["-c", "\"$0\" \"$@\" | tee -a \"$PT_CAPTURE_FILE\""])
            .env("PT_CAPTURE_FILE", file);
        spec.prefix = Some(tee);
    }
    spec
}

pub struct Run {
    pub session: Session,
    events: Receiver<Event>,
    pub log: Vec<Event>,
    last_seq: u64,
    next_req: usize,
    finals: std::collections::HashSet<(bool, String)>,
}

#[derive(Debug, Default)]
pub struct Turn {
    pub text: String,
    pub settled: Option<(Stop, Option<String>)>,
    pub errors: Vec<(ErrorClass, String)>,
    pub tool_calls: Vec<(String, ToolKind, String)>,
    pub tool_results: Vec<(String, ToolStatus)>,
    pub approvals: Vec<String>,
    pub ack: Option<Ack>,
}

impl Turn {
    pub fn auth_failed(&self) -> bool {
        self.errors
            .iter()
            .any(|(c, _)| matches!(c, ErrorClass::Auth | ErrorClass::Quota))
    }
}

impl Run {
    pub fn start(spec: SpawnSpec) -> Run {
        let (session, events) = pt_drivers::spawn(spec).expect("spawn harness");
        Run {
            session,
            events,
            log: Vec::new(),
            last_seq: 0,
            next_req: 0,
            finals: std::collections::HashSet::new(),
        }
    }

    /// The next event, checked for seq order and the incarnation's g.
    pub fn next(&mut self, deadline: Instant) -> Option<Entry> {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.events.recv_timeout(left) {
            Ok(event) => {
                assert_eq!(event.g, 7, "event carries the spawn generation");
                assert!(
                    event.seq >= self.last_seq,
                    "seq went back: {} after {}",
                    event.seq,
                    self.last_seq
                );
                // Only a text block's deltas and final share one seq.
                let text_block = matches!(
                    event.entry,
                    Entry::AssistantText { .. } | Entry::Reasoning { .. }
                );
                assert!(
                    text_block || event.seq > self.last_seq,
                    "seq is strictly increasing outside text blocks"
                );
                self.last_seq = event.seq;
                match &event.entry {
                    Entry::AssistantText {
                        id, r#final: true, ..
                    } => {
                        assert!(
                            self.finals.insert((false, id.clone())),
                            "one final per text block {id}"
                        )
                    }
                    Entry::Reasoning {
                        id, r#final: true, ..
                    } => {
                        assert!(
                            self.finals.insert((true, id.clone())),
                            "one final per reasoning block {id}"
                        )
                    }
                    _ => {}
                }
                // Every entry serializes and parses back.
                let text = pt_json::ToJson::to_json(&event).to_string();
                let back: Event = pt_json::from_str(&text).expect("event round trip");
                assert_eq!(back.seq, event.seq);
                let entry = event.entry.clone();
                self.log.push(event);
                Some(entry)
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }

    /// Waits for `session_info` and returns the native id.
    pub fn ready(&mut self, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        while let Some(entry) = self.next(deadline) {
            match entry {
                Entry::SessionInfo { native_id, .. } => return Ok(native_id),
                Entry::Error { class, text } => return Err(format!("{class}: {text}")),
                Entry::Exited { stderr_tail, .. } => return Err(format!("exited: {stderr_tail}")),
                _ => {}
            }
        }
        Err("no session_info before the deadline".into())
    }

    pub fn send(&mut self, text: &str) -> String {
        self.next_req += 1;
        let req = format!("req-{}", self.next_req);
        self.session
            .send_user(&req, "owner", text)
            .expect("send_user");
        req
    }

    /// Sends `text` and collects the turn until it settles.
    pub fn turn(&mut self, text: &str, timeout: Duration) -> Turn {
        let req = self.send(text);
        let mut turn = self.collect(timeout, |_, _| {});
        turn.ack = Some(self.session.wait_ack(&req, Duration::from_secs(1)));
        turn
    }

    /// Collects until `turn_settled`, an exit or the deadline; `on` sees each entry first.
    pub fn collect(&mut self, timeout: Duration, mut on: impl FnMut(&Session, &Entry)) -> Turn {
        let deadline = Instant::now() + timeout;
        let mut turn = Turn::default();
        while let Some(entry) = self.next(deadline) {
            on(&self.session, &entry);
            match entry {
                Entry::AssistantText {
                    text,
                    r#final: true,
                    ..
                } => turn.text.push_str(&text),
                Entry::ToolCall { id, tool, name, .. } => turn.tool_calls.push((id, tool, name)),
                Entry::ToolResult { id, status, .. } => turn.tool_results.push((id, status)),
                Entry::ApprovalRequest { id, .. } => turn.approvals.push(id),
                Entry::Error { class, text } => turn.errors.push((class, text)),
                Entry::TurnSettled {
                    stop, native_stop, ..
                } => {
                    turn.settled = Some((stop, native_stop));
                    break;
                }
                Entry::Exited { .. } => break,
                _ => {}
            }
        }
        turn
    }

    pub fn stop(self) -> Vec<Event> {
        let Run {
            session,
            events,
            mut log,
            ..
        } = self;
        session.shutdown(Duration::from_secs(10));
        log.extend(events.try_iter());
        let last = log.last().map(|e| &e.entry);
        assert!(
            matches!(
                last,
                Some(Entry::Exited {
                    requested: true,
                    ..
                })
            ),
            "the last event is the requested exit, got {last:?}"
        );
        log
    }
}

pub fn has(log: &[Event], pred: impl Fn(&Entry) -> bool) -> bool {
    log.iter().any(|e| pred(&e.entry))
}

pub fn says_ready(text: &str) -> bool {
    text.to_ascii_uppercase().contains("READY")
}

const TURN: Duration = Duration::from_secs(240);
const BOOT: Duration = Duration::from_secs(90);

fn boot(harness: Harness, spec: SpawnSpec) -> Option<(Run, String)> {
    let mut run = Run::start(spec);
    match run.ready(BOOT) {
        Ok(id) => {
            assert!(!id.is_empty(), "native session id");
            Some((run, id))
        }
        Err(e) if pt_drivers::classify(&e, None) == ErrorClass::Auth => {
            skip(harness, &format!("no login: {e}"));
            None
        }
        Err(e) => panic!("{harness} did not become ready: {e}"),
    }
}

/// A first turn, a native resume that remembers it, and a file read through a tool.
pub fn ready_resume_tool(harness: Harness, configure: impl Fn(&mut SpawnSpec)) {
    let Some(program) = program(harness) else {
        return skip(harness, "binary not found");
    };
    let repo = temp_repo(harness.as_str());
    let mut first = spec(harness, &program, &repo);
    configure(&mut first);
    let Some((mut run, id)) = boot(harness, first) else {
        return;
    };
    let turn = run.turn(READY, TURN);
    if turn.auth_failed() {
        run.stop();
        return skip(harness, &format!("no usable login: {:?}", turn.errors));
    }
    eprintln!("{harness} first turn: {turn:?}");
    assert_eq!(turn.settled.as_ref().map(|s| s.0), Some(Stop::EndTurn));
    assert!(says_ready(&turn.text), "first reply {:?}", turn.text);
    assert!(
        matches!(turn.ack, Some(Ack::Confirmed { .. })),
        "prompt ack {:?}",
        turn.ack
    );
    let log = run.stop();
    assert!(has(
        &log,
        |e| matches!(e, Entry::UserMessage { from, .. } if from == "owner")
    ));
    assert!(has(&log, |e| matches!(
        e,
        Entry::TurnStarted {
            request_id: Some(_),
            ..
        }
    )));
    assert!(
        has(&log, |e| matches!(
            e,
            Entry::AssistantText { r#final: false, .. }
        )),
        "streamed deltas"
    );
    assert_eq!(session_ids(&log), vec![id.clone()], "one native session");

    let mut again = spec(harness, &program, &repo);
    configure(&mut again);
    again.resume = Some(id.clone());
    let Some((mut run, resumed)) = boot(harness, again) else {
        return;
    };
    assert_eq!(resumed, id, "resume keeps the native session");
    let turn = run.turn(RECALL, TURN);
    eprintln!("{harness} resumed turn: {turn:?}");
    assert!(says_ready(&turn.text), "resumed reply {:?}", turn.text);

    let turn = run.turn(READ_FILE, TURN);
    eprintln!("{harness} tool turn: {turn:?}");
    assert!(
        turn.text.to_ascii_uppercase().contains("PELICAN"),
        "tool reply {:?}",
        turn.text
    );
    assert!(!turn.tool_calls.is_empty(), "a tool call was mapped");
    assert!(
        turn.tool_results
            .iter()
            .any(|(id, status)| *status == ToolStatus::Completed
                && turn.tool_calls.iter().any(|(call, _, _)| call == id)),
        "a completed tool result matches a call"
    );
    let log = run.stop();
    assert!(session_ids(&log).iter().all(|s| *s == id));
}

fn session_ids(log: &[Event]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for e in log {
        if let Entry::SessionInfo { native_id, .. } = &e.entry
            && !ids.contains(native_id)
        {
            ids.push(native_id.clone());
        }
    }
    ids
}

/// Interrupts a long turn after its first delta; the session must take another turn.
pub fn interrupt(harness: Harness, configure: impl Fn(&mut SpawnSpec)) {
    let Some(program) = program(harness) else {
        return skip(harness, "binary not found");
    };
    let repo = temp_repo(harness.as_str());
    let mut s = spec(harness, &program, &repo);
    configure(&mut s);
    let Some((mut run, _)) = boot(harness, s) else {
        return;
    };
    run.send(LONG);
    let mut interrupted = false;
    let turn = run.collect(TURN, |session, entry| {
        let delta = matches!(
            entry,
            Entry::AssistantText { r#final: false, .. } | Entry::Reasoning { r#final: false, .. }
        );
        if delta && !interrupted {
            interrupted = true;
            session.interrupt().expect("interrupt");
        }
    });
    if turn.auth_failed() {
        run.stop();
        return skip(harness, &format!("no usable login: {:?}", turn.errors));
    }
    eprintln!("{harness} interrupted turn: {turn:?}");
    assert!(interrupted, "a delta arrived");
    let (stop, _) = turn.settled.expect("the interrupted turn settled");
    assert!(
        matches!(stop, Stop::Cancelled | Stop::EndTurn),
        "interrupted stop {stop:?}"
    );
    assert!(
        !turn.text.contains("four hundred"),
        "the turn stopped early: {:?}",
        turn.text.len()
    );
    let turn = run.turn(READY, TURN);
    eprintln!("{harness} turn after interrupt: {turn:?}");
    assert!(
        says_ready(&turn.text),
        "alive after interrupt {:?}",
        turn.text
    );
    run.stop();
}
