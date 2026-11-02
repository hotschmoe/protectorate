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
/// Live source-text bytes one sleeve may keep. Only that sleeve's sources stop or shed;
/// the others keep ingesting.
const MAX_INDEX: u64 = 1 << 30;
/// Live source-text bytes one source may keep. Past this, its oldest docs are dropped and
/// later lines are what stay.
const MAX_SOURCE: u64 = 1 << 30;
/// Docs one [`Index::read`] materializes. The hub's history read uses the same cap.
pub const READ_DOCS: usize = 200;
/// Source-text bytes one [`Index::read`] materializes before it reports more.
pub const READ_BYTES: usize = 8 << 20;

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
        /// Oldest docs were dropped, or this source was refused, because its sleeve or it
        /// reached its text budget. `doctor` and `hub info` report that.
        capped: bool = false,
        /// File length at which a capped source last made no progress. Zero when it is not stalled.
        stall_len: u64 = 0,
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
    /// This call is what first marked the source capped.
    pub capped: bool,
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
    /// Live source-text bytes per stored source key (`kind:path`), including pending docs.
    kept: BTreeMap<String, u64>,
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

/// What a document records as its source. The path is the part after the first colon.
fn source_key(kind: SourceKind, path: &str) -> String {
    format!("{kind}:{path}")
}

/// The cursor key. Two sleeves that name the same path do not share a cursor.
fn cursor_key(kind: SourceKind, path: &str, sleeve: Option<&str>) -> String {
    match sleeve {
        Some(s) => format!("{kind}:{s}:{path}"),
        None => format!("{kind}:{path}"),
    }
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
            kept: BTreeMap::new(),
            dead: Default::default(),
            usage: BTreeMap::new(),
            pending: Vec::new(),
            pending_bytes: 0,
            dirty: false,
            live_docs: 0,
            live_len: 0,
        };
        for s in m.sources {
            ix.sources
                .insert(cursor_key(s.kind, &s.path, s.sleeve.as_deref()), s);
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
        ix.sweep_segments()?;
        ix.recount();
        Ok(ix)
    }

    /// A crash after a segment is written, or before a merged segment is removed, leaves
    /// `seg-*` files the manifest does not name. They are not part of the index.
    fn sweep_segments(&self) -> io::Result<()> {
        let keep: std::collections::HashSet<&str> =
            self.segs.iter().map(|s| s.name.as_str()).collect();
        let rd = fs::read_dir(&self.dir).map_err(|e| ctx(e, "read", self.dir.display()))?;
        for ent in rd {
            let ent = ent.map_err(|e| ctx(e, "read", self.dir.display()))?;
            let name = ent.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.starts_with("seg-") || keep.contains(name) {
                continue;
            }
            let path = ent.path();
            let ft = ent
                .file_type()
                .map_err(|e| ctx(e, "stat", path.display()))?;
            if !ft.is_file() {
                continue;
            }
            fs::remove_file(&path).map_err(|e| ctx(e, "remove", path.display()))?;
        }
        Ok(())
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
        let mut kept: BTreeMap<String, u64> = BTreeMap::new();
        for s in &self.segs {
            for m in s.docs.iter().filter(|m| self.live(m)) {
                n += 1;
                len += u64::from(m.len);
                if let Some(src) = self.syms.name(m.source) {
                    *kept.entry(src.to_owned()).or_default() += u64::from(m.text_len);
                }
            }
        }
        for p in &self.pending {
            *kept.entry(p.source.clone()).or_default() += p.draft.text.len() as u64;
        }
        self.live_docs = n;
        self.live_len = len;
        self.kept = kept;
    }

    /// Reads `source` from its cursor to its current end (or `limit`) and queues the docs it
    /// yields. They become searchable at the next [`Index::commit`], which this call makes
    /// itself once enough text is pending.
    pub fn ingest(&mut self, source: &Source) -> io::Result<Ingested> {
        let path = source
            .path
            .to_str()
            .ok_or_else(|| invalid(source.path.display(), "path is not UTF-8"))?;
        let key = cursor_key(source.kind, path, source.sleeve.as_deref());
        let stored = source_key(source.kind, path);
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
                capped: false,
                stall_len: 0,
            });
        rec.sleeve = source.sleeve.clone();
        let mut opened = tail::open(&source.path, &rec.cursor)?;
        let mut got = Ingested {
            reset: opened.reset,
            ..Ingested::default()
        };
        if opened.reset {
            self.drop_source(stored.clone(), self.next_doc);
            self.discard_pending(&stored);
            self.kept.insert(stored.clone(), 0);
            rec.state = fresh_state(source);
            rec.cursor = opened.start.clone();
            rec.capped = false;
            rec.stall_len = 0;
            self.dirty = true;
        }
        if source.kind == SourceKind::Grok && rec.state.time_ms == 0 {
            rec.state.time_ms = grok_start(&source.path).unwrap_or(0);
        }
        let was_capped = rec.capped;
        self.sources.insert(key.clone(), rec.clone());
        // Another source of this sleeve already holds the budget. Leave this cursor where
        // it is, and do not read the same window again until the file grows.
        if self.blocked_by_others(&stored, source.sleeve.as_deref()) {
            if !rec.capped || rec.stall_len != opened.len {
                rec.capped = true;
                rec.stall_len = opened.len;
                got.capped = !was_capped;
                self.dirty = true;
            }
            self.sources.insert(key, rec);
            return Ok(got);
        }
        if rec.stall_len > 0
            && opened.len <= rec.stall_len
            && self.at_cap(&stored, source.sleeve.as_deref())
        {
            return Ok(got);
        }
        let end = source.limit.unwrap_or(opened.len).min(opened.len);
        let mut aged: Option<(Vec<(u64, u32)>, usize)> = None;
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
                let id = self.next_doc;
                let text_len = draft.text.len() as u64;
                let bytes = queued_bytes(&draft, &stored, source.sleeve.as_deref());
                self.pending_bytes = self.pending_bytes.saturating_add(bytes);
                *self.kept.entry(stored.clone()).or_default() += text_len;
                self.pending.push(Pending {
                    id,
                    offset,
                    source: stored.clone(),
                    sleeve: source.sleeve.clone(),
                    draft,
                });
                self.next_doc += 1;
                let kept_now = self.kept_of(&stored);
                if self.fit(&stored, source.sleeve.as_deref(), id, text_len, &mut aged) {
                    if self.kept_of(&stored) < kept_now {
                        note_capped(&mut rec, &mut got, was_capped);
                    }
                } else {
                    self.undo_pending(id, &stored, text_len, bytes);
                    got.docs = got.docs.saturating_sub(1);
                    note_capped(&mut rec, &mut got, was_capped);
                }
            }
            rec.cursor = tail.cursor;
            rec.state = state;
            if tail.bytes > 0 {
                rec.stall_len = 0;
            }
            self.dirty |= tail.bytes > 0 || got.capped;
            self.sources.insert(key.clone(), rec.clone());
            if self.pending_bytes >= flush_limit() {
                self.commit()?;
                aged = None;
            }
            if opened.start.offset == start
                && opened.len > start
                && self.at_cap(&stored, source.sleeve.as_deref())
            {
                rec.stall_len = opened.len;
                note_capped(&mut rec, &mut got, was_capped);
                self.dirty = true;
                self.sources.insert(key, rec);
                return Ok(got);
            }
            if opened.start.offset >= end || opened.start.offset == start {
                break;
            }
        }
        self.sources.insert(key, rec);
        Ok(got)
    }

    /// True when other sources of `sleeve` already hold its whole budget, so dropping this
    /// source's own docs still could not free a byte.
    fn blocked_by_others(&self, stored: &str, sleeve: Option<&str>) -> bool {
        let Some(sleeve) = sleeve else {
            return false;
        };
        let mine = self.kept_of(stored);
        self.sleeve_kept(sleeve).saturating_sub(mine) >= MAX_INDEX
    }

    fn at_cap(&self, stored: &str, sleeve: Option<&str>) -> bool {
        self.kept_of(stored) >= MAX_SOURCE
            || sleeve.is_some_and(|s| self.sleeve_kept(s) >= MAX_INDEX)
    }

    fn kept_of(&self, stored: &str) -> u64 {
        self.kept.get(stored).copied().unwrap_or(0)
    }

    fn sleeve_kept(&self, sleeve: &str) -> u64 {
        let mut seen = std::collections::HashSet::new();
        let mut n = 0u64;
        for s in self.sources.values() {
            if s.sleeve.as_deref() != Some(sleeve) {
                continue;
            }
            let key = source_key(s.kind, &s.path);
            if seen.insert(key.clone()) {
                n = n.saturating_add(self.kept_of(&key));
            }
        }
        n
    }

    /// Drops older docs of `stored` until it and its sleeve can hold `keep`. False when
    /// `keep` itself cannot fit; nothing is dropped in that case.
    fn fit(
        &mut self,
        stored: &str,
        sleeve: Option<&str>,
        keep: u64,
        need: u64,
        aged: &mut Option<(Vec<(u64, u32)>, usize)>,
    ) -> bool {
        let mine = self.kept_of(stored);
        let others = sleeve
            .map(|s| self.sleeve_kept(s).saturating_sub(mine))
            .unwrap_or(0);
        let limit = MAX_SOURCE.min(MAX_INDEX.saturating_sub(others.min(MAX_INDEX)));
        if need > limit {
            return false;
        }
        while self.kept_of(stored) > limit {
            if !self.drop_oldest_except(stored, keep, aged) {
                return false;
            }
        }
        true
    }

    fn drop_oldest_except(
        &mut self,
        stored: &str,
        keep: u64,
        aged: &mut Option<(Vec<(u64, u32)>, usize)>,
    ) -> bool {
        if aged.is_none() {
            *aged = Some((self.aged_docs(stored), 0));
        }
        let (docs, at) = aged.as_mut().unwrap();
        while *at < docs.len() {
            let (id, len) = docs[*at];
            *at += 1;
            if id == keep || id < self.drops.get(stored).copied().unwrap_or(0) {
                continue;
            }
            self.drop_source(stored.to_owned(), id.saturating_add(1));
            self.sub_kept(stored, u64::from(len));
            return true;
        }
        let Some(i) = self
            .pending
            .iter()
            .position(|p| p.source == stored && p.id != keep)
        else {
            return false;
        };
        let p = self.pending.remove(i);
        let bytes = queued_bytes(&p.draft, stored, p.sleeve.as_deref());
        self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
        self.sub_kept(stored, p.draft.text.len() as u64);
        true
    }

    fn aged_docs(&self, stored: &str) -> Vec<(u64, u32)> {
        let Some(sym) = self.syms.get(stored) else {
            return Vec::new();
        };
        let mut docs = Vec::new();
        for s in &self.segs {
            for m in &s.docs {
                if m.source == sym && self.live(m) {
                    docs.push((m.id, m.text_len));
                }
            }
        }
        docs.sort_by_key(|(id, _)| *id);
        docs
    }

    fn undo_pending(&mut self, id: u64, stored: &str, text_len: u64, bytes: usize) {
        let Some(i) = self.pending.iter().position(|p| p.id == id) else {
            return;
        };
        self.pending.remove(i);
        self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
        self.sub_kept(stored, text_len);
        if self.next_doc == id.saturating_add(1) {
            self.next_doc = id;
        }
    }

    fn discard_pending(&mut self, stored: &str) {
        let n = self.pending.len();
        self.pending.retain(|p| p.source != stored);
        if self.pending.len() == n {
            return;
        }
        self.pending_bytes = self
            .pending
            .iter()
            .map(|p| queued_bytes(&p.draft, &p.source, p.sleeve.as_deref()))
            .sum();
    }

    fn sub_kept(&mut self, stored: &str, n: u64) {
        if let Some(k) = self.kept.get_mut(stored) {
            *k = k.saturating_sub(n);
        }
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
            files: extract::take_files(m.files.iter().filter_map(|f| self.syms.name(*f))),
            text: seg.text(m)?,
            path: PathBuf::from(path),
            offset: m.offset,
        })
    }

    /// Committed docs: one by id, or a session's turn range in order.
    ///
    /// `sleeves` is the scope applied before any text is copied: `None` is every sleeve,
    /// `Some` is those ids only (an empty list matches nothing). The read stops at `limit`
    /// docs and at [`READ_BYTES`] of source text. The bool is whether a later in-scope doc
    /// was left out.
    pub fn read(
        &self,
        sel: &Selector,
        sleeves: Option<&[String]>,
        limit: usize,
    ) -> io::Result<(Vec<Doc>, bool)> {
        let sleeve_syms: Vec<u32> = match sleeves {
            None => Vec::new(),
            Some(ids) => ids.iter().filter_map(|s| self.syms.get(s)).collect(),
        };
        if sleeves.is_some() && sleeve_syms.is_empty() {
            return Ok((Vec::new(), false));
        }
        let allow =
            |m: &Meta| self.live(m) && sleeves.is_none_or(|_| sleeve_syms.contains(&m.sleeve));
        let limit = limit.min(READ_DOCS);
        let mut picked: Vec<(&Seg, &Meta)> = Vec::new();
        let mut bytes = 0usize;
        let mut more = false;
        match sel {
            Selector::Doc(DocId(id)) => {
                for s in &self.segs {
                    if let Ok(i) = s.docs.binary_search_by_key(id, |m| m.id) {
                        let m = &s.docs[i];
                        if allow(m) {
                            picked.push((s, m));
                            break;
                        }
                    }
                }
            }
            Selector::Turns { session, from, to } => {
                let Some(sym) = self.syms.get(session) else {
                    return Ok((Vec::new(), false));
                };
                for s in &self.segs {
                    for m in &s.docs {
                        if m.session != sym || !(*from..=*to).contains(&m.turn) || !allow(m) {
                            continue;
                        }
                        let w = m.text_len as usize;
                        if picked.len() >= limit
                            || (!picked.is_empty() && bytes.saturating_add(w) > READ_BYTES)
                        {
                            more = true;
                            break;
                        }
                        bytes = bytes.saturating_add(w);
                        picked.push((s, m));
                    }
                    if more {
                        break;
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(picked.len());
        for (s, m) in picked {
            out.push(self.doc(s, m)?);
        }
        Ok((out, more))
    }

    /// The latest context fill seen for `session`, including uncommitted ingests.
    pub fn context_usage(&self, session: &str) -> Option<ContextUsage> {
        self.usage.get(session).cloned()
    }

    /// Committed live docs; cheaper than [`Index::stats`].
    pub fn docs(&self) -> u64 {
        self.live_docs
    }

    /// A source has hit its own text budget or its sleeve's, and is keeping its newest lines
    /// or waiting.
    pub fn capped(&self) -> bool {
        self.sources.values().any(|s| s.capped)
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

fn note_capped(rec: &mut SourceRec, got: &mut Ingested, was_capped: bool) {
    rec.capped = true;
    got.capped = !was_capped;
}

fn queued_bytes(draft: &Draft, stored: &str, sleeve: Option<&str>) -> usize {
    draft.files.iter().fold(
        draft
            .text
            .len()
            .saturating_add(draft.session.len())
            .saturating_add(stored.len())
            .saturating_add(sleeve.map_or(0, str::len))
            .saturating_add(draft.tool.as_ref().map_or(0, String::len))
            .saturating_add(128),
        |n, f| n.saturating_add(f.len().saturating_add(24)),
    )
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
