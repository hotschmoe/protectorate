use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Never follows a symlink in the last component, and never blocks on a FIFO.
fn open(path: &Path, directory: bool) -> io::Result<File> {
    let mut flags = libc::O_NOFOLLOW | libc::O_NONBLOCK;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    OpenOptions::new().read(true).custom_flags(flags).open(path)
}

pub fn fd_path(dir: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
}

pub fn child(dir: &File, name: &std::ffi::OsStr, directory: bool) -> io::Result<File> {
    open(&fd_path(dir).join(name), directory)
}

/// A configured root: the owner's path, so symlinks in it are followed. Discovery below it is not.
pub fn directory(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NONBLOCK)
        .open(path)
}

pub fn file(path: &Path) -> io::Result<File> {
    let file = open(path, false)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source is not a regular file",
        ));
    }
    Ok(file)
}
