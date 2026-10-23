//! notes.json storage with atomic replacement and OS writer locks.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    NotFound(String),
    Io(io::Error),
    Json(serde_json::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "invalid argument: {s}"),
            Self::NotFound(s) => write!(f, "note not found: {s:?}"),
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => write!(f, "parse notes.json: {e}"),
        }
    }
}
impl std::error::Error for Error {}
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
type Result<T> = std::result::Result<T, Error>;
fn empty(s: &str) -> bool {
    s.is_empty()
}
fn no(b: &bool) -> bool {
    !*b
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "empty")]
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "no")]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "empty")]
    pub project: String,
    #[serde(default, skip_serializing_if = "empty")]
    pub source: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "empty")]
    pub done_by: String,
    #[serde(default, skip_serializing_if = "empty")]
    pub resolution: String,
}
#[derive(Default, Clone, Debug)]
pub struct NewNote {
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub project: String,
    pub source: String,
}
#[derive(Default, Clone, Debug)]
pub struct Patch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub pinned: Option<bool>,
    pub project: Option<String>,
    pub state: Option<String>,
    pub resolution: Option<String>,
    pub done_by: String,
}
#[derive(Default, Clone, Debug)]
pub struct Filter {
    pub project: String,
    pub query: String,
    pub state: String,
}
#[derive(Serialize, Deserialize)]
struct Document {
    notes: Vec<Note>,
}
// Lowercases each scalar on its own (no final-sigma context), so a search
// key never depends on its neighbours.
fn simple_lower(s: &str) -> String {
    s.chars()
        .map(|c| c.to_lowercase().next().unwrap())
        .collect()
}
fn invalid<T>(s: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(s.into()))
}
fn tag(s: &str, label: &str, max: usize) -> Result<String> {
    let s = s.trim();
    if s.contains(['\r', '\n']) {
        return invalid(format!("{label} must be one line of text"));
    }
    if s.chars().count() > max {
        return invalid(format!("{label} is longer than {max} characters"));
    }
    Ok(s.into())
}
fn body(s: &str) -> Result<String> {
    let s = s.replace("\r\n", "\n");
    if s.contains('\0') {
        return invalid("body must be text");
    }
    if s.len() > 256 << 10 {
        return invalid("body is larger than 256 KiB");
    }
    Ok(s)
}
fn nonempty(n: &Note) -> Result<()> {
    if n.title.is_empty() && n.body.trim().is_empty() {
        invalid("a note needs a title or a body")
    } else {
        Ok(())
    }
}
impl NewNote {
    fn build(self, id: String, now: DateTime<Utc>) -> Result<Note> {
        let n = Note {
            id,
            title: tag(&self.title, "title", 200)?,
            body: body(&self.body)?,
            project: tag(&self.project, "project", 100)?,
            source: tag(&self.source, "source", 100)?,
            pinned: self.pinned,
            created_at: now,
            updated_at: now,
            state: "open".into(),
            done_at: None,
            done_by: String::new(),
            resolution: String::new(),
        };
        nonempty(&n)?;
        Ok(n)
    }
}
impl Patch {
    fn apply(self, n: &Note, now: DateTime<Utc>) -> Result<(Note, bool)> {
        let mut out = n.clone();
        if let Some(s) = self.title {
            out.title = tag(&s, "title", 200)?
        }
        if let Some(s) = self.body {
            out.body = body(&s)?
        }
        if let Some(s) = self.project {
            out.project = tag(&s, "project", 100)?
        }
        if let Some(p) = self.pinned {
            out.pinned = p
        }
        if let Some(s) = self.state {
            match s.as_str() {
                "open" => {
                    out.state = s;
                    out.done_at = None;
                    out.done_by.clear();
                    out.resolution.clear()
                }
                "done" => {
                    if out.state != "done" {
                        out.state = s;
                        out.done_at = Some(now);
                        out.done_by = self.done_by
                    }
                }
                _ => return invalid("state must be open or done"),
            }
        }
        if let Some(s) = self.resolution {
            if out.state != "done" {
                return invalid("resolution requires a done note");
            }
            out.resolution = tag(&s, "resolution", 200)?
        }
        nonempty(&out)?;
        let changed = out != *n;
        if changed {
            out.updated_at = now
        }
        Ok((out, changed))
    }
}

pub struct Store {
    dir: PathBuf,
    mu: Mutex<()>,
}
impl Store {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            dir: home.into(),
            mu: Mutex::new(()),
        }
    }
    pub fn path(&self) -> PathBuf {
        self.dir.join("notes.json")
    }
    fn snapshot(&self) -> Result<Vec<Note>> {
        let bytes = match retry(|| fs::read(self.path())) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
        };
        let d: Document = serde_json::from_slice(&bytes)?;
        if let Some(n) = d
            .notes
            .iter()
            .find(|n| n.state != "open" && n.state != "done")
        {
            return invalid(format!("note {:?} has state {:?}", n.id, n.state));
        }
        Ok(d.notes)
    }
    pub fn list(&self, f: Filter) -> Result<Vec<Note>> {
        if !["", "open", "done", "all"].contains(&f.state.as_str()) {
            return invalid("state must be open, done or all");
        }
        let _guard = self.mu.lock().unwrap_or_else(|e| e.into_inner());
        let state = if f.state.is_empty() { "open" } else { &f.state };
        let q = simple_lower(&f.query);
        let mut notes: Vec<_> = self
            .snapshot()?
            .into_iter()
            .filter(|n| {
                (state == "all" || n.state == state)
                    && (f.project.is_empty() || n.project == f.project)
                    && (q.is_empty()
                        || simple_lower(&n.title).contains(&q)
                        || simple_lower(&n.body).contains(&q))
            })
            .collect();
        notes.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.updated_at.cmp(&a.updated_at))
                .then(b.created_at.cmp(&a.created_at))
        });
        Ok(notes)
    }
    pub fn get(&self, id: &str) -> Result<Note> {
        let _guard = self.mu.lock().unwrap_or_else(|e| e.into_inner());
        self.snapshot()?
            .into_iter()
            .find(|n| n.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))
    }
    fn modify<T>(&self, change: impl FnOnce(&mut Vec<Note>) -> Result<(T, bool)>) -> Result<T> {
        let _guard = self.mu.lock().unwrap_or_else(|e| e.into_inner());
        let target = fs::canonicalize(self.path()).unwrap_or_else(|_| self.path());
        let _lock = WriterLock::acquire(&target)?;
        let mut notes = self.snapshot()?;
        let (out, changed) = change(&mut notes)?;
        if changed {
            self.write(&target, notes)?
        }
        Ok(out)
    }
    pub fn create(&self, n: NewNote) -> Result<Note> {
        self.modify(|notes| {
            let id = new_id(notes)?;
            let out = n.build(id, DateTime::<Utc>::from(std::time::SystemTime::now()))?;
            notes.push(out.clone());
            Ok((out, true))
        })
    }
    pub fn update(&self, id: &str, p: Patch) -> Result<(Note, bool)> {
        self.modify(|notes| {
            let n = notes
                .iter_mut()
                .find(|n| n.id == id)
                .ok_or_else(|| Error::NotFound(id.into()))?;
            let (out, changed) = p.apply(n, DateTime::<Utc>::from(std::time::SystemTime::now()))?;
            *n = out.clone();
            Ok(((out, changed), changed))
        })
    }
    pub fn delete(&self, id: &str) -> Result<Note> {
        self.modify(|notes| {
            let i = notes
                .iter()
                .position(|n| n.id == id)
                .ok_or_else(|| Error::NotFound(id.into()))?;
            Ok((notes.remove(i), true))
        })
    }
    fn write(&self, target: &Path, notes: Vec<Note>) -> Result<()> {
        private_dir(target.parent().unwrap())?;
        let mut bytes = serde_json::to_vec_pretty(&Document { notes })?;
        bytes.push(b'\n');
        let mode = 0o600;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(target)
                .map(|m| m.permissions().mode() & 0o777)
                .unwrap_or(mode)
        };
        protectorate_fsutil::write_file(target, &bytes, mode)?;
        Ok(())
    }
}
fn new_id(notes: &[Note]) -> Result<String> {
    for n in 2..=8 {
        for _ in 0..64 {
            let mut bytes = vec![0; n];
            getrandom::getrandom(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
            let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            if !notes.iter().any(|n| n.id == id) {
                return Ok(id);
            }
        }
    }
    Err(io::Error::other("generate id: no free id").into())
}
struct WriterLock(File);
impl WriterLock {
    fn acquire(target: &Path) -> Result<Self> {
        let dir = target.parent().unwrap();
        private_dir(dir)?;
        let name = dir.join(format!(
            ".{}.lock",
            target.file_name().unwrap().to_string_lossy()
        ));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let f = options.open(name)?;
        let start = Instant::now();
        loop {
            match f.try_lock() {
                Ok(()) => return Ok(Self(f)),
                Err(fs::TryLockError::WouldBlock) => {
                    if start.elapsed() >= Duration::from_secs(10) {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "lock notes: timed out",
                        )
                        .into());
                    }
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(fs::TryLockError::Error(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(fs::TryLockError::Error(e)) => return Err(e.into()),
            }
        }
    }
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn private_dir(path: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}
fn retry<T>(f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    protectorate_fsutil::retry(f)
}
