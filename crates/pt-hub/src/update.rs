//! Updates from GitHub releases (SPEC 6a).
//!
//! - The hub: `UpdateHub` stages the release into `<home>/releases/<tag>/` (every asset checked
//!   against SHA256SUMS), snapshots the journal and then headscale, switches `releases/current`
//!   on probation and exits with [`RESTART_CODE`], so the service manager starts the new binary
//!   through
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
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, ErrorCode, EventKind as Kind, Level};
use pt_frame::msg;
use pt_update::{
    Channel, HUB_HEALTH_WINDOW, NODE_CONTACT_WINDOW, Probation, Release, RestoreError, Source,
    Store, Version,
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
        pt_update::ErrorKind::Store
        | pt_update::ErrorKind::Health
        | pt_update::ErrorKind::Restore
        | pt_update::ErrorKind::RestorePartial => ErrorCode::Internal,
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
    // The journal snapshot is taken first. A node that enrolls between the two snapshots is in
    // the restored headscale database and absent from the restored journal.
    let scratch = hub.home.root.join("update-snapshots").join(format!(
        "{}-{}-before-{target}",
        crate::home::now_ms(),
        std::process::id()
    ));
    let journal_in_scratch = scratch.join("journal");
    let journal_copy = match hub.snapshot_journal(journal_in_scratch.clone()) {
        Ok(copy) => copy,
        Err(e) => {
            discard_dir(&scratch);
            return Err(err(
                ErrorCode::Internal,
                format!("snapshotting journal: {e}"),
            ));
        }
    };
    let headscale_dir = match hub.fleet.status() {
        crate::fleet::Status::Up(up) => {
            let dir = match up.fleet.snapshot_headscale(&format!("before-{target}")) {
                Ok(dir) => dir,
                Err(e) => {
                    discard_dir(&scratch);
                    return Err(err(ErrorCode::Internal, e.to_string()));
                }
            };
            if ["db.sqlite-wal", "db.sqlite-shm"]
                .iter()
                .any(|name| dir.join(name).exists())
            {
                discard_dir(&dir);
                discard_dir(&scratch);
                return Err(err(
                    ErrorCode::Internal,
                    format!(
                        "headscale snapshot {} has SQLite sidecars; cannot restore it atomically",
                        dir.display()
                    ),
                ));
            }
            let journal_final = dir.join("journal");
            if let Err(e) = fs::rename(&journal_in_scratch, &journal_final) {
                discard_dir(&dir);
                discard_dir(&scratch);
                return Err(err(
                    ErrorCode::Internal,
                    format!(
                        "moving the journal snapshot into {}: {e}",
                        journal_final.display()
                    ),
                ));
            }
            if let Err(e) = File::open(&dir).and_then(|f| f.sync_all()) {
                discard_dir(&dir);
                discard_dir(&scratch);
                return Err(err(
                    ErrorCode::Internal,
                    format!("syncing {}: {e}", dir.display()),
                ));
            }
            discard_dir(&scratch);
            info!("update: headscale snapshot in {}", dir.display());
            Some(dir)
        }
        crate::fleet::Status::Off => None,
        crate::fleet::Status::Down(why) => {
            discard_dir(&scratch);
            return Err(err(
                ErrorCode::Unavailable,
                format!("cannot snapshot headscale while the fleet is down: {why}"),
            ));
        }
    };
    let journal_dir = match &headscale_dir {
        Some(dir) => dir.join("journal"),
        None => journal_in_scratch,
    };
    let journal_snapshot = match journal_dir.to_str() {
        Some(path) => path.to_string(),
        None => {
            if let Some(dir) = &headscale_dir {
                discard_dir(dir);
            } else {
                discard_dir(&scratch);
            }
            return Err(err(
                ErrorCode::Internal,
                "journal snapshot path is not UTF-8",
            ));
        }
    };
    let headscale_snapshot = match &headscale_dir {
        Some(dir) => match dir.to_str() {
            Some(path) => Some(path.to_string()),
            None => {
                discard_dir(dir);
                return Err(err(
                    ErrorCode::Internal,
                    format!("headscale snapshot path is not UTF-8: {}", dir.display()),
                ));
            }
        },
        None => None,
    };
    let sw = match store.switch_on_probation_with_snapshots(
        &target,
        HUB_HEALTH_WINDOW,
        headscale_snapshot,
        Some(journal_snapshot),
        Some(journal_copy.at_ms),
        Some(journal_copy.next_sleeve),
    ) {
        Ok(sw) => sw,
        Err(e) => {
            // The record is written before the pointer switch. Deleting a directory it
            // names would leave the trial with nothing to restore.
            let snapshot = headscale_dir.as_deref().unwrap_or(scratch.as_path());
            if !record_names_snapshot(&store, snapshot) {
                discard_dir(snapshot);
            }
            return Err(update_err(e));
        }
    };
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

fn discard_dir(path: &Path) {
    if path.components().count() < 2 {
        return;
    }
    let _ = fs::remove_dir_all(path);
}

/// Whether `probation.json` names `dir` or a file inside it. An unreadable record might
/// name it, so the directory stays.
fn record_names_snapshot(store: &pt_update::Store, dir: &Path) -> bool {
    match store.probation() {
        Ok(Some(p)) => [
            p.headscale_snapshot.as_deref(),
            p.journal_snapshot.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|named| {
            let named = Path::new(named);
            named == dir || named.starts_with(dir)
        }),
        Err(_) => true,
        Ok(None) => false,
    }
}

fn sync_dir(dir: &Path) -> Result<(), String> {
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("syncing {}: {e}", dir.display()))
}

fn copy_synced(from: &Path, to: &Path) -> Result<(), String> {
    let mut src = File::open(from).map_err(|e| format!("opening {}: {e}", from.display()))?;
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| format!("creating {}: {e}", to.display()))?;
    std::io::copy(&mut src, &mut dst)
        .and_then(|_| dst.sync_all())
        .map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

fn prefixed(dir: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("reading {}: {e}", dir.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// Finishes a journal swap that died between the two renames. A staged copy left beside a
/// live journal is removed; an aside directory is the trial journal and stays.
pub fn finish_interrupted_journal_swap(home: &Path) -> Result<(), String> {
    let live = home.join("journal");
    if live.exists() {
        for stage in prefixed(home, ".journal-stage-")? {
            discard_dir(&stage);
        }
        return Ok(());
    }
    let stages = prefixed(home, ".journal-stage-")?;
    let asides = prefixed(home, ".journal-aside-")?;
    if stages.is_empty() && asides.is_empty() {
        return Ok(());
    }
    if stages.len() == 1 {
        fs::rename(&stages[0], &live).map_err(|e| {
            format!(
                "finishing the journal restore from {} to {}: {e}",
                stages[0].display(),
                live.display()
            )
        })?;
        sync_dir(home)?;
        return Ok(());
    }
    Err(format!(
        "journal directory is missing after an interrupted restore; staged copies: {}; preserved journals: {}",
        stages.len(),
        asides
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn journal_seq(name: &str, prefix: &str) -> Option<u64> {
    let digits = name.strip_prefix(prefix)?;
    if digits.len() != 20 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn journal_data_name(name: &str) -> bool {
    journal_seq(name, "segment-").is_some() || journal_seq(name, "snapshot-").is_some()
}

/// Whether this process, which is the previous release, can open `dir`. A torn tail is ignored
/// the way the journal ignores it on open. There is no separate event-tag declaration: the
/// codec that decodes the frames is the one that will serve them.
fn journal_readable(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut snapshots = Vec::new();
    let mut segments = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(seq) = journal_seq(name, "snapshot-") {
            snapshots.push(seq);
        } else if let Some(seq) = journal_seq(name, "segment-") {
            segments.push(seq);
        }
    }
    let newest = snapshots.iter().copied().max().unwrap_or(0);
    if segments.iter().any(|seq| *seq > newest) {
        return false;
    }
    if newest > 0 && !snapshot_header_ok(&dir.join(format!("snapshot-{newest:020}"))) {
        return false;
    }
    let segment = dir.join(format!("segment-{newest:020}"));
    if !segment.is_file() {
        return newest == 0 && snapshots.is_empty();
    }
    segment_events_ok(&segment)
}

fn snapshot_header_ok(path: &Path) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    if data.len() < 24 || data.get(..12) != Some(b"PTSNAPSH\x01\x00\x00\x00") {
        return false;
    }
    let len = u64::from_le_bytes(data[12..20].try_into().unwrap_or([0; 8]));
    let crc = u32::from_le_bytes(data[20..24].try_into().unwrap_or([0; 4]));
    let payload = &data[24..];
    len == payload.len() as u64 && pt_journal::crc32c::crc32c(payload) == crc
}

fn segment_events_ok(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    if bytes.get(..12) != Some(b"PTJOURNL\x01\x00\x00\x00") {
        return false;
    }
    let body = &bytes[12..];
    let decoded = pt_journal::codec::decode_frames(body);
    let intact_after = decoded.stop == Some(pt_journal::Stop::CrcMismatch)
        && pt_journal::codec::complete_len(&body[decoded.valid_len..]).is_some_and(|end| {
            !pt_journal::codec::decode_frames(&body[decoded.valid_len + end..])
                .frames
                .is_empty()
        });
    if intact_after {
        return false;
    }
    decoded
        .frames
        .iter()
        .all(|frame| pt_core::codec::decode(frame).is_ok())
}

fn validate_journal_snapshot(snapshot: &Path) -> Result<(), RestoreError> {
    let entries = match fs::read_dir(snapshot) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(RestoreError::failed(format!(
                "reading journal snapshot {}: {e}",
                snapshot.display()
            )));
        }
    };
    let mut segments = 0;
    for entry in entries {
        let entry = entry.map_err(|e| {
            RestoreError::failed(format!(
                "reading journal snapshot {}: {e}",
                snapshot.display()
            ))
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if journal_seq(name, "segment-").is_some() {
            let path = entry.path();
            if !segment_events_ok(&path) {
                return Err(RestoreError::failed(format!(
                    "journal snapshot {} is not readable",
                    path.display()
                )));
            }
            segments += 1;
        } else if journal_seq(name, "snapshot-").is_some() && !snapshot_header_ok(&entry.path()) {
            return Err(RestoreError::failed(format!(
                "journal snapshot {} is not readable",
                entry.path().display()
            )));
        }
    }
    if segments == 0 {
        return Err(RestoreError::failed(format!(
            "journal snapshot {} has no segment",
            snapshot.display()
        )));
    }
    Ok(())
}

fn stage_journal(home: &Path, snapshot: &Path) -> Result<PathBuf, RestoreError> {
    let staged = home.join(format!(
        ".journal-stage-{}-{}",
        std::process::id(),
        crate::home::now_ms()
    ));
    fs::create_dir(&staged)
        .map_err(|e| RestoreError::failed(format!("creating {}: {e}", staged.display())))?;
    let copy = (|| {
        let mut n = 0;
        for entry in
            fs::read_dir(snapshot).map_err(|e| format!("reading {}: {e}", snapshot.display()))?
        {
            let entry = entry.map_err(|e| format!("reading {}: {e}", snapshot.display()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !journal_data_name(name) {
                continue;
            }
            copy_synced(&entry.path(), &staged.join(name))?;
            n += 1;
        }
        if n == 0 {
            return Err(format!(
                "journal snapshot {} has no segment",
                snapshot.display()
            ));
        }
        sync_dir(&staged)
    })();
    if let Err(e) = copy {
        discard_dir(&staged);
        return Err(RestoreError::failed(e));
    }
    Ok(staged)
}

fn swap_journal(home: &Path, staged: PathBuf) -> Result<PathBuf, RestoreError> {
    let live = home.join("journal");
    let aside = home.join(format!(
        ".journal-aside-{}-{}",
        std::process::id(),
        crate::home::now_ms()
    ));
    if let Err(e) = fs::rename(&live, &aside) {
        discard_dir(&staged);
        return Err(RestoreError::failed(format!(
            "moving {} aside to {}: {e}",
            live.display(),
            aside.display()
        )));
    }
    if let Err(e) = fs::rename(&staged, &live) {
        return match fs::rename(&aside, &live) {
            Ok(()) => {
                discard_dir(&staged);
                Err(RestoreError::failed(format!(
                    "restoring the journal from {}: {e}",
                    staged.display()
                )))
            }
            Err(back) => Err(RestoreError::partial(format!(
                "restoring the journal from {} failed: {e}; putting {} back failed: {back}; the trial journal is at {}",
                staged.display(),
                live.display(),
                aside.display()
            ))),
        };
    }
    sync_dir(home).map_err(RestoreError::partial)?;
    Ok(aside)
}

/// Puts moved sleeves back. The destination is removed only when that succeeds: a sleeve
/// that could not be put back stays in the aside directory.
fn quarantine_undo(dest: &Path, sleeves: &[(PathBuf, PathBuf)], why: String) -> RestoreError {
    let mut stuck = Vec::new();
    for (from, to) in sleeves.iter().rev() {
        if let Err(e) = fs::rename(from, to) {
            stuck.push(format!("{} -> {}: {e}", from.display(), to.display()));
        }
    }
    if stuck.is_empty() {
        discard_dir(dest);
        RestoreError::failed(why)
    } else {
        RestoreError::partial(format!(
            "{why}; could not put trial sleeves back: {}",
            stuck.join("; ")
        ))
    }
}

/// Checkout and gitdir from `worktree.path`. A root that claimed the owner's linked
/// worktree has no marker, so that tree is not listed.
fn worktree_left(sleeve: &Path) -> Option<(PathBuf, PathBuf)> {
    let bytes = fs::read(sleeve.join("worktree.path")).ok()?;
    let checkout = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    let text = fs::read_to_string(checkout.join(".git")).ok()?;
    let rel = text.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
    let gitdir = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        checkout.join(rel)
    };
    gitdir.is_dir().then_some((checkout, gitdir))
}

fn hub_tag(home: &Path) -> Option<String> {
    let bytes = fs::read(home.join("tokens")).ok()?;
    let v: pt_json::Value = pt_json::from_bytes(&bytes).ok()?;
    v["hub_tag"].as_str().map(str::to_string)
}

/// Stops systemd user scopes whose sleeve id is `>= next_sleeve`. Those ids are not in the
/// restored journal. No user bus means no such scope exists.
pub(crate) fn stop_orphan_scopes(tag: &str, next_sleeve: u64) {
    if tag.is_empty() || !pt_sandbox::user_bus_ready() {
        return;
    }
    let units = match pt_sandbox::list_scope_units() {
        Ok(units) => units,
        Err(e) => {
            eprintln!("update: could not list sleeve scopes: {e}");
            return;
        }
    };
    for unit in units {
        let Some(id) = pt_sandbox::scope_sleeve_id(tag, &unit) else {
            continue;
        };
        if id < next_sleeve {
            continue;
        }
        match pt_sandbox::kill_scope(&unit) {
            Ok(()) => eprintln!("update: stopped {unit}"),
            Err(e) => eprintln!("update: stopping {unit}: {e}"),
        }
    }
}

/// Moves `sleeves/<id>` for every id >= `next_sleeve` into a new directory under
/// `update-aside`. Worktree checkouts and gitdirs stay; their paths are printed.
/// Nothing is deleted.
fn quarantine_sleeves(home: &Path, next_sleeve: u64) -> Result<Option<PathBuf>, RestoreError> {
    if let Some(tag) = hub_tag(home) {
        stop_orphan_scopes(&tag, next_sleeve);
    }
    let sleeves = home.join("sleeves");
    let mut ids = Vec::new();
    match fs::read_dir(&sleeves) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| {
                    RestoreError::failed(format!("reading {}: {e}", sleeves.display()))
                })?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Ok(id) = name.parse::<u64>() else {
                    continue;
                };
                if name != id.to_string() || !entry.path().is_dir() {
                    continue;
                }
                if id >= next_sleeve {
                    ids.push(id);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(RestoreError::failed(format!(
                "reading {}: {e}",
                sleeves.display()
            )));
        }
    }
    ids.sort_unstable();
    if ids.is_empty() {
        return Ok(None);
    }
    let mut left = Vec::new();
    for id in &ids {
        if let Some(pair) = worktree_left(&sleeves.join(id.to_string())) {
            left.push(pair);
        }
    }
    let dest =
        home.join("update-aside")
            .join(format!("{}-{}", crate::home::now_ms(), std::process::id()));
    fs::create_dir_all(dest.join("sleeves"))
        .map_err(|e| RestoreError::failed(format!("creating {}: {e}", dest.display())))?;
    let mut moved_sleeves = Vec::new();
    for id in ids {
        let src = sleeves.join(id.to_string());
        let to = dest.join("sleeves").join(id.to_string());
        if let Err(e) = fs::rename(&src, &to) {
            return Err(quarantine_undo(
                &dest,
                &moved_sleeves,
                format!("moving {} to {}: {e}", src.display(), to.display()),
            ));
        }
        moved_sleeves.push((to, src));
    }
    // The renames are durable only once both directories, `update-aside/`, and the new
    // entry under it have been synced. A failure here already moved live sleeve dirs.
    sync_dir(&dest.join("sleeves")).map_err(RestoreError::partial)?;
    sync_dir(&dest).map_err(RestoreError::partial)?;
    let aside = dest.parent().ok_or_else(|| {
        RestoreError::partial(format!(
            "quarantine directory {} has no parent",
            dest.display()
        ))
    })?;
    sync_dir(aside).map_err(RestoreError::partial)?;
    if let Some(root) = aside.parent() {
        sync_dir(root).map_err(RestoreError::partial)?;
    }
    sync_dir(&sleeves).map_err(RestoreError::partial)?;
    for (checkout, gitdir) in &left {
        eprintln!(
            "update: left worktree {} and gitdir {} in place",
            checkout.display(),
            gitdir.display()
        );
    }
    Ok(Some(dest))
}

fn headscale_paths(snapshot: &Path) -> Result<(PathBuf, PathBuf), RestoreError> {
    let fleet = snapshot.parent().and_then(Path::parent).ok_or_else(|| {
        RestoreError::failed(format!("invalid headscale snapshot {}", snapshot.display()))
    })?;
    Ok((fleet.join("headscale"), snapshot.join("db.sqlite")))
}

fn validate_headscale(snapshot: &Path) -> Result<(), RestoreError> {
    let (_dest, db) = headscale_paths(snapshot)?;
    if !db.is_file() {
        return Err(RestoreError::failed(format!(
            "headscale snapshot {} has no database",
            snapshot.display()
        )));
    }
    if ["db.sqlite-wal", "db.sqlite-shm"]
        .iter()
        .any(|name| snapshot.join(name).exists())
    {
        return Err(RestoreError::failed(format!(
            "headscale snapshot {} has SQLite sidecars; cannot restore it atomically",
            snapshot.display()
        )));
    }
    Ok(())
}

fn park_rollback_files(dest: &Path) -> Result<(), RestoreError> {
    // Sidecars first. Parking the database first and crashing leaves the trial WAL
    // beside the restored database, and prepare puts that WAL back.
    let names = [
        ".rollback-db.sqlite-wal",
        ".rollback-db.sqlite-shm",
        ".rollback-db.sqlite",
    ];
    if names.iter().all(|name| !dest.join(name).exists()) {
        return Ok(());
    }
    let aside = dest.join(format!(
        "rollback-aside-{}-{}",
        crate::home::now_ms(),
        std::process::id()
    ));
    fs::create_dir(&aside)
        .map_err(|e| RestoreError::partial(format!("creating {}: {e}", aside.display())))?;
    for name in names {
        let path = dest.join(name);
        if !path.exists() {
            continue;
        }
        fs::rename(&path, aside.join(name)).map_err(|e| {
            RestoreError::partial(format!(
                "moving {} to {}: {e}",
                path.display(),
                aside.display()
            ))
        })?;
    }
    sync_dir(&aside).map_err(RestoreError::partial)?;
    sync_dir(dest).map_err(RestoreError::partial)?;
    Ok(())
}

fn put_back_file(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        return Err(format!(
            "{} and {} both exist",
            from.display(),
            to.display()
        ));
    }
    fs::rename(from, to).map_err(|e| format!("moving {} to {}: {e}", from.display(), to.display()))
}

/// The database replace is staged as `.restore-db.sqlite`, then the live WAL, SHM and database
/// move to `.rollback-*` names and the staged file takes the live name. A crash in that window
/// is finished or undone here, and the `.rollback-*` files are parked so SQLite never opens them.
fn prepare_headscale(dest: &Path) -> Result<HeadscaleReady, RestoreError> {
    let db = dest.join("db.sqlite");
    let staged = dest.join(".restore-db.sqlite");
    let old = dest.join(".rollback-db.sqlite");
    let old_wal = dest.join(".rollback-db.sqlite-wal");
    let old_shm = dest.join(".rollback-db.sqlite-shm");
    // The staged copy already replaced the live database. `.rollback-*` is the previous
    // database, not yet parked. Reporting Pending would let a later failure keep the trial
    // on that replaced database.
    if db.exists() && !staged.exists() && old.exists() {
        park_rollback_files(dest)?;
        return Ok(HeadscaleReady::Restored);
    }
    if db.exists() && !old.exists() && (old_wal.exists() || old_shm.exists()) {
        put_back_file(&old_wal, &dest.join("db.sqlite-wal")).map_err(RestoreError::partial)?;
        put_back_file(&old_shm, &dest.join("db.sqlite-shm")).map_err(RestoreError::partial)?;
        sync_dir(dest).map_err(RestoreError::partial)?;
    }
    if old.exists() && !db.exists() {
        if !staged.exists() {
            return Err(RestoreError::partial(format!(
                "headscale database was moved to {} and the staged copy is missing",
                old.display()
            )));
        }
        fs::rename(&staged, &db).map_err(|e| {
            RestoreError::partial(format!(
                "finishing the headscale restore from {} to {}: {e}",
                staged.display(),
                db.display()
            ))
        })?;
        sync_dir(dest).map_err(RestoreError::partial)?;
        park_rollback_files(dest)?;
        return Ok(HeadscaleReady::Restored);
    }
    if staged.exists() {
        fs::remove_file(&staged)
            .map_err(|e| RestoreError::failed(format!("removing {}: {e}", staged.display())))?;
    }
    Ok(HeadscaleReady::Pending)
}

enum HeadscaleReady {
    Pending,
    Restored,
}

fn stage_headscale(snapshot: &Path, dest: &Path) -> Result<(), RestoreError> {
    let db = snapshot.join("db.sqlite");
    let staged = dest.join(".restore-db.sqlite");
    copy_synced(&db, &staged).map_err(RestoreError::failed)?;
    sync_dir(dest).map_err(|e| {
        let _ = fs::remove_file(&staged);
        RestoreError::failed(e)
    })
}

fn commit_headscale(dest: &Path) -> Result<(), RestoreError> {
    let db = dest.join("db.sqlite");
    let staged = dest.join(".restore-db.sqlite");
    let moved = |name: &str| -> Result<bool, RestoreError> {
        let path = dest.join(name);
        let backup = dest.join(format!(".rollback-{name}"));
        match fs::rename(&path, &backup) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(RestoreError::failed(format!(
                "saving {} before restore: {e}",
                path.display()
            ))),
        }
    };
    let undo = |names: &[&str]| -> Result<(), String> {
        for name in names.iter().rev() {
            put_back_file(&dest.join(format!(".rollback-{name}")), &dest.join(name))?;
        }
        Ok(())
    };
    let mut aside = Vec::new();
    for name in ["db.sqlite-wal", "db.sqlite-shm"] {
        match moved(name) {
            Ok(true) => aside.push(name),
            Ok(false) => {}
            Err(e) => {
                if let Err(back) = undo(&aside) {
                    return Err(RestoreError::partial(format!(
                        "{}; putting headscale files back failed: {back}",
                        e.message()
                    )));
                }
                let _ = fs::remove_file(&staged);
                return Err(e);
            }
        }
    }
    if let Err(e) = sync_dir(dest) {
        if let Err(back) = undo(&aside) {
            return Err(RestoreError::partial(format!(
                "{e}; putting headscale files back failed: {back}"
            )));
        }
        let _ = fs::remove_file(&staged);
        return Err(RestoreError::failed(e));
    }
    let db_aside = match fs::rename(&db, dest.join(".rollback-db.sqlite")) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            if let Err(back) = undo(&aside) {
                return Err(RestoreError::partial(format!(
                    "moving {} aside: {e}; putting headscale files back failed: {back}",
                    db.display()
                )));
            }
            let _ = fs::remove_file(&staged);
            return Err(RestoreError::failed(format!(
                "moving {} aside: {e}",
                db.display()
            )));
        }
    };
    if let Err(e) = fs::rename(&staged, &db) {
        let restore_old = if db_aside {
            fs::rename(dest.join(".rollback-db.sqlite"), &db)
        } else {
            Ok(())
        };
        let restore_side = undo(&aside);
        return Err(match (restore_old, restore_side) {
            (Ok(()), Ok(())) => {
                let _ = fs::remove_file(&staged);
                RestoreError::failed(format!(
                    "restoring {} to {}: {e}",
                    staged.display(),
                    db.display()
                ))
            }
            _ => RestoreError::partial(format!(
                "restoring {} to {} failed: {e}; the previous database is at {}",
                staged.display(),
                db.display(),
                dest.join(".rollback-db.sqlite").display()
            )),
        });
    }
    sync_dir(dest).map_err(RestoreError::partial)?;
    park_rollback_files(dest)?;
    Ok(())
}

pub fn restore_snapshots(home: &Path, store: &Store, p: &Probation) -> Result<(), RestoreError> {
    // Validation and staging return before this attempt would write the marker.
    // A marker left by an earlier attempt means a live file was already replaced,
    // so that failure is partial and must not keep the trial.
    let open = store
        .rollback_open()
        .map_err(|e| RestoreError::failed(e.to_string()))?;
    match restore_snapshot_files(home, store, p) {
        Err(RestoreError::Failed(message)) if open => Err(RestoreError::partial(message)),
        other => other,
    }
}

fn restore_snapshot_files(home: &Path, store: &Store, p: &Probation) -> Result<(), RestoreError> {
    if p.journal_snapshot.is_none() && p.headscale_snapshot.is_none() {
        return Ok(());
    }
    if store.running_release().as_deref() != Some(p.previous.as_str()) {
        return Err(RestoreError::failed(format!(
            "rollback restore must run as releases/{}/protectorate",
            p.previous
        )));
    }
    // The caller holds `journal/lock` when that file exists. A second flock in this
    // process fails, so this function does not lock again.
    let keep_journal = p.manual_rollback && journal_readable(&home.join("journal"));
    let journal_snapshot = if keep_journal {
        None
    } else {
        p.journal_snapshot.as_deref()
    };
    if let Some(snapshot) = journal_snapshot {
        validate_journal_snapshot(Path::new(snapshot))?;
        if p.next_sleeve.is_none() {
            return Err(RestoreError::failed(
                "probation record has no next_sleeve, so trial sleeve directories cannot be quarantined",
            ));
        }
    }
    if let Some(snapshot) = p.headscale_snapshot.as_deref() {
        validate_headscale(Path::new(snapshot))?;
    }

    let staged_journal = journal_snapshot
        .map(|snapshot| stage_journal(home, Path::new(snapshot)))
        .transpose()?;
    let headscale = match p.headscale_snapshot.as_deref() {
        Some(snapshot) => {
            let (dest, _) = headscale_paths(Path::new(snapshot))?;
            fs::create_dir_all(&dest).map_err(|e| {
                if let Some(staged) = &staged_journal {
                    discard_dir(staged);
                }
                RestoreError::failed(format!("creating {}: {e}", dest.display()))
            })?;
            match prepare_headscale(&dest) {
                Ok(HeadscaleReady::Pending) => {
                    if let Err(e) = stage_headscale(Path::new(snapshot), &dest) {
                        if let Some(staged) = &staged_journal {
                            discard_dir(staged);
                        }
                        return Err(e);
                    }
                    Some((dest, false))
                }
                Ok(HeadscaleReady::Restored) => Some((dest, true)),
                Err(e) => {
                    if let Some(staged) = &staged_journal {
                        discard_dir(staged);
                    }
                    return Err(e);
                }
            }
        }
        None => None,
    };

    // Synced before scope stop, quarantine, the headscale replace, or the journal
    // swap. A later failure while it exists stays partial, including one that
    // replaces nothing on a later prestart.
    let replacing = staged_journal.is_some() || headscale.is_some();
    if replacing && let Err(e) = store.mark_rollback_open() {
        if let Some(staged) = &staged_journal {
            discard_dir(staged);
        }
        if let Some((dest, false)) = &headscale {
            let _ = fs::remove_file(dest.join(".restore-db.sqlite"));
        }
        return Err(RestoreError::failed(e.to_string()));
    }
    let opened = |e: RestoreError| match e {
        RestoreError::Failed(message) => RestoreError::Partial(message),
        other => other,
    };

    let mut committed = headscale.as_ref().is_some_and(|(_, done)| *done);
    if let (Some(next), Some(_)) = (p.next_sleeve, staged_journal.as_ref()) {
        match quarantine_sleeves(home, next) {
            Ok(Some(dir)) => {
                committed = true;
                eprintln!(
                    "update: moved trial sleeves at or above {next} to {}",
                    dir.display()
                );
            }
            Ok(None) => {}
            Err(e) => {
                if let Some(staged) = &staged_journal {
                    discard_dir(staged);
                }
                if let Some((dest, false)) = &headscale {
                    let _ = fs::remove_file(dest.join(".restore-db.sqlite"));
                }
                return Err(opened(e));
            }
        }
    }
    if let Some((dest, false)) = &headscale {
        if let Err(e) = commit_headscale(dest) {
            if !committed && let Some(staged) = &staged_journal {
                discard_dir(staged);
            }
            return Err(opened(e));
        }
        eprintln!(
            "update: restored headscale database from {}; nodes enrolled after this snapshot are lost",
            p.headscale_snapshot.as_deref().unwrap_or_default()
        );
    }
    if let Some(staged) = staged_journal {
        let snapshot = journal_snapshot.unwrap_or_default();
        match swap_journal(home, staged) {
            Ok(aside) => {
                eprintln!(
                    "update: restored journal snapshot from {snapshot}; events after the snapshot are lost (snapshot time: {} ms since epoch); trial journal kept at {}",
                    p.journal_snapshot_ms
                        .map_or_else(|| "unknown".into(), |ms| ms.to_string()),
                    aside.display()
                );
            }
            Err(e) => {
                return Err(opened(e));
            }
        }
    } else if p.journal_snapshot.is_some() {
        eprintln!("update: kept the journal; this release reads every event");
    }
    Ok(())
}

/// How far [`prepare_prestart`] got with the journal lock.
pub enum Prestart {
    /// This process may judge. The file is the journal lock when this process holds it.
    Ready(Option<File>),
    /// Another process holds the journal lock. Do not judge, restore, or count a start.
    Busy,
}

/// Probation state for a caller that must not judge. Reads the record and `current`
/// and does not take the release-store lock, so it cannot finish a switch or roll back.
pub fn probation_report(home: &Path) -> Result<String, String> {
    let path = home.join("releases/probation.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok("no release on trial".into());
        }
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    let p: Probation =
        pt_json::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if p.rolled_back_ms.is_some() {
        return Ok("no release on trial".into());
    }
    if p.restore_failed.is_some() && p.reason.is_none() {
        return Ok("snapshot restore failed; keeping the current release".into());
    }
    if p.contacted_ms.is_some() && !p.manual_rollback {
        return Ok("the release on trial was confirmed".into());
    }
    let current = fs::read_link(home.join("releases/current"))
        .ok()
        .and_then(|target| target.to_str().map(str::to_string));
    if current.as_deref() != Some(p.version.as_str()) {
        return Ok("no release on trial".into());
    }
    let now = crate::home::now_ms();
    if p.reason.is_some() || now >= p.deadline_ms || p.starts >= pt_update::MAX_TRIAL_STARTS {
        return Ok("on trial, rollback pending while the hub holds the journal".into());
    }
    let left = p.deadline_ms.saturating_sub(now) / 1000;
    Ok(format!("on trial, {left} s left"))
}

enum JournalGate {
    Held(File),
    Missing,
    Busy,
}

fn take_journal_lock(home: &Path) -> Result<JournalGate, String> {
    let path = home.join("journal/lock");
    if !path.exists() {
        return Ok(JournalGate::Missing);
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    match lock.try_lock() {
        Ok(()) => Ok(JournalGate::Held(lock)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(JournalGate::Busy),
        Err(std::fs::TryLockError::Error(e)) => Err(format!(
            "stop the hub before restoring a rollback ({}: {e})",
            path.display()
        )),
    }
}

/// Takes the journal lock before [`finish_interrupted_journal_swap`] when `journal/`
/// exists, and holds it for the caller's judge. A busy lock leaves the journal alone.
/// `held` finishes under the caller's lock and does not lock again. A missing journal
/// is finished first, then locked if `journal/lock` is there.
pub fn prepare_prestart(home: &Path, held: bool) -> Result<Prestart, String> {
    if held {
        finish_interrupted_journal_swap(home)?;
        return Ok(Prestart::Ready(None));
    }
    if !home.join("journal").exists() {
        finish_interrupted_journal_swap(home)?;
    }
    match take_journal_lock(home)? {
        JournalGate::Busy => Ok(Prestart::Busy),
        JournalGate::Missing => {
            if home.join("journal").exists() {
                finish_interrupted_journal_swap(home)?;
            }
            Ok(Prestart::Ready(None))
        }
        JournalGate::Held(lock) => {
            finish_interrupted_journal_swap(home)?;
            Ok(Prestart::Ready(Some(lock)))
        }
    }
}

/// The journal lock, taken before the release-store lock. `None` when the journal has no lock
/// file yet. Fails while another process holds it.
pub fn acquire_journal_lock(home: &Path) -> Result<Option<File>, String> {
    match take_journal_lock(home)? {
        JournalGate::Held(lock) => Ok(Some(lock)),
        JournalGate::Missing => Ok(None),
        JournalGate::Busy => Err(format!(
            "stop the hub before restoring a rollback ({}: lock acquisition failed because the operation would block)",
            home.join("journal/lock").display()
        )),
    }
}

/// `--retry-rollback` refuses while headscale still has its database or socket open. The
/// process dies with the hub, but a retry must not rename the database out from under it.
pub fn ensure_headscale_stopped(home: &Path) -> Result<(), String> {
    let store = Store::open(home).map_err(|e| e.to_string())?;
    let Some(snapshot) = store
        .probation()
        .map_err(|e| e.to_string())?
        .and_then(|p| p.headscale_snapshot)
    else {
        return Ok(());
    };
    let Some(fleet) = Path::new(&snapshot).parent().and_then(Path::parent) else {
        return Ok(());
    };
    let dest = fleet.join("headscale");
    let sock = dest.join("headscale.sock");
    if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
        return Err(format!(
            "stop the hub before retrying rollback: headscale is still listening on {}",
            sock.display()
        ));
    }
    let mut open = Vec::new();
    for name in ["db.sqlite", "db.sqlite-wal", "db.sqlite-shm"] {
        if let Ok(meta) = fs::metadata(dest.join(name)) {
            open.push((meta.dev(), meta.ino()));
        }
    }
    if open.is_empty() {
        return Ok(());
    }
    let me = std::process::id();
    for entry in fs::read_dir("/proc").map_err(|e| format!("reading /proc: {e}"))? {
        let entry = entry.map_err(|e| format!("reading /proc: {e}"))?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(meta) = fs::metadata(fd.path()) else {
                continue;
            };
            if open.iter().any(|id| *id == (meta.dev(), meta.ino())) {
                return Err(format!(
                    "stop the hub before retrying rollback: process {pid} still has {} open",
                    dest.display()
                ));
            }
        }
    }
    Ok(())
}

/// A failed restore that kept the trial release: `hub info` carries it, and the event log
/// records an alarm. Called once the stream writer is running.
pub fn announce_restore_failure(hub: &crate::hub::Hub) {
    let Ok(store) = Store::open(&hub.home.root) else {
        return;
    };
    let Ok(Some(p)) = store.probation() else {
        return;
    };
    let Some(msg) = p.restore_failed else { return };
    let Ok(Some(current)) = store.current() else {
        return;
    };
    if current != p.version {
        return;
    }
    lock(&hub.info).restore_error = Some(msg.clone());
    hub.event(
        Kind::Hub,
        Level::Alarm,
        "update",
        "owner",
        &format!("rollback restore failed: {msg}"),
    );
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
