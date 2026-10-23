//! Monthly audit logs. Entries deliberately have no credential or request-body
//! fields. Appending fails independently of the operation being audited.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub const PAGE_SCAN_BUDGET: usize = 200_000;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Cursor(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "audit log: {e}"),
            Self::Json(e) => write!(f, "audit log: {e}"),
            Self::Cursor(s) => write!(f, "invalid audit cursor: {s:?}"),
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub time: DateTime<Utc>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub token_id: String,
    #[serde(default)]
    pub tailnet: String,
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
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            time: std::time::SystemTime::now().into(),
            actor: String::new(),
            token_id: String::new(),
            tailnet: String::new(),
            procedure: String::new(),
            subject: String::new(),
            outcome: String::new(),
            code: String::new(),
            task: String::new(),
            detail: String::new(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Query {
    pub limit: usize,
    pub before: String,
    pub actor: String,
    pub procedure: String,
    pub subject: String,
    pub outcome: String,
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
        }
    }
}
fn lower(s: &str) -> String {
    s.chars()
        .map(|c| c.to_lowercase().next().unwrap())
        .collect()
}
impl Query {
    fn matches(&self, e: &Entry) -> bool {
        let has = |field: &str, want: &str| want.is_empty() || lower(field).contains(&lower(want));
        has(&e.actor, &self.actor)
            && has(&e.procedure, &self.procedure)
            && has(&e.subject, &self.subject)
            && (self.outcome.is_empty() || self.outcome == e.outcome)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: String,
}
struct Writer {
    file: Option<File>,
    month: String,
}
type ErrorHandler = Arc<dyn Fn(&Error) + Send + Sync>;
pub struct Audit {
    dir: PathBuf,
    writer: Mutex<Writer>,
    on_error: Option<ErrorHandler>,
}
impl Audit {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            writer: Mutex::new(Writer {
                file: None,
                month: String::new(),
            }),
            on_error: None,
        }
    }
    pub fn with_on_error(mut self, handler: impl Fn(&Error) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(handler));
        self
    }
    /// Best-effort append; errors go to the configured handler, outside the writer lock.
    pub fn write(&self, entry: Entry) {
        if let Err(e) = self.try_write(&entry) {
            if let Some(handler) = &self.on_error {
                handler(&e)
            }
        }
    }
    /// Explicit error-returning variant for callers that need to inspect log failures.
    pub fn try_write(&self, entry: &Entry) -> Result<()> {
        let mut line = marshal_entry(entry)?;
        line.push(b'\n');
        let month = entry.time.format("%Y-%m").to_string();
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if writer.file.is_none() || writer.month != month {
            writer.file = None;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&self.dir)?;
            let mut options = OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            writer.file = Some(options.open(self.dir.join(format!("audit-{month}.jsonl")))?);
            writer.month = month;
        }
        writer.file.as_mut().unwrap().write_all(&line)?;
        Ok(())
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
    /// Releases the descriptor; a later write may reopen it, like monthly rotation.
    pub fn close(&self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = writer.file.take() {
            f.sync_all()?;
        }
        Ok(())
    }
}
pub fn marshal_entry(entry: &Entry) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(entry)?)
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
