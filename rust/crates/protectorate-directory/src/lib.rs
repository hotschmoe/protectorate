//! Local workspace discovery, dependency graph and release observations.
mod manifests;
mod refs;
pub use manifests::{read_manifests, Manifest};
pub use refs::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Git(String),
    NotFound(String),
    Ambiguous { query: String, matches: Vec<String> },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Git(e) => write!(f, "git: {e}"),
            Self::NotFound(q) => write!(f, "no such project: {q}"),
            Self::Ambiguous { query, matches } => {
                write!(f, "ambiguous project {query}: {}", matches.join(", "))
            }
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
pub type Result<T> = std::result::Result<T, Error>;
/// Only metadata commands belong here: worktree commands can execute configured filters.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String> {
    if !matches!(
        args.first(),
        Some(
            &("config"
                | "rev-parse"
                | "symbolic-ref"
                | "for-each-ref"
                | "merge-base"
                | "cat-file"
                | "log"
                | "rev-list"
                | "diff")
        )
    ) {
        return Err(Error::Git("unsupported metadata command".into()));
    }
    let mut c = Command::new("git");
    c.arg("-C").arg(dir).arg("--no-pager");
    for setting in [
        "core.fsmonitor=false",
        "core.hooksPath=/dev/null",
        "core.pager=cat",
        "diff.external=",
        "core.sshCommand=false",
        "log.showSignature=false",
    ] {
        c.arg("-c").arg(setting);
    }
    c.arg("-c")
        .arg(format!("safe.directory={}", dir.display()))
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_EXTERNAL_DIFF", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).map(|_| b)
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).map(|_| b)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Git("metadata command timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let out = out
        .join()
        .map_err(|_| Error::Git("output reader failed".into()))??;
    let err = err
        .join()
        .map_err(|_| Error::Git("error reader failed".into()))??;
    if !status.success() {
        return Err(Error::Git(String::from_utf8_lossy(&err).trim().into()));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub path: PathBuf,
    pub remote: String,
    pub branch: String,
    pub head: String,
    pub manifests: Vec<Manifest>,
}
pub fn inspect(dir: impl AsRef<Path>) -> Workspace {
    let dir = dir.as_ref();
    Workspace {
        path: dir.into(),
        remote: normalize_remote(
            &git(dir, &["config", "--get", "remote.origin.url"]).unwrap_or_default(),
        ),
        head: git(dir, &["rev-parse", "HEAD"])
            .unwrap_or_default()
            .trim()
            .into(),
        branch: git(dir, &["symbolic-ref", "--short", "-q", "HEAD"])
            .unwrap_or_default()
            .trim()
            .into(),
        manifests: read_manifests(dir),
    }
}
fn clean_path(s: &str) -> String {
    let mut out = Vec::new();
    for c in s.split('/') {
        match c {
            "" | "." => {}
            ".." if out.last().is_some_and(|x| *x != "..") => {
                out.pop();
            }
            ".." if s.starts_with('/') => {}
            _ => out.push(c),
        }
    }
    let p = out.join("/");
    if s.starts_with('/') {
        format!("/{p}")
    } else if p.is_empty() {
        ".".into()
    } else {
        p
    }
}
pub fn normalize_remote(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let local = |s: &str| clean_path(s.trim_end_matches(".git"));
    if raw.starts_with('/')
        || raw.starts_with('.')
        || raw.as_bytes().get(1) == Some(&b':')
            && raw
                .as_bytes()
                .get(2)
                .is_some_and(|b| *b == b'\\' || *b == b'/')
    {
        return local(raw);
    }
    let (host, path) = if let Some((scheme, rest)) = raw.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return local(rest.strip_prefix("localhost").unwrap_or(rest));
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let authority = authority.rsplit('@').next().unwrap_or(authority);
        let host = if authority.starts_with('[') {
            authority
                .split(']')
                .next()
                .unwrap_or(authority)
                .trim_start_matches('[')
        } else {
            authority.split(':').next().unwrap_or(authority)
        };
        (host, path)
    } else {
        let rest = raw.rsplit('@').next().unwrap_or(raw);
        match rest.split_once(':') {
            Some(x) => x,
            None => return raw.trim_end_matches(".git").to_lowercase(),
        }
    };
    format!(
        "{}/{}",
        host,
        path.trim_matches('/')
            .trim_end_matches(".git")
            .trim_matches('/')
    )
    .to_lowercase()
}
pub fn project_id(w: &Workspace) -> String {
    if w.remote.is_empty() {
        format!("path:{}", clean_path(&w.path.to_string_lossy()))
    } else {
        w.remote.clone()
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Owner {
    pub sleeve: String,
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub display: String,
    #[serde(default)]
    pub remote: String,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default)]
    pub refs: Option<RefState>,
    #[serde(skip)]
    pub owners: Vec<Owner>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub project: String,
    pub via: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub project: Project,
    pub depends_on: Vec<Edge>,
    pub dependents: Vec<Edge>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub sleeve: String,
    pub workspace: Workspace,
}
#[derive(Default, Serialize, Deserialize)]
struct Disk {
    projects: Vec<Project>,
}
pub struct Directory {
    file: PathBuf,
    projects: Mutex<BTreeMap<String, Project>>,
    writer: Mutex<()>,
}
impl Directory {
    pub fn open(file: impl Into<PathBuf>) -> Result<Self> {
        let file = file.into();
        let disk: Disk = match fs::read(&file) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Disk::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            file,
            writer: Mutex::new(()),
            projects: Mutex::new(
                disk.projects
                    .into_iter()
                    .map(|mut p| {
                        p.provides.sort();
                        p.provides.dedup();
                        p.deps.sort();
                        p.deps.dedup();
                        (p.id.clone(), p)
                    })
                    .collect(),
            ),
        })
    }
    fn persist(&self, p: &BTreeMap<String, Project>) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&Disk {
            projects: p.values().cloned().collect(),
        })?;
        if fs::read(&self.file).ok().as_deref() == Some(&bytes) {
            return Ok(());
        }
        let parent = self
            .file
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(parent)?;
        protectorate_fsutil::write_file(&self.file, &bytes, 0o600)?;
        Ok(())
    }
    pub fn update(&self, scans: &[Scan]) -> Result<()> {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = self
            .projects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for p in next.values_mut() {
            p.owners.clear()
        }
        let mut groups: BTreeMap<String, Vec<&Scan>> = BTreeMap::new();
        for s in scans {
            groups.entry(project_id(&s.workspace)).or_default().push(s)
        }
        for (id, mut group) in groups {
            group.sort_by(|a, b| a.sleeve.cmp(&b.sleeve));
            let remote = group[0].workspace.remote.clone();
            let name = if remote.is_empty() {
                group[0]
                    .workspace
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                remote.rsplit('/').next().unwrap_or("").into()
            };
            let segments: Vec<_> = remote.split('/').collect();
            let display = if segments.len() >= 3 {
                segments[segments.len() - 2..].join("/")
            } else if remote.is_empty() {
                name.clone()
            } else {
                remote.clone()
            };
            let p = next.entry(id.clone()).or_insert_with(|| Project {
                id,
                name,
                display,
                remote,
                ..Default::default()
            });
            let mut provides = BTreeSet::new();
            let mut deps = BTreeSet::new();
            for s in group {
                let w = &s.workspace;
                p.owners.push(Owner {
                    sleeve: s.sleeve.clone(),
                    path: w.path.clone(),
                    branch: w.branch.clone(),
                    head: w.head.clone(),
                });
                for m in &w.manifests {
                    if !m.name.is_empty() {
                        provides.insert(m.name.clone());
                    }
                    deps.extend(m.deps.iter().cloned());
                }
            }
            deps.retain(|n| !provides.contains(n));
            p.provides = provides.into_iter().collect();
            p.deps = deps.into_iter().collect();
        }
        self.persist(&next)?;
        *self.projects.lock().unwrap_or_else(|e| e.into_inner()) = next;
        Ok(())
    }
    pub fn set_refs(&self, id: &str, refs: RefState) -> Result<()> {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = self
            .projects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        next.get_mut(id)
            .ok_or_else(|| Error::NotFound(id.into()))?
            .refs = Some(refs);
        self.persist(&next)?;
        *self.projects.lock().unwrap_or_else(|e| e.into_inner()) = next;
        Ok(())
    }
    pub fn list(&self) -> Vec<View> {
        let p = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        p.values().map(|v| view(v, &p)).collect()
    }
    pub fn get(&self, query: &str) -> Result<View> {
        let p = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        let q = query.trim();
        if let Some(v) = p.get(q).or_else(|| p.get(&q.to_lowercase())) {
            return Ok(view(v, &p));
        }
        for field in [false, true] {
            let hits: Vec<_> = p
                .values()
                .filter(|v| if field { &v.name } else { &v.display }.eq_ignore_ascii_case(q))
                .collect();
            match hits.len() {
                0 => {}
                1 => return Ok(view(hits[0], &p)),
                _ => {
                    return Err(Error::Ambiguous {
                        query: q.into(),
                        matches: hits.iter().map(|p| p.id.clone()).collect(),
                    })
                }
            }
        }
        Err(Error::NotFound(q.into()))
    }
    pub fn projects_of(&self, sleeve: &str) -> Vec<String> {
        self.projects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|p| p.owners.iter().any(|o| o.sleeve == sleeve))
            .map(|p| p.id.clone())
            .collect()
    }
}
fn view(p: &Project, all: &BTreeMap<String, Project>) -> View {
    let edges = |reverse: bool| {
        all.values()
            .filter(|q| q.id != p.id)
            .filter_map(|q| {
                let (a, b) = if reverse {
                    (&q.deps, &p.provides)
                } else {
                    (&p.deps, &q.provides)
                };
                let via: Vec<_> = a.iter().filter(|n| b.contains(n)).cloned().collect();
                if via.is_empty() {
                    None
                } else {
                    Some(Edge {
                        project: q.id.clone(),
                        via,
                    })
                }
            })
            .collect()
    };
    View {
        project: p.clone(),
        depends_on: edges(false),
        dependents: edges(true),
    }
}
