//! Monthly, hash-chained audit logs. Entries deliberately have no credential
//! or request-body fields. Appending fails independently of the operation
//! being audited.
//!
//! Every line carries its sequence number, the previous line's hash and its
//! own hash: SHA-256 over the line's exact bytes with an empty `hash` value.
//! The chain runs across monthly files in name order. `head.json` beside the
//! log records the newest hash, so truncating the tail is detected too.
//! Writers in every process serialize on `.lock` in the same directory.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub const PAGE_SCAN_BUDGET: usize = 200_000;
const HEAD: &str = "head.json";
const EMPTY_HASH: &[u8] = b",\"hash\":\"\"}";
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Cursor(String),
    Broken(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "audit log: {e}"),
            Self::Json(e) => write!(f, "audit log: {e}"),
            Self::Cursor(s) => write!(f, "invalid audit cursor: {s:?}"),
            Self::Broken(s) => write!(f, "audit chain broken: {s}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
/// Field order is the line format: `hash` must stay last.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub time: DateTime<Utc>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub token_id: String,
    #[serde(default)]
    pub procedure: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub prev: String,
    #[serde(default)]
    pub hash: String,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            time: std::time::SystemTime::now().into(),
            actor: String::new(),
            token_id: String::new(),
            procedure: String::new(),
            subject: String::new(),
            outcome: String::new(),
            code: String::new(),
            task: String::new(),
            detail: String::new(),
            run_id: String::new(),
            seq: 0,
            prev: String::new(),
            hash: String::new(),
        }
    }
}
/// The newest entry, recorded outside the log.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Head {
    pub seq: u64,
    pub hash: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub entries: u64,
    pub head: Head,
    /// Entries after the recorded head: a writer died between appending and
    /// recording the head. The next write records it.
    pub beyond_head: u64,
}
#[derive(Clone, Debug)]
pub struct Query {
    pub limit: usize,
    pub before: String,
    pub actor: String,
    pub procedure: String,
    pub subject: String,
    pub outcome: String,
    pub run_id: String,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            limit: 50,
            before: String::new(),
            actor: String::new(),
            procedure: String::new(),
            subject: String::new(),
            outcome: String::new(),
            run_id: String::new(),
        }
    }
}
impl Query {
    fn matches(&self, e: &Entry) -> bool {
        let has = |field: &str, want: &str| {
            want.is_empty() || field.to_lowercase().contains(&want.to_lowercase())
        };
        has(&e.actor, &self.actor)
            && has(&e.procedure, &self.procedure)
            && has(&e.subject, &self.subject)
            && (self.outcome.is_empty() || self.outcome == e.outcome)
            && (self.run_id.is_empty() || self.run_id == e.run_id)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: String,
}
struct Tail {
    path: PathBuf,
    len: u64,
    head: Head,
}
#[derive(Default)]
struct Writer {
    lock: Option<File>,
    tail: Option<Tail>,
}
type ErrorHandler = Arc<dyn Fn(&Error) + Send + Sync>;
type RunIds = Arc<dyn Fn(&Entry) -> String + Send + Sync>;
pub struct Audit {
    dir: PathBuf,
    writer: Mutex<Writer>,
    on_error: Option<ErrorHandler>,
    run_ids: Option<RunIds>,
}
impl Audit {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            writer: Mutex::new(Writer::default()),
            on_error: None,
            run_ids: None,
        }
    }
    pub fn with_on_error(mut self, handler: impl Fn(&Error) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(handler));
        self
    }
    /// Fills `run_id` of entries written without one.
    pub fn with_run_ids(
        mut self,
        resolve: impl Fn(&Entry) -> String + Send + Sync + 'static,
    ) -> Self {
        self.run_ids = Some(Arc::new(resolve));
        self
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    /// Best-effort append; errors go to the configured handler, outside the writer lock.
    pub fn write(&self, entry: Entry) {
        if let Err(e) = self.try_write(entry) {
            if let Some(handler) = &self.on_error {
                handler(&e)
            }
        }
    }
    pub fn try_write(&self, mut entry: Entry) -> Result<Head> {
        if entry.run_id.is_empty() {
            if let Some(resolve) = &self.run_ids {
                entry.run_id = resolve(&entry);
            }
        }
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if writer.lock.is_none() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&self.dir)?;
            writer.lock = Some(
                private_options(OpenOptions::new().create(true).append(true))
                    .open(self.dir.join(".lock"))?,
            );
        }
        let lock = writer.lock.as_ref().unwrap();
        lock.lock()?;
        let result = self.append(&mut writer.tail, entry);
        let _ = writer.lock.as_ref().unwrap().unlock();
        if result.is_err() {
            writer.tail = None;
        }
        result
    }
    fn append(&self, cache: &mut Option<Tail>, mut entry: Entry) -> Result<Head> {
        let month = self
            .dir
            .join(format!("audit-{}.jsonl", entry.time.format("%Y-%m")));
        let newest = files(&self.dir)?.pop();
        // A clock that steps back across a month keeps appending to the
        // newest file, so file order stays chain order.
        let path = match newest {
            Some(newest) if newest > month => newest,
            _ => month,
        };
        let mut file =
            private_options(OpenOptions::new().create(true).read(true).append(true)).open(&path)?;
        let len = file.metadata()?.len();
        let prior = match cache.take() {
            Some(tail) if tail.path == path && tail.len == len => tail.head,
            _ => self.recover_tail(&mut file, &path, len)?,
        };
        entry.seq = prior.seq + 1;
        entry.prev = prior.hash;
        let line = seal(&mut entry)?;
        file.write_all(&line)?;
        let head = Head {
            seq: entry.seq,
            hash: entry.hash,
        };
        let temp = self.dir.join(".head.tmp");
        fs::write(&temp, serde_json::to_vec(&head)?)?;
        fs::rename(&temp, self.dir.join(HEAD))?;
        *cache = Some(Tail {
            path,
            len: len + line.len() as u64,
            head: head.clone(),
        });
        Ok(head)
    }
    /// The chain continues from the last complete line on disk, which
    /// another process may have written. A torn final line from a crash is
    /// cut off; an empty target file continues from the previous month.
    fn recover_tail(&self, file: &mut File, path: &Path, len: u64) -> Result<Head> {
        let (complete, last) = last_line(file, len)?;
        if complete < len {
            file.set_len(complete)?;
        }
        if let Some(line) = last {
            return chained(&line).ok_or_else(|| {
                Error::Broken(format!(
                    "{}: the last line is not a chained entry",
                    path.display()
                ))
            });
        }
        let older = files(&self.dir)?.into_iter().rfind(|f| f.as_path() < path);
        if let Some(older) = older {
            let mut file = File::open(&older)?;
            let len = file.metadata()?.len();
            if let (_, Some(line)) = last_line(&mut file, len)? {
                return chained(&line).ok_or_else(|| {
                    Error::Broken(format!(
                        "{}: the last line is not a chained entry",
                        older.display()
                    ))
                });
            }
        }
        Ok(read_head(&self.dir)?.unwrap_or_default())
    }
    pub fn page(&self, query: Query) -> Result<Page> {
        read_page(&self.dir, query)
    }
    pub fn tail(&self, limit: usize) -> Result<Vec<Entry>> {
        Ok(self
            .page(Query {
                limit,
                ..Default::default()
            })?
            .entries)
    }
    pub fn head(&self) -> Result<Option<Head>> {
        read_head(&self.dir)
    }
    pub fn verify(&self) -> Result<Report> {
        verify(&self.dir)
    }
}
fn private_options(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
/// Serializes the entry with its hash, which is computed over the same bytes
/// with an empty hash value.
fn seal(entry: &mut Entry) -> Result<Vec<u8>> {
    entry.hash.clear();
    let mut line = serde_json::to_vec(&*entry)?;
    entry.hash = format!("{:x}", Sha256::digest(&line));
    line.truncate(line.len() - 2);
    line.extend_from_slice(entry.hash.as_bytes());
    line.extend_from_slice(b"\"}\n");
    Ok(line)
}
/// Checks a line's own hash and returns its entry; the chain is checked by
/// the caller.
fn unseal(line: &[u8]) -> std::result::Result<Entry, &'static str> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let entry: Entry = serde_json::from_slice(line).map_err(|_| "not a JSON entry")?;
    let suffix = line
        .len()
        .checked_sub(66)
        .filter(|&at| line[..at].ends_with(b",\"hash\":\"") && line.ends_with(b"\"}"))
        .ok_or("no trailing hash")?;
    let digest = &line[suffix..line.len() - 2];
    if !digest
        .iter()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        || entry.hash.as_bytes() != digest
    {
        return Err("malformed hash");
    }
    let mut sealed = line[..suffix - 9].to_vec();
    sealed.extend_from_slice(EMPTY_HASH);
    if format!("{:x}", Sha256::digest(&sealed)).as_bytes() != digest {
        return Err("hash does not match the entry");
    }
    Ok(entry)
}
fn chained(line: &[u8]) -> Option<Head> {
    unseal(line).ok().map(|entry| Head {
        seq: entry.seq,
        hash: entry.hash,
    })
}
/// Returns the length up to the last newline and the last complete line.
fn last_line(file: &mut File, len: u64) -> io::Result<(u64, Option<Vec<u8>>)> {
    let mut end = len;
    let mut buffer = Vec::new();
    let mut start = len;
    loop {
        let step = start.min(64 << 10);
        if step == 0 {
            break;
        }
        start -= step;
        let mut chunk = vec![0; step as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut chunk)?;
        chunk.extend_from_slice(&buffer);
        buffer = chunk;
        if end == len {
            match buffer.iter().rposition(|&b| b == b'\n') {
                Some(at) => {
                    end = start + at as u64 + 1;
                    buffer.truncate(at + 1);
                }
                None => continue,
            }
        }
        let body = &buffer[..buffer.len() - 1];
        if let Some(at) = body.iter().rposition(|&b| b == b'\n') {
            return Ok((end, Some(buffer[at + 1..].to_vec())));
        }
        if start == 0 {
            return Ok((end, Some(buffer)));
        }
    }
    Ok((if end == len { 0 } else { end }, None))
}
pub fn read_head(dir: impl AsRef<Path>) -> Result<Option<Head>> {
    match fs::read(dir.as_ref().join(HEAD)) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
/// Walks the whole chain and checks it against the recorded head. The first
/// break is the error, naming the file and line.
pub fn verify(dir: impl AsRef<Path>) -> Result<Report> {
    let dir = dir.as_ref();
    let head = read_head(dir)?;
    let mut last = Head::default();
    let mut at_head = None;
    for path in files(dir)? {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut reader = BufReader::with_capacity(64 << 10, File::open(&path)?);
        let mut number = 0;
        loop {
            let mut line = Vec::new();
            if reader.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            number += 1;
            let broken = |reason: &str| Error::Broken(format!("{name}:{number}: {reason}"));
            if !line.ends_with(b"\n") {
                return Err(broken("incomplete final line"));
            }
            let entry = unseal(&line).map_err(broken)?;
            if entry.seq != last.seq + 1 {
                return Err(broken(&format!(
                    "sequence {} follows {}",
                    entry.seq, last.seq
                )));
            }
            if entry.prev != last.hash {
                return Err(broken("previous hash does not match the line before"));
            }
            if head.as_ref().is_some_and(|head| head.seq == entry.seq) {
                at_head = Some(entry.hash.clone());
            }
            last = Head {
                seq: entry.seq,
                hash: entry.hash,
            };
        }
    }
    let Some(head) = head else {
        if last.seq == 0 {
            return Ok(Report::default());
        }
        return Err(Error::Broken(format!(
            "{HEAD} is missing for {} entries",
            last.seq
        )));
    };
    match at_head {
        Some(hash) if hash == head.hash => Ok(Report {
            entries: last.seq,
            beyond_head: last.seq - head.seq,
            head,
        }),
        Some(_) => Err(Error::Broken(format!(
            "entry {} differs from the recorded head",
            head.seq
        ))),
        None => Err(Error::Broken(format!(
            "the log ends at entry {} but the recorded head is entry {}",
            last.seq, head.seq
        ))),
    }
}
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut files = Vec::new();
    for e in entries {
        let e = e?;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("audit-") && name.ends_with(".jsonl") {
            files.push(e.path())
        }
    }
    files.sort();
    Ok(files)
}
/// Results follow append order, newest first, across monthly files. Complete
/// malformed lines are skipped; a partial final line is left for a later read.
/// The scan budget is checked between months, matching the operational model:
/// one large monthly file can require more than PAGE_SCAN_BUDGET lines.
pub fn read_page(dir: impl AsRef<Path>, query: Query) -> Result<Page> {
    if query.limit == 0 {
        return Ok(Page::default());
    }
    let dir = dir.as_ref();
    let files = files(dir)?;
    let mut index = files.len().checked_sub(1);
    let mut bound = None;
    if !query.before.is_empty() {
        let cursor_error = || Error::Cursor(query.before.clone());
        let (name, line) = query.before.split_once(':').ok_or_else(cursor_error)?;
        if name.is_empty()
            || name.contains(['/', '\\', ':'])
            || Path::new(name).file_name().and_then(|n| n.to_str()) != Some(name)
        {
            return Err(cursor_error());
        }
        index = Some(
            files
                .iter()
                .position(|f| f.file_name().and_then(|s| s.to_str()) == Some(name))
                .ok_or_else(cursor_error)?,
        );
        if line != "*" {
            if line.is_empty() || !line.bytes().all(|c| c.is_ascii_digit()) {
                return Err(cursor_error());
            }
            bound = Some(line.parse::<usize>().map_err(|_| cursor_error())?);
        }
    }
    let mut out = Page::default();
    let mut scanned = 0usize;
    while let Some(i) = index {
        let Scan {
            entries: keep,
            lines,
            more,
        } = scan_file(&files[i], bound, query.limit - out.entries.len(), &query)?;
        if more {
            out.next = format!(
                "{}:{}",
                files[i].file_name().unwrap().to_string_lossy(),
                keep.front().unwrap().0
            )
        }
        out.entries.extend(keep.into_iter().rev().map(|(_, e)| e));
        if more {
            return Ok(out);
        }
        scanned = scanned.saturating_add(lines);
        bound = None;
        index = i.checked_sub(1);
        if let Some(older) = index {
            if out.entries.len() == query.limit || scanned >= PAGE_SCAN_BUDGET {
                out.next = format!("{}:*", files[older].file_name().unwrap().to_string_lossy());
                return Ok(out);
            }
        }
    }
    Ok(out)
}
struct Scan {
    entries: VecDeque<(usize, Entry)>,
    lines: usize,
    more: bool,
}
fn scan_file(path: &Path, bound: Option<usize>, need: usize, query: &Query) -> Result<Scan> {
    let mut reader = BufReader::with_capacity(64 << 10, File::open(path)?);
    let mut keep = VecDeque::new();
    let mut dropped = false;
    let mut count = 0;
    loop {
        if bound.is_some_and(|n| count >= n) {
            break;
        }
        let mut line = Vec::new();
        if reader.read_until(b'\n', &mut line)? == 0 || !line.ends_with(b"\n") {
            break;
        }
        let index = count;
        count += 1;
        if let Ok(entry) = serde_json::from_slice::<Entry>(&line) {
            if query.matches(&entry) {
                keep.push_back((index, entry));
                if keep.len() > need {
                    keep.pop_front();
                    dropped = true;
                }
            }
        }
    }
    Ok(Scan {
        entries: keep,
        lines: count,
        more: dropped,
    })
}
