use crate::{hex_digest, integrity, invalid, mode, set_mode, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"PTXDIR1\n";
const END: u8 = 0;
const DIRECTORY: u8 = 1;
const REGULAR: u8 = 2;
const MAX_NAME: usize = 4096;

pub(crate) fn write_directory(root: &Path, writer: &mut impl Write) -> Result<u32> {
    writer.write_all(MAGIC)?;
    let mut skipped = 0_u32;
    visit(root, root, writer, &mut skipped)?;
    writer.write_all(&[END])?;
    Ok(skipped)
}

fn visit(root: &Path, dir: &Path, writer: &mut impl Write, skipped: &mut u32) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid("source path escaped root"))?;
        let name = relative
            .to_str()
            .ok_or_else(|| invalid("directory contains a non-UTF-8 name"))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        validate_entry_name(&name)?;
        if meta.is_dir() {
            write_record(writer, DIRECTORY, &name, mode(&meta), 0)?;
            visit(root, &path, writer, skipped)?;
        } else if meta.is_file() {
            write_record(writer, REGULAR, &name, mode(&meta), meta.len())?;
            let file = open_regular_nofollow(&path)?;
            let opened = file.metadata()?;
            if !same_file(&meta, &opened)? {
                return Err(integrity(format!(
                    "source file changed while archiving: {}",
                    path.display()
                )));
            }
            let mut limited = file.take(meta.len());
            let copied = io::copy(&mut limited, writer)?;
            if copied != meta.len() {
                return Err(integrity(format!(
                    "source file shrank while archiving: {}",
                    path.display()
                )));
            }
            let file = limited.into_inner();
            let after = file.metadata()?;
            let at_path = fs::symlink_metadata(&path)?;
            if !same_file(&meta, &after)? || !same_file(&meta, &at_path)? {
                return Err(integrity(format!(
                    "source file changed while archiving: {}",
                    path.display()
                )));
            }
        } else {
            *skipped = skipped
                .checked_add(1)
                .ok_or_else(|| invalid("too many skipped entries"))?;
        }
    }
    Ok(())
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> Result<bool> {
    if a.len() != b.len() || a.modified()? != b.modified()? || !b.is_file() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn open_regular_nofollow(path: &Path) -> Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    Ok(opts.open(path)?)
}

fn write_record(writer: &mut impl Write, kind: u8, name: &str, mode: u32, size: u64) -> Result<()> {
    writer.write_all(&[kind])?;
    writer.write_all(&(name.len() as u32).to_le_bytes())?;
    writer.write_all(name.as_bytes())?;
    writer.write_all(&mode.to_le_bytes())?;
    writer.write_all(&size.to_le_bytes())?;
    Ok(())
}

struct DigestWriter(Sha256);

impl Write for DigestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn directory_digest(root: &Path) -> Result<String> {
    let mut writer = DigestWriter(Sha256::new());
    if write_directory(root, &mut writer)? != 0 {
        return Err(integrity("directory contains unsupported entries"));
    }
    Ok(hex_digest(&writer.0.finalize()))
}

pub(crate) fn extract_directory(reader: &mut fs::File, root: &Path) -> Result<()> {
    reader.seek(SeekFrom::Start(0))?;
    let mut magic = [0_u8; MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("invalid directory archive"));
    }
    // Directory records are pre-order. Delay permissions until their children
    // have been written so restrictive source modes are preserved exactly.
    let mut open_dirs: Vec<(PathBuf, u32)> = Vec::new();
    loop {
        let mut kind = [0_u8];
        reader.read_exact(&mut kind)?;
        if kind[0] == END {
            if reader.read(&mut kind)? != 0 {
                return Err(invalid("directory archive has trailing data"));
            }
            for (path, mode) in open_dirs.into_iter().rev() {
                set_mode(&path, mode)?;
            }
            return Ok(());
        }
        if kind[0] != DIRECTORY && kind[0] != REGULAR {
            return Err(invalid("unsupported directory entry type"));
        }
        let len = read_u32(reader)? as usize;
        if len == 0 || len > MAX_NAME {
            return Err(invalid("directory entry name is too long"));
        }
        let mut bytes = vec![0_u8; len];
        reader.read_exact(&mut bytes)?;
        let name = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("directory entry name is not UTF-8"))?;
        validate_entry_name(name)?;
        let mode = read_u32(reader)?;
        if mode & !0o777 != 0 {
            return Err(invalid("directory entry has invalid permissions"));
        }
        let size = read_u64(reader)?;
        let target = root.join(name);
        while open_dirs
            .last()
            .is_some_and(|(path, _)| !target.starts_with(path))
        {
            let (path, mode) = open_dirs
                .pop()
                .ok_or_else(|| invalid("directory stack underflow"))?;
            set_mode(&path, mode)?;
        }
        let expected_parent = open_dirs.last().map_or(root, |(path, _)| path.as_path());
        if target.parent() != Some(expected_parent) {
            return Err(invalid("directory entry is out of order"));
        }
        if kind[0] == DIRECTORY {
            if size != 0 {
                return Err(invalid("directory entry has data"));
            }
            fs::create_dir(&target)?;
            open_dirs.push((target, mode));
        } else {
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let copied = io::copy(&mut reader.take(size), &mut out)?;
            if copied != size {
                return Err(integrity("directory file is truncated"));
            }
            out.sync_all()?;
            set_mode(&target, mode)?;
        }
    }
}

fn read_u32(reader: &mut fs::File) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn read_u64(reader: &mut fs::File) -> Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Applies portable path rules to each archive entry. Backslashes, drive
/// letters, ADS colons and Windows device names are rejected on every host.
pub fn validate_entry_name(name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > MAX_NAME
        || name.starts_with('/')
        || name.contains(['\\', ':', '\0'])
    {
        return Err(invalid("unsafe directory entry name"));
    }
    let mut path = PathBuf::new();
    for part in name.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.chars().any(|c| c.is_control())
            || windows_device_name(part)
        {
            return Err(invalid("unsafe directory entry name"));
        }
        path.push(part);
    }
    Ok(path)
}

fn windows_device_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}
