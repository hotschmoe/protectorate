//! The history index (SPEC 3.7) over the sessions this hub launched (D21): every incarnation's
//! native session log, found by the session id its driver reported in the sleeve's private home,
//! and the hub's own per-sleeve transcript. One background thread tails new bytes of each, about
//! once a second, and commits after each catch-up pass.
//!
//! The index is derived. A missing directory is built from the sources; a corrupt one is moved
//! aside (never deleted) and rebuilt the same way, so the hub always starts.

use std::collections::{BTreeMap, HashMap};
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
    choices: HashMap<String, Choice>,
    /// The last error per source and of the last commit, so a lasting one is logged once.
    errors: HashMap<PathBuf, String>,
    commit_error: Option<String>,
}

pub struct History {
    ix: Mutex<Result<Index, String>>,
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
        match &ix {
            Ok(ix) => {
                let st = ix.stats();
                info!(
                    "history: {} docs of {} sessions in {}",
                    st.docs,
                    st.sessions,
                    dir.display()
                );
            }
            Err(e) => warning!("history is off: {e}"),
        }
        History {
            ix: Mutex::new(ix),
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

    pub fn read(&self, sel: &Selector) -> Result<Vec<Doc>, Failure> {
        self.with(|ix| ix.read(sel))
    }

    pub fn docs(&self) -> u64 {
        self.with(|ix| Ok(ix.docs())).unwrap_or(0)
    }

    pub fn context_usage(&self, session: &str) -> Option<ContextUsage> {
        self.with(|ix| Ok(ix.context_usage(session))).ok().flatten()
    }

    pub fn harness(&self, session: &str) -> Option<ApiHarness> {
        lock(&self.harnesses).get(session).copied()
    }

    /// Tails every source of `only` (or of every sleeve) to its end and commits. The brief calls
    /// this before its pre-run search, so the outgoing session's last lines are in.
    pub fn catch_up(&self, hub: &Hub, only: Option<SleeveId>) {
        let mut p = lock(&self.passes);
        let view = hub.view();
        let mut changed = false;
        for s in view.state.sleeves.values() {
            if only.is_none_or(|o| o == s.id) {
                changed |= self.sleeve_pass(hub, &mut p, s);
            }
        }
        if let Ok(ix) = &mut *lock(&self.ix)
            && (changed || ix.has_uncommitted())
        {
            match ix.commit() {
                Ok(()) => p.commit_error = None,
                Err(e) => {
                    let e = e.to_string();
                    if p.commit_error.replace(e.clone()).as_ref() != Some(&e) {
                        warning!("history commit: {e}");
                    }
                }
            }
        }
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
                .entry(seen.native_id.clone())
                .or_insert(Choice::Undecided);
            if *choice != Choice::Undecided {
                continue;
            }
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
            match &p.choices[&seen.native_id] {
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
                    changed |= g.bytes > 0 || g.reset;
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
            loop {
                let Some(hub) = weak.upgrade() else { return };
                hub.history.catch_up(&hub, None);
                drop(hub);
                thread::sleep(POLL);
            }
        });
    if let Err(e) = started {
        warning!("history: starting the ingest thread: {e}");
    }
}
