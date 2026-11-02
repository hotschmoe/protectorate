use std::fs::{self, OpenOptions};
use std::path::Path;

use crate::error::{Error, Result};

pub(crate) fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| Error::new(format!("creating {}: {e}", dir.display())))?;
    let mode = fs::metadata(dir)
        .map_err(|e| Error::new(format!("{}: {e}", dir.display())))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| Error::new(format!("restricting {}: {e}", dir.display())))?;
    }
    Ok(())
}

/// Writes through a temporary file and a rename, so a reader never sees half a file.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let ctx = |e: std::io::Error| Error::new(format!("writing {}: {e}", path.display()));
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(ctx)?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(ctx)?;
    fs::rename(&tmp, path).map_err(ctx)
}

pub(crate) fn append_log(path: &Path) -> Result<fs::File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| Error::new(format!("opening {}: {e}", path.display())))
}

pub(crate) fn path_str(p: &Path) -> Result<&str> {
    match p.to_str() {
        Some(s) if p.is_absolute() => Ok(s),
        _ => Err(Error::new(format!(
            "{} is not an absolute UTF-8 path",
            p.display()
        ))),
    }
}
