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
//!   and restarts; the hub then dials it until it answers a compatible `Hello` and a `Presence`
//!   request from the new release.

use std::cmp::Ordering;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, EventKind as Kind, Level};
use pt_frame::msg;
use pt_update::{
    Channel, HUB_HEALTH_WINDOW, NODE_CONTACT_WINDOW, Probation, Release, Source, Store, Version,
};

use crate::home::Home;
use crate::hub::Hub;
use crate::log::{info, warning};
use crate::transcript::lock;

/// The hub's exit status when it stops to be restarted on another release.
pub const RESTART_CODE: u8 = 75;
const NODE_CALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// How long past its probation the hub waits for an updated node to answer.
const NODE_BACK_SLACK: Duration = Duration::from_secs(120);
/// The longest node probation an `UpdateNodes` may ask for: a day.
const MAX_NODE_PROBATION: u64 = 86_400;

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
        let previous = store
            .previous()
            .map_err(update_err)?
            .ok_or_else(|| err(ErrorCode::Conflict, "no previous release"))?;
        let version = store
            .check_binary(&previous, "protectorate", "version")
            .map_err(|e| err(ErrorCode::Conflict, e.message))?;
        if !version.lines().any(|line| line == "hub-rollback: 1") {
            return Err(err(
                ErrorCode::Conflict,
                format!(
                    "previous release {previous} cannot run snapshot-aware probation; rollback refused"
                ),
            ));
        }
        let fleet_off = matches!(hub.fleet.status(), crate::fleet::Status::Off);
        let sw = store
            .request_manual_rollback(fleet_off)
            .map_err(|e| err(ErrorCode::Conflict, e.message))?;
        let note = format!(
            "rollback from {} to {} requested; restarting for offline restore. Headscale changes after its snapshot are lost; journal events are kept only if the target can read them",
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
    let running = current.as_deref().unwrap_or(pt_version::VERSION);
    let ordering = Version::parse(&target)
        .zip(Version::parse(running))
        .map(|(to, from)| to.cmp(&from));
    if r.version.is_none() && ordering.is_none() {
        return Err(err(
            ErrorCode::Invalid,
            format!("cannot compare {target} with running release {running}; specify --version"),
        ));
    }
    let downgrade = ordering == Some(Ordering::Less);
    let available = if r.version.is_some() {
        running != target
    } else {
        ordering == Some(Ordering::Greater)
    };
    let mut answer = api::HubUpdate {
        current: current.clone(),
        target: target.clone(),
        available,
        restarting: false,
        note: String::new(),
    };
    if !available {
        answer.note = format!("no newer release than {running} on this channel (latest {target})");
        return Ok(answer);
    }
    if r.check {
        answer.note = format!(
            "{target} is available{}",
            if downgrade { " (downgrade)" } else { "" }
        );
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
    let version_output = store
        .check_binary(&target, "protectorate", "version")
        .map_err(|e| err(ErrorCode::Invalid, e.message))?;
    if !version_output.lines().any(|line| line == "hub-rollback: 1") {
        return Err(err(
            ErrorCode::Invalid,
            format!(
                "{target} does not support snapshot-aware hub rollback; install a release that does"
            ),
        ));
    }
    let snapshot = match hub.fleet.status() {
        crate::fleet::Status::Up(up) => {
            let dir = up
                .fleet
                .snapshot_headscale(&format!("before-{target}"))
                .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
            if ["db.sqlite-wal", "db.sqlite-shm"]
                .iter()
                .any(|name| dir.join(name).exists())
            {
                return Err(err(
                    ErrorCode::Internal,
                    format!(
                        "headscale snapshot {} has SQLite sidecars; cannot restore it atomically",
                        dir.display()
                    ),
                ));
            }
            info!("update: headscale snapshot in {}", dir.display());
            Some(
                dir.to_str()
                    .ok_or_else(|| {
                        err(
                            ErrorCode::Internal,
                            format!("headscale snapshot path is not UTF-8: {}", dir.display()),
                        )
                    })?
                    .to_owned(),
            )
        }
        crate::fleet::Status::Off => None,
        crate::fleet::Status::Down(why) => {
            return Err(err(
                ErrorCode::Unavailable,
                format!("cannot snapshot headscale while the fleet is down: {why}"),
            ));
        }
    };
    let journal_dir = match &snapshot {
        Some(path) => Path::new(path).join("journal"),
        None => hub
            .home
            .root
            .join("update-snapshots")
            .join(format!(
                "{}-{}-before-{target}",
                crate::home::now_ms(),
                std::process::id()
            ))
            .join("journal"),
    };
    let journal_snapshot_ms = hub
        .snapshot_journal(journal_dir.clone())
        .map_err(|e| err(ErrorCode::Internal, format!("snapshotting journal: {e}")))?;
    let journal_snapshot = journal_dir
        .to_str()
        .ok_or_else(|| err(ErrorCode::Internal, "journal snapshot path is not UTF-8"))?
        .to_string();
    let sw = store
        .switch_on_probation_with_snapshots(
            &target,
            HUB_HEALTH_WINDOW,
            snapshot,
            Some(journal_snapshot),
            Some(journal_snapshot_ms),
        )
        .map_err(update_err)?;
    let note = format!(
        "switched from {} to {target}{}{}; restarting on probation",
        sw.from.as_deref().unwrap_or("-"),
        if downgrade { " (downgrade)" } else { "" },
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
    let trial = store.probation().map_err(|e| e.to_string())?.filter(|p| {
        Some(&p.version) == started.as_ref()
            && p.contacted_ms.is_none()
            && p.rolled_back_ms.is_none()
    });
    let Some(p) = trial else { return Ok(None) };
    if p.reason.is_some() || crate::home::now_ms() >= p.deadline_ms {
        return Err(format!(
            "release {} needs rollback by hub probation before startup",
            p.version
        ));
    }
    Ok(started)
}

/// A start that failed while on trial requests an offline rollback at the next prestart.
pub fn failed_start(home: &Home, why: &str) -> String {
    let Ok(store) = Store::open(&home.root) else {
        return why.to_string();
    };
    match store.request_rollback(&format!("failed to start: {why}")) {
        Ok(true) => format!("{why}; rollback requested for the next prestart"),
        Ok(false) => why.to_string(),
        Err(e) => format!("{why}; requesting rollback failed: {e}"),
    }
}

const SEGMENT: &str = "segment-00000000000000000000";

fn event_tags(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if bytes.get(..12) != Some(b"PTJOURNL\x01\x00\x00\x00") {
        return Err(format!("invalid journal header in {}", path.display()));
    }
    let decoded = pt_journal::codec::decode_frames(&bytes[12..]);
    if decoded.stop.is_some() {
        return Err(format!(
            "incomplete or corrupt journal in {}",
            path.display()
        ));
    }
    decoded
        .frames
        .into_iter()
        .map(|frame| {
            pt_core::codec::decode(frame)
                .map_err(|e| format!("invalid journal event in {}: {e}", path.display()))?;
            frame
                .get(9)
                .copied()
                .ok_or_else(|| format!("short event in {}", path.display()))
        })
        .collect()
}

fn target_reads_journal(home: &Path, store: &Store, p: &Probation) -> bool {
    let Ok(version) = store.check_binary(&p.previous, "protectorate", "version") else {
        return false;
    };
    let mut caps = version
        .lines()
        .filter_map(|line| line.strip_prefix("journal-event-tag: "));
    let Some(cap) = caps.next().and_then(|v| v.parse::<u8>().ok()) else {
        return false;
    };
    if caps.next().is_some() {
        return false;
    }
    let dir = home.join("journal");
    let Ok(entries) = fs::read_dir(&dir) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("snapshot-") || (name.starts_with("segment-") && name != SEGMENT) {
            return false;
        }
    }
    event_tags(&dir.join(SEGMENT)).is_ok_and(|tags| tags.into_iter().all(|tag| tag <= cap))
}

fn stage_journal(home: &Path, snapshot: &str) -> Result<PathBuf, String> {
    let source = Path::new(snapshot).join(SEGMENT);
    event_tags(&source)?;
    let dir = home.join("journal");
    let tmp = dir.join(format!(
        ".restore-{}-{}",
        std::process::id(),
        crate::home::now_ms()
    ));
    let mut src = File::open(&source).map_err(|e| format!("opening {}: {e}", source.display()))?;
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("creating {}: {e}", tmp.display()))?;
    std::io::copy(&mut src, &mut dst)
        .and_then(|_| dst.sync_all())
        .map_err(|e| format!("copying {} to {}: {e}", source.display(), tmp.display()))?;
    Ok(tmp)
}

pub fn restore_snapshots(home: &Path, store: &Store, p: &Probation) -> Result<(), String> {
    let restore_journal = p
        .journal_snapshot
        .as_deref()
        .filter(|_| !p.manual_rollback || !target_reads_journal(home, store, p));
    let staged = restore_journal
        .map(|snapshot| stage_journal(home, snapshot))
        .transpose()?;
    restore_headscale(p)?;
    if let Some(tmp) = staged {
        let dir = home.join("journal");
        let target = dir.join(SEGMENT);
        fs::rename(&tmp, &target)
            .map_err(|e| format!("restoring {} to {}: {e}", tmp.display(), target.display()))?;
        File::open(&dir)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("syncing {}: {e}", dir.display()))?;
        eprintln!(
            "update: restored journal snapshot from {}; events after the snapshot are lost (snapshot time: {} ms since epoch)",
            restore_journal.unwrap_or_default(),
            p.journal_snapshot_ms
                .map_or_else(|| "unknown".into(), |ms| ms.to_string())
        );
    } else if p.journal_snapshot.is_some() {
        eprintln!("update: kept the journal; the target release reads every event");
    }
    Ok(())
}

fn restore_headscale(p: &Probation) -> Result<(), String> {
    let Some(snapshot) = p.headscale_snapshot.as_deref() else {
        return Ok(());
    };
    let snapshot = Path::new(snapshot);
    let fleet = snapshot
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("invalid headscale snapshot {}", snapshot.display()))?;
    let dest = fleet.join("headscale");
    let db = snapshot.join("db.sqlite");
    if !db.is_file() {
        return Err(format!(
            "headscale snapshot {} has no database",
            snapshot.display()
        ));
    }
    if ["db.sqlite-wal", "db.sqlite-shm"]
        .iter()
        .any(|name| snapshot.join(name).exists())
    {
        return Err(format!(
            "headscale snapshot {} has SQLite sidecars; cannot restore it atomically",
            snapshot.display()
        ));
    }
    let tmp = dest.join(format!(
        ".restore-db.sqlite-{}-{}",
        std::process::id(),
        crate::home::now_ms()
    ));
    let mut src = File::open(&db).map_err(|e| format!("opening {}: {e}", db.display()))?;
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("creating {}: {e}", tmp.display()))?;
    std::io::copy(&mut src, &mut dst)
        .and_then(|_| dst.sync_all())
        .map_err(|e| format!("copying {} to {}: {e}", db.display(), tmp.display()))?;
    let mut aside = Vec::new();
    let put_back = |aside: &[(PathBuf, PathBuf)]| {
        for (backup, original) in aside.iter().rev() {
            if let Err(e) = fs::rename(backup, original) {
                eprintln!(
                    "update: could not put {} back from {}: {e}",
                    original.display(),
                    backup.display()
                );
            }
        }
    };
    for name in ["db.sqlite-wal", "db.sqlite-shm"] {
        let path = dest.join(name);
        let backup = dest.join(format!(
            ".rollback-{name}-{}-{}",
            std::process::id(),
            crate::home::now_ms()
        ));
        match fs::rename(&path, &backup) {
            Ok(()) => aside.push((backup, path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                put_back(&aside);
                return Err(format!("saving {} before restore: {e}", path.display()));
            }
        }
    }
    if let Err(e) = File::open(&dest).and_then(|f| f.sync_all()) {
        put_back(&aside);
        return Err(format!("syncing {} before restore: {e}", dest.display()));
    }
    let target = dest.join("db.sqlite");
    if let Err(e) = fs::rename(&tmp, &target) {
        put_back(&aside);
        return Err(format!(
            "restoring {} to {}: {e}",
            tmp.display(),
            target.display()
        ));
    }
    File::open(&dest)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("syncing {}: {e}", dest.display()))?;
    eprintln!(
        "update: restored headscale database from {}; nodes enrolled after this snapshot are lost",
        snapshot.display()
    );
    Ok(())
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
                    let kept = Store::open(&hub.home.root).and_then(|s| s.confirm(&version));
                    match kept {
                        Ok(true) => {
                            let text =
                                format!("hub {version} passed its health check; the update stays");
                            info!("update: {text}");
                            hub.event(Kind::Hub, Level::Info, "update", "owner", &text);
                            return;
                        }
                        Ok(false) => "confirmation refused".to_string(),
                        Err(e) => format!("confirming a healthy hub failed: {e}"),
                    }
                }
                Err(e) => e,
            };
            if crate::home::now_ms() >= deadline {
                let why = format!("health check failed: {last}");
                let text =
                    format!("hub {version} {why}; restarting for offline probation rollback");
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
    let views = up.poller.nodes();
    for name in &r.nodes {
        let v = views
            .iter()
            .find(|v| &v.node.id == name || &v.node.name == name)
            .ok_or_else(|| err(ErrorCode::NotFound, format!("no node {name:?}")))?;
        if let Some(e) = v.protocol_error() {
            return Err(err(ErrorCode::Unavailable, e.to_string()));
        }
    }
    let src = source(hub)?;
    let rel = release(hub, &src, r.version.as_deref(), r.channel)?;
    let tag = rel.tag.clone();
    let probation = r.probation_secs.unwrap_or(NODE_CONTACT_WINDOW.as_secs());
    if !(1..=MAX_NODE_PROBATION).contains(&probation) {
        return Err(err(
            ErrorCode::Invalid,
            format!("probation must be 1 to {MAX_NODE_PROBATION} s"),
        ));
    }
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
        for v in &views {
            if let Some(e) = v.protocol_error() {
                results.push(failed(
                    &v.node.id,
                    &v.node.name,
                    &v.node.node_version,
                    &tag,
                    &e.to_string(),
                ));
            }
        }
        for v in views.iter().filter(|v| v.node.online) {
            match target(v) {
                Some(t) => todo.push(t),
                None => results.push(failed(&v.node.id, &v.node.name, "", &tag, "no hello yet")),
            }
        }
        let mut first = std::collections::BTreeMap::<(String, String), String>::new();
        for t in &todo {
            first
                .entry((t.os.clone(), t.arch.clone()))
                .and_modify(|id| {
                    if t.id < *id {
                        *id = t.id.clone();
                    }
                })
                .or_insert_with(|| t.id.clone());
        }
        let mut ordered: Vec<_> = todo
            .into_iter()
            .map(|t| {
                let canary = first.get(&(t.os.clone(), t.arch.clone())) == Some(&t.id);
                (!canary, t)
            })
            .collect();
        ordered.sort_by(|a, b| {
            (a.0, &a.1.os, &a.1.arch, &a.1.id).cmp(&(b.0, &b.1.os, &b.1.arch, &b.1.id))
        });
        todo = ordered.into_iter().map(|(_, t)| t).collect();
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
        let cmp = Version::parse(&tag)
            .zip(Version::parse(&t.release))
            .map(|(to, from)| to.cmp(&from));
        let res = if r.version.is_none() && cmp.is_none() {
            Err(format!(
                "cannot compare {tag} with {}; specify --version",
                t.release
            ))
        } else if r.version.is_none() && cmp != Some(Ordering::Greater) {
            Ok(format!(
                "no newer release than {} on this channel (latest {tag})",
                t.release
            ))
        } else {
            update_one(hub, &up, &rel, &t, probation, r.no_wait).map(|s| {
                if cmp == Some(Ordering::Less) {
                    format!("{s} (downgrade)")
                } else {
                    s
                }
            })
        };
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
    let mut c = up.fleet.connect(&t.ip).map_err(|e| e.to_string())?;
    c.set_timeout(NODE_CALL_TIMEOUT)
        .map_err(|e| e.to_string())?;
    c.require_compatible().map_err(|e| e.to_string())?;
    if t.release == rel.tag {
        return Ok("already on it".into());
    }
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
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(probation) + NODE_BACK_SLACK)
        .ok_or("the probation window is too long")?;
    let mut last = String::from("no answer yet");
    // The old process answers until it exits; once it is gone, an answer from the old
    // release means the new one rolled back.
    let mut gone = false;
    while Instant::now() < deadline {
        thread::sleep(Duration::from_secs(2));
        let hello = up
            .fleet
            .connect(&t.ip)
            .and_then(|mut c| {
                let h = c.hello()?;
                if h.release.as_deref() == Some(rel.tag.as_str()) {
                    c.require_compatible()?;
                    c.presence()?;
                }
                Ok(h)
            })
            .map_err(|e| e.to_string());
        match hello {
            Ok(h) if h.release.as_deref() == Some(rel.tag.as_str()) => {
                return Ok(format!(
                    "{switched}; back on {} and answered Presence",
                    rel.tag
                ));
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
