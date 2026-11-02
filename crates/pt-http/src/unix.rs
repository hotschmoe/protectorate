use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

/// Binds a unix socket at `path`, however long: see [`unix_addr`].
pub(crate) fn unix_bind(path: &Path) -> io::Result<UnixListener> {
    let (_dir, addr) = unix_addr(path)?;
    UnixListener::bind(addr)
}

/// Connects to a unix socket at `path`, however long: see [`unix_addr`].
pub(crate) fn unix_connect(path: &Path) -> io::Result<UnixStream> {
    let (_dir, addr) = unix_addr(path)?;
    UnixStream::connect(addr)
}

/// An address for `path` that fits `sun_path` (107 bytes and a NUL). A longer path is reached
/// through its directory's descriptor under /proc/self/fd, which the returned file keeps open.
fn unix_addr(path: &Path) -> io::Result<(Option<File>, Cow<'_, Path>)> {
    const SUN_PATH_MAX: usize = 107;
    if path.as_os_str().len() <= SUN_PATH_MAX || !cfg!(target_os = "linux") {
        return Ok((None, Cow::Borrowed(path)));
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok((None, Cow::Borrowed(path)));
    };
    let dir = File::open(dir)
        .map_err(|e| io::Error::new(e.kind(), format!("open {}: {e}", dir.display())))?;
    let short = Path::new("/proc/self/fd")
        .join(dir.as_raw_fd().to_string())
        .join(name);
    Ok((Some(dir), Cow::Owned(short)))
}
