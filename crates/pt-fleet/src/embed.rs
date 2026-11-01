//! headscale and member as the hub binary carries them: gzip bytes plus the SHA-256 of the
//! binary. At start each is unpacked once to a content-addressed path and checked again on every
//! start, so a corrupted or replaced copy is never run.

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug)]
pub struct Embedded {
    pub name: &'static str,
    /// Lowercase hex SHA-256 of the unpacked binary.
    pub sha256: &'static str,
    pub gz: &'static [u8],
}

fn digest(path: &Path) -> Result<String> {
    pt_hash::sha256_file(path)
        .map(|d| pt_hash::to_hex(&d))
        .map_err(|e| Error::new(e.to_string()))
}

impl Embedded {
    /// `<dir>/<name>-<sha256>`, mode 0755. A missing or mismatching file is unpacked again
    /// through the system `gzip -dc` into a temporary sibling, checked, then renamed in place.
    pub fn unpack(&self, dir: &Path) -> Result<PathBuf> {
        let ctx = |e: Error| e.context(format!("unpacking the embedded {}", self.name));
        crate::private_dir(dir).map_err(ctx)?;
        let path = dir.join(format!("{}-{}", self.name, self.sha256));
        if path.is_file() && digest(&path).map_err(ctx)? == self.sha256 {
            return Ok(path);
        }
        let tmp = dir.join(format!(
            ".{}-{}.{}",
            self.name,
            self.sha256,
            std::process::id()
        ));
        self.gunzip(&tmp).map_err(ctx)?;
        let got = digest(&tmp).map_err(ctx)?;
        if got != self.sha256 {
            return Err(ctx(Error::new(format!(
                "{} has SHA-256 {got}, the build recorded {}",
                tmp.display(),
                self.sha256
            ))));
        }
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|e| ctx(Error::new(format!("installing {}: {e}", path.display()))))?;
        Ok(path)
    }

    fn gunzip(&self, to: &Path) -> Result<()> {
        let out =
            File::create(to).map_err(|e| Error::new(format!("creating {}: {e}", to.display())))?;
        let mut child = Command::new("gzip")
            .arg("-dc")
            .stdin(Stdio::piped())
            .stdout(out)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(format!("running gzip -dc: {e}")))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::new("gzip has no stdin"))?;
        let gz = self.gz;
        let writer = thread::spawn(move || stdin.write_all(gz));
        let done = child
            .wait_with_output()
            .map_err(|e| Error::new(format!("waiting for gzip -dc: {e}")))?;
        let wrote = writer
            .join()
            .map_err(|_| Error::new("the gzip writer panicked"))?;
        if !done.status.success() {
            return Err(Error::new(format!(
                "gzip -dc exited {}: {}",
                done.status,
                String::from_utf8_lossy(&done.stderr).trim()
            )));
        }
        wrote.map_err(|e| Error::new(format!("feeding gzip -dc: {e}")))
    }
}
