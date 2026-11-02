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

use crate::fleet::{Status as FleetStatus, Up};
use crate::home::{now_ms, random_hex};
use crate::hub::{CommandError, Hub, View};
use crate::jobs::JobRequest;
use crate::log::info;
use crate::rule::{self, Guarded};
use crate::transcript::lock;
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

/// The cgroup of `pid` from `/proc/<pid>/cgroup`, or `None` when it cannot be read (the
/// process is gone, or never was).
pub fn peer_cgroup(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()
}

pub enum CgroupSleeve {
    /// In no sleeve scope of this hub.
    None,
    Live(Principal),
    /// In the scope of a sleeve that is removed or unknown: refused.
    Gone,
}

/// The sleeve whose scope a cgroup is. A process inside a sleeve is that sleeve whatever token
/// it presents.
pub fn cgroup_sleeve(hub: &Hub, view: &View, cgroup: &str) -> CgroupSleeve {
    let needle = format!("/pt-{}-", hub.tokens.hub_tag);
    let Some(at) = cgroup.find(&needle).map(|i| i + needle.len()) else {
        return CgroupSleeve::None;
    };
    let digits: String = cgroup[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let Ok(n) = digits.parse() else {
        return CgroupSleeve::Gone;
    };
    let id = SleeveId(n);
    match view.state.sleeve(id) {
        Some(s) if s.life != Life::Removed => CgroupSleeve::Live(sleeve_principal(&view.state, id)),
        _ => CgroupSleeve::Gone,
    }
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
    crate::doctor::precheck(hub, name, body)?;
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
        GetEnrollment => get_enrollment,
        AddNode => add_node,
        ApproveNode => approve_node,
        RemoveNode => remove_node,
        RenameNode => rename_node,
        RunJob => run_job,
        ListJobs => list_jobs,
        GetJob => get_job,
        GetJobLog => get_job_log,
        GetJobArtifact => get_job_artifact,
        CancelJob => cancel_job,
        ListQuotas => list_quotas,
        ListEvents => list_events,
        MarkEventsRead => mark_events_read,
        EnvoySend => envoy_send,
        EnvoyWait => envoy_wait,
        EnvoyRead => envoy_read,
        GetEnvoyToken => get_envoy_token,
        Wait => wait,
        SearchHistory => search_history,
        ReadHistory => read_history,
        GetHubInfo => get_hub_info,
        ListDevices => list_devices,
        AddDevice => add_device,
        PairDevice => pair_device,
        RevokeDevice => revoke_device,
        GetDoctor => get_doctor,
        UpdateHarnesses => update_harnesses,
        UpdateHub => update_hub,
        UpdateNodes => update_nodes,
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

pub(crate) fn command(hub: &Hub, kind: EventKind) -> Result<Reply> {
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
        // A new worktree's path need not exist yet; when it does, it is claimed by its real
        // path, and a symlink there is refused (it could name another sleeve's worktree).
        (Layout::Worktree, p) => match std::fs::symlink_metadata(p) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(err(ErrorCode::Invalid, format!("path {p} is a symlink")));
            }
            Ok(_) => canonical(p)?,
            Err(_) => {
                let p = Path::new(p);
                let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("/");
                match p.file_name().and_then(|f| f.to_str()) {
                    Some(file) if file != ".." => format!("{}/{file}", canonical(dir)?),
                    _ => return Err(err(ErrorCode::Invalid, "path names no directory")),
                }
            }
        },
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
        effort: r.effort.filter(|e| !e.is_empty()),
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
    let view = c.hub.view();
    Ok(api::ApprovalList {
        approvals: view
            .state
            .approvals
            .values()
            .filter(|a| !r.pending_only || a.state == pt_core::ApprovalState::Pending)
            .map(crate::approvals::view)
            .collect(),
    })
}

fn decide(c: &Ctx<'_>, r: api::DecideRequest) -> Result<api::Approval> {
    crate::approvals::decide(c.hub, &r.approval_id, r.decision)
}

// ---- nodes and jobs ------------------------------------------------------------------------

const LOG_DEFAULT: u64 = 64 << 10;
const LOG_MAX: u64 = 1 << 20;
const ARTIFACT_MAX: u64 = 512 << 10;
const JOBS_DEFAULT: u64 = 200;

fn fleet(hub: &Hub) -> Result<Up> {
    match hub.fleet.status() {
        FleetStatus::Up(u) => Ok(u),
        FleetStatus::Off => Err(err(
            ErrorCode::Unavailable,
            "no fleet is configured (config.json fleet.listen or serve --fleet-listen)",
        )),
        FleetStatus::Down(why) => Err(err(ErrorCode::Unavailable, why)),
    }
}

fn fleet_err(e: pt_fleet::Error) -> ApiError {
    let m = e.to_string();
    let code = if m.contains("already exists") {
        ErrorCode::Conflict
    } else if m.contains("control key mismatch") {
        ErrorCode::Forbidden
    } else {
        ErrorCode::Invalid
    };
    err(code, m)
}

fn list_nodes(c: &Ctx<'_>, _: api::Empty) -> Result<api::NodeList> {
    if matches!(c.hub.fleet.status(), FleetStatus::Off) {
        return Ok(api::NodeList { nodes: Vec::new() });
    }
    let up = fleet(c.hub)?;
    Ok(api::NodeList {
        nodes: up.poller.nodes().into_iter().map(|v| v.node).collect(),
    })
}

/// Whether `s` names the node `(id, name)`: digits are an id (no name is digits alone), anything
/// else a name.
fn names(s: &str, id: &str, name: &str) -> bool {
    if s.bytes().all(|b| b.is_ascii_digit()) {
        s == id
    } else {
        s == name
    }
}

fn find_node(up: &Up, s: &str) -> Result<pt_fleet::NodeView> {
    up.poller
        .nodes()
        .into_iter()
        .find(|v| names(s, &v.node.id, &v.node.name))
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no node {s:?}")))
}

/// A fleet node by headscale id or name, as `(id, name)`: the poller's views, then headscale's
/// list (the poller has not seen it yet), then with `records` the enrollment records (a removal
/// cut short left one).
fn resolve_node(up: &Up, s: &str, records: bool) -> Result<(u64, String)> {
    if let Ok(v) = find_node(up, s) {
        return Ok((v.node.id.parse().map_err(internal)?, v.node.name));
    }
    let listed = up.fleet.admin().nodes().map_err(internal)?;
    if let Some(n) = listed
        .into_iter()
        .filter(|n| n.has_tag(pt_fleet::TAG_NODE))
        .find(|n| names(s, &n.id.to_string(), &n.name))
    {
        return Ok((n.id, n.name));
    }
    if records
        && let Some(e) = up
            .fleet
            .enrolled()
            .map_err(internal)?
            .into_iter()
            .find(|e| names(s, &e.id.to_string(), &e.name))
    {
        return Ok((e.id, e.name));
    }
    Err(err(ErrorCode::NotFound, format!("no node {s:?}")))
}

fn get_node(c: &Ctx<'_>, r: api::NodeRef) -> Result<api::Node> {
    Ok(find_node(&fleet(c.hub)?, &r.node_id)?.node)
}

fn pending_node(p: &pt_fleet::Pending, fp: &str) -> api::PendingNode {
    let r = &p.registration;
    api::PendingNode {
        auth_id: r.auth_id.clone(),
        name: r.name.clone(),
        control_key: r.control_key.clone(),
        seen_at_ms: api::EpochMs(p.seen_at_ms),
        expires_at_ms: api::EpochMs(p.seen_at_ms + pt_fleet::PENDING_TTL_MS),
        key_matches: r.control_key.eq_ignore_ascii_case(fp),
    }
}

fn get_enrollment(c: &Ctx<'_>, _: api::Empty) -> Result<api::Enrollment> {
    let up = fleet(c.hub)?;
    let fp = up.fleet.control_fingerprint().map_err(internal)?;
    let url = up.fleet.config().server_url.clone();
    let pending = up.fleet.pending().map_err(internal)?;
    Ok(api::Enrollment {
        join_command: format!("protectorate-node join {url} --name <name>"),
        server_url: url,
        pending: pending.iter().map(|p| pending_node(p, &fp)).collect(),
        control_key: fp,
    })
}

fn add_node(c: &Ctx<'_>, r: api::AddNodeRequest) -> Result<api::PendingNode> {
    let up = fleet(c.hub)?;
    let reg = pt_fleet::Registration::parse(&r.registration)
        .map_err(|e| err(ErrorCode::Invalid, e.to_string()))?;
    let fp = up.fleet.control_fingerprint().map_err(internal)?;
    up.fleet.record_pending(reg.clone()).map_err(fleet_err)?;
    let p = up
        .fleet
        .pending()
        .map_err(internal)?
        .into_iter()
        .find(|p| p.registration.auth_id == reg.auth_id)
        .ok_or_else(|| err(ErrorCode::Internal, "the registration was not recorded"))?;
    let shown = pending_node(&p, &fp);
    c.hub.event(
        api::EventKind::Node,
        api::Level::Info,
        &reg.name,
        "owner",
        &format!(
            "node {} asks to join{}",
            reg.name,
            if shown.key_matches {
                ""
            } else {
                " (it saw another control key)"
            }
        ),
    );
    Ok(shown)
}

fn approve_node(c: &Ctx<'_>, r: api::ApproveNodeRequest) -> Result<api::ApprovedNode> {
    let up = fleet(c.hub)?;
    let pending = up
        .fleet
        .pending()
        .map_err(internal)?
        .into_iter()
        .find(|p| p.registration.auth_id == r.auth_id);
    let from_pending = |f: fn(&pt_fleet::Registration) -> &String, what: &str| {
        pending
            .as_ref()
            .map(|p| f(&p.registration).clone())
            .ok_or_else(|| {
                err(
                    ErrorCode::Invalid,
                    format!("{what} is required: no pending registration {}", r.auth_id),
                )
            })
    };
    let name = match r.name {
        Some(n) => n,
        None => from_pending(|g| &g.name, "name")?,
    };
    let fingerprint = match r.fingerprint {
        Some(f) => f,
        None => from_pending(|g| &g.control_key, "fingerprint")?,
    };
    let n = up
        .fleet
        .approve(&r.auth_id, &name, &fingerprint)
        .map_err(fleet_err)?;
    info!("node {} approved as headscale node {}", n.name, n.id);
    c.hub.event(
        api::EventKind::Node,
        api::Level::Info,
        &n.name,
        "owner",
        &format!("node {} approved", n.name),
    );
    Ok(api::ApprovedNode {
        node_id: n.id.to_string(),
        name: n.name,
    })
}

/// A queued job that may run only on node `id`.
fn pinned_to(j: &pt_core::Job, id: &str) -> bool {
    j.state == pt_core::JobState::Queued
        && JobRequest::of(&j.spec)
            .is_some_and(|r| matches!(r.target, api::JobTarget::Node { node_id } if node_id == id))
}

fn remove_node(c: &Ctx<'_>, r: api::RemoveNodeRequest) -> Result<api::RemovedNode> {
    let up = fleet(c.hub)?;
    let (id, name) = resolve_node(&up, &r.node_id, true)?;
    let key = id.to_string();
    let viewed = up.poller.node(&key);
    let view = c.hub.view();
    let in_flight: Vec<String> = view
        .state
        .jobs
        .values()
        .filter(|j| {
            j.node == Some(pt_core::NodeId(id))
                && j.state != pt_core::JobState::Queued
                && !crate::jobs::ended(j.state)
        })
        .map(|j| j.id.0.to_string())
        .collect();
    if !r.force {
        if viewed.as_ref().is_some_and(|v| v.node.online) {
            return Err(err(
                ErrorCode::Conflict,
                format!(
                    "node {name} is online; run protectorate-node uninstall on it first, \
                     or remove it with force"
                ),
            ));
        }
        if !in_flight.is_empty() {
            return Err(err(
                ErrorCode::Conflict,
                format!(
                    "node {name} has jobs {} in flight; cancel them and let them settle, \
                     or remove it with force",
                    in_flight.join(", ")
                ),
            ));
        }
    }
    let (removal, forgotten) = up.poller.remove(id);
    if forgotten {
        crate::fleet::removed(c.hub, id);
    }
    let removal = removal.map_err(|e| {
        let m = e.to_string();
        if m.contains("enrollment record") {
            c.hub.event(
                api::EventKind::Node,
                api::Level::Alarm,
                &name,
                "owner",
                &format!("node {name}: {m}"),
            );
            err(ErrorCode::Internal, m)
        } else {
            fleet_err(e)
        }
    })?;
    // The scheduler fails the same jobs on its own (a crash here, a job queued while its
    // approval was pending); this only makes it immediate.
    let mut failed = Vec::new();
    for j in c
        .hub
        .view()
        .state
        .jobs
        .values()
        .filter(|j| pinned_to(j, &key))
    {
        c.hub.submit(EventKind::JobNodeRemoved {
            job: j.id,
            node: pt_core::NodeId(id),
        });
        failed.push(j.id.0.to_string());
    }
    let mut text = format!("node {name} removed");
    if !failed.is_empty() {
        text.push_str(&format!("; queued jobs {} fail", failed.join(", ")));
    }
    if !in_flight.is_empty() {
        text.push_str(&format!("; jobs {} were in flight", in_flight.join(", ")));
    }
    info!("{text} (headscale node {id})");
    c.hub.event(
        api::EventKind::Node,
        api::Level::Info,
        &name,
        "owner",
        &text,
    );
    Ok(api::RemovedNode {
        node_id: key,
        name: removal.name,
        failed_jobs: failed,
        orphaned_jobs: in_flight,
        record_only: removal.record_only,
    })
}

fn rename_node(c: &Ctx<'_>, r: api::RenameNodeRequest) -> Result<api::RenamedNode> {
    let up = fleet(c.hub)?;
    let (id, from) = resolve_node(&up, &r.node_id, false)?;
    let n = up.poller.rename(id, &r.name).map_err(|e| {
        let m = e.to_string();
        if m.contains("failed too") {
            c.hub.event(
                api::EventKind::Node,
                api::Level::Alarm,
                &from,
                "owner",
                &format!("node {from}: {m}"),
            );
            err(ErrorCode::Internal, m)
        } else if m.contains("enrollment record") {
            err(ErrorCode::Internal, m)
        } else {
            fleet_err(e)
        }
    })?;
    if n.name != from {
        if let Some(v) = up.poller.node(&id.to_string()) {
            crate::fleet::renamed(c.hub, v.node);
        }
        let text = format!("node {from} renamed {}", n.name);
        info!("{text} (headscale node {id})");
        c.hub.event(
            api::EventKind::Node,
            api::Level::Info,
            &n.name,
            "owner",
            &text,
        );
    }
    Ok(api::RenamedNode {
        node_id: id.to_string(),
        from,
        to: n.name,
    })
}

/// The job's requester as the caller may see it: the owner sees every job, a sleeve its own
/// and its children's.
fn job_visible(state: &State, who: &Principal, req: Option<&JobRequest>) -> bool {
    match req.and_then(|r| r.sleeve) {
        _ if who.full() => true,
        Some(id) => reaches(state, who, SleeveId(id)),
        None => false,
    }
}

fn find_job(c: &Ctx<'_>, view: &View, id: &str) -> Result<pt_core::JobId> {
    let id = id
        .parse::<u64>()
        .map(pt_core::JobId)
        .map_err(|_| err(ErrorCode::NotFound, format!("no job {id:?}")))?;
    let j = view
        .state
        .jobs
        .get(&id)
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no job {}", id.0)))?;
    if !job_visible(&view.state, c.who, JobRequest::of(&j.spec).as_ref()) {
        return Err(err(
            ErrorCode::Forbidden,
            "that job is outside this capability",
        ));
    }
    Ok(id)
}

fn job_view(hub: &Hub, id: pt_core::JobId) -> Result<api::Job> {
    let view = hub.view();
    view.state
        .jobs
        .get(&id)
        .map(|j| crate::jobs::view(hub, j))
        .ok_or_else(|| err(ErrorCode::NotFound, format!("job {} is gone", id.0)))
}

/// A cwd is judged by the target node's OS. For `--arch` the node is not chosen yet, and a node
/// that has not answered Hello has no known OS: the node checks then, and the job fails there.
fn check_cwd(c: &Ctx<'_>, target: &api::JobTarget, cwd: &str) -> Result<()> {
    let api::JobTarget::Node { node_id } = target else {
        return Ok(());
    };
    let os = fleet(c.hub)
        .ok()
        .and_then(|up| find_node(&up, node_id).ok())
        .and_then(|n| n.hello)
        .map(|h| h.os);
    let (ok, shape) = match os.as_deref() {
        Some("windows") => (
            windows_absolute(cwd),
            r"a drive path like C:\dir or C:/dir, or a UNC path",
        ),
        Some(_) => (cwd.starts_with('/'), "a path starting with /"),
        None => return Ok(()),
    };
    if ok {
        return Ok(());
    }
    Err(err(
        ErrorCode::Invalid,
        format!(
            "cwd must be an absolute path on node {node_id} ({}): {shape}",
            os.unwrap_or_default()
        ),
    ))
}

fn windows_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
        || p.starts_with("\\\\")
}

fn run_job(c: &Ctx<'_>, r: api::RunJobRequest) -> Result<api::Job> {
    if r.argv.is_empty() {
        return Err(err(ErrorCode::Invalid, "argv is required"));
    }
    if let Some(d) = &r.cwd {
        check_cwd(c, &r.target, d)?;
    }
    let timeout_secs = r
        .timeout_secs
        .unwrap_or(pt_frame::msg::DEFAULT_TIMEOUT_SECS);
    if timeout_secs == 0 || timeout_secs > pt_frame::msg::MAX_TIMEOUT_SECS {
        return Err(err(ErrorCode::Invalid, "timeout_secs is 1 s to 24 h"));
    }
    let up = fleet(c.hub)?;
    // A node target is kept as its id: a later rename, or another node taking the name, does
    // not move the job.
    let (target, on) = match r.target {
        api::JobTarget::Node { node_id } => {
            let (id, name) = resolve_node(&up, &node_id, false)?;
            if let Some(e) = up
                .poller
                .node(&id.to_string())
                .and_then(|v| v.protocol_error())
            {
                return Err(err(ErrorCode::Unavailable, e.to_string()));
            }
            let node_id = id.to_string();
            (api::JobTarget::Node { node_id }, format!("node {name}"))
        }
        api::JobTarget::Arch { arch } => {
            let on = format!("a node of arch {arch}");
            (api::JobTarget::Arch { arch }, on)
        }
    };
    let what = format!("{} on {on}", r.argv.join(" "));
    let approval_id = rule::gate(c.hub, c.who, Guarded::RunJob(what))?;
    let view = c.hub.view();
    let sleeve = match c.who {
        Principal::Envoy(x) | Principal::Sleeve(x) => Some(*x),
        _ => None,
    };
    let requested_by = sleeve
        .and_then(|x| view.state.sleeve(x))
        .map_or_else(|| "owner".to_string(), |s| s.name.clone());
    let req = JobRequest {
        argv: r.argv,
        target,
        cwd: r.cwd,
        artifacts: r.artifacts,
        on_present: r.on_present,
        timeout_secs,
        maintenance: r.maintenance,
        requested_by,
        sleeve: sleeve.map(|x| x.0),
        approval_id,
        queued_at_ms: now_ms(),
    };
    let spec = req.spec(r.even_if_present);
    let request_id = new_id("job:")?;
    match command(c.hub, EventKind::CreateJob { request_id, spec })? {
        Reply::JobCreated(id) => job_view(c.hub, id),
        other => Err(internal(format!("CreateJob answered {other:?}"))),
    }
}

fn list_jobs(c: &Ctx<'_>, r: api::ListJobsRequest) -> Result<api::JobList> {
    let view = c.hub.view();
    let node = match &r.node_id {
        Some(n) => Some(match n.parse::<u64>() {
            Ok(id) => id,
            Err(_) => resolve_node(&fleet(c.hub)?, n, false)?.0,
        }),
        None => None,
    };
    let limit = r.limit.unwrap_or(JOBS_DEFAULT).min(10_000) as usize;
    let mut jobs: Vec<api::Job> = view
        .state
        .jobs
        .values()
        .rev()
        .filter(|j| node.is_none_or(|n| j.node.map(|x| x.0) == Some(n)))
        .filter(|j| job_visible(&view.state, c.who, JobRequest::of(&j.spec).as_ref()))
        .take(limit)
        .map(|j| crate::jobs::view(c.hub, j))
        .collect();
    jobs.reverse();
    Ok(api::JobList {
        jobs,
        scoped_to_caller: matches!(c.who, Principal::Sleeve(_)),
    })
}

fn get_job(c: &Ctx<'_>, r: api::JobRef) -> Result<api::Job> {
    let id = find_job(c, &c.hub.view(), &r.job_id)?;
    job_view(c.hub, id)
}

fn get_job_log(c: &Ctx<'_>, r: api::GetJobLogRequest) -> Result<api::JobLog> {
    let view = c.hub.view();
    let id = find_job(c, &view, &r.job_id)?;
    let j = &view.state.jobs[&id];
    let limit = r.limit.unwrap_or(LOG_DEFAULT).clamp(1, LOG_MAX);
    let (bytes, len) = c
        .hub
        .jobs
        .read_log(id, j.attempt, r.offset, limit)
        .map_err(internal)?;
    let next = r.offset.min(len) + bytes.len() as u64;
    Ok(api::JobLog {
        job_id: r.job_id,
        offset: r.offset,
        next_offset: next,
        text: String::from_utf8_lossy(&bytes).into_owned(),
        eof: crate::jobs::ended(j.state) && next >= len,
    })
}

fn get_job_artifact(c: &Ctx<'_>, r: api::GetJobArtifactRequest) -> Result<api::JobArtifactData> {
    let view = c.hub.view();
    let id = find_job(c, &view, &r.job_id)?;
    let j = &view.state.jobs[&id];
    let limit = r.limit.unwrap_or(ARTIFACT_MAX).clamp(1, ARTIFACT_MAX);
    let (f, bytes) = c
        .hub
        .jobs
        .read_artifact(id, j.attempt, &r.path, r.offset, limit)
        .map_err(internal)?
        .ok_or_else(|| {
            err(
                ErrorCode::NotFound,
                format!("job {} has no fetched artifact {:?}", id.0, r.path),
            )
        })?;
    let next = r.offset.min(f.size_bytes) + bytes.len() as u64;
    Ok(api::JobArtifactData {
        job_id: r.job_id,
        path: r.path,
        offset: r.offset,
        next_offset: next,
        size_bytes: f.size_bytes,
        sha256: f.sha256,
        hex: pt_hash::to_hex(&bytes),
        eof: next >= f.size_bytes,
    })
}

fn cancel_job(c: &Ctx<'_>, r: api::JobRef) -> Result<api::Job> {
    let id = find_job(c, &c.hub.view(), &r.job_id)?;
    command(c.hub, EventKind::CancelJob { job: id })?;
    job_view(c.hub, id)
}

// ---- quota and events ----------------------------------------------------------------------

fn list_quotas(c: &Ctx<'_>, _: api::Empty) -> Result<api::QuotaList> {
    Ok(api::QuotaList {
        quotas: c.hub.quotas.list(),
    })
}

/// From `from_seq` on, or without it the newest `limit` (500), oldest first either way.
fn list_events(c: &Ctx<'_>, r: api::ListEventsRequest) -> Result<api::EventList> {
    let limit = r.limit.unwrap_or(500) as usize;
    let events = match r.from_seq {
        Some(from) => c.hub.bus.events_from(from, limit),
        None => c.hub.bus.events_latest(limit),
    };
    Ok(api::EventList { events })
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

/// The owner's scope reads Envoy's whole transcript; vendor agents read only their own
/// messages and the answers of the turns those started; a sleeve reads none of it.
fn envoy_read(c: &Ctx<'_>, r: api::EnvoyReadRequest) -> Result<api::Transcript> {
    let id = envoy(&c.hub.view())?;
    match c.who {
        Principal::Sleeve(_) => Err(not_for_sleeves()),
        Principal::EnvoyOnly => agent_transcript(c.hub, id, r.from_seq, r.limit),
        _ => transcript(c.hub, id, Some(r.from_seq), r.limit),
    }
}

fn not_for_sleeves() -> ApiError {
    err(
        ErrorCode::Forbidden,
        "sleeves reach Envoy through their parent, not the Envoy methods",
    )
}

/// What vendor agents see of Envoy's transcript: each `ga:` message, then the assistant text
/// and settle of the turn it started, until another sender's message. No tool calls, no
/// reasoning, nothing the owner or a sleeve said.
fn agent_transcript(
    hub: &Hub,
    id: SleeveId,
    from: u64,
    limit: Option<u64>,
) -> Result<api::Transcript> {
    let limit = limit.unwrap_or(TRANSCRIPT_DEFAULT).clamp(1, TRANSCRIPT_MAX) as usize;
    let store = hub.transcripts.get(id).map_err(internal)?;
    let lines = lock(&store).all().map_err(internal)?;
    let mut agent = false;
    let mut shown = lines.iter().filter(|l| {
        let keep = match &l.entry {
            Entry::UserMessage { request_id, .. } => {
                agent = request_id.starts_with("ga:");
                agent
            }
            Entry::AssistantText { .. } => agent,
            Entry::TurnSettled { .. } => std::mem::replace(&mut agent, false),
            _ => false,
        };
        keep && l.seq >= from
    });
    let entries: Vec<_> = shown
        .by_ref()
        .take(limit)
        .map(|l| wire::chat_entry(id, l, false))
        .collect();
    Ok(api::Transcript {
        sleeve_id: id.0.to_string(),
        entries,
        more: shown.next().is_some(),
    })
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
    if matches!(c.who, Principal::Sleeve(_)) {
        return Err(not_for_sleeves());
    }
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
pub(crate) fn wait_for<T>(
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

// ---- history -------------------------------------------------------------------------------

const HISTORY_DEFAULT: u64 = 10;
const HISTORY_MAX: u64 = 100;
const HISTORY_READ_MAX: usize = 200;

fn history_err(e: crate::history::Failure) -> ApiError {
    match e {
        crate::history::Failure::Off(_) => err(ErrorCode::Unavailable, e.to_string()),
        crate::history::Failure::Io(_) => internal(e),
    }
}

/// The index's sleeve keys a history call covers; `None` is every sleeve. Names match removed
/// sleeves too, since their history stays.
fn history_sleeves(c: &Ctx<'_>, st: &State, asked: &[String]) -> Result<Option<Vec<String>>> {
    let mut ids: Vec<SleeveId> = Vec::new();
    for a in asked {
        let named: Vec<SleeveId> = st
            .sleeves
            .values()
            .filter(|s| s.id.0.to_string() == *a || s.name == *a)
            .map(|s| s.id)
            .collect();
        if named.is_empty() {
            return Err(err(ErrorCode::NotFound, format!("no sleeve {a:?}")));
        }
        let reached: Vec<SleeveId> = named
            .into_iter()
            .filter(|id| reaches(st, c.who, *id))
            .collect();
        if reached.is_empty() {
            return Err(err(
                ErrorCode::Forbidden,
                "that sleeve is outside this capability",
            ));
        }
        ids.extend(reached);
    }
    if ids.is_empty() {
        if c.who.full() {
            return Ok(None);
        }
        ids = st
            .sleeves
            .values()
            .filter(|s| reaches(st, c.who, s.id))
            .map(|s| s.id)
            .collect();
    }
    Ok(Some(ids.iter().map(|i| i.0.to_string()).collect()))
}

fn doc_sleeve(d: &pt_history::Doc) -> Option<SleeveId> {
    d.sleeve.as_deref()?.parse().ok().map(SleeveId)
}

fn history_role(r: pt_history::Role) -> api::HistoryRole {
    match r {
        pt_history::Role::User => api::HistoryRole::User,
        pt_history::Role::Assistant => api::HistoryRole::Assistant,
        pt_history::Role::ToolCall => api::HistoryRole::ToolCall,
        pt_history::Role::ToolError => api::HistoryRole::ToolError,
        pt_history::Role::Compaction => api::HistoryRole::Compaction,
    }
}

fn history_doc(hub: &Hub, st: &State, d: &pt_history::Doc) -> api::HistoryDoc {
    let id = doc_sleeve(d);
    api::HistoryDoc {
        doc_id: d.id.0,
        sleeve_id: id.map(|i| i.0.to_string()).unwrap_or_default(),
        sleeve: id
            .and_then(|i| st.sleeve(i))
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        harness: hub.history.harness(&d.session).or(match d.harness {
            Some(pt_drivers::Harness::Claude) => Some(api::Harness::Claude),
            Some(pt_drivers::Harness::Codex) => Some(api::Harness::Codex),
            Some(pt_drivers::Harness::Grok) => Some(api::Harness::Grok),
            Some(pt_drivers::Harness::Pi) | None => None,
        }),
        session: d.session.clone(),
        turn: u64::from(d.turn),
        role: history_role(d.role),
        time_ms: api::EpochMs(d.time_ms),
        tool: d.tool.clone(),
        files: d.files.clone(),
        path: d.path.to_string_lossy().into_owned(),
        offset: d.offset,
    }
}

fn search_history(c: &Ctx<'_>, r: api::SearchHistoryRequest) -> Result<api::HistoryHits> {
    if r.query.trim().is_empty() {
        return Err(err(ErrorCode::Invalid, "query is required"));
    }
    let view = c.hub.view();
    let st = &view.state;
    let harness = match r.harness {
        Some(h) => Some(
            crate::exec::driver_harness(wire::core_harness(h))
                .ok_or_else(|| err(ErrorCode::Invalid, format!("{h} has no history")))?,
        ),
        None => None,
    };
    let filters = pt_history::Filters {
        sleeves: history_sleeves(c, st, &r.sleeves)?.unwrap_or_default(),
        harness,
        session: r.session,
        since_ms: r.since_ms.map(|t| t.0),
        until_ms: r.until_ms.map(|t| t.0),
        roles: r
            .roles
            .iter()
            .map(|x| match x {
                api::HistoryRole::User => pt_history::Role::User,
                api::HistoryRole::Assistant => pt_history::Role::Assistant,
                api::HistoryRole::ToolCall => pt_history::Role::ToolCall,
                api::HistoryRole::ToolError => pt_history::Role::ToolError,
                api::HistoryRole::Compaction => pt_history::Role::Compaction,
            })
            .collect(),
    };
    let query = pt_history::Query {
        text: r.query,
        mode: match r.mode {
            api::HistoryMode::All => pt_history::Mode::All,
            api::HistoryMode::Any => pt_history::Mode::Any,
            api::HistoryMode::Phrase => pt_history::Mode::Phrase,
        },
    };
    let limit = r.limit.unwrap_or(HISTORY_DEFAULT).clamp(1, HISTORY_MAX) as usize;
    let hits = c
        .hub
        .history
        .search(&query, &filters, limit)
        .map_err(history_err)?;
    Ok(api::HistoryHits {
        hits: hits
            .iter()
            .map(|h| api::HistoryHit {
                doc: history_doc(c.hub, st, &h.doc),
                score: h.score,
                snippet: h.snippet.text.clone(),
                highlights: h
                    .snippet
                    .highlights
                    .iter()
                    .map(|&(s, e)| api::HistorySpan {
                        start: s as u64,
                        end: e as u64,
                    })
                    .collect(),
            })
            .collect(),
        indexed_docs: c.hub.history.docs(),
    })
}

fn read_history(c: &Ctx<'_>, r: api::ReadHistoryRequest) -> Result<api::HistoryRead> {
    let turn = |t: u64| u32::try_from(t).unwrap_or(u32::MAX);
    let (sel, what) = match (r.doc_id, r.session) {
        (Some(id), None) => (
            pt_history::Selector::Doc(pt_history::DocId(id)),
            format!("no history doc {id}"),
        ),
        (None, Some(session)) => (
            pt_history::Selector::Turns {
                from: turn(r.from_turn),
                to: r.to_turn.map_or(u32::MAX, turn),
                session: session.clone(),
            },
            format!("no history of session {session} in that turn range"),
        ),
        _ => return Err(err(ErrorCode::Invalid, "give doc_id or session, not both")),
    };
    let view = c.hub.view();
    let st = &view.state;
    let mut docs: Vec<pt_history::Doc> = c
        .hub
        .history
        .read(&sel)
        .map_err(history_err)?
        .into_iter()
        .filter(|d| match doc_sleeve(d) {
            Some(id) => reaches(st, c.who, id),
            None => c.who.full(),
        })
        .collect();
    // Out of reach reads the same as absent.
    if docs.is_empty() {
        return Err(err(ErrorCode::NotFound, what));
    }
    let more = docs.len() > HISTORY_READ_MAX;
    docs.truncate(HISTORY_READ_MAX);
    Ok(api::HistoryRead {
        docs: docs
            .into_iter()
            .map(|d| api::HistoryText {
                doc: history_doc(c.hub, st, &d),
                text: d.text,
            })
            .collect(),
        more,
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

// ---- doctor and updates --------------------------------------------------------------------

fn get_doctor(c: &Ctx<'_>, r: api::GetDoctorRequest) -> Result<api::DoctorReport> {
    if matches!(c.who, Principal::Sleeve(_)) {
        return Err(err(
            ErrorCode::Forbidden,
            "the doctor report is for the owner and Envoy",
        ));
    }
    let report = if r.refresh {
        Some(crate::doctor::run_now(c.hub))
    } else {
        c.hub.doctor.report()
    };
    Ok(crate::doctor::api_report(c.hub, report.as_deref()))
}

fn update_harnesses(c: &Ctx<'_>, r: api::UpdateHarnessesRequest) -> Result<api::HarnessUpdates> {
    Ok(api::HarnessUpdates {
        programs: crate::harness_update::request(c.hub, &r.programs),
    })
}

fn update_hub(c: &Ctx<'_>, r: api::UpdateHubRequest) -> Result<api::HubUpdate> {
    crate::update::update_hub(c.hub, r)
}

fn update_nodes(c: &Ctx<'_>, r: api::UpdateNodesRequest) -> Result<api::NodeUpdates> {
    crate::update::update_nodes(c.hub, r)
}
