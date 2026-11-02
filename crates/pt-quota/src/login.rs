use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The login-file home: `$HOME`, or an override.
pub(crate) fn home_dir(over: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = over {
        return Ok(p.to_path_buf());
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::other("HOME is not set"))
}

pub(crate) fn read_login(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::unavailable("not logged in (no login file)"),
            _ => Error::other(format!("open login file: {e}")),
        })?;
    if !file
        .metadata()
        .map_err(|e| Error::other(format!("stat login file: {e}")))?
        .is_file()
    {
        return Err(Error::other("login file is not a regular file"));
    }
    let mut buf = Vec::new();
    file.take(1 << 20)
        .read_to_end(&mut buf)
        .map_err(|e| Error::other(format!("read login file: {e}")))?;
    Ok(buf)
}
