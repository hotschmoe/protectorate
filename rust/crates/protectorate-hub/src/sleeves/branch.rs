//! Fresh Git clone preparation for branch-dispatched sleeves.
use super::*;
use protectorate_mailbox::{Kind, NewMessage};
use protectorate_sandbox::{Access, BindGrant};
use protectorate_sleeve_registry::Record;
use sha2::{Digest, Sha256};
use std::process::{Command, Stdio};

const GIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) struct Prepared {
    pub request: Value,
    destination: Option<PathBuf>,
    view: Option<PathBuf>,
}

impl Prepared {
    pub fn commit(&mut self) {
        self.destination = None;
        self.view = None;
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(path) = &self.destination {
            let _ = fs::remove_dir_all(path);
        }
        if let Some(path) = &self.view {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub(super) fn prepare(hub: &Hub, value: &Value) -> Result<Prepared> {
    let mut request = value.clone();
    if let Some(object) = request.as_object_mut() {
        for field in ["baseRepo", "baseBranch", "baseTip", "managedClone"] {
            object.remove(field);
        }
    }
    let branch = string(value, "branch");
    let new_branch = string(value, "newBranch");
    for name in [&branch, &new_branch] {
        if !name.is_empty() && !valid_branch(name) {
            return Err(RpcError::new(
                Code::InvalidArgument,
                format!("invalid Git branch {name:?}"),
            ));
        }
    }
    let source = string(value, "workspace");
    let remote = is_git_url(&source);
    if !remote && branch.is_empty() && new_branch.is_empty() {
        return Ok(Prepared {
            request,
            destination: None,
            view: None,
        });
    }
    let name = string(value, "name");
    protectorate_sleeve_registry::valid_name(&name).map_err(super::registry_error)?;
    if source.is_empty() || source.starts_with('-') {
        return Err(RpcError::new(Code::InvalidArgument, "invalid Git source"));
    }
    if hub.sleeves.get(&name).is_ok() {
        return Err(RpcError::new(
            Code::AlreadyExists,
            "branch dispatch requires a new sleeve name",
        ));
    }
    let mut base_repo = String::new();
    let mut base_branch = branch.clone();
    let mut base_tip = String::new();
    let mut git_dirs = Vec::new();
    let origin;
    let clone_source = if remote {
        origin = String::new();
        source.clone()
    } else {
        let path = Path::new(&source);
        if !path.is_absolute() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "workspace must be an absolute directory or Git URL",
            ));
        }
        let path = fs::canonicalize(path)
            .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
        if !path.is_dir() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "Git source is not a directory",
            ));
        }
        if !path.join(".git").exists() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "Git source must be the checkout root",
            ));
        }
        let path_text = path.to_string_lossy().into_owned();
        if git(&["-C", &path_text, "rev-parse", "--is-inside-work-tree"])?.trim() != "true" {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "source is not a Git checkout",
            ));
        }
        if base_branch.is_empty() {
            base_branch = git(&[
                "-C",
                &path_text,
                "symbolic-ref",
                "--quiet",
                "--short",
                "HEAD",
            ])
            .map_err(|_| RpcError::new(Code::InvalidArgument, "detached source needs --branch"))?
            .trim()
            .to_owned();
        }
        if !valid_branch(&base_branch) {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "source has no valid branch",
            ));
        }
        base_tip = git(&[
            "-C",
            &path_text,
            "rev-parse",
            "--verify",
            &format!("refs/heads/{base_branch}^{{commit}}"),
        ])?
        .trim()
        .to_owned();
        let git_dir = git(&["-C", &path_text, "rev-parse", "--absolute-git-dir"])?;
        let common_dir = git(&["-C", &path_text, "rev-parse", "--git-common-dir"])?;
        for raw in [git_dir.trim(), common_dir.trim()] {
            let directory = PathBuf::from(raw);
            let directory = if directory.is_absolute() {
                directory
            } else {
                path.join(directory)
            };
            let directory = fs::canonicalize(directory)
                .map_err(|error| RpcError::new(Code::InvalidArgument, error.to_string()))?;
            if !directory.is_dir() {
                return Err(RpcError::new(
                    Code::InvalidArgument,
                    "source Git directory is missing",
                ));
            }
            if !git_dirs.contains(&directory) {
                git_dirs.push(directory);
            }
        }
        origin = git(&["-C", &path_text, "config", "--get", "remote.origin.url"])
            .ok()
            .map(|url| strip_credentials(url.trim()))
            .filter(|url| is_git_url(url))
            .unwrap_or_default();
        base_repo = path_text.clone();
        path_text
    };

    let parent = hub.workspaces.state_root().join("workspaces");
    fs::create_dir_all(&parent).map_err(internal)?;
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    if fs::canonicalize(&parent).map_err(internal)? != parent {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "workspaces directory must not be a symlink",
        ));
    }
    let destination = parent.join(&name);
    fs::create_dir(&destination).map_err(|error| {
        RpcError::new(
            if error.kind() == io::ErrorKind::AlreadyExists {
                Code::AlreadyExists
            } else {
                Code::Internal
            },
            format!("fresh clone destination {}: {error}", destination.display()),
        )
    })?;
    let mut prepared = Prepared {
        request,
        destination: Some(destination.clone()),
        view: None,
    };
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    let destination_text = destination.to_string_lossy().into_owned();
    let mut clone = vec![
        "clone".to_owned(),
        "--quiet".into(),
        "--no-checkout".into(),
        "--no-hardlinks".into(),
        "--dissociate".into(),
    ];
    if !base_branch.is_empty() {
        clone.extend(["--branch".into(), base_branch.clone()]);
    }
    clone.extend(["--".into(), clone_source, destination_text.clone()]);
    git(&clone.iter().map(String::as_str).collect::<Vec<_>>())?;
    if !base_repo.is_empty() {
        git(&[
            "-C",
            &destination_text,
            "remote",
            "rename",
            "origin",
            "base",
        ])?;
        let view = create_base_view(
            hub.workspaces.state_root(),
            &name,
            &base_branch,
            &base_tip,
            &git_dirs,
        )?;
        prepared.view = Some(view.clone());
        let git_dir = view.to_string_lossy().into_owned();
        git(&[
            "-C",
            &destination_text,
            "remote",
            "set-url",
            "base",
            &git_dir,
        ])?;
        if !origin.is_empty() {
            git(&["-C", &destination_text, "remote", "add", "origin", &origin])?;
        }
    } else {
        git(&[
            "-C",
            &destination_text,
            "remote",
            "set-url",
            "origin",
            &strip_credentials(&source),
        ])?;
    }
    if !new_branch.is_empty() {
        git(&[
            "-C",
            &destination_text,
            "switch",
            "--quiet",
            "-c",
            &new_branch,
        ])?;
        // `switch -c` retains the empty index from `clone --no-checkout`.
        // Populate tracked files before exposing the clone to a sleeve.
        git(&[
            "-C",
            &destination_text,
            "reset",
            "--hard",
            "--quiet",
            "HEAD",
        ])?;
    } else {
        git(&["-C", &destination_text, "checkout", "--quiet"])?;
    }
    prepared.request["workspace"] = json!(destination_text);
    prepared.request["baseRepo"] = json!(base_repo);
    prepared.request["baseBranch"] = json!(if base_repo.is_empty() {
        ""
    } else {
        &base_branch
    });
    prepared.request["baseTip"] = json!(base_tip);
    prepared.request["managedClone"] = json!(true);
    Ok(prepared)
}

pub(crate) fn base_grants(state: &Path, name: &str, repo: &str) -> Result<Vec<BindGrant>> {
    if repo.is_empty() {
        return Ok(Vec::new());
    }
    let directories = git_directories(Path::new(repo))
        .map_err(|error| RpcError::new(Code::InvalidArgument, error))?;
    let common = directories.last().unwrap();
    for part in ["objects", "refs"] {
        let metadata = fs::symlink_metadata(common.join(part)).map_err(internal)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(RpcError::new(
                Code::InvalidArgument,
                "base Git objects and refs must be ordinary directories",
            ));
        }
    }
    let mut grants: Vec<_> = ["objects", "refs"]
        .into_iter()
        .map(|part| common.join(part))
        .map(|path| BindGrant {
            source: path.clone(),
            target: path,
            access: Access::ReadOnly,
        })
        .collect();
    let view = state.join("base-views").join(name);
    grants.push(BindGrant {
        source: view.clone(),
        target: view,
        access: Access::ReadOnly,
    });
    Ok(grants)
}

fn create_base_view(
    state: &Path,
    name: &str,
    branch: &str,
    tip: &str,
    dirs: &[PathBuf],
) -> Result<PathBuf> {
    let common = dirs
        .last()
        .ok_or_else(|| RpcError::new(Code::Internal, "base Git directory missing"))?;
    let parent = state.join("base-views");
    fs::create_dir_all(&parent).map_err(internal)?;
    if fs::canonicalize(&parent).map_err(internal)? != parent {
        return Err(RpcError::new(
            Code::FailedPrecondition,
            "base view directory must not be a symlink",
        ));
    }
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).map_err(internal)?;
    let view = parent.join(name);
    fs::create_dir(&view).map_err(internal)?;
    let setup = || -> io::Result<()> {
        fs::set_permissions(&view, fs::Permissions::from_mode(0o700))?;
        let mut config = "[core]\nrepositoryformatversion = 1\nbare = true\n".to_owned();
        if tip.len() == 64 {
            config.push_str("[extensions]\nobjectFormat = sha256\n");
        }
        fs::write(view.join("config"), config)?;
        fs::write(view.join("HEAD"), format!("ref: refs/heads/{branch}\n"))?;
        for part in ["objects", "refs"] {
            std::os::unix::fs::symlink(common.join(part), view.join(part))?;
        }
        refresh_packed_refs(&view, common)
    };
    if let Err(error) = setup() {
        let _ = fs::remove_dir_all(&view);
        return Err(internal(error));
    }
    Ok(view)
}

fn refresh_packed_refs(view: &Path, common: &Path) -> io::Result<()> {
    for directory in [view.parent().unwrap(), view] {
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "base view is not an ordinary directory",
            ));
        }
    }
    let source = common.join("packed-refs");
    let bytes = if source.exists() {
        let metadata = fs::symlink_metadata(&source)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 << 20 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source packed refs must be a bounded regular file",
            ));
        }
        fs::read(source)?
    } else {
        Vec::new()
    };
    let target = view.join("packed-refs");
    if fs::read(&target).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(view)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(&target).map_err(|error| error.error)?;
    Ok(())
}

fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch != "HEAD"
        && !branch.starts_with('-')
        && !branch.ends_with('.')
        && !branch.contains("..")
        && !branch.contains("@{")
        && !branch
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "~^:?*[\\".contains(ch))
        && branch
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

fn is_git_url(source: &str) -> bool {
    ["https://", "http://", "ssh://", "git://", "file://"]
        .iter()
        .any(|prefix| source.starts_with(prefix))
        || source.starts_with("git@") && source.contains(':')
}

fn strip_credentials(source: &str) -> String {
    if let Some((scheme, rest)) = source.split_once("://") {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if let Some((_, host)) = authority.rsplit_once('@') {
            return format!("{scheme}://{host}/{path}");
        }
    }
    source.to_owned()
}

fn git(args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "init.templateDir=",
    ]);
    command.args(args);
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "https:http:ssh:git:file")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Private URL clones may need the owner's credential helper. Subsequent
    // commands ignore host Git config so a checkout cannot invoke local tools.
    if args.first() != Some(&"clone") {
        command
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
    }
    let mut child = command
        .spawn()
        .map_err(|error| RpcError::new(Code::Unavailable, format!("start Git: {error}")))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_reader = thread::spawn(move || read_git_output(stdout, 16 * 1024));
    let err_reader = thread::spawn(move || read_git_output(stderr, 16 * 1024));
    let deadline = Instant::now() + GIT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RpcError::new(Code::Unavailable, "Git command timed out"));
            }
            Err(error) => return Err(RpcError::new(Code::Internal, error.to_string())),
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if !status.success() {
        return Err(RpcError::new(
            Code::InvalidArgument,
            format!(
                "Git {}: {}",
                args.first().copied().unwrap_or("command"),
                stderr.trim()
            ),
        ));
    }
    Ok(stdout)
}

fn read_git_output(mut reader: impl Read, limit: u64) -> String {
    let mut bytes = Vec::new();
    let _ = reader.by_ref().take(limit).read_to_end(&mut bytes);
    let _ = io::copy(&mut reader, &mut io::sink());
    String::from_utf8_lossy(&bytes).into_owned()
}

struct Change {
    tip: String,
    first: Instant,
    last: Instant,
}

pub(super) fn start_watcher(runtime: &tokio::runtime::Handle, hub: &Arc<Hub>) {
    let weak = Arc::downgrade(hub);
    runtime.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut changes = BTreeMap::<String, Change>::new();
        loop {
            tick.tick().await;
            let Some(hub) = weak.upgrade() else { break };
            match tokio::task::spawn_blocking(move || {
                check_bases(&hub, &mut changes);
                changes
            })
            .await
            {
                Ok(next) => changes = next,
                Err(error) => {
                    eprintln!("base watcher stopped: {error}");
                    break;
                }
            }
        }
    });
}

fn check_bases(hub: &Hub, changes: &mut BTreeMap<String, Change>) {
    let rows = hub.sleeves.list();
    changes.retain(|name, _| {
        rows.iter()
            .any(|row| row.name == *name && row.state == State::Running)
    });
    for row in rows {
        if row.state != State::Running || row.mode != Mode::Chat || row.base_repo.is_empty() {
            changes.remove(&row.name);
            continue;
        }
        let Ok(dirs) = git_directories(Path::new(&row.base_repo)) else {
            continue;
        };
        if refresh_packed_refs(
            &hub.workspaces
                .state_root()
                .join("base-views")
                .join(&row.name),
            dirs.last().unwrap(),
        )
        .is_err()
        {
            continue;
        }
        let Ok(tip) = branch_tip(Path::new(&row.base_repo), &row.base_branch) else {
            continue;
        };
        if tip == row.base_tip {
            changes.remove(&row.name);
            continue;
        }
        let now = Instant::now();
        let change = changes.entry(row.name.clone()).or_insert_with(|| Change {
            tip: tip.clone(),
            first: now,
            last: now,
        });
        if change.tip != tip {
            change.tip = tip.clone();
            change.last = now;
        }
        if now.duration_since(change.last) < Duration::from_secs(2)
            && now.duration_since(change.first) < Duration::from_secs(10)
        {
            continue;
        }
        if let Err(error) = notice_base_change(hub, &row, &tip) {
            eprintln!("base notice {}: {error}", row.name);
        } else {
            changes.remove(&row.name);
        }
    }
}

fn branch_tip(repo: &Path, branch: &str) -> std::result::Result<String, String> {
    if !valid_branch(branch) {
        return Err("invalid base branch".into());
    }
    let dirs = git_directories(repo)?;
    let common = dirs.last().unwrap();
    let reference = format!("refs/heads/{branch}");
    let loose = common.join(&reference);
    if loose.exists() {
        return read_tip(&loose);
    }
    let packed = common.join("packed-refs");
    let bytes = fs::read(packed).map_err(|error| error.to_string())?;
    if bytes.len() > 16 << 20 {
        return Err("packed refs exceeds limit".into());
    }
    let contents = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    for line in contents.lines() {
        if let Some((sha, name)) = line.split_once(' ') {
            if name == reference {
                return valid_tip(sha);
            }
        }
    }
    Err("base branch is absent".into())
}

fn read_tip(path: &Path) -> std::result::Result<String, String> {
    let meta = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !meta.is_file() || meta.len() > 256 {
        return Err("invalid loose ref".into());
    }
    let value = fs::read_to_string(path).map_err(|error| error.to_string())?;
    valid_tip(value.trim())
}

fn valid_tip(tip: &str) -> std::result::Result<String, String> {
    if matches!(tip.len(), 40 | 64) && tip.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(tip.to_ascii_lowercase())
    } else {
        Err("invalid base tip".into())
    }
}

fn git_directories(repo: &Path) -> std::result::Result<Vec<PathBuf>, String> {
    let dot = repo.join(".git");
    let dir = if dot.is_dir() {
        fs::canonicalize(&dot).map_err(|error| error.to_string())?
    } else {
        let raw = fs::read_to_string(&dot).map_err(|error| error.to_string())?;
        let target = raw
            .trim()
            .strip_prefix("gitdir: ")
            .ok_or("invalid .git file")?;
        let path = Path::new(target);
        fs::canonicalize(if path.is_absolute() {
            path.to_path_buf()
        } else {
            repo.join(path)
        })
        .map_err(|error| error.to_string())?
    };
    let common_file = dir.join("commondir");
    if !common_file.is_file() {
        return Ok(vec![dir]);
    }
    let raw = fs::read_to_string(common_file).map_err(|error| error.to_string())?;
    let path = Path::new(raw.trim());
    let common = fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        dir.join(path)
    })
    .map_err(|error| error.to_string())?;
    if common == dir {
        Ok(vec![dir])
    } else {
        Ok(vec![dir, common])
    }
}

fn notice_base_change(hub: &Hub, row: &Record, tip: &str) -> std::result::Result<(), String> {
    let source_git = hub
        .workspaces
        .state_root()
        .join("base-views")
        .join(&row.name);
    let source_git = source_git.to_string_lossy().into_owned();
    let base = sandbox_git(
        hub,
        row,
        &[
            "--git-dir",
            &source_git,
            "merge-base",
            "--is-ancestor",
            &row.base_tip,
            tip,
        ],
    )?;
    if base != 0 {
        if base != 1 {
            return Err("base ancestry check failed".into());
        }
        hub.sleeves
            .set_base_tip(&row.name, &row.base_tip, tip)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let own = sandbox_git(
        hub,
        row,
        &[
            "-C",
            &row.workspace,
            "merge-base",
            "--is-ancestor",
            tip,
            "HEAD",
        ],
    )?;
    if own != 0 {
        let history = sandbox_git_output(
            hub,
            row,
            &[
                "--git-dir",
                &source_git,
                "-c",
                "log.showSignature=false",
                "log",
                "--format=%h %s",
                "-10",
                &format!("{}..{tip}", row.base_tip),
                "--",
            ],
        )?;
        let lines = history
            .lines()
            .map(|line| line.chars().take(160).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let reference = shell_quote_ref(&format!("base/{}", row.base_branch));
        let request_hash = Sha256::digest(format!(
            "{}:{}:{tip}:{}",
            row.base_repo, row.base_tip, row.name
        ));
        let request_id = format!("base-{request_hash:x}");
        hub.mail.send(NewMessage {
            from: "hub".into(), to: row.name.clone(), kind: Kind::NOTICE,
            body: format!("Base branch {} advanced {}..{}.\n{}\nUpdate with: git fetch base && git rebase {}\nNo reply needed.", row.base_branch, &row.base_tip[..10], &tip[..10], lines, reference),
            request_id, recipients: vec![row.name.clone()], ..Default::default()
        }).map_err(|error| error.to_string())?;
    }
    hub.sleeves
        .set_base_tip(&row.name, &row.base_tip, tip)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn shell_quote_ref(reference: &str) -> String {
    if reference
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
    {
        reference.to_owned()
    } else {
        format!("'{}'", reference.replace('\'', "'\"'\"'"))
    }
}

fn sandbox_git(hub: &Hub, row: &Record, args: &[&str]) -> std::result::Result<i32, String> {
    sandbox_git_run(hub, row, args).map(|(status, _)| status)
}

fn sandbox_git_output(
    hub: &Hub,
    row: &Record,
    args: &[&str],
) -> std::result::Result<String, String> {
    let (status, output) = sandbox_git_run(hub, row, args)?;
    if status == 0 {
        Ok(output)
    } else {
        Err(format!("Git helper exited {status}"))
    }
}

fn sandbox_git_run(
    hub: &Hub,
    row: &Record,
    args: &[&str],
) -> std::result::Result<(i32, String), String> {
    let launcher = hub
        .chat_runtime
        .as_ref()
        .ok_or("chat runtime unavailable")?
        .launcher();
    let home = launcher
        .state_root
        .join("helpers")
        .join(format!("base-{}", row.name))
        .join("home");
    fs::create_dir_all(&home).map_err(|error| error.to_string())?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let mut command = vec![
        "/usr/bin/git".into(),
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-c".into(),
        "core.fsmonitor=false".into(),
    ];
    command.extend(args.iter().map(|arg| (*arg).to_owned()));
    let grants = base_grants(&launcher.state_root, &row.name, &row.base_repo)
        .map_err(|error| error.to_string())?;
    let policy = Policy {
        state_root: launcher.state_root.clone(),
        user_home: launcher.user_home.clone(),
        protected: Vec::new(),
        provider_credentials: BTreeMap::new(),
        trusted_tool_roots: Vec::new(),
        immutable_cache_roots: Vec::new(),
        nested_cgroups_available: false,
    };
    let plan = policy
        .plan(Request {
            workload: Workload::ReadOnlyHelper(Helper::Git),
            command,
            workspace: Some(PathBuf::from(&row.workspace)),
            managed_workspace_name: protectorate_sandbox::managed_workspace_name(
                &launcher.state_root,
                Path::new(&row.workspace),
            ),
            private_home: Some(home),
            grants,
            tool_roots: Vec::new(),
            selected_executable: None,
            credential: None,
            overlays: Vec::new(),
            masks: Vec::new(),
            limits: SandboxLimits {
                cpu_percent: 100,
                memory_bytes: 128 << 20,
                tasks_max: 24,
            },
        })
        .map_err(|error| error.to_string())?;
    let mut options = LaunchOptions::new(
        &launcher.helper,
        &launcher.runtime_dir,
        &launcher.user_home,
        Path::new(&row.workspace),
    );
    options.setup_timeout = Duration::from_secs(5);
    options.environment = BTreeMap::from([
        (
            "HOME".into(),
            launcher.user_home.to_string_lossy().into_owned(),
        ),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
        ("GIT_ATTR_NOSYSTEM".into(), "1".into()),
        ("GIT_PAGER".into(), "cat".into()),
    ]);
    let mut child =
        protectorate_sandbox::runtime::spawn(&plan, &options).map_err(|error| error.to_string())?;
    let stdout = child.take_stdout().ok_or("Git helper stdout unavailable")?;
    let stderr = child.take_stderr().ok_or("Git helper stderr unavailable")?;
    let out_reader = thread::spawn(move || read_git_output(stdout, 16 * 1024));
    let err_reader = thread::spawn(move || read_git_output(stderr, 16 * 1024));
    let kill = child.kill_handle();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    let status = match rx.recv_timeout(Duration::from_secs(12)) {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(_) => {
            kill.kill();
            return Err("Git helper timed out".into());
        }
    };
    let output = out_reader.join().unwrap_or_default();
    let error = err_reader.join().unwrap_or_default();
    if status.code().is_none() {
        return Err(format!("Git helper {args:?}: {}", error.trim()));
    }
    Ok((status.code().unwrap_or(128), output))
}
