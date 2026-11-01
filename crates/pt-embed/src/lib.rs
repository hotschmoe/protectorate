//! Binaries a protectorate binary carries (headscale and member in the hub, member in the node):
//! gzip bytes plus the SHA-256 of the binary. At start each is unpacked once to a
//! content-addressed path and checked again on every start, so a corrupted or replaced copy is
//! never run. Unpacking is our own inflate (no system gzip, so it works on Windows too); the
//! build scripts make the values with `build`.

#![forbid(unsafe_code)]

pub mod build;
mod inflate;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub use inflate::{InflateError, crc32, gunzip, inflate};

/// Largest binary `unpack` inflates.
pub const MAX_UNPACKED: usize = 512 << 20;

#[derive(Clone, Copy, Debug)]
pub struct Embedded {
    pub name: &'static str,
    /// Lowercase hex SHA-256 of the unpacked binary.
    pub sha256: &'static str,
    pub gz: &'static [u8],
}

fn digest(path: &Path) -> Result<String, String> {
    pt_hash::sha256_file(path)
        .map(|d| pt_hash::to_hex(&d))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// `dir` and its parents, `dir` itself owner-only on Unix.
pub fn private_dir(dir: &Path) -> Result<(), String> {
    #[cfg(not(unix))]
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let mode = fs::metadata(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("restricting {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

impl Embedded {
    /// `<name>-<sha256>`, with `.exe` on Windows.
    pub fn file_name(&self) -> String {
        format!(
            "{}-{}{}",
            self.name,
            self.sha256,
            std::env::consts::EXE_SUFFIX
        )
    }

    /// `<dir>/<file_name>`, executable. A missing or mismatching file is inflated again into a
    /// temporary sibling, checked, then renamed in place.
    pub fn unpack(&self, dir: &Path) -> Result<PathBuf, String> {
        let ctx = |e: String| format!("unpacking the embedded {}: {e}", self.name);
        private_dir(dir).map_err(ctx)?;
        let path = dir.join(self.file_name());
        if path.is_file() && digest(&path).map_err(ctx)? == self.sha256 {
            return Ok(path);
        }
        let bytes = gunzip(self.gz, MAX_UNPACKED).map_err(|e| ctx(e.to_string()))?;
        let got = pt_hash::to_hex(&pt_hash::sha256(&bytes));
        if got != self.sha256 {
            return Err(ctx(format!(
                "it inflates to SHA-256 {got}, the build recorded {}",
                self.sha256
            )));
        }
        let tmp = dir.join(format!(".{}.{}", self.file_name(), std::process::id()));
        let written = fs::File::create(&tmp).and_then(|mut f| {
            f.write_all(&bytes)?;
            f.sync_all()
        });
        written.map_err(|e| ctx(format!("writing {}: {e}", tmp.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
                .map_err(|e| ctx(format!("chmod {}: {e}", tmp.display())))?;
        }
        fs::rename(&tmp, &path).map_err(|e| ctx(format!("installing {}: {e}", path.display())))?;
        Ok(path)
    }
}
