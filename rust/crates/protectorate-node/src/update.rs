//! Verified node binary replacement with a restart trial and rollback marker.
use crate::files::Files;
use protectorate_fsutil as fsutil;
use protectorate_wire::{Code, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TRIAL: Duration = Duration::from_secs(4 * 60);
const VERSION_TIMEOUT: Duration = Duration::from_secs(15);
pub type Restart = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Marker {
    phase: String,
    from: String,
    to: String,
    attempts: u32,
    deadline_ms: u64,
    reason: String,
    #[serde(default)]
    previous_release: String,
    #[serde(default)]
    candidate_release: String,
}

#[cfg(target_os = "linux")]
struct VerifiedBundle {
    components: Vec<(String, &'static str)>,
}

#[cfg(target_os = "linux")]
fn component_name(component: &str) -> String {
    format!("{component}-linux-{}", node_arch())
}

#[cfg(target_os = "linux")]
fn regular_file(path: &Path, max_size: u64) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > max_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected a bounded regular file: {}", path.display()),
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn verify_bundle(
    dir: &Path,
    version: &str,
    expected_size: u64,
    expected_sha: &str,
) -> io::Result<VerifiedBundle> {
    if version.is_empty()
        || version.len() > 160
        || version == "."
        || version == ".."
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid release version",
        ));
    }
    regular_file(&dir.join("VERSION"), 1024)?;
    if fs::read_to_string(dir.join("VERSION"))?.trim() != version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "release VERSION differs",
        ));
    }
    regular_file(&dir.join("SHA256SUMS"), 1 << 20)?;
    let sums = fs::read_to_string(dir.join("SHA256SUMS"))?;
    let components = [
        "protectorate-rs",
        "protectorate-sleeve-runner",
        "protectorate-sleeve-sandbox-helper",
        "protectorate-node-sandbox-helper",
        "protectorate-cli",
        "protectorate-mcpbridge",
    ];
    let mut checked = Vec::with_capacity(components.len());
    for component in components {
        let name = component_name(component);
        regular_file(&dir.join(&name), 512 << 20)?;
        let (size, sha) = digest(&dir.join(&name))?;
        let mut matches = sums.lines().filter_map(|line| {
            let (digest, file) = line.split_once("  ")?;
            (file == name).then_some(digest)
        });
        if matches.next() != Some(sha.as_str()) || matches.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("release checksum differs for {name}"),
            ));
        }
        if component == "protectorate-rs"
            && (size != expected_size || !sha.eq_ignore_ascii_case(expected_sha))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "release binary differs from the requested size or SHA-256",
            ));
        }
        checked.push((name, component));
    }
    Ok(VerifiedBundle {
        components: checked,
    })
}

#[cfg(target_os = "linux")]
fn copy_regular(from: &Path, to: &Path) -> io::Result<()> {
    regular_file(from, 4 << 20)?;
    let mut source = File::open(from)?;
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    io::copy(&mut source, &mut target)?;
    target.set_permissions(fs::Permissions::from_mode(0o644))?;
    target.sync_all()
}

#[cfg(target_os = "linux")]
fn current_pointer(root: &Path) -> io::Result<String> {
    let pointer = fs::read_link(root.join("current"))?;
    if pointer.components().count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsafe node release pointer",
        ));
    }
    let name = pointer
        .to_str()
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsafe node release pointer"))?;
    let release = fs::symlink_metadata(root.join(name))?;
    if !release.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "node release pointer does not name a real directory",
        ));
    }
    Ok(name.into())
}

#[cfg(target_os = "linux")]
fn installed_layout(exe: &Path) -> io::Result<Option<Installed>> {
    if exe.file_name().and_then(|name| name.to_str()) != Some("protectorate-rs") {
        return Ok(None);
    }
    let Some(release) = exe.parent() else {
        return Ok(None);
    };
    let Some(root) = release.parent() else {
        return Ok(None);
    };
    match current_pointer(root) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let Some(release_name) = release.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let native = root.file_name().and_then(|name| name.to_str()) == Some("node-install")
        && release_name.starts_with("release-");
    let shell = release.join("SHA256SUMS").is_file() && release.join("VERSION").is_file();
    if !native && !shell {
        return Ok(None);
    }
    Ok(Some(Installed {
        root: root.into(),
        #[cfg(target_os = "linux")]
        native,
    }))
}

#[cfg(not(target_os = "linux"))]
fn installed_layout(_exe: &Path) -> io::Result<Option<Installed>> {
    Ok(None)
}

#[derive(Clone)]
struct Installed {
    root: PathBuf,
    #[cfg(target_os = "linux")]
    native: bool,
}

pub struct Updater {
    files: Arc<Files>,
    exe: PathBuf,
    version: String,
    installed: Option<Installed>,
    restart: Restart,
    lock: Mutex<()>,
}

impl Updater {
    pub(crate) fn new(
        files: Arc<Files>,
        exe: PathBuf,
        version: String,
        restart: Restart,
    ) -> io::Result<Self> {
        let exe = fs::canonicalize(exe)?;
        if !exe.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "node executable is not a file",
            ));
        }
        let installed = installed_layout(&exe)?;
        Ok(Self {
            files,
            exe,
            version,
            installed,
            restart,
            lock: Mutex::new(()),
        })
    }

    fn marker_path(&self) -> PathBuf {
        if let Some(installed) = &self.installed {
            return installed.root.join(".node-update.json");
        }
        PathBuf::from(format!("{}.update", self.exe.display()))
    }
    fn previous_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.old", self.exe.display()))
    }
    fn candidate_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.new", self.exe.display()))
    }

    fn load(&self) -> io::Result<Option<Marker>> {
        match fs::read(self.marker_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn save(&self, marker: &Marker) -> io::Result<()> {
        fsutil::write_file(&self.marker_path(), &serde_json::to_vec(marker)?, 0o600)
    }

    /// Called before accepting requests. An unconfirmed second start restores
    /// the last binary. The first trial start arms a deadline watchdog.
    pub fn start(self: &Arc<Self>) -> io::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let Some(mut marker) = self.load()? else {
            return Ok(false);
        };
        #[cfg(target_os = "linux")]
        if let Some(installed) = &self.installed {
            let current = current_pointer(&installed.root)?;
            if marker.phase == "ready"
                && marker.to == self.version
                && current == marker.candidate_release
            {
                marker.phase = "confirmed".into();
                self.save(&marker)?;
                return Ok(false);
            }
            if matches!(marker.phase.as_str(), "trial" | "ready")
                && marker.from == self.version
                && current == marker.previous_release
            {
                marker.phase = "rolledback".into();
                marker.reason = "trial interrupted before confirmation".into();
                self.save(&marker)?;
                return Ok(false);
            }
        }
        if marker.phase != "trial" || marker.to != self.version {
            return Ok(false);
        }
        if marker.attempts != 0 || now_ms() >= marker.deadline_ms {
            self.rollback(&mut marker, "new binary never confirmed")?;
            return Ok(true);
        }
        marker.attempts = 1;
        self.save(&marker)?;
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep_until(
                tokio::time::Instant::now()
                    + Duration::from_millis(marker.deadline_ms.saturating_sub(now_ms())),
            )
            .await;
            if let Some(updater) = weak.upgrade() {
                let _guard = updater.lock.lock().unwrap();
                match updater.load() {
                    Ok(Some(mut current))
                        if current.phase == "trial"
                            && current.to == updater.version
                            && now_ms() >= current.deadline_ms =>
                    {
                        if updater
                            .rollback(&mut current, "hub did not reach the new binary in time")
                            .is_ok()
                        {
                            (updater.restart)();
                        }
                    }
                    _ => {}
                }
            }
        });
        Ok(false)
    }

    pub fn touch(&self, reported_version: &str) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap();
        if reported_version != self.version {
            return Ok(());
        }
        if let Some(mut marker) = self.load()? {
            if marker.phase == "trial" && marker.to == self.version && marker.attempts > 0 {
                marker.phase = if self.installed.is_some() {
                    "ready".into()
                } else {
                    "confirmed".into()
                };
                self.save(&marker)?;
            }
        }
        Ok(())
    }

    pub fn state(&self) -> io::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        Ok(self.load()?.map(|marker| {
            json!({
                "phase": marker.phase,
                "previousVersion": marker.from,
                "version": marker.to,
                "attempts": marker.attempts,
            })
        }))
    }

    fn rollback(&self, marker: &mut Marker, reason: &str) -> io::Result<()> {
        if self.installed.is_none() {
            let previous = self.previous_path();
            if !previous.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "previous node binary is missing",
                ));
            }
            restore_binary(&self.exe, &previous)?;
        }
        marker.phase = "rolledback".into();
        marker.reason = reason.into();
        self.save(marker)
    }

    pub(crate) fn apply(&self, body: &Value) -> Result<Value, RpcError> {
        let _guard = self.lock.lock().unwrap();
        if self
            .load()
            .map_err(internal)?
            .is_some_and(|marker| marker.phase == "trial" || marker.phase == "ready")
        {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "a node binary update is still on trial",
            ));
        }
        let name = body.get("path").and_then(Value::as_str).unwrap_or_default();
        let staged = self.files.resolve(name)?;
        if let Some(installed) = &self.installed {
            return self.apply_installed(installed, &staged, body);
        }
        let expected_size = body
            .get("size")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
            .or_else(|| body.get("size").and_then(Value::as_u64))
            .unwrap_or_default();
        let expected_sha = body
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if expected_sha.len() != 64 || !expected_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(RpcError::invalid(
                "sha256 must be 64 hexadecimal characters",
            ));
        }
        let (size, sha) = digest(&staged)
            .map_err(|e| RpcError::new(Code::NotFound, format!("read staged binary: {e}")))?;
        if size != expected_size || !sha.eq_ignore_ascii_case(expected_sha) {
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "staged binary size or SHA-256 does not match",
            ));
        }
        let candidate = self.candidate_path();
        copy_binary(&staged, &candidate).map_err(internal)?;
        let (copied_size, copied_sha) = digest(&candidate).map_err(internal)?;
        if copied_size != expected_size || !copied_sha.eq_ignore_ascii_case(expected_sha) {
            let _ = fs::remove_file(&candidate);
            return Err(RpcError::new(
                Code::FailedPrecondition,
                "candidate binary changed during staging",
            ));
        }
        let candidate_version = match run_version(&candidate) {
            Ok(version) => version,
            Err(error) => {
                let _ = fs::remove_file(&candidate);
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    format!("new binary does not run: {error}"),
                ));
            }
        };
        let expected_version = body
            .get("expectedVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !expected_version.is_empty() && candidate_version != expected_version {
            let _ = fs::remove_file(&candidate);
            return Err(RpcError::new(
                Code::FailedPrecondition,
                format!("candidate reports {candidate_version}, expected {expected_version}"),
            ));
        }
        let previous = self.previous_path();
        swap_binary(&self.exe, &candidate, &previous).map_err(internal)?;
        let marker = Marker {
            phase: "trial".into(),
            from: self.version.clone(),
            to: candidate_version.clone(),
            attempts: 0,
            deadline_ms: now_ms() + TRIAL.as_millis() as u64,
            reason: String::new(),
            previous_release: String::new(),
            candidate_release: String::new(),
        };
        if let Err(error) = self.save(&marker) {
            let _ = restore_binary(&self.exe, &previous);
            return Err(internal(error));
        }
        let restart = Arc::clone(&self.restart);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            restart()
        });
        Ok(
            json!({"version": candidate_version, "previousVersion":self.version, "restart":"re-exec"}),
        )
    }

    fn apply_installed(
        &self,
        installed: &Installed,
        staged: &Path,
        body: &Value,
    ) -> Result<Value, RpcError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (installed, staged, body);
            Err(RpcError::new(
                Code::FailedPrecondition,
                "installed node updates require Linux",
            ))
        }
        #[cfg(target_os = "linux")]
        {
            if !fs::symlink_metadata(staged)
                .map_err(internal)?
                .file_type()
                .is_dir()
            {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "installed node update requires a complete release bundle",
                ));
            }
            let expected_version = body
                .get("expectedVersion")
                .and_then(Value::as_str)
                .filter(|version| !version.is_empty())
                .ok_or_else(|| RpcError::invalid("installed release version is required"))?;
            let expected_sha = body
                .get("sha256")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let expected_size = body
                .get("size")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u64>().ok())
                .or_else(|| body.get("size").and_then(Value::as_u64))
                .unwrap_or_default();
            let bundle = verify_bundle(staged, expected_version, expected_size, expected_sha)
                .map_err(|error| RpcError::new(Code::FailedPrecondition, error.to_string()))?;
            let previous_release = current_pointer(&installed.root).map_err(internal)?;
            let running_release = self
                .exe
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .ok_or_else(|| RpcError::new(Code::Internal, "running release has no name"))?;
            if previous_release != running_release {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "installed release pointer changed during update",
                ));
            }
            let canonical_available = expected_version != previous_release
                && !installed.root.join(expected_version).exists();
            let target_name = if installed.native {
                format!(
                    "release-{expected_version}-{}-{}",
                    std::process::id(),
                    now_ms()
                )
            } else if canonical_available {
                expected_version.to_owned()
            } else {
                let suffix = format!("+node.{}.{}", std::process::id(), now_ms());
                if expected_version.len() + suffix.len() > 160 {
                    return Err(RpcError::new(
                        Code::FailedPrecondition,
                        "release version is too long for an update attempt",
                    ));
                }
                format!("{expected_version}{suffix}")
            };
            if installed.root.join(&target_name).exists() {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    "installed update attempt already exists",
                ));
            }
            let prepared = installed
                .root
                .join(format!(".node-update-{target_name}-{}", std::process::id()));
            fs::create_dir(&prepared).map_err(internal)?;
            for (name, component) in &bundle.components {
                let destination = prepared.join(name);
                copy_binary(&staged.join(name), &destination).map_err(internal)?;
                fs::hard_link(&destination, prepared.join(component)).map_err(internal)?;
            }
            copy_regular(&staged.join("VERSION"), &prepared.join("VERSION")).map_err(internal)?;
            copy_regular(&staged.join("SHA256SUMS"), &prepared.join("SHA256SUMS"))
                .map_err(internal)?;
            let metadata = staged.join("release.json");
            if metadata.is_file()
                && !fs::symlink_metadata(&metadata)
                    .map_err(internal)?
                    .file_type()
                    .is_symlink()
            {
                copy_regular(&metadata, &prepared.join("release.json")).map_err(internal)?;
            } else {
                fsutil::write_file(
                    &prepared.join("release.json"),
                    &serde_json::to_vec(&json!({"version": expected_version})).map_err(internal)?,
                    0o644,
                )
                .map_err(internal)?;
            }
            verify_bundle(&prepared, expected_version, expected_size, expected_sha).map_err(
                |error| {
                    RpcError::new(
                        Code::FailedPrecondition,
                        format!("copied release differs: {error}"),
                    )
                },
            )?;
            let candidate = prepared.join("protectorate-rs");
            let actual_version = run_version(&candidate).map_err(|error| {
                RpcError::new(
                    Code::FailedPrecondition,
                    format!("candidate does not run: {error}"),
                )
            })?;
            if actual_version != expected_version {
                return Err(RpcError::new(
                    Code::FailedPrecondition,
                    format!("candidate reports {actual_version}, expected {expected_version}"),
                ));
            }
            fs::rename(&prepared, installed.root.join(&target_name)).map_err(internal)?;
            let marker = Marker {
                phase: "trial".into(),
                from: self.version.clone(),
                to: expected_version.into(),
                attempts: 0,
                deadline_ms: now_ms() + TRIAL.as_millis() as u64,
                reason: String::new(),
                previous_release,
                candidate_release: target_name,
            };
            self.save(&marker).map_err(internal)?;
            let restart = Arc::clone(&self.restart);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                restart()
            });
            Ok(
                json!({"version":expected_version,"previousVersion":self.version,"restart":"re-exec"}),
            )
        }
    }
}

fn swap_binary(exe: &Path, candidate: &Path, previous: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Windows permits moving a mapped EXE, but cannot replace its mapped
        // image in place. The separate task guardian never maps this path.
        fsutil::rename(exe, previous)?;
        if let Err(error) = fsutil::rename(candidate, exe) {
            let _ = fsutil::rename(previous, exe);
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    {
        let stage = PathBuf::from(format!(
            "{}.old.stage-{}",
            exe.display(),
            std::process::id()
        ));
        copy_binary(exe, &stage)?;
        fsutil::rename(&stage, previous)?;
        fsutil::rename(candidate, exe)?;
    }
    Ok(())
}

fn restore_binary(exe: &Path, previous: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        let failed = PathBuf::from(format!("{}.failed", exe.display()));
        fsutil::rename(exe, &failed)?;
        if let Err(error) = fsutil::rename(previous, exe) {
            let _ = fsutil::rename(&failed, exe);
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    fsutil::rename(previous, exe)?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
fn internal(error: impl std::fmt::Display) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
fn digest(path: &Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("binary size overflow"))?;
    }
    Ok((size, format!("{:x}", hash.finalize())))
}
fn copy_binary(from: &Path, to: &Path) -> io::Result<()> {
    let mut src = File::open(from)?;
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        dst.set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    io::copy(&mut src, &mut dst)?;
    dst.flush()?;
    dst.sync_all()
}
fn run_version(path: &Path) -> io::Result<String> {
    let mut child = Command::new(path)
        .args(["version", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(io::Error::other(format!("version command exited {status}")));
            }
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("missing version output"))?
                .take(4096)
                .read_to_end(&mut bytes)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            let os = value.get("os").and_then(Value::as_str).unwrap_or_default();
            let arch = value
                .get("arch")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if os != std::env::consts::OS || arch != node_arch() {
                return Err(io::Error::other("binary platform does not match this node"));
            }
            let version = value
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if version.is_empty() {
                return Err(io::Error::other("binary reports no version"));
            }
            return Ok(version.into());
        }
        if std::time::Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "version command timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// Called by the previous known-good process when its replacement exits or
/// fails to confirm. Only an active trial may be restored.
pub fn recover_failed_trial(exe: &Path, reason: &str) -> io::Result<bool> {
    let installed = installed_layout(exe)?;
    let marker_path = installed.as_ref().map_or_else(
        || PathBuf::from(format!("{}.update", exe.display())),
        |layout| layout.root.join(".node-update.json"),
    );
    let data = match fs::read(&marker_path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let mut marker: Marker = serde_json::from_slice(&data)?;
    if marker.phase != "trial" && marker.phase != "ready" {
        return Ok(false);
    }
    if let Some(installed) = installed {
        #[cfg(target_os = "linux")]
        if current_pointer(&installed.root)? == marker.candidate_release {
            switch_pointer(&installed.root, &marker.previous_release)?;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = installed;
    } else {
        let previous = PathBuf::from(format!("{}.old", exe.display()));
        if !previous.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "previous node binary is missing",
            ));
        }
        restore_binary(exe, &previous)?;
    }
    marker.phase = "rolledback".into();
    marker.reason = reason.into();
    fsutil::write_file(&marker_path, &serde_json::to_vec(&marker)?, 0o600)?;
    Ok(true)
}

/// The guardian owns the installed release pointer while the candidate runs.
pub fn guardian_update_state(exe: &Path) -> io::Result<(String, u64)> {
    let marker_path = installed_layout(exe)?.map_or_else(
        || PathBuf::from(format!("{}.update", exe.display())),
        |layout| layout.root.join(".node-update.json"),
    );
    let marker: Marker = serde_json::from_slice(&fs::read(marker_path)?)?;
    Ok((marker.phase, marker.deadline_ms))
}

pub fn guardian_restart_target(exe: &Path) -> io::Result<PathBuf> {
    let Some(installed) = installed_layout(exe)? else {
        return Ok(exe.into());
    };
    let path = installed.root.join(".node-update.json");
    let marker: Marker = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(exe.into()),
        Err(error) => return Err(error),
    };
    let release = if matches!(marker.phase.as_str(), "trial" | "ready" | "confirmed") {
        &marker.candidate_release
    } else {
        &marker.previous_release
    };
    if release.is_empty() || release.contains(['/', '\\', '\n', '\r']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid trial release",
        ));
    }
    let target = installed.root.join(release).join("protectorate-rs");
    if !target.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "trial release binary missing",
        ));
    }
    Ok(target)
}

#[cfg(target_os = "linux")]
fn switch_pointer(root: &Path, release: &str) -> io::Result<()> {
    if release.is_empty()
        || release == "."
        || release == ".."
        || release.contains(['/', '\\', '\n', '\r'])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsafe release pointer target",
        ));
    }
    let staged = root.join(format!(
        ".current-update-{}-{}",
        std::process::id(),
        now_ms()
    ));
    symlink(release, &staged)?;
    fs::rename(staged, root.join("current"))
}

/// Return true after a healthy installed candidate has atomically become current.
pub fn guardian_confirm_ready(exe: &Path) -> io::Result<bool> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = exe;
        Ok(false)
    }
    #[cfg(target_os = "linux")]
    {
        let Some(installed) = installed_layout(exe)? else {
            return Ok(false);
        };
        let marker_path = installed.root.join(".node-update.json");
        let mut marker: Marker = match fs::read(&marker_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if marker.phase != "ready" {
            return Ok(false);
        }
        let current = current_pointer(&installed.root)?;
        if current != marker.previous_release && current != marker.candidate_release {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "node release pointer changed during trial",
            ));
        }
        let candidate = installed.root.join(&marker.candidate_release);
        let binary = candidate.join(component_name("protectorate-rs"));
        let (size, sha) = digest(&binary)?;
        verify_bundle(&candidate, &marker.to, size, &sha)?;
        if current == marker.previous_release {
            switch_pointer(&installed.root, &marker.candidate_release)?;
        }
        marker.phase = "confirmed".into();
        fsutil::write_file(&marker_path, &serde_json::to_vec(&marker)?, 0o600)?;
        Ok(true)
    }
}
