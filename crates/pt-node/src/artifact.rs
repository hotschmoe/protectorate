//! Opening a job's artifact: a path without `..`, relative to the job's directory or absolute on
//! this node's OS, naming a regular file that resolves, symlinks and all, to a path inside the
//! job's directory. The check is made on the canonical path, the open refuses a symlink in its
//! last component, and on Linux the opened file's own path (`/proc/self/fd`) is checked again,
//! so a directory swapped in between cannot lead the read outside.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use pt_frame::msg::{self, ErrorCode};

use crate::job_error::JobError;

fn bad(m: String) -> JobError {
    JobError::new(ErrorCode::BadRequest, m)
}

fn resolve(path: &Path, what: &str) -> Result<PathBuf, JobError> {
    fs::canonicalize(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => JobError::new(
            ErrorCode::NotFound,
            format!("{what} {}: {e}", path.display()),
        ),
        _ => bad(format!("{what} {}: {e}", path.display())),
    })
}

pub(crate) fn open(dir: &Path, rel: &str) -> Result<File, JobError> {
    if rel.is_empty() || rel.len() > msg::MAX_PATH || rel.contains('\0') {
        return Err(bad(format!(
            "an artifact path is 1 to {} bytes with no NUL",
            msg::MAX_PATH
        )));
    }
    let path = Path::new(rel);
    let lexical = path.components().all(|c| match c {
        Component::Normal(_) | Component::CurDir => true,
        Component::Prefix(_) | Component::RootDir => path.is_absolute(),
        Component::ParentDir => false,
    });
    if !lexical {
        return Err(bad(format!(
            "artifact {rel:?} is outside the job's directory: give a path inside it, relative or \
             absolute for this node, without .."
        )));
    }
    let root = resolve(dir, "the job's directory")?;
    let real = resolve(&root.join(path), "artifact")?;
    if !real.starts_with(&root) {
        return Err(bad(format!(
            "artifact {rel:?} resolves to {}, outside the job's directory {}",
            real.display(),
            root.display()
        )));
    }
    let f = open_file(&real).map_err(|e| bad(format!("opening {}: {e}", real.display())))?;
    let meta = f
        .metadata()
        .map_err(|e| bad(format!("{}: {e}", real.display())))?;
    if !meta.is_file() {
        return Err(bad(format!(
            "artifact {rel:?} ({}) is not a regular file",
            real.display()
        )));
    }
    check_opened(&f, &real)?;
    Ok(f)
}

/// Never follows a symlink in the last component, and never blocks on a FIFO.
#[cfg(unix)]
fn open_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(target_os = "linux")]
fn check_opened(f: &File, real: &Path) -> Result<(), JobError> {
    use std::os::fd::AsRawFd;
    let link = format!("/proc/self/fd/{}", f.as_raw_fd());
    let opened = fs::read_link(&link).map_err(|e| bad(format!("{link}: {e}")))?;
    if opened != real {
        return Err(bad(format!(
            "{} moved to {} while it was opened",
            real.display(),
            opened.display()
        )));
    }
    Ok(())
}

// Elsewhere the canonical-path check stands alone (REVISIT).
#[cfg(not(target_os = "linux"))]
fn check_opened(_f: &File, _real: &Path) -> Result<(), JobError> {
    Ok(())
}
