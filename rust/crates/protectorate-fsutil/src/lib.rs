//! Small helpers shared across the workspace: crash-safe file replacement,
//! the wall clock as Unix time, and hex identifiers.

#[cfg(not(windows))]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(windows)]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Replace a file after syncing its data. `mode` is the Unix permission mask.
/// The parent directory must already exist.
pub fn write_file(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let basename = path.file_name().unwrap_or_default().to_string_lossy();
    let (mut file, tmp) = loop {
        let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{basename}.{}.{}", std::process::id(), n));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (file, tmp),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        rename(&tmp, path)?;
        sync_dir(dir);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Removes temp files that `write_file` left in `dir` when its process died
/// before the rename. A file whose process is still running is left alone.
/// Returns how many it removed.
pub fn remove_stale_temps(dir: &Path) -> io::Result<usize> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(pid) = temp_owner(&name.to_string_lossy()) else {
            continue;
        };
        if !process_exists(pid) {
            match fs::remove_file(entry.path()) {
                Ok(()) => removed += 1,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(removed)
}

/// The process id in a name of the form `.<name>.<pid>.<counter>`.
fn temp_owner(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('.')?;
    let mut parts = rest.rsplitn(3, '.');
    let counter = parts.next()?;
    let pid = parts.next()?;
    parts.next().filter(|base| !base.is_empty())?;
    counter.parse::<u64>().ok()?;
    pid.parse().ok()
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return true;
    };
    // EPERM means the process exists under another user.
    unsafe { libc::kill(pid, 0) == 0 || *libc::__errno_location() != libc::ESRCH }
}

/// Without a cheap liveness probe the file stays.
#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    true
}

/// Retry Windows sharing conflicts for up to five seconds.
pub fn retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    #[cfg(windows)]
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match op() {
                Err(e)
                    if e.kind() == io::ErrorKind::PermissionDenied && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                result => return result,
            }
        }
    }
    #[cfg(not(windows))]
    {
        op()
    }
}

pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    retry(|| fs::rename(from, to))
}

fn sync_dir(dir: &Path) {
    #[cfg(not(windows))]
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(windows)]
    let _ = dir;
}

/// Whole seconds since the Unix epoch; 0 if the clock reads earlier.
pub fn unix_seconds() -> i64 {
    since_epoch().as_secs().try_into().unwrap_or(i64::MAX)
}

/// Milliseconds since the Unix epoch; 0 if the clock reads earlier.
pub fn unix_millis() -> i64 {
    since_epoch().as_millis().try_into().unwrap_or(i64::MAX)
}

fn since_epoch() -> std::time::Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// Lowercase hexadecimal of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0xf)] as char);
    }
    out
}

/// `len` bytes from the operating system's random source, as lowercase hex.
pub fn random_hex(len: usize) -> io::Result<String> {
    let mut bytes = vec![0; len];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| io::Error::other(format!("read random bytes: {error}")))?;
    Ok(hex(&bytes))
}
