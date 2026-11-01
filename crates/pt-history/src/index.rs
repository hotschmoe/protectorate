//! The index directory: manifest, segments, ingest, commit and merges.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use pt_journal::crc32c::crc32c;
use pt_json::ToJson;

use crate::extract::{self, Draft, Item, State};
use crate::segment::{self, Dict, DocIn, Meta, NONE};
use crate::tail::{self, Cursor};
use crate::{ContextUsage, Doc, DocId, Selector, Source, SourceKind};

const MANIFEST: &str = "manifest";
const MANIFEST_MAGIC: &str = "PTHISTMF";
const VERSION: u32 = 1;
/// Pending text that triggers a commit after a read window.
const FLUSH: usize = 96 << 20;
/// More live segments than this start merges.
const MAX_SEGMENTS: usize = 8;
/// Merges never build a segment larger than this.
const MAX_MERGE: u64 = 1 << 30;

pt_json::record! {
    #[derive(Clone, Debug, Default)]
    struct Manifest {
        version: u32 = 0,
        next_doc: u64 = 0,
        next_seg: u64 = 0,
        segments: Vec<SegRec> = Vec::new(),
        sources: Vec<SourceRec> = Vec::new(),
        drops: Vec<DropRec> = Vec::new(),
        usage: Vec<ContextUsage> = Vec::new(),
    }

    #[derive(Clone, Debug)]
    struct SegRec {
        file: String,
        docs: u64,
        bytes: u64,
    }

    #[derive(Clone, Debug)]
    struct SourceRec {
        kind: SourceKind,
        path: String,
        sleeve: Option<String>,
        cursor: Cursor,
        state: State,
    }

    #[derive(Clone, Debug)]
    struct DropRec {
        source: String,
        below: u64,
    }
}

/// Interned strings shared by all loaded segments.
#[derive(Default)]
pub(crate) struct Syms {
    names: Vec<Box<str>>,
    ids: std::collections::HashMap<Box<str>, u32, std::hash::BuildHasherDefault<segment::Fx>>,
}

impl Syms {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(s.into());
        self.ids.insert(s.into(), id);
        id
    }

    pub(crate) fn get(&self, s: &str) -> Option<u32> {
        self.ids.get(s).copied()
    }

    pub(crate) fn name(&self, id: u32) -> Option<&str> {
        if id == NONE {
            return None;
        }
        self.names.get(id as usize).map(|s| &**s)
    }
}

pub(crate) struct Seg {
    pub name: String,
    pub file: File,
    pub docs: Vec<Meta>,
    pub dict: Dict,
    pub postings: std::ops::Range<u64>,
    pub texts: std::ops::Range<u64>,
    pub bytes: u64,
}

impl Seg {
    pub(crate) fn text(&self, m: &Meta) -> io::Result<String> {
        let mut buf = vec![0u8; m.text_len as usize];
        self.file
            .read_exact_at(&mut buf, self.texts.start + m.text_off)
            .map_err(|e| ctx(e, "read text", &self.name))?;
        String::from_utf8(buf).map_err(|_| invalid(&self.name, "text is not UTF-8"))
    }

    pub(crate) fn postings(&self, p: segment::Post) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; p.len as usize];
        self.file
            .read_exact_at(&mut buf, self.postings.start + p.off)
            .map_err(|e| ctx(e, "read postings", &self.name))?;
        Ok(buf)
    }
}

struct Pending {
    id: u64,
    offset: u64,
    source: String,
    sleeve: Option<String>,
    draft: Draft,
}

/// What one [`Index::ingest`] call read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ingested {
    pub bytes: u64,
    pub lines: u64,
    /// Lines that were not typed JSON records.
    pub unknown: u64,
    pub docs: u64,
    pub overlong: u32,
    /// The file was replaced or rewritten; its earlier docs were dropped.
    pub reset: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub docs: u64,
    pub sessions: u64,
    pub sources: u64,
    pub segments: u64,
    pub terms: u64,
    /// Bytes of the manifest and segments on disk.
    pub bytes: u64,
}

pub struct Index {
    dir: PathBuf,
    _lock: File,
    next_doc: u64,
    next_seg: u64,
    pub(crate) segs: Vec<Seg>,
    pub(crate) syms: Syms,
    sources: BTreeMap<String, SourceRec>,
    drops: BTreeMap<String, u64>,
    /// Dead docs by source symbol: ids below the value were dropped.
    dead: std::collections::HashMap<u32, u64>,
    usage: BTreeMap<String, ContextUsage>,
    pending: Vec<Pending>,
    pending_bytes: usize,
    dirty: bool,
    pub(crate) live_docs: u64,
    pub(crate) live_len: u64,
}

fn ctx(e: io::Error, what: &str, path: impl std::fmt::Display) -> io::Error {
    io::Error::new(e.kind(), format!("history {what} {path}: {e}"))
}

fn invalid(path: impl std::fmt::Display, msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("history {path}: {msg}"))
}

fn source_key(kind: SourceKind, path: &str) -> String {
    format!("{kind}:{path}")
}

impl Index {
    /// Opens or creates the index in `dir`. One writer at a time, enforced by a lock file.
    /// A corrupt manifest or segment is an `InvalidData` error; the index is derived, so the
    /// remedy is a rebuild into a fresh directory.
    pub fn open(dir: &Path) -> io::Result<Index> {
        fs::create_dir_all(dir).map_err(|e| ctx(e, "create", dir.display()))?;
        let lock_path = dir.join("lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| ctx(e, "open", lock_path.display()))?;
        lock.try_lock().map_err(|e| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("history {}: index is open elsewhere ({e})", dir.display()),
            )
        })?;
        let m = read_manifest(&dir.join(MANIFEST))?;
        let mut ix = Index {
            dir: dir.to_owned(),
            _lock: lock,
            next_doc: m.next_doc,
            next_seg: m.next_seg,
            segs: Vec::new(),
            syms: Syms::default(),
            sources: BTreeMap::new(),
            drops: BTreeMap::new(),
            dead: Default::default(),
            usage: BTreeMap::new(),
            pending: Vec::new(),
            pending_bytes: 0,
            dirty: false,
            live_docs: 0,
            live_len: 0,
        };
        for s in m.sources {
            ix.sources.insert(source_key(s.kind, &s.path), s);
        }
        for d in m.drops {
            ix.drop_source(d.source, d.below);
        }
        for u in m.usage {
            ix.usage.insert(u.session.clone(), u);
        }
        for r in &m.segments {
            let seg = ix.load(&r.file)?;
            ix.segs.push(seg);
        }
        ix.recount();
        Ok(ix)
    }

    fn load(&mut self, name: &str) -> io::Result<Seg> {
        let path = self.dir.join(name);
        let bytes = fs::read(&path).map_err(|e| ctx(e, "read", path.display()))?;
        let file = File::open(&path).map_err(|e| ctx(e, "open", path.display()))?;
        self.adopt(name, file, &bytes)
    }

    fn adopt(&mut self, name: &str, file: File, bytes: &[u8]) -> io::Result<Seg> {
        let d = segment::decode(bytes).map_err(|e| invalid(name, e))?;
        let map: Vec<u32> = d.strings.iter().map(|s| self.syms.intern(s)).collect();
        let g = |i: u32| if i == NONE { NONE } else { map[i as usize] };
        let docs = d
            .docs
            .into_iter()
            .map(|mut m| {
                m.session = g(m.session);
                m.sleeve = g(m.sleeve);
                m.source = g(m.source);
                m.tool = g(m.tool);
                for f in m.files.iter_mut() {
                    *f = g(*f);
                }
                m
            })
            .collect();
        Ok(Seg {
            name: name.to_owned(),
            file,
            docs,
            dict: d.dict,
            postings: d.postings,
            texts: d.texts,
            bytes: bytes.len() as u64,
        })
    }

    fn drop_source(&mut self, source: String, below: u64) {
        let sym = self.syms.intern(&source);
        self.dead.insert(sym, below);
        self.drops.insert(source, below);
    }

    pub(crate) fn live(&self, m: &Meta) -> bool {
        self.dead.get(&m.source).is_none_or(|&below| m.id >= below)
    }

    fn recount(&mut self) {
        let (mut n, mut len) = (0, 0);
        for s in &self.segs {
            for m in s.docs.iter().filter(|m| self.live(m)) {
                n += 1;
                len += u64::from(m.len);
            }
        }
        self.live_docs = n;
        self.live_len = len;
    }

    /// Reads `source` from its cursor to its current end (or `limit`) and queues the docs it
    /// yields. They become searchable at the next [`Index::commit`], which this call makes
    /// itself once enough text is pending.
    pub fn ingest(&mut self, source: &Source) -> io::Result<Ingested> {
        let path = source
            .path
            .to_str()
            .ok_or_else(|| invalid(source.path.display(), "path is not UTF-8"))?;
        let key = source_key(source.kind, path);
        let mut rec = self
            .sources
            .get(&key)
            .cloned()
            .unwrap_or_else(|| SourceRec {
                kind: source.kind,
                path: path.to_owned(),
                sleeve: source.sleeve.clone(),
                cursor: Cursor::default(),
                state: fresh_state(source),
            });
        rec.sleeve = source.sleeve.clone();
        let mut opened = tail::open(&source.path, &rec.cursor)?;
        let mut got = Ingested {
            reset: opened.reset,
            ..Ingested::default()
        };
        if opened.reset {
            self.drop_source(key.clone(), self.next_doc);
            rec.state = fresh_state(source);
            self.dirty = true;
        }
        if source.kind == SourceKind::Grok && rec.state.time_ms == 0 {
            rec.state.time_ms = grok_start(&source.path).unwrap_or(0);
        }
        let end = source.limit.unwrap_or(opened.len).min(opened.len);
        loop {
            let start = opened.start.offset;
            let limit = start.saturating_add(tail::WINDOW as u64).min(end);
            let mut items = Vec::new();
            let mut state = rec.state.clone();
            let mut queued: Vec<(u64, Draft)> = Vec::new();
            let mut usage: Vec<Item> = Vec::new();
            let tail = opened.read(Some(limit), |off, line| {
                got.lines = got.lines.saturating_add(1);
                if !extract::line(source.kind, line, &mut state, &mut items) {
                    got.unknown = got.unknown.saturating_add(1);
                }
                for it in items.drain(..) {
                    if source.skip.iter().any(|s| s == it.session()) {
                        continue;
                    }
                    match it {
                        Item::Doc(d) => queued.push((off, d)),
                        u => usage.push(u),
                    }
                }
            })?;
            self.next_doc
                .checked_add(queued.len() as u64)
                .ok_or_else(|| invalid(source.path.display(), "document ids exhausted"))?;
            got.bytes = got.bytes.saturating_add(tail.bytes);
            got.overlong = got.overlong.saturating_add(tail.overlong);
            got.docs = got.docs.saturating_add(queued.len() as u64);
            for u in usage {
                self.note_usage(u);
            }
            for (offset, draft) in queued {
                let u = self
                    .usage
                    .entry(draft.session.clone())
                    .or_insert_with(|| ContextUsage {
                        session: draft.session.clone(),
                        harness: draft.harness,
                        ..ContextUsage::default()
                    });
                u.turns = u.turns.max(draft.turn);
                let bytes = draft.files.iter().fold(
                    draft
                        .text
                        .len()
                        .saturating_add(draft.session.len())
                        .saturating_add(key.len())
                        .saturating_add(source.sleeve.as_ref().map_or(0, String::len))
                        .saturating_add(draft.tool.as_ref().map_or(0, String::len))
                        .saturating_add(128),
                    |n, f| n.saturating_add(f.len().saturating_add(24)),
                );
                self.pending_bytes = self.pending_bytes.saturating_add(bytes);
                self.pending.push(Pending {
                    id: self.next_doc,
                    offset,
                    source: key.clone(),
                    sleeve: source.sleeve.clone(),
                    draft,
                });
                self.next_doc += 1;
            }
            rec.cursor = tail.cursor;
            rec.state = state;
            self.dirty |= tail.bytes > 0;
            self.sources.insert(key.clone(), rec.clone());
            if self.pending_bytes >= flush_limit() {
                self.commit()?;
            }
            if opened.start.offset >= end || opened.start.offset == start {
                break;
            }
        }
        Ok(got)
    }

    fn note_usage(&mut self, item: Item) {
        match item {
            Item::Usage {
                session,
                harness,
                tokens,
                window,
                at_ms,
            } => {
                let u = self.usage.entry(session.clone()).or_default();
                u.session = session;
                u.harness = harness.or(u.harness);
                u.tokens = tokens;
                u.window = window.or(u.window);
                u.at_ms = at_ms;
            }
            Item::Compacted { session } => {
                let u = self.usage.entry(session.clone()).or_default();
                u.session = session;
                u.compactions = u.compactions.saturating_add(1);
            }
            Item::Doc(_) => {}
        }
    }

    /// Writes pending docs as a new segment, then the manifest (cursors, drops, usage) that
    /// makes it live, then merges small segments. A crash before the manifest rename leaves
    /// the previous state, and the sources are read again from their old cursors.
    pub fn commit(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            let docs: Vec<DocIn<'_>> = pending
                .iter()
                .map(|p| DocIn {
                    id: p.id,
                    time_ms: p.draft.time_ms,
                    offset: p.offset,
                    turn: p.draft.turn,
                    role: p.draft.role,
                    harness: p.draft.harness,
                    session: &p.draft.session,
                    sleeve: p.sleeve.as_deref(),
                    source: &p.source,
                    tool: p.draft.tool.as_deref(),
                    files: &p.draft.files,
                    text: &p.draft.text,
                })
                .collect();
            let seg = match inject_failure("segment").and_then(|()| self.write_segment(&docs)) {
                Ok(seg) => seg,
                Err(e) => {
                    self.pending = pending;
                    return Err(e);
                }
            };
            self.segs.push(seg);
            if let Err(e) = inject_failure("manifest").and_then(|()| self.write_manifest()) {
                self.segs.pop();
                self.pending = pending;
                return Err(e);
            }
            self.pending_bytes = 0;
        } else {
            self.write_manifest()?;
        }
        self.dirty = false;
        self.recount();
        self.merge()?;
        self.recount();
        Ok(())
    }

    pub fn has_uncommitted(&self) -> bool {
        self.dirty
    }

    fn write_segment(&mut self, docs: &[DocIn<'_>]) -> io::Result<Seg> {
        let bytes = segment::encode(docs);
        let name = format!("seg-{:08}", self.next_seg);
        self.next_seg = self
            .next_seg
            .checked_add(1)
            .ok_or_else(|| invalid(self.dir.display(), "segment ids exhausted"))?;
        let path = self.dir.join(&name);
        write_atomic(&self.dir, &path, &bytes)?;
        let file = File::open(&path).map_err(|e| ctx(e, "open", path.display()))?;
        self.adopt(&name, file, &bytes)
    }

    fn write_manifest(&self) -> io::Result<()> {
        let m = Manifest {
            version: VERSION,
            next_doc: self.next_doc,
            next_seg: self.next_seg,
            segments: self
                .segs
                .iter()
                .map(|s| SegRec {
                    file: s.name.clone(),
                    docs: s.docs.len() as u64,
                    bytes: s.bytes,
                })
                .collect(),
            sources: self.sources.values().cloned().collect(),
            drops: self
                .drops
                .iter()
                .map(|(s, &b)| DropRec {
                    source: s.clone(),
                    below: b,
                })
                .collect(),
            usage: self.usage.values().cloned().collect(),
        };
        let json = m.to_json().to_string();
        let text = format!("{MANIFEST_MAGIC} {:08x}\n{json}", crc32c(json.as_bytes()));
        write_atomic(&self.dir, &self.dir.join(MANIFEST), text.as_bytes())
    }

    /// While there are too many segments, merges the adjacent pair with the fewest docs
    /// (dropping dead docs), keeping doc ids ascending across segments.
    fn merge(&mut self) -> io::Result<()> {
        while self.segs.len() > MAX_SEGMENTS {
            let pick = (0..self.segs.len() - 1)
                .filter(|&i| self.segs[i].bytes + self.segs[i + 1].bytes <= MAX_MERGE)
                .min_by_key(|&i| self.segs[i].docs.len() + self.segs[i + 1].docs.len());
            let Some(i) = pick else {
                return Ok(());
            };
            let mut owned: Vec<(Meta, String)> = Vec::new();
            for s in &self.segs[i..i + 2] {
                let mut texts = vec![0u8; (s.texts.end - s.texts.start) as usize];
                s.file
                    .read_exact_at(&mut texts, s.texts.start)
                    .map_err(|e| ctx(e, "read texts", &s.name))?;
                for m in s.docs.iter().filter(|m| self.live(m)) {
                    let t =
                        &texts[m.text_off as usize..(m.text_off + u64::from(m.text_len)) as usize];
                    let t = String::from_utf8(t.to_vec())
                        .map_err(|_| invalid(&s.name, "text is not UTF-8"))?;
                    owned.push((m.clone(), t));
                }
            }
            let files: Vec<Vec<String>> = owned
                .iter()
                .map(|(m, _)| {
                    m.files
                        .iter()
                        .filter_map(|f| self.syms.name(*f).map(str::to_owned))
                        .collect()
                })
                .collect();
            let name = |id: u32| self.syms.name(id).unwrap_or_default();
            let docs: Vec<DocIn<'_>> = owned
                .iter()
                .zip(&files)
                .map(|((m, t), f)| DocIn {
                    id: m.id,
                    time_ms: m.time_ms,
                    offset: m.offset,
                    turn: m.turn,
                    role: m.role,
                    harness: m.harness,
                    session: name(m.session),
                    sleeve: self.syms.name(m.sleeve),
                    source: name(m.source),
                    tool: self.syms.name(m.tool),
                    files: f,
                    text: t,
                })
                .collect();
            let bytes = segment::encode(&docs);
            drop(docs);
            let seg_name = format!("seg-{:08}", self.next_seg);
            self.next_seg = self
                .next_seg
                .checked_add(1)
                .ok_or_else(|| invalid(self.dir.display(), "segment ids exhausted"))?;
            let path = self.dir.join(&seg_name);
            write_atomic(&self.dir, &path, &bytes)?;
            let file = File::open(&path).map_err(|e| ctx(e, "open", path.display()))?;
            let merged = self.adopt(&seg_name, file, &bytes)?;
            let old: Vec<Seg> = self.segs.splice(i..i + 2, [merged]).collect();
            if let Err(e) = self.write_manifest() {
                self.segs.splice(i..i + 1, old);
                return Err(e);
            }
            for s in old {
                let p = self.dir.join(&s.name);
                match fs::remove_file(&p) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(ctx(e, "remove merged", p.display())),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn doc(&self, seg: &Seg, m: &Meta) -> io::Result<Doc> {
        let source = self.syms.name(m.source).unwrap_or_default();
        let path = source.split_once(':').map_or(source, |(_, p)| p);
        Ok(Doc {
            id: DocId(m.id),
            session: self.syms.name(m.session).unwrap_or_default().to_owned(),
            harness: m.harness,
            sleeve: self.syms.name(m.sleeve).map(str::to_owned),
            turn: m.turn,
            role: m.role,
            time_ms: m.time_ms,
            tool: self.syms.name(m.tool).map(str::to_owned),
            files: m
                .files
                .iter()
                .filter_map(|f| self.syms.name(*f).map(str::to_owned))
                .collect(),
            text: seg.text(m)?,
            path: PathBuf::from(path),
            offset: m.offset,
        })
    }

    /// Committed docs: one by id, or a session's turn range in order.
    pub fn read(&self, sel: &Selector) -> io::Result<Vec<Doc>> {
        let mut out = Vec::new();
        match sel {
            Selector::Doc(DocId(id)) => {
                for s in &self.segs {
                    if let Ok(i) = s.docs.binary_search_by_key(id, |m| m.id)
                        && self.live(&s.docs[i])
                    {
                        out.push(self.doc(s, &s.docs[i])?);
                    }
                }
            }
            Selector::Turns { session, from, to } => {
                let Some(sym) = self.syms.get(session) else {
                    return Ok(out);
                };
                for s in &self.segs {
                    for m in &s.docs {
                        if m.session == sym && (*from..=*to).contains(&m.turn) && self.live(m) {
                            out.push(self.doc(s, m)?);
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// The latest context fill seen for `session`, including uncommitted ingests.
    pub fn context_usage(&self, session: &str) -> Option<ContextUsage> {
        self.usage.get(session).cloned()
    }

    /// Committed live docs; cheaper than [`Index::stats`].
    pub fn docs(&self) -> u64 {
        self.live_docs
    }

    pub fn stats(&self) -> Stats {
        let mut sessions = std::collections::HashSet::new();
        for s in &self.segs {
            for m in s.docs.iter().filter(|m| self.live(m)) {
                sessions.insert(m.session);
            }
        }
        let manifest = fs::metadata(self.dir.join(MANIFEST)).map_or(0, |m| m.len());
        Stats {
            docs: self.live_docs,
            sessions: sessions.len() as u64,
            sources: self.sources.len() as u64,
            segments: self.segs.len() as u64,
            terms: self.segs.iter().map(|s| s.dict.term_count() as u64).sum(),
            bytes: manifest + self.segs.iter().map(|s| s.bytes).sum::<u64>(),
        }
    }
}

fn flush_limit() -> usize {
    #[cfg(debug_assertions)]
    if let Some(bytes) = std::env::var("PT_TEST_HISTORY_FLUSH_BYTES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
    {
        return bytes;
    }
    FLUSH
}

fn inject_failure(phase: &str) -> io::Result<()> {
    #[cfg(debug_assertions)]
    {
        static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let armed = std::env::var_os("PT_TEST_HISTORY_FAIL_ARM")
            .is_none_or(|path| Path::new(&path).exists());
        if std::env::var("PT_TEST_HISTORY_FAIL_COMMIT").as_deref() == Ok(phase)
            && armed
            && !FAILED.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return Err(io::Error::other(format!(
                "injected history {phase} commit failure"
            )));
        }
    }
    let _ = phase;
    Ok(())
}

fn fresh_state(source: &Source) -> State {
    State {
        session: source.session.clone(),
        harness: extract::harness_of(source.kind),
        ..State::default()
    }
}

/// A Grok session's start time from the `summary.json` beside its chat history.
fn grok_start(path: &Path) -> Option<u64> {
    let file = crate::safe_open::file(&path.parent()?.join("summary.json")).ok()?;
    let mut text = String::new();
    file.take(1 << 20).read_to_string(&mut text).ok()?;
    let v = pt_json::parse(&text).ok()?;
    extract::parse_time(v["created_at"].as_str()?)
}

fn read_manifest(path: &Path) -> io::Result<Manifest> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Manifest::default()),
        Err(e) => return Err(ctx(e, "read", path.display())),
    };
    let bad = |msg: &str| invalid(path.display(), msg);
    let (head, json) = text.split_once('\n').ok_or_else(|| bad("no header line"))?;
    let crc = head
        .strip_prefix(MANIFEST_MAGIC)
        .and_then(|h| u32::from_str_radix(h.trim(), 16).ok())
        .ok_or_else(|| bad("bad header"))?;
    if crc32c(json.as_bytes()) != crc {
        return Err(bad("checksum mismatch"));
    }
    let m: Manifest = pt_json::from_str(json).map_err(|e| bad(&format!("manifest: {e}")))?;
    if m.version != VERSION {
        return Err(bad("unknown manifest version"));
    }
    Ok(m)
}

/// Writes `bytes` to `path` through a temp file, fsync and rename, then fsyncs the directory.
fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp).map_err(|e| ctx(e, "create", tmp.display()))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| ctx(e, "write", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| ctx(e, "rename", path.display()))?;
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| ctx(e, "sync", dir.display()))
}
