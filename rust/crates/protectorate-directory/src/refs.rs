use crate::{git, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub sha: String,
    pub unix: i64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefState {
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub default_tip: String,
    #[serde(default)]
    pub tags: Vec<Tag>,
}
impl RefState {
    pub fn latest_tag(&self) -> &str {
        self.tags.last().map(|t| t.name.as_str()).unwrap_or("")
    }
}
fn sort_tags(tags: &mut [Tag]) {
    tags.sort_by(|a, b| a.unix.cmp(&b.unix).then(a.name.cmp(&b.name)))
}
pub fn read_refs(dir: impl AsRef<Path>) -> Result<RefState> {
    let dir = dir.as_ref();
    let mut r = RefState {
        default: git(
            dir,
            &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
        )
        .unwrap_or_default()
        .trim()
        .trim_start_matches("origin/")
        .into(),
        ..Default::default()
    };
    let text = git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(creatordate:unix)",
            "refs/tags",
            "refs/heads",
            "refs/remotes/origin",
        ],
    )?;
    let mut heads = BTreeMap::new();
    let mut remotes = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<_> = line.split('\0').collect();
        if f.len() != 4 {
            continue;
        }
        if let Some(name) = f[0].strip_prefix("refs/tags/") {
            r.tags.push(Tag {
                name: name.into(),
                sha: if f[2].is_empty() { f[1] } else { f[2] }.into(),
                unix: f[3].parse().unwrap_or(0),
            });
        } else if let Some(name) = f[0].strip_prefix("refs/heads/") {
            heads.insert(name.to_string(), f[1].to_string());
        } else if let Some(name) = f[0].strip_prefix("refs/remotes/origin/") {
            remotes.insert(name.to_string(), f[1].to_string());
        }
    }
    sort_tags(&mut r.tags);
    if r.default.is_empty() {
        for n in ["main", "master"] {
            if heads.contains_key(n) || remotes.contains_key(n) {
                r.default = n.into();
                break;
            }
        }
    }
    r.default_tip = heads
        .get(&r.default)
        .or_else(|| remotes.get(&r.default))
        .cloned()
        .unwrap_or_default();
    Ok(r)
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub kind: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub from_name: String,
    pub tag: String,
}
pub fn changes(
    old: &RefState,
    now: &RefState,
    is_ancestor: impl Fn(&str, &str) -> bool,
) -> Vec<Change> {
    let known: BTreeSet<_> = old.tags.iter().map(|t| &t.name).collect();
    let (mut prev, mut prev_name) = old
        .tags
        .last()
        .map(|t| (t.sha.clone(), t.name.clone()))
        .unwrap_or((old.default_tip.clone(), String::new()));
    let mut out = Vec::new();
    let mut covered = false;
    for t in &now.tags {
        if known.contains(&t.name) {
            continue;
        }
        out.push(Change {
            kind: "tag".into(),
            name: t.name.clone(),
            tag: t.name.clone(),
            from: prev,
            from_name: prev_name,
            to: t.sha.clone(),
        });
        prev = t.sha.clone();
        prev_name = t.name.clone();
        covered |= t.sha == now.default_tip && !now.default_tip.is_empty()
    }
    if out.len() > 3 {
        out.drain(..out.len() - 3);
    }
    if !covered
        && !old.default.is_empty()
        && old.default == now.default
        && !old.default_tip.is_empty()
        && !now.default_tip.is_empty()
        && old.default_tip != now.default_tip
        && is_ancestor(&old.default_tip, &now.default_tip)
    {
        out.push(Change {
            kind: "branch".into(),
            name: now.default.clone(),
            from: old.default_tip.clone(),
            to: now.default_tip.clone(),
            ..Default::default()
        });
    }
    out
}
pub fn merge(old: &RefState, now: &RefState, is_ancestor: impl Fn(&str, &str) -> bool) -> RefState {
    let mut next = now.clone();
    for t in &old.tags {
        if !next.tags.iter().any(|n| n.name == t.name) {
            next.tags.push(t.clone())
        }
    }
    sort_tags(&mut next.tags);
    if old.default == next.default
        && !old.default_tip.is_empty()
        && next.default_tip != old.default_tip
        && is_ancestor(&next.default_tip, &old.default_tip)
    {
        next.default_tip = old.default_tip.clone()
    }
    next
}
pub fn is_ancestor(dir: impl AsRef<Path>, a: &str, b: &str) -> bool {
    if [a, b].iter().any(|s| s.is_empty() || s.starts_with('-')) {
        return false;
    }
    git(dir.as_ref(), &["merge-base", "--is-ancestor", a, b]).is_ok()
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub commits: Vec<String>,
    pub total_commits: usize,
    pub files: Vec<String>,
    pub total_files: usize,
    pub short_stat: String,
}
pub fn describe(dir: impl AsRef<Path>, from: &str, to: &str) -> Result<Facts> {
    let dir = dir.as_ref();
    if to.is_empty() || to.starts_with('-') || from.starts_with('-') {
        return Err(Error::Git("invalid revision".into()));
    }
    let from = if !from.is_empty()
        && git(dir, &["cat-file", "-e", &format!("{from}^{{commit}}")]).is_ok()
    {
        from
    } else {
        ""
    };
    let range = if from.is_empty() {
        to.into()
    } else {
        format!("{from}..{to}")
    };
    let mut f = Facts::default();
    for line in git(dir, &["log", "--format=%h%x00%s", "-n", "20", &range, "--"])?.lines() {
        if let Some((sha, subject)) = line.split_once('\0') {
            let subject = if subject.chars().count() > 120 {
                format!("{}...", subject.chars().take(119).collect::<String>())
            } else {
                subject.into()
            };
            f.commits.push(format!("{sha} {subject}"));
        }
    }
    f.total_commits = f.commits.len();
    if from.is_empty() {
        return Ok(f);
    }
    f.total_commits = git(dir, &["rev-list", "--count", &range, "--"])
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or(0);
    if let Ok(names) = git(
        dir,
        &[
            "diff",
            "--no-textconv",
            "--no-ext-diff",
            "--name-status",
            from,
            to,
            "--",
        ],
    ) {
        for line in names.lines().filter(|s| !s.is_empty()) {
            f.total_files += 1;
            if f.files.len() < 40 {
                f.files
                    .push(line.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        }
    }
    f.short_stat = git(
        dir,
        &[
            "diff",
            "--no-textconv",
            "--no-ext-diff",
            "--shortstat",
            from,
            to,
            "--",
        ],
    )
    .unwrap_or_default()
    .trim()
    .into();
    Ok(f)
}
pub fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}
impl std::fmt::Display for Facts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.commits.is_empty() {
            write!(f, "Commits ({}", self.total_commits)?;
            if self.total_commits > self.commits.len() {
                write!(f, ", newest {} shown", self.commits.len())?
            }
            writeln!(f, "):")?;
            for c in &self.commits {
                writeln!(f, "  {c}")?
            }
        }
        if !self.files.is_empty() {
            write!(f, "Changed files ({}", self.total_files)?;
            if self.total_files > self.files.len() {
                write!(f, ", first {} shown", self.files.len())?
            }
            writeln!(f, "):")?;
            for c in &self.files {
                writeln!(f, "  {c}")?
            }
            if !self.short_stat.is_empty() {
                writeln!(f, "  {}", self.short_stat)?
            }
        }
        Ok(())
    }
}
