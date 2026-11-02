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

const MASK_SIZE_BYTES: u64 = 1 << 20;

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
    pub scratch: PathBuf,
    pub ro_paths: Vec<PathBuf>,
    /// Never `/`. A writable path may hold masked paths; the masks still apply inside it.
    pub rw_paths: Vec<PathBuf>,
    /// Git dirs of a child worktree (see `resolve_git_dirs`): bound writable, with `hooks/` over
    /// them read-only so a sleeve cannot plant a hook that runs outside the sandbox. They may not
    /// overlap a mask.
    pub git_dirs: Vec<PathBuf>,
    pub tmp_size_bytes: u64,
    /// Directories are covered with a tmpfs, files with /dev/null. Missing paths are skipped. A
    /// mask holds over every bind that contains it; a bind inside a mask (a home inside the
    /// masked state dir) is still reachable. The host's runtime dir is always replaced by a
    /// private empty one.
    pub masks: Vec<PathBuf>,
    /// Host device nodes bound at the same path when they exist. Missing paths are skipped.
    pub devices: Vec<PathBuf>,
    /// Host variables copied in when set (the allowlist).
    pub env_allow: Vec<String>,
    /// Set after the allowlist, so these win.
    pub env: Vec<(String, String)>,
    pub bwrap: PathBuf,
}

impl SandboxSpec {
    pub fn new(
        sleeve: &str,
        generation: u64,
        workspace: PathBuf,
        home: PathBuf,
        scratch: PathBuf,
    ) -> SandboxSpec {
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
            scratch,
            ro_paths: Vec::new(),
            rw_paths: Vec::new(),
            git_dirs: Vec::new(),
            tmp_size_bytes: 1 << 30,
            masks: Vec::new(),
            devices: Vec::new(),
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
            // The default stop policy SIGKILLs the whole scope after one child is OOM-killed,
            // so a build dies with the harness and the guardrail cannot tell them apart.
            args.extend(["-p".into(), "OOMPolicy=continue".into()]);
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
        let scratch = existing(&self.scratch).ok_or_else(|| {
            Error(format!(
                "sandbox scratch {} does not exist",
                self.scratch.display()
            ))
        })?;
        bind(a, "--bind", &scratch, Path::new("/tmp/pt"));

        let runtime = existing(&runtime_dir());
        let mounts = self.mounts(runtime.as_deref())?;
        for m in &mounts {
            let p = path_arg(&m.path)?;
            match m.kind {
                Kind::Ro => a.extend(["--ro-bind".into(), p.clone(), p]),
                Kind::Rw => a.extend(["--bind".into(), p.clone(), p]),
                Kind::MaskDir => a.extend([
                    "--size".into(),
                    MASK_SIZE_BYTES.to_string(),
                    "--tmpfs".into(),
                    p,
                ]),
                Kind::MaskFile => a.extend(["--ro-bind".into(), "/dev/null".into(), p]),
                Kind::Runtime => {
                    a.extend([
                        "--perms".into(),
                        "0700".into(),
                        "--size".into(),
                        MASK_SIZE_BYTES.to_string(),
                        "--tmpfs".into(),
                        p,
                    ]);
                }
            }
        }
        // After the fresh /dev and any mask, so a listed node stays visible.
        for dev in &self.devices {
            let p = path_arg(dev)?;
            if !dev.exists() {
                continue;
            }
            a.extend(["--dev-bind".into(), p.clone(), p]);
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
        env.push(("TMPDIR".into(), "/tmp/pt/tmp".into()));
        env.push(("CARGO_TARGET_DIR".into(), "/tmp/pt/target".into()));
        if let Some(rt) = &runtime {
            env.push(("XDG_RUNTIME_DIR".into(), path_arg(rt)?));
        }
        // A quota does not change the core count tools detect, so each would start a worker per
        // host core and the scope would run out of memory.
        if self.scope && self.limits.cpu_quota_percent > 0 {
            let jobs = (self.limits.cpu_quota_percent / 100).max(1).to_string();
            env.push(("CARGO_BUILD_JOBS".into(), jobs.clone()));
            env.push(("GOMAXPROCS".into(), jobs.clone()));
            env.push(("GOFLAGS".into(), format!("-p={jobs}")));
            env.push(("FLUTTER_TEST_CONCURRENCY".into(), jobs));
        }
        env.extend(self.env.iter().cloned());
        for (k, v) in env {
            a.extend(["--setenv".into(), k, v]);
        }
        Ok(())
    }

    /// Every mount over the read-only root, by canonical path, ordered so that a mount on a path
    /// always comes after the mounts on its ancestors. A mask inside a writable bind therefore
    /// still holds, and a home inside a masked state dir is still reachable.
    fn mounts(&self, runtime: Option<&Path>) -> Result<Vec<Mount>> {
        let mut masks = Vec::new();
        for m in &self.masks {
            if let Some(path) = existing(m) {
                let kind = if path.is_dir() {
                    Kind::MaskDir
                } else {
                    Kind::MaskFile
                };
                masks.push(Mount { path, kind });
            }
        }
        if let Some(rt) = runtime {
            masks.push(Mount {
                path: rt.to_path_buf(),
                kind: Kind::Runtime,
            });
        }

        let mut binds = Vec::new();
        let mut add = |p: &Path, kind: Kind, what: &str| -> Result<()> {
            path_arg(p)?;
            let Some(path) = existing(p) else {
                return err(format!("sandbox {what} {} does not exist", p.display()));
            };
            if kind == Kind::Rw && path.parent().is_none() {
                return err(format!(
                    "sandbox {what} {} would make / writable",
                    p.display()
                ));
            }
            if let Some(m) = masks.iter().find(|m| m.path == path) {
                return err(format!(
                    "sandbox {what} {} is masked ({})",
                    p.display(),
                    m.path.display()
                ));
            }
            binds.push(Mount { path, kind });
            Ok(())
        };
        for p in &self.ro_paths {
            add(p, Kind::Ro, "read-only path")?;
        }
        add(&self.workspace, Kind::Rw, "workspace")?;
        add(&self.home, Kind::Rw, "private home")?;
        for p in &self.rw_paths {
            add(p, Kind::Rw, "writable path")?;
        }
        for g in &self.git_dirs {
            add(g, Kind::Rw, "git dir")?;
            let hooks = g.join("hooks");
            if hooks.is_dir() {
                add(&hooks, Kind::Ro, "git hooks dir")?;
            }
        }
        // Git dirs come from files a sleeve can rewrite, so they may not overlap a mask at all.
        for g in self.git_dirs.iter().filter_map(|g| existing(g)) {
            if let Some(m) = masks
                .iter()
                .find(|m| m.path.starts_with(&g) || g.starts_with(&m.path))
            {
                return err(format!(
                    "sandbox git dir {} overlaps the mask {}",
                    g.display(),
                    m.path.display()
                ));
            }
        }

        let mut all = binds;
        all.extend(masks);
        all.sort_by_key(|m| m.path.components().count());
        Ok(all)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ro,
    Rw,
    MaskDir,
    MaskFile,
    /// The private runtime dir: an empty tmpfs over the host's, so the user bus, the systemd
    /// user manager and every other per-user control socket stay out of reach.
    Runtime,
}

struct Mount {
    path: PathBuf,
    kind: Kind,
}

fn existing(p: &Path) -> Option<PathBuf> {
    fs::canonicalize(p).ok()
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

fn clean_scope(sleeve: &str) -> String {
    sleeve
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `pt-<sleeve>-g<generation>.scope` with every character outside `[A-Za-z0-9_-]` replaced.
pub fn scope_name(sleeve: &str, generation: u64) -> String {
    format!("pt-{}-g{generation}.scope", clean_scope(sleeve))
}

/// Sleeve id in `pt-<tag>-<id>-g<generation>.scope` for this hub's `tag`.
/// A job scope (`pt-<tag>-j<id>-gN.scope`) is not a sleeve.
pub fn scope_sleeve_id(tag: &str, unit: &str) -> Option<u64> {
    let rest = unit.strip_prefix("pt-")?.strip_suffix(".scope")?;
    let (body, generation) = rest.rsplit_once("-g")?;
    if generation.is_empty() || !generation.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let id = body.strip_prefix(&format!("{}-", clean_scope(tag)))?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

/// Whether this user has a systemd bus. Scopes cannot exist without one.
pub fn user_bus_ready() -> bool {
    runtime_dir().join("bus").exists()
}

/// Loaded and inactive user scopes. The unit name is the first field.
pub fn list_scope_units() -> Result<Vec<String>> {
    let out = systemctl(&[
        "list-units",
        "--type=scope",
        "--all",
        "--no-legend",
        "--plain",
        "--no-pager",
    ])?;
    if !out.status.success() {
        return err(format!(
            "systemctl list-units: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            name.ends_with(".scope").then(|| name.to_string())
        })
        .collect())
}

/// The git dirs a worktree `workspace` of the repository at `repo` needs: its private gitdir and
/// the common dir. The files naming them are writable inside sandboxes, so both are checked
/// against `repo`: the private gitdir must be `<common>/worktrees/<name>`, point back at this
/// workspace and name `<common>` as its common dir, where `<common>` is `repo`'s git dir.
pub fn resolve_git_dirs(workspace: &Path, repo: &Path) -> Result<Vec<PathBuf>> {
    let ctx = |what: &Path, e: std::io::Error| {
        Error(format!(
            "resolve git dirs of {}: {}: {e}",
            workspace.display(),
            what.display()
        ))
    };
    let canon = |p: PathBuf| fs::canonicalize(&p).map_err(|e| ctx(&p, e));
    let read = |p: PathBuf| fs::read_to_string(&p).map_err(|e| ctx(&p, e));
    let common = common_dir(repo)?;
    let dot = workspace.join(".git");
    let gitdir = canon(workspace.join(gitdir_line(&read(dot.clone())?, &dot)?))?;
    if gitdir.parent() != Some(common.join("worktrees").as_path()) {
        return err(format!(
            "{} names {}, which is not a worktree of {}",
            dot.display(),
            gitdir.display(),
            common.display()
        ));
    }
    let back = canon(gitdir.join(read(gitdir.join("gitdir"))?.trim()))?;
    let named = canon(gitdir.join(read(gitdir.join("commondir"))?.trim()))?;
    if back != canon(dot.clone())? || named != common {
        return err(format!(
            "{} does not belong to {}",
            gitdir.display(),
            dot.display()
        ));
    }
    Ok(vec![gitdir, common])
}

/// The real git dir of the repository at `repo`, which may itself be a worktree.
fn common_dir(repo: &Path) -> Result<PathBuf> {
    let ctx = |what: &Path, e: std::io::Error| {
        Error(format!(
            "git dir of {}: {}: {e}",
            repo.display(),
            what.display()
        ))
    };
    let canon = |p: PathBuf| fs::canonicalize(&p).map_err(|e| ctx(&p, e));
    let dot = repo.join(".git");
    let dir = if dot.is_dir() {
        canon(dot)?
    } else {
        let text = fs::read_to_string(&dot).map_err(|e| ctx(&dot, e))?;
        let own = canon(repo.join(gitdir_line(&text, &dot)?))?;
        match fs::read_to_string(own.join("commondir")) {
            Ok(c) => canon(own.join(c.trim()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => own,
            Err(e) => return Err(ctx(&own.join("commondir"), e)),
        }
    };
    if dir.parent().is_none() || !dir.join("HEAD").is_file() || !dir.join("objects").is_dir() {
        return err(format!(
            "git dir {} of {} is not a repository",
            dir.display(),
            repo.display()
        ));
    }
    Ok(dir)
}

fn gitdir_line<'a>(text: &'a str, dot: &Path) -> Result<&'a str> {
    match text.lines().find_map(|l| l.strip_prefix("gitdir:")) {
        Some(rest) if !rest.trim().is_empty() => Ok(rest.trim()),
        _ => err(format!("{} has no gitdir line", dot.display())),
    }
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
