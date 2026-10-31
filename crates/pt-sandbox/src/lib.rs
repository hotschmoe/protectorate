#![forbid(unsafe_code)]
//! Builds the `systemd-run --user --scope` plus `bwrap` command prefix for one sleeve
//! incarnation. The harness driver appends the harness argv to the returned `Command`.
//!
//! Light isolation: it prevents accidents, it is not a boundary against a hostile agent.

use std::fmt;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn err<T>(msg: String) -> Result<T> {
    Err(Error(msg))
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub memory_max_bytes: u64,
    pub cpu_quota_percent: u32,
    pub tasks_max: u32,
}

#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub sleeve: String,
    pub generation: u64,
    /// Wrap in a systemd scope. Turn off only when scopes are unavailable (see REVISIT.md).
    pub scope: bool,
    pub limits: Limits,
    pub workspace: PathBuf,
    /// Private home, writable. Becomes `$HOME` inside. The hub fills it; we never do.
    pub home: PathBuf,
    pub ro_paths: Vec<PathBuf>,
    pub rw_paths: Vec<PathBuf>,
    /// Git dirs of a child worktree (see `resolve_git_dirs`): bound writable, with `hooks/` over
    /// them read-only so a sleeve cannot plant a hook that runs outside the sandbox.
    pub git_dirs: Vec<PathBuf>,
    pub tmp_size_bytes: u64,
    /// Directories are covered with a tmpfs, files with /dev/null. Missing paths are skipped.
    pub masks: Vec<PathBuf>,
    /// Host variables copied in when set (the allowlist).
    pub env_allow: Vec<String>,
    /// Set after the allowlist, so these win.
    pub env: Vec<(String, String)>,
    pub bwrap: PathBuf,
}

impl SandboxSpec {
    pub fn new(sleeve: &str, generation: u64, workspace: PathBuf, home: PathBuf) -> SandboxSpec {
        SandboxSpec {
            sleeve: sleeve.to_string(),
            generation,
            scope: true,
            limits: Limits {
                memory_max_bytes: 4 << 30,
                cpu_quota_percent: 400,
                tasks_max: 4096,
            },
            workspace,
            home,
            ro_paths: Vec::new(),
            rw_paths: Vec::new(),
            git_dirs: Vec::new(),
            tmp_size_bytes: 1 << 30,
            masks: Vec::new(),
            env_allow: default_env_allow(),
            env: Vec::new(),
            bwrap: PathBuf::from("bwrap"),
        }
    }

    pub fn scope_name(&self) -> String {
        scope_name(&self.sleeve, self.generation)
    }

    pub fn command(&self) -> Result<Command> {
        let mut args: Vec<String> = Vec::new();
        let bwrap = self.bwrap.to_string_lossy().into_owned();
        let mut cmd;
        if self.scope {
            cmd = Command::new("systemd-run");
            args.extend(
                [
                    "--user",
                    "--scope",
                    "--quiet",
                    "--collect",
                    "--unit",
                    &self.scope_name(),
                ]
                .map(String::from),
            );
            args.push("-p".into());
            args.push(format!("MemoryMax={}", self.limits.memory_max_bytes));
            args.extend(["-p".into(), "MemorySwapMax=0".into()]);
            args.push("-p".into());
            args.push(format!("CPUQuota={}%", self.limits.cpu_quota_percent));
            args.push("-p".into());
            args.push(format!("TasksMax={}", self.limits.tasks_max));
            args.push(bwrap);
        } else {
            cmd = Command::new(&bwrap);
        }
        self.bwrap_args(&mut args)?;
        cmd.args(&args);
        cmd.env_clear();
        for key in ["PATH", "DBUS_SESSION_BUS_ADDRESS"] {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        cmd.env("XDG_RUNTIME_DIR", runtime_dir());
        Ok(cmd)
    }

    fn bwrap_args(&self, a: &mut Vec<String>) -> Result<()> {
        for f in [
            "--unshare-user",
            "--unshare-pid",
            "--unshare-ipc",
            "--unshare-uts",
            "--die-with-parent",
        ] {
            a.push(f.into());
        }
        bind(a, "--ro-bind", Path::new("/"), Path::new("/"));
        a.extend([
            "--dev".into(),
            "/dev".into(),
            "--proc".into(),
            "/proc".into(),
        ]);
        a.extend(["--size".into(), self.tmp_size_bytes.to_string()]);
        a.extend(["--tmpfs".into(), "/tmp".into()]);

        for m in &self.masks {
            match fs::metadata(m) {
                Ok(md) if md.is_dir() => a.extend(["--tmpfs".into(), path_arg(m)?]),
                Ok(_) => bind(a, "--ro-bind", Path::new("/dev/null"), m),
                Err(_) => {}
            }
        }
        // Binds come after the masks: the home and workspace may live under a masked directory
        // such as the hub state dir.
        for p in &self.ro_paths {
            require_exists(p, "read-only path")?;
            bind(a, "--ro-bind", p, p);
        }
        require_exists(&self.workspace, "workspace")?;
        bind(a, "--bind", &self.workspace, &self.workspace);
        require_exists(&self.home, "private home")?;
        bind(a, "--bind", &self.home, &self.home);
        for p in self.rw_paths.iter().chain(&self.git_dirs) {
            require_exists(p, "writable path")?;
            bind(a, "--bind", p, p);
        }
        for g in &self.git_dirs {
            let hooks = g.join("hooks");
            if hooks.is_dir() {
                bind(a, "--ro-bind", &hooks, &hooks);
            }
        }

        a.extend(["--chdir".into(), path_arg(&self.workspace)?]);
        a.push("--clearenv".into());
        let mut env: Vec<(String, String)> = Vec::new();
        for k in &self.env_allow {
            if let Ok(v) = std::env::var(k) {
                env.push((k.clone(), v));
            }
        }
        env.push(("HOME".into(), path_arg(&self.home)?));
        env.push(("TMPDIR".into(), "/tmp".into()));
        env.extend(self.env.iter().cloned());
        for (k, v) in env {
            a.extend(["--setenv".into(), k, v]);
        }
        Ok(())
    }
}

fn bind(a: &mut Vec<String>, flag: &str, src: &Path, dst: &Path) {
    a.push(flag.into());
    a.push(src.to_string_lossy().into_owned());
    a.push(dst.to_string_lossy().into_owned());
}

fn path_arg(p: &Path) -> Result<String> {
    match p.to_str() {
        Some(s) if p.is_absolute() => Ok(s.to_string()),
        _ => err(format!(
            "sandbox path {} is not absolute UTF-8",
            p.display()
        )),
    }
}

fn require_exists(p: &Path, what: &str) -> Result<()> {
    path_arg(p)?;
    if p.exists() {
        Ok(())
    } else {
        err(format!("sandbox {what} {} does not exist", p.display()))
    }
}

pub fn default_env_allow() -> Vec<String> {
    [
        "PATH",
        "LANG",
        "LANGUAGE",
        "LC_ALL",
        "LC_CTYPE",
        "TERM",
        "COLORTERM",
        "TZ",
        "USER",
        "LOGNAME",
        "SHELL",
    ]
    .map(String::from)
    .to_vec()
}

/// Credential locations under the owner's home that no sleeve should see by accident.
pub fn default_masks(host_home: &Path) -> Vec<PathBuf> {
    [
        ".ssh",
        ".claude",
        ".claude.json",
        ".codex",
        ".grok",
        ".pi",
        ".config/gh",
        ".git-credentials",
        ".config/git/credentials",
        ".netrc",
        ".gnupg",
        ".aws",
        ".config/gcloud",
        ".azure",
        ".docker",
        ".kube",
    ]
    .iter()
    .map(|r| host_home.join(r))
    .collect()
}

/// `pt-<sleeve>-g<generation>.scope` with every character outside `[A-Za-z0-9_-]` replaced.
pub fn scope_name(sleeve: &str, generation: u64) -> String {
    let clean: String = sleeve
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("pt-{clean}-g{generation}.scope")
}

/// The git dirs a worktree workspace needs: its private gitdir and the common dir. A plain
/// repository yields its `.git` directory alone.
pub fn resolve_git_dirs(workspace: &Path) -> Result<Vec<PathBuf>> {
    let dot = workspace.join(".git");
    let ctx = |e: std::io::Error| Error(format!("resolve git dir of {}: {e}", workspace.display()));
    if dot.is_dir() {
        return Ok(vec![dot]);
    }
    let text = fs::read_to_string(&dot).map_err(ctx)?;
    let Some(rest) = text.lines().find_map(|l| l.strip_prefix("gitdir:")) else {
        return err(format!("{} has no gitdir line", dot.display()));
    };
    let gitdir = workspace.join(rest.trim());
    let gitdir = fs::canonicalize(&gitdir).map_err(ctx)?;
    let mut out = vec![gitdir.clone()];
    if let Ok(c) = fs::read_to_string(gitdir.join("commondir")) {
        let common = fs::canonicalize(gitdir.join(c.trim())).map_err(ctx)?;
        if common != gitdir {
            out.push(common);
        }
    }
    Ok(out)
}

fn runtime_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        return d.into();
    }
    let uid = fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
    PathBuf::from(format!("/run/user/{uid}"))
}

fn systemctl(args: &[&str]) -> Result<std::process::Output> {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .env("XDG_RUNTIME_DIR", runtime_dir())
        .output()
        .map_err(|e| Error(format!("run systemctl --user {}: {e}", args.join(" "))))
}

/// The scope's cgroup directory, or `None` when the scope is not loaded.
pub fn scope_cgroup(name: &str) -> Result<Option<PathBuf>> {
    let out = systemctl(&["show", "-p", "ControlGroup", "--value", name])?;
    if !out.status.success() {
        return err(format!(
            "systemctl show {name}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let cg = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if cg.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        Path::new("/sys/fs/cgroup").join(cg.trim_start_matches('/')),
    ))
}

/// Pids (host namespace) in the scope. `Ok(None)` means the scope is gone or empty, which is the
/// confirmed-dead answer the resleeve rule needs; `Err` means the probe itself failed.
pub fn scope_alive(name: &str) -> Result<Option<Vec<u32>>> {
    let Some(cg) = scope_cgroup(name)? else {
        return Ok(None);
    };
    let procs = match fs::read_to_string(cg.join("cgroup.procs")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return err(format!("read cgroup.procs of {name}: {e}")),
    };
    let pids: Vec<u32> = procs
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    Ok(if pids.is_empty() { None } else { Some(pids) })
}

/// SIGKILLs every process in the scope at once through `cgroup.kill`, falling back to systemctl.
/// Returns after the signal is sent; poll `scope_alive` for the death.
pub fn kill_scope(name: &str) -> Result<()> {
    let Some(cg) = scope_cgroup(name)? else {
        return Ok(());
    };
    match fs::write(cg.join("cgroup.kill"), "1") {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(first) => {
            let out = systemctl(&["kill", "--signal=SIGKILL", "--kill-whom=all", name])?;
            if out.status.success() {
                Ok(())
            } else {
                err(format!(
                    "kill scope {name}: cgroup.kill: {first}; systemctl: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        }
    }
}
