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
/// 2: the cursor, the drop watermark and the budget share one key (`kind:sleeve:path`).
const VERSION: u32 = 2;
/// Pending text that triggers a commit after a read window.
const FLUSH: usize = 96 << 20;
/// More live segments than this start merges.
const MAX_SEGMENTS: usize = 8;
/// Merges never build a segment larger than this.
const MAX_MERGE: u64 = 1 << 30;
/// Live indexed bytes one sleeve may keep: each doc's source text plus its per-doc
/// overhead. Past this, the oldest docs of that sleeve are dropped, whichever of its
/// sources they came from. Other sleeves keep ingesting.
const MAX_INDEX: u64 = 1 << 30;
/// Live indexed bytes one source may keep, on the same unit as [`MAX_INDEX`]. Past this,
/// that source's own oldest docs are dropped and its later lines stay.
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
        /// This source's sleeve (or the source, when it has no sleeve) is at its text budget.
        /// Cleared once it is back under the budget. `doctor` and `hub info` name those sleeves.
        capped: bool = false,
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
    /// Live indexed bytes per stored source key, including pending docs. Each doc
    /// contributes its source text plus the per-doc overhead in [`queued_bytes`].
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

/// Cursor, drop watermark and budget. Two sleeves that name the same path do not share one.
fn source_key(kind: SourceKind, path: &str, sleeve: Option<&str>) -> String {
    match sleeve {
        Some(s) => format!("{kind}:{s}:{path}"),
        None => format!("{kind}:{path}"),
    }
}

/// The path stored in a source key. `sleeve` is the doc's own sleeve, when it has one.
fn source_path<'a>(source: &'a str, sleeve: Option<&str>) -> &'a str {
    let Some((_, rest)) = source.split_once(':') else {
        return source;
    };
    match sleeve {
        Some(s) => rest
            .strip_prefix(s)
            .and_then(|r| r.strip_prefix(':'))
            .unwrap_or(rest),
        None => rest,
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
        let manifest_path = dir.join(MANIFEST);
        let missing_manifest = !manifest_path.is_file();
        let m = read_manifest(&manifest_path)?;
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
                .insert(source_key(s.kind, &s.path, s.sleeve.as_deref()), s);
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
        let removed = ix.sweep_segments()?;
        if missing_manifest && removed > 0 {
            eprintln!(
                "warning: history: no manifest in {}; removed {removed} segment file(s)",
                dir.display()
            );
        }
        ix.reclaim()?;
        ix.recount();
        Ok(ix)
    }

    /// A crash after a segment is written, or before a merged segment is removed, leaves
    /// `seg-*` files the manifest does not name. They are not part of the index. The count
    /// is how many were removed; the caller warns when that happens with no manifest.
    fn sweep_segments(&self) -> io::Result<usize> {
        let keep: std::collections::HashSet<&str> =
            self.segs.iter().map(|s| s.name.as_str()).collect();
        let rd = fs::read_dir(&self.dir).map_err(|e| ctx(e, "read", self.dir.display()))?;
        let mut removed = 0usize;
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
            removed += 1;
        }
        Ok(removed)
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

    /// Drops every doc from `path` and forgets its cursor. A renamed native log calls this
    /// on the path it is leaving.
    pub fn forget(&mut self, kind: SourceKind, path: &Path, sleeve: Option<&str>) {
        let Some(path) = path.to_str() else {
            return;
        };
        let stored = source_key(kind, path, sleeve);
        self.drop_source(stored.clone(), self.next_doc);
        self.discard_pending(&stored);
        self.kept.insert(stored.clone(), 0);
        self.sources.remove(&stored);
        self.dirty = true;
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
                    *kept.entry(src.to_owned()).or_default() += self.doc_charge(m);
                }
            }
        }
        for p in &self.pending {
            *kept.entry(p.source.clone()).or_default() +=
                queued_bytes(&p.draft, &p.source, p.sleeve.as_deref()) as u64;
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
        let stored = source_key(source.kind, path, source.sleeve.as_deref());
        let key = stored.clone();
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
            self.dirty = true;
        }
        if source.kind == SourceKind::Grok && rec.state.time_ms == 0 {
            rec.state.time_ms = grok_start(&source.path).unwrap_or(0);
        }
        let was_capped = rec.capped;
        self.sources.insert(key.clone(), rec.clone());
        // One sum of this sleeve's sources for the whole call. Later docs adjust it.
        let mut shed = Shed {
            aged: None,
            sleeve: None,
            total: source.sleeve.as_deref().map(|s| self.sleeve_kept(s)),
        };
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
                let id = self.next_doc;
                let bytes = queued_bytes(&draft, &stored, source.sleeve.as_deref());
                let charge = bytes as u64;
                self.pending_bytes = self.pending_bytes.saturating_add(bytes);
                *self.kept.entry(stored.clone()).or_default() += charge;
                if let Some(t) = shed.total.as_mut() {
                    *t = t.saturating_add(charge);
                }
                self.pending.push(Pending {
                    id,
                    offset,
                    source: stored.clone(),
                    sleeve: source.sleeve.clone(),
                    draft,
                });
                self.next_doc += 1;
                if !self.fit(&stored, source.sleeve.as_deref(), id, charge, &mut shed) {
                    self.undo_pending(id, &stored, charge, bytes, &mut shed.total);
                    got.docs = got.docs.saturating_sub(1);
                }
            }
            rec.cursor = tail.cursor;
            rec.state = state;
            self.dirty |= tail.bytes > 0;
            self.sources.insert(key.clone(), rec.clone());
            if self.pending_bytes >= flush_limit() {
                self.commit()?;
                shed.aged = None;
                shed.sleeve = None;
                if let Some(s) = source.sleeve.as_deref() {
                    shed.total = Some(self.sleeve_kept(s));
                }
            }
            if opened.start.offset >= end || opened.start.offset == start {
                break;
            }
        }
        self.sources.insert(key, rec);
        self.settle_capped(&stored, source.sleeve.as_deref(), was_capped, &mut got);
        Ok(got)
    }

    fn at_cap(&self, stored: &str, sleeve_total: Option<u64>) -> bool {
        self.kept_of(stored) >= MAX_SOURCE || sleeve_total.is_some_and(|n| n >= MAX_INDEX)
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
            let key = source_key(s.kind, &s.path, s.sleeve.as_deref());
            if seen.insert(key.clone()) {
                n = n.saturating_add(self.kept_of(&key));
            }
        }
        n
    }

    /// Drops older docs until `stored` is within its own budget and its sleeve is within
    /// the sleeve budget. Sleeve eviction takes the oldest doc of any of that sleeve's
    /// sources. False when `keep` itself cannot fit.
    fn fit(
        &mut self,
        stored: &str,
        sleeve: Option<&str>,
        keep: u64,
        need: u64,
        shed: &mut Shed,
    ) -> bool {
        if need > MAX_SOURCE || shed.total.is_some_and(|_| need > MAX_INDEX) {
            return false;
        }
        while self.kept_of(stored) > MAX_SOURCE {
            if !self.drop_oldest_except(stored, keep, &mut shed.aged, &mut shed.total) {
                return false;
            }
        }
        if let Some(sleeve) = sleeve {
            while shed.total.is_some_and(|n| n > MAX_INDEX) {
                if !self.evict_oldest(sleeve, keep, &mut shed.sleeve, &mut shed.total) {
                    return false;
                }
            }
        }
        true
    }

    /// Drops the oldest live doc of `sleeve` other than `keep`.
    fn evict_oldest(
        &mut self,
        sleeve: &str,
        keep: u64,
        aged: &mut Option<SleeveWalk>,
        sleeve_total: &mut Option<u64>,
    ) -> bool {
        if aged.is_none() {
            *aged = Some(SleeveWalk {
                docs: self.sleeve_docs(sleeve),
                at: 0,
            });
        }
        let walk = aged.as_mut().unwrap();
        while walk.at < walk.docs.len() {
            let doc = walk.docs[walk.at].clone();
            walk.at += 1;
            let (id, len, source) = (doc.id, doc.len, doc.source);
            if id == keep {
                continue;
            }
            if let Some(i) = self.pending.iter().position(|p| p.id == id) {
                let p = self.pending.remove(i);
                let bytes = queued_bytes(&p.draft, &p.source, p.sleeve.as_deref());
                self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
                self.sub_kept(&p.source, bytes as u64);
                sub_charge(sleeve_total, bytes as u64);
                return true;
            }
            let Some(sym) = self.syms.get(&source) else {
                continue;
            };
            let live = self.segs.iter().any(|s| {
                s.docs
                    .iter()
                    .any(|m| m.id == id && m.source == sym && self.live(m))
            });
            if !live {
                continue;
            }
            self.drop_source(source.clone(), id.saturating_add(1));
            self.sub_kept(&source, len);
            sub_charge(sleeve_total, len);
            return true;
        }
        false
    }

    fn sleeve_docs(&self, sleeve: &str) -> Vec<SleeveDoc> {
        let mut docs = Vec::new();
        if let Some(sym) = self.syms.get(sleeve) {
            for s in &self.segs {
                for m in &s.docs {
                    if m.sleeve == sym
                        && self.live(m)
                        && let Some(src) = self.syms.name(m.source)
                    {
                        docs.push(SleeveDoc {
                            id: m.id,
                            len: self.doc_charge(m),
                            source: src.to_owned(),
                        });
                    }
                }
            }
        }
        for p in &self.pending {
            if p.sleeve.as_deref() == Some(sleeve) {
                docs.push(SleeveDoc {
                    id: p.id,
                    len: queued_bytes(&p.draft, &p.source, p.sleeve.as_deref()) as u64,
                    source: p.source.clone(),
                });
            }
        }
        docs.sort_by_key(|d| d.id);
        docs
    }

    fn drop_oldest_except(
        &mut self,
        stored: &str,
        keep: u64,
        aged: &mut Option<(Vec<(u64, u64)>, usize)>,
        sleeve_total: &mut Option<u64>,
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
            self.sub_kept(stored, len);
            sub_charge(sleeve_total, len);
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
        self.sub_kept(stored, bytes as u64);
        sub_charge(sleeve_total, bytes as u64);
        true
    }

    fn aged_docs(&self, stored: &str) -> Vec<(u64, u64)> {
        let Some(sym) = self.syms.get(stored) else {
            return Vec::new();
        };
        let mut docs = Vec::new();
        for s in &self.segs {
            for m in &s.docs {
                if m.source == sym && self.live(m) {
                    docs.push((m.id, self.doc_charge(m)));
                }
            }
        }
        docs.sort_by_key(|(id, _)| *id);
        docs
    }

    fn undo_pending(
        &mut self,
        id: u64,
        stored: &str,
        charge: u64,
        bytes: usize,
        sleeve_total: &mut Option<u64>,
    ) {
        let Some(i) = self.pending.iter().position(|p| p.id == id) else {
            return;
        };
        self.pending.remove(i);
        self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
        self.sub_kept(stored, charge);
        sub_charge(sleeve_total, charge);
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

    /// Indexed bytes of one committed doc: source text plus the same per-doc overhead as
    /// [`queued_bytes`].
    fn doc_charge(&self, m: &Meta) -> u64 {
        let n = |id: u32| self.syms.name(id).map_or(0, str::len) as u64;
        let mut c = u64::from(m.text_len)
            .saturating_add(n(m.session))
            .saturating_add(n(m.source))
            .saturating_add(n(m.sleeve))
            .saturating_add(n(m.tool))
            .saturating_add(128);
        for f in &m.files {
            c = c.saturating_add(n(*f)).saturating_add(24);
        }
        c
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
    /// makes it live, then merges small segments. An unchanged index does not rewrite the
    /// manifest. A crash before the manifest rename leaves the previous state, and the
    /// sources are read again from their old cursors. A failed attempt removes the segment
    /// files it created.
    pub fn commit(&mut self) -> io::Result<()> {
        if self.pending.is_empty() && !self.dirty {
            return Ok(());
        }
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
                if let Some(seg) = self.segs.pop() {
                    self.drop_unreferenced([seg]);
                }
                self.pending = pending;
                return Err(e);
            }
            self.pending_bytes = 0;
        } else {
            self.write_manifest()?;
        }
        self.dirty = false;
        self.reclaim()?;
        self.recount();
        self.merge()?;
        self.recount();
        Ok(())
    }

    pub fn has_uncommitted(&self) -> bool {
        self.dirty
    }

    fn write_segment(&mut self, docs: &[DocIn<'_>]) -> io::Result<Seg> {
        self.write_new_segment(&segment::encode(docs))
    }

    /// Writes `bytes` as the next segment. On failure the file is removed and the segment
    /// number is reused, so a retry does not leave an orphan.
    fn write_new_segment(&mut self, bytes: &[u8]) -> io::Result<Seg> {
        let n = self.next_seg;
        self.next_seg = n
            .checked_add(1)
            .ok_or_else(|| invalid(self.dir.display(), "segment ids exhausted"))?;
        let name = format!("seg-{n:08}");
        let path = self.dir.join(&name);
        let built = (|| {
            write_atomic(&self.dir, &path, bytes)?;
            let file = File::open(&path).map_err(|e| ctx(e, "open", path.display()))?;
            self.adopt(&name, file, bytes)
        })();
        if built.is_err() {
            self.next_seg = n;
            let _ = fs::remove_file(&path);
        }
        built
    }

    /// Drops segment files this attempt created that the manifest does not name, and rewinds
    /// the segment counter when those numbers are still the newest.
    fn drop_unreferenced(&mut self, segs: impl IntoIterator<Item = Seg>) {
        let mut segs: Vec<Seg> = segs.into_iter().collect();
        segs.sort_by(|a, b| b.name.cmp(&a.name));
        for seg in segs {
            if manifest_names(&self.dir, &seg.name) {
                continue;
            }
            if let Some(n) = seg_number(&seg.name)
                && self.next_seg == n.saturating_add(1)
            {
                self.next_seg = n;
            }
            let _ = fs::remove_file(self.dir.join(&seg.name));
        }
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

    /// Removes a segment whose docs are all dead, and rewrites one that is at least half
    /// dead, in one manifest commit. New segment files are written first. The old files
    /// are removed only after that manifest is durable. A crash before the rename keeps
    /// the previous manifest; the unreferenced files are swept on the next open.
    fn reclaim(&mut self) -> io::Result<()> {
        let mut plan: Vec<(usize, bool)> = Vec::new();
        for (i, s) in self.segs.iter().enumerate() {
            let n = s.docs.len();
            let dead = s.docs.iter().filter(|m| !self.live(m)).count();
            if n == 0 || dead == n {
                plan.push((i, false));
            } else if dead >= n - dead {
                plan.push((i, true));
            }
        }
        if plan.is_empty() {
            return Ok(());
        }
        let mut fresh = std::collections::BTreeMap::new();
        for &(i, rewrite) in &plan {
            if rewrite {
                match self.write_live_segment(i) {
                    Ok(seg) => {
                        fresh.insert(i, seg);
                    }
                    Err(e) => {
                        self.drop_unreferenced(fresh.into_values());
                        return Err(e);
                    }
                }
            }
        }
        // High indexes first, so a removal does not shift one that is still planned.
        enum Undo {
            Insert(usize, Seg),
            Put(usize, Seg),
        }
        let mut undo = Vec::new();
        for (i, rewrite) in plan.into_iter().rev() {
            if rewrite {
                let neu = fresh.remove(&i).expect("rewritten segment");
                let old = std::mem::replace(&mut self.segs[i], neu);
                undo.push(Undo::Put(i, old));
            } else {
                undo.push(Undo::Insert(i, self.segs.remove(i)));
            }
        }
        if let Err(e) = self.write_manifest() {
            let mut neu = Vec::new();
            for u in undo.into_iter().rev() {
                match u {
                    Undo::Insert(i, seg) => self.segs.insert(i, seg),
                    Undo::Put(i, seg) => neu.push(std::mem::replace(&mut self.segs[i], seg)),
                }
            }
            self.drop_unreferenced(neu);
            return Err(e);
        }
        for u in undo {
            let seg = match u {
                Undo::Insert(_, seg) | Undo::Put(_, seg) => seg,
            };
            let p = self.dir.join(&seg.name);
            match fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(ctx(e, "remove dead", p.display())),
            }
        }
        Ok(())
    }

    /// Writes a new segment containing the live docs of `index`, in their original order.
    fn write_live_segment(&mut self, index: usize) -> io::Result<Seg> {
        let seg = &self.segs[index];
        let mut owned: Vec<(Meta, String)> = Vec::new();
        for m in &seg.docs {
            if self.live(m) {
                owned.push((m.clone(), seg.text(m)?));
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
        let bytes = {
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
                    session: self.syms.name(m.session).unwrap_or_default(),
                    sleeve: self.syms.name(m.sleeve),
                    source: self.syms.name(m.source).unwrap_or_default(),
                    tool: self.syms.name(m.tool),
                    files: f,
                    text: t,
                })
                .collect();
            segment::encode(&docs)
        };
        self.write_new_segment(&bytes)
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
            let merged = self.write_new_segment(&bytes)?;
            let old: Vec<Seg> = self.segs.splice(i..i + 2, [merged]).collect();
            if let Err(e) = self.write_manifest() {
                let neu: Vec<Seg> = self.segs.splice(i..i + 1, old).collect();
                self.drop_unreferenced(neu);
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
        let sleeve = self.syms.name(m.sleeve);
        let path = source_path(source, sleeve);
        Ok(Doc {
            id: DocId(m.id),
            session: self.syms.name(m.session).unwrap_or_default().to_owned(),
            harness: m.harness,
            sleeve: sleeve.map(str::to_owned),
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
            Selector::Turns {
                session,
                from,
                to,
                after,
            } => {
                let Some(sym) = self.syms.get(session) else {
                    return Ok((Vec::new(), false));
                };
                for s in &self.segs {
                    for m in &s.docs {
                        if m.session != sym
                            || !(*from..=*to).contains(&m.turn)
                            || after.is_some_and(|id| m.id <= id)
                            || !allow(m)
                        {
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

    /// Sleeve ids whose live indexed bytes are at the sleeve budget. Empty when every sleeve
    /// is under it. Readers of the hub copy this set out so `doctor` does not take the index lock.
    pub fn capped_sleeves(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        for s in self.sources.values() {
            if let Some(sleeve) = &s.sleeve {
                seen.insert(sleeve.clone());
            }
        }
        seen.into_iter()
            .filter(|sleeve| self.sleeve_kept(sleeve) >= MAX_INDEX)
            .collect()
    }

    /// Records whether `stored` ended this ingest at its budget, and the same for every
    /// source of its sleeve. A sleeve back under the budget loses the flag.
    fn settle_capped(&mut self, stored: &str, sleeve: Option<&str>, was: bool, got: &mut Ingested) {
        let over = self.at_cap(stored, sleeve.map(|s| self.sleeve_kept(s)));
        if over && !was {
            got.capped = true;
        }
        let mut changed = false;
        if let Some(sleeve) = sleeve {
            for s in self.sources.values_mut() {
                if s.sleeve.as_deref() == Some(sleeve) && s.capped != over {
                    s.capped = over;
                    changed = true;
                }
            }
        } else if let Some(s) = self.sources.get_mut(stored)
            && s.capped != over
        {
            s.capped = over;
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
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

#[derive(Clone)]
struct SleeveDoc {
    id: u64,
    len: u64,
    source: String,
}

struct SleeveWalk {
    docs: Vec<SleeveDoc>,
    at: usize,
}

/// Oldest-doc cursors and the sleeve total for one ingest call.
struct Shed {
    aged: Option<(Vec<(u64, u64)>, usize)>,
    sleeve: Option<SleeveWalk>,
    total: Option<u64>,
}

fn sub_charge(total: &mut Option<u64>, n: u64) {
    if let Some(t) = total.as_mut() {
        *t = t.saturating_sub(n);
    }
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
/// A failure before the rename removes the temp file.
fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let wrote = (|| -> io::Result<()> {
        let mut f = File::create(&tmp).map_err(|e| ctx(e, "create", tmp.display()))?;
        f.write_all(bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| ctx(e, "write", tmp.display()))?;
        fs::rename(&tmp, path).map_err(|e| ctx(e, "rename", path.display()))?;
        Ok(())
    })();
    if let Err(e) = wrote {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| ctx(e, "sync", dir.display()))
}

fn seg_number(name: &str) -> Option<u64> {
    name.strip_prefix("seg-")?.parse().ok()
}

fn manifest_names(dir: &Path, name: &str) -> bool {
    fs::read_to_string(dir.join(MANIFEST))
        .ok()
        .is_some_and(|text| text.contains(&format!("\"{name}\"")))
}
