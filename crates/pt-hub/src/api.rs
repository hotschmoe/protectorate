//! The thin handler layer: who is calling, what they may touch, and one function per API method.
//! Commands become core events whose replies come back after the journal fsync; reads use the
//! published view. Re-pointed at pt-api's types once that crate lands.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pt_core::{
    Claim, Code, EventKind, Harness, Layout, Life, MsgState, Party, Reply, SendMode, SleeveId,
    SpawnSpec, State, Target,
};
use pt_drivers::Entry;
use pt_json::{Value, obj};

use crate::home::random_hex;
use crate::hub::{CommandError, Hub, View};
use crate::transcript::lock;
use crate::wire;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_WAIT: Duration = Duration::from_secs(3600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Principal {
    Owner,
    /// Envoy's own capability: owner-scoped (SPEC 3.1).
    Envoy(SleeveId),
    /// A sleeve's `pc1.` capability: itself and its children.
    Sleeve(SleeveId),
    /// Vendor general agents: the Envoy methods only (D9).
    EnvoyOnly,
}

impl Principal {
    fn party(self) -> Party {
        match self {
            Principal::Owner => Party::Owner,
            Principal::Envoy(_) => Party::Envoy,
            Principal::Sleeve(x) => Party::Sleeve(x),
            Principal::EnvoyOnly => Party::Agent,
        }
    }

    fn full(self) -> bool {
        matches!(self, Principal::Owner | Principal::Envoy(_))
    }
}

pub fn sleeve_principal(state: &State, id: SleeveId) -> Principal {
    if Hub::envoy_id(state) == Some(id) {
        Principal::Envoy(id)
    } else {
        Principal::Sleeve(id)
    }
}

pub fn from_token(hub: &Hub, view: &View, token: &str) -> Option<Principal> {
    if hub.tokens.is_owner(token) {
        return Some(Principal::Owner);
    }
    if token == hub.tokens.envoy_only() {
        return Some(Principal::EnvoyOnly);
    }
    view.state
        .sleeves
        .values()
        .filter(|s| s.life != Life::Removed)
        .find(|s| hub.tokens.sleeve(s.id) == token)
        .map(|s| sleeve_principal(&view.state, s.id))
}

/// The sleeve whose scope holds `pid`, read from `/proc/<pid>/cgroup`. A process inside a sleeve
/// is that sleeve whatever token it presents.
pub fn from_cgroup(hub: &Hub, view: &View, pid: i32) -> Option<Principal> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    let needle = format!("/pt-{}-", hub.tokens.hub_tag);
    let at = text.find(&needle)? + needle.len();
    let digits: String = text[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let id = SleeveId(digits.parse().ok()?);
    view.state
        .sleeve(id)
        .filter(|s| s.life != Life::Removed)
        .map(|_| sleeve_principal(&view.state, id))
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }
    pub fn bad(m: impl Into<String>) -> ApiError {
        ApiError::new(400, "invalid_argument", m)
    }
    fn not_found(m: impl Into<String>) -> ApiError {
        ApiError::new(404, "not_found", m)
    }
    pub fn denied(m: impl Into<String>) -> ApiError {
        ApiError::new(403, "permission_denied", m)
    }

    pub fn to_json(&self) -> Value {
        obj! { "error" => obj! { "code" => self.code, "message" => self.message.as_str() } }
    }
}

type Result<T> = std::result::Result<T, ApiError>;

/// Methods an envoy_only caller may use.
const ENVOY_METHODS: &[&str] = &["EnvoySend", "EnvoyRead", "EnvoyWait"];

pub fn call(hub: &Hub, who: Principal, method: &str, req: &Value) -> Result<Value> {
    if who == Principal::EnvoyOnly && !ENVOY_METHODS.contains(&method) {
        return Err(ApiError::denied(format!(
            "an envoy-only capability cannot call {method}"
        )));
    }
    match method {
        "GetState" => get_state(hub, who),
        "SpawnSleeve" => spawn(hub, who, req),
        "SendMessage" => send_message(hub, who, req),
        "GetTranscript" => get_transcript(hub, who, req),
        "Interrupt" => simple(hub, who, req, |sleeve| EventKind::Interrupt { sleeve }),
        "StopSleeve" => simple(hub, who, req, |sleeve| EventKind::Stop { sleeve }),
        "RemoveSleeve" => simple(hub, who, req, |sleeve| EventKind::Remove { sleeve }),
        "StartSleeve" => start(hub, who, req),
        "Resleeve" => resleeve(hub, who, req),
        "Decide" => decide(hub, who, req),
        "Wait" => wait(hub, who, req),
        "EnvoySend" => envoy_send(hub, who, req),
        "EnvoyRead" => envoy_read(hub, who, req),
        "EnvoyWait" => envoy_wait(hub, who, req),
        "EnvoyToken" if who == Principal::Owner => Ok(obj! { "token" => hub.tokens.envoy_only() }),
        _ => Err(ApiError::not_found(format!("no method {method}"))),
    }
}

fn can_see(state: &State, who: Principal, id: SleeveId) -> bool {
    match who {
        Principal::Owner | Principal::Envoy(_) => true,
        Principal::Sleeve(x) => {
            let parent = |s: SleeveId| state.sleeve(s).and_then(|s| s.parent);
            id == x
                || parent(id) == Some(x)
                || parent(x) == Some(id)
                || Hub::envoy_id(state) == Some(id)
        }
        Principal::EnvoyOnly => false,
    }
}

fn can_act(state: &State, who: Principal, id: SleeveId) -> bool {
    match who {
        Principal::Owner | Principal::Envoy(_) => true,
        Principal::Sleeve(x) => id == x || state.sleeve(id).and_then(|s| s.parent) == Some(x),
        Principal::EnvoyOnly => false,
    }
}

fn str_field<'a>(req: &'a Value, k: &str) -> &'a str {
    req[k].as_str().unwrap_or("")
}

fn required<'a>(req: &'a Value, k: &str) -> Result<&'a str> {
    match req[k].as_str() {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Err(ApiError::bad(format!("{k} is required"))),
    }
}

/// A sleeve named by id (`"3"` or `3`) or by name, not removed.
fn find(state: &State, v: &Value) -> Result<SleeveId> {
    let live = |id: SleeveId| state.sleeve(id).is_some_and(|s| s.life != Life::Removed);
    if let Some(n) = v.as_u64()
        && live(SleeveId(n))
    {
        return Ok(SleeveId(n));
    }
    let Some(s) = v.as_str() else {
        return Err(ApiError::bad("sleeve is required"));
    };
    if let Ok(n) = s.parse::<u64>()
        && live(SleeveId(n))
    {
        return Ok(SleeveId(n));
    }
    state
        .sleeves
        .values()
        .find(|x| x.name == s && x.life != Life::Removed)
        .map(|x| x.id)
        .ok_or_else(|| ApiError::not_found(format!("no sleeve {s:?}")))
}

fn target_sleeve(view: &View, who: Principal, req: &Value, act: bool) -> Result<SleeveId> {
    let id = find(&view.state, &req["sleeve"])?;
    let ok = if act {
        can_act(&view.state, who, id)
    } else {
        can_see(&view.state, who, id)
    };
    if !ok {
        return Err(ApiError::denied("that sleeve is outside this capability"));
    }
    Ok(id)
}

fn command(hub: &Hub, kind: EventKind) -> Result<Reply> {
    let r = hub.command(kind, COMMAND_TIMEOUT).map_err(|e| match e {
        CommandError::Timeout => ApiError::new(504, "deadline_exceeded", "the core did not answer"),
        CommandError::Down => ApiError::new(503, "unavailable", "the core is stopped"),
    })?;
    if let Reply::Refused(code, why) = r {
        let (status, name) = match code {
            Code::InvalidArgument => (400, "invalid_argument"),
            Code::NotFound => (404, "not_found"),
            Code::AlreadyExists => (409, "already_exists"),
            Code::FailedPrecondition => (412, "failed_precondition"),
        };
        return Err(ApiError::new(status, name, why));
    }
    Ok(r)
}

fn reply_word(r: &Reply) -> &'static str {
    match r {
        Reply::Spawned(_) => "spawned",
        Reply::JobCreated(_) => "job_created",
        Reply::Accepted => "accepted",
        Reply::Queued => "queued",
        Reply::Delivering => "delivering",
        Reply::Steering => "steering",
        Reply::Inboxed => "inboxed",
        Reply::Duplicate => "duplicate",
        Reply::Refused(..) => "refused",
    }
}

fn sleeve_json(hub: &Hub, id: SleeveId) -> Value {
    let view = hub.view();
    view.state
        .sleeve(id)
        .map_or(Value::Null, |s| wire::sleeve(hub, &view, s))
}

fn get_state(hub: &Hub, who: Principal) -> Result<Value> {
    let view = hub.view();
    Ok(wire::state(hub, &view, &|id| can_see(&view.state, who, id)))
}

fn parse_harness(s: &str) -> Result<Harness> {
    wire::harness_from(s).ok_or_else(|| ApiError::bad(format!("unknown harness {s:?}")))
}

fn parse_target(hub: &Hub, req: &Value, default: &str) -> Result<Target> {
    let h = match str_field(req, "harness") {
        "" => default,
        h => h,
    };
    Ok(wire::target(
        &hub.config,
        parse_harness(h)?,
        str_field(req, "provider"),
        str_field(req, "model"),
    ))
}

fn canonical(p: &str) -> Result<String> {
    let path = std::fs::canonicalize(p).map_err(|e| ApiError::bad(format!("path {p}: {e}")))?;
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| ApiError::bad("path must be UTF-8"))
}

fn spawn(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let st = &view.state;
    let name = required(req, "name")?.to_string();
    let parent = match (&req["parent"], who) {
        (Value::Null, Principal::Sleeve(x)) => Some(x),
        (Value::Null, _) => None,
        (v, _) => Some(find(st, v)?),
    };
    if let Principal::Sleeve(x) = who
        && parent != Some(x)
    {
        return Err(ApiError::denied("a sleeve spawns only its own children"));
    }
    let layout = match (str_field(req, "layout"), parent) {
        ("", None) | ("root", _) => Layout::Root,
        ("", Some(_)) | ("worktree", _) => Layout::Worktree,
        ("view", _) => Layout::View,
        (l, _) => return Err(ApiError::bad(format!("unknown layout {l:?}"))),
    };
    let parent_path = parent
        .and_then(|p| st.sleeve(p))
        .map(|p| p.claim.path.clone());
    let path = match (layout, str_field(req, "path")) {
        (Layout::Root, "") => return Err(ApiError::bad("path is required for a root")),
        (Layout::Root, p) => canonical(p)?,
        (Layout::Worktree, "") => {
            let dir: PathBuf = hub.home.worktrees().join(format!(
                "{name}-{}",
                random_hex(6).map_err(|e| ApiError::bad(e.to_string()))?
            ));
            dir.to_string_lossy().into_owned()
        }
        (Layout::Worktree, p) => {
            let p = Path::new(p);
            let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("/");
            let file = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
            format!("{}/{file}", canonical(dir)?)
        }
        (Layout::View, "") => parent_path.clone().unwrap_or_default(),
        (Layout::View, p) => canonical(p)?,
    };
    let target = parse_target(hub, req, &hub.config.default_harness)?;
    let resleeve_to = match req["resleeve_to"].as_array() {
        Some(list) => list
            .iter()
            .map(|v| {
                let h = parse_harness(v.as_str().unwrap_or(""))?;
                Ok(wire::target(&hub.config, h, "", ""))
            })
            .collect::<Result<Vec<_>>>()?,
        None => hub
            .config
            .resleeve_to
            .iter()
            .map(|h| Ok(wire::target(&hub.config, parse_harness(h)?, "", "")))
            .collect::<Result<Vec<_>>>()?,
    };
    let request_id = match str_field(req, "request_id") {
        "" => format!(
            "spawn:{}",
            random_hex(16).map_err(|e| ApiError::bad(e.to_string()))?
        ),
        r => r.to_string(),
    };
    let spec = SpawnSpec {
        request_id,
        name,
        parent,
        layout,
        claim: Claim {
            path,
            writable: layout != Layout::View,
        },
        target,
        resleeve_to,
        budget: req["budget"].as_u64(),
        disk_limit: req["disk_limit_bytes"].as_u64(),
    };
    match command(hub, EventKind::Spawn(spec))? {
        Reply::Spawned(id) => Ok(obj! { "sleeve" => sleeve_json(hub, id) }),
        r => Err(ApiError::bad(format!(
            "unexpected reply {}",
            reply_word(&r)
        ))),
    }
}

fn prefix(state: &State, who: Principal, agent: &str, text: &str) -> String {
    match who {
        Principal::Sleeve(x) => {
            let name = state.sleeve(x).map_or("?", |s| s.name.as_str());
            format!("[from sleeve {name}] {text}")
        }
        Principal::EnvoyOnly => format!("[from agent:{agent}] {text}"),
        Principal::Owner | Principal::Envoy(_) => text.to_string(),
    }
}

fn new_request(who: Principal, agent: &str) -> Result<String> {
    let r = random_hex(16).map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(match who {
        Principal::EnvoyOnly => format!("ga:{agent}:{r}"),
        Principal::Owner => format!("owner:{r}"),
        Principal::Envoy(_) => format!("envoy:{r}"),
        Principal::Sleeve(x) => format!("mail:{}:{r}", x.0),
    })
}

fn send_message(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let id = find(&view.state, &req["sleeve"])?;
    let allowed = match who {
        Principal::Sleeve(_) => can_see(&view.state, who, id),
        other => other.full(),
    };
    if !allowed {
        return Err(ApiError::denied("that sleeve is outside this capability"));
    }
    let text = required(req, "text")?;
    let mode = match str_field(req, "mode") {
        "" | "queue" => SendMode::Queue,
        "steer" => SendMode::Steer,
        m => return Err(ApiError::bad(format!("unknown mode {m:?}"))),
    };
    let request_id = match str_field(req, "request_id") {
        "" => new_request(who, "")?,
        r => r.to_string(),
    };
    let r = command(
        hub,
        EventKind::Send {
            to: id,
            request_id: request_id.clone(),
            from: who.party(),
            text: prefix(&view.state, who, "", text),
            mode,
        },
    )?;
    Ok(obj! { "request_id" => request_id, "result" => reply_word(&r) })
}

fn entries_after(
    hub: &Hub,
    id: SleeveId,
    after: u64,
    limit: usize,
) -> Result<Vec<pt_drivers::Event>> {
    let store = hub
        .transcripts
        .get(id)
        .map_err(|e| ApiError::new(500, "internal", e.to_string()))?;
    let out = lock(&store)
        .after(after, limit)
        .map_err(|e| ApiError::new(500, "internal", e.to_string()))?;
    Ok(out)
}

fn transcript_json(hub: &Hub, id: SleeveId, req: &Value) -> Result<Value> {
    let after = req["after_seq"].as_u64().unwrap_or(0);
    let limit = req["limit"].as_u64().unwrap_or(10_000).min(100_000) as usize;
    let raw = req["raw"].as_bool() == Some(true);
    let entries: Vec<Value> = entries_after(hub, id, after, limit)?
        .iter()
        .map(|e| {
            if raw {
                pt_json::ToJson::to_json(e)
            } else {
                wire::chat_entry(e, false)
            }
        })
        .collect();
    Ok(obj! { "sleeve_id" => id.0.to_string(), "entries" => entries })
}

fn get_transcript(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let id = target_sleeve(&view, who, req, false)?;
    transcript_json(hub, id, req)
}

fn simple(
    hub: &Hub,
    who: Principal,
    req: &Value,
    kind: impl FnOnce(SleeveId) -> EventKind,
) -> Result<Value> {
    let view = hub.view();
    let id = target_sleeve(&view, who, req, true)?;
    let r = command(hub, kind(id))?;
    Ok(obj! { "result" => reply_word(&r), "sleeve" => sleeve_json(hub, id) })
}

fn optional_target(hub: &Hub, req: &Value) -> Result<Option<Target>> {
    if str_field(req, "harness").is_empty() {
        return Ok(None);
    }
    parse_target(hub, req, "").map(Some)
}

fn start(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let target = optional_target(hub, req)?;
    simple(hub, who, req, |sleeve| EventKind::Start { sleeve, target })
}

fn resleeve(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let target = optional_target(hub, req)?;
    simple(hub, who, req, |sleeve| EventKind::Resleeve {
        sleeve,
        target,
    })
}

fn decide(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let full = required(req, "approval_id")?;
    let (sid, approval) = full
        .split_once(':')
        .ok_or_else(|| ApiError::bad("approval_id is <sleeve id>:<approval>"))?;
    let id = find(&view.state, &Value::from(sid))?;
    if !can_act(&view.state, who, id) {
        return Err(ApiError::denied("that sleeve is outside this capability"));
    }
    let allow = req["approve"]
        .as_bool()
        .ok_or_else(|| ApiError::bad("approve is required"))?;
    let r = command(
        hub,
        EventKind::RespondApproval {
            sleeve: id,
            approval: approval.to_string(),
            allow,
            note: str_field(req, "note").to_string(),
        },
    )?;
    Ok(obj! { "result" => reply_word(&r) })
}

fn timeout(req: &Value) -> Duration {
    Duration::from_millis(req["timeout_ms"].as_u64().unwrap_or(60_000)).min(MAX_WAIT)
}

/// Re-evaluates `check` on every change until it answers or `deadline` passes. The bus seq is
/// read before the view, so a change between the two wakes the wait at once.
fn wait_for<T>(
    hub: &Hub,
    deadline: Instant,
    mut check: impl FnMut(&View) -> Option<T>,
) -> Option<T> {
    loop {
        let seq = hub.bus.seq();
        let view = hub.view();
        if let Some(x) = check(&view) {
            return Some(x);
        }
        if Instant::now() >= deadline {
            return None;
        }
        hub.bus.wait_after(seq, deadline);
    }
}

fn reached(state: &State, id: SleeveId, until: &[String]) -> Option<String> {
    let s = state.sleeve(id)?;
    let idle = matches!(s.activity, pt_core::Activity::Idle { .. })
        && s.queue.is_empty()
        && s.life == Life::Running;
    for u in until {
        let hit = match u.as_str() {
            "idle" => idle,
            "done" => idle && s.activity == (pt_core::Activity::Idle { seen: false }),
            "blocked" => matches!(s.activity, pt_core::Activity::Blocked { .. }),
            "parked" => matches!(s.life, Life::Parked(_)),
            "ended" => matches!(s.life, Life::Stopped | Life::Removed),
            _ => false,
        };
        if hit {
            return Some(u.clone());
        }
    }
    None
}

fn wait(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let id = target_sleeve(&view, who, req, false)?;
    let until: Vec<String> = match &req["until"] {
        Value::Null => vec!["idle".into(), "done".into(), "blocked".into()],
        Value::String(s) => s.split(',').map(str::to_string).collect(),
        v => v
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    };
    const KNOWN: &[&str] = &["idle", "done", "blocked", "parked", "ended"];
    if let Some(u) = until.iter().find(|u| !KNOWN.contains(&u.as_str())) {
        return Err(ApiError::bad(format!("cannot wait until {u:?}")));
    }
    let deadline = Instant::now() + timeout(req);
    let got = wait_for(hub, deadline, |v| {
        if let Some(u) = reached(&v.state, id, &until) {
            return Some(Some(u));
        }
        // A sleeve that stopped or parked never gets there: answer instead of hanging.
        let s = v.state.sleeve(id)?;
        matches!(s.life, Life::Stopped | Life::Removed | Life::Parked(_)).then_some(None)
    });
    let state = match &got {
        Some(Some(u)) => u.clone(),
        Some(None) => "ended".to_string(),
        None => "timeout".to_string(),
    };
    Ok(obj! {
        "reached" => matches!(got, Some(Some(_))),
        "state" => state,
        "sleeve" => sleeve_json(hub, id),
    })
}

fn envoy(view: &View) -> Result<SleeveId> {
    Hub::envoy_id(&view.state)
        .ok_or_else(|| ApiError::new(503, "unavailable", "Envoy is not running"))
}

fn envoy_send(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    if matches!(who, Principal::Sleeve(_)) {
        return Err(ApiError::denied("sleeves mail Envoy with SendMessage"));
    }
    let view = hub.view();
    let id = envoy(&view)?;
    let agent = match str_field(req, "agent") {
        "" => "agent",
        a => a,
    };
    if !agent
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError::bad("agent is a name of [A-Za-z0-9_-]"));
    }
    let text = required(req, "text")?;
    let request_id = match str_field(req, "request_id") {
        "" => new_request(who, agent)?,
        r => r.to_string(),
    };
    let r = command(
        hub,
        EventKind::Send {
            to: id,
            request_id: request_id.clone(),
            from: who.party(),
            text: prefix(&view.state, who, agent, text),
            mode: SendMode::Queue,
        },
    )?;
    Ok(obj! { "request_id" => request_id, "result" => reply_word(&r) })
}

fn envoy_read(hub: &Hub, _who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let id = envoy(&view)?;
    transcript_json(hub, id, req)
}

/// The assistant text of the turn that answered `request_id`, once that turn settled.
fn answer(hub: &Hub, id: SleeveId, request_id: &str, idle: bool) -> Option<String> {
    let entries = entries_after(hub, id, 0, usize::MAX).ok()?;
    let start = entries.iter().rposition(
        |e| matches!(&e.entry, Entry::UserMessage { request_id: r, .. } if r == request_id),
    )?;
    let mut text = Vec::new();
    for e in &entries[start + 1..] {
        match &e.entry {
            Entry::AssistantText { text: t, .. } => text.push(t.clone()),
            Entry::TurnSettled { .. } => return Some(text.join("\n")),
            Entry::UserMessage { .. } => break,
            _ => {}
        }
    }
    idle.then(|| text.join("\n"))
}

fn envoy_wait(hub: &Hub, who: Principal, req: &Value) -> Result<Value> {
    let view = hub.view();
    let id = envoy(&view)?;
    let party = who.party();
    let request_id = match str_field(req, "request_id") {
        "" => {
            let s = view
                .state
                .sleeve(id)
                .ok_or_else(|| ApiError::not_found("no Envoy"))?;
            let mine = |r: &&String| s.messages.get(*r).is_some_and(|m| m.from == party);
            // The latest message from this party, by its place in the transcript.
            let entries = entries_after(hub, id, 0, usize::MAX)?;
            let from_log = entries.iter().rev().find_map(|e| match &e.entry {
                Entry::UserMessage { request_id, .. }
                    if s.messages.get(request_id).is_some_and(|m| m.from == party) =>
                {
                    Some(request_id.clone())
                }
                _ => None,
            });
            from_log
                .or_else(|| s.queue.iter().rfind(mine).cloned())
                .ok_or_else(|| ApiError::not_found("no message from you to wait for"))?
        }
        r => r.to_string(),
    };
    {
        let s = view
            .state
            .sleeve(id)
            .ok_or_else(|| ApiError::not_found("no Envoy"))?;
        match s.messages.get(&request_id) {
            None => return Err(ApiError::not_found(format!("no message {request_id}"))),
            Some(m) if who == Principal::EnvoyOnly && m.from != Party::Agent => {
                return Err(ApiError::denied("that message is not yours"));
            }
            Some(_) => {}
        }
    }
    let deadline = Instant::now() + timeout(req);
    let got = wait_for(hub, deadline, |v| {
        let s = v.state.sleeve(id)?;
        let m = s.messages.get(&request_id)?;
        match m.state {
            MsgState::Rejected => Some(("rejected", String::new())),
            MsgState::MayHaveApplied => Some(("may_have_applied", String::new())),
            MsgState::Delivered => {
                let idle = matches!(s.activity, pt_core::Activity::Idle { .. });
                answer(hub, id, &request_id, idle).map(|t| ("settled", t))
            }
            _ => None,
        }
    });
    let (state, text) = got.unwrap_or(("timeout", String::new()));
    Ok(obj! {
        "request_id" => request_id,
        "settled" => state == "settled",
        "state" => state,
        "text" => text,
    })
}
