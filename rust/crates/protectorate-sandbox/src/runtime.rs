//! First Linux launcher slice. Namespace operations happen only in a fresh,
//! single-threaded helper; this module never unshares the calling hub process.
//! Requires Linux openat2/open_tree/mount_setattr/move_mount, unprivileged user
//! namespaces and a functioning systemd user manager for ordinary cgroup limits.
use crate::{egress, Access, Cgroup, Network, PlanKind, ValidatedPlan, Workload};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsStr},
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt, net::UnixStream, process::CommandExt},
    },
    path::{Component, Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
const HELPER_FLAG: &str = "--internal-protectorate-sandbox-v1";
const CONTROL_FD: RawFd = 3;
const MAX_PLAN_BYTES: usize = 4 << 20;
const AT_EMPTY_PATH: u32 = 0x1000;
const AT_RECURSIVE: u32 = 0x8000;
const OPEN_TREE_CLONE: u32 = 1;
const MOVE_MOUNT_F_EMPTY_PATH: u32 = 4;
const MOVE_MOUNT_T_EMPTY_PATH: u32 = 0x40;
const MOUNT_ATTR_RDONLY: u64 = 1;
const MOUNT_ATTR_NOSUID: u64 = 2;
const MOUNT_ATTR_NODEV: u64 = 4;
#[derive(Debug)]
pub enum LaunchError {
    Unsupported(String),
    Invalid(String),
    Io {
        operation: String,
        source: io::Error,
    },
    Setup(String),
}
impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(s) => write!(f, "unsupported sandbox capability: {s}"),
            Self::Invalid(s) => write!(f, "invalid sandbox launch: {s}"),
            Self::Io { operation, source } => write!(f, "sandbox {operation}: {source}"),
            Self::Setup(s) => write!(f, "sandbox setup failed: {s}"),
        }
    }
}
impl std::error::Error for LaunchError {}
pub type Result<T> = std::result::Result<T, LaunchError>;
fn ioerr(op: impl Into<String>) -> impl FnOnce(io::Error) -> LaunchError {
    let operation = op.into();
    move |source| LaunchError::Io { operation, source }
}
fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(LaunchError::Invalid(message.into()))
}
fn cstr(s: impl AsRef<OsStr>) -> Result<CString> {
    CString::new(s.as_ref().as_bytes())
        .map_err(|_| LaunchError::Invalid("NUL in path or argument".into()))
}
fn syscall_ok(value: libc::c_long, op: &str) -> Result<libc::c_long> {
    if value < 0 {
        Err(ioerr(op)(io::Error::last_os_error()))
    } else {
        Ok(value)
    }
}
fn owned_fd(value: libc::c_long, op: &str) -> Result<File> {
    let fd = syscall_ok(value, op)? as RawFd; // SAFETY: successful syscall returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
#[derive(Clone)]
pub struct LaunchOptions {
    pub helper: PathBuf,
    /// Runtime-owned directory outside every mounted source.
    pub runtime_dir: PathBuf,
    /// The host user's HOME path where private_home is mounted.
    pub home_target: PathBuf,
    pub working_directory: PathBuf,
    /// Values travel through the private descriptor, never through argv.
    pub environment: BTreeMap<String, String>,
    pub setup_timeout: Duration,
    /// A terminal sleeve needs the tmux pane's pty; jobs capture output.
    pub stdio: StdioMode,
    /// Receives every egress proxy decision for a proxied plan.
    pub egress_log: Option<egress::Log>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StdioMode {
    #[default]
    Capture,
    /// A headless harness speaks its official RPC protocol over stdin/stdout.
    Duplex,
    Inherit,
}
impl LaunchOptions {
    pub fn new(
        helper: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
        home_target: impl Into<PathBuf>,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            helper: helper.into(),
            runtime_dir: runtime_dir.into(),
            home_target: home_target.into(),
            working_directory: working_directory.into(),
            environment: BTreeMap::new(),
            setup_timeout: Duration::from_secs(15),
            stdio: StdioMode::Capture,
            egress_log: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Plain,
    Device,
    /// Attached after overlays so it can cover a file inside one.
    Mask,
    /// An egress proxy socket owned by the launcher.
    Socket,
}
#[derive(Serialize, Deserialize)]
struct PinnedMount {
    source: PathBuf,
    target: PathBuf,
    read_only: bool,
    directory: bool,
    kind: Kind,
    device_id: u64,
    inode: u64,
}
/// Overlay layers are pinned like every other mount source: the lower is
/// attached read-only at a private staging path. Upper/work are mounted from
/// private workload state into the same transient staging tree. All three
/// staging mounts are detached before the workload runs.
#[derive(Serialize, Deserialize)]
struct PinnedOverlay {
    lower_target: PathBuf,
    upper: PathBuf,
    work: PathBuf,
    target: PathBuf,
    upper_device: u64,
    upper_inode: u64,
    work_device: u64,
    work_inode: u64,
}
#[derive(Serialize, Deserialize)]
struct HelperPlan {
    uid: u32,
    gid: u32,
    root: PathBuf,
    home_target: PathBuf,
    cwd: PathBuf,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
    mounts: Vec<PinnedMount>,
    overlays: Vec<PinnedOverlay>,
    system_links: Vec<(PathBuf, PathBuf)>,
    nested_unit: Option<String>,
    direct_scope: Option<PathBuf>,
    tmp_bytes: u64,
    /// Namespace loopback ports and the in-sandbox sockets they relay to.
    relays: Vec<(u16, PathBuf)>,
}
struct NestedCgroup {
    path: PathBuf,
    device_id: u64,
    inode: u64,
}
fn host_supports_overlay() -> bool {
    fs::read("/proc/filesystems")
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .any(|line| line.split_whitespace().last() == Some("overlay"))
        })
        .unwrap_or(false)
}
const MIN_TMP_BYTES: u64 = 512 << 20;
const MAX_TMP_BYTES: u64 = 4 << 30;

/// /tmp is a tmpfs and /var/tmp is a bind mount of it, so both share one
/// temporary-space ceiling and their contents count against the workload's
/// memory allowance. A fixed 256 MiB cap breaks ordinary builds, because a Go
/// build of a repository of this size peaks near 1.5 GiB in its temporary
/// work directory and fails with "no space left on device". The ceiling
/// scales with the memory the workload is already allowed to use, but always
/// leaves a floor for the workload's own memory: tmpfs pages are charged to
/// the cgroup and cannot be reclaimed without swap, so a ceiling equal to the
/// whole allowance lets a full temporary filesystem push the cgroup past its
/// limit and OOM-kill the workload instead of failing the write with ENOSPC.
/// Other workload memory can still exhaust the cgroup before this ceiling.
pub fn tmpfs_bytes(limits: crate::Limits) -> u64 {
    let allowance = if limits.memory_bytes > 0 {
        limits.memory_bytes / 2
    } else {
        host_memory_total() / 2
    };
    let bounded = allowance.clamp(MIN_TMP_BYTES, MAX_TMP_BYTES);
    if limits.memory_bytes > 0 {
        let headroom = (limits.memory_bytes / 4).max(64 << 20);
        bounded
            .min(limits.memory_bytes.saturating_sub(headroom))
            .max(limits.memory_bytes / 2)
    } else {
        bounded
    }
}

fn host_memory_total() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut fields = line.split_whitespace();
                if fields.next()? != "MemTotal:" {
                    return None;
                }
                fields
                    .next()?
                    .parse::<u64>()
                    .ok()
                    .map(|kilobytes| kilobytes.saturating_mul(1024))
            })
        })
        .unwrap_or(MIN_TMP_BYTES)
}
fn open_resolved(path: &Path, flags: i32) -> Result<File> {
    let path = cstr(path)?;
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = OpenHow {
        flags: (flags | libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve: 0x02 | 0x04,
    }; // NO_MAGICLINKS | NO_SYMLINKS, across every component.
       // SAFETY: pointers describe live NUL-terminated path and correctly sized open_how.
    owned_fd(
        unsafe {
            libc::syscall(
                libc::SYS_openat2,
                libc::AT_FDCWD,
                path.as_ptr(),
                &how,
                std::mem::size_of::<OpenHow>(),
            )
        },
        "pin mount source with openat2 (symlinks are forbidden)",
    )
}
fn open_pinned(path: &Path) -> Result<File> {
    open_resolved(path, libc::O_PATH)
}
fn open_directory(path: &Path) -> Result<File> {
    open_resolved(path, libc::O_RDONLY | libc::O_DIRECTORY)
}
fn pin(
    source: &Path,
    target: &Path,
    read_only: bool,
    kind: Kind,
    files: &mut Vec<File>,
    mounts: &mut Vec<PinnedMount>,
) -> Result<()> {
    if !absolute(source) || !absolute(target) || target == Path::new("/") {
        return invalid("mount paths must be clean absolute paths below root");
    }
    let file = open_pinned(source)?;
    let metadata = file.metadata().map_err(ioerr("stat pinned source"))?;
    let expected = match kind {
        Kind::Plain | Kind::Mask => metadata.is_dir() || metadata.is_file(),
        Kind::Device => metadata.mode() & libc::S_IFMT == libc::S_IFCHR,
        Kind::Socket => metadata.mode() & libc::S_IFMT == libc::S_IFSOCK,
    };
    if !expected {
        return invalid("mount source is not the expected file type");
    }
    mounts.push(PinnedMount {
        source: source.into(),
        target: target.into(),
        read_only,
        directory: metadata.is_dir(),
        kind,
        device_id: metadata.dev(),
        inode: metadata.ino(),
    });
    files.push(file);
    Ok(())
}
/// A child owns only its transient scope and namespace tree. Dropping it kills
/// that scope and waits; no unrelated process or cgroup is touched.
pub struct ChildHandle {
    child: Option<Child>,
    unit: String,
    direct_scope: Option<PathBuf>,
    host_direct: bool,
    alive: Arc<AtomicBool>,
    kill_gate: Arc<Mutex<()>>,
    root: Option<tempfile::TempDir>,
    egress: Option<egress::Proxy>,
    pub init_pid: u32,
}
/// A scoped cancellation handle that can be used while another thread waits
/// for the sandbox process. It never targets an unrelated systemd unit.
#[derive(Clone)]
pub struct KillHandle {
    pid: u32,
    unit: String,
    direct_scope: Option<PathBuf>,
    host_direct: bool,
    alive: Arc<AtomicBool>,
    kill_gate: Arc<Mutex<()>>,
}
impl KillHandle {
    pub fn kill(&self) {
        if !self.alive.load(Ordering::Acquire) {
            return;
        }
        if self.host_direct {
            let _gate = self.kill_gate.lock().unwrap();
            if !self.alive.load(Ordering::Acquire) {
                return;
            }
            if self.pid > 0 {
                unsafe { libc::kill(-(self.pid as i32), libc::SIGKILL) };
            }
            // This transition is serialized with every host group kill. A
            // concurrent handle cannot signal the PGID after the leader is
            // reaped and its numeric PID becomes reusable.
            self.alive.store(false, Ordering::Release);
            return;
        }
        if let Some(scope) = &self.direct_scope {
            let _ = fs::write(scope.join("cgroup.kill"), "1\n");
        } else {
            let _ = Command::new("systemctl")
                .args([
                    "--user",
                    "kill",
                    "--signal=KILL",
                    "--kill-whom=all",
                    &self.unit,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if self.pid > 0 {
            // SAFETY: the helper starts in its own session and process group.
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
        }
    }
}
impl ChildHandle {
    pub fn id(&self) -> u32 {
        self.child.as_ref().map_or(0, Child::id)
    }
    pub fn unit(&self) -> &str {
        &self.unit
    }
    pub fn kill_handle(&self) -> KillHandle {
        KillHandle {
            pid: self.id(),
            unit: self.unit.clone(),
            direct_scope: self.direct_scope.clone(),
            host_direct: self.host_direct,
            alive: Arc::clone(&self.alive),
            kill_gate: Arc::clone(&self.kill_gate),
        }
    }
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }
    pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }
    pub fn cancel(&mut self) -> Result<()> {
        let kill = self.kill_handle();
        if let Some(child) = &mut self.child {
            kill.kill();
            let _ = child.kill();
        }
        Ok(())
    }
    pub fn wait(mut self) -> Result<std::process::ExitStatus> {
        let kill = self.kill_handle();
        let mut child = self
            .child
            .take()
            .ok_or_else(|| LaunchError::Invalid("child already waited".into()))?;
        if self.host_direct {
            // Keep the leader as a zombie until its process group is killed:
            // reaping first would allow the PID (and PGID) to be reused.
            wait_unreaped(child.id())?;
            kill.kill();
        }
        let status = child.wait().map_err(ioerr("wait sandbox"))?;
        self.alive.store(false, Ordering::Release);
        self.cleanup_direct_scope()?;
        Ok(status)
    }
    pub fn wait_with_output(mut self) -> Result<Output> {
        let kill = self.kill_handle();
        let mut child = self
            .child
            .take()
            .ok_or_else(|| LaunchError::Invalid("child already waited".into()))?;
        let output = if self.host_direct {
            drop(child.stdin.take());
            let stdout = child.stdout.take().map(|mut pipe| {
                std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    pipe.read_to_end(&mut bytes).map(|_| bytes)
                })
            });
            let stderr = child.stderr.take().map(|mut pipe| {
                std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    pipe.read_to_end(&mut bytes).map(|_| bytes)
                })
            });
            wait_unreaped(child.id())?;
            kill.kill();
            let status = child.wait().map_err(ioerr("wait direct harness"))?;
            let read = |thread: Option<std::thread::JoinHandle<io::Result<Vec<u8>>>>| {
                thread.map_or(Ok(Vec::new()), |thread| {
                    thread
                        .join()
                        .map_err(|_| LaunchError::Setup("output reader panicked".into()))?
                        .map_err(ioerr("read direct harness output"))
                })
            };
            Output {
                status,
                stdout: read(stdout)?,
                stderr: read(stderr)?,
            }
        } else {
            child.wait_with_output().map_err(ioerr("wait sandbox"))?
        };
        self.alive.store(false, Ordering::Release);
        self.cleanup_direct_scope()?;
        Ok(output)
    }
    fn cleanup_direct_scope(&mut self) -> Result<()> {
        let Some(scope) = self.direct_scope.take() else {
            return Ok(());
        };
        let _ = fs::write(scope.join("cgroup.kill"), "1\n");
        for _ in 0..100 {
            remove_cgroup_tree(&scope)?;
            if !scope.exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(LaunchError::Setup(format!(
            "nested cgroup {} remained after workload exit",
            scope.display()
        )))
    }
}

/// Wait for a direct child to exit without releasing its PID. The caller must
/// kill the process group before it reaps the leader.
fn wait_unreaped(pid: u32) -> Result<()> {
    let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    loop {
        let result =
            unsafe { libc::waitid(libc::P_PID, pid, &mut info, libc::WEXITED | libc::WNOWAIT) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(ioerr("wait for direct harness without reaping")(error));
        }
    }
}
impl Drop for ChildHandle {
    fn drop(&mut self) {
        if self.child.is_some() {
            let _ = self.cancel();
            if let Some(mut child) = self.child.take() {
                let _ = child.wait();
            }
        }
        self.alive.store(false, Ordering::Release);
        let _ = self.cleanup_direct_scope();
    }
}
fn remove_cgroup_tree(path: &Path) -> Result<()> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ioerr("list nested cgroup descendants")(error)),
    };
    for entry in entries {
        let entry = entry.map_err(ioerr("read nested cgroup entry"))?;
        if entry
            .file_type()
            .map_err(ioerr("stat nested cgroup entry"))?
            .is_dir()
        {
            remove_cgroup_tree(&entry.path())?;
        }
    }
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::EBUSY)
                || error.raw_os_error() == Some(libc::ENOTEMPTY) =>
        {
            Ok(())
        }
        Err(error) => Err(ioerr("remove nested cgroup")(error)),
    }
}
struct ScopeGuard(Option<PathBuf>);
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if let Some(scope) = self.0.take() {
            let _ = fs::write(scope.join("cgroup.kill"), "1\n");
            let _ = remove_cgroup_tree(&scope);
        }
    }
}
fn delegated_parent_root() -> Option<PathBuf> {
    let root = PathBuf::from("/run/protectorate/cgroup");
    root.join("cgroup.controllers").is_file().then_some(root)
}
fn create_direct_scope(root: &Path, unit: &str, limits: crate::Limits) -> Result<PathBuf> {
    let controllers = fs::read_to_string(root.join("cgroup.subtree_control"))
        .map_err(ioerr("read nested parent controllers"))?;
    for required in ["cpu", "memory", "pids"] {
        if !controllers.split_whitespace().any(|part| part == required) {
            return Err(LaunchError::Unsupported(format!(
                "nested parent has not enabled {required} controller"
            )));
        }
    }
    let scope = root.join(unit);
    fs::create_dir(&scope).map_err(ioerr("create nested child cgroup"))?;
    let limits_result = (|| {
        if limits.cpu_percent > 0 {
            write_cgroup(
                &scope.join("cpu.max"),
                &format!("{} 100000\n", u64::from(limits.cpu_percent) * 1000),
            )?;
        }
        if limits.memory_bytes > 0 {
            write_cgroup(
                &scope.join("memory.max"),
                &format!("{}\n", limits.memory_bytes),
            )?;
            write_cgroup(&scope.join("memory.swap.max"), "0\n")?;
        }
        write_cgroup(&scope.join("pids.max"), &format!("{}\n", limits.tasks_max))
    })();
    if let Err(error) = limits_result {
        let _ = remove_cgroup_tree(&scope);
        return Err(error);
    }
    Ok(scope)
}
/// Ordinary limits use a bounded transient systemd user scope. Missing user
/// manager/controller support is a launch error; there is no host fallback.
pub fn spawn_direct_harness(plan: &ValidatedPlan, options: &LaunchOptions) -> Result<ChildHandle> {
    if plan.kind() != PlanKind::IntentionallyUnsandboxedHarness {
        return invalid("direct harness launch requires an explicit host driver plan");
    }
    if options.stdio != StdioMode::Duplex || !options.working_directory.is_dir() {
        return invalid("direct harness launch requires duplex stdio and an existing directory");
    }
    let command = plan.command();
    if command.is_empty() || !Path::new(&command[0]).is_absolute() {
        return invalid("direct harness launch requires an absolute executable");
    }
    let mut process = Command::new(&command[0]);
    process
        .args(&command[1..])
        .current_dir(&options.working_directory)
        .envs(&options.environment)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = process.spawn().map_err(ioerr("start direct harness"))?;
    let init_pid = child.id();
    Ok(ChildHandle {
        child: Some(child),
        unit: String::new(),
        direct_scope: None,
        host_direct: true,
        alive: Arc::new(AtomicBool::new(true)),
        kill_gate: Arc::new(Mutex::new(())),
        root: None,
        egress: None,
        init_pid,
    })
}

pub fn spawn(plan: &ValidatedPlan, options: &LaunchOptions) -> Result<ChildHandle> {
    if plan.kind() != PlanKind::Sandboxed {
        return invalid("plain node jobs must use their explicit unsandboxed execution path");
    }
    if !plan.overlays().is_empty() && !host_supports_overlay() {
        return Err(LaunchError::Unsupported(
            "host kernel provides no overlay filesystem for cache overlays".into(),
        ));
    }
    if plan.command().is_empty() || !Path::new(&plan.command()[0]).is_absolute() {
        return Err(LaunchError::Unsupported(
            "this launcher requires an absolute executable path".into(),
        ));
    }
    if !absolute(&options.home_target)
        || options.home_target.components().count() < 3
        || options.home_target.starts_with("/.overlay")
        || ["/usr", "/etc", "/dev", "/proc", "/run", "/sys"]
            .iter()
            .any(|p| options.home_target.starts_with(p))
    {
        return invalid("private HOME target shadows sandbox infrastructure");
    }
    if !absolute(&options.working_directory) {
        return invalid("working directory must be a clean absolute path");
    }
    if plan
        .grants()
        .iter()
        .chain(plan.tool_grants())
        .chain(plan.credential())
        .map(|grant| grant.target.as_path())
        .chain(plan.masks().iter().map(|mask| mask.target.as_path()))
        .any(|target| target.starts_with("/.overlay"))
    {
        return invalid("mount target shadows the private overlay staging tree");
    }
    for overlay in plan.overlays() {
        for path in [
            &overlay.lower,
            &overlay.upper,
            &overlay.work,
            &overlay.target,
        ] {
            if !absolute(path) {
                return invalid("overlay paths must be clean absolute paths");
            }
            let spelling = path
                .to_str()
                .ok_or_else(|| LaunchError::Invalid("overlay paths must be UTF-8".into()))?;
            if spelling.contains([',', ':', '\\', '\n', '\r']) {
                return invalid("overlay paths contain a mount-option separator");
            }
        }
        if overlay.target.starts_with("/.overlay") {
            return invalid("overlay target shadows the private overlay staging mount");
        }
    }
    for (k, v) in &options.environment {
        if k.is_empty() || k.contains(['=', '\0']) || v.contains('\0') {
            return invalid("invalid environment entry");
        }
    }
    let helper = fs::canonicalize(&options.helper).map_err(ioerr("resolve helper binary"))?;
    fs::create_dir_all(&options.runtime_dir).map_err(ioerr("create runtime directory"))?;
    let runtime =
        fs::canonicalize(&options.runtime_dir).map_err(ioerr("resolve runtime directory"))?;
    if plan
        .grants()
        .iter()
        .chain(plan.tool_grants())
        .map(|m| m.source.as_path())
        .chain(plan.private_home())
        .chain(plan.overlays().iter().map(|o| o.lower.as_path()))
        .chain(plan.credential().map(|m| m.source.as_path()))
        .any(|source| runtime.starts_with(source))
    {
        return invalid("runtime directory must be outside every mounted source");
    }
    let root = tempfile::Builder::new()
        .prefix("sandbox-")
        .tempdir_in(&runtime)
        .map_err(ioerr("create private mount root"))?;
    fs::set_permissions(
        root.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .map_err(ioerr("protect runtime directory"))?;
    let (parent_control, child_control) =
        UnixStream::pair().map_err(ioerr("create private control socket"))?;
    parent_control
        .set_read_timeout(Some(options.setup_timeout))
        .map_err(ioerr("set setup timeout"))?;
    let mut files = Vec::new();
    let mut mounts = Vec::new();
    let mut links = Vec::new();
    for system in ["/usr", "/etc", "/bin", "/sbin", "/lib", "/lib32", "/lib64"] {
        let path = Path::new(system);
        match fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => {
                let link = fs::read_link(path).map_err(ioerr("read system symlink"))?;
                let destination =
                    fs::canonicalize(path).map_err(ioerr("resolve system symlink"))?;
                if !destination.starts_with("/usr") {
                    return invalid("system symlink escapes /usr");
                }
                links.push((path.into(), link));
            }
            Ok(_) => pin(path, path, true, Kind::Plain, &mut files, &mut mounts)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(ioerr("inspect system directory")(e)),
        }
    }
    // /etc may contain a resolver symlink into a transient /run tree.
    // Preserve just its resolved regular file, never the surrounding host tree.
    if let Some(resolver) = external_resolver_target(Path::new("/etc/resolv.conf"))? {
        pin(
            &resolver,
            &resolver,
            true,
            Kind::Plain,
            &mut files,
            &mut mounts,
        )?;
    }
    for device in [
        "/dev/null",
        "/dev/zero",
        "/dev/full",
        "/dev/random",
        "/dev/urandom",
        "/dev/tty",
    ] {
        pin(
            Path::new(device),
            Path::new(device),
            false,
            Kind::Device,
            &mut files,
            &mut mounts,
        )?;
    }
    let private_home = plan
        .private_home()
        .ok_or_else(|| LaunchError::Invalid("missing private home".into()))?;
    pin(
        private_home,
        &options.home_target,
        false,
        Kind::Plain,
        &mut files,
        &mut mounts,
    )?;
    for grant in plan
        .tool_grants()
        .iter()
        .chain(plan.grants())
        .chain(plan.credential())
    {
        pin(
            &grant.source,
            &grant.target,
            grant.access == Access::ReadOnly,
            Kind::Plain,
            &mut files,
            &mut mounts,
        )?;
    }
    // Fail closed: a sandbox that may use the network never starts without
    // its proxy, and a sandbox without one has no route at all.
    let proxy = match plan.network() {
        Network::Proxied(policy) => Some(
            egress::Proxy::start(policy, &runtime, options.egress_log.clone())
                .map_err(ioerr("start egress proxy"))?,
        ),
        Network::Loopback => None,
        Network::Host => return invalid("a sandboxed plan cannot use the host network"),
    };
    for (source, target) in proxy.iter().flat_map(|proxy| &proxy.sockets) {
        pin(source, target, true, Kind::Socket, &mut files, &mut mounts)?;
    }
    // Overlay layers are staged after every tool and read-only mount so an
    // overlay can carve a writable cache out of a read-only tool bind. The
    // upper/work sources live in workload state, outside its mounted HOME.
    let mut overlays = Vec::new();
    for (index, overlay) in plan.overlays().iter().enumerate() {
        let staging = PathBuf::from(format!("/.overlay/{index}"));
        let lower_stage = staging.join("lower");
        let layer_stage = staging.join("writable");
        let layer_parent = overlay.upper.parent().ok_or_else(|| {
            LaunchError::Invalid("overlay upper has no workload state parent".into())
        })?;
        if overlay.work.parent() != Some(layer_parent) {
            return invalid("overlay upper and work must share one parent directory");
        }
        let upper_stage = layer_stage.join(overlay.upper.file_name().unwrap());
        let work_stage = layer_stage.join(overlay.work.file_name().unwrap());
        pin(
            &overlay.lower,
            &lower_stage,
            true,
            Kind::Plain,
            &mut files,
            &mut mounts,
        )?;
        let upper = open_pinned(&overlay.upper)?;
        let upper_meta = upper.metadata().map_err(ioerr("pin overlay upper"))?;
        let work = open_pinned(&overlay.work)?;
        let work_meta = work.metadata().map_err(ioerr("pin overlay work"))?;
        if !upper_meta.is_dir() || !work_meta.is_dir() {
            return invalid("overlay upper and work must be directories");
        }
        if upper_meta.dev() != work_meta.dev() {
            return invalid("overlay upper and work must share a filesystem");
        }
        let protect = |path: &Path, meta: &std::fs::Metadata| -> Result<()> {
            let directory = open_directory(path)?;
            let current = directory
                .metadata()
                .map_err(ioerr("recheck overlay layer"))?;
            if current.dev() != meta.dev() || current.ino() != meta.ino() {
                return invalid("overlay layer changed while being protected");
            }
            syscall_ok(
                unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } as libc::c_long,
                "protect pinned overlay layer directory",
            )?;
            Ok(())
        };
        protect(&overlay.upper, &upper_meta)?;
        protect(&overlay.work, &work_meta)?;
        let parent_meta = open_pinned(layer_parent)?
            .metadata()
            .map_err(ioerr("pin overlay state parent"))?;
        protect(layer_parent, &parent_meta)?;
        pin(
            layer_parent,
            &layer_stage,
            false,
            Kind::Plain,
            &mut files,
            &mut mounts,
        )?;
        drop(upper);
        drop(work);
        overlays.push(PinnedOverlay {
            lower_target: lower_stage,
            upper: upper_stage,
            work: work_stage,
            target: overlay.target.clone(),
            upper_device: upper_meta.dev(),
            upper_inode: upper_meta.ino(),
            work_device: work_meta.dev(),
            work_inode: work_meta.ino(),
        });
    }
    for mask in plan.masks() {
        let file = open_pinned(&mask.source)?;
        let m = file.metadata().map_err(ioerr("stat mask"))?;
        if !m.is_file() || m.len() != 0 {
            return invalid("mask source changed; expected an ordinary empty file");
        }
        drop(file);
        pin(
            &mask.source,
            &mask.target,
            true,
            Kind::Mask,
            &mut files,
            &mut mounts,
        )?;
    }
    let mut environment = options.environment.clone();
    environment.insert(
        "HOME".into(),
        options.home_target.to_string_lossy().into_owned(),
    );
    environment.entry("PATH".into()).or_insert_with(|| {
        format!(
            "{}:/usr/local/bin:/usr/bin:/bin",
            plan.tool_grants()
                .iter()
                .flat_map(|m| {
                    let mut paths = vec![m.target.to_string_lossy().into_owned()];
                    if m.source.join("bin").is_dir() {
                        paths.push(m.target.join("bin").to_string_lossy().into_owned());
                    }
                    paths
                })
                .collect::<Vec<_>>()
                .join(":")
        )
    });
    if matches!(
        plan.workload(),
        Workload::ChatSleeve | Workload::TerminalSleeve | Workload::NestedRuntimeTestSleeve
    ) && plan
        .overlays()
        .iter()
        .any(|overlay| overlay.target == options.home_target.join(".pub-cache"))
    {
        let bin = options
            .home_target
            .join(".pub-cache/bin")
            .to_string_lossy()
            .into_owned();
        if let Some(path) = environment.get_mut("PATH") {
            if !path.split(':').any(|part| part == bin) {
                path.insert_str(0, &format!("{bin}:"));
            }
        }
    }
    if matches!(
        plan.workload(),
        Workload::ChatSleeve | Workload::TerminalSleeve | Workload::NestedRuntimeTestSleeve
    ) {
        // The seeded ~/.profile restores this after /etc/profile resets PATH
        // for the login shells used by coding-agent harnesses.
        if let Some(path) = environment.get("PATH").cloned() {
            environment.insert("PROTECTORATE_SLEEVE_PATH".into(), path);
        }
    }
    environment.entry("LANG".into()).or_insert("C.UTF-8".into());
    if let Some(proxy) = &proxy {
        let port = proxy.relays[0].0;
        let url = format!("http://127.0.0.1:{port}");
        for key in [
            "HTTPS_PROXY",
            "https_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            environment.insert(key.into(), url.clone());
        }
        for key in ["NO_PROXY", "no_proxy"] {
            environment.insert(key.into(), "localhost,127.0.0.1,::1".into());
        }
        // Node's built-in fetch and http clients ignore the variables above
        // unless this documented switch is set (Node 22.21 and 24.5 onward).
        environment.insert("NODE_USE_ENV_PROXY".into(), "1".into());
    }
    let unit = format!(
        "protectorate-rust-{}-{}.scope",
        std::process::id(),
        root.path().file_name().unwrap().to_string_lossy()
    );
    let limits = plan.limits();
    let tmp_bytes = tmpfs_bytes(limits);
    let mut direct_scope = ScopeGuard(match delegated_parent_root() {
        Some(root) => Some(create_direct_scope(&root, &unit, limits)?),
        None => None,
    });
    let payload = HelperPlan {
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        root: root.path().into(),
        home_target: options.home_target.clone(),
        cwd: options.working_directory.clone(),
        command: plan.command().to_vec(),
        environment,
        mounts,
        overlays,
        system_links: links,
        nested_unit: (plan.cgroup() == Some(Cgroup::DelegatedNested)).then(|| unit.clone()),
        direct_scope: direct_scope.0.clone(),
        tmp_bytes,
        relays: proxy
            .as_ref()
            .map(|proxy| proxy.relays.clone())
            .unwrap_or_default(),
    };
    let raw = serde_json::to_vec(&payload)
        .map_err(|e| LaunchError::Invalid(format!("cannot encode helper plan: {e}")))?;
    if raw.len() > MAX_PLAN_BYTES {
        return invalid("helper plan is too large");
    }
    let mut command = if direct_scope.0.is_some() {
        Command::new(&helper)
    } else {
        Command::new("systemd-run")
    };
    if direct_scope.0.is_none() {
        command
            .args(["--user", "--scope", "--quiet", "--collect"])
            .arg(format!("--unit={unit}"))
            .args(["-p", &format!("TasksMax={}", limits.tasks_max)]);
        if limits.cpu_percent > 0 {
            command.args(["-p", &format!("CPUQuota={}%", limits.cpu_percent)]);
        }
        if limits.memory_bytes > 0 {
            command.args([
                "-p",
                &format!("MemoryMax={}", limits.memory_bytes),
                "-p",
                "MemorySwapMax=0",
            ]);
        }
        if plan.cgroup() == Some(Cgroup::DelegatedNested) {
            command.args(["-p", "Delegate=yes"]);
        }
        command.arg("--").arg(&helper);
    }
    command
        .arg(HELPER_FLAG)
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    for key in ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    match options.stdio {
        StdioMode::Capture => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }
        StdioMode::Duplex => {
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }
        StdioMode::Inherit => {
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
    }
    let control_fd = child_control.as_raw_fd();
    let supervisor_pid = unsafe { libc::getpid() };
    // SAFETY: this pre_exec closure performs only async-signal-safe Linux syscalls;
    // it allocates nothing, and does not enter namespaces in the async parent.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != supervisor_pid {
                return Err(io::Error::other("sandbox supervisor died before exec"));
            }
            if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::dup2(control_fd, CONTROL_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().map_err(ioerr(
        "start bounded sandbox helper (systemd user manager or delegated parent required)",
    ))?;
    drop(child_control);
    let mut handle = ChildHandle {
        child: Some(child),
        unit,
        direct_scope: direct_scope.0.take(),
        host_direct: false,
        alive: Arc::new(AtomicBool::new(true)),
        kill_gate: Arc::new(Mutex::new(())),
        root: Some(root),
        egress: proxy,
        init_pid: 0,
    };
    let mut control = parent_control;
    let send = (|| -> io::Result<()> {
        control.write_all(&(raw.len() as u32).to_le_bytes())?;
        control.write_all(&raw)
    })();
    if let Err(e) = send {
        let _ = handle.cancel();
        return Err(ioerr("send private helper plan")(e));
    }
    let mut reader = BufReader::new(control);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(ioerr(
        "wait for namespace setup (systemd/userns/mount capability required)",
    ))?;
    drop(files);
    if let Some(pid) = line.strip_prefix("READY ") {
        handle.init_pid = pid
            .trim()
            .parse()
            .map_err(|_| LaunchError::Setup("invalid helper ready message".into()))?;
        // Setup has pivoted into its private mount namespace. The host-side
        // staging directory is empty now and must not rely on this supervisor
        // reaching Drop: tmux and service managers can kill it abruptly.
        if let Err(error) =
            fs::remove_dir(handle.root.as_ref().expect("sandbox has staging").path())
        {
            let _ = handle.cancel();
            return Err(ioerr("remove host sandbox staging directory")(error));
        }
        if let Err(error) = handle.egress.as_ref().map_or(Ok(()), egress::Proxy::unlink) {
            let _ = handle.cancel();
            return Err(ioerr("remove host egress socket names")(error));
        }
        return Ok(handle);
    }
    let _ = handle.cancel();
    let output = handle
        .child
        .take()
        .unwrap()
        .wait_with_output()
        .map_err(ioerr("collect failed helper"))?;
    Err(LaunchError::Setup(format!(
        "{} {}",
        line.trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}
#[repr(C)]
struct MountAttr {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}
fn set_attrs(fd: RawFd, flags: u64, recursive: bool) -> Result<()> {
    let attr = MountAttr {
        attr_set: flags,
        attr_clr: 0,
        propagation: 0,
        userns_fd: 0,
    }; // SAFETY: kernel receives a correctly sized mount_attr and empty relative path.
    syscall_ok(
        unsafe {
            libc::syscall(
                libc::SYS_mount_setattr,
                fd,
                c"".as_ptr(),
                AT_EMPTY_PATH | if recursive { AT_RECURSIVE } else { 0 },
                &attr,
                std::mem::size_of::<MountAttr>(),
            )
        },
        "set detached mount safety flags",
    )?;
    Ok(())
}
fn target(root: &File, path: &Path, directory: bool) -> Result<File> {
    if !absolute(path) || path == Path::new("/") {
        return invalid("invalid mount target");
    }
    let parts: Vec<_> = path
        .components()
        .filter_map(|c| {
            if let Component::Normal(s) = c {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    let mut parent = root.try_clone().map_err(ioerr("duplicate root handle"))?;
    for (i, part) in parts.iter().enumerate() {
        let name = cstr(part)?;
        let last = i + 1 == parts.len();
        let is_dir = !last || directory;
        if is_dir {
            let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) };
            if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                return Err(ioerr("create mount target directory")(
                    io::Error::last_os_error(),
                ));
            }
        } else {
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o644,
                )
            };
            if fd >= 0 {
                unsafe {
                    libc::close(fd);
                }
            } else if io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                return Err(ioerr("create mount target file")(io::Error::last_os_error()));
            }
        }
        let flags = libc::O_PATH
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if is_dir { libc::O_DIRECTORY } else { 0 };
        parent = owned_fd(
            unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) } as libc::c_long,
            "open mount target without symlinks",
        )?;
        if !is_dir
            && parent
                .metadata()
                .map_err(ioerr("stat target"))?
                .file_type()
                .is_symlink()
        {
            return invalid("mount target is a symlink");
        }
    }
    Ok(parent)
}
fn attach(
    source: RawFd,
    target: RawFd,
    read_only: bool,
    device: bool,
    directory: bool,
) -> Result<()> {
    let tree = owned_fd(
        unsafe {
            libc::syscall(
                libc::SYS_open_tree,
                source,
                c"".as_ptr(),
                OPEN_TREE_CLONE
                    | libc::O_CLOEXEC as u32
                    | AT_EMPTY_PATH
                    | if directory { AT_RECURSIVE } else { 0 },
            )
        },
        "clone pinned mount source",
    )?;
    set_attrs(
        tree.as_raw_fd(),
        MOUNT_ATTR_NOSUID
            | if device { 0 } else { MOUNT_ATTR_NODEV }
            | if read_only { MOUNT_ATTR_RDONLY } else { 0 },
        true,
    )?;
    syscall_ok(
        unsafe {
            libc::syscall(
                libc::SYS_move_mount,
                tree.as_raw_fd(),
                c"".as_ptr(),
                target,
                c"".as_ptr(),
                MOVE_MOUNT_F_EMPTY_PATH | MOVE_MOUNT_T_EMPTY_PATH,
            )
        },
        "attach pinned mount",
    )?;
    Ok(())
}
fn mount_fs(kind: &str, destination: &Path, flags: libc::c_ulong, data: &str) -> Result<()> {
    let kind = cstr(kind)?;
    let destination = cstr(destination)?;
    let data = cstr(data)?;
    syscall_ok(
        unsafe {
            libc::mount(
                kind.as_ptr(),
                destination.as_ptr(),
                kind.as_ptr(),
                flags,
                data.as_ptr().cast(),
            )
        } as libc::c_long,
        "mount private filesystem",
    )?;
    Ok(())
}
/// Mount each planned overlay from transient mounts of its read-only lower
/// and private upper/work state. The kernel overlay retains the layer mounts
/// after their staging paths are detached and removed before exec.
fn mount_overlays(plan: &HelperPlan, root: &File) -> Result<()> {
    if plan.overlays.is_empty() {
        return Ok(());
    }
    let staging_root = plan.root.join(".overlay");
    for overlay in &plan.overlays {
        let lower = plan
            .root
            .join(overlay.lower_target.strip_prefix("/").unwrap());
        let upper = plan.root.join(overlay.upper.strip_prefix("/").unwrap());
        let work = plan.root.join(overlay.work.strip_prefix("/").unwrap());
        for (path, device, inode) in [
            (&upper, overlay.upper_device, overlay.upper_inode),
            (&work, overlay.work_device, overlay.work_inode),
        ] {
            let file = open_pinned(path)?;
            let meta = file
                .metadata()
                .map_err(ioerr("verify namespace-rebased overlay layer"))?;
            if !meta.is_dir() || meta.dev() != device || meta.ino() != inode {
                return invalid(
                    "overlay upper or work was replaced between planning and namespace setup",
                );
            }
        }
        let destination = target(root, &overlay.target, true)?;
        drop(destination);
        let data = format!(
            "lowerdir={},upperdir={},workdir={}",
            lower.display(),
            upper.display(),
            work.display()
        );
        if data.len() > 4096 {
            return invalid("overlay mount options are too long");
        }
        mount_fs(
            "overlay",
            &plan.root.join(overlay.target.strip_prefix("/").unwrap()),
            libc::MS_NOSUID | libc::MS_NODEV,
            &data,
        )
        .map_err(|e| {
            LaunchError::Setup(format!(
                "mount cache overlay at {}: {e} (host overlayfs may be unusable in a user namespace)",
                overlay.target.display()
            ))
        })?;
    }
    for overlay in &plan.overlays {
        let writable = overlay
            .upper
            .parent()
            .ok_or_else(|| LaunchError::Invalid("staged overlay upper has no parent".into()))?;
        for staged in [&overlay.lower_target, writable] {
            let path = plan.root.join(staged.strip_prefix("/").unwrap());
            syscall_ok(
                unsafe { libc::umount2(cstr(&path)?.as_ptr(), libc::MNT_DETACH) } as libc::c_long,
                "detach private overlay staging mount",
            )?;
        }
    }
    fs::remove_dir_all(&staging_root).map_err(ioerr("remove overlay staging tree"))?;
    if staging_root.exists() {
        return invalid("overlay staging tree remained visible after setup");
    }
    Ok(())
}
fn parent_death() -> Result<()> {
    let before = unsafe { libc::getppid() };
    syscall_ok(
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } as libc::c_long,
        "arm parent-death cleanup",
    )?;
    if unsafe { libc::getppid() } != before {
        return Err(LaunchError::Setup("parent died during setup".into()));
    }
    Ok(())
}
fn write_cgroup(path: &Path, value: &str) -> Result<()> {
    fs::write(path, value).map_err(ioerr(format!("write cgroup control {}", path.display())))
}
fn systemd_scope_path(unit: &str) -> Result<PathBuf> {
    let membership =
        fs::read_to_string("/proc/self/cgroup").map_err(ioerr("read systemd scope membership"))?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| LaunchError::Unsupported("cgroup v2 membership is unavailable".into()))?;
    let relative = Path::new(relative);
    if !absolute(relative) || relative.file_name() != Some(OsStr::new(unit)) {
        return invalid("helper is not inside its delegated systemd scope");
    }
    Ok(Path::new("/sys/fs/cgroup").join(relative.strip_prefix("/").unwrap()))
}
fn setup_nested_cgroup(scope: &Path) -> Result<NestedCgroup> {
    let controllers = fs::read_to_string(scope.join("cgroup.controllers"))
        .map_err(ioerr("read delegated systemd controllers"))?;
    for required in ["cpu", "memory", "pids"] {
        if !controllers.split_whitespace().any(|part| part == required) {
            return Err(LaunchError::Unsupported(format!(
                "delegated systemd scope lacks {required} controller"
            )));
        }
    }
    write_cgroup(&scope.join("cgroup.max.descendants"), "256\n")?;
    write_cgroup(&scope.join("cgroup.max.depth"), "16\n")?;
    let delegated = scope.join("delegated");
    fs::create_dir(&delegated).map_err(ioerr("create delegated cgroup subtree"))?;
    write_cgroup(
        &delegated.join("cgroup.procs"),
        &format!("{}\n", std::process::id()),
    )?;
    write_cgroup(
        &scope.join("cgroup.subtree_control"),
        "+cpu +memory +pids\n",
    )?;
    // The cgroup namespace root is fixed at delegated/, before the helper
    // moves into its run/ leaf. The scope's outer resource limits stay above
    // this namespace root and are never mounted into the workload.
    syscall_ok(
        unsafe { libc::unshare(libc::CLONE_NEWCGROUP) } as libc::c_long,
        "create delegated cgroup namespace",
    )?;
    let run = delegated.join("run");
    fs::create_dir(&run).map_err(ioerr("create nested supervisor cgroup"))?;
    write_cgroup(
        &run.join("cgroup.procs"),
        &format!("{}\n", std::process::id()),
    )?;
    write_cgroup(
        &delegated.join("cgroup.subtree_control"),
        "+cpu +memory +pids\n",
    )?;
    let metadata = open_pinned(&delegated)?
        .metadata()
        .map_err(ioerr("stat delegated cgroup root"))?;
    Ok(NestedCgroup {
        path: delegated,
        device_id: metadata.dev(),
        inode: metadata.ino(),
    })
}
fn setup_user_namespace(plan: &HelperPlan) -> Result<Option<NestedCgroup>> {
    syscall_ok(
        unsafe { libc::unshare(libc::CLONE_NEWUSER) } as libc::c_long,
        "create user namespace",
    )?;
    fs::write("/proc/self/setgroups", "deny").map_err(ioerr("disable supplementary groups"))?;
    fs::write("/proc/self/uid_map", format!("0 {} 1\n", plan.uid))
        .map_err(ioerr("map user identity"))?;
    fs::write("/proc/self/gid_map", format!("0 {} 1\n", plan.gid))
        .map_err(ioerr("map group identity"))?;
    parent_death()?;
    let nested = if let Some(unit) = plan.nested_unit.as_deref() {
        let scope = match &plan.direct_scope {
            Some(scope) => scope.clone(),
            None => systemd_scope_path(unit)?,
        };
        Some(setup_nested_cgroup(&scope)?)
    } else {
        if plan.direct_scope.is_some() {
            syscall_ok(
                unsafe { libc::unshare(libc::CLONE_NEWCGROUP) } as libc::c_long,
                "create child cgroup namespace",
            )?;
        }
        None
    };
    syscall_ok(
        unsafe {
            libc::unshare(
                libc::CLONE_NEWNS
                    | libc::CLONE_NEWPID
                    | libc::CLONE_NEWIPC
                    | libc::CLONE_NEWUTS
                    | libc::CLONE_NEWNET,
            )
        } as libc::c_long,
        "create mount/PID/IPC/UTS/network namespaces",
    )?;
    egress::loopback_up().map_err(ioerr("bring up namespace loopback"))?;
    syscall_ok(
        unsafe {
            libc::mount(
                std::ptr::null(),
                c"/".as_ptr(),
                std::ptr::null(),
                libc::MS_REC | libc::MS_PRIVATE,
                std::ptr::null(),
            )
        } as libc::c_long,
        "make mount propagation private",
    )?;
    Ok(nested)
}
/// The helper cannot create threads once it has unshared a PID namespace, so
/// the relay is a second process inside the workload's PID namespace. It has
/// no capabilities and cannot be ptraced; the namespace ends it with the
/// workload.
fn spawn_relay(relays: Vec<(std::net::TcpListener, PathBuf)>) -> Result<()> {
    let pid = syscall_ok(unsafe { libc::fork() } as libc::c_long, "fork egress relay")?;
    if pid == 0 {
        unsafe { libc::close(CONTROL_FD) };
        let confined = parent_death().is_ok()
            && drop_privileges(false).is_ok()
            && unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == 0;
        if confined {
            egress::serve_relays(relays);
        }
        unsafe { libc::_exit(125) };
    }
    Ok(())
}
fn drop_privileges(nested: bool) -> Result<()> {
    // Linux requires CAP_SETFCAP in the parent user namespace to write a
    // nested uid_map that maps UID 0. It grants no host capability outside
    // this already unprivileged user namespace, but must survive exec for the
    // nested launcher. Every other capability is dropped.
    const CAP_SETFCAP: u32 = 31;
    for capability in 0..64 {
        if nested && capability == CAP_SETFCAP {
            continue;
        }
        let result = unsafe { libc::prctl(libc::PR_CAPBSET_DROP, capability, 0, 0, 0) };
        if result < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
            return Err(ioerr("drop capability bounding set")(
                io::Error::last_os_error(),
            ));
        }
    }
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let header = Header {
        version: 0x20080522,
        pid: 0,
    };
    let retained = if nested { 1 << CAP_SETFCAP } else { 0 };
    let data = [
        Data {
            effective: retained,
            permitted: retained,
            inheritable: 0,
        },
        Data {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
    ];
    syscall_ok(
        unsafe { libc::syscall(libc::SYS_capset, &header, data.as_ptr()) },
        "drop effective capabilities",
    )?;
    syscall_ok(
        unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } as libc::c_long,
        "set no-new-privileges",
    )?;
    Ok(())
}
fn setup_and_exec(
    plan: &HelperPlan,
    nested: Option<&NestedCgroup>,
    error_fd: RawFd,
    parent_pidfd: RawFd,
) -> Result<()> {
    parent_death()?;
    let mut pollfd = libc::pollfd {
        fd: parent_pidfd,
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut pollfd, 1, 0) } > 0 {
        return Err(LaunchError::Setup("helper parent already died".into()));
    }
    // The parent keeps original source descriptors open until setup finishes.
    // Reopen each path inside this mount namespace and compare dev/inode;
    // systemd-run may remap other inherited descriptors, and an open_tree
    // clone of an FD from the old mount namespace fails on this host.
    let mut sources = Vec::new();
    for mount in &plan.mounts {
        let source = open_pinned(&mount.source)?;
        let metadata = source
            .metadata()
            .map_err(ioerr("verify namespace-rebased source"))?;
        if metadata.dev() != mount.device_id || metadata.ino() != mount.inode {
            return invalid("mount source was replaced between planning and namespace setup");
        }
        sources.push(source);
    }
    mount_fs(
        "tmpfs",
        &plan.root,
        libc::MS_NOSUID | libc::MS_NODEV,
        "mode=0755,size=64m",
    )?;
    let root = open_pinned(&plan.root)?;
    for name in ["dev", "proc", "tmp", "var", "run", ".old_root"] {
        let _ = target(&root, &Path::new("/").join(name), true)?;
    }
    mount_fs(
        "tmpfs",
        &plan.root.join("dev"),
        libc::MS_NOSUID,
        "mode=0755,size=4m",
    )?;
    let tmp_options = format!("mode=1777,size={}", plan.tmp_bytes);
    mount_fs(
        "tmpfs",
        &plan.root.join("tmp"),
        libc::MS_NOSUID | libc::MS_NODEV,
        &tmp_options,
    )?;
    let tmp_source = open_pinned(&plan.root.join("tmp"))?;
    let var_tmp = target(&root, Path::new("/var/tmp"), true)?;
    attach(
        tmp_source.as_raw_fd(),
        var_tmp.as_raw_fd(),
        false,
        false,
        true,
    )
    .map_err(|e| LaunchError::Setup(format!("share /tmp with /var/tmp: {e}")))?;
    for (mount, source) in plan.mounts.iter().zip(&sources) {
        if mount.kind == Kind::Mask {
            continue;
        }
        let destination = target(&root, &mount.target, mount.directory)?;
        attach(
            source.as_raw_fd(),
            destination.as_raw_fd(),
            mount.read_only,
            mount.kind == Kind::Device,
            mount.directory,
        )
        .map_err(|e| LaunchError::Setup(format!("mount {}: {e}", mount.target.display())))?;
    }
    if let Some(nested) = nested {
        let source = open_pinned(&nested.path)?;
        let metadata = source
            .metadata()
            .map_err(ioerr("verify delegated cgroup"))?;
        if metadata.dev() != nested.device_id || metadata.ino() != nested.inode {
            return invalid("delegated cgroup changed during namespace setup");
        }
        let destination = target(&root, Path::new("/run/protectorate/cgroup"), true)?;
        attach(
            source.as_raw_fd(),
            destination.as_raw_fd(),
            false,
            false,
            true,
        )
        .map_err(|e| LaunchError::Setup(format!("mount delegated cgroup root: {e}")))?;
    }
    mount_overlays(plan, &root)
        .map_err(|e| LaunchError::Setup(format!("cache overlay setup: {e}")))?;
    for (mount, source) in plan.mounts.iter().zip(&sources) {
        if mount.kind != Kind::Mask {
            continue;
        }
        let destination = target(&root, &mount.target, mount.directory)?;
        attach(
            source.as_raw_fd(),
            destination.as_raw_fd(),
            mount.read_only,
            false,
            mount.directory,
        )
        .map_err(|e| LaunchError::Setup(format!("mount {}: {e}", mount.target.display())))?;
    }
    for (path, link) in &plan.system_links {
        let destination = plan.root.join(path.strip_prefix("/").unwrap());
        std::os::unix::fs::symlink(link, destination).map_err(ioerr("create system symlink"))?;
    }
    for name in ["dev/pts", "dev/shm"] {
        let _ = target(&root, &Path::new("/").join(name), true)?;
    }
    mount_fs(
        "devpts",
        &plan.root.join("dev/pts"),
        libc::MS_NOSUID | libc::MS_NOEXEC,
        "newinstance,ptmxmode=0666,mode=0620",
    )?;
    mount_fs(
        "tmpfs",
        &plan.root.join("dev/shm"),
        libc::MS_NOSUID | libc::MS_NODEV,
        "mode=1777,size=64m",
    )?;
    for (link, to) in [
        ("dev/ptmx", "pts/ptmx"),
        ("dev/fd", "/proc/self/fd"),
        ("dev/stdin", "/proc/self/fd/0"),
        ("dev/stdout", "/proc/self/fd/1"),
        ("dev/stderr", "/proc/self/fd/2"),
    ] {
        std::os::unix::fs::symlink(to, plan.root.join(link))
            .map_err(ioerr("create private device link"))?;
    }
    mount_fs(
        "proc",
        &plan.root.join("proc"),
        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
        "",
    )?;
    syscall_ok(
        unsafe { libc::sethostname(c"protectorate".as_ptr(), 12) } as libc::c_long,
        "set private hostname",
    )?;
    let path = cstr(&plan.root)?;
    syscall_ok(
        unsafe { libc::chdir(path.as_ptr()) } as libc::c_long,
        "enter new root",
    )?;
    syscall_ok(
        unsafe { libc::syscall(libc::SYS_pivot_root, c".".as_ptr(), c".old_root".as_ptr()) },
        "pivot to fresh root",
    )?;
    syscall_ok(
        unsafe { libc::chdir(c"/".as_ptr()) } as libc::c_long,
        "leave detached old root",
    )?;
    syscall_ok(
        unsafe { libc::umount2(c"/.old_root".as_ptr(), libc::MNT_DETACH) } as libc::c_long,
        "detach host root",
    )?;
    fs::remove_dir("/.old_root").map_err(ioerr("remove old-root mountpoint"))?;
    set_attrs(
        root.as_raw_fd(),
        MOUNT_ATTR_RDONLY | MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV,
        false,
    )?;
    let cwd = cstr(&plan.cwd)?;
    syscall_ok(
        unsafe { libc::chdir(cwd.as_ptr()) } as libc::c_long,
        "enter sandbox working directory",
    )?;
    drop_privileges(plan.nested_unit.is_some())?;
    let executable = cstr(&plan.command[0])?;
    let args: Vec<CString> = plan.command.iter().map(cstr).collect::<Result<_>>()?;
    let mut argv: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());
    let env: Vec<CString> = plan
        .environment
        .iter()
        .map(|(k, v)| cstr(format!("{k}={v}")))
        .collect::<Result<_>>()?;
    let mut environ: Vec<_> = env.iter().map(|s| s.as_ptr()).collect();
    environ.push(std::ptr::null());
    if error_fd > 3 {
        syscall_ok(
            unsafe { libc::syscall(libc::SYS_close_range, 3u32, (error_fd - 1) as u32, 0u32) },
            "close inherited host descriptors",
        )?;
    }
    syscall_ok(
        unsafe { libc::syscall(libc::SYS_close_range, (error_fd + 1) as u32, u32::MAX, 0u32) },
        "close inherited host descriptors",
    )?;
    unsafe {
        libc::execve(executable.as_ptr(), argv.as_ptr(), environ.as_ptr());
    }
    Err(ioerr("exec isolated workload")(io::Error::last_os_error()))
}
fn external_resolver_target(path: &Path) -> Result<Option<PathBuf>> {
    let resolved = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ioerr("resolve DNS configuration")(error)),
    };
    if resolved.starts_with("/etc") || resolved.starts_with("/usr") {
        return Ok(None);
    }
    let metadata = fs::metadata(&resolved).map_err(ioerr("inspect DNS configuration"))?;
    if !metadata.is_file() {
        return invalid("resolved DNS configuration is not a regular file");
    }
    Ok(Some(resolved))
}

/// Entrypoint for the dedicated helper binary, not a public argv plan loader.
/// The only plan input is an inherited Unix socket. The parent holds source
/// descriptors while this helper reopens and verifies their dev/inode identity.
pub fn helper_main() -> Result<i32> {
    if std::env::args().nth(1).as_deref() != Some(HELPER_FLAG) {
        return invalid("helper requires its private launcher protocol");
    }
    parent_death()?;
    let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
    let mut peer_len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    syscall_ok(
        unsafe {
            libc::getsockopt(
                CONTROL_FD,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut peer as *mut libc::ucred).cast(),
                &mut peer_len,
            )
        } as libc::c_long,
        "authenticate inherited control socket",
    )?;
    if peer.uid != unsafe { libc::getuid() } {
        return invalid("control socket has a different owner");
    }
    let mut control = unsafe { UnixStream::from_raw_fd(CONTROL_FD) };
    let result = (|| {
        let mut size = [0u8; 4];
        control
            .read_exact(&mut size)
            .map_err(ioerr("read private helper plan"))?;
        let size = u32::from_le_bytes(size) as usize;
        if size > MAX_PLAN_BYTES {
            return invalid("private plan exceeds limit");
        }
        let mut bytes = vec![0; size];
        control
            .read_exact(&mut bytes)
            .map_err(ioerr("read private helper plan"))?;
        let plan: HelperPlan = serde_json::from_slice(&bytes)
            .map_err(|_| LaunchError::Invalid("invalid private plan".into()))?;
        if plan.uid != peer.uid || plan.gid != unsafe { libc::getgid() } {
            return invalid("plan identity does not match caller");
        }
        if let Some(scope) = &plan.direct_scope {
            let parent = delegated_parent_root().ok_or_else(|| {
                LaunchError::Unsupported("nested delegated parent disappeared".into())
            })?;
            if scope.parent() != Some(parent.as_path())
                || !scope
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| {
                        name.starts_with("protectorate-rust-") && name.ends_with(".scope")
                    })
            {
                return invalid("direct scope is outside delegated parent");
            }
            write_cgroup(
                &scope.join("cgroup.procs"),
                &format!("{}\n", std::process::id()),
            )?;
        }
        let nested = setup_user_namespace(&plan)?;
        let relays =
            egress::bind_relays(&plan.relays).map_err(ioerr("bind namespace egress relay"))?;
        let parent_pidfd = owned_fd(
            unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) },
            "pin helper parent lifetime",
        )?;
        let mut pipe = [0; 2];
        syscall_ok(
            unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) } as libc::c_long,
            "create exec-error pipe",
        )?;
        let pid = syscall_ok(
            unsafe { libc::fork() } as libc::c_long,
            "fork namespace init",
        )? as i32;
        if pid == 0 {
            unsafe {
                libc::close(pipe[0]);
            }
            if let Err(error) =
                setup_and_exec(&plan, nested.as_ref(), pipe[1], parent_pidfd.as_raw_fd())
            {
                let message = error.to_string();
                unsafe {
                    libc::write(pipe[1], message.as_ptr().cast(), message.len());
                    libc::_exit(125);
                }
            }
            unsafe {
                libc::_exit(125);
            }
        }
        unsafe {
            libc::close(pipe[1]);
        }
        let mut errors = unsafe { File::from_raw_fd(pipe[0]) };
        let mut message = String::new();
        errors
            .read_to_string(&mut message)
            .map_err(ioerr("read workload setup result"))?;
        if message.is_empty() {
            if !relays.is_empty() {
                // The workload is already running; without its relay it must
                // not keep running either.
                if let Err(error) = spawn_relay(relays) {
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                    return Err(error);
                }
            }
            writeln!(control, "READY {pid}").map_err(ioerr("report namespace readiness"))?;
        } else {
            writeln!(control, "ERROR {}", message.replace('\n', " "))
                .map_err(ioerr("report namespace failure"))?;
        }
        let mut status = 0;
        // Reap the relay too: the init's PID namespace cannot finish while
        // the relay is an unreaped zombie of this helper.
        loop {
            let waited = unsafe { libc::waitpid(-1, &mut status, 0) };
            if waited == pid {
                break;
            }
            if waited > 0 {
                continue;
            }
            if io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                return Err(ioerr("reap namespace init")(io::Error::last_os_error()));
            }
        }
        if libc::WIFEXITED(status) {
            Ok(libc::WEXITSTATUS(status))
        } else if libc::WIFSIGNALED(status) {
            Ok(128 + libc::WTERMSIG(status))
        } else {
            Ok(125)
        }
    })();
    if let Err(error) = &result {
        let _ = writeln!(control, "ERROR {}", error.to_string().replace('\n', " "));
    }
    result
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn resolver_symlink_preserves_only_its_regular_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("resolved.conf");
        fs::write(&target, "nameserver 192.0.2.1\n").unwrap();
        let link = dir.path().join("resolv.conf");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(external_resolver_target(&link).unwrap(), Some(target));
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        assert!(external_resolver_target(&link).is_err());
    }
}
