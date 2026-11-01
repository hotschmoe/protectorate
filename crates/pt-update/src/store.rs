//! `<root>/releases/`: one directory per version, plus the `current` and `previous` symlinks,
//! `probation.json` and the `.lock` file that serialises changes.

use crate::fetch::{MAX_ASSET, MAX_SUMS, MAX_VERSION};
use crate::sums::plain_name;
use crate::{Error, ErrorKind, Pick, Release, Result, Source, Sums};
use pt_json::{ToJson, record};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// A node's update on trial (SPEC 6a): it reverts to `previous` unless the hub makes contact
    /// before `deadline_ms`. Times are wall-clock milliseconds since the epoch.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Probation {
        pub version: String,
        pub previous: String,
        pub switched_ms: u64,
        pub deadline_ms: u64,
        pub contacted_ms: Option<u64>,
        pub rolled_back_ms: Option<u64>,
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
    /// The window passed without contact and `current` went back. The caller exits so the
    /// service manager restarts the previous binary.
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
        let from = self.current()?;
        if from.as_deref() != Some(version) {
            if let Some(f) = &from {
                self.set_pointer(PREVIOUS, f)?;
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
        self.rollback_locked()
    }

    fn rollback_locked(&self) -> Result<Switch> {
        let prev = self
            .previous()?
            .ok_or_else(|| store_err(format!("{}: no previous release", self.dir.display())))?;
        self.require_version_dir(&prev)?;
        let cur = self.current()?;
        if let Some(c) = &cur {
            self.set_pointer(PREVIOUS, c)?;
        }
        self.set_pointer(CURRENT, &prev)?;
        sync_dir(&self.dir)?;
        Ok(Switch {
            from: cur,
            to: prev,
        })
    }

    /// The hub supervisor's hook: switches, then calls `check` every `poll` until it passes or
    /// `within` runs out, and then rolls back. The check usually restarts nothing itself; it asks
    /// the new hub for its health.
    pub fn switch_checked(
        &self,
        version: &str,
        within: Duration,
        poll: Duration,
        mut check: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<Switch> {
        let sw = self.switch(version)?;
        let deadline = Instant::now() + within;
        let last = loop {
            let e = match check() {
                Ok(()) => return Ok(sw),
                Err(e) => e,
            };
            let now = Instant::now();
            if now >= deadline {
                break e;
            }
            std::thread::sleep(poll.min(deadline - now));
        };
        let failed = format!(
            "{version} failed its health check for {} s: {last}",
            within.as_secs_f32()
        );
        if sw.from.is_none() || sw.from.as_deref() == Some(version) {
            return Err(Error::new(
                ErrorKind::Health,
                format!("{failed}; nothing to roll back to"),
            ));
        }
        let back = self.rollback().map_err(|e| e.context(&failed))?;
        Err(Error::new(
            ErrorKind::Health,
            format!("{failed}; rolled back to {}", back.to),
        ))
    }

    /// A node's update: switches and records a probation that [`Store::check_probation`] turns
    /// into a rollback unless [`Store::hub_contacted`] is called within `window`.
    pub fn switch_on_probation(&self, version: &str, window: Duration) -> Result<Switch> {
        let from = self.current()?.ok_or_else(|| {
            store_err("no current release to fall back to; use switch for a first install")
        })?;
        let sw = self.switch(version)?;
        if sw.from.as_deref() == Some(version) {
            return Ok(sw);
        }
        let now = now_ms();
        let p = Probation {
            version: version.to_string(),
            previous: from,
            switched_ms: now,
            deadline_ms: now.saturating_add(window.as_millis() as u64),
            contacted_ms: None,
            rolled_back_ms: None,
        };
        let _l = self.lock()?;
        self.write_probation(&p)?;
        Ok(sw)
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

    /// Records the hub's contact. True when this confirmed an update on trial.
    pub fn hub_contacted(&self) -> Result<bool> {
        let _l = self.lock()?;
        let Some(mut p) = self.open_probation()? else {
            return Ok(false);
        };
        p.contacted_ms = Some(now_ms());
        self.write_probation(&p)?;
        Ok(true)
    }

    /// Run at start and then periodically by the node.
    pub fn check_probation(&self) -> Result<Verdict> {
        let _l = self.lock()?;
        let Some(mut p) = self.probation()? else {
            return Ok(Verdict::Idle);
        };
        if p.contacted_ms.is_some() {
            return Ok(Verdict::Confirmed);
        }
        if self.open_probation()?.is_none() {
            return Ok(Verdict::Idle);
        }
        let now = now_ms();
        if now < p.deadline_ms {
            return Ok(Verdict::Waiting {
                remaining: Duration::from_millis(p.deadline_ms - now),
            });
        }
        if self.previous()?.as_deref() != Some(p.previous.as_str()) {
            return Err(store_err(format!(
                "probation of {} expects previous {}, found {:?}",
                p.version,
                p.previous,
                self.previous()?
            )));
        }
        let sw = self.rollback_locked()?;
        p.rolled_back_ms = Some(now);
        self.write_probation(&p)?;
        Ok(Verdict::RolledBack(sw))
    }
}
