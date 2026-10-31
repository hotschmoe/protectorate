//! Linux job processes: a transient systemd scope per job (`systemd-run --scope`, the user
//! manager unless the agent runs as root), or a plain process group when systemd-run does not
//! work here (REVISIT). Either way the job leads its own process group, so a signal reaches the
//! whole tree; a scope also catches processes that leave the group.

use super::linux::ffi;
use super::{Orphan, Receipt};
use crate::exec;
use pt_frame::msg::{Isolation, SubmitJob};
use std::fs;
use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

/// The scope's own time limit sits this far past the job's, so a job outlives a dead agent by
/// a bounded time only.
const BACKSTOP_SECS: u64 = 60;
const SYSTEMCTL_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Isolator {
    scope: bool,
    user: bool,
}

impl Isolator {
    /// Chooses scopes when a trial `systemd-run --scope` succeeds; the note says what was chosen.
    pub fn probe(allow_scope: bool) -> (Isolator, String) {
        let user = ffi::euid() != 0;
        let groups = Isolator { scope: false, user };
        if !allow_scope {
            return (groups, "process groups (scopes turned off)".into());
        }
        let mut args = manager(user);
        args.extend([
            "--scope",
            "--quiet",
            "--collect",
            "-p",
            "RuntimeMaxSec=30",
            "--",
            "true",
        ]);
        match exec::run("systemd-run", &args, Duration::from_secs(15)) {
            Some(o) if o.success => {
                let which = if user { "user" } else { "system" };
                (
                    Isolator { scope: true, user },
                    format!("systemd {which} scopes"),
                )
            }
            Some(o) => (
                groups,
                format!("process groups (systemd-run failed: {})", o.stderr.trim()),
            ),
            None => (groups, "process groups (systemd-run unavailable)".into()),
        }
    }

    pub fn kind(&self) -> Isolation {
        if self.scope {
            Isolation::Scope
        } else {
            Isolation::Group
        }
    }

    /// `unit` names the scope (without `.scope`); `out` and `err` take the job's output.
    pub fn spawn(
        &self,
        spec: &SubmitJob,
        unit: &str,
        cwd: &Path,
        out: Stdio,
        err: Stdio,
    ) -> io::Result<Proc> {
        let (prog, args): (&str, Vec<&str>) = match (&spec.argv, &spec.script) {
            (Some(argv), None) if !argv.is_empty() => {
                (&argv[0], argv[1..].iter().map(String::as_str).collect())
            }
            (None, Some(script)) => ("/bin/sh", vec!["-c", script]),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a job needs exactly one of argv and script",
                ));
            }
        };
        let mut cmd = if self.scope {
            let mut c = Command::new("systemd-run");
            c.args(manager(self.user))
                .args(["--scope", "--quiet", "--collect"])
                .arg(format!("--unit={unit}"))
                .arg("-p")
                .arg(format!(
                    "RuntimeMaxSec={}",
                    spec.timeout_secs.saturating_add(BACKSTOP_SECS)
                ))
                .arg("--")
                .arg(prog)
                .args(&args);
            c
        } else {
            let mut c = Command::new(prog);
            c.args(&args);
            c
        };
        cmd.current_dir(cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .process_group(0);
        let mut child = cmd.spawn()?;
        let start_time = start_time(child.id());
        let scope = self.scope.then(|| format!("{unit}.scope"));
        if let Some(s) = &scope {
            in_scope(&mut child, s);
        }
        Ok(Proc {
            child,
            start_time,
            scope,
            user: self.user,
        })
    }

    /// After a restart: kills what is left of a job the journal shows unfinished.
    pub fn reap_orphan(&self, receipt: Option<&Receipt>, unit: &str) -> Orphan {
        let mut killed = false;
        if let Some(r) = receipt
            && r.start_time.is_some()
            && start_time(r.pid) == r.start_time
        {
            killed |= ffi::kill_group(r.pid, libc::SIGKILL).is_ok();
        }
        let scope = receipt
            .and_then(|r| r.scope.clone())
            .or_else(|| self.scope.then(|| format!("{unit}.scope")));
        if let Some(scope) = scope
            && scope_active(self.user, &scope)
        {
            systemctl(self.user, &["kill", "--signal=SIGKILL", "--", &scope]);
            killed = true;
        }
        if killed { Orphan::Killed } else { Orphan::Gone }
    }
}

pub struct Proc {
    child: Child,
    start_time: Option<u64>,
    scope: Option<String>,
    user: bool,
}

impl Proc {
    pub fn receipt(&self) -> Receipt {
        Receipt {
            pid: self.child.id(),
            start_time: self.start_time,
            scope: self.scope.clone(),
        }
    }

    /// The exit code once the leader has exited (128 + signal for a signal death).
    pub fn try_wait(&mut self) -> io::Result<Option<i64>> {
        Ok(self.child.try_wait()?.map(exit_code))
    }

    /// Asks the whole tree to stop.
    pub fn terminate(&mut self) {
        self.signal(libc::SIGTERM, "SIGTERM");
    }

    /// Kills the whole tree, including anything left after the leader exited.
    pub fn kill(&mut self) {
        self.signal(libc::SIGKILL, "SIGKILL");
    }

    fn signal(&self, sig: i32, name: &str) {
        let _ = ffi::kill_group(self.child.id(), sig);
        if let Some(scope) = &self.scope {
            systemctl(
                self.user,
                &["kill", &format!("--signal={name}"), "--", scope],
            );
        }
    }

    /// Nice 19 and idle I/O for the tree, and for every process in the scope.
    pub fn lower(&mut self) -> io::Result<()> {
        let pid = self.child.id();
        let result = super::lower_to_idle(pid);
        if self.scope.is_some() {
            for p in cgroup_members(pid) {
                if p != pid {
                    let _ = super::lower_to_idle(p);
                }
            }
        }
        result
    }
}

/// Waits until systemd-run has moved the job into its scope (or exited), so the job is never
/// signalled mid-registration: a scope registered after its only process died stays active,
/// empty, until its time limit.
fn in_scope(child: &mut Child, scope: &str) {
    let suffix = format!("/{scope}");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let cg = fs::read_to_string(format!("/proc/{}/cgroup", child.id())).unwrap_or_default();
        if cg.lines().any(|l| l.trim_end().ends_with(&suffix)) {
            return;
        }
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn manager(user: bool) -> Vec<&'static str> {
    if user { vec!["--user"] } else { Vec::new() }
}

fn systemctl(user: bool, args: &[&str]) -> Option<String> {
    let mut all = manager(user);
    all.extend_from_slice(args);
    exec::run("systemctl", &all, SYSTEMCTL_TIMEOUT).map(|o| o.stdout)
}

fn scope_active(user: bool, scope: &str) -> bool {
    systemctl(user, &["is-active", "--", scope]).is_some_and(|s| {
        matches!(
            s.trim(),
            "active" | "activating" | "deactivating" | "reloading"
        )
    })
}

fn exit_code(s: ExitStatus) -> i64 {
    match (s.code(), s.signal()) {
        (Some(c), _) => i64::from(c),
        (None, Some(sig)) => 128 + i64::from(sig),
        (None, None) => -1,
    }
}

/// Field 22 of `/proc/<pid>/stat`; `None` for a missing or zombie process.
fn start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let mut fields = stat.rsplit_once(')')?.1.split_whitespace();
    if fields.next()? == "Z" {
        return None;
    }
    fields.nth(18)?.parse().ok()
}

fn cgroup_members(pid: u32) -> Vec<u32> {
    let Ok(cg) = fs::read_to_string(format!("/proc/{pid}/cgroup")) else {
        return Vec::new();
    };
    let Some(path) = cg.lines().find_map(|l| l.strip_prefix("0::")) else {
        return Vec::new();
    };
    fs::read_to_string(format!("/sys/fs/cgroup{path}/cgroup.procs"))
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}
