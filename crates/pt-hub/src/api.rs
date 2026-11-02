//! The handler layer: what each caller (`principal.rs`) may touch, and one function per method
//! of pt-api's table. Commands become core events whose replies come back after the journal
//! fsync; reads use the published view.

use std::path::Path;
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, Method, method as m};
use pt_core::{
    ApprovalState, Claim, EventKind, Layout, Life, MsgKind, MsgState, Party, Reply, SendMode,
    SleeveId, Source, SpawnSpec, State, Target, name_ok, overlap_refusal, path_ok, path_within,
};
use pt_drivers::{Entry, PromptKind};
use pt_json::{FromJson, ToJson, Value};

use crate::approvals;
use crate::command::{command, err, wait_for};
use crate::envoy::{d9_origin, envoy_id};
use crate::fleet::{Status as FleetStatus, Up};
use crate::home::{now_ms, random_hex};
use crate::hub::Hub;
use crate::jobs::JobRequest;
use crate::lock::lock;
use crate::log::info;
use crate::principal::Principal;
use crate::rule::{self, Guarded};
use crate::snapshot;
use crate::transcript::Note;
use crate::view::View;
use crate::wire;

const MAX_WAIT: Duration = Duration::from_secs(3600);
const TRANSCRIPT_DEFAULT: u64 = 2000;
const TRANSCRIPT_MAX: u64 = 10_000;

type Result<T> = std::result::Result<T, ApiError>;

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
    precheck(hub, name, body)?;
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

/// The API's gate: SpawnSleeve, StartSleeve and Resleeve onto an ineligible harness are refused
/// before they reach the core. Bodies that do not parse are left to the handler.
fn precheck(hub: &Hub, method: &str, body: &Value) -> Result<()> {
    let harness = if method == m::SpawnSleeve::NAME {
        api::SpawnSleeveRequest::from_json(body)
            .ok()
            .map(|r| wire::core_harness(r.harness))
    } else if method == m::Resleeve::NAME {
        api::ResleeveRequest::from_json(body)
            .ok()
            .map(|r| wire::core_harness(r.target))
    } else if method == m::StartSleeve::NAME {
        api::StartSleeveRequest::from_json(body)
            .ok()
            .and_then(|r| match r.target {
                Some(t) => Some(wire::core_harness(t)),
                None => {
                    let view = hub.view();
                    let st = &view.state;
                    st.sleeves
                        .values()
                        .find(|s| {
                            s.life != Life::Removed
                                && (s.id.0.to_string() == r.sleeve_id || s.name == r.sleeve_id)
                        })
                        .map(|s| s.target.harness)
                }
            })
    } else {
        None
    };
    match harness {
        Some(h) => hub
            .doctor
            .check(&hub.config, h)
            .map_err(|why| ApiError::new(ErrorCode::Conflict, why)),
        None => Ok(()),
    }
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

fn sleeve_view(hub: &Hub, id: SleeveId) -> Result<api::Sleeve> {
    let view = hub.view();
    view.state
        .sleeve(id)
        .map(|s| wire::sleeve(&hub.transcripts, &view, s))
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
    Ok(snapshot::state(c.hub, &view, seq, &|_| true))
}

fn list_sleeves(c: &Ctx<'_>, _: api::Empty) -> Result<api::SleeveList> {
    let view = c.hub.view();
    let st = &view.state;
    Ok(api::SleeveList {
        sleeves: st
            .sleeves
            .values()
            .filter(|s| s.life != Life::Removed && reaches(st, c.who, s.id))
            .map(|s| wire::sleeve(&c.hub.transcripts, &view, s))
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
    let path = match std::fs::canonicalize(p) {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(err(ErrorCode::Invalid, format!("the hub cannot see {p}")));
        }
        Err(e) => return Err(err(ErrorCode::Invalid, format!("path {p}: {e}"))),
    };
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| err(ErrorCode::Invalid, "path must be UTF-8"))
}

/// Branch of a repo the hub creates. A child worktree needs a commit to branch from.
const NEW_PROJECT_BRANCH: &str = "main";

/// The outermost root or writable claim that overlaps `path`, for the 409.
fn overlap_claim(st: &State, path: &str) -> Option<String> {
    overlap_refusal(
        st.sleeves.values().filter(|s| s.life != Life::Removed),
        path,
        "directory",
    )
}

fn name_taken(st: &State, name: &str) -> bool {
    st.sleeves
        .values()
        .any(|s| s.life != Life::Removed && s.name == name)
}

/// The absolute directory a new project will occupy, or why it will not.
fn resolve_new_project(hub: &Hub, given: &str) -> Result<String> {
    let given = given.strip_suffix('/').unwrap_or(given);
    let configured = hub.config.projects_dir.as_str();
    if configured.is_empty() {
        return Err(err(
            ErrorCode::Invalid,
            "projects_dir is not set; new projects are off",
        ));
    }
    if !path_ok(configured) {
        return Err(err(
            ErrorCode::Invalid,
            "projects_dir must be an absolute normalized path",
        ));
    }
    let root = match std::fs::canonicalize(configured) {
        Ok(p) if p.is_dir() => p,
        Ok(p) => {
            return Err(err(
                ErrorCode::Invalid,
                format!("projects_dir {} is not a directory", p.display()),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(err(
                ErrorCode::Invalid,
                format!("the hub cannot see projects_dir {configured}"),
            ));
        }
        Err(e) => {
            return Err(err(
                ErrorCode::Invalid,
                format!("projects_dir {configured}: {e}"),
            ));
        }
    };
    let Some(root_s) = root.to_str() else {
        return Err(err(ErrorCode::Invalid, "projects_dir must be UTF-8"));
    };
    // `path_within` treats `/` as containing every absolute path.
    if root_s == "/" {
        return Err(err(ErrorCode::Invalid, "projects_dir must not be /"));
    }
    if given.contains('\0') {
        return Err(err(
            ErrorCode::Invalid,
            format!("new_path {given} is not a normalized path"),
        ));
    }
    // An absolute path may name the configured spelling when `projects_dir` is a symlink.
    // The claim is still the canonical directory.
    let full = if given.starts_with('/') {
        if configured != root_s && path_within(given, configured) && given != configured {
            format!("{root_s}{}", &given[configured.len()..])
        } else {
            given.to_string()
        }
    } else {
        format!("{root_s}/{given}")
    };
    if !path_ok(&full) {
        return Err(err(
            ErrorCode::Invalid,
            format!("new_path {given} is not a normalized path"),
        ));
    }
    if !path_within(&full, root_s) || full == root_s {
        return Err(err(
            ErrorCode::Invalid,
            format!("{given} is outside projects_dir ({root_s})"),
        ));
    }
    if let Ok(home) = std::fs::canonicalize(&hub.home.root)
        && let Some(home) = home.to_str()
        && path_within(&full, home)
    {
        return Err(err(
            ErrorCode::Invalid,
            "a project cannot be inside the hub's home",
        ));
    }
    let Some(rel) = full.strip_prefix(root_s).and_then(|r| r.strip_prefix('/')) else {
        return Err(err(
            ErrorCode::Invalid,
            format!("{given} is outside projects_dir"),
        ));
    };
    let mut cur = root;
    let names: Vec<&str> = rel.split('/').collect();
    for (i, name) in names.iter().enumerate() {
        let next = cur.join(name);
        let last = i + 1 == names.len();
        match std::fs::symlink_metadata(&next) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(err(
                    ErrorCode::Invalid,
                    format!("path {} goes through a symlink", next.display()),
                ));
            }
            Ok(_) if last => {
                return Err(err(ErrorCode::Conflict, exists_reason(hub, &next)));
            }
            Ok(m) if m.is_dir() => cur = next,
            Ok(_) => {
                return Err(err(
                    ErrorCode::Invalid,
                    format!("{} is not a directory", next.display()),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && last => {
                return next
                    .to_str()
                    .map(str::to_string)
                    .ok_or_else(|| err(ErrorCode::Invalid, "path must be UTF-8"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(err(
                    ErrorCode::Invalid,
                    format!("the hub cannot see the parent {}", next.display()),
                ));
            }
            Err(e) => {
                return Err(err(
                    ErrorCode::Invalid,
                    format!("path {}: {e}", next.display()),
                ));
            }
        }
    }
    Err(err(ErrorCode::Invalid, "new_path names no directory"))
}

/// Why a path that is already on disk cannot be a new project.
fn exists_reason(hub: &Hub, path: &Path) -> String {
    let Some(text) = path.to_str() else {
        return format!(
            "{} already exists; sleeve spawn --path adopts it",
            path.display()
        );
    };
    if let Some(why) = overlap_claim(&hub.view().state, text) {
        return why;
    }
    if path.join(".git").symlink_metadata().is_ok() && crate::git::head(path).is_none() {
        return format!(
            "{text} already exists and its repository has no commit; remove it and retry, or \
             commit in it before sleeve spawn --path"
        );
    }
    format!("{text} already exists; sleeve spawn --path adopts it")
}

fn create_project(hub: &Hub, path: &Path) -> Result<String> {
    if let Err(e) = std::fs::create_dir(path) {
        return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
            err(ErrorCode::Conflict, exists_reason(hub, path))
        } else {
            err(
                ErrorCode::Invalid,
                format!("create {}: {e}", path.display()),
            )
        });
    }
    info!("project: created {}", path.display());
    if let Err(why) = same_real_path(path) {
        return Err(err(
            ErrorCode::Invalid,
            format!("kept {}: {why}", path.display()),
        ));
    }
    match crate::git::init_empty(path, NEW_PROJECT_BRANCH, "Initial commit") {
        Ok(commit) => Ok(commit),
        Err(e) => {
            let kept = !remove_failed_init(hub, path);
            let extra = if kept { "; kept the directory" } else { "" };
            Err(err(
                ErrorCode::Internal,
                format!("git init {}: {e}{extra}", path.display()),
            ))
        }
    }
}

/// True when `path` canonicalizes to itself. A symlink planted under it must not be followed.
fn same_real_path(path: &Path) -> std::result::Result<(), String> {
    match std::fs::canonicalize(path) {
        Ok(c) if c.as_path() == path => Ok(()),
        Ok(c) => Err(format!("it resolves to {}", c.display())),
        Err(e) => Err(e.to_string()),
    }
}

/// Removes a directory whose `git init` failed, so a retry is not blocked by an empty dir.
/// A symlink, a claimed path, or anything that does not canonicalize to `path` is left alone.
fn remove_failed_init(hub: &Hub, path: &Path) -> bool {
    if same_real_path(path).is_err() || claimed(hub, path) || !plain_dir(path) {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for ent in entries {
        let Ok(ent) = ent else {
            return false;
        };
        if ent.file_name() != ".git" || !plain_dir(&ent.path()) {
            return false;
        }
    }
    if claimed(hub, path) {
        return false;
    }
    remove_project_dir(path).is_ok()
}

fn remove_project_dir(path: &Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(path).inspect(|_| info!("project: removed {}", path.display()))
}

fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_symlink() && m.is_dir())
}

fn claimed(hub: &Hub, path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return true;
    };
    overlap_claim(&hub.view().state, path).is_some()
}

enum Kept {
    Gone,
    Still(String),
}

/// Drops a repo this call made after the spawn was refused. Keeps it when a live claim overlaps
/// it, it no longer canonicalizes to itself, or it is no longer that empty commit.
fn settle_new_project(hub: &Hub, path: &str, commit: &str) -> Kept {
    if let Some(why) = overlap_claim(&hub.view().state, path) {
        return Kept::Still(why);
    }
    let p = Path::new(path);
    if let Err(why) = same_real_path(p) {
        return Kept::Still(why);
    }
    if !plain_dir(p) || !repo_still_ours(p, commit) {
        return Kept::Still("it is no longer the empty repository this call made".into());
    }
    // A claim can land between the check above and the delete.
    if let Some(why) = overlap_claim(&hub.view().state, path) {
        return Kept::Still(why);
    }
    match remove_project_dir(p) {
        Ok(()) => Kept::Gone,
        Err(e) => Kept::Still(e.to_string()),
    }
}

fn repo_still_ours(path: &Path, commit: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    let mut git = false;
    for ent in entries {
        let Ok(ent) = ent else {
            return false;
        };
        if ent.file_name() != ".git" || !plain_dir(&ent.path()) {
            return false;
        }
        git = true;
    }
    git && crate::git::head(path).as_deref() == Some(commit)
}

fn with_kept(e: ApiError, path: &str, kept: Kept) -> ApiError {
    match kept {
        Kept::Gone => e,
        Kept::Still(why) => err(e.code, format!("{}; kept {path} ({why})", e.message)),
    }
}

/// A timeout or a lost reply may mean the core already journaled the spawn. Any other refusal
/// removes the directory only when it is still the empty repository this call made and no live
/// claim overlaps it.
fn after_create(hub: &Hub, path: &str, commit: Option<&str>, e: ApiError) -> ApiError {
    let Some(commit) = commit else {
        return e;
    };
    if matches!(e.code, ErrorCode::Timeout | ErrorCode::Unavailable) {
        return err(e.code, format!("{}; kept {path}", e.message));
    }
    with_kept(e, path, settle_new_project(hub, path, commit))
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

fn worktree_taken(root: &str, path: &Path) -> Result<bool> {
    if root.is_empty() {
        return Ok(false);
    }
    crate::git::worktree_registered(Path::new(root), path).map_err(|e| err(ErrorCode::Invalid, e))
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
    let creating = match r.new_path.as_deref() {
        Some("") => return Err(err(ErrorCode::Invalid, "new_path is empty")),
        Some(_) if !c.who.full() => {
            return Err(err(
                ErrorCode::Forbidden,
                "only the owner, a paired device or Envoy creates a project",
            ));
        }
        Some(_) if r.path.is_some() || r.parent_id.is_some() || r.view.is_some() => {
            return Err(err(
                ErrorCode::Invalid,
                "new_path makes a root and takes no path, parent or view",
            ));
        }
        other => other,
    };
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
    if creating.is_some() && name_taken(st, &r.name) {
        return Err(err(ErrorCode::Conflict, "a sleeve has that name"));
    }
    if creating.is_some() && !name_ok(&r.name) {
        return Err(err(
            ErrorCode::Invalid,
            "name must be 1-64 of [A-Za-z0-9._-]",
        ));
    }
    let path = if let Some(given) = creating {
        resolve_new_project(hub, given)?
    } else {
        match (layout, given) {
            (Layout::Root, "") => {
                return Err(err(ErrorCode::Invalid, "path is required for a root"));
            }
            (Layout::Root, p) => canonical(p)?,
            (Layout::Worktree, "") => {
                let root = parent_path.as_deref().unwrap_or("");
                let mut chosen = None;
                for _ in 0..8 {
                    let dir = hub.home.worktrees().join(format!(
                        "{}-{}",
                        r.name,
                        random_hex(6).map_err(internal)?
                    ));
                    match dir.symlink_metadata() {
                        Ok(_) => continue,
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            return Err(err(ErrorCode::Invalid, format!("{}: {e}", dir.display())));
                        }
                        Err(_) => {}
                    }
                    if worktree_taken(root, &dir)? {
                        continue;
                    }
                    chosen = Some(dir.to_string_lossy().into_owned());
                    break;
                }
                chosen.ok_or_else(|| err(ErrorCode::Invalid, "no free worktree path"))?
            }
            // A path that already exists is a leftover checkout or the owner's own tree.
            // Adopting it would reset that worktree. A symlink could name another sleeve's.
            (Layout::Worktree, p) => {
                let path = match std::fs::symlink_metadata(p) {
                    Ok(m) if m.file_type().is_symlink() => {
                        return Err(err(ErrorCode::Invalid, format!("path {p} is a symlink")));
                    }
                    Ok(_) => {
                        return Err(err(ErrorCode::Invalid, format!("path {p} already exists")));
                    }
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        return Err(err(ErrorCode::Invalid, format!("path {p}: {e}")));
                    }
                    Err(_) => {
                        let p = Path::new(p);
                        let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("/");
                        match p.file_name().and_then(|f| f.to_str()) {
                            Some(file) if file != ".." => format!("{}/{file}", canonical(dir)?),
                            _ => return Err(err(ErrorCode::Invalid, "path names no directory")),
                        }
                    }
                };
                if worktree_taken(parent_path.as_deref().unwrap_or(""), Path::new(&path))? {
                    return Err(err(
                        ErrorCode::Invalid,
                        format!("path {path} is already a worktree"),
                    ));
                }
                path
            }
            (Layout::View, "") => parent_path.unwrap_or_default(),
            (Layout::View, p) => canonical(p)?,
        }
    };
    let target = wire::target(
        &hub.config,
        wire::core_harness(r.harness),
        r.provider.as_deref().unwrap_or(""),
        r.model.as_deref().unwrap_or(""),
    );
    let resleeve_to = targets(hub, &r.resleeve_to);
    if creating.is_some()
        && let Some(why) = std::iter::once(&target)
            .chain(&resleeve_to)
            .find_map(Target::invalid_reason)
    {
        return Err(err(ErrorCode::Invalid, why));
    }
    if creating.is_some()
        && let Some(why) = overlap_claim(st, &path)
    {
        return Err(err(ErrorCode::Conflict, why));
    }
    let request_id = new_id("spawn:")?;
    let by_envoy = matches!(c.who, Principal::Envoy(_));
    // The gate reads the turn's spawn count, which the core moves when this spawn applies.
    let one_at_a_time = by_envoy.then(|| lock(&hub.d9_spawn));
    // Before the gate: an approval wait can outlive the turn that made this spawn count.
    let d9 = marks_d9(hub, c.who);
    rule::gate(hub, c.who, Guarded::Spawn(&r.name))?;
    // The gate can block for minutes. Re-check the name and the path, then create.
    let commit = if let Some(given) = creating {
        let fresh = hub.view();
        if name_taken(&fresh.state, &r.name) {
            return Err(err(ErrorCode::Conflict, "a sleeve has that name"));
        }
        let again = resolve_new_project(hub, given)?;
        if again != path {
            return Err(err(
                ErrorCode::Invalid,
                format!("new_path {given} now resolves to {again}"),
            ));
        }
        if let Some(why) = overlap_claim(&fresh.state, &path) {
            return Err(err(ErrorCode::Conflict, why));
        }
        Some(create_project(hub, Path::new(&path))?)
    } else {
        None
    };
    let spec = SpawnSpec {
        request_id,
        name: r.name,
        parent,
        layout,
        claim: Claim {
            path: path.clone(),
            writable: layout != Layout::View,
        },
        target,
        effort: r.effort.filter(|e| !e.is_empty()),
        resleeve_to,
        budget: r.budget,
        disk_limit: None,
        d9,
        by_envoy,
    };
    let spawned = command(hub, EventKind::Spawn(spec));
    drop(one_at_a_time);
    let id = match spawned {
        Ok(Reply::Spawned(id)) => id,
        Ok(other) => {
            return Err(after_create(
                hub,
                &path,
                commit.as_deref(),
                internal(format!("unexpected reply {other:?}")),
            ));
        }
        Err(e) => return Err(after_create(hub, &path, commit.as_deref(), e)),
    };
    if let Some(text) = r.message.filter(|t| !t.is_empty()) {
        command(
            hub,
            EventKind::Send {
                to: id,
                request_id: new_request(c, "")?,
                from: c.who.party(),
                agent: None,
                text: prefix(&view.state, c.who, "", &text),
                mode: SendMode::Queue,
                d9,
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

/// A request id that names its sender's kind, for logs. Nothing parses it: the message carries
/// its party and agent name.
fn new_request(c: &Ctx<'_>, agent: &str) -> Result<String> {
    let r = random_hex(16).map_err(internal)?;
    Ok(match c.who {
        Principal::EnvoyOnly => format!("ga:{agent}:{r}"),
        Principal::Envoy(_) => format!("envoy:{r}"),
        Principal::Sleeve(x) => format!("mail:{}:{r}", x.0),
        _ => format!("owner:{r}"),
    })
}

/// Envoy, in a turn that already has D9 origin, marks the sleeve it spawns or sends to.
fn marks_d9(hub: &Hub, who: &Principal) -> bool {
    match *who {
        Principal::Envoy(envoy) => d9_origin(&hub.view().state, envoy).is_some(),
        _ => false,
    }
}

/// `reaches`, plus a sleeve mailing its parent or Envoy. Nothing else opens.
fn may_mail(state: &State, who: &Principal, id: SleeveId) -> bool {
    if reaches(state, who, id) {
        return true;
    }
    let Principal::Sleeve(me) = *who else {
        return false;
    };
    envoy_id(state) == Some(id) || state.sleeve(me).and_then(|s| s.parent) == Some(id)
}

fn send_message(c: &Ctx<'_>, r: api::SendMessageRequest) -> Result<api::Sent> {
    let view = c.hub.view();
    let id = find(&view.state, &r.sleeve_id)?;
    if !may_mail(&view.state, c.who, id) {
        return Err(err(
            ErrorCode::Forbidden,
            "that sleeve is outside this capability",
        ));
    }
    if r.text.is_empty() {
        return Err(err(ErrorCode::Invalid, "text is required"));
    }
    let request_id = new_request(c, "")?;
    let from = c.who.party();
    let sender_marked =
        matches!(from, Party::Sleeve(id) if view.state.sleeve(id).is_some_and(|s| s.d9));
    command(
        c.hub,
        EventKind::Send {
            to: id,
            request_id: request_id.clone(),
            from,
            agent: None,
            text: prefix(&view.state, c.who, "", &r.text),
            mode: match r.delivery {
                api::Delivery::Queue => SendMode::Queue,
                api::Delivery::Steer => SendMode::Steer,
            },
            d9: marks_d9(c.hub, c.who) || sender_marked,
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
    let view = c.hub.view();
    let id = target_sleeve(c, &view, &r.sleeve_id)?;
    let target = if r.target.is_none() && (r.provider.is_some() || r.model.is_some()) {
        let mut target = view.state.sleeves[&id].target.clone();
        if let Some(provider) = r.provider {
            target.provider = provider;
        }
        if let Some(model) = r.model {
            target.model = model;
        }
        Some(target)
    } else {
        optional_target(c.hub, r.target, r.provider, r.model)
    };
    command(
        c.hub,
        EventKind::Start {
            sleeve: id,
            target,
            effort: r.effort.filter(|e| !e.is_empty()),
        },
    )?;
    Ok(api::Accepted { approval_id: None })
}

fn resleeve(c: &Ctx<'_>, r: api::ResleeveRequest) -> Result<api::Accepted> {
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
    let target = optional_target(c.hub, Some(r.target), r.provider, r.model);
    command(
        c.hub,
        EventKind::Resleeve {
            sleeve: id,
            target,
            effort: r.effort.filter(|e| !e.is_empty()),
        },
    )?;
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
            .filter(|a| !r.pending_only || a.state == ApprovalState::Pending)
            .map(approvals::view)
            .collect(),
    })
}

fn decide(c: &Ctx<'_>, r: api::DecideRequest) -> Result<api::Approval> {
    decide_approval(c.hub, &r.approval_id, r.decision)
}

/// The owner decides approval `id`. A tool decision reaches the incarnation that asked only
/// while it is the sleeve's live one; the core records any other as fenced.
fn decide_approval(hub: &Hub, id: &str, d: api::Decision) -> Result<api::Approval> {
    let view = hub.view();
    let a = view
        .state
        .approvals
        .get(id)
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no approval {id}")))?;
    if a.state != ApprovalState::Pending {
        return Err(err(
            ErrorCode::Conflict,
            format!("approval {id} is already {}", approvals::state(a)),
        ));
    }
    if a.source == Source::Action && d == api::Decision::AllowSession {
        return Err(err(
            ErrorCode::Invalid,
            "an action approval is allow_once or deny",
        ));
    }
    let allow = d != api::Decision::Deny;
    let sleeve = a.sleeve;
    command(
        hub,
        EventKind::RespondApproval {
            sleeve,
            approval: id.to_string(),
            allow,
            note: String::new(),
        },
    )?;
    let view = hub.view();
    let a = view
        .state
        .approvals
        .get(id)
        .ok_or_else(|| err(ErrorCode::Internal, format!("approval {id} vanished")))?;
    if a.source == Source::Action && a.state == ApprovalState::Decided {
        let g = view.state.sleeve(sleeve).map_or(0, |s| s.g.0);
        hub.record(
            sleeve,
            g,
            now_ms(),
            Entry::ApprovalDecision {
                id: id.to_string(),
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
    Ok(approvals::view(a))
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
    c.hub.bus.event(
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
    c.hub.bus.event(
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
        c.hub.fleet.removed(&c.hub.bus, id);
    }
    let removal = removal.map_err(|e| {
        let m = e.to_string();
        if m.contains("enrollment record") {
            c.hub.bus.event(
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
    c.hub.bus.event(
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
            c.hub.bus.event(
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
            c.hub.fleet.renamed(&c.hub.bus, v.node);
        }
        let text = format!("node {from} renamed {}", n.name);
        info!("{text} (headscale node {id})");
        c.hub.bus.event(
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
        .map(|j| crate::jobs::view(&hub.jobs, j))
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
        .map(|j| crate::jobs::view(&c.hub.jobs, j))
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
    envoy_id(&view.state).ok_or_else(|| err(ErrorCode::Unavailable, "Envoy is not running"))
}

fn envoy_send(c: &Ctx<'_>, r: api::EnvoySendRequest) -> Result<api::Sent> {
    if matches!(c.who, Principal::Sleeve(_) | Principal::Envoy(_)) {
        return Err(err(
            ErrorCode::Forbidden,
            "sleeves mail Envoy with sleeve send, not EnvoySend",
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
            agent: Some(agent.to_string()),
            text: prefix(&view.state, c.who, agent, &r.text),
            mode: SendMode::Queue,
            d9: false,
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
        "sleeves mail Envoy with sleeve send, not the Envoy methods",
    )
}

/// What vendor agents see of Envoy's transcript: each agent message, then the assistant text
/// and settle of the turn it started, until another sender's message. A continuation is the
/// same turn, so the answer after a crash stays visible. No tool calls, no reasoning, nothing
/// the owner or a sleeve said.
fn agent_transcript(
    hub: &Hub,
    id: SleeveId,
    from: u64,
    limit: Option<u64>,
) -> Result<api::Transcript> {
    let limit = limit.unwrap_or(TRANSCRIPT_DEFAULT).clamp(1, TRANSCRIPT_MAX) as usize;
    let store = hub.transcripts.get(id).map_err(internal)?;
    let lines = lock(&store).all().map_err(internal)?;
    let view = hub.view();
    let from_agent = |r: &str| {
        view.state
            .sleeve(id)
            .and_then(|s| s.messages.get(r))
            .is_some_and(|m| m.from == Party::Agent)
    };
    let mut agent = false;
    let mut shown = lines.iter().filter(|l| {
        let keep = match &l.entry {
            Entry::UserMessage {
                kind: PromptKind::Continuation,
                ..
            } => false,
            Entry::UserMessage { request_id, .. } => {
                agent = from_agent(request_id);
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
            // The hub's continuation of this turn. The reply after it is still this answer.
            Entry::UserMessage {
                kind: PromptKind::Continuation,
                ..
            } => {}
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
            MsgState::Delivered => answer(hub, id, &request_id, s.settled_idle()).map(Ok),
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

fn reached(state: &State, id: SleeveId, until: api::WaitUntil) -> bool {
    let Some(s) = state.sleeve(id) else {
        return false;
    };
    let idle = s.settled_idle();
    match until {
        api::WaitUntil::Idle => idle,
        api::WaitUntil::Done => idle && s.activity == (pt_core::Activity::Idle { seen: false }),
        api::WaitUntil::Blocked => matches!(s.activity, pt_core::Activity::Blocked { .. }),
        api::WaitUntil::Parked => matches!(s.life, Life::Parked(_)),
        api::WaitUntil::Ended => matches!(s.life, Life::Stopped | Life::Removed),
        api::WaitUntil::Report => false,
    }
}

/// The newest message the waiter has already sent to `waited`, if any.
/// A continuation is the same turn, not mail: it does not move the cursor.
fn report_watermark(state: &State, waited: SleeveId, waiter: Party) -> Option<u64> {
    state.sleeve(waited).and_then(|s| {
        s.messages
            .values()
            .filter(|m| m.from == waiter && m.kind == MsgKind::Prompt)
            .map(|m| m.seq)
            .max()
    })
}

/// The newest upward report from `waited` with a seq above the cursor.
/// `after` replaces the default, which is the waiter's newest message to `waited`
/// (or every past report when the waiter has sent none).
/// A parent counts only mail in its own mailbox. Anyone else also counts mail to Envoy.
/// A continuation is not a report.
fn upward_report(
    state: &State,
    waited: SleeveId,
    waiter: Party,
    after: Option<u64>,
) -> Option<u64> {
    let s = state.sleeve(waited)?;
    let floor = after.or_else(|| report_watermark(state, waited, waiter));
    let parent_only = matches!(waiter, Party::Sleeve(me) if s.parent == Some(me));
    let mut dests = Vec::new();
    if let Some(p) = s.parent {
        dests.push(p);
    }
    if !parent_only
        && let Some(e) = envoy_id(state)
        && !dests.contains(&e)
    {
        dests.push(e);
    }
    let mut best = None;
    for d in dests {
        let Some(dest) = state.sleeve(d) else {
            continue;
        };
        for m in dest.messages.values() {
            if m.kind != MsgKind::Prompt {
                continue;
            }
            if m.from == Party::Sleeve(waited) && floor.is_none_or(|w| m.seq > w) {
                best = Some(best.map_or(m.seq, |b: u64| b.max(m.seq)));
            }
        }
    }
    best
}

fn wait(c: &Ctx<'_>, r: api::WaitRequest) -> Result<api::WaitResult> {
    let deadline = Instant::now() + timeout(r.timeout_ms);
    if r.until == api::WaitUntil::Report {
        let view = c.hub.view();
        let resolved = find(&view.state, &r.sleeve_id).ok();
        if r.sleeve_id == crate::envoy::ENVOY
            || resolved.is_some_and(|id| envoy_id(&view.state) == Some(id))
        {
            return Err(err(
                ErrorCode::Invalid,
                "Envoy never sends as a sleeve; wait for a sleeve",
            ));
        }
        let id = target_sleeve(c, &view, &r.sleeve_id)?;
        let waiter = c.who.party();
        let after = r.after_seq;
        let got = wait_for(c.hub, deadline, |v| {
            if let Some(seq) = upward_report(&v.state, id, waiter, after) {
                return Some(Some(seq));
            }
            let s = v.state.sleeve(id)?;
            matches!(s.life, Life::Stopped | Life::Removed | Life::Parked(_)).then_some(None)
        });
        return Ok(api::WaitResult {
            reached: got.as_ref().is_some_and(Option::is_some),
            sleeve: sleeve_view(c.hub, id)?,
            seq: got.flatten(),
        });
    }
    let id = target_sleeve(c, &c.hub.view(), &r.sleeve_id)?;
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
        seq: None,
    })
}

// ---- history -------------------------------------------------------------------------------

const HISTORY_DEFAULT: u64 = 10;
const HISTORY_MAX: u64 = 100;

fn history_err(e: crate::history::Failure) -> ApiError {
    match e {
        crate::history::Failure::Off(_) => err(ErrorCode::Unavailable, e.to_string()),
        crate::history::Failure::Io(_) => internal(e),
    }
}

/// D26: a sleeve reads its own run only. Envoy, the owner and a device read every sleeve.
fn history_reaches(who: &Principal, id: SleeveId) -> bool {
    match who {
        Principal::Sleeve(x) => id == *x,
        other => other.full(),
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
            .filter(|id| history_reaches(c.who, *id))
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
            .filter(|s| history_reaches(c.who, s.id))
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
                after: r.after_doc,
            },
            format!("no history of session {session} in that turn range"),
        ),
        _ => return Err(err(ErrorCode::Invalid, "give doc_id or session, not both")),
    };
    let view = c.hub.view();
    let st = &view.state;
    // Scope and the doc cap run inside the index, before it copies text out.
    let scope = history_sleeves(c, st, &[])?;
    let (docs, more) = c
        .hub
        .history
        .read(&sel, scope.as_deref(), pt_history::READ_DOCS)
        .map_err(history_err)?;
    // Out of reach reads the same as absent.
    if docs.is_empty() {
        return Err(err(ErrorCode::NotFound, what));
    }
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
    Ok(snapshot::hub_info(c.hub))
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
            c.hub.bus.event(
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

/// `GetDoctor`'s answer: the report compacted, with the hub's own harness update runs.
fn doctor_report(hub: &Hub, r: Option<&pt_doctor::Report>) -> api::DoctorReport {
    let Some(r) = r else {
        return api::DoctorReport {
            checked_at_ms: api::EpochMs(0),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            harnesses: Vec::new(),
            gh_logged_in: false,
            gh_account: String::new(),
            sandbox_ok: false,
            problems: vec!["doctor has not finished its first run".into()],
        };
    };
    let runs = hub.harness_updates.last_runs();
    api::DoctorReport {
        checked_at_ms: api::EpochMs(r.checked_ms),
        os: r.os.clone(),
        arch: r.arch.clone(),
        harnesses: r
            .harnesses
            .iter()
            .map(|h| {
                let last = runs.get(&h.binary.name);
                api::DoctorHarness {
                    name: h.name.clone(),
                    present: h.binary.present,
                    path: h.binary.path.clone(),
                    version: h.binary.version.clone(),
                    install: h.binary.install.as_str().into(),
                    login: h.credentials.present,
                    eligible: h.eligible,
                    reasons: h.reasons.clone(),
                    update: h.update.clone(),
                    updated_at_ms: last.map(|l| api::EpochMs(l.at_ms)),
                    update_note: last.map_or_else(String::new, |l| l.note.clone()),
                }
            })
            .collect(),
        gh_logged_in: r.gh.logged_in,
        gh_account: r.gh.account.clone(),
        sandbox_ok: r.sandbox.bwrap.present
            && r.sandbox.bwrap.error.is_empty()
            && r.sandbox.systemd_run_user,
        problems: {
            let mut problems = r.problems.clone();
            let capped = hub.history.capped_sleeves();
            if !capped.is_empty() {
                problems.push(format!("history capped: {}", capped.join(", ")));
            }
            problems
        },
    }
}

fn get_doctor(c: &Ctx<'_>, r: api::GetDoctorRequest) -> Result<api::DoctorReport> {
    if matches!(c.who, Principal::Sleeve(_)) {
        return Err(err(
            ErrorCode::Forbidden,
            "the doctor report is for the owner and Envoy",
        ));
    }
    let report = if r.refresh {
        Some(crate::workers::run_doctor(c.hub))
    } else {
        c.hub.doctor.report()
    };
    Ok(doctor_report(c.hub, report.as_deref()))
}

fn update_harnesses(c: &Ctx<'_>, r: api::UpdateHarnessesRequest) -> Result<api::HarnessUpdates> {
    Ok(api::HarnessUpdates {
        programs: c
            .hub
            .harness_updates
            .request(&c.hub.doctor, || c.hub.view(), &r.programs),
    })
}

fn update_hub(c: &Ctx<'_>, r: api::UpdateHubRequest) -> Result<api::HubUpdate> {
    crate::update::update_hub(c.hub, r)
}

fn update_nodes(c: &Ctx<'_>, r: api::UpdateNodesRequest) -> Result<api::NodeUpdates> {
    crate::update::update_nodes(c.hub, r)
}
