//! Per-sleeve transcript: an append-only JSONL file of normalized entries with a monotonic seq,
//! plus an in-memory tail. Streaming text deltas never reach the file; the final block does.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pt_core::SleeveId;
use pt_drivers::{Entry, Event};
use pt_json::ToJson;

use crate::home::Home;

const TAIL: usize = 4096;

/// Facts the sleeve listing shows, folded from the entries as they arrive.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub turns: u64,
    pub context_tokens: u64,
    pub context_window: u64,
    pub provider: String,
    pub model: String,
    pub started_ms: u64,
    pub g: u64,
}

impl Stats {
    fn fold(&mut self, e: &Event) {
        match &e.entry {
            Entry::SessionInfo {
                model, provider, ..
            } => {
                self.provider = provider.clone().unwrap_or_default();
                self.model = model.clone().unwrap_or_default();
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
    file: File,
    next_seq: u64,
    tail: VecDeque<Event>,
    pub stats: Stats,
}

fn ctx(path: &std::path::Path) -> impl Fn(io::Error) -> io::Error + '_ {
    move |e| io::Error::new(e.kind(), format!("transcript {}: {e}", path.display()))
}

/// Every entry in the file, skipping a torn or corrupt line (a crash mid-write).
fn read_all(path: &std::path::Path) -> io::Result<Vec<Event>> {
    let f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ctx(path)(e)),
    };
    let mut out = Vec::new();
    for line in BufReader::new(f).split(b'\n') {
        let line = line.map_err(ctx(path))?;
        if let Ok(e) = pt_json::from_bytes::<Event>(&line) {
            out.push(e);
        }
    }
    Ok(out)
}

impl Store {
    pub fn open(path: PathBuf) -> io::Result<Store> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(ctx(&path))?;
        }
        let all = read_all(&path)?;
        let mut stats = Stats::default();
        for e in &all {
            stats.fold(e);
        }
        let next_seq = all.last().map_or(1, |e| e.seq + 1);
        let tail: VecDeque<Event> = all.into_iter().rev().take(TAIL).rev().collect();
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
            file,
            next_seq,
            tail,
            stats,
        })
    }

    /// The seq the next stored entry gets; live deltas of an open block carry it.
    pub fn peek_seq(&self) -> u64 {
        self.next_seq
    }

    pub fn append(&mut self, g: u64, time_ms: u64, entry: Entry) -> io::Result<Event> {
        let e = Event {
            seq: self.next_seq,
            g,
            time_ms,
            entry,
        };
        let mut line = e.to_json().to_string();
        line.push('\n');
        self.file
            .write_all(line.as_bytes())
            .map_err(ctx(&self.path))?;
        self.next_seq += 1;
        self.stats.fold(&e);
        if self.tail.len() == TAIL {
            self.tail.pop_front();
        }
        self.tail.push_back(e.clone());
        Ok(e)
    }

    /// Entries with seq greater than `after`, at most `limit`.
    pub fn after(&self, after: u64, limit: usize) -> io::Result<Vec<Event>> {
        let in_tail = self.tail.front().is_none_or(|f| f.seq <= after + 1);
        if in_tail {
            return Ok(self
                .tail
                .iter()
                .filter(|e| e.seq > after)
                .take(limit)
                .cloned()
                .collect());
        }
        Ok(read_all(&self.path)?
            .into_iter()
            .filter(|e| e.seq > after)
            .take(limit)
            .collect())
    }

    /// The whole transcript, for the continuation brief.
    pub fn all(&self) -> io::Result<Vec<Event>> {
        if self.tail.front().is_none_or(|f| f.seq == 1) {
            return Ok(self.tail.iter().cloned().collect());
        }
        read_all(&self.path)
    }
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
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
        let mut map = lock(&self.map);
        if let Some(s) = map.get(&id) {
            return Ok(s.clone());
        }
        let s = Arc::new(Mutex::new(Store::open(self.home.transcript(id))?));
        map.insert(id, s.clone());
        Ok(s)
    }

    pub fn stats(&self, id: SleeveId) -> Stats {
        self.get(id)
            .map(|s| lock(&s).stats.clone())
            .unwrap_or_default()
    }
}
