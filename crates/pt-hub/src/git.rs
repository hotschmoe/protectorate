//! The git the hub runs itself: child worktrees, checkpoints, the facts for a brief, and the
//! empty repository of a new project root. Nothing here writes into a repository a sleeve
//! already controls.
//!
//! A sleeve controls its workspace and, for a child, the shared git dir, so it controls the
//! repo's config and `.gitattributes`. Git runs commands from those (filters, hooks, fsmonitor,
//! textconv), and the hub runs git with its own authority. So the hub reads the real repo only
//! through `rev-parse` and `worktree add --no-checkout`, which run nothing from config once
//! hooks are off, and does everything that touches a work tree or an index (checkout, add,
//! status, diff) against a shadow git dir it writes itself: its own config, `HEAD` as a plain
//! commit id, the repo's objects as an alternate. System and global config are not read either.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Settings on every hub git command line, which win over any config file.
const SAFE: &[&str] = &[
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "core.sshCommand=false",
    "submodule.recurse=false",
    "gc.auto=0",
    "maintenance.auto=false",
    "commit.gpgSign=false",
];

fn base() -> Command {
    let mut c = Command::new("git");
    for kv in SAFE {
        c.args(["-c", kv]);
    }
    c.arg("--no-pager")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "protectorate")
        .env("GIT_AUTHOR_EMAIL", "protectorate@localhost")
        .env("GIT_COMMITTER_NAME", "protectorate")
        .env("GIT_COMMITTER_EMAIL", "protectorate@localhost")
        .stdin(Stdio::null());
    c
}

/// A command on the real repository. Only for commands that read refs and paths or create
/// refs and admin files: never one that reads a work tree or an index.
fn git(dir: &Path) -> Command {
    let mut c = base();
    c.arg("-C").arg(dir);
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

fn rev_parse(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut c = git(dir);
    c.arg("rev-parse").args(args);
    run(c, "rev-parse")
}

pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// `git init -b <branch>` and one empty commit, in a directory this hub just created.
pub fn init_empty(dir: &Path, branch: &str, message: &str) -> Result<(), String> {
    if !branch_ok(branch) {
        return Err(format!("bad branch name {branch}"));
    }
    let mut c = git(dir);
    c.args(["init", "-q", "-b", branch]);
    run(c, "init")?;
    let mut c = git(dir);
    c.args(["commit", "--allow-empty", "-qm", message]);
    run(c, "commit")?;
    Ok(())
}

fn branch_ok(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 64
        && !branch.starts_with('-')
        && !branch.ends_with('.')
        && !branch.ends_with('/')
        && !branch.contains("..")
        && !branch.contains("@{")
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
}

/// A git dir of the hub's own under `state_dir`, with `work` as its work tree: the config is
/// ours, `HEAD` names `head` (or an unborn branch), objects go to `objects` with `alternate`
/// (the repo's) behind them, and the repo's `info/exclude` is copied in as data.
struct Shadow {
    dir: PathBuf,
    work: PathBuf,
    index: PathBuf,
    objects: PathBuf,
    alternate: PathBuf,
}

impl Shadow {
    fn new(
        repo: &Path,
        work: &Path,
        state_dir: &Path,
        head: Option<&str>,
        index: PathBuf,
        objects: PathBuf,
    ) -> Result<Shadow, String> {
        let io = |p: &Path| {
            let p = p.to_path_buf();
            move |e: std::io::Error| format!("{}: {e}", p.display())
        };
        let alternate = repo.join(rev_parse(repo, &["--git-path", "objects"])?);
        let format = rev_parse(repo, &["--show-object-format"])?;
        let exclude = repo.join(rev_parse(repo, &["--git-path", "info/exclude"])?);
        let dir = state_dir.join("shadow.git");
        for d in [dir.join("refs"), dir.join("info"), objects.clone()] {
            fs::create_dir_all(&d).map_err(io(&d))?;
        }
        let config = if format == "sha1" {
            "[core]\n\trepositoryformatversion = 0\n\tbare = false\n".to_string()
        } else {
            format!(
                "[core]\n\trepositoryformatversion = 1\n\tbare = false\n\
                 [extensions]\n\tobjectformat = {format}\n"
            )
        };
        let files = [
            (dir.join("config"), config),
            (
                dir.join("HEAD"),
                match head {
                    Some(h) => format!("{h}\n"),
                    None => "ref: refs/heads/unborn\n".into(),
                },
            ),
            (
                dir.join("info/exclude"),
                fs::read_to_string(&exclude).unwrap_or_default(),
            ),
        ];
        for (p, text) in files {
            fs::write(&p, text).map_err(io(&p))?;
        }
        Ok(Shadow {
            dir,
            work: work.to_path_buf(),
            index,
            objects,
            alternate,
        })
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = base();
        c.arg("--git-dir")
            .arg(&self.dir)
            .arg("--work-tree")
            .arg(&self.work)
            .arg("-C")
            .arg(&self.work)
            .args(args)
            .env("GIT_INDEX_FILE", &self.index)
            .env("GIT_OBJECT_DIRECTORY", &self.objects)
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &self.alternate)
            .env("GIT_NO_REPLACE_OBJECTS", "1");
        c
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        run(self.cmd(args), args[0])
    }
}

fn head(repo: &Path) -> Option<String> {
    rev_parse(repo, &["--verify", "-q", "HEAD^{commit}"])
        .ok()
        .filter(|h| !h.is_empty())
}

/// `git worktree add -b <branch> <path> HEAD` in the root's repository, checked out through a
/// shadow git dir. Done only once [`Completion`] says so; a cut-short add is finished here.
pub fn worktree_add(
    root: &Path,
    path: &Path,
    branch: &str,
    state_dir: &Path,
) -> Result<(), String> {
    // The claim was checked on real paths; a symlink planted since then would point elsewhere.
    let real_parent = path.parent().and_then(|d| fs::canonicalize(d).ok());
    if real_parent.as_deref() != path.parent()
        || fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(format!("{} goes through a symlink", path.display()));
    }
    let commit = head(root).ok_or("the root's repository has no commit")?;
    if !registered(root, path)? {
        let has_branch = rev_parse(root, &["--verify", "-q", &format!("refs/heads/{branch}")])
            .is_ok_and(|b| !b.is_empty());
        let mut c = git(root);
        c.args(["worktree", "add", "-q", "--no-checkout"]);
        if has_branch {
            c.arg(path).arg(branch);
        } else {
            c.args(["-b", branch]).arg(path).arg(&commit);
        }
        run(c, "worktree add")?;
    }
    let tip = rev_parse(path, &["--verify", "-q", "HEAD^{commit}"])?;
    let index = path.join(rev_parse(path, &["--git-path", "index"])?);
    let objects = root.join(rev_parse(root, &["--git-path", "objects"])?);
    let shadow = Shadow::new(root, path, state_dir, Some(&tip), index, objects)?;
    shadow.run(&["read-tree", "--reset", "-u", "HEAD"])?;
    let mut c = git(root);
    c.args(["worktree", "unlock"]).arg(path);
    // Only a cut-short add is still locked.
    let _ = c.output();
    Ok(())
}

/// Whether `path` is a work tree of the root's repository.
fn registered(root: &Path, path: &Path) -> Result<bool, String> {
    let mut c = git(root);
    c.args(["worktree", "list", "--porcelain"]);
    let list = run(c, "worktree list")?;
    Ok(list
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .any(|p| Path::new(p) == path))
}

/// Hides `.protectorate/` from git status through the repo's own `info/exclude`.
pub fn exclude_protectorate(workspace: &Path) -> Result<(), String> {
    let rel = rev_parse(workspace, &["--git-path", "info/exclude"])?;
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

/// The working tree as a tree object in the hub's own object dir: HEAD (or nothing), then
/// every file of the work tree added, through a private index. Returns the shadow and HEAD.
fn snapshot(workspace: &Path, state_dir: &Path) -> Result<(Shadow, Option<String>), String> {
    let head = head(workspace);
    let shadow = Shadow::new(
        workspace,
        workspace,
        state_dir,
        head.as_deref(),
        state_dir.join("checkpoint.index"),
        state_dir.join("objects"),
    )?;
    match &head {
        Some(h) => shadow.run(&["read-tree", h])?,
        None => shadow.run(&["read-tree", "--empty"])?,
    };
    shadow.run(&["add", "-A", "--", "."])?;
    Ok((shadow, head))
}

/// Snapshots the working tree as a commit whose parent is HEAD, written into a hub-owned object
/// directory (the repo's objects are an alternate) through a private index. No ref moves.
pub fn checkpoint(workspace: &Path, state_dir: &Path, message: &str) -> Result<String, String> {
    let (shadow, head) = snapshot(workspace, state_dir)?;
    let tree = shadow.run(&["write-tree"])?;
    let mut args = vec!["commit-tree", tree.as_str(), "-m", message];
    if let Some(h) = &head {
        args.extend(["-p", h.as_str()]);
    }
    shadow.run(&args)
}

#[derive(Default)]
pub struct Facts {
    pub branch: String,
    pub head: String,
    pub status: String,
    pub diff_stat: String,
    pub changed: Vec<String>,
}

/// What the brief says about the workspace: changes against HEAD, staged or not, as one set.
pub fn facts(workspace: &Path, state_dir: &Path) -> Facts {
    let mut f = Facts {
        branch: rev_parse(workspace, &["--abbrev-ref", "HEAD"]).unwrap_or_default(),
        head: rev_parse(workspace, &["--short", "HEAD"]).unwrap_or_default(),
        ..Facts::default()
    };
    let Ok((shadow, head)) = snapshot(workspace, state_dir) else {
        return f;
    };
    // An unborn HEAD diffs against nothing, which lists every file as added.
    let q = |how: &str| {
        let mut args = vec!["diff", "--cached", "--no-ext-diff", "--no-textconv", how];
        args.extend(head.as_deref());
        shadow.run(&args).unwrap_or_default()
    };
    f.status = q("--name-status");
    f.diff_stat = q("--stat");
    f.changed = q("--name-only").lines().map(str::to_string).collect();
    f
}
