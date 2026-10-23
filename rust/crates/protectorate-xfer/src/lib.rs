//! Bounded transfers of files and directory trees. The directory archive is a
//! small Rust-native format, not Go tar: no byte compatibility is required.
//! Sources and sinks each hold at most one 4 MiB raw chunk plus compressor
//! output. Directories spool to disk, so tree size does not grow RAM use.

mod archive;
pub use archive::validate_entry_name;

use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::{
    fmt, fs, io,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tempfile::{NamedTempFile, TempDir};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

pub const CHUNK_SIZE: usize = 4 << 20;
const MAX_PACKED_SIZE: usize = CHUNK_SIZE + (64 << 10);
const MAX_MISSES: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: Kind,
    pub mode: u32,
    /// For a file, the size observed when opened; zero for directories.
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub compressed: bool,
    pub raw_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trailer {
    pub size: u64,
    pub sha256: String,
    pub skipped: u32,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(String),
    Integrity(String),
    Compression(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => err.fmt(f),
            Self::Invalid(message) | Self::Integrity(message) | Self::Compression(message) => {
                f.write_str(message)
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn integrity(message: impl Into<String>) -> Error {
    Error::Integrity(message.into())
}

pub struct Source {
    header: Header,
    reader: fs::File,
    _spool: Option<NamedTempFile>,
    original: Option<(u64, SystemTime)>,
    sum: Sha256,
    size: u64,
    skipped: u32,
    try_compress: bool,
    misses: u8,
    finished: bool,
}

impl Source {
    pub fn open(path: impl AsRef<Path>, compress: bool) -> Result<Self> {
        let path = path.as_ref();
        let meta = fs::metadata(path)?;
        let (header, reader, spool, original, skipped) = if meta.is_file() {
            let reader = fs::File::open(path)?;
            let opened = reader.metadata()?;
            if !same_file_snapshot(&meta, &opened)? {
                return Err(integrity("source file changed while opening"));
            }
            let header = Header {
                kind: Kind::File,
                mode: mode(&opened),
                size: opened.len(),
            };
            (
                header,
                reader,
                None,
                Some((opened.len(), opened.modified()?)),
                0,
            )
        } else if meta.is_dir() {
            let mut spool = NamedTempFile::new()?;
            let skipped = archive::write_directory(path, spool.as_file_mut())?;
            spool.as_file_mut().sync_all()?;
            let reader = spool.reopen()?;
            (
                Header {
                    kind: Kind::Directory,
                    mode: mode(&meta),
                    size: 0,
                },
                reader,
                Some(spool),
                None,
                skipped,
            )
        } else {
            return Err(invalid("source must be a regular file or directory"));
        };
        let skip_compression = header.kind == Kind::File && incompressible(path);
        Ok(Self {
            header,
            reader,
            _spool: spool,
            original,
            sum: Sha256::new(),
            size: 0,
            skipped,
            try_compress: compress && !skip_compression,
            misses: 0,
            finished: false,
        })
    }

    pub fn header(&self) -> Header {
        self.header
    }

    /// Returns `None` at EOF. `trailer` becomes available only then.
    pub fn next_chunk(&mut self) -> Result<Option<Chunk>> {
        if self.finished {
            return Ok(None);
        }
        let mut raw = vec![0_u8; CHUNK_SIZE];
        let mut len = 0;
        while len < CHUNK_SIZE {
            match self.reader.read(&mut raw[len..]) {
                Ok(0) => break,
                Ok(n) => len += n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.into()),
            }
        }
        if len == 0 {
            self.finished = true;
            return Ok(None);
        }
        raw.truncate(len);
        self.sum.update(&raw);
        self.size = self
            .size
            .checked_add(len as u64)
            .ok_or_else(|| invalid("transfer size overflow"))?;
        let mut chunk = Chunk {
            data: raw,
            compressed: false,
            raw_size: len as u32,
        };
        if self.try_compress {
            let packed = zstd::bulk::compress(&chunk.data, 3)
                .map_err(|err| Error::Compression(format!("compress chunk: {err}")))?;
            if packed.len() < len - len / 16 {
                chunk.data = packed;
                chunk.compressed = true;
                self.misses = 0;
            } else {
                self.misses += 1;
                if self.misses >= MAX_MISSES {
                    self.try_compress = false;
                }
            }
        }
        Ok(Some(chunk))
    }

    pub fn trailer(&self) -> Result<Trailer> {
        if !self.finished {
            return Err(invalid("source not fully read"));
        }
        if let Some((size, modified)) = self.original {
            let now = self.reader.metadata()?;
            if self.size != size || now.len() != size || now.modified()? != modified {
                return Err(integrity("source file changed during transfer"));
            }
        }
        Ok(Trailer {
            size: self.size,
            sha256: hex_digest(&self.sum.clone().finalize()),
            skipped: self.skipped,
        })
    }
}

pub struct Sink {
    dest: PathBuf,
    header: Header,
    expected_file_size: Option<u64>,
    overwrite: bool,
    spool: Option<NamedTempFile>,
    sum: Sha256,
    received: u64,
    done: bool,
}

/// A verified file whose inode can be recorded before it is published.
pub struct PreparedFile {
    dest: PathBuf,
    overwrite: bool,
    spool: NamedTempFile,
}

/// A verified extracted directory retained until its publication decision.
pub struct PreparedDirectory {
    dest: PathBuf,
    overwrite: bool,
    stage: TempDir,
}

impl PreparedDirectory {
    pub fn path(&self) -> &Path {
        self.stage.path()
    }

    pub fn publish(self) -> Result<()> {
        check_destination(&self.dest, Kind::Directory, self.overwrite)?;
        publish(self.stage.path(), &self.dest, self.overwrite)?;
        #[cfg(unix)]
        let parent = destination_parent(&self.dest)?;
        drop(self.stage);
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

/// Hash a directory tree using its transfer archive representation.
/// Unsupported entries cause an error rather than being silently skipped.
pub fn directory_digest(path: &Path) -> Result<String> {
    archive::directory_digest(path)
}

impl PreparedFile {
    pub fn path(&self) -> &Path {
        self.spool.path()
    }

    pub fn publish(self) -> Result<()> {
        check_destination(&self.dest, Kind::File, self.overwrite)?;
        publish(self.spool.path(), &self.dest, self.overwrite)?;
        #[cfg(unix)]
        let parent = destination_parent(&self.dest)?;
        drop(self.spool);
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

impl Sink {
    pub fn new(dest: impl AsRef<Path>, header: Header, overwrite: bool) -> Result<Self> {
        Self::create(dest, header, overwrite, Some(header.size))
    }

    /// Accepts protocols that announce file size only in the mandatory trailer.
    pub fn new_unknown_size(
        dest: impl AsRef<Path>,
        kind: Kind,
        mode: u32,
        overwrite: bool,
    ) -> Result<Self> {
        Self::create(
            dest,
            Header {
                kind,
                mode,
                size: 0,
            },
            overwrite,
            None,
        )
    }

    fn create(
        dest: impl AsRef<Path>,
        header: Header,
        overwrite: bool,
        expected_file_size: Option<u64>,
    ) -> Result<Self> {
        let dest = dest.as_ref().to_path_buf();
        validate_destination(&dest)?;
        check_destination(&dest, header.kind, overwrite)?;
        let parent = destination_parent(&dest)?;
        fs::create_dir_all(parent)?;
        let spool = NamedTempFile::new_in(parent)?;
        Ok(Self {
            dest,
            header,
            expected_file_size,
            overwrite,
            spool: Some(spool),
            sum: Sha256::new(),
            received: 0,
            done: false,
        })
    }

    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn kind(&self) -> Kind {
        self.header.kind
    }

    pub fn write(&mut self, chunk: Chunk) -> Result<()> {
        if self.done {
            return Err(invalid("transfer already finished"));
        }
        if chunk.raw_size == 0
            || chunk.raw_size as usize > CHUNK_SIZE
            || chunk.data.len() > MAX_PACKED_SIZE
        {
            return Err(invalid("chunk exceeds 4 MiB raw or packed bound"));
        }
        let raw = if chunk.compressed {
            zstd::bulk::decompress(&chunk.data, chunk.raw_size as usize)
                .map_err(|err| Error::Compression(format!("decompress chunk: {err}")))?
        } else {
            chunk.data
        };
        if raw.len() != chunk.raw_size as usize {
            return Err(integrity("chunk raw size mismatch"));
        }
        let next = self
            .received
            .checked_add(raw.len() as u64)
            .ok_or_else(|| invalid("transfer size overflow"))?;
        if self.header.kind == Kind::File && self.expected_file_size.is_some_and(|size| next > size)
        {
            return Err(integrity("file exceeds advertised size"));
        }
        self.spool
            .as_mut()
            .ok_or_else(|| invalid("sink closed"))?
            .write_all(&raw)?;
        self.sum.update(&raw);
        self.received = next;
        Ok(())
    }

    /// Verifies the trailer and publishes the destination. On failure the
    /// original destination remains untouched. A dropped sink aborts.
    pub fn commit(self, trailer: Trailer) -> Result<()> {
        match self.header.kind {
            Kind::File => self.prepare_file(trailer)?.publish(),
            Kind::Directory => self.prepare_directory(trailer)?.publish(),
        }
    }

    /// Verify and sync file content while retaining its staged inode for a
    /// durable publication intent.
    pub fn prepare_file(mut self, trailer: Trailer) -> Result<PreparedFile> {
        if self.done || self.header.kind != Kind::File {
            return Err(invalid("sink is not an open file"));
        }
        self.done = true;
        if self.received != trailer.size
            || self
                .expected_file_size
                .is_some_and(|size| self.received != size)
        {
            return Err(integrity("transfer size mismatch"));
        }
        if hex_digest(&self.sum.finalize()) != trailer.sha256.to_ascii_lowercase() {
            return Err(integrity("transfer SHA-256 mismatch"));
        }
        let spool = self.spool.take().ok_or_else(|| invalid("sink closed"))?;
        set_mode(spool.path(), self.header.mode)?;
        spool.as_file().sync_all()?;
        check_destination(&self.dest, Kind::File, self.overwrite)?;
        Ok(PreparedFile {
            dest: self.dest,
            overwrite: self.overwrite,
            spool,
        })
    }

    /// Verify and extract a directory while retaining its root inode until
    /// the caller records a publication intent.
    pub fn prepare_directory(mut self, trailer: Trailer) -> Result<PreparedDirectory> {
        if self.done || self.header.kind != Kind::Directory {
            return Err(invalid("sink is not an open directory"));
        }
        self.done = true;
        if self.received != trailer.size {
            return Err(integrity("transfer size mismatch"));
        }
        if hex_digest(&self.sum.finalize()) != trailer.sha256.to_ascii_lowercase() {
            return Err(integrity("transfer SHA-256 mismatch"));
        }
        let mut spool = self.spool.take().ok_or_else(|| invalid("sink closed"))?;
        spool.as_file_mut().sync_all()?;
        check_destination(&self.dest, Kind::Directory, self.overwrite)?;
        let parent = destination_parent(&self.dest)?;
        let stage = TempDir::new_in(parent)?;
        archive::extract_directory(spool.as_file_mut(), stage.path())?;
        set_mode(stage.path(), self.header.mode)?;
        check_destination(&self.dest, Kind::Directory, self.overwrite)?;
        Ok(PreparedDirectory {
            dest: self.dest,
            overwrite: self.overwrite,
            stage,
        })
    }
}

fn validate_destination(dest: &Path) -> Result<()> {
    if dest.as_os_str().is_empty() || dest == Path::new("/") || dest.file_name().is_none() {
        return Err(invalid("invalid destination path"));
    }
    Ok(())
}

fn destination_parent(dest: &Path) -> Result<&Path> {
    let parent = dest
        .parent()
        .ok_or_else(|| invalid("destination has no parent"))?;
    if parent.as_os_str().is_empty() {
        Ok(Path::new("."))
    } else {
        Ok(parent)
    }
}

fn check_destination(dest: &Path, kind: Kind, overwrite: bool) -> Result<()> {
    match fs::symlink_metadata(dest) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
        Ok(_) if !overwrite => Err(invalid("destination exists")),
        Ok(meta) if meta.file_type().is_symlink() || meta.is_dir() != (kind == Kind::Directory) => {
            Err(invalid("destination has a different kind or is a symlink"))
        }
        Ok(_) => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn publish(from: &Path, to: &Path, overwrite: bool) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let exists = match fs::symlink_metadata(to) {
        Ok(_) => true,
        Err(err) if err.kind() == io::ErrorKind::NotFound => false,
        Err(err) => return Err(err.into()),
    };
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())
        .map_err(|_| invalid("NUL in source path"))?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())
        .map_err(|_| invalid("NUL in destination path"))?;
    let flags = if overwrite && exists {
        libc::RENAME_EXCHANGE
    } else {
        libc::RENAME_NOREPLACE
    };
    // SAFETY: both paths are NUL-terminated; renameat2 receives valid flags.
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            flags,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

fn same_file_snapshot(a: &fs::Metadata, b: &fs::Metadata) -> Result<bool> {
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

#[cfg(target_os = "windows")]
fn publish(from: &Path, to: &Path, overwrite: bool) -> Result<()> {
    match fs::symlink_metadata(to) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(fs::rename(from, to)?),
        Err(err) => return Err(err.into()),
        Ok(_) if !overwrite => return Err(invalid("destination exists")),
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            if !fs::symlink_metadata(from)?.is_file() {
                return Err(invalid("source and destination have different kinds"));
            }
            let source = windows_wide_path(from)?;
            let destination = windows_wide_path(to)?;
            // SAFETY: both paths are NUL-terminated and refer to staged paths.
            if unsafe {
                MoveFileExW(
                    source.as_ptr(),
                    destination.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(io::Error::last_os_error().into());
            }
            return Ok(());
        }
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            if !fs::symlink_metadata(from)?.is_dir() {
                return Err(invalid("source and destination have different kinds"));
            }
        }
        Ok(_) => return Err(invalid("destination has a different kind or is a symlink")),
    }
    // Windows cannot rename a new tree over an existing nonempty tree. Move
    // the old target into a private sibling first, and restore it if the new
    // rename fails. This preserves overwrite behavior, though a crash between
    // the two renames may leave the old target in the sibling directory.
    let parent = destination_parent(to)?;
    let backup = TempDir::new_in(parent)?;
    let old = backup.path().join("old");
    fs::rename(to, &old)?;
    if let Err(err) = fs::rename(from, to) {
        if let Err(restore) = fs::rename(&old, to) {
            let preserved = backup.keep().join("old");
            return Err(integrity(format!(
                "publish failed: {err}; restoring previous destination failed: {restore}; previous data is at {}",
                preserved.display()
            )));
        }
        return Err(err.into());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_wide_path(path: &Path) -> Result<Vec<u16>> {
    let parent = fs::canonicalize(destination_parent(path)?)?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid("path has no file name"))?;
    Ok(parent
        .join(name)
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn publish(from: &Path, to: &Path, overwrite: bool) -> Result<()> {
    if to.exists() && !overwrite {
        return Err(invalid("destination exists"));
    }
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(unix)]
fn mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}
#[cfg(not(unix))]
fn mode(_: &fs::Metadata) -> u32 {
    0o644
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))?;
    Ok(())
}
#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> Result<()> {
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}

fn incompressible(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "zip"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "zst"
            | "7z"
            | "rar"
            | "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "heic"
            | "avif"
            | "mp3"
            | "aac"
            | "ogg"
            | "flac"
            | "mp4"
            | "mkv"
            | "mov"
            | "webm"
            | "avi"
            | "pdf"
            | "docx"
            | "xlsx"
            | "pptx"
            | "jar"
            | "apk"
            | "whl"
    )
}
