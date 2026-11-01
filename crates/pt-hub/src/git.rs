//! The git the hub runs itself: child worktrees, checkpoints and the facts for a brief. Nothing
//! here creates a ref or a file the worked-on repo would show.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "protectorate")
        .env("GIT_AUTHOR_EMAIL", "protectorate@localhost")
        .env("GIT_COMMITTER_NAME", "protectorate")
        .env("GIT_COMMITTER_EMAIL", "protectorate@localhost")
        .stdin(Stdio::null());
    c
}

fn run(mut c: Command, what: &str) -> Result<String, String> {
    let out = c.output().map_err(|e| format!("git {what}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {what}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// `git worktree add -b <branch> <path> HEAD` in the root's repository.
pub fn worktree_add(root: &Path, path: &Path, branch: &str) -> Result<(), String> {
    if path.join(".git").is_file() {
        return Ok(());
    }
    let mut c = git(root);
    c.args(["worktree", "add", "-q", "-b", branch])
        .arg(path)
        .arg("HEAD");
    run(c, "worktree add").map(drop)
}

/// Hides `.protectorate/` from git status through the repo's own `info/exclude`.
pub fn exclude_protectorate(workspace: &Path) -> Result<(), String> {
    let mut c = git(workspace);
    c.args(["rev-parse", "--git-path", "info/exclude"]);
    let rel = run(c, "rev-parse")?;
    let path = workspace.join(rel);
    let text = fs::read_to_string(&path).unwrap_or_default();
    if text.lines().any(|l| l.trim() == "/.protectorate/") {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let sep = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    writeln!(f, "{sep}/.protectorate/").map_err(|e| format!("{}: {e}", path.display()))
}

/// Snapshots the working tree as a commit whose parent is HEAD, written into a hub-owned object
/// directory (the repo's objects are an alternate) through a private index. No ref moves.
pub fn checkpoint(workspace: &Path, state_dir: &Path, message: &str) -> Result<String, String> {
    let objects = state_dir.join("objects");
    fs::create_dir_all(&objects).map_err(|e| format!("{}: {e}", objects.display()))?;
    let mut c = git(workspace);
    c.args(["rev-parse", "--git-path", "objects"]);
    let repo_objects = workspace.join(run(c, "rev-parse")?);
    let index = state_dir.join("checkpoint.index");
    let env = |c: &mut Command| {
        c.env("GIT_INDEX_FILE", &index)
            .env("GIT_OBJECT_DIRECTORY", &objects)
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &repo_objects);
    };
    let mut c = git(workspace);
    env(&mut c);
    c.args(["rev-parse", "--verify", "-q", "HEAD"]);
    let head = run(c, "rev-parse HEAD").ok().filter(|h| !h.is_empty());
    let mut c = git(workspace);
    env(&mut c);
    match &head {
        Some(h) => c.args(["read-tree", h.as_str()]),
        None => c.args(["read-tree", "--empty"]),
    };
    run(c, "read-tree")?;
    let mut c = git(workspace);
    env(&mut c);
    c.args(["add", "-A", "--", "."]);
    run(c, "add")?;
    let mut c = git(workspace);
    env(&mut c);
    c.arg("write-tree");
    let tree = run(c, "write-tree")?;
    let mut c = git(workspace);
    env(&mut c);
    c.args(["commit-tree", tree.as_str(), "-m", message]);
    if let Some(h) = &head {
        c.args(["-p", h.as_str()]);
    }
    run(c, "commit-tree")
}

#[derive(Default)]
pub struct Facts {
    pub branch: String,
    pub head: String,
    pub status: String,
    pub diff_stat: String,
    pub changed: Vec<String>,
}

pub fn facts(workspace: &Path) -> Facts {
    let q = |args: &[&str]| {
        let mut c = git(workspace);
        c.args(args);
        run(c, args[0]).unwrap_or_default()
    };
    Facts {
        branch: q(&["rev-parse", "--abbrev-ref", "HEAD"]),
        head: q(&["rev-parse", "--short", "HEAD"]),
        status: q(&["status", "--short"]),
        diff_stat: q(&["diff", "--stat", "HEAD"]),
        changed: q(&["diff", "--name-only", "HEAD"])
            .lines()
            .map(str::to_string)
            .collect(),
    }
}
