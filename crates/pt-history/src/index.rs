//! The index directory: manifest, segments, ingest, commit and merges.

use std::collections::{BTreeMap, BTreeSet};
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
/// Live indexed bytes one sleeve may keep, and one source may keep, when the caller
/// does not pass another budget. Each doc counts its source text plus its per-doc
/// overhead. The hub reads `history_budget_bytes` once at start and passes it here.
pub const DEFAULT_BUDGET: u64 = 1 << 30;
/// Smallest `history_budget_bytes` the hub accepts. Exactly this value is allowed.
/// A configured 0 is not the default.
pub const MIN_BUDGET: u64 = 1 << 20;
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
        /// Budget last committed. 0 is a manifest from before this field.
        budget: u64 = 0,
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
        /// Eviction has dropped a doc from this source's sleeve (or from the source, when
        /// it has no sleeve). It stays set under the same budget. A later open clears it
        /// when the total is under a raised budget. `doctor` and `hub info` name the sleeves.
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

/// One live doc in a sleeve's id order. `seg` is the segment index, or `None` while pending.
struct Loc {
    source: String,
    charge: u64,
    seg: Option<usize>,
}

/// Live docs of one sleeve, oldest id first, plus their byte total.
#[derive(Default)]
struct SleeveDocs {
    by_id: BTreeMap<u64, Loc>,
    total: u64,
}

/// Live docs of one source, oldest id first. The value is the doc's charge.
#[derive(Default)]
struct SourceDocs {
    by_id: BTreeMap<u64, u64>,
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
    /// Live indexed bytes one sleeve may keep, and one source may keep.
    budget: u64,
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
    /// Live docs per sleeve, kept current as docs are added and removed.
    sleeves: BTreeMap<String, SleeveDocs>,
    /// Live docs per source, kept current as docs are added and removed.
    by_source: BTreeMap<String, SourceDocs>,
    /// Source keys of each sleeve, so a flag update does not scan every source.
    members: BTreeMap<String, BTreeSet<String>>,
    /// Sleeves whose capped flag is set.
    capped_ids: BTreeSet<String>,
    /// Dead docs by source symbol: ids below the value were dropped.
    dead: std::collections::HashMap<u32, u64>,
    usage: BTreeMap<String, ContextUsage>,
    pending: BTreeMap<u64, Pending>,
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
    /// Opens or creates the index in `dir`. `budget` is the live-byte cap for one sleeve
    /// and for one source (the hub's `history_budget_bytes`). One writer at a time, enforced
    /// by a lock file. A corrupt manifest or segment is an `InvalidData` error; the index is
    /// derived, so the remedy is a rebuild into a fresh directory.
    pub fn open(dir: &Path, budget: u64) -> io::Result<Index> {
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
        let stored_budget = m.budget;
        let mut ix = Index {
            dir: dir.to_owned(),
            budget,
            _lock: lock,
            next_doc: m.next_doc,
            next_seg: m.next_seg,
            segs: Vec::new(),
            syms: Syms::default(),
            sources: BTreeMap::new(),
            drops: BTreeMap::new(),
            kept: BTreeMap::new(),
            sleeves: BTreeMap::new(),
            by_source: BTreeMap::new(),
            members: BTreeMap::new(),
            capped_ids: BTreeSet::new(),
            dead: Default::default(),
            usage: BTreeMap::new(),
            pending: BTreeMap::new(),
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
        // A full disk here must not fail the open. The hub treats any other open
        // error as a corrupt index and rebuilds, which drops history whose logs are gone.
        if let Err(e) = ix.reclaim() {
            eprintln!("warning: history: reclaim failed in {}: {e}", dir.display());
        }
        ix.recount();
        ix.bind_members();
        ix.release_if_budget_raised(stored_budget);
        ix.rebuild_capped_ids();
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
        self.drops.insert(source.clone(), below);
        // Pending docs stay for `discard_pending`. Older live docs of this source die
        // with the watermark, so their charges leave the totals here.
        let gone: Vec<(u64, u64)> = self
            .by_source
            .get(&source)
            .map(|d| {
                d.by_id
                    .range(..below)
                    .map(|(&id, &charge)| (id, charge))
                    .collect()
            })
            .unwrap_or_default();
        let sleeve = self.sources.get(&source).and_then(|s| s.sleeve.clone());
        for (id, charge) in gone {
            if self.pending.contains_key(&id) {
                continue;
            }
            self.unlink(id, &source, charge, sleeve.as_deref());
        }
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
        self.kept.remove(&stored);
        self.unbind_source(&stored, sleeve);
        self.sources.remove(&stored);
        self.dirty = true;
    }

    pub(crate) fn live(&self, m: &Meta) -> bool {
        self.dead.get(&m.source).is_none_or(|&below| m.id >= below)
    }

    fn recount(&mut self) {
        let (mut n, mut len) = (0, 0);
        let mut kept: BTreeMap<String, u64> = BTreeMap::new();
        let mut by_source: BTreeMap<String, SourceDocs> = BTreeMap::new();
        let mut sleeves: BTreeMap<String, SleeveDocs> = BTreeMap::new();
        for (si, s) in self.segs.iter().enumerate() {
            for m in s.docs.iter().filter(|m| self.live(m)) {
                n += 1;
                len += u64::from(m.len);
                let Some(src) = self.syms.name(m.source) else {
                    continue;
                };
                let charge = self.doc_charge(m);
                *kept.entry(src.to_owned()).or_default() += charge;
                by_source
                    .entry(src.to_owned())
                    .or_default()
                    .by_id
                    .insert(m.id, charge);
                if let Some(sleeve) = self.syms.name(m.sleeve) {
                    let slot = sleeves.entry(sleeve.to_owned()).or_default();
                    slot.total = slot.total.saturating_add(charge);
                    slot.by_id.insert(
                        m.id,
                        Loc {
                            source: src.to_owned(),
                            charge,
                            seg: Some(si),
                        },
                    );
                }
            }
        }
        for (&id, p) in &self.pending {
            let charge = queued_bytes(&p.draft, &p.source, p.sleeve.as_deref()) as u64;
            *kept.entry(p.source.clone()).or_default() += charge;
            by_source
                .entry(p.source.clone())
                .or_default()
                .by_id
                .insert(id, charge);
            if let Some(sleeve) = &p.sleeve {
                let slot = sleeves.entry(sleeve.clone()).or_default();
                slot.total = slot.total.saturating_add(charge);
                slot.by_id.insert(
                    id,
                    Loc {
                        source: p.source.clone(),
                        charge,
                        seg: None,
                    },
                );
            }
        }
        self.live_docs = n;
        self.live_len = len;
        self.kept = kept;
        self.by_source = by_source;
        self.sleeves = sleeves;
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
            self.kept.remove(&stored);
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
        self.bind_source(&key, source.sleeve.as_deref());
        let mut dropped = false;
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
                self.add_live(id, &stored, charge, source.sleeve.as_deref());
                self.pending.insert(
                    id,
                    Pending {
                        id,
                        offset,
                        source: stored.clone(),
                        sleeve: source.sleeve.clone(),
                        draft,
                    },
                );
                self.next_doc += 1;
                if !self.fit(&stored, source.sleeve.as_deref(), id, charge, &mut dropped) {
                    self.remove_pending(id);
                    if self.next_doc == id.saturating_add(1) {
                        self.next_doc = id;
                    }
                    got.docs = got.docs.saturating_sub(1);
                }
                if dropped && !rec.capped {
                    rec.capped = true;
                    self.mark_capped(&stored, source.sleeve.as_deref());
                }
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
        self.sources.insert(key, rec);
        self.settle_capped(&stored, source.sleeve.as_deref(), was_capped, &mut got);
        Ok(got)
    }

    fn kept_of(&self, stored: &str) -> u64 {
        self.kept.get(stored).copied().unwrap_or(0)
    }

    fn sleeve_kept(&self, sleeve: &str) -> u64 {
        self.sleeves.get(sleeve).map(|s| s.total).unwrap_or(0)
    }

    /// Drops older docs until the budget holds. A sleeved source loses the oldest doc of
    /// the sleeve, from any of its logs. An unsleeved source loses its own oldest doc.
    /// False when `keep` itself cannot fit.
    fn fit(
        &mut self,
        stored: &str,
        sleeve: Option<&str>,
        keep: u64,
        need: u64,
        dropped: &mut bool,
    ) -> bool {
        if need > self.budget {
            return false;
        }
        if sleeve.is_none() {
            while self.kept_of(stored) > self.budget {
                if !self.drop_oldest_except(stored, keep) {
                    return false;
                }
                *dropped = true;
            }
        }
        if let Some(sleeve) = sleeve {
            while self.sleeve_kept(sleeve) > self.budget {
                if !self.evict_oldest(sleeve, keep) {
                    return false;
                }
                *dropped = true;
            }
        }
        true
    }

    /// Drops the oldest live doc of `sleeve` other than `keep`.
    fn evict_oldest(&mut self, sleeve: &str, keep: u64) -> bool {
        let Some((id, source, pending)) = self.oldest_in_sleeve(sleeve, keep) else {
            return false;
        };
        if pending {
            self.remove_pending(id)
        } else {
            self.drop_source(source, id.saturating_add(1));
            true
        }
    }

    fn oldest_in_sleeve(&self, sleeve: &str, keep: u64) -> Option<(u64, String, bool)> {
        let slot = self.sleeves.get(sleeve)?;
        let mut it = slot.by_id.iter();
        let (&first, first_loc) = it.next()?;
        let (id, loc) = if first == keep {
            it.next().map(|(&id, loc)| (id, loc))?
        } else {
            (first, first_loc)
        };
        Some((id, loc.source.clone(), loc.seg.is_none()))
    }

    /// Drops the oldest live doc of `stored` other than `keep`.
    fn drop_oldest_except(&mut self, stored: &str, keep: u64) -> bool {
        let Some(id) = self.oldest_in_source(stored, keep) else {
            return false;
        };
        if self.pending.contains_key(&id) {
            self.remove_pending(id)
        } else {
            self.drop_source(stored.to_owned(), id.saturating_add(1));
            true
        }
    }

    fn oldest_in_source(&self, stored: &str, keep: u64) -> Option<u64> {
        let docs = self.by_source.get(stored)?;
        let mut it = docs.by_id.keys();
        let &id = it.next()?;
        if id == keep {
            it.next().copied()
        } else {
            Some(id)
        }
    }

    fn add_live(&mut self, id: u64, source: &str, charge: u64, sleeve: Option<&str>) {
        self.by_source
            .entry(source.to_owned())
            .or_default()
            .by_id
            .insert(id, charge);
        *self.kept.entry(source.to_owned()).or_default() += charge;
        let Some(sleeve) = sleeve else {
            return;
        };
        let slot = self.sleeves.entry(sleeve.to_owned()).or_default();
        slot.total = slot.total.saturating_add(charge);
        slot.by_id.insert(
            id,
            Loc {
                source: source.to_owned(),
                charge,
                seg: None,
            },
        );
    }

    fn unlink(&mut self, id: u64, source: &str, charge: u64, sleeve: Option<&str>) {
        let empty = self.by_source.get_mut(source).is_some_and(|docs| {
            docs.by_id.remove(&id);
            docs.by_id.is_empty()
        });
        if empty {
            self.by_source.remove(source);
        }
        self.sub_kept(source, charge);
        let Some(sleeve) = sleeve else {
            return;
        };
        let gone = if let Some(slot) = self.sleeves.get_mut(sleeve) {
            if let Some(loc) = slot.by_id.remove(&id) {
                slot.total = slot.total.saturating_sub(loc.charge);
            }
            slot.by_id.is_empty()
        } else {
            false
        };
        if gone {
            self.sleeves.remove(sleeve);
        }
    }

    /// Removes one pending doc and its charge. False when `id` is not pending.
    fn remove_pending(&mut self, id: u64) -> bool {
        let Some(p) = self.pending.remove(&id) else {
            return false;
        };
        let bytes = queued_bytes(&p.draft, &p.source, p.sleeve.as_deref());
        self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
        self.unlink(id, &p.source, bytes as u64, p.sleeve.as_deref());
        true
    }

    fn discard_pending(&mut self, stored: &str) {
        let ids: Vec<u64> = self
            .by_source
            .get(stored)
            .map(|d| d.by_id.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            self.remove_pending(id);
        }
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
                .values()
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
        // Pending docs are in a segment now. Rebuild before reclaim so a failed
        // reclaim does not leave them indexed as pending, and again after so a
        // reclaim that deleted dead segments and then failed still has live maps
        // that match the segments that remain.
        self.recount();
        let reclaimed = self.reclaim();
        self.recount();
        reclaimed?;
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
            budget: self.budget,
        };
        let json = m.to_json().to_string();
        let text = format!("{MANIFEST_MAGIC} {:08x}\n{json}", crc32c(json.as_bytes()));
        write_atomic(&self.dir, &self.dir.join(MANIFEST), text.as_bytes())
    }

    /// Removes segments whose docs are all dead, then rewrites one whose dead bytes are at
    /// least its live bytes. Each half is its own manifest commit. Dead files are deleted
    /// only after their manifest is durable, and a later rewrite failure does not put them
    /// back. New rewrite files are written first; the old file goes after that rename. A
    /// crash before a rename keeps the previous manifest, and the next open sweeps files
    /// it does not name.
    fn reclaim(&mut self) -> io::Result<()> {
        self.drop_dead_segments()?;
        self.rewrite_half_dead()
    }

    /// Drops segments with no live docs, high indexes first. The manifest is committed
    /// before any file is removed. A manifest failure puts the segments back and leaves
    /// the files in place.
    fn drop_dead_segments(&mut self) -> io::Result<()> {
        let dead: Vec<usize> = self
            .segs
            .iter()
            .enumerate()
            .filter(|(_, s)| segment_fully_dead(s, |m| self.live(m)))
            .map(|(i, _)| i)
            .collect();
        if dead.is_empty() {
            return Ok(());
        }
        let mut removed = Vec::new();
        for i in dead.into_iter().rev() {
            removed.push((i, self.segs.remove(i)));
        }
        if let Err(e) = self.write_manifest() {
            for (i, seg) in removed.into_iter().rev() {
                self.segs.insert(i, seg);
            }
            return Err(e);
        }
        for (_, seg) in removed {
            self.remove_segment_file(&seg.name)?;
        }
        Ok(())
    }

    /// Rewrites a segment whose dead bytes are at least its live bytes, and which still
    /// has a live doc. A write or manifest failure drops only the new files. Segments
    /// already removed as fully dead stay removed.
    fn rewrite_half_dead(&mut self) -> io::Result<()> {
        let plan: Vec<usize> = self
            .segs
            .iter()
            .enumerate()
            .filter(|(_, s)| segment_half_dead(s, |m| self.live(m), |m| self.doc_charge(m)))
            .map(|(i, _)| i)
            .collect();
        if plan.is_empty() {
            return Ok(());
        }
        let mut fresh = Vec::new();
        for &i in &plan {
            match self.write_live_segment(i) {
                Ok(seg) => fresh.push((i, seg)),
                Err(e) => {
                    self.drop_unreferenced(fresh.into_iter().map(|(_, seg)| seg));
                    return Err(e);
                }
            }
        }
        let mut old = Vec::new();
        for (i, neu) in fresh.into_iter().rev() {
            old.push((i, std::mem::replace(&mut self.segs[i], neu)));
        }
        if let Err(e) = self.write_manifest() {
            let mut neu = Vec::new();
            for (i, seg) in old.into_iter().rev() {
                neu.push(std::mem::replace(&mut self.segs[i], seg));
            }
            self.drop_unreferenced(neu);
            return Err(e);
        }
        for (_, seg) in old {
            self.remove_segment_file(&seg.name)?;
        }
        Ok(())
    }

    fn remove_segment_file(&self, name: &str) -> io::Result<()> {
        let p = self.dir.join(name);
        match fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ctx(e, "remove dead", p.display())),
        }
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

    /// Sleeve ids eviction has dropped a doc from. The name stays while the sleeve is under
    /// the same budget. It leaves when that sleeve is removed, when its sources are gone,
    /// or when a later open finds the total under a raised budget. Readers of the hub copy
    /// this set out so `doctor` does not take the index lock.
    pub fn capped_sleeves(&self) -> Vec<String> {
        self.capped_ids.iter().cloned().collect()
    }

    /// Clears the capped name for a sleeve that was removed. Docs stay. Sets `dirty` when
    /// a flag or the name actually changed, so the next commit records it.
    pub fn clear_sleeve_cap(&mut self, sleeve: &str) {
        let keys: Vec<String> = self
            .members
            .get(sleeve)
            .map(|keys| keys.iter().cloned().collect())
            .unwrap_or_default();
        let mut changed = self.capped_ids.remove(sleeve);
        for key in keys {
            if let Some(s) = self.sources.get_mut(&key)
                && s.capped
            {
                s.capped = false;
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
    }

    fn bind_members(&mut self) {
        self.members.clear();
        let pairs: Vec<(String, Option<String>)> = self
            .sources
            .iter()
            .map(|(k, s)| (k.clone(), s.sleeve.clone()))
            .collect();
        for (key, sleeve) in pairs {
            self.bind_source(&key, sleeve.as_deref());
        }
    }

    fn bind_source(&mut self, key: &str, sleeve: Option<&str>) {
        let Some(sleeve) = sleeve else {
            return;
        };
        self.members
            .entry(sleeve.to_owned())
            .or_default()
            .insert(key.to_owned());
    }

    fn unbind_source(&mut self, key: &str, sleeve: Option<&str>) {
        let Some(sleeve) = sleeve else {
            return;
        };
        let empty = self.members.get_mut(sleeve).is_some_and(|keys| {
            keys.remove(key);
            keys.is_empty()
        });
        if empty {
            self.members.remove(sleeve);
        }
        self.refresh_capped_id(sleeve);
    }

    fn refresh_capped_id(&mut self, sleeve: &str) {
        let on = self.members.get(sleeve).is_some_and(|keys| {
            keys.iter()
                .any(|k| self.sources.get(k).is_some_and(|s| s.capped))
        });
        if on {
            self.capped_ids.insert(sleeve.to_owned());
        } else {
            self.capped_ids.remove(sleeve);
        }
    }

    fn rebuild_capped_ids(&mut self) {
        self.capped_ids.clear();
        for s in self.sources.values() {
            if s.capped
                && let Some(sleeve) = &s.sleeve
            {
                self.capped_ids.insert(sleeve.clone());
            }
        }
    }

    /// A manifest with no recorded budget counts as not-yet-raised: flags clear only where
    /// the live total is already under the budget being opened. The same or a lower budget
    /// keeps a flag that eviction set.
    fn release_if_budget_raised(&mut self, stored: u64) {
        if stored != 0 && self.budget <= stored {
            return;
        }
        let budget = self.budget;
        let mut sleeves = std::collections::BTreeSet::new();
        let mut unsleeved = Vec::new();
        for (key, s) in &self.sources {
            if !s.capped {
                continue;
            }
            if let Some(sleeve) = &s.sleeve {
                sleeves.insert(sleeve.clone());
            } else {
                unsleeved.push(key.clone());
            }
        }
        let clear_sleeves: Vec<String> = sleeves
            .into_iter()
            .filter(|sleeve| self.sleeve_kept(sleeve) < budget)
            .collect();
        let clear_sources: Vec<String> = unsleeved
            .into_iter()
            .filter(|key| self.kept_of(key) < budget)
            .collect();
        let mut changed = false;
        for sleeve in &clear_sleeves {
            let keys: Vec<String> = self
                .members
                .get(sleeve)
                .map(|keys| keys.iter().cloned().collect())
                .unwrap_or_default();
            for key in keys {
                if let Some(s) = self.sources.get_mut(&key)
                    && s.capped
                {
                    s.capped = false;
                    changed = true;
                }
            }
            self.capped_ids.remove(sleeve);
        }
        for key in &clear_sources {
            if let Some(s) = self.sources.get_mut(key)
                && s.capped
            {
                s.capped = false;
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
    }

    fn mark_capped(&mut self, stored: &str, sleeve: Option<&str>) {
        let mut changed = false;
        if let Some(sleeve) = sleeve {
            self.bind_source(stored, Some(sleeve));
            let keys: Vec<String> = self
                .members
                .get(sleeve)
                .map(|keys| keys.iter().cloned().collect())
                .unwrap_or_default();
            for key in keys {
                if let Some(s) = self.sources.get_mut(&key)
                    && !s.capped
                {
                    s.capped = true;
                    changed = true;
                }
            }
            self.capped_ids.insert(sleeve.to_owned());
        } else if let Some(s) = self.sources.get_mut(stored)
            && !s.capped
        {
            s.capped = true;
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    fn is_capped(&self, stored: &str, sleeve: Option<&str>) -> bool {
        match sleeve {
            Some(sleeve) => self.capped_ids.contains(sleeve),
            None => self.sources.get(stored).is_some_and(|s| s.capped),
        }
    }

    /// Keeps the capped flag once eviction has dropped a doc, and copies it onto every
    /// source of that sleeve. A later ingest under the same budget does not clear it.
    fn settle_capped(&mut self, stored: &str, sleeve: Option<&str>, was: bool, got: &mut Ingested) {
        if !self.is_capped(stored, sleeve) {
            return;
        }
        if !was {
            got.capped = true;
        }
        self.mark_capped(stored, sleeve);
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

/// A segment with no docs, or whose every doc is dead, is deleted rather than rewritten.
fn segment_fully_dead(s: &Seg, mut live: impl FnMut(&Meta) -> bool) -> bool {
    let n = s.docs.len();
    n == 0 || s.docs.iter().filter(|m| !live(m)).count() == n
}

/// Dead bytes are at least the live bytes, and some live bytes remain. A segment whose
/// docs are all dead is deleted instead. The charge is the indexed byte charge, not the
/// token count stored on the doc.
fn segment_half_dead(
    s: &Seg,
    mut live: impl FnMut(&Meta) -> bool,
    mut charge: impl FnMut(&Meta) -> u64,
) -> bool {
    let mut dead = 0u64;
    let mut alive = 0u64;
    for m in &s.docs {
        let c = charge(m);
        if live(m) {
            alive = alive.saturating_add(c);
        } else {
            dead = dead.saturating_add(c);
        }
    }
    alive > 0 && dead >= alive
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
