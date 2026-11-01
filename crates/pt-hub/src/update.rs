//! Updates from GitHub releases (SPEC 6a).
//!
//! - The hub: `UpdateHub` stages the release into `<home>/releases/<tag>/` (every asset checked
//!   against SHA256SUMS), snapshots headscale, switches `releases/current` on probation and exits
//!   with [`RESTART_CODE`], so the service manager starts the new binary through
//!   `releases/current/protectorate`. The new hub keeps the release only once its own health
//!   check passes within `HUB_HEALTH_WINDOW`: the core replayed the journal and applied
//!   `HubRecovered`, and its socket answers `GetHubInfo`. A failed start or check switches back
//!   and exits again; a binary that cannot even start is switched back by `hub probation`, which
//!   the unit runs from `releases/previous` before each start.
//! - Nodes: `UpdateNodes` sends `Update` over the hub's own connection. The node fetches the
//!   release from GitHub itself (outbound, never to the hub), checks it, switches on probation
//!   and restarts; the hub then dials it until it answers `Hello` from the new release, which is
//!   the contact that confirms it.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, EventKind as Kind, Level};
use pt_frame::msg;
use pt_update::{Channel, HUB_HEALTH_WINDOW, NODE_CONTACT_WINDOW, Release, Source, Store, Verdict};

use crate::home::Home;
use crate::hub::Hub;
use crate::log::{info, warning};
use crate::transcript::lock;

/// The hub's exit status when it stops to be restarted on another release.
pub const RESTART_CODE: u8 = 75;
const NODE_CALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// How long past its probation the hub waits for an updated node to answer.
const NODE_BACK_SLACK: Duration = Duration::from_secs(120);

/// Why `serve` returns.
pub enum Stop {
    Signal(i32),
    /// Exit for the service manager to start `releases/current`.
    Restart(String),
}

fn err(code: ErrorCode, m: impl Into<String>) -> ApiError {
    ApiError::new(code, m)
}

fn update_err(e: pt_update::Error) -> ApiError {
    let code = match e.kind {
        pt_update::ErrorKind::Fetch => ErrorCode::Unavailable,
        pt_update::ErrorKind::Release | pt_update::ErrorKind::Verify => ErrorCode::Invalid,
        pt_update::ErrorKind::Store | pt_update::ErrorKind::Health => ErrorCode::Internal,
    };
    err(code, e.message)
}

fn source(hub: &Hub) -> Result<Source, ApiError> {
    Source::new(&hub.config.release_api, &hub.config.release_repo).map_err(update_err)
}

fn channel(hub: &Hub, c: Option<api::ReleaseChannel>) -> Result<Channel, ApiError> {
    Ok(match c {
        Some(api::ReleaseChannel::Stable) => Channel::Stable,
        Some(api::ReleaseChannel::Dev) => Channel::Dev,
        None => hub.config.release_channel.parse().map_err(|_| {
            err(
                ErrorCode::Invalid,
                format!(
                    "config release_channel {:?} is not stable or dev",
                    hub.config.release_channel
                ),
            )
        })?,
    })
}

fn release(
    hub: &Hub,
    src: &Source,
    version: Option<&str>,
    ch: Option<api::ReleaseChannel>,
) -> Result<Release, ApiError> {
    match version {
        Some(v) => src.release(v).map_err(update_err),
        None => src.latest(channel(hub, ch)?).map_err(update_err),
    }
}

pub fn request_restart(hub: &Hub, why: String) {
    let tx: Option<Sender<Stop>> = lock(&hub.stop).clone();
    match tx {
        Some(tx) => {
            let _ = tx.send(Stop::Restart(why));
        }
        None => warning!("no serve loop to restart: {why}"),
    }
}

/// Lets the reply reach the caller, then stops the hub for a restart.
fn restart_soon(hub: &Hub, why: String) {
    let tx: Option<Sender<Stop>> = lock(&hub.stop).clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        match tx {
            Some(tx) => drop(tx.send(Stop::Restart(why))),
            None => warning!("no serve loop to restart: {why}"),
        }
    });
}

pub fn update_hub(hub: &Hub, r: api::UpdateHubRequest) -> Result<api::HubUpdate, ApiError> {
    let _one = hub
        .update_lock
        .try_lock()
        .map_err(|_| err(ErrorCode::Conflict, "an update is already running"))?;
    let store = Store::open(&hub.home.root).map_err(update_err)?;
    let current = store.current().map_err(update_err)?;
    let not_here = |e: String| err(ErrorCode::Conflict, e);
    if r.rollback {
        store
            .started_from_current("protectorate")
            .map_err(not_here)?;
        let sw = store.rollback().map_err(update_err)?;
        let note = format!(
            "rolled back from {} to {}; restarting",
            sw.from.as_deref().unwrap_or("-"),
            sw.to
        );
        hub.event(
            Kind::Hub,
            Level::Warn,
            "update",
            "owner",
            &format!("hub {note}"),
        );
        restart_soon(hub, note.clone());
        return Ok(api::HubUpdate {
            current: sw.from,
            target: sw.to,
            available: true,
            restarting: true,
            note,
        });
    }
    let src = source(hub)?;
    let rel = release(hub, &src, r.version.as_deref(), r.channel)?;
    let target = rel.tag.clone();
    let available = current.as_deref() != Some(target.as_str());
    let mut answer = api::HubUpdate {
        current: current.clone(),
        target: target.clone(),
        available,
        restarting: false,
        note: String::new(),
    };
    if !available {
        answer.note = format!("already on {target}");
        return Ok(answer);
    }
    if r.check {
        answer.note = format!("{target} is available");
        return Ok(answer);
    }
    store
        .started_from_current("protectorate")
        .map_err(not_here)?;
    let arch = pt_update::arch()
        .ok_or_else(|| err(ErrorCode::Invalid, "no hub build for this architecture"))?;
    let staged = store
        .stage(&src, &rel, &pt_update::hub_picks(arch))
        .map_err(update_err)?;
    store
        .check_binary(&target, "protectorate", "version")
        .map_err(|e| err(ErrorCode::Invalid, e.message))?;
    if let crate::fleet::Status::Up(up) = hub.fleet.status() {
        let dir = up
            .fleet
            .snapshot_headscale(&format!("before-{target}"))
            .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
        info!("update: headscale snapshot in {}", dir.display());
    }
    let sw = store
        .switch_on_probation(&target, HUB_HEALTH_WINDOW)
        .map_err(update_err)?;
    let note = format!(
        "switched from {} to {target}{}; restarting on probation",
        sw.from.as_deref().unwrap_or("-"),
        if staged.reused {
            " (already staged)"
        } else {
            ""
        }
    );
    hub.event(
        Kind::Hub,
        Level::Info,
        "update",
        "owner",
        &format!("hub {note}"),
    );
    info!("update: {note}");
    restart_soon(hub, note.clone());
    answer.restarting = true;
    answer.note = note;
    Ok(answer)
}

/// At start: a release on trial whose window has passed goes back now. Returns the version on
/// trial when this process is it.
pub fn probation_at_start(home: &Home) -> Result<Option<String>, String> {
    let store = Store::open(&home.root).map_err(|e| e.to_string())?;
    let started = store.running_release();
    let on_trial = store
        .probation()
        .map_err(|e| e.to_string())?
        .is_some_and(|p| Some(&p.version) == started.as_ref());
    if !on_trial {
        return Ok(None);
    }
    match store.check_probation().map_err(|e| e.to_string())? {
        Verdict::RolledBack(sw) => Err(format!(
            "release {} was not confirmed in time; rolled back to {}",
            sw.from.as_deref().unwrap_or("-"),
            sw.to
        )),
        Verdict::Waiting { .. } => Ok(started),
        Verdict::Idle | Verdict::Confirmed => Ok(None),
    }
}

/// A start that failed while on trial switches back, so the restart runs the previous release.
pub fn failed_start(home: &Home, why: &str) -> String {
    let Ok(store) = Store::open(&home.root) else {
        return why.to_string();
    };
    match store.fail_probation(&format!("failed to start: {why}")) {
        Ok(Some(sw)) => format!(
            "{why}; release {} rolled back to {}",
            sw.from.as_deref().unwrap_or("-"),
            sw.to
        ),
        Ok(None) => why.to_string(),
        Err(e) => format!("{why}; rolling back failed: {e}"),
    }
}

fn healthy(hub: &Hub) -> Result<(), String> {
    if hub.view().batch == 0 {
        return Err("the core has not applied HubRecovered yet".into());
    }
    let client =
        pt_http::Client::unix(hub.home.socket()).read_timeout(Some(Duration::from_secs(10)));
    let path = format!("{}GetHubInfo", pt_api::API_PREFIX);
    let r = client
        .request(
            "POST",
            &path,
            &[("Content-Type", "application/json")],
            b"{}",
        )
        .map_err(|e| format!("GetHubInfo on {}: {e}", hub.home.socket().display()))?;
    let v = pt_json::parse_bytes(&r.body).map_err(|e| format!("GetHubInfo: {e}"))?;
    if r.status != 200 || v["started_at_ms"].as_u64() != Some(hub.started_ms) {
        return Err(format!("GetHubInfo answered {}: {v}", r.status));
    }
    Ok(())
}

/// The new hub's health check, on its own thread: keep the release once it passes, else switch
/// back and restart when the window ends.
pub fn watch_probation(hub: &Arc<Hub>, version: String) {
    let weak = Arc::downgrade(hub);
    thread::spawn(move || {
        let deadline = {
            let Some(hub) = weak.upgrade() else { return };
            Store::open(&hub.home.root)
                .ok()
                .and_then(|s| s.probation().ok().flatten())
                .map_or(0, |p| p.deadline_ms)
        };
        loop {
            let Some(hub) = weak.upgrade() else { return };
            let last = match healthy(&hub) {
                Ok(()) => {
                    let kept = Store::open(&hub.home.root).and_then(|s| s.confirm());
                    let text = match kept {
                        Ok(_) => format!("hub {version} passed its health check; the update stays"),
                        Err(e) => format!("hub {version} is healthy, but confirming failed: {e}"),
                    };
                    info!("update: {text}");
                    hub.event(Kind::Hub, Level::Info, "update", "owner", &text);
                    return;
                }
                Err(e) => e,
            };
            if crate::home::now_ms() >= deadline {
                let why = format!("health check failed: {last}");
                let text = match Store::open(&hub.home.root).and_then(|s| s.fail_probation(&why)) {
                    Ok(Some(sw)) => format!("hub {version} {why}; rolled back to {}", sw.to),
                    Ok(None) => format!("hub {version} {why}; nothing on trial"),
                    Err(e) => format!("hub {version} {why}; rolling back failed: {e}"),
                };
                warning!("update: {text}");
                hub.event(Kind::Hub, Level::Alarm, "update", "owner", &text);
                request_restart(&hub, text);
                return;
            }
            drop(hub);
            thread::sleep(Duration::from_secs(1));
        }
    });
}

/// The release asset names use amd64 and arm64.
fn asset_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => a,
    }
}

struct Target {
    id: String,
    name: String,
    ip: String,
    os: String,
    arch: String,
    release: String,
}

pub fn update_nodes(hub: &Hub, r: api::UpdateNodesRequest) -> Result<api::NodeUpdates, ApiError> {
    let up = hub
        .fleet
        .wait_up(Duration::from_secs(10))
        .map_err(|e| err(ErrorCode::Unavailable, e))?;
    if r.all == !r.nodes.is_empty() {
        return Err(err(ErrorCode::Invalid, "name the nodes, or all of them"));
    }
    let src = source(hub)?;
    let rel = release(hub, &src, r.version.as_deref(), r.channel)?;
    let tag = rel.tag.clone();
    let probation = r
        .probation_secs
        .unwrap_or(NODE_CONTACT_WINDOW.as_secs())
        .max(1);
    let views = up.poller.nodes();
    let target = |v: &pt_fleet::NodeView| -> Option<Target> {
        let h = v.hello.as_ref()?;
        Some(Target {
            id: v.node.id.clone(),
            name: v.node.name.clone(),
            ip: v.ip.clone()?,
            os: h.os.clone(),
            arch: asset_arch(&h.arch).to_string(),
            release: h.release.clone().unwrap_or_else(|| h.node_version.clone()),
        })
    };
    let mut results = Vec::new();
    let mut todo: Vec<Target> = Vec::new();
    if r.all {
        for v in views.iter().filter(|v| v.node.online) {
            match target(v) {
                Some(t) => todo.push(t),
                None => results.push(failed(&v.node.id, &v.node.name, "", &tag, "no hello yet")),
            }
        }
        // One canary per OS and arch first, then the rest in their groups.
        let mut first: Vec<(String, String)> = Vec::new();
        todo.sort_by_key(|t| {
            let key = (t.os.clone(), t.arch.clone());
            let canary = !first.contains(&key);
            if canary {
                first.push(key.clone());
            }
            (!canary, key)
        });
    } else {
        for n in &r.nodes {
            let v = views
                .iter()
                .find(|v| &v.node.id == n || &v.node.name == n)
                .ok_or_else(|| err(ErrorCode::NotFound, format!("no node {n:?}")))?;
            todo.push(target(v).ok_or_else(|| {
                err(
                    ErrorCode::Unavailable,
                    format!("node {n} has not answered Hello yet"),
                )
            })?);
        }
    }
    let mut failed_groups: Vec<(String, String)> = Vec::new();
    for t in todo {
        let group = (t.os.clone(), t.arch.clone());
        if failed_groups.contains(&group) {
            results.push(failed(
                &t.id,
                &t.name,
                &t.release,
                &tag,
                "skipped: the canary for this OS and arch failed",
            ));
            continue;
        }
        let res = update_one(hub, &up, &rel, &t, probation, r.no_wait);
        let line = match &res {
            Ok(d) => format!("node {}: {} -> {tag}: {d}", t.name, t.release),
            Err(e) => format!("node {}: update to {tag} failed: {e}", t.name),
        };
        let level = if res.is_ok() {
            Level::Info
        } else {
            Level::Warn
        };
        hub.event(Kind::Node, level, &t.name, "owner", &line);
        info!("update: {line}");
        match res {
            Ok(detail) => results.push(api::NodeUpdate {
                node_id: t.id,
                name: t.name,
                from: t.release,
                to: tag.clone(),
                ok: true,
                detail,
            }),
            Err(e) => {
                failed_groups.push(group);
                results.push(failed(&t.id, &t.name, &t.release, &tag, &e));
            }
        }
    }
    Ok(api::NodeUpdates {
        version: tag,
        nodes: results,
    })
}

fn failed(id: &str, name: &str, from: &str, to: &str, why: &str) -> api::NodeUpdate {
    api::NodeUpdate {
        node_id: id.into(),
        name: name.into(),
        from: from.into(),
        to: to.into(),
        ok: false,
        detail: why.into(),
    }
}

fn update_one(
    hub: &Hub,
    up: &crate::fleet::Up,
    rel: &Release,
    t: &Target,
    probation: u64,
    no_wait: bool,
) -> Result<String, String> {
    let ext = if t.os == "windows" { ".exe" } else { "" };
    let asset = format!("protectorate-node-{}-{}{ext}", t.os, t.arch);
    rel.asset(&asset).map_err(|e| e.message)?;
    if t.release == rel.tag {
        return Ok("already on it".into());
    }
    let mut c = up.fleet.connect(&t.ip).map_err(|e| e.to_string())?;
    c.set_timeout(NODE_CALL_TIMEOUT)
        .map_err(|e| e.to_string())?;
    let reply = c
        .update(msg::Update {
            version: rel.tag.clone(),
            api: hub.config.release_api.clone(),
            repo: hub.config.release_repo.clone(),
            probation_secs: probation,
        })
        .map_err(|e| match e {
            pt_fleet::Error::Node(r) => format!("{}: {}", r.code.as_str(), r.message),
            e => e.to_string(),
        })?;
    drop(c);
    let switched = format!(
        "switched from {}{}",
        reply.from.as_deref().unwrap_or("-"),
        if reply.reused {
            " (already staged)"
        } else {
            ""
        }
    );
    if no_wait {
        return Ok(format!("{switched}; restarting on {probation} s probation"));
    }
    let deadline = Instant::now() + Duration::from_secs(probation) + NODE_BACK_SLACK;
    let mut last = String::from("no answer yet");
    // The old process answers until it exits; once it is gone, an answer from the old
    // release means the new one rolled back.
    let mut gone = false;
    while Instant::now() < deadline {
        thread::sleep(Duration::from_secs(2));
        let hello = up
            .fleet
            .connect(&t.ip)
            .and_then(|mut c| c.hello())
            .map_err(|e| e.to_string());
        match hello {
            Ok(h) if h.release.as_deref() == Some(rel.tag.as_str()) => {
                return Ok(format!("{switched}; back on {} and confirmed", rel.tag));
            }
            Ok(h) if gone && reply.from.is_some() && h.release == reply.from => {
                return Err(format!(
                    "{switched}, but it answers from {} again: it rolled back",
                    reply.from.as_deref().unwrap_or("-")
                ));
            }
            Ok(h) => last = format!("the node answers from {:?}", h.release),
            Err(e) => {
                gone = true;
                last = e;
            }
        }
    }
    Err(format!(
        "{switched}, but it did not come back on {}: {last}",
        rel.tag
    ))
}
