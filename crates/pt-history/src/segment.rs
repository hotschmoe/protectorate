//! The segment file: an immutable, checksummed block of documents and their inverted index.
//! Written once to a temporary name and renamed into place, so a segment is either whole or
//! absent. All integers are little-endian; `v` is an unsigned LEB128 varint.
//!
//! ```text
//! header    "PTHISTSG", u32 version, u32 strings, u32 docs, u32 terms,
//!           u64 byte lengths of the five sections below
//! strings   per string: v len, UTF-8 bytes (sessions, sleeves, sources, tools, paths)
//! docs      per doc: v id (delta from the previous, absolute for the first), v time_ms,
//!           v offset in the source file, v turn, v positions, v text bytes, v session,
//!           v sleeve+1 (0 none), v source, v tool+1 (0 none), v harness (0 none, 1..4),
//!           v role, v files, files x v string
//! dict      per term, strictly ascending bytes: v len, UTF-8 bytes, v docs, v postings bytes
//! postings  per term in dict order, per doc: v doc (delta, absolute for the first), v count,
//!           count x v position (delta, absolute for the first)
//! texts     the doc texts back to back
//! footer    u64 body length, u32 CRC-32C of the body, "PTHX"
//! ```

use std::collections::HashMap;
use std::ops::Range;

use pt_drivers::Harness;
use pt_journal::crc32c::crc32c;

use crate::doc::Role;
use crate::token::tokenize;

const MAGIC: &[u8; 8] = b"PTHISTSG";
const VERSION: u32 = 1;
const FOOTER_MAGIC: &[u8; 4] = b"PTHX";
pub const HEADER: usize = 8 + 4 + 3 * 4 + 5 * 8;
const FOOTER: usize = 8 + 4 + 4;
pub const NONE: u32 = u32::MAX;

pub struct DocIn<'a> {
    pub id: u64,
    pub time_ms: u64,
    pub offset: u64,
    pub turn: u32,
    pub role: Role,
    pub harness: Option<Harness>,
    pub session: &'a str,
    pub sleeve: Option<&'a str>,
    pub source: &'a str,
    pub tool: Option<&'a str>,
    pub files: &'a [String],
    pub text: &'a str,
}

/// A document's metadata. String fields index the segment's string table ([`NONE`] when
/// absent) until the index maps them to its own symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub id: u64,
    pub time_ms: u64,
    pub offset: u64,
    pub turn: u32,
    pub len: u32,
    pub text_off: u64,
    pub text_len: u32,
    pub session: u32,
    pub sleeve: u32,
    pub source: u32,
    pub tool: u32,
    pub harness: Option<Harness>,
    pub role: Role,
    pub files: Box<[u32]>,
}

/// The term dictionary: sorted terms back to back, with their postings ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dict {
    terms: Vec<u8>,
    ends: Vec<u32>,
    posts: Vec<Post>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Post {
    /// Offset within the postings section.
    pub off: u64,
    pub len: u32,
    pub df: u32,
}

impl Dict {
    pub fn term_count(&self) -> usize {
        self.posts.len()
    }

    fn term(&self, i: usize) -> &[u8] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        &self.terms[start..self.ends[i] as usize]
    }

    pub fn find(&self, term: &str) -> Option<Post> {
        let (mut lo, mut hi) = (0, self.posts.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.term(mid).cmp(term.as_bytes()) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(self.posts[mid]),
            }
        }
        None
    }

    /// Every term with its postings, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Post)> {
        (0..self.posts.len()).map(|i| {
            let t = std::str::from_utf8(self.term(i)).unwrap_or_default();
            (t, self.posts[i])
        })
    }
}

#[derive(Debug)]
pub struct Decoded {
    pub strings: Vec<String>,
    pub docs: Vec<Meta>,
    pub dict: Dict,
    /// Byte ranges of the postings and texts sections within the file.
    pub postings: Range<u64>,
    pub texts: Range<u64>,
}

fn put(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn harness_code(h: Option<Harness>) -> u64 {
    match h {
        None => 0,
        Some(Harness::Claude) => 1,
        Some(Harness::Codex) => 2,
        Some(Harness::Grok) => 3,
        Some(Harness::Pi) => 4,
    }
}

fn harness_from(c: u64) -> Option<Option<Harness>> {
    Some(match c {
        0 => None,
        1 => Some(Harness::Claude),
        2 => Some(Harness::Codex),
        3 => Some(Harness::Grok),
        4 => Some(Harness::Pi),
        _ => return None,
    })
}

/// A string-keyed map with a fast non-cryptographic hash; keys come from our own logs.
type FastMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<Fx>>;

#[derive(Default)]
pub struct Fx(u64);

impl std::hash::Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut h = self.0;
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
        for &b in rest {
            h = (h.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
        self.0 = h;
    }
}

struct TermBuf {
    last: u32,
    df: u32,
    bytes: Vec<u8>,
}

/// Builds a segment file from documents in ascending id order.
pub fn encode<'a>(docs: &[DocIn<'a>]) -> Vec<u8> {
    let mut strings: Vec<&'a str> = Vec::new();
    let mut string_ids: FastMap<&'a str, u32> = FastMap::default();
    let mut sym = |s: &'a str| -> u32 {
        *string_ids.entry(s).or_insert_with(|| {
            strings.push(s);
            strings.len() as u32 - 1
        })
    };
    let mut term_ids: FastMap<Box<str>, u32> = FastMap::default();
    let mut terms: Vec<TermBuf> = Vec::new();
    let mut doc_bytes = Vec::new();
    let mut texts_len = 0u64;
    let mut occ: Vec<(u32, u32)> = Vec::new();
    let mut prev_id = None;
    for (local, d) in docs.iter().enumerate() {
        let local = local as u32;
        occ.clear();
        let len = tokenize(d.text, |t| {
            let id = match term_ids.get(t.term) {
                Some(&id) => id,
                None => {
                    let id = terms.len() as u32;
                    term_ids.insert(t.term.into(), id);
                    terms.push(TermBuf {
                        last: NONE,
                        df: 0,
                        bytes: Vec::new(),
                    });
                    id
                }
            };
            occ.push((id, t.pos));
        });
        occ.sort_unstable();
        occ.dedup();
        let mut i = 0;
        while i < occ.len() {
            let term = occ[i].0;
            let mut j = i;
            while j < occ.len() && occ[j].0 == term {
                j += 1;
            }
            let tb = &mut terms[term as usize];
            let delta = if tb.last == NONE {
                local
            } else {
                local - tb.last
            };
            put(&mut tb.bytes, u64::from(delta));
            put(&mut tb.bytes, (j - i) as u64);
            let mut prev = 0;
            for (k, &(_, pos)) in occ[i..j].iter().enumerate() {
                put(
                    &mut tb.bytes,
                    u64::from(if k == 0 { pos } else { pos - prev }),
                );
                prev = pos;
            }
            tb.last = local;
            tb.df += 1;
            i = j;
        }
        put(
            &mut doc_bytes,
            match prev_id {
                None => d.id,
                Some(p) => d.id - p,
            },
        );
        prev_id = Some(d.id);
        put(&mut doc_bytes, d.time_ms);
        put(&mut doc_bytes, d.offset);
        put(&mut doc_bytes, u64::from(d.turn));
        put(&mut doc_bytes, u64::from(len));
        put(&mut doc_bytes, d.text.len() as u64);
        texts_len += d.text.len() as u64;
        let session = sym(d.session);
        put(&mut doc_bytes, u64::from(session));
        let sleeve = d.sleeve.map_or(0, |s| u64::from(sym(s)) + 1);
        put(&mut doc_bytes, sleeve);
        let source = sym(d.source);
        put(&mut doc_bytes, u64::from(source));
        let tool = d.tool.map_or(0, |s| u64::from(sym(s)) + 1);
        put(&mut doc_bytes, tool);
        put(&mut doc_bytes, harness_code(d.harness));
        put(&mut doc_bytes, d.role as u64);
        put(&mut doc_bytes, d.files.len() as u64);
        for f in d.files {
            let f = sym(f);
            put(&mut doc_bytes, u64::from(f));
        }
    }
    let mut order: Vec<(&str, u32)> = term_ids.iter().map(|(t, &id)| (&**t, id)).collect();
    order.sort_unstable();

    let mut out = vec![0u8; HEADER];
    let mut lens = [0u64; 5];
    let mark = out.len();
    for s in &strings {
        put(&mut out, s.len() as u64);
        out.extend_from_slice(s.as_bytes());
    }
    lens[0] = (out.len() - mark) as u64;
    out.extend_from_slice(&doc_bytes);
    lens[1] = doc_bytes.len() as u64;
    let mark = out.len();
    for &(t, id) in &order {
        let tb = &terms[id as usize];
        put(&mut out, t.len() as u64);
        out.extend_from_slice(t.as_bytes());
        put(&mut out, u64::from(tb.df));
        put(&mut out, tb.bytes.len() as u64);
    }
    lens[2] = (out.len() - mark) as u64;
    let mark = out.len();
    for &(_, id) in &order {
        out.extend_from_slice(&terms[id as usize].bytes);
    }
    lens[3] = (out.len() - mark) as u64;
    out.reserve(texts_len as usize + FOOTER);
    for d in docs {
        out.extend_from_slice(d.text.as_bytes());
    }
    lens[4] = texts_len;

    let mut h = Vec::with_capacity(HEADER);
    h.extend_from_slice(MAGIC);
    h.extend_from_slice(&VERSION.to_le_bytes());
    h.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    h.extend_from_slice(&(docs.len() as u32).to_le_bytes());
    h.extend_from_slice(&(order.len() as u32).to_le_bytes());
    for l in lens {
        h.extend_from_slice(&l.to_le_bytes());
    }
    out[..HEADER].copy_from_slice(&h);
    let body = out.len() as u64;
    let crc = crc32c(&out);
    out.extend_from_slice(&body.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(FOOTER_MAGIC);
    out
}

pub type Corrupt = &'static str;

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, i: 0 }
    }

    fn done(&self) -> bool {
        self.i == self.b.len()
    }

    fn var(&mut self) -> Result<u64, Corrupt> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let byte = *self.b.get(self.i).ok_or("truncated varint")?;
            self.i += 1;
            if shift == 63 && byte > 1 {
                return Err("varint overflow");
            }
            v |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                return Err("varint overflow");
            }
        }
    }

    fn u32(&mut self) -> Result<u32, Corrupt> {
        u32::try_from(self.var()?).map_err(|_| "value exceeds u32")
    }

    fn bytes(&mut self, n: u64) -> Result<&'a [u8], Corrupt> {
        let n = usize::try_from(n).map_err(|_| "length overflow")?;
        let end = self.i.checked_add(n).ok_or("length overflow")?;
        let s = self.b.get(self.i..end).ok_or("truncated bytes")?;
        self.i = end;
        Ok(s)
    }

    fn sym(&mut self, n: u32, optional: bool) -> Result<u32, Corrupt> {
        let v = self.u32()?;
        if optional {
            if v == 0 {
                return Ok(NONE);
            }
            if v > n {
                return Err("string index out of range");
            }
            return Ok(v - 1);
        }
        if v >= n {
            return Err("string index out of range");
        }
        Ok(v)
    }
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le_u64(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

/// Decodes and fully validates a segment file: checksum, structure, every string, every doc
/// and every posting. Any inconsistency is an error, never a panic.
pub fn decode(b: &[u8]) -> Result<Decoded, Corrupt> {
    if b.len() < HEADER + FOOTER {
        return Err("shorter than header and footer");
    }
    let body = b.len() - FOOTER;
    if &b[b.len() - 4..] != FOOTER_MAGIC {
        return Err("bad footer magic");
    }
    if le_u64(b, body) != body as u64 {
        return Err("footer length mismatch");
    }
    if crc32c(&b[..body]) != le_u32(b, body + 8) {
        return Err("checksum mismatch");
    }
    if &b[..8] != MAGIC {
        return Err("bad magic");
    }
    if le_u32(b, 8) != VERSION {
        return Err("unknown version");
    }
    let nstr = le_u32(b, 12);
    let ndocs = le_u32(b, 16);
    let nterms = le_u32(b, 20);
    let mut ranges = Vec::with_capacity(5);
    let mut at = HEADER as u64;
    for k in 0..5 {
        let l = le_u64(b, 24 + 8 * k);
        let end = at.checked_add(l).ok_or("section length overflow")?;
        ranges.push(at..end);
        at = end;
    }
    if at != body as u64 {
        return Err("section lengths do not match the body");
    }
    let sect = |k: usize| &b[ranges[k].start as usize..ranges[k].end as usize];

    let mut r = Rd::new(sect(0));
    let mut strings = Vec::with_capacity(nstr.min(1 << 20) as usize);
    for _ in 0..nstr {
        let n = r.var()?;
        let s = std::str::from_utf8(r.bytes(n)?).map_err(|_| "string is not UTF-8")?;
        strings.push(s.to_owned());
    }
    if !r.done() {
        return Err("trailing bytes in strings");
    }

    let texts = sect(4);
    let mut r = Rd::new(sect(1));
    let mut docs: Vec<Meta> = Vec::with_capacity(ndocs.min(1 << 20) as usize);
    let mut text_off = 0u64;
    for i in 0..ndocs {
        let delta = r.var()?;
        let id = match docs.last() {
            None => delta,
            Some(p) if delta > 0 => p.id.checked_add(delta).ok_or("doc id overflow")?,
            Some(_) => return Err("doc ids not ascending"),
        };
        let time_ms = r.var()?;
        let offset = r.var()?;
        let turn = r.u32()?;
        let len = r.u32()?;
        let text_len = r.u32()?;
        let end = text_off
            .checked_add(u64::from(text_len))
            .ok_or("text range overflow")?;
        let text = texts
            .get(text_off as usize..end as usize)
            .ok_or("text out of range")?;
        if std::str::from_utf8(text).is_err() {
            return Err("text is not UTF-8");
        }
        let session = r.sym(nstr, false)?;
        let sleeve = r.sym(nstr, true)?;
        let source = r.sym(nstr, false)?;
        let tool = r.sym(nstr, true)?;
        let harness = harness_from(r.var()?).ok_or("bad harness")?;
        let role = Role::from_code(r.var()?).ok_or("bad role")?;
        let nfiles = r.var()?;
        if nfiles > (r.b.len() - r.i) as u64 {
            return Err("file count out of range");
        }
        let mut files = Vec::with_capacity(nfiles as usize);
        for _ in 0..nfiles {
            files.push(r.sym(nstr, false)?);
        }
        let _ = i;
        docs.push(Meta {
            id,
            time_ms,
            offset,
            turn,
            len,
            text_off,
            text_len,
            session,
            sleeve,
            source,
            tool,
            harness,
            role,
            files: files.into_boxed_slice(),
        });
        text_off = end;
    }
    if !r.done() {
        return Err("trailing bytes in docs");
    }
    if text_off != texts.len() as u64 {
        return Err("texts length mismatch");
    }

    let postings = sect(3);
    let mut r = Rd::new(sect(2));
    let mut dict = Dict::default();
    let mut post_off = 0u64;
    let mut scratch = Vec::new();
    for i in 0..nterms as usize {
        let n = r.var()?;
        let t = r.bytes(n)?;
        if t.is_empty() || std::str::from_utf8(t).is_err() {
            return Err("term is empty or not UTF-8");
        }
        if i > 0 && dict.term(i - 1) >= t {
            return Err("terms not ascending");
        }
        let df = r.u32()?;
        let len = r.u32()?;
        let end = post_off
            .checked_add(u64::from(len))
            .ok_or("postings overflow")?;
        let p = postings
            .get(post_off as usize..end as usize)
            .ok_or("postings out of range")?;
        let mut seen = 0u32;
        let mut bad = None;
        walk(p, ndocs, &mut scratch, |doc, pos| {
            seen += 1;
            if pos.last().is_some_and(|l| *l >= docs[doc as usize].len) {
                bad = Some("position beyond doc length");
            }
        })?;
        if let Some(e) = bad {
            return Err(e);
        }
        if seen != df || df == 0 {
            return Err("doc count mismatch");
        }
        dict.terms.extend_from_slice(t);
        dict.ends
            .push(u32::try_from(dict.terms.len()).map_err(|_| "dictionary too large")?);
        dict.posts.push(Post {
            off: post_off,
            len,
            df,
        });
        post_off = end;
    }
    if !r.done() {
        return Err("trailing bytes in dict");
    }
    if post_off != postings.len() as u64 {
        return Err("postings length mismatch");
    }
    Ok(Decoded {
        strings,
        docs,
        dict,
        postings: ranges[3].clone(),
        texts: ranges[4].clone(),
    })
}

/// Walks one term's postings: `f(local doc, positions)` per doc, in ascending doc order.
pub fn walk(
    p: &[u8],
    ndocs: u32,
    scratch: &mut Vec<u32>,
    mut f: impl FnMut(u32, &[u32]),
) -> Result<(), Corrupt> {
    let mut r = Rd::new(p);
    let mut doc: Option<u32> = None;
    while !r.done() {
        let delta = r.u32()?;
        let d = match doc {
            None => delta,
            Some(_) if delta == 0 => return Err("postings not ascending"),
            Some(prev) => prev.checked_add(delta).ok_or("doc overflow")?,
        };
        if d >= ndocs {
            return Err("posting beyond doc count");
        }
        let tf = r.u32()?;
        if tf == 0 || tf as usize > r.b.len() - r.i {
            return Err("bad term frequency");
        }
        scratch.clear();
        let mut pos = 0u32;
        for k in 0..tf {
            let v = r.u32()?;
            pos = if k == 0 {
                v
            } else if v == 0 {
                return Err("positions not ascending");
            } else {
                pos.checked_add(v).ok_or("position overflow")?
            };
            scratch.push(pos);
        }
        f(d, scratch);
        doc = Some(d);
    }
    Ok(())
}
