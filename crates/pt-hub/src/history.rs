//! The history index (SPEC 3.7) over the sessions this hub launched (D21): every incarnation's
//! native session log, found by the session id its driver reported in the sleeve's private home,
//! and the hub's own per-sleeve transcript. One background thread tails new bytes of each, about
//! once a second, and commits after each catch-up pass.
//!
//! The index is derived. A missing directory is built from the sources; a corrupt one is moved
//! aside (never deleted) and rebuilt the same way, so the hub always starts.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use pt_api::Harness as ApiHarness;
use pt_core::{Proc, Sleeve, SleeveId};
use pt_drivers::{Entry, Harness};
use pt_history::{ContextUsage, Doc, Filters, Hit, Index, Query, Selector, Source, SourceKind};

use crate::home::now_ms;
use crate::hub::Hub;
use crate::log::{debug, info, warning};
use crate::transcript::{Line, lock};

const POLL: Duration = Duration::from_secs(1);
/// Longest wait after a failed commit before the next catch-up.
const COMMIT_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Why a history read failed.
#[derive(Debug)]
pub enum Failure {
    /// There is no index (it could not be opened or rebuilt).
    Off(String),
    Io(io::Error),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Off(e) => write!(f, "the history index is off: {e}"),
            Failure::Io(e) => write!(f, "history: {e}"),
        }
    }
}

/// Which source indexes a session.
#[derive(Clone, Debug, PartialEq)]
enum Choice {
    /// Live, and its native log has not appeared yet: the transcript waits before it.
    Undecided,
    Native(PathBuf),
    Transcript,
}

/// One `session_info` line of a sleeve's transcript.
struct Seen {
    offset: u64,
    g: u64,
    native_id: String,
    kind: SourceKind,
    harness: Option<ApiHarness>,
}

#[derive(Default)]
struct Track {
    /// Transcript bytes already scanned for `session_info` lines.
    cursor: pt_history::tail::Cursor,
    seen: Vec<Seen>,
}

/// What only the ingest pass touches.
#[derive(Default)]
struct Passes {
    tracks: BTreeMap<SleeveId, Track>,
    /// Native session id, per sleeve. One sleeve's id never selects another's log.
    choices: HashMap<(SleeveId, String), Choice>,
    /// The last error per source and of the last commit, so a lasting one is logged once.
    errors: HashMap<PathBuf, String>,
    commit_error: Option<String>,
}

pub struct History {
    ix: Mutex<Result<Index, String>>,
    /// Sleeve ids at the history budget. Copied out of the index at open and after each
    /// catch-up, so a reader does not take the index lock. A sleeve leaves the set once it
    /// is back under the budget.
    capped: Mutex<BTreeSet<String>>,
    /// The sleeve kind each native session ran as (the drivers say only `pi`).
    harnesses: Mutex<HashMap<String, ApiHarness>>,
    passes: Mutex<Passes>,
}

fn source_kind(h: Harness) -> SourceKind {
    match h {
        Harness::Claude => SourceKind::Claude,
        Harness::Codex => SourceKind::Codex,
        Harness::Grok => SourceKind::Grok,
        Harness::Pi => SourceKind::Pi,
    }
}

/// Where each harness keeps its session logs under the `$HOME` it ran with.
fn log_root(home: &Path, kind: SourceKind) -> PathBuf {
    match kind {
        SourceKind::Claude => home.join(".claude/projects"),
        SourceKind::Codex => home.join(".codex/sessions"),
        SourceKind::Grok => home.join(".grok/sessions"),
        SourceKind::Pi | SourceKind::Hub => home.join(".pi/agent/sessions"),
    }
}

fn open_index(dir: &Path) -> Result<Index, String> {
    match Index::open(dir) {
        Ok(ix) => Ok(ix),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::PermissionDenied
            ) =>
        {
            Err(e.to_string())
        }
        Err(e) => {
            let aside = dir.with_file_name(format!(
                "{}.corrupt-{}",
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("history"),
                now_ms()
            ));
            warning!(
                "history: {e}; moving the index to {} and rebuilding it from the sources",
                aside.display()
            );
            fs::rename(dir, &aside)
                .map_err(|e| format!("history: move {} aside: {e}", dir.display()))?;
            Index::open(dir).map_err(|e| e.to_string())
        }
    }
}

impl History {
    /// Opens the index in `dir`, rebuilding a corrupt one. Never fails: without an index the
    /// history methods answer `unavailable` with the reason.
    pub fn open(dir: &Path) -> History {
        let ix = open_index(dir);
        let capped = match &ix {
            Ok(ix) => {
                let st = ix.stats();
                info!(
                    "history: {} docs of {} sessions in {}",
                    st.docs,
                    st.sessions,
                    dir.display()
                );
                ix.capped_sleeves().into_iter().collect()
            }
            Err(e) => {
                warning!("history is off: {e}");
                BTreeSet::new()
            }
        };
        History {
            ix: Mutex::new(ix),
            capped: Mutex::new(capped),
            harnesses: Mutex::new(HashMap::new()),
            passes: Mutex::new(Passes::default()),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&Index) -> io::Result<T>) -> Result<T, Failure> {
        match &*lock(&self.ix) {
            Ok(ix) => f(ix).map_err(Failure::Io),
            Err(e) => Err(Failure::Off(e.clone())),
        }
    }

    pub fn search(&self, q: &Query, f: &Filters, limit: usize) -> Result<Vec<Hit>, Failure> {
        self.with(|ix| ix.search(q, f, limit))
    }

    pub fn read(
        &self,
        sel: &Selector,
        sleeves: Option<&[String]>,
        limit: usize,
    ) -> Result<(Vec<Doc>, bool), Failure> {
        self.with(|ix| ix.read(sel, sleeves, limit))
    }

    pub fn docs(&self) -> u64 {
        self.with(|ix| Ok(ix.docs())).unwrap_or(0)
    }

    /// Sleeve ids at the history budget, in order. This does not take the index lock.
    /// The set is filled when the index opens, before the first `doctor` run.
    pub fn capped_sleeves(&self) -> Vec<String> {
        lock(&self.capped).iter().cloned().collect()
    }

    pub fn context_usage(&self, session: &str) -> Option<ContextUsage> {
        self.with(|ix| Ok(ix.context_usage(session))).ok().flatten()
    }

    pub fn harness(&self, session: &str) -> Option<ApiHarness> {
        lock(&self.harnesses).get(session).copied()
    }

    /// Tails every source of `only` (or of every sleeve) to its end and commits. The brief calls
    /// this before its pre-run search, so the outgoing session's last lines are in. True when
    /// that commit failed.
    pub fn catch_up(&self, hub: &Hub, only: Option<SleeveId>) -> bool {
        let mut p = lock(&self.passes);
        let view = hub.view();
        let mut changed = false;
        for s in view.state.sleeves.values() {
            if only.is_none_or(|o| o == s.id) {
                changed |= self.sleeve_pass(hub, &mut p, s);
            }
        }
        let mut failed = false;
        if let Ok(ix) = &mut *lock(&self.ix)
            && (changed || ix.has_uncommitted())
        {
            match ix.commit() {
                Ok(()) => p.commit_error = None,
                Err(e) => {
                    failed = true;
                    let e = e.to_string();
                    if p.commit_error.replace(e.clone()).as_ref() != Some(&e) {
                        warning!("history commit: {e}");
                    }
                }
            }
        }
        if let Ok(ix) = &*lock(&self.ix) {
            self.sync_capped(&ix.capped_sleeves());
        }
        failed
    }

    /// Replaces the named set with `now`. A sleeve that was not named warns once.
    fn sync_capped(&self, now: &[String]) {
        let mut set = lock(&self.capped);
        for id in now {
            if set.insert(id.clone()) {
                warning!("history capped: {id}");
            }
        }
        set.retain(|id| now.iter().any(|n| n == id));
    }

    /// One sleeve: new `session_info` lines, a source per session, then the reads. Returns
    /// whether anything was read.
    fn sleeve_pass(&self, hub: &Hub, p: &mut Passes, s: &Sleeve) -> bool {
        let transcript = hub.home.transcript(s.id);
        let track = p.tracks.entry(s.id).or_default();
        if let Err(e) = scan(&transcript, track) {
            debug!("history: scan {}: {e}", transcript.display());
        }
        if track.seen.is_empty() && track.cursor.offset == 0 {
            return false;
        }
        let sleeve = s.id.0.to_string();
        // The session still being written: the newest one, while its incarnation runs.
        let open = track
            .seen
            .last()
            .filter(|l| l.g == s.g.0 && s.proc != Proc::Down)
            .map(|l| l.native_id.clone());
        let home = hub.home.sleeve_home(s.id);
        let mut found: HashMap<SourceKind, HashMap<String, PathBuf>> = HashMap::new();
        for seen in &track.seen {
            let choice = p
                .choices
                .entry((s.id, seen.native_id.clone()))
                .or_insert(Choice::Undecided);
            if *choice != Choice::Undecided {
                continue;
            }
            // Walk a log root only while some session of that harness is still undecided.
            let logs = found.entry(seen.kind).or_insert_with(|| {
                pt_history::discover(seen.kind, &log_root(&home, seen.kind))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|src| (src.session, src.path))
                    .collect()
            });
            // One source per session: a headless session is in both its native log and the
            // transcript. The native log wins when the hub can find it: it is the harness's own
            // record (compaction summaries, per-call context usage, lines written while the hub
            // was down), and `history read` points into it. The transcript covers a session
            // whose log never appeared by the time it ended (or was pruned before a rebuild).
            if let Some(path) = logs.get(&seen.native_id) {
                *choice = Choice::Native(path.clone());
            } else if open.as_deref() != Some(seen.native_id.as_str()) {
                *choice = Choice::Transcript;
            }
        }
        // This pass's discover may show that a native log moved.
        for seen in &track.seen {
            let Some(logs) = found.get(&seen.kind) else {
                continue;
            };
            let Some(found_path) = logs.get(&seen.native_id) else {
                continue;
            };
            let Some(Choice::Native(path)) = p.choices.get_mut(&(s.id, seen.native_id.clone()))
            else {
                continue;
            };
            // A second file with the same session id is not a move. Follow the new
            // path only once the old one is gone, so a copy does not drop the live log.
            if path == found_path || path.exists() {
                continue;
            }
            let old = path.clone();
            if let Ok(ix) = &mut *lock(&self.ix) {
                ix.forget(seen.kind, &old, Some(&sleeve));
            }
            *path = found_path.clone();
        }
        {
            let mut harnesses = lock(&self.harnesses);
            for seen in &track.seen {
                if let Some(h) = seen.harness {
                    harnesses.insert(seen.native_id.clone(), h);
                }
            }
        }
        let mut sources = Vec::new();
        let mut skip = Vec::new();
        let mut hold = None;
        for seen in &track.seen {
            match &p.choices[&(s.id, seen.native_id.clone())] {
                // Always skip the transcript for a native choice, including when this pass
                // did not see the file. A missed discover must not index the session twice.
                Choice::Native(path) => {
                    if !skip.contains(&seen.native_id) {
                        skip.push(seen.native_id.clone());
                        sources.push(
                            Source::new(seen.kind, path, seen.native_id.clone())
                                .sleeve(sleeve.clone()),
                        );
                    }
                }
                Choice::Undecided => {
                    hold.get_or_insert(seen.offset);
                }
                Choice::Transcript => {}
            }
        }
        let mut t = Source::new(SourceKind::Hub, transcript, sleeve.clone()).sleeve(sleeve);
        t.skip = skip;
        t.limit = hold;
        sources.push(t);
        let mut changed = false;
        for src in &sources {
            let got = match &mut *lock(&self.ix) {
                Ok(ix) => ix.ingest(src),
                Err(_) => return false,
            };
            match got {
                Ok(g) => {
                    p.errors.remove(&src.path);
                    changed |= g.bytes > 0 || g.reset || g.capped;
                }
                Err(e) => {
                    let e = e.to_string();
                    if p.errors.insert(src.path.clone(), e.clone()).as_ref() != Some(&e) {
                        if src.kind == SourceKind::Hub || !src.path.exists() {
                            debug!("history ingest: {e}");
                        } else {
                            warning!("history ingest: {e}");
                        }
                    }
                }
            }
        }
        changed
    }
}

/// Reads the transcript's new complete lines for `session_info` entries.
fn scan(path: &Path, track: &mut Track) -> io::Result<()> {
    let mut opened = match pt_history::tail::open(path, &track.cursor) {
        Ok(opened) => opened,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if opened.reset {
        *track = Track::default();
    }
    loop {
        let start = opened.start.offset;
        let end = start
            .saturating_add(pt_history::tail::WINDOW as u64)
            .min(opened.len);
        let tail = opened.read(Some(end), |at, line| {
            if !line.windows(14).any(|w| w == b"\"session_info\"") {
                return;
            }
            if let Ok(Line {
                g,
                harness: api,
                entry:
                    Entry::SessionInfo {
                        harness, native_id, ..
                    },
                ..
            }) = pt_json::from_bytes::<Line>(line)
                && native_id.len() <= 1024
            {
                track.seen.push(Seen {
                    offset: at,
                    g,
                    native_id,
                    kind: source_kind(harness),
                    harness: api,
                });
            }
        })?;
        track.cursor = tail.cursor;
        if opened.start.offset >= opened.len || opened.start.offset == start {
            return Ok(());
        }
    }
}

/// The ingest thread: a catch-up pass about once a second until the hub is gone.
pub fn start(hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    let started = thread::Builder::new()
        .name("pt-history".into())
        .spawn(move || {
            let mut pause = POLL;
            loop {
                let Some(hub) = weak.upgrade() else { return };
                let failed = hub.history.catch_up(&hub, None);
                drop(hub);
                thread::sleep(pause);
                pause = if failed {
                    pause.saturating_mul(2).min(COMMIT_BACKOFF_MAX)
                } else {
                    POLL
                };
            }
        });
    if let Err(e) = started {
        warning!("history: starting the ingest thread: {e}");
    }
}
