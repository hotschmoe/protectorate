//! Per-sleeve transcript: an append-only JSONL file of the drivers' normalized entries with a
//! monotonic seq and the hub's turn number. Streaming text deltas never reach the file; the
//! final block does. `wire::chat_entry` turns a line into the API's entry. Reads run on the file
//! outside every lock, up to the length it had when they started: the latest entries from the
//! end back, older ones from the start.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pt_api::{Harness, Level};
use pt_core::SleeveId;
use pt_drivers::Entry;
use pt_json::ToJson;

use crate::home::Home;
use crate::lock::lock;

/// What a backward read takes from the file at a time, more when one line is longer.
const CHUNK: u64 = 64 * 1024;

pt_json::record! {
    /// One stored entry. `harness` is set on `session_info` (the drivers say only `pi`), `level`
    /// on the hub's own notices.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Line {
        pub seq: u64,
        pub g: u64,
        pub time_ms: u64,
        pub turn: u64 = 0,
        pub harness: Option<Harness>,
        pub level: Option<Level>,
        pub entry: Entry,
    }
}

/// What `Store::append` takes besides the entry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Note {
    pub harness: Option<Harness>,
    pub level: Option<Level>,
}

/// Facts the sleeve listing shows, folded from the entries as they arrive.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub turns: u64,
    pub context_tokens: u64,
    pub context_window: u64,
    pub provider: String,
    pub model: String,
    pub effort: Option<String>,
    pub started_ms: u64,
    pub g: u64,
}

impl Stats {
    fn fold(&mut self, e: &Line) {
        match &e.entry {
            Entry::SessionInfo {
                model,
                provider,
                effort,
                ..
            } => {
                self.provider = provider.clone().unwrap_or_default();
                self.model = model.clone().unwrap_or_default();
                self.effort = effort.clone();
                self.started_ms = e.time_ms;
                self.g = e.g;
            }
            Entry::TurnSettled { usage, .. } => {
                self.turns += 1;
                if let Some(c) = usage.context_tokens {
                    self.context_tokens = c;
                }
                if let Some(w) = usage.context_window {
                    self.context_window = w;
                }
            }
            _ => {}
        }
    }
}

pub struct Store {
    path: PathBuf,
    /// None once the sleeve is removed: a late entry opens the file for its one write.
    file: Option<File>,
    next_seq: u64,
    turn: u64,
    pub stats: Stats,
}

fn ctx(path: &Path) -> impl Fn(io::Error) -> io::Error + '_ {
    move |e| io::Error::new(e.kind(), format!("transcript {}: {e}", path.display()))
}

/// Each entry in the first `len` bytes of the file, in order, until `f` breaks. A torn or
/// corrupt line (a crash mid-write) is skipped.
fn scan(path: &Path, len: u64, mut f: impl FnMut(Line) -> ControlFlow<()>) -> io::Result<()> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(ctx(path)(e)),
    };
    let mut r = BufReader::new(file.take(len));
    let mut line = Vec::new();
    loop {
        line.clear();
        if r.read_until(b'\n', &mut line).map_err(ctx(path))? == 0 {
            return Ok(());
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if let Ok(e) = pt_json::from_bytes::<Line>(&line)
            && f(e).is_break()
        {
            return Ok(());
        }
    }
}

/// Each entry in the first `len` bytes of the file, from the last to the first, until `f`
/// breaks. It skips the lines `scan` skips.
fn rscan(path: &Path, len: u64, mut f: impl FnMut(Line) -> ControlFlow<()>) -> io::Result<()> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(ctx(path)(e)),
    };
    // The bytes from `pos` up to the end of the lines not read yet; the first line in it is
    // whole only at `pos` 0.
    let mut pos = len;
    let mut buf = Vec::new();
    loop {
        if let Some(i) = buf.iter().rposition(|b| *b == b'\n') {
            if let Ok(e) = pt_json::from_bytes::<Line>(&buf[i + 1..])
                && f(e).is_break()
            {
                return Ok(());
            }
            buf.truncate(i);
            continue;
        }
        if pos == 0 {
            if let Ok(e) = pt_json::from_bytes::<Line>(&buf) {
                let _ = f(e);
            }
            return Ok(());
        }
        let n = CHUNK.max(buf.len() as u64).min(pos);
        pos -= n;
        let mut chunk = vec![0; n as usize];
        file.seek(SeekFrom::Start(pos)).map_err(ctx(path))?;
        file.read_exact(&mut chunk).map_err(ctx(path))?;
        chunk.extend_from_slice(&buf);
        buf = chunk;
    }
}

impl Store {
    pub fn open(path: PathBuf) -> io::Result<Store> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(ctx(&path))?;
        }
        let mut stats = Stats::default();
        let (mut next_seq, mut turn) = (1, 0);
        scan(&path, u64::MAX, |e| {
            stats.fold(&e);
            next_seq = e.seq + 1;
            turn = e.turn;
            ControlFlow::Continue(())
        })?;
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(ctx(&path))?;
        // A crash can leave half a line: start the next entry on a fresh one.
        let len = file.metadata().map_err(ctx(&path))?.len();
        if len > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::Start(len - 1)).map_err(ctx(&path))?;
            file.read_exact(&mut last).map_err(ctx(&path))?;
            if last[0] != b'\n' {
                file.write_all(b"\n").map_err(ctx(&path))?;
            }
        }
        Ok(Store {
            path,
            file: Some(file),
            next_seq,
            turn,
            stats,
        })
    }

    /// The seq the next stored entry gets; live deltas of an open block carry it.
    pub fn peek_seq(&self) -> u64 {
        self.next_seq
    }

    /// The turn number the next entry gets (a `turn_started` opens the next one).
    pub fn peek_turn(&self, entry: &Entry) -> u64 {
        match entry {
            Entry::TurnStarted { .. } => self.turn + 1,
            _ => self.turn,
        }
    }

    pub fn append(&mut self, g: u64, time_ms: u64, entry: Entry, note: Note) -> io::Result<Line> {
        let e = Line {
            seq: self.next_seq,
            g,
            time_ms,
            turn: self.peek_turn(&entry),
            harness: note.harness,
            level: note.level,
            entry,
        };
        let mut line = e.to_json().to_string();
        line.push('\n');
        match &mut self.file {
            Some(f) => f.write_all(line.as_bytes()),
            None => OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .and_then(|mut f| f.write_all(line.as_bytes())),
        }
        .map_err(ctx(&self.path))?;
        self.next_seq += 1;
        self.turn = e.turn;
        self.stats.fold(&e);
        Ok(e)
    }

    /// The file's length: every line before it was written whole by an append under this lock,
    /// or is a crash's torn line that reads skip.
    fn len(&self) -> io::Result<u64> {
        match fs::metadata(&self.path) {
            Ok(m) => Ok(m.len()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(ctx(&self.path)(e)),
        }
    }
}

pub struct Transcripts {
    home: Home,
    map: Mutex<HashMap<SleeveId, Arc<Mutex<Store>>>>,
}

impl Transcripts {
    pub fn new(home: Home) -> Transcripts {
        Transcripts {
            home,
            map: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, id: SleeveId) -> io::Result<Arc<Mutex<Store>>> {
        if let Some(s) = lock(&self.map).get(&id) {
            return Ok(s.clone());
        }
        // Opening reads the whole file, so not under the map lock. The loser of a racing open
        // is dropped unused; its only write was a newline, and reads skip an empty line.
        let s = Arc::new(Mutex::new(Store::open(self.home.transcript(id))?));
        Ok(lock(&self.map).entry(id).or_insert(s).clone())
    }

    /// Closes a removed sleeve's file. The store stays, so an entry its ending pump still
    /// records takes the next seq: a store opened again would count seqs from the file while
    /// the old one was still appending.
    pub fn close(&self, id: SleeveId) {
        let Some(store) = lock(&self.map).get(&id).cloned() else {
            return;
        };
        lock(&store).file = None;
    }

    pub fn stats(&self, id: SleeveId) -> Stats {
        self.get(id)
            .map(|s| lock(&s).stats.clone())
            .unwrap_or_default()
    }

    /// Entries with seq greater than `after`, at most `limit`, oldest first.
    pub fn after(&self, id: SleeveId, after: u64, limit: usize) -> io::Result<Vec<Line>> {
        self.page(id, |_| after, limit)
    }

    /// The last `n` entries, oldest first.
    pub fn last(&self, id: SleeveId, n: usize) -> io::Result<Vec<Line>> {
        self.page(id, |next_seq| next_seq.saturating_sub(n as u64 + 1), n)
    }

    /// The whole transcript.
    pub fn all(&self, id: SleeveId) -> io::Result<Vec<Line>> {
        let mut out = Vec::new();
        self.scan(id, |e| {
            out.push(e);
            ControlFlow::Continue(())
        })?;
        Ok(out)
    }

    /// Each entry from the first, until `f` breaks.
    pub fn scan(&self, id: SleeveId, f: impl FnMut(Line) -> ControlFlow<()>) -> io::Result<()> {
        let (path, len, _) = self.cut(id)?;
        scan(&path, len, f)
    }

    /// Each entry from the last back, until `f` breaks.
    pub fn rscan(&self, id: SleeveId, f: impl FnMut(Line) -> ControlFlow<()>) -> io::Result<()> {
        let (path, len, _) = self.cut(id)?;
        rscan(&path, len, f)
    }

    /// The file, its length and the next seq: what a read outside the store lock may see.
    fn cut(&self, id: SleeveId) -> io::Result<(PathBuf, u64, u64)> {
        let store = self.get(id)?;
        let s = lock(&store);
        Ok((s.path.clone(), s.len()?, s.next_seq))
    }

    /// Entries with seq greater than what `after` makes of the next seq, at most `limit`.
    /// Seqs ascend through the file, so a page in its newer half is read from the end back down
    /// to `after`, keeping the oldest `limit`, and an older one from the start up to `limit`.
    fn page(
        &self,
        id: SleeveId,
        after: impl FnOnce(u64) -> u64,
        limit: usize,
    ) -> io::Result<Vec<Line>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (path, len, next_seq) = self.cut(id)?;
        let after = after(next_seq);
        let mut out = VecDeque::new();
        if after.saturating_mul(2) >= next_seq {
            rscan(&path, len, |e| {
                if e.seq <= after {
                    return ControlFlow::Break(());
                }
                if out.len() == limit {
                    out.pop_back();
                }
                out.push_front(e);
                ControlFlow::Continue(())
            })?;
        } else {
            scan(&path, len, |e| {
                if e.seq > after {
                    out.push_back(e);
                }
                if out.len() == limit {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })?;
        }
        Ok(out.into())
    }
}
