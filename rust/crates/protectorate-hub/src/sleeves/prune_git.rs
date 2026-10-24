//! Destructive clone verification. Git only opens repositories built here from
//! regular files; agent-written config, hooks and filters are never loaded.
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn open_git_file(root: &File, relative: &str) -> std::result::Result<File, String> {
    let mut parts = relative.split('/').peekable();
    let mut directory = root.try_clone().map_err(|e| e.to_string())?;
    while let Some(part) = parts.next() {
        if part.is_empty() || part == "." || part == ".." {
            return Err("invalid Git metadata path".into());
        }
        if parts.peek().is_some() {
            directory = super::open_child_directory(&directory, OsStr::new(part))
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("missing Git metadata: {relative}"))?;
        } else {
            let name = std::ffi::CString::new(part.as_bytes()).map_err(|e| e.to_string())?;
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(format!("{relative}: {}", io::Error::last_os_error()));
            }
            let file = unsafe { File::from_raw_fd(fd) };
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err(format!("non-regular Git file: {relative}"));
            }
            return Ok(file);
        }
    }
    Err("empty Git metadata path".into())
}

fn has_entry(root: &File, relative: &str) -> bool {
    let path = format!("/proc/self/fd/{}/{}", root.as_raw_fd(), relative);
    fs::symlink_metadata(path).is_ok()
}

struct CopyBudget {
    bytes: u64,
    files: u64,
    deadline: Instant,
}

impl CopyBudget {
    fn new() -> Self {
        Self {
            bytes: 0,
            files: 0,
            deadline: Instant::now() + Duration::from_secs(120),
        }
    }
    fn file(&mut self) -> std::result::Result<(), String> {
        self.files += 1;
        if self.files > 1_000_000 || Instant::now() >= self.deadline {
            return Err("Git snapshot file-count or time limit exceeded".into());
        }
        Ok(())
    }
    fn bytes(&mut self, count: usize) -> std::result::Result<(), String> {
        self.bytes = self.bytes.saturating_add(count as u64);
        if self.bytes > 8 * 1024 * 1024 * 1024 || Instant::now() >= self.deadline {
            return Err("Git snapshot size or time limit exceeded".into());
        }
        Ok(())
    }
}

fn copy_bounded(
    input: &mut File,
    output: &mut File,
    budget: &mut CopyBudget,
) -> std::result::Result<(), String> {
    budget.file()?;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        budget.bytes(count)?;
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
    }
    output.flush().map_err(|e| e.to_string())
}

fn copy_git_file(
    root: &File,
    relative: &str,
    to: &Path,
    budget: &mut CopyBudget,
) -> std::result::Result<(), String> {
    let mut input = open_git_file(root, relative)?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(to)
        .map_err(|e| e.to_string())?;
    copy_bounded(&mut input, &mut output, budget)
}

fn copy_tree(from: &File, to: &Path, budget: &mut CopyBudget) -> std::result::Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in
        fs::read_dir(format!("/proc/self/fd/{}", from.as_raw_fd())).map_err(|e| e.to_string())?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let target = to.join(&name);
        match super::open_child_directory(from, &name) {
            Ok(Some(child)) => copy_tree(&child, &target, budget)?,
            Ok(None) => return Err("Git object or ref disappeared".into()),
            Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
                let name = std::ffi::CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
                let fd = unsafe {
                    libc::openat(
                        from.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                    )
                };
                if fd < 0 {
                    return Err(format!(
                        "non-regular Git object or ref: {}",
                        io::Error::last_os_error()
                    ));
                }
                let mut input = unsafe { File::from_raw_fd(fd) };
                if !input.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("non-regular Git object or ref".into());
                }
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&target)
                    .map_err(|e| e.to_string())?;
                copy_bounded(&mut input, &mut output, budget)?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn command(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", repo)
        .env("XDG_CONFIG_HOME", repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -F /dev/null -oBatchMode=yes -oStrictHostKeyChecking=yes",
        )
        .env("GIT_ALLOW_PROTOCOL", "file:http:https:ssh:git")
        .env("LC_ALL", "C")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-c")
        .arg("core.attributesFile=/dev/null")
        .arg("-c")
        .arg("protocol.ext.allow=never")
        .stdin(Stdio::null());
    cmd
}

fn run(repo: &Path, args: &[&OsStr]) -> std::result::Result<Output, String> {
    run_with_input(repo, args, Vec::new())
}

fn execute(repo: &Path, args: &[&OsStr], input: Vec<u8>) -> std::result::Result<Output, String> {
    let mut child = command(repo)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start Git: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("missing Git stdin")?;
    let feed = thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let stdout = child.stdout.take().ok_or("missing Git stdout")?;
    let stderr = child.stderr.take().ok_or("missing Git stderr")?;
    let out = thread::spawn(move || read_limited(stdout));
    let err = thread::spawn(move || read_limited(stderr));
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Git verification command timed out".into());
            }
        }
    };
    let stdout = out.join().map_err(|_| "Git stdout reader failed")??;
    let stderr = err.join().map_err(|_| "Git stderr reader failed")??;
    let _ = feed.join();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_limited(mut input: impl Read) -> std::result::Result<Vec<u8>, String> {
    const LIMIT: usize = 128 * 1024 * 1024;
    let mut data = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut overflow = false;
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if data.len().saturating_add(count) > LIMIT {
            overflow = true;
        }
        if !overflow {
            data.extend_from_slice(&buffer[..count]);
        }
    }
    if overflow {
        Err("Git verification output limit exceeded".into())
    } else {
        Ok(data)
    }
}

fn timed_fetch(repo: &Path, origin: &Path) -> std::result::Result<(), String> {
    let mut child = command(repo)
        .arg(format!("--git-dir={}", repo.display()))
        .args([
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
        ])
        .arg(origin)
        .args(["+refs/*:refs/verified/*", "+HEAD:refs/verified/HEAD"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("start origin fetch: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => return Err("origin fetch failed".into()),
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("origin fetch timed out".into());
            }
        }
    }
}

fn config_value(config: &Path, key: &str) -> std::result::Result<Option<String>, String> {
    let config = config.as_os_str();
    let root = Path::new("/");
    let output = command(root)
        .args([
            OsStr::new("config"),
            OsStr::new("--no-includes"),
            OsStr::new("--file"),
            config,
            OsStr::new("--get-all"),
            OsStr::new(key),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err("invalid clone config".into());
    }
    let text = String::from_utf8(output.stdout).map_err(|_| "non-UTF-8 clone config")?;
    let values: Vec<_> = text.lines().collect();
    if values.len() != 1 {
        return Err("ambiguous clone config".into());
    }
    Ok(Some(values[0].to_owned()))
}

fn init_repo(path: &Path, format: &str) -> std::result::Result<(), String> {
    let mut args = vec![
        OsStr::new("init"),
        OsStr::new("--bare"),
        OsStr::new("--template="),
        OsStr::new("--quiet"),
    ];
    let option;
    if format == "sha256" {
        option = OsStr::new("--object-format=sha256");
        args.push(option);
    } else if format != "sha1" {
        return Err("unsupported Git object format".into());
    }
    args.push(path.as_os_str());
    run(path, &args)?;
    Ok(())
}

fn snapshot(git_dir: &Path, dest: &Path) -> std::result::Result<String, String> {
    let root =
        super::open_root_directory(git_dir).map_err(|e| format!("clone Git directory: {e}"))?;
    let mut budget = CopyBudget::new();
    for forbidden in [
        "shallow",
        "commondir",
        "objects/info/alternates",
        "objects/info/http-alternates",
        "info/grafts",
        "refs/stash",
        "objects/info/packs",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
        "worktrees",
        "modules",
        "lfs",
        "rr-cache",
        "index.lock",
    ] {
        if has_entry(&root, forbidden) {
            return Err(format!("unverifiable Git metadata: {forbidden}"));
        }
    }
    let source_config = dest.with_extension("source-config");
    copy_git_file(&root, "config", &source_config, &mut budget)?;
    let format =
        config_value(&source_config, "extensions.objectformat")?.unwrap_or_else(|| "sha1".into());
    init_repo(dest, &format)?;
    let objects = super::open_child_directory(&root, OsStr::new("objects"))
        .map_err(|e| e.to_string())?
        .ok_or("missing Git object store")?;
    copy_tree(&objects, &dest.join("objects"), &mut budget)?;
    if let Some(refs) =
        super::open_child_directory(&root, OsStr::new("refs")).map_err(|e| e.to_string())?
    {
        copy_tree(&refs, &dest.join("refs"), &mut budget)?;
    }
    if has_entry(&root, "packed-refs") {
        copy_git_file(&root, "packed-refs", &dest.join("packed-refs"), &mut budget)?;
    }
    for forbidden in [
        "objects/info/alternates",
        "objects/info/http-alternates",
        "refs/stash",
    ] {
        if dest.join(forbidden).exists() {
            return Err(format!("unverifiable Git metadata: {forbidden}"));
        }
    }
    let mut head = String::new();
    open_git_file(&root, "HEAD")?
        .take(1025)
        .read_to_string(&mut head)
        .map_err(|e| e.to_string())?;
    if head.len() > 1024 {
        return Err("oversized clone HEAD".into());
    }
    let head = head.trim();
    let oid_len = if format == "sha256" { 64 } else { 40 };
    if let Some(reference) = head.strip_prefix("ref: ") {
        if !reference.starts_with("refs/heads/") || reference.contains(['\n', '\r']) {
            return Err("invalid clone HEAD".into());
        }
    } else if head.len() != oid_len || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid clone HEAD".into());
    }
    fs::write(dest.join("HEAD"), format!("{head}\n")).map_err(|e| e.to_string())?;
    Ok(format)
}

fn remote_url(git_dir: &Path, scratch: &Path) -> std::result::Result<String, String> {
    let root = super::open_root_directory(git_dir).map_err(|e| e.to_string())?;
    let config = scratch.join("clone-config");
    copy_git_file(&root, "config", &config, &mut CopyBudget::new())?;
    let url = config_value(&config, "remote.origin.url")?.ok_or("clone has no origin")?;
    if url.starts_with('-') || url.chars().any(char::is_control) {
        return Err("invalid origin URL".into());
    }
    Ok(url)
}

/// Untracked paths that no committed .gitignore ignores. The rules are the
/// HEAD blobs checked out into a scratch tree, so .git/info/exclude, global
/// excludes and the working copy's own .gitignore files never apply. Nested
/// repositories and any file named .gitignore are never excused.
fn unexcused(
    clone: &Path,
    repo_arg: &str,
    tree: &[u8],
    others: &[u8],
    scratch: &Path,
) -> std::result::Result<bool, String> {
    let mut rules = Vec::new();
    for line in tree.split(|&b| b == 0).filter(|l| !l.is_empty()) {
        let tab = line
            .iter()
            .position(|&b| b == b'\t')
            .ok_or("invalid Git tree listing")?;
        let (meta, path) = (&line[..tab], &line[tab + 1..]);
        let name = path.rsplit(|&b| b == b'/').next().unwrap_or(path);
        if name == b".gitignore" && meta.starts_with(b"100") {
            rules.extend_from_slice(path);
            rules.push(0);
        }
    }
    let mut candidates = Vec::new();
    for path in others.split(|&b| b == 0).filter(|p| !p.is_empty()) {
        let name = path.rsplit(|&b| b == b'/').next().unwrap_or(path);
        if path.ends_with(b"/") || name == b".gitignore" || name.is_empty() {
            return Ok(true);
        }
        candidates.push(path);
    }
    if rules.is_empty() {
        return Ok(!candidates.is_empty());
    }
    let rules_dir = scratch.join("rules");
    fs::create_dir(&rules_dir).map_err(|e| e.to_string())?;
    let rules_arg = format!("--work-tree={}", rules_dir.display());
    let prefix = format!("--prefix={}/", rules_dir.display());
    run_with_input(
        clone,
        &[
            OsStr::new(repo_arg),
            OsStr::new(&rules_arg),
            OsStr::new("checkout-index"),
            OsStr::new(&prefix),
            OsStr::new("-z"),
            OsStr::new("--stdin"),
        ],
        rules,
    )?;
    let mut stdin = Vec::new();
    for path in &candidates {
        stdin.extend_from_slice(path);
        stdin.push(0);
    }
    let output = execute(
        clone,
        &[
            OsStr::new("-c"),
            OsStr::new("core.excludesFile=/dev/null"),
            OsStr::new(repo_arg),
            OsStr::new(&rules_arg),
            OsStr::new("check-ignore"),
            OsStr::new("--no-index"),
            OsStr::new("-z"),
            OsStr::new("--stdin"),
        ],
        stdin,
    )?;
    match output.status.code() {
        Some(0) => {
            let ignored = output.stdout.split(|&b| b == 0).filter(|p| !p.is_empty());
            Ok(ignored.count() != candidates.len())
        }
        Some(1) => Ok(true),
        _ => Err(format!(
            "Git ignore check failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

fn run_with_input(
    repo: &Path,
    args: &[&OsStr],
    input: Vec<u8>,
) -> std::result::Result<Output, String> {
    let output = execute(repo, args, input)?;
    if !output.status.success() {
        return Err(format!(
            "Git verification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

pub(super) fn verify(
    workspace: &Path,
    origin_hint: &str,
    workspaces: &Path,
    excuse_ignored: bool,
) -> std::result::Result<(), String> {
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clone_git = workspace.join(".git");
    super::open_root_directory(&clone_git).map_err(|e| format!("clone Git directory: {e}"))?;
    let clone = scratch.path().join("clone.git");
    let format = snapshot(&clone_git, &clone)?;
    let repo_arg = format!("--git-dir={}", clone.display());
    let work_arg = format!("--work-tree={}", workspace.display());
    let local_refs = run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new("for-each-ref"),
            OsStr::new("--format=%(objectname)"),
            OsStr::new("refs"),
        ],
    )?;
    let mut tips: Vec<String> = String::from_utf8(local_refs.stdout)
        .map_err(|_| "invalid local refs")?
        .lines()
        .map(str::to_owned)
        .collect();
    let head = run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("HEAD"),
        ],
    )?;
    tips.push(
        String::from_utf8(head.stdout)
            .map_err(|_| "invalid HEAD")?
            .trim()
            .to_owned(),
    );
    let oid_len = if format == "sha256" { 64 } else { 40 };
    if tips
        .iter()
        .any(|oid| oid.len() != oid_len || !oid.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("invalid local ref".into());
    }
    let tree = run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new("ls-tree"),
            OsStr::new("-r"),
            OsStr::new("-z"),
            OsStr::new("HEAD"),
        ],
    )?;
    if tree
        .stdout
        .split(|&b| b == 0)
        .any(|line| line.starts_with(b"160000 "))
    {
        return Err("submodule checkout cannot be verified".into());
    }
    let clone_git_root = super::open_root_directory(&clone_git).map_err(|e| e.to_string())?;
    if has_entry(&clone_git_root, "index") {
        copy_git_file(
            &clone_git_root,
            "index",
            &clone.join("index"),
            &mut CopyBudget::new(),
        )?;
        run(
            &clone,
            &[
                OsStr::new(&repo_arg),
                OsStr::new(&work_arg),
                OsStr::new("diff"),
                OsStr::new("--cached"),
                OsStr::new("--quiet"),
                OsStr::new("--no-ext-diff"),
                OsStr::new("--no-textconv"),
                OsStr::new("HEAD"),
                OsStr::new("--"),
            ],
        )
        .map_err(|_| "staged changes or unreadable index")?;
        fs::remove_file(clone.join("index")).map_err(|e| e.to_string())?;
    }
    run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new("read-tree"),
            OsStr::new("HEAD"),
        ],
    )?;
    run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new(&work_arg),
            OsStr::new("diff"),
            OsStr::new("--quiet"),
            OsStr::new("--no-ext-diff"),
            OsStr::new("--no-textconv"),
            OsStr::new("HEAD"),
            OsStr::new("--"),
        ],
    )
    .map_err(|_| "uncommitted changes or unreadable worktree")?;
    let others = run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new(&work_arg),
            OsStr::new("ls-files"),
            OsStr::new("--others"),
            OsStr::new("-z"),
            OsStr::new("--"),
        ],
    )?;
    let kept = if excuse_ignored {
        unexcused(
            &clone,
            &repo_arg,
            &tree.stdout,
            &others.stdout,
            scratch.path(),
        )?
    } else {
        !others.stdout.is_empty()
    };
    if kept {
        return Err("untracked or ignored files".into());
    }

    let raw_origin = if origin_hint.is_empty() {
        remote_url(&clone_git, scratch.path())?
    } else {
        origin_hint.to_owned()
    };
    let origin = if raw_origin.starts_with('/') || raw_origin.starts_with("file://") {
        let local = if let Some(path) = raw_origin.strip_prefix("file://") {
            if path.starts_with("localhost/") {
                PathBuf::from(&path[9..])
            } else if path.starts_with('/') {
                PathBuf::from(path)
            } else {
                return Err("non-local file origin".into());
            }
        } else {
            PathBuf::from(&raw_origin)
        };
        let real = fs::canonicalize(&local).map_err(|e| format!("origin: {e}"))?;
        let root = fs::canonicalize(workspaces).map_err(|e| e.to_string())?;
        let here = fs::canonicalize(workspace).map_err(|e| e.to_string())?;
        if real.starts_with(&root) || real.starts_with(&here) || here.starts_with(&real) {
            return Err("origin is not independent of hub clones".into());
        }
        let source_git = if real.join(".git").exists() {
            real.join(".git")
        } else {
            real
        };
        let source = scratch.path().join("origin.git");
        if snapshot(&source_git, &source)? != format {
            return Err("origin object format differs".into());
        }
        source
    } else {
        if raw_origin.chars().any(char::is_control)
            || raw_origin.starts_with('-')
            || !["https://", "http://", "ssh://", "git://"]
                .iter()
                .any(|prefix| raw_origin.starts_with(prefix))
        {
            return Err("unsupported origin URL".into());
        }
        PathBuf::from(raw_origin)
    };
    let fetched = scratch.path().join("fetched.git");
    init_repo(&fetched, &format)?;
    timed_fetch(&fetched, &origin)?;
    let fetched_arg = format!("--git-dir={}", fetched.display());
    // A reset or rebase may leave the only copy of a user's commit dangling.
    // The clone's complete object inventory must also exist at the origin.
    let inventory = run(
        &clone,
        &[
            OsStr::new(&repo_arg),
            OsStr::new("cat-file"),
            OsStr::new("--batch-all-objects"),
            OsStr::new("--batch-check=%(objectname)"),
        ],
    )?;
    let inventory =
        String::from_utf8(inventory.stdout).map_err(|_| "invalid clone object inventory")?;
    tips.extend(inventory.lines().map(str::to_owned));
    let remote_objects = run(
        &fetched,
        &[
            OsStr::new(&fetched_arg),
            OsStr::new("cat-file"),
            OsStr::new("--batch-all-objects"),
            OsStr::new("--batch-check=%(objectname)"),
        ],
    )?;
    let remote_objects: HashSet<String> = String::from_utf8(remote_objects.stdout)
        .map_err(|_| "invalid origin object inventory")?
        .lines()
        .map(str::to_owned)
        .collect();
    for oid in tips {
        if !remote_objects.contains(&oid) {
            return Err("local Git object is not independently recoverable from origin".into());
        }
    }
    Ok(())
}
