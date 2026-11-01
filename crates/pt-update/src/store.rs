//! `<root>/releases/`: one directory per version, plus the `current` and `previous` symlinks,
//! `probation.json` and the `.lock` file that serialises changes.

use crate::fetch::{MAX_ASSET, MAX_SUMS, MAX_VERSION};
use crate::sums::plain_name;
use crate::{Error, ErrorKind, Pick, Release, Result, Source, Sums};
use pt_json::{ToJson, record};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CURRENT: &str = "current";
const PREVIOUS: &str = "previous";
const PROBATION: &str = "probation.json";

/// A name usable as `releases/<version>`: a tag such as `v1.2.3-dev.4`, never a pointer name.
pub(crate) fn version_name(v: &str) -> bool {
    plain_name(v)
        && v.len() <= 128
        && v.as_bytes()[0].is_ascii_alphanumeric()
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
        && v != CURRENT
        && v != PREVIOUS
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    /// What `current` named before, if anything.
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub version: String,
    pub dir: PathBuf,
    /// The version was on disk already and matched `SHA256SUMS`; nothing was downloaded.
    pub reused: bool,
}

record! {
    /// A release on trial (SPEC 6a): it reverts to `previous` unless confirmed before
    /// `deadline_ms` (a node: the hub made contact; the hub: its own health check passed).
    /// Times are wall-clock milliseconds since the epoch.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Probation {
        pub version: String,
        pub previous: String,
        pub switched_ms: u64,
        pub deadline_ms: u64,
        pub contacted_ms: Option<u64>,
        pub rolled_back_ms: Option<u64>,
        /// Headscale snapshot to restore before rolling back a hub release.
        pub headscale_snapshot: Option<String>,
        /// Starts counted by [`Store::start_attempt`].
        pub starts: u64 = 0,
        /// Why it was rolled back.
        pub reason: Option<String>,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No update on trial.
    Idle,
    /// On trial and not contacted yet.
    Waiting { remaining: Duration },
    /// The hub made contact; the update stays.
    Confirmed,
    /// The window passed without confirmation, or the release kept failing, and `current` went
    /// back. The caller exits so the service manager restarts the previous binary.
    RolledBack(Switch),
}

fn store_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Store, msg)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn unique(prefix: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!(".{prefix}-{}-{n}", std::process::id())
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| Error::io(format!("syncing {}", dir.display()), e))
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    File::create(path)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .map_err(|e| Error::io(format!("writing {}", path.display()), e))
}

/// Test knob: `PT_TEST_CRASH_AT=<step>` ends the process right after that step of a switch or
/// rollback, as a power loss would.
#[cfg(debug_assertions)]
fn crash_point(step: &str) {
    if std::env::var("PT_TEST_CRASH_AT").is_ok_and(|s| s == step) {
        std::process::exit(86);
    }
}

#[cfg(not(debug_assertions))]
fn crash_point(_step: &str) {}

/// argv[0] as an absolute path.
fn argv0() -> PathBuf {
    let a = std::env::args_os()
        .next()
        .map(PathBuf::from)
        .unwrap_or_default();
    if a.is_absolute() || a.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        return a;
    }
    std::env::current_dir().unwrap_or_default().join(a)
}

/// The local release store. Directories of old versions are never removed here.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

/// Held while pointers or `probation.json` change; released on drop.
struct Lock(#[allow(dead_code)] File);

impl Store {
    /// `<root>/releases`, created if missing.
    pub fn open(root: &Path) -> Result<Store> {
        let dir = root.join("releases");
        fs::create_dir_all(&dir)
            .map_err(|e| Error::io(format!("creating {}", dir.display()), e))?;
        Ok(Store { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn version_dir(&self, version: &str) -> PathBuf {
        self.dir.join(version)
    }

    /// `releases/current`, the path a service runs its binary through.
    pub fn current_dir(&self) -> PathBuf {
        self.dir.join(CURRENT)
    }

    /// Whether this process was started as `releases/current/<program>`, the only start a
    /// switch of `current` changes. Read from argv[0], which a service manager sets to the path
    /// it ran.
    pub fn started_from_current(&self, program: &str) -> std::result::Result<(), String> {
        let argv0 = argv0();
        let parent = argv0.parent();
        let ok = argv0.file_name().is_some_and(|f| f == program)
            && parent
                .and_then(|p| p.file_name())
                .is_some_and(|f| f == CURRENT)
            && parent
                .and_then(Path::parent)
                .and_then(|d| fs::canonicalize(d).ok())
                .is_some_and(|d| fs::canonicalize(&self.dir).is_ok_and(|r| r == d));
        if ok {
            return Ok(());
        }
        Err(format!(
            "this process runs {}, not {}/{CURRENT}/{program}, so a switch would not take \
             effect; point the service at releases/{CURRENT} first",
            argv0.display(),
            self.dir.display()
        ))
    }

    /// The release this process was started from, when argv[0] lies in this store: what
    /// `current` named at the start, for a service run through it.
    pub fn running_release(&self) -> Option<String> {
        let real = fs::canonicalize(argv0()).ok()?;
        let vdir = real.parent()?;
        let name = vdir.file_name()?.to_str()?.to_string();
        let store = fs::canonicalize(&self.dir).ok()?;
        (version_name(&name) && vdir.parent()? == store).then_some(name)
    }

    /// Runs `releases/<version>/<program> <arg>` and checks that its first line is
    /// `<program> <version>`, as the installers do: a binary for the wrong machine or release
    /// never becomes `current`.
    pub fn check_binary(&self, version: &str, program: &str, arg: &str) -> Result<()> {
        const TIMEOUT: Duration = Duration::from_secs(30);
        #[cfg(debug_assertions)]
        let timeout = std::env::var("PT_TEST_CHECK_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(TIMEOUT);
        #[cfg(not(debug_assertions))]
        let timeout = TIMEOUT;
        let bin = self.version_dir(version).join(program);
        let health = |m: String| Error::new(ErrorKind::Health, m);
        let mut command = Command::new(&bin);
        command
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|e| health(format!("running {}: {e}", bin.display())))?;
        let mut stdout = child.stdout.take();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(s) = stdout.as_mut() {
                let _ = s.take(64 * 1024).read_to_end(&mut out);
            }
            let _ = tx.send(out);
        });
        let deadline = Instant::now() + timeout;
        let mut status = None;
        let mut out = None;
        while Instant::now() < deadline {
            if status.is_none() {
                match child.try_wait() {
                    Ok(s) => status = s,
                    Err(e) => {
                        #[cfg(unix)]
                        let _ = pt_sys::kill_group(child.id(), pt_sys::SIGKILL);
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(health(format!("waiting for {}: {e}", bin.display())));
                    }
                }
            }
            if out.is_none() {
                out = rx.try_recv().ok();
            }
            if status.is_some() && out.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if status.is_none() || out.is_none() {
            #[cfg(unix)]
            let _ = pt_sys::kill_group(child.id(), pt_sys::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            return Err(health(format!(
                "{} {arg} did not finish in {} s",
                bin.display(),
                timeout.as_secs()
            )));
        }
        let out = out.unwrap_or_default();
        let line = String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let want = format!("{program} {version}");
        match status {
            None => Err(health(format!(
                "{} {arg} did not finish in {} s",
                bin.display(),
                TIMEOUT.as_secs()
            ))),
            Some(st) if !st.success() || line != want => Err(health(format!(
                "{} {arg} reports {line:?} ({st}), not {want:?}",
                bin.display()
            ))),
            Some(_) => Ok(()),
        }
    }

    fn lock(&self) -> Result<Lock> {
        let path = self.dir.join(".lock");
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| Error::io(format!("opening {}", path.display()), e))?;
        f.lock()
            .map_err(|e| Error::io(format!("locking {}", path.display()), e))?;
        Ok(Lock(f))
    }

    fn pointer(&self, name: &str) -> Result<Option<String>> {
        let path = self.dir.join(name);
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("inspecting {}", path.display()), e)),
        };
        if !meta.file_type().is_symlink() {
            return Err(store_err(format!("{} is not a symlink", path.display())));
        }
        let target = fs::read_link(&path)
            .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;
        match target.to_str() {
            Some(v) if version_name(v) => Ok(Some(v.to_string())),
            _ => Err(store_err(format!(
                "{} points at {}, not a version in {}",
                path.display(),
                target.display(),
                self.dir.display()
            ))),
        }
    }

    pub fn current(&self) -> Result<Option<String>> {
        self.pointer(CURRENT)
    }

    pub fn previous(&self) -> Result<Option<String>> {
        self.pointer(PREVIOUS)
    }

    pub fn ensure_previous(&self) -> Result<()> {
        let _l = self.lock()?;
        if self.previous()?.is_none() {
            let current = self
                .current()?
                .ok_or_else(|| store_err("no current release"))?;
            self.set_pointer(PREVIOUS, &current)?;
            sync_dir(&self.dir)?;
        }
        Ok(())
    }

    /// Every staged version directory, sorted by name.
    pub fn versions(&self) -> Result<Vec<String>> {
        let rd = fs::read_dir(&self.dir)
            .map_err(|e| Error::io(format!("listing {}", self.dir.display()), e))?;
        let mut out = Vec::new();
        for e in rd {
            let e = e.map_err(|e| Error::io(format!("listing {}", self.dir.display()), e))?;
            let real_dir = e.file_type().is_ok_and(|t| t.is_dir());
            if let Some(name) = e.file_name().to_str()
                && real_dir
                && version_name(name)
            {
                out.push(name.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    fn require_version_dir(&self, version: &str) -> Result<PathBuf> {
        if !version_name(version) {
            return Err(store_err(format!("{version:?} is not a version name")));
        }
        let d = self.version_dir(version);
        match fs::symlink_metadata(&d) {
            Ok(m) if m.is_dir() => Ok(d),
            Ok(_) => Err(store_err(format!("{} is not a directory", d.display()))),
            Err(e) => Err(Error::io(format!("release {version}: {}", d.display()), e)),
        }
    }

    #[cfg(unix)]
    fn set_pointer(&self, name: &str, version: &str) -> Result<()> {
        let tmp = self.dir.join(unique(name));
        std::os::unix::fs::symlink(version, &tmp)
            .map_err(|e| Error::io(format!("creating {}", tmp.display()), e))?;
        let dst = self.dir.join(name);
        fs::rename(&tmp, &dst).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            Error::io(
                format!("renaming {} to {}", tmp.display(), dst.display()),
                e,
            )
        })
    }

    #[cfg(not(unix))]
    fn set_pointer(&self, name: &str, _version: &str) -> Result<()> {
        Err(store_err(format!(
            "switching releases/{name} is not implemented on this OS yet"
        )))
    }

    /// Downloads `picks` of `release` into `releases/<tag>/` under their `install_as` names,
    /// with the release's `SHA256SUMS` and `VERSION` beside them. Every asset is checked
    /// against its GitHub size and its sha256 before the directory appears (by rename); a failed
    /// stage leaves nothing behind. A version already on disk is checked and reused, never
    /// overwritten.
    pub fn stage(&self, src: &Source, release: &Release, picks: &[Pick]) -> Result<Staged> {
        let tag = release.tag.as_str();
        if !version_name(tag) {
            return Err(store_err(format!("{tag:?} is not a version name")));
        }
        let ctx = |e: Error| e.context(format!("staging {tag}"));
        let sums_bytes = src
            .get(
                &release.asset("SHA256SUMS").map_err(ctx)?.url,
                MAX_SUMS,
                false,
            )
            .map_err(ctx)?;
        let sums = Sums::parse(&sums_bytes).map_err(ctx)?;
        let version_bytes = src
            .get(
                &release.asset("VERSION").map_err(ctx)?.url,
                MAX_VERSION,
                false,
            )
            .map_err(ctx)?;
        sums.verify_bytes("VERSION", &version_bytes).map_err(ctx)?;
        let said = String::from_utf8_lossy(&version_bytes);
        if said.trim() != tag {
            return Err(ctx(Error::new(
                ErrorKind::Verify,
                format!("VERSION says {:?}", said.trim()),
            )));
        }
        let mut assets = Vec::new();
        for p in picks {
            if !plain_name(&p.install_as) {
                return Err(ctx(store_err(format!(
                    "bad install name {:?}",
                    p.install_as
                ))));
            }
            sums.get(&p.asset).ok_or_else(|| {
                ctx(Error::new(
                    ErrorKind::Verify,
                    format!("SHA256SUMS has no entry for {}", p.asset),
                ))
            })?;
            assets.push((p, release.asset(&p.asset).map_err(ctx)?));
        }

        let vdir = self.version_dir(tag);
        if fs::symlink_metadata(&vdir).is_ok() {
            let _l = self.lock()?;
            self.verify_dir(&vdir, &sums, picks).map_err(ctx)?;
            return Ok(Staged {
                version: tag.to_string(),
                dir: vdir,
                reused: true,
            });
        }

        let staging = self.dir.join(unique(&format!("staging-{tag}")));
        fs::create_dir(&staging)
            .map_err(|e| ctx(Error::io(format!("creating {}", staging.display()), e)))?;
        let fill = || -> Result<bool> {
            for (p, a) in &assets {
                let dest = staging.join(&p.install_as);
                let len = src.download(&a.url, MAX_ASSET, &dest)?;
                if a.size != 0 && len != a.size {
                    return Err(Error::new(
                        ErrorKind::Verify,
                        format!("{}: got {len} bytes, the release says {}", p.asset, a.size),
                    ));
                }
                sums.verify_file(&p.asset, &dest)?;
                let ioctx = |e| Error::io(format!("finishing {}", dest.display()), e);
                if p.executable {
                    set_executable(&dest).map_err(ioctx)?;
                }
                File::open(&dest)
                    .and_then(|f| f.sync_all())
                    .map_err(ioctx)?;
            }
            write_synced(&staging.join("SHA256SUMS"), &sums_bytes)?;
            write_synced(&staging.join("VERSION"), &version_bytes)?;
            sync_dir(&staging)?;
            let _l = self.lock()?;
            if fs::symlink_metadata(&vdir).is_ok() {
                self.verify_dir(&vdir, &sums, picks)?;
                return Ok(true);
            }
            fs::rename(&staging, &vdir).map_err(|e| {
                Error::io(
                    format!("renaming {} to {}", staging.display(), vdir.display()),
                    e,
                )
            })?;
            sync_dir(&self.dir)?;
            Ok(false)
        };
        let res = fill();
        if !matches!(res, Ok(false)) {
            // Only our own staging directory: a failed or redundant download.
            let _ = fs::remove_dir_all(&staging);
        }
        Ok(Staged {
            version: tag.to_string(),
            dir: vdir,
            reused: res.map_err(ctx)?,
        })
    }

    fn verify_dir(&self, vdir: &Path, sums: &Sums, picks: &[Pick]) -> Result<()> {
        for p in picks {
            let path = vdir.join(&p.install_as);
            sums.verify_file(&p.asset, &path).map_err(|e| {
                e.context(format!(
                    "{} exists and differs from the release",
                    vdir.display()
                ))
            })?;
        }
        Ok(())
    }

    /// Points `current` at a staged version; `previous` takes what `current` named.
    pub fn switch(&self, version: &str) -> Result<Switch> {
        self.require_version_dir(version)?;
        let _l = self.lock()?;
        self.refuse_open_trial()?;
        self.switch_locked(version)
    }

    /// `previous` first, so a crash between the two renames leaves both on the old release.
    fn switch_locked(&self, version: &str) -> Result<Switch> {
        let from = self.current()?;
        if from.as_deref() != Some(version) {
            if let Some(f) = &from {
                self.set_pointer(PREVIOUS, f)?;
                sync_dir(&self.dir)?;
                crash_point("switch-previous");
            }
            self.set_pointer(CURRENT, version)?;
            sync_dir(&self.dir)?;
        }
        Ok(Switch {
            from,
            to: version.to_string(),
        })
    }

    /// Swaps `current` and `previous`, so a second rollback undoes the first.
    pub fn rollback(&self) -> Result<Switch> {
        let _l = self.lock()?;
        self.refuse_open_trial()?;
        self.rollback_locked()
    }

    fn rollback_locked(&self) -> Result<Switch> {
        let prev = self
            .previous()?
            .ok_or_else(|| store_err(format!("{}: no previous release", self.dir.display())))?;
        self.require_version_dir(&prev)?;
        let cur = self.current()?;
        // `current` first: a crash between the renames leaves both on the release that worked,
        // never both on the one being left.
        self.set_pointer(CURRENT, &prev)?;
        sync_dir(&self.dir)?;
        crash_point("rollback-current");
        if let Some(c) = &cur {
            self.set_pointer(PREVIOUS, c)?;
        }
        sync_dir(&self.dir)?;
        Ok(Switch {
            from: cur,
            to: prev,
        })
    }

    /// Switches and records a probation that [`Store::check_probation`] turns into a rollback
    /// unless [`Store::confirm`] is called within `window`. The probation is written before
    /// the switch, under the same lock, so no crash leaves the new release current untried;
    /// one that names a release `current` does not is not open.
    pub fn switch_on_probation(&self, version: &str, window: Duration) -> Result<Switch> {
        self.switch_on_probation_with_snapshot(version, window, None)
    }

    pub fn switch_on_probation_with_snapshot(
        &self,
        version: &str,
        window: Duration,
        headscale_snapshot: Option<String>,
    ) -> Result<Switch> {
        self.require_version_dir(version)?;
        let _l = self.lock()?;
        self.refuse_open_trial()?;
        let from = self.current()?.ok_or_else(|| {
            store_err("no current release to fall back to; use switch for a first install")
        })?;
        if from == version {
            return Ok(Switch {
                from: Some(from),
                to: version.to_string(),
            });
        }
        let now = now_ms();
        let window = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
        let p = Probation {
            version: version.to_string(),
            previous: from,
            switched_ms: now,
            deadline_ms: now.saturating_add(window),
            contacted_ms: None,
            rolled_back_ms: None,
            headscale_snapshot,
            starts: 0,
            reason: None,
        };
        self.write_probation(&p)?;
        crash_point("probation-written");
        self.switch_locked(version)
    }

    pub fn probation(&self) -> Result<Option<Probation>> {
        let path = self.dir.join(PROBATION);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("reading {}", path.display()), e)),
        };
        pt_json::from_bytes(&bytes)
            .map(Some)
            .map_err(|e| store_err(format!("{}: {e}", path.display())))
    }

    fn write_probation(&self, p: &Probation) -> Result<()> {
        let tmp = self.dir.join(unique("probation"));
        write_synced(&tmp, format!("{}\n", p.to_json()).as_bytes())?;
        let dst = self.dir.join(PROBATION);
        fs::rename(&tmp, &dst).map_err(|e| {
            Error::io(
                format!("renaming {} to {}", tmp.display(), dst.display()),
                e,
            )
        })?;
        sync_dir(&self.dir)
    }

    fn open_probation(&self) -> Result<Option<Probation>> {
        let Some(p) = self.probation()? else {
            return Ok(None);
        };
        let open = p.contacted_ms.is_none()
            && p.rolled_back_ms.is_none()
            && self.current()?.as_deref() == Some(p.version.as_str());
        Ok(open.then_some(p))
    }

    fn refuse_open_trial(&self) -> Result<()> {
        if let Some(p) = self.open_probation()? {
            return Err(store_err(format!(
                "release {} is still on probation; wait for confirmation or rollback",
                p.version
            )));
        }
        Ok(())
    }

    /// Keeps `version` when it is the release on trial; true when this confirmed it. A check
    /// that passed for one release never confirms another switched in since.
    pub fn confirm(&self, version: &str) -> Result<bool> {
        let _l = self.lock()?;
        let Some(mut p) = self.open_probation()?.filter(|p| p.version == version) else {
            return Ok(false);
        };
        let now = now_ms();
        if now >= p.deadline_ms || p.reason.is_some() {
            return Ok(false);
        }
        p.contacted_ms = Some(now);
        self.write_probation(&p)?;
        Ok(true)
    }

    /// Run at start and then periodically by the release on trial.
    pub fn check_probation(&self) -> Result<Verdict> {
        let _l = self.lock()?;
        self.judge(false, 0, &mut no_snapshot)
    }

    /// Run by the service manager before each start, from the previous release (known to work):
    /// counts the start, and rolls back once a release on trial has started more than
    /// `max_starts` times without being confirmed, or its window has passed.
    pub fn start_attempt(&self, max_starts: u64) -> Result<Verdict> {
        let _l = self.lock()?;
        self.judge(true, max_starts, &mut no_snapshot)
    }

    pub fn start_attempt_with_restore(
        &self,
        max_starts: u64,
        mut restore: impl FnMut(&Probation) -> std::result::Result<(), String>,
    ) -> Result<Verdict> {
        let _l = self.lock()?;
        self.judge(true, max_starts, &mut restore)
    }

    pub fn request_rollback(&self, reason: &str) -> Result<bool> {
        let _l = self.lock()?;
        let Some(mut p) = self.open_probation()? else {
            return Ok(false);
        };
        p.reason = Some(reason.to_string());
        self.write_probation(&p)?;
        Ok(true)
    }

    /// Rolls a release on trial back now, for `reason` (it failed its own health check).
    pub fn fail_probation(&self, reason: &str) -> Result<Option<Switch>> {
        let _l = self.lock()?;
        let Some(mut p) = self.open_probation()? else {
            return Ok(None);
        };
        self.roll_back_trial(&mut p, reason, &mut no_snapshot)
            .map(Some)
    }

    fn judge(
        &self,
        count: bool,
        max_starts: u64,
        restore: &mut impl FnMut(&Probation) -> std::result::Result<(), String>,
    ) -> Result<Verdict> {
        let Some(mut p) = self.probation()? else {
            return Ok(Verdict::Idle);
        };
        if p.contacted_ms.is_some() {
            return Ok(Verdict::Confirmed);
        }
        if self.open_probation()?.is_none() {
            return Ok(Verdict::Idle);
        }
        if let Some(reason) = p.reason.clone() {
            return self
                .roll_back_trial(&mut p, &reason, restore)
                .map(Verdict::RolledBack);
        }
        let now = now_ms();
        if count {
            p.starts += 1;
            if p.starts > max_starts {
                let why = format!("{} starts without passing its check", p.starts - 1);
                return self
                    .roll_back_trial(&mut p, &why, restore)
                    .map(Verdict::RolledBack);
            }
            self.write_probation(&p)?;
        }
        if now < p.deadline_ms {
            return Ok(Verdict::Waiting {
                remaining: Duration::from_millis(p.deadline_ms - now),
            });
        }
        let why = format!(
            "not confirmed within {} s",
            p.deadline_ms.saturating_sub(p.switched_ms) / 1000
        );
        self.roll_back_trial(&mut p, &why, restore)
            .map(Verdict::RolledBack)
    }

    fn roll_back_trial(
        &self,
        p: &mut Probation,
        reason: &str,
        restore: &mut impl FnMut(&Probation) -> std::result::Result<(), String>,
    ) -> Result<Switch> {
        if self.previous()?.as_deref() != Some(p.previous.as_str()) {
            return Err(store_err(format!(
                "probation of {} expects previous {}, found {:?}",
                p.version,
                p.previous,
                self.previous()?
            )));
        }
        self.require_version_dir(&p.previous)?;
        if p.reason.as_deref() != Some(reason) {
            p.reason = Some(reason.to_string());
            self.write_probation(p)?;
        }
        restore(p).map_err(|e| store_err(format!("restoring before rollback: {e}")))?;
        self.set_pointer(CURRENT, &p.previous)?;
        sync_dir(&self.dir)?;
        crash_point("rollback-current");
        let sw = Switch {
            from: Some(p.version.clone()),
            to: p.previous.clone(),
        };
        p.rolled_back_ms = Some(now_ms());
        p.reason = Some(reason.to_string());
        self.write_probation(p)?;
        Ok(sw)
    }
}

fn no_snapshot(p: &Probation) -> std::result::Result<(), String> {
    if p.headscale_snapshot.is_some() {
        return Err("headscale restore requires hub probation before start".into());
    }
    Ok(())
}
