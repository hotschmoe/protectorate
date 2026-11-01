//! The stream log: every kept message of the event stream, on disk, so seqs continue across a
//! restart and a client resuming from a retained seq gets the delta instead of a snapshot.
//!
//! ```text
//! <home>/stream.log   magic, version, then pt-journal frames (u32 length, u32 CRC-32C, payload)
//! payload             kind u8, seq u64 LE, at_ms u64 LE, [event seq u64 LE], message JSON
//! ```
//!
//! Kind 0 is the window marker a compaction writes first: from its seq on, the file holds every
//! kept message. Kind 1 is a message, kind 2 a message carrying a hub event (`event`). Appends
//! are fdatasynced before the messages become visible (the bus's group commit), so no client
//! ever saw a seq the file lacks. A torn tail is cut at open.
//!
//! The file is derived output, not core state, so it is not in the core journal. Compaction
//! rewrites it through a temporary file and a rename once it holds twice what retention keeps:
//! the last `KEEP_MESSAGES` messages or `KEEP_AGE` of them, whichever is more, plus older hub
//! events by the same rule counted in events.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use pt_journal::codec::{decode_frames, encode_frame};

const MAGIC: [u8; 8] = *b"PTSTREAM";
const VERSION: u32 = 1;
const HEADER: usize = 12;
pub const KEEP_MESSAGES: usize = 10_000;
pub const KEEP_EVENTS: usize = 10_000;
pub const KEEP_AGE: u64 = 7 * 24 * 3600 * 1000;

const WINDOW: u8 = 0;
const MESSAGE: u8 = 1;
const EVENT: u8 = 2;

/// One kept message as stored.
#[derive(Clone, Debug)]
pub struct Rec {
    pub seq: u64,
    pub at_ms: u64,
    /// The hub event's own seq, for an `event` message.
    pub event: Option<u64>,
    pub text: std::sync::Arc<str>,
}

/// What `open` found.
pub struct Opened {
    pub log: Log,
    /// From this seq on the file holds every kept message.
    pub window: Option<u64>,
    /// Every stored message, in seq order (older hub events before the window first).
    pub recs: Vec<Rec>,
    /// Bytes cut off a torn or damaged tail.
    pub cut: u64,
}

/// The open file. Only the bus's writer thread appends and compacts.
pub struct Log {
    path: PathBuf,
    file: File,
    len: u64,
    /// Messages in the file, and how many the last compaction kept.
    messages: usize,
    kept: usize,
}

fn ctx(e: io::Error, what: &str, path: &Path) -> io::Error {
    io::Error::new(e.kind(), format!("{what} {}: {e}", path.display()))
}

fn invalid(path: &Path, msg: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {msg}", path.display()),
    )
}

fn header() -> [u8; HEADER] {
    let mut h = [0u8; HEADER];
    h[..8].copy_from_slice(&MAGIC);
    h[8..].copy_from_slice(&VERSION.to_le_bytes());
    h
}

fn sync_parent(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| ctx(e, "fsync dir", dir))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// A frame's payload: `(kind, seq, at, event, text)`, or `None` when it is malformed.
fn parse(p: &[u8]) -> Option<(u8, u64, u64, Option<u64>, &str)> {
    let kind = *p.first()?;
    let seq = u64_at(p, 1)?;
    let at = u64_at(p, 9)?;
    match kind {
        WINDOW if p.len() == 17 => Some((kind, seq, at, None, "")),
        MESSAGE => Some((kind, seq, at, None, std::str::from_utf8(&p[17..]).ok()?)),
        EVENT => {
            let ev = u64_at(p, 17)?;
            Some((kind, seq, at, Some(ev), std::str::from_utf8(&p[25..]).ok()?))
        }
        _ => None,
    }
}

fn payload(kind: u8, seq: u64, at: u64, event: Option<u64>, text: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(25 + text.len());
    p.push(kind);
    p.extend_from_slice(&seq.to_le_bytes());
    p.extend_from_slice(&at.to_le_bytes());
    if let Some(e) = event {
        p.extend_from_slice(&e.to_le_bytes());
    }
    p.extend_from_slice(text.as_bytes());
    p
}

fn frame(buf: &mut Vec<u8>, r: &Rec) -> io::Result<()> {
    let kind = if r.event.is_some() { EVENT } else { MESSAGE };
    encode_frame(&payload(kind, r.seq, r.at_ms, r.event, &r.text), buf).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("stream message {}: {e:?}", r.seq),
        )
    })
}

struct Scan {
    window: Option<u64>,
    recs: Vec<Rec>,
    valid: u64,
}

/// Reads every intact frame from the start of `data`.
fn scan(path: &Path, data: &[u8]) -> io::Result<Scan> {
    if data.len() < HEADER || data[..HEADER] != header() {
        return Err(invalid(path, "not a protectorate stream log"));
    }
    let decoded = decode_frames(&data[HEADER..]);
    let mut out = Scan {
        window: None,
        recs: Vec::with_capacity(decoded.frames.len()),
        valid: HEADER as u64,
    };
    for f in decoded.frames {
        let Some((kind, seq, at_ms, event, text)) = parse(f) else {
            break;
        };
        if kind == WINDOW {
            out.window = Some(seq);
        } else {
            out.recs.push(Rec {
                seq,
                at_ms,
                event,
                text: text.into(),
            });
        }
        out.valid += (8 + f.len()) as u64;
    }
    Ok(out)
}

pub fn open(path: &Path) -> io::Result<Opened> {
    let mut data = Vec::new();
    match File::open(path) {
        Ok(mut f) => {
            f.read_to_end(&mut data).map_err(|e| ctx(e, "read", path))?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(ctx(e, "open", path)),
    }
    // Missing, or torn at its creation: start it afresh.
    if data.len() < HEADER && header().starts_with(&data) {
        {
            let mut f = File::create(path).map_err(|e| ctx(e, "create", path))?;
            f.write_all(&header())
                .and_then(|()| f.sync_all())
                .map_err(|e| ctx(e, "write", path))?;
        }
        sync_parent(path)?;
        data = header().to_vec();
    }
    let s = scan(path, &data)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| ctx(e, "open", path))?;
    let cut = data.len() as u64 - s.valid;
    if cut > 0 {
        file.set_len(s.valid)
            .and_then(|()| file.sync_all())
            .map_err(|e| ctx(e, "truncate", path))?;
    }
    let messages = s.recs.len();
    Ok(Opened {
        log: Log {
            path: path.to_path_buf(),
            file,
            len: s.valid,
            messages,
            kept: messages,
        },
        window: s.window,
        recs: s.recs,
        cut,
    })
}

/// The kept messages with `from <= seq < until`, in order, when the file still holds all of
/// them; `None` when any is gone (a compaction moved the window past `from`).
pub fn read_range(path: &Path, from: u64, until: u64) -> io::Result<Option<Vec<Rec>>> {
    let mut data = Vec::new();
    File::open(path)
        .and_then(|mut f| f.read_to_end(&mut data))
        .map_err(|e| ctx(e, "read", path))?;
    let s = scan(path, &data)?;
    if s.window.is_some_and(|w| from < w) {
        return Ok(None);
    }
    let got: Vec<Rec> = s
        .recs
        .into_iter()
        .filter(|r| r.seq >= from && r.seq < until)
        .collect();
    let whole = got.len() as u64 == until.saturating_sub(from)
        && got.iter().zip(from..).all(|(r, want)| r.seq == want);
    Ok(whole.then_some(got))
}

/// Where the retained suffix of `recs` (messages at or after the window) starts, by index.
fn suffix_start(recs: &[&Rec], now: u64) -> usize {
    let by_count = recs.len().saturating_sub(KEEP_MESSAGES);
    let by_age = recs
        .iter()
        .position(|r| now.saturating_sub(r.at_ms) < KEEP_AGE)
        .unwrap_or(recs.len());
    by_count.min(by_age)
}

/// What a compaction kept: the new window, and the hub events it dropped.
pub struct Compacted {
    pub window: u64,
    pub dropped_events: BTreeSet<u64>,
}

impl Log {
    /// Appends the records and fdatasyncs once.
    pub fn append(&mut self, recs: &[Rec]) -> io::Result<()> {
        let mut buf = Vec::new();
        for r in recs {
            frame(&mut buf, r)?;
        }
        self.file
            .write_all_at(&buf, self.len)
            .and_then(|()| self.file.sync_data())
            .map_err(|e| ctx(e, "append to", &self.path))?;
        self.len += buf.len() as u64;
        self.messages += recs.len();
        Ok(())
    }

    /// The file holds twice what retention keeps.
    pub fn wants_compaction(&self) -> bool {
        self.messages >= 2 * self.kept.max(KEEP_MESSAGES)
    }

    /// Rewrites the file with what retention keeps; `next_seq` is the window when nothing is.
    pub fn compact(&mut self, now: u64, next_seq: u64) -> io::Result<Compacted> {
        let mut data = Vec::new();
        File::open(&self.path)
            .and_then(|mut f| f.read_to_end(&mut data))
            .map_err(|e| ctx(e, "read", &self.path))?;
        let s = scan(&self.path, &data)?;
        let old_window = s.window.unwrap_or(0);
        let live: Vec<&Rec> = s.recs.iter().filter(|r| r.seq >= old_window).collect();
        let start = suffix_start(&live, now);
        let window = live.get(start).map_or(next_seq, |r| r.seq);
        let kept = &live[start..];

        // Hub events before the window: the newest record of each, unless a kept message has a
        // newer one, by the same rule counted in events.
        let in_suffix: BTreeSet<u64> = kept.iter().filter_map(|r| r.event).collect();
        let mut older: BTreeMap<u64, &Rec> = BTreeMap::new();
        for r in s.recs.iter().filter(|r| r.seq < window) {
            if let Some(e) = r.event
                && !in_suffix.contains(&e)
            {
                older.insert(e, r);
            }
        }
        let room = KEEP_EVENTS.saturating_sub(in_suffix.len());
        let skip = older.len().saturating_sub(room);
        let mut dropped_events = BTreeSet::new();
        let mut old_events: Vec<&Rec> = Vec::new();
        for (i, (e, r)) in older.iter().enumerate() {
            if i >= skip || now.saturating_sub(r.at_ms) < KEEP_AGE {
                old_events.push(r);
            } else {
                dropped_events.insert(*e);
            }
        }
        old_events.sort_by_key(|r| r.seq);

        let mut buf = header().to_vec();
        encode_frame(&payload(WINDOW, window, now, None, ""), &mut buf)
            .map_err(|e| invalid(&self.path, &format!("window frame: {e:?}")))?;
        for r in old_events.iter().chain(kept.iter()) {
            frame(&mut buf, r)?;
        }
        let tmp = self.path.with_extension("log.tmp");
        {
            let mut f = File::create(&tmp).map_err(|e| ctx(e, "create", &tmp))?;
            f.write_all(&buf)
                .and_then(|()| f.sync_all())
                .map_err(|e| ctx(e, "write", &tmp))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| ctx(e, "rename to", &self.path))?;
        sync_parent(&self.path)?;
        self.file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| ctx(e, "open", &self.path))?;
        self.len = buf.len() as u64;
        self.messages = old_events.len() + kept.len();
        self.kept = self.messages;
        Ok(Compacted {
            window,
            dropped_events,
        })
    }
}
