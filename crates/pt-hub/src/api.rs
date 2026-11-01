//! The handler layer: who is calling, what they may touch, and one function per method of
//! pt-api's table. Commands become core events whose replies come back after the journal fsync;
//! reads use the published view.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, Method, Scope, method as m};
use pt_core::{
    Claim, Code, EventKind, Layout, Life, MsgState, Party, Reply, SendMode, SleeveId, SpawnSpec,
    State, Target,
};
use pt_drivers::Entry;
use pt_json::{FromJson, ToJson, Value};

use crate::approvals::ACTION_PREFIX;
use crate::home::{now_ms, random_hex};
use crate::hub::{CommandError, Hub, View};
use crate::rule::{self, Guarded};
use crate::transcript::{Note, lock};
use crate::wire;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_WAIT: Duration = Duration::from_secs(3600);
const TRANSCRIPT_DEFAULT: u64 = 2000;
const TRANSCRIPT_MAX: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Principal {
    /// The owner token, or the hub's own user on the unix socket.
    Owner,
    /// A paired app's `pd1.` token: the owner's scope.
    Device(String),
    /// Envoy's capability: sleeve scope, reaching every sleeve.
    Envoy(SleeveId),
    /// A sleeve's `pc1.` capability: itself and its children.
    Sleeve(SleeveId),
    /// Vendor general agents: the Envoy methods only (D9).
    EnvoyOnly,
    /// No credential: public methods only.
    Anonymous,
}

impl Principal {
    pub fn scope(&self) -> Scope {
        match self {
            Principal::Owner | Principal::Device(_) => Scope::Owner,
            Principal::Envoy(_) | Principal::Sleeve(_) => Scope::Sleeve,
            Principal::EnvoyOnly => Scope::EnvoyOnly,
            Principal::Anonymous => Scope::Public,
        }
    }

    fn party(&self) -> Party {
        match self {
            Principal::Owner | Principal::Device(_) | Principal::Anonymous => Party::Owner,
            Principal::Envoy(_) => Party::Envoy,
            Principal::Sleeve(x) => Party::Sleeve(*x),
            Principal::EnvoyOnly => Party::Agent,
        }
    }

    /// Reaches every sleeve.
    fn full(&self) -> bool {
        matches!(
            self,
            Principal::Owner | Principal::Device(_) | Principal::Envoy(_)
        )
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
    if let Some(id) = hub.devices.check(token) {
        return Some(Principal::Device(id));
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

type Result<T> = std::result::Result<T, ApiError>;

pub fn err(code: ErrorCode, m: impl Into<String>) -> ApiError {
    ApiError::new(code, m)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    err(ErrorCode::Internal, e.to_string())
}

pub fn unavailable(what: &str) -> ApiError {
    err(ErrorCode::Unavailable, format!("{what} is not built yet"))
}

struct Ctx<'a> {
    hub: &'a Hub,
    who: &'a Principal,
}

fn run<M: Method>(
    c: &Ctx<'_>,
    body: &Value,
    f: fn(&Ctx<'_>, M::Request) -> Result<M::Response>,
) -> Result<Value> {
    let req = M::Request::from_json(body)
        .map_err(|e| err(ErrorCode::Invalid, format!("{} request: {e}", M::NAME)))?;
    f(c, req).map(|r| r.to_json())
}

macro_rules! routes {
    ($c:expr, $name:expr, $body:expr; $($M:ident => $f:ident),* $(,)?) => {
        match $name {
            $( n if n == <m::$M as Method>::NAME => run::<m::$M>($c, $body, $f), )*
            other => Err(err(ErrorCode::Internal, format!("{other} has no handler"))),
        }
    };
}

/// Calls API method `name` for `who` with a JSON request body.
pub fn call(hub: &Hub, who: &Principal, name: &str, body: &Value) -> Result<Value> {
    let def = api::find_method(name)
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no method {name}")))?;
    if !who.scope().allows(def.scope) {
        return Err(match who {
            Principal::Anonymous => err(ErrorCode::Unauthenticated, "a bearer token is required"),
            _ => err(
                ErrorCode::Forbidden,
                format!(
                    "{name} needs {} scope; this credential has {}",
                    def.scope,
                    who.scope()
                ),
            ),
        });
    }
    let c = Ctx { hub, who };
    routes!(&c, name, body;
        GetState => get_state,
        ListSleeves => list_sleeves,
        GetTranscript => get_transcript,
        SpawnSleeve => spawn,
        SendMessage => send_message,
        Interrupt => interrupt,
        StopSleeve => stop,
        RemoveSleeve => remove,
        StartSleeve => start,
        Resleeve => resleeve,
        TakeoverTerminal => takeover,
        ListApprovals => list_approvals,
        Decide => decide,
        ListNodes => list_nodes,
        GetNode => get_node,
        RunJob => run_job,
        GetJob => get_job,
        GetJobLog => get_job_log,
        CancelJob => cancel_job,
        ListQuotas => list_quotas,
        ListEvents => list_events,
        MarkEventsRead => mark_events_read,
        EnvoySend => envoy_send,
        EnvoyWait => envoy_wait,
        EnvoyRead => envoy_read,
        GetEnvoyToken => get_envoy_token,
        Wait => wait,
        GetHubInfo => get_hub_info,
        ListDevices => list_devices,
        AddDevice => add_device,
        PairDevice => pair_device,
        RevokeDevice => revoke_device,
    )
}

// ---- access --------------------------------------------------------------------------------

/// A sleeve credential reaches itself and its children; Envoy and the owner reach all.
fn reaches(state: &State, who: &Principal, id: SleeveId) -> bool {
    match who {
        Principal::Sleeve(x) => id == *x || state.sleeve(id).and_then(|s| s.parent) == Some(*x),
        other => other.full(),
    }
}

/// A sleeve named by id or by name, not removed.
fn find(state: &State, s: &str) -> Result<SleeveId> {
    let live = |id: SleeveId| state.sleeve(id).is_some_and(|s| s.life != Life::Removed);
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
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no sleeve {s:?}")))
}

fn target_sleeve(c: &Ctx<'_>, view: &View, s: &str) -> Result<SleeveId> {
    let id = find(&view.state, s)?;
    if !reaches(&view.state, c.who, id) {
        return Err(err(
            ErrorCode::Forbidden,
            "that sleeve is outside this capability",
        ));
    }
    Ok(id)
}

fn command(hub: &Hub, kind: EventKind) -> Result<Reply> {
    let r = hub.command(kind, COMMAND_TIMEOUT).map_err(|e| match e {
        CommandError::Timeout => err(ErrorCode::Timeout, "the core did not answer"),
        CommandError::Down => err(ErrorCode::Unavailable, "the core is stopped"),
    })?;
    if let Reply::Refused(code, why) = r {
        let code = match code {
            Code::InvalidArgument => ErrorCode::Invalid,
            Code::NotFound => ErrorCode::NotFound,
            Code::AlreadyExists | Code::FailedPrecondition => ErrorCode::Conflict,
        };
        return Err(err(code, why));
    }
    Ok(r)
}

fn sleeve_view(hub: &Hub, id: SleeveId) -> Result<api::Sleeve> {
    let view = hub.view();
    view.state
        .sleeve(id)
        .map(|s| wire::sleeve(hub, &view, s))
        .ok_or_else(|| err(ErrorCode::NotFound, format!("sleeve {} is gone", id.0)))
}

fn new_id(prefix: &str) -> Result<String> {
    Ok(format!("{prefix}{}", random_hex(16).map_err(internal)?))
}

fn timeout(ms: u64) -> Duration {
    Duration::from_millis(ms).min(MAX_WAIT)
}

// ---- state ---------------------------------------------------------------------------------

fn get_state(c: &Ctx<'_>, _: api::Empty) -> Result<api::HubState> {
    let (seq, _) = c.hub.bus.mark();
    let view = c.hub.view();
    Ok(wire::state(c.hub, &view, seq, &|_| true))
}

fn list_sleeves(c: &Ctx<'_>, _: api::Empty) -> Result<api::SleeveList> {
    let view = c.hub.view();
    let st = &view.state;
    Ok(api::SleeveList {
        sleeves: st
            .sleeves
            .values()
            .filter(|s| s.life != Life::Removed && reaches(st, c.who, s.id))
            .map(|s| wire::sleeve(c.hub, &view, s))
            .collect(),
    })
}

fn transcript(
    hub: &Hub,
    id: SleeveId,
    from: Option<u64>,
    limit: Option<u64>,
) -> Result<api::Transcript> {
    let limit = limit.unwrap_or(TRANSCRIPT_DEFAULT).clamp(1, TRANSCRIPT_MAX) as usize;
    let store = hub.transcripts.get(id).map_err(internal)?;
    let store = lock(&store);
    let (lines, more) = match from {
        Some(f) => {
            let mut l = store
                .after(f.saturating_sub(1), limit + 1)
                .map_err(internal)?;
            let more = l.len() > limit;
            l.truncate(limit);
            (l, more)
        }
        None => (store.last(limit).map_err(internal)?, false),
    };
    Ok(api::Transcript {
        sleeve_id: id.0.to_string(),
        entries: lines
            .iter()
            .map(|l| wire::chat_entry(id, l, false))
            .collect(),
        more,
    })
}

fn get_transcript(c: &Ctx<'_>, r: api::GetTranscriptRequest) -> Result<api::Transcript> {
    let view = c.hub.view();
    let id = target_sleeve(c, &view, &r.sleeve_id)?;
    transcript(c.hub, id, r.from_seq, r.limit)
}

// ---- sleeves -------------------------------------------------------------------------------

fn canonical(p: &str) -> Result<String> {
    let path =
        std::fs::canonicalize(p).map_err(|e| err(ErrorCode::Invalid, format!("path {p}: {e}")))?;
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| err(ErrorCode::Invalid, "path must be UTF-8"))
}

fn targets(hub: &Hub, list: &[api::Harness]) -> Vec<Target> {
    if list.is_empty() {
        return hub
            .config
            .resleeve_to
            .iter()
            .filter_map(|h| wire::harness_from(h))
            .map(|h| wire::target(&hub.config, h, "", ""))
            .collect();
    }
    list.iter()
        .map(|h| wire::target(&hub.config, wire::core_harness(*h), "", ""))
        .collect()
}

fn spawn(c: &Ctx<'_>, r: api::SpawnSleeveRequest) -> Result<api::Sleeve> {
    let hub = c.hub;
    let view = hub.view();
    let st = &view.state;
    if r.name.is_empty() {
        return Err(err(ErrorCode::Invalid, "name is required"));
    }
    if r.mode == api::Mode::Pane {
        return Err(unavailable("pane mode"));
    }
    let parent = match (&r.parent_id, c.who) {
        (None, Principal::Sleeve(x)) => Some(*x),
        (None, _) => None,
        (Some(p), _) => Some(find(st, p)?),
    };
    if let Principal::Sleeve(x) = c.who
        && parent != Some(*x)
    {
        return Err(err(
            ErrorCode::Forbidden,
            "a sleeve spawns only its own children",
        ));
    }
    let layout = match (parent, r.view) {
        (None, None) => Layout::Root,
        (None, Some(_)) => return Err(err(ErrorCode::Invalid, "view is for children only")),
        (Some(_), Some(api::WorkspaceView::Readonly)) => Layout::View,
        (Some(_), _) => Layout::Worktree,
    };
    let parent_path = parent
        .and_then(|p| st.sleeve(p))
        .map(|p| p.claim.path.clone());
    let given = r.path.as_deref().unwrap_or("");
    let path = match (layout, given) {
        (Layout::Root, "") => return Err(err(ErrorCode::Invalid, "path is required for a root")),
        (Layout::Root, p) => canonical(p)?,
        (Layout::Worktree, "") => {
            let dir: PathBuf = hub.home.worktrees().join(format!(
                "{}-{}",
                r.name,
                random_hex(6).map_err(internal)?
            ));
            dir.to_string_lossy().into_owned()
        }
        (Layout::Worktree, p) => {
            let p = Path::new(p);
            let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("/");
            let file = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
            format!("{}/{file}", canonical(dir)?)
        }
        (Layout::View, "") => parent_path.unwrap_or_default(),
        (Layout::View, p) => canonical(p)?,
    };
    let target = wire::target(
        &hub.config,
        wire::core_harness(r.harness),
        r.provider.as_deref().unwrap_or(""),
        r.model.as_deref().unwrap_or(""),
    );
    rule::gate(hub, c.who, Guarded::Spawn(&r.name))?;
    let spec = SpawnSpec {
        request_id: new_id("spawn:")?,
        name: r.name,
        parent,
        layout,
        claim: Claim {
            path,
            writable: layout != Layout::View,
        },
        target,
        resleeve_to: targets(hub, &r.resleeve_to),
        budget: r.budget,
        disk_limit: None,
    };
    let id = match command(hub, EventKind::Spawn(spec))? {
        Reply::Spawned(id) => id,
        other => return Err(internal(format!("unexpected reply {other:?}"))),
    };
    if let Some(text) = r.message.filter(|t| !t.is_empty()) {
        command(
            hub,
            EventKind::Send {
                to: id,
                request_id: new_request(c, "")?,
                from: c.who.party(),
                text: prefix(&view.state, c.who, "", &text),
                mode: SendMode::Queue,
            },
        )?;
    }
    sleeve_view(hub, id)
}

/// What the harness reads: mail from a sleeve and agent messages are marked (Envoy's CLAUDE.md
/// relies on it); the owner's and Envoy's own are not.
fn prefix(state: &State, who: &Principal, agent: &str, text: &str) -> String {
    match who {
        Principal::Sleeve(x) => {
            let name = state.sleeve(*x).map_or("?", |s| s.name.as_str());
            format!("[from sleeve {name}] {text}")
        }
        Principal::EnvoyOnly => format!("[from agent:{agent}] {text}"),
        _ => text.to_string(),
    }
}

/// A request id that names its sender's kind; `Hub::from_label` reads the agent name back.
fn new_request(c: &Ctx<'_>, agent: &str) -> Result<String> {
    let r = random_hex(16).map_err(internal)?;
    Ok(match c.who {
        Principal::EnvoyOnly => format!("ga:{agent}:{r}"),
        Principal::Envoy(_) => format!("envoy:{r}"),
        Principal::Sleeve(x) => format!("mail:{}:{r}", x.0),
        _ => format!("owner:{r}"),
    })
}

fn send_message(c: &Ctx<'_>, r: api::SendMessageRequest) -> Result<api::Sent> {
    let view = c.hub.view();
    let id = target_sleeve(c, &view, &r.sleeve_id)?;
    if r.text.is_empty() {
        return Err(err(ErrorCode::Invalid, "text is required"));
    }
    let request_id = new_request(c, "")?;
    command(
        c.hub,
        EventKind::Send {
            to: id,
            request_id: request_id.clone(),
            from: c.who.party(),
            text: prefix(&view.state, c.who, "", &r.text),
            mode: match r.delivery {
                api::Delivery::Queue => SendMode::Queue,
                api::Delivery::Steer => SendMode::Steer,
            },
        },
    )?;
    Ok(api::Sent { request_id })
}

fn interrupt(c: &Ctx<'_>, r: api::SleeveRef) -> Result<api::Empty> {
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    command(c.hub, EventKind::Interrupt { sleeve: id })?;
    Ok(api::Empty {})
}

fn stop(c: &Ctx<'_>, r: api::SleeveRef) -> Result<api::Accepted> {
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    command(c.hub, EventKind::Stop { sleeve: id })?;
    Ok(api::Accepted { approval_id: None })
}

fn remove(c: &Ctx<'_>, r: api::SleeveRef) -> Result<api::Accepted> {
    let view = c.hub.view();
    let id = target_sleeve(c, &view, &r.sleeve_id)?;
    let name = view.state.sleeve(id).map_or("", |s| s.name.as_str());
    let approval_id = rule::gate(c.hub, c.who, Guarded::Remove(name))?;
    command(c.hub, EventKind::Remove { sleeve: id })?;
    Ok(api::Accepted { approval_id })
}

fn optional_target(
    hub: &Hub,
    h: Option<api::Harness>,
    provider: Option<String>,
    model: Option<String>,
) -> Option<Target> {
    h.map(|h| {
        wire::target(
            &hub.config,
            wire::core_harness(h),
            provider.as_deref().unwrap_or(""),
            model.as_deref().unwrap_or(""),
        )
    })
}

fn start(c: &Ctx<'_>, r: api::StartSleeveRequest) -> Result<api::Accepted> {
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    let target = optional_target(c.hub, r.target, r.provider, r.model);
    command(c.hub, EventKind::Start { sleeve: id, target })?;
    Ok(api::Accepted { approval_id: None })
}

fn resleeve(c: &Ctx<'_>, r: api::ResleeveRequest) -> Result<api::Accepted> {
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    let target = optional_target(c.hub, Some(r.target), r.provider, r.model);
    command(c.hub, EventKind::Resleeve { sleeve: id, target })?;
    Ok(api::Accepted { approval_id: None })
}

fn takeover(_: &Ctx<'_>, _: api::SleeveRef) -> Result<api::Empty> {
    Err(unavailable("terminal takeover"))
}

// ---- approvals -----------------------------------------------------------------------------

fn list_approvals(c: &Ctx<'_>, r: api::ListApprovalsRequest) -> Result<api::ApprovalList> {
    Ok(api::ApprovalList {
        approvals: c.hub.approvals.list(r.pending_only),
    })
}

fn decide(c: &Ctx<'_>, r: api::DecideRequest) -> Result<api::Approval> {
    let hub = c.hub;
    let (a, origin) = hub.approvals.get(&r.approval_id).ok_or_else(|| {
        err(
            ErrorCode::NotFound,
            format!("no approval {}", r.approval_id),
        )
    })?;
    if a.state != api::ApprovalState::Pending {
        return Err(err(
            ErrorCode::Conflict,
            format!("approval {} is already {}", a.id, a.state),
        ));
    }
    let allow = r.decision != api::Decision::Deny;
    if a.kind == api::ApprovalKind::Action && r.decision == api::Decision::AllowSession {
        return Err(err(
            ErrorCode::Invalid,
            "an action approval is allow_once or deny",
        ));
    }
    if a.kind == api::ApprovalKind::Tool {
        command(
            hub,
            EventKind::RespondApproval {
                sleeve: origin.sleeve,
                approval: origin.native.clone(),
                allow,
                note: String::new(),
            },
        )?;
    }
    let state = if allow {
        api::ApprovalState::Approved
    } else {
        api::ApprovalState::Denied
    };
    let done = hub
        .approvals
        .resolve(
            &hub.bus,
            &a.id,
            state,
            Some(r.decision),
            api::DecidedBy::Owner,
        )
        .ok_or_else(|| err(ErrorCode::Conflict, format!("approval {} just ended", a.id)))?;
    if a.id.starts_with(ACTION_PREFIX) {
        let g = hub.view().state.sleeve(origin.sleeve).map_or(0, |s| s.g.0);
        hub.record(
            origin.sleeve,
            g,
            now_ms(),
            Entry::ApprovalDecision {
                id: a.id.clone(),
                decision: if allow {
                    pt_drivers::Decision::AllowOnce
                } else {
                    pt_drivers::Decision::Deny
                },
                by: Some("owner".into()),
                confirmed: true,
            },
            Note::default(),
        );
    }
    Ok(done)
}

// ---- nodes and jobs ------------------------------------------------------------------------

fn list_nodes(_: &Ctx<'_>, _: api::Empty) -> Result<api::NodeList> {
    Ok(api::NodeList { nodes: Vec::new() })
}

fn get_node(_: &Ctx<'_>, r: api::NodeRef) -> Result<api::Node> {
    Err(err(
        ErrorCode::NotFound,
        format!("no node {} (no node transport yet)", r.node_id),
    ))
}

fn run_job(c: &Ctx<'_>, r: api::RunJobRequest) -> Result<api::Job> {
    if r.argv.is_empty() {
        return Err(err(ErrorCode::Invalid, "argv is required"));
    }
    let on = match &r.target {
        api::JobTarget::Node { node_id } => format!("node {node_id}"),
        api::JobTarget::Arch { arch } => format!("a node of arch {arch}"),
    };
    rule::gate(
        c.hub,
        c.who,
        Guarded::RunJob(format!("{} on {on}", r.argv.join(" "))),
    )?;
    Err(unavailable("node jobs (the node transport)"))
}

fn get_job(_: &Ctx<'_>, _: api::JobRef) -> Result<api::Job> {
    Err(unavailable("node jobs (the node transport)"))
}

fn get_job_log(_: &Ctx<'_>, _: api::GetJobLogRequest) -> Result<api::JobLog> {
    Err(unavailable("node jobs (the node transport)"))
}

fn cancel_job(_: &Ctx<'_>, _: api::JobRef) -> Result<api::Job> {
    Err(unavailable("node jobs (the node transport)"))
}

// ---- quota and events ----------------------------------------------------------------------

fn list_quotas(c: &Ctx<'_>, _: api::Empty) -> Result<api::QuotaList> {
    Ok(api::QuotaList {
        quotas: wire::quotas(c.hub),
    })
}

fn list_events(c: &Ctx<'_>, r: api::ListEventsRequest) -> Result<api::EventList> {
    let from = r.from_seq.unwrap_or(0);
    let limit = r.limit.unwrap_or(500) as usize;
    Ok(api::EventList {
        events: c
            .hub
            .bus
            .events()
            .into_iter()
            .filter(|e| e.seq >= from)
            .take(limit)
            .collect(),
    })
}

fn mark_events_read(c: &Ctx<'_>, r: api::MarkEventsReadRequest) -> Result<api::Empty> {
    for event in c.hub.bus.mark_read(r.up_to_seq) {
        c.hub
            .bus
            .push(|seq| api::HubStreamMessage::Event { seq, event });
    }
    Ok(api::Empty {})
}

// ---- Envoy ---------------------------------------------------------------------------------

fn envoy(view: &View) -> Result<SleeveId> {
    Hub::envoy_id(&view.state).ok_or_else(|| err(ErrorCode::Unavailable, "Envoy is not running"))
}

fn envoy_send(c: &Ctx<'_>, r: api::EnvoySendRequest) -> Result<api::Sent> {
    if matches!(c.who, Principal::Sleeve(_) | Principal::Envoy(_)) {
        return Err(err(
            ErrorCode::Forbidden,
            "sleeves reach Envoy through their parent, not EnvoySend",
        ));
    }
    let view = c.hub.view();
    let id = envoy(&view)?;
    let agent = r
        .agent
        .as_deref()
        .filter(|a| !a.is_empty())
        .unwrap_or("agent");
    if !agent
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(err(ErrorCode::Invalid, "agent is a name of [A-Za-z0-9_-]"));
    }
    if r.text.is_empty() {
        return Err(err(ErrorCode::Invalid, "text is required"));
    }
    let request_id = new_request(c, agent)?;
    command(
        c.hub,
        EventKind::Send {
            to: id,
            request_id: request_id.clone(),
            from: c.who.party(),
            text: prefix(&view.state, c.who, agent, &r.text),
            mode: SendMode::Queue,
        },
    )?;
    Ok(api::Sent { request_id })
}

fn envoy_read(c: &Ctx<'_>, r: api::EnvoyReadRequest) -> Result<api::Transcript> {
    let id = envoy(&c.hub.view())?;
    transcript(c.hub, id, Some(r.from_seq), r.limit)
}

fn get_envoy_token(c: &Ctx<'_>, _: api::Empty) -> Result<api::EnvoyToken> {
    Ok(api::EnvoyToken {
        token: c.hub.tokens.envoy_only(),
    })
}

/// The assistant text of the turn that answered `request_id`, with the last seq read, once that
/// turn settled (or, with `idle`, whatever it said before the sleeve went idle).
fn answer(hub: &Hub, id: SleeveId, request_id: &str, idle: bool) -> Option<(String, u64)> {
    let store = hub.transcripts.get(id).ok()?;
    let lines = lock(&store).all().ok()?;
    let start = lines.iter().rposition(
        |e| matches!(&e.entry, Entry::UserMessage { request_id: r, .. } if r == request_id),
    )?;
    let mut text = Vec::new();
    let mut seq = lines[start].seq;
    for e in &lines[start + 1..] {
        match &e.entry {
            Entry::AssistantText { text: t, .. } => text.push(t.clone()),
            Entry::TurnSettled { .. } => return Some((text.join("\n"), e.seq)),
            Entry::UserMessage { .. } => break,
            _ => {}
        }
        seq = e.seq;
    }
    idle.then(|| (text.join("\n"), seq))
}

/// The latest message from `party` to Envoy: queued, then being sent, then in the transcript.
fn latest_from(hub: &Hub, view: &View, id: SleeveId, party: Party) -> Result<String> {
    let s = view
        .state
        .sleeve(id)
        .ok_or_else(|| err(ErrorCode::NotFound, "no Envoy"))?;
    let mine = |r: &str| s.messages.get(r).is_some_and(|m| m.from == party);
    if let Some(r) = s.queue.iter().rfind(|r| mine(r)) {
        return Ok(r.clone());
    }
    let sending = s.messages.iter().find(|(_, m)| {
        m.from == party && matches!(m.state, MsgState::InFlight(_) | MsgState::SteerHeld)
    });
    if let Some((r, _)) = sending {
        return Ok(r.clone());
    }
    let store = hub.transcripts.get(id).map_err(internal)?;
    let lines = lock(&store).all().map_err(internal)?;
    let from_log = lines.iter().rev().find_map(|e| match &e.entry {
        Entry::UserMessage { request_id, .. } if mine(request_id) => Some(request_id.clone()),
        _ => None,
    });
    from_log.ok_or_else(|| err(ErrorCode::NotFound, "no message from you to wait for"))
}

fn envoy_wait(c: &Ctx<'_>, r: api::EnvoyWaitRequest) -> Result<api::EnvoyReply> {
    let hub = c.hub;
    let view = hub.view();
    let id = envoy(&view)?;
    let party = c.who.party();
    let request_id = match r.request_id.filter(|r| !r.is_empty()) {
        Some(r) => r,
        None => latest_from(hub, &view, id, party)?,
    };
    match view
        .state
        .sleeve(id)
        .and_then(|s| s.messages.get(&request_id))
    {
        None => {
            return Err(err(ErrorCode::NotFound, format!("no message {request_id}")));
        }
        Some(m) if *c.who == Principal::EnvoyOnly && m.from != Party::Agent => {
            return Err(err(ErrorCode::Forbidden, "that message is not yours"));
        }
        Some(_) => {}
    }
    let deadline = Instant::now() + timeout(r.timeout_ms);
    let got = wait_for(hub, deadline, |v| {
        let s = v.state.sleeve(id)?;
        match s.messages.get(&request_id)?.state {
            MsgState::Rejected => Some(Err(err(
                ErrorCode::Conflict,
                format!("message {request_id} was rejected by the harness"),
            ))),
            MsgState::MayHaveApplied => Some(Err(err(
                ErrorCode::Conflict,
                format!("message {request_id} may or may not have reached Envoy"),
            ))),
            MsgState::Delivered => {
                let idle = matches!(s.activity, pt_core::Activity::Idle { .. });
                answer(hub, id, &request_id, idle).map(Ok)
            }
            _ => None,
        }
    });
    match got {
        Some(Ok((text, seq))) => Ok(api::EnvoyReply {
            request_id,
            settled: true,
            text,
            seq,
        }),
        Some(Err(e)) => Err(e),
        None => {
            let (text, seq) = answer(hub, id, &request_id, true).unwrap_or_default();
            Ok(api::EnvoyReply {
                request_id,
                settled: false,
                text,
                seq,
            })
        }
    }
}

// ---- wait ----------------------------------------------------------------------------------

/// Re-evaluates `check` on every change until it answers or `deadline` passes. The bus position
/// is read before the view, so a change between the two wakes the wait at once.
fn wait_for<T>(
    hub: &Hub,
    deadline: Instant,
    mut check: impl FnMut(&View) -> Option<T>,
) -> Option<T> {
    loop {
        let pos = hub.bus.pos();
        let view = hub.view();
        if let Some(x) = check(&view) {
            return Some(x);
        }
        if Instant::now() >= deadline {
            return None;
        }
        hub.bus.wait_after(pos, deadline);
    }
}

fn reached(state: &State, id: SleeveId, until: api::WaitUntil) -> bool {
    let Some(s) = state.sleeve(id) else {
        return false;
    };
    let idle = matches!(s.activity, pt_core::Activity::Idle { .. })
        && s.queue.is_empty()
        && s.life == Life::Running;
    match until {
        api::WaitUntil::Idle => idle,
        api::WaitUntil::Done => idle && s.activity == (pt_core::Activity::Idle { seen: false }),
        api::WaitUntil::Blocked => matches!(s.activity, pt_core::Activity::Blocked { .. }),
        api::WaitUntil::Parked => matches!(s.life, Life::Parked(_)),
        api::WaitUntil::Ended => matches!(s.life, Life::Stopped | Life::Removed),
        api::WaitUntil::Report => false,
    }
}

fn wait(c: &Ctx<'_>, r: api::WaitRequest) -> Result<api::WaitResult> {
    if r.until == api::WaitUntil::Report {
        return Err(unavailable("waiting for a report (sleeve mail upward)"));
    }
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    let deadline = Instant::now() + timeout(r.timeout_ms);
    let got = wait_for(c.hub, deadline, |v| {
        if reached(&v.state, id, r.until) {
            return Some(true);
        }
        // A sleeve that stopped or parked never gets there: answer instead of hanging.
        let s = v.state.sleeve(id)?;
        matches!(s.life, Life::Stopped | Life::Removed | Life::Parked(_)).then_some(false)
    });
    Ok(api::WaitResult {
        reached: got == Some(true),
        sleeve: sleeve_view(c.hub, id)?,
    })
}

fn get_hub_info(c: &Ctx<'_>, _: api::Empty) -> Result<api::HubInfo> {
    Ok(wire::hub_info(c.hub))
}

// ---- devices -------------------------------------------------------------------------------

fn list_devices(c: &Ctx<'_>, _: api::Empty) -> Result<api::DeviceList> {
    Ok(api::DeviceList {
        devices: c.hub.devices.list(),
    })
}

fn add_device(c: &Ctx<'_>, r: api::AddDeviceRequest) -> Result<api::PairingCode> {
    let name = r.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(err(ErrorCode::Invalid, "name is 1 to 64 bytes"));
    }
    let (code, expires) = c.hub.devices.add_code(name).map_err(internal)?;
    Ok(api::PairingCode {
        name: name.to_string(),
        code,
        expires_at_ms: api::EpochMs(expires),
    })
}

fn pair_device(c: &Ctx<'_>, r: api::PairDeviceRequest) -> Result<api::PairedDevice> {
    match c.hub.devices.pair(&r.code).map_err(internal)? {
        Some((device, token)) => {
            c.hub.event(
                api::EventKind::Hub,
                api::Level::Info,
                "hub",
                "owner",
                &format!("device {} paired", device.name),
            );
            Ok(api::PairedDevice {
                device,
                token,
                expires_at_ms: None,
            })
        }
        None => Err(err(
            ErrorCode::Unauthenticated,
            "unknown or expired pairing code",
        )),
    }
}

fn revoke_device(c: &Ctx<'_>, r: api::DeviceRef) -> Result<api::Empty> {
    if !c.hub.devices.revoke(&r.device_id).map_err(internal)? {
        return Err(err(
            ErrorCode::NotFound,
            format!("no device {}", r.device_id),
        ));
    }
    Ok(api::Empty {})
}
