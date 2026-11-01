//! "Everything under a directory" mode: every log of one kind below `root`, sorted by path.
//! The hub uses its allowlist instead (D21); this is for tests, the probe and rebuilds.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::safe_open;
use crate::{Source, SourceKind};

pub fn discover(kind: SourceKind, root: &Path) -> io::Result<Vec<Source>> {
    let mut files = Vec::new();
    let dir = safe_open::directory(root)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", root.display())))?;
    walk(root, &dir, kind, 0, &mut files)?;
    files.sort();
    Ok(files
        .into_iter()
        .filter_map(|p| {
            let session = session_of(kind, &p)?;
            let mut s = Source::new(kind, p, session.clone());
            if kind == SourceKind::Hub {
                s.sleeve = Some(session);
            }
            Some(s)
        })
        .collect())
}

fn walk(
    dir: &Path,
    opened: &fs::File,
    kind: SourceKind,
    depth: u32,
    out: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let rd = match fs::read_dir(safe_open::fd_path(opened)) {
        Ok(rd) => rd,
        Err(e) if depth > 0 && e.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", dir.display()))),
    };
    for entry in rd {
        let entry =
            entry.map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", dir.display())))?;
        let ft = entry.file_type()?;
        let path = dir.join(entry.file_name());
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if ft.is_dir() {
            // Claude subagent logs are all sidechain records, which are not indexed.
            if depth < 8
                && !(kind == SourceKind::Claude && name == "subagents")
                && let Ok(child) = safe_open::child(opened, &entry.file_name(), true)
            {
                walk(&path, &child, kind, depth + 1, out)?;
            }
        } else if ft.is_file() && wanted(kind, &name) {
            out.push(path);
        }
    }
    Ok(())
}

fn wanted(kind: SourceKind, name: &str) -> bool {
    match kind {
        SourceKind::Claude | SourceKind::Pi => name.ends_with(".jsonl"),
        SourceKind::Codex => name.starts_with("rollout-") && name.ends_with(".jsonl"),
        SourceKind::Grok => name == "chat_history.jsonl",
        SourceKind::Hub => name == "transcript.jsonl",
    }
}

/// The session id a log file's name carries.
fn session_of(kind: SourceKind, path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let parent = || path.parent()?.file_name()?.to_str().map(str::to_owned);
    match kind {
        SourceKind::Claude => Some(stem.to_owned()),
        SourceKind::Codex => Some(stem.get(stem.len().checked_sub(36)?..)?.to_owned()),
        SourceKind::Pi => Some(stem.rsplit_once('_').map_or(stem, |(_, id)| id).to_owned()),
        SourceKind::Grok | SourceKind::Hub => parent(),
    }
}
