//! Verified streaming snapshots in a Rust-native, uncompressed archive format.
//! Restore verifies in a private staging directory before publishing an empty
//! destination. Workspaces, credentials, binaries and transient files are excluded.
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub const FORMAT_VERSION: u32 = 1;
pub const ARCHIVE_EXTENSION: &str = ".pbackup";
const PREFIX: &str = "protectorate-hub-";
const STAMP: &str = "%Y%m%dT%H%M%SZ";
const MAGIC: &[u8] = b"PROTECTORATE-BACKUP\x01\n";
const MAX_HEADER: u32 = 64 << 20;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Cancelled,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "backup: {e}"),
            Self::Json(e) => write!(f, "backup: {e}"),
            Self::Invalid(s) => write!(f, "backup: {s}"),
            Self::Cancelled => write!(f, "backup cancelled"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release)
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub mode: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub created: DateTime<Utc>,
    pub hub_version: String,
    pub home: PathBuf,
    pub files: Vec<ArchivedFile>,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub home: PathBuf,
    pub dir: PathBuf,
    pub version: String,
    pub created_at: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Header {
    File { path: String, size: u64, mode: u32 },
    Manifest { manifest: Manifest },
}
fn invalid<T>(s: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(s.into()))
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
fn private_dir(path: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}
fn private_temp(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn private_subdirs(root: &Dir, path: &Path) -> io::Result<()> {
    let mut builder = cap_std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use cap_std::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    root.create_dir_with(path, &builder)
}
fn root(path: &Path) -> Result<Dir> {
    Ok(Dir::open_ambient_dir(path, ambient_authority())?)
}
pub fn safe_relative(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0'])
        && !name.chars().any(char::is_control)
        && name
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.ends_with(['.', ' ']))
}
fn keep(rel: &str) -> bool {
    let parts: Vec<_> = rel.split('/').collect();
    match parts[0] {
        "audit" | "ca" => true,
        "handoffs" => parts.len() == 3 && parts[2].ends_with(".md"),
        "sleeves" => parts.len() == 3 && parts[2].starts_with("chat.jsonl"),
        _ => false,
    }
}
fn plan(home: &Dir) -> Result<Vec<String>> {
    let mut paths = BTreeSet::new();
    for name in [
        "token",
        "identity.json",
        "sleeves.json",
        "directory.json",
        "mailbox.jsonl",
        "notes.json",
    ] {
        match home.symlink_metadata(name) {
            Ok(m) if m.is_file() => {
                paths.insert(name.into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    fn walk(home: &Dir, path: &Path, paths: &mut BTreeSet<String>) -> Result<()> {
        let entries = match home.read_dir(path) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = path.join(entry.file_name());
            let rel = path.to_string_lossy().replace('\\', "/");
            let metadata = home.symlink_metadata(&path)?;
            if metadata.is_dir() {
                if rel.starts_with("sleeves/") && rel.matches('/').count() >= 2 {
                    continue;
                }
                if rel.starts_with("handoffs/") && rel.matches('/').count() >= 2 {
                    continue;
                }
                walk(home, &path, paths)?;
            } else if metadata.is_file() && keep(&rel) {
                if !safe_relative(&rel) {
                    return invalid("state contains a nonportable archive path");
                }
                paths.insert(rel);
            }
        }
        Ok(())
    }
    for dir in ["audit", "ca", "handoffs", "sleeves"] {
        if home.symlink_metadata(dir).is_ok_and(|m| m.is_symlink()) {
            continue;
        }
        walk(home, Path::new(dir), &mut paths)?;
    }
    Ok(paths.into_iter().collect())
}
fn copy_exact(
    reader: &mut impl Read,
    writer: &mut impl Write,
    size: u64,
    cancel: &Cancellation,
) -> Result<String> {
    let mut left = size;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 << 10];
    while left > 0 {
        cancel.check()?;
        let count = left.min(buf.len() as u64) as usize;
        reader.read_exact(&mut buf[..count])?;
        writer.write_all(&buf[..count])?;
        hash.update(&buf[..count]);
        left -= count as u64;
    }
    Ok(hex(hash.finalize()))
}
struct HashWriter<W> {
    writer: W,
    hash: Sha256,
}
impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(b)?;
        self.hash.update(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
fn header(writer: &mut impl Write, h: &Header) -> Result<()> {
    let bytes = serde_json::to_vec(h)?;
    let size =
        u32::try_from(bytes.len()).map_err(|_| Error::Invalid("manifest too large".into()))?;
    if size > MAX_HEADER {
        return invalid("manifest too large");
    }
    writer.write_all(&size.to_le_bytes())?;
    writer.write_all(&bytes)?;
    Ok(())
}
/// Archives the hub state, including its CA. No credentials from harness
/// homes are included.
pub fn create(options: &Options, cancel: &Cancellation) -> Result<PathBuf> {
    cancel.check()?;
    let home = fs::canonicalize(&options.home)?;
    let home_root = root(&home)?;
    private_dir(&options.dir)?;
    let sources = plan(&home_root)?;
    let created = options
        .created_at
        .unwrap_or_else(|| std::time::SystemTime::now().into());
    let filename = format!("{PREFIX}{}{ARCHIVE_EXTENSION}", created.format(STAMP));
    let final_path = options.dir.join(&filename);
    if final_path.exists() {
        return invalid(format!("{} already exists", final_path.display()));
    }
    let mut part = tempfile::Builder::new()
        .prefix(".backup-")
        .suffix(".part")
        .tempfile_in(&options.dir)?;
    let mut manifest = Manifest {
        format: FORMAT_VERSION,
        created,
        hub_version: options.version.clone(),
        home,
        files: Vec::new(),
    };
    let sum;
    {
        let mut writer = HashWriter {
            writer: part.as_file_mut(),
            hash: Sha256::new(),
        };
        writer.write_all(MAGIC)?;
        for rel in sources {
            cancel.check()?;
            let mut f = match home_root.open(&rel) {
                Ok(f) => f.into_std(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let meta = f.metadata()?;
            if !meta.is_file() {
                return invalid("state file changed to a non-file");
            }
            #[cfg(not(unix))]
            let mode = 0o600;
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                meta.permissions().mode() & 0o777
            };
            let size = meta.len();
            header(
                &mut writer,
                &Header::File {
                    path: rel.clone(),
                    size,
                    mode,
                },
            )?;
            let sha256 = copy_exact(&mut f, &mut writer, size, cancel)?;
            manifest.files.push(ArchivedFile {
                path: rel,
                size,
                sha256,
                mode,
            });
        }
        header(&mut writer, &Header::Manifest { manifest })?;
        writer.flush()?;
        sum = hex(writer.hash.finalize());
    }
    part.as_file().sync_all()?;
    cancel.check()?;
    let mut sidecar = tempfile::NamedTempFile::new_in(&options.dir)?;
    writeln!(sidecar, "{sum}  {filename}")?;
    sidecar.as_file().sync_all()?;
    let sidecar_path = sidecar_path(&final_path);
    sidecar
        .persist_noclobber(&sidecar_path)
        .map_err(|e| e.error)?;
    if let Err(e) = part.persist_noclobber(&final_path) {
        let _ = fs::remove_file(sidecar_path);
        return Err(e.error.into());
    }
    Ok(final_path)
}
fn sidecar_path(archive: &Path) -> PathBuf {
    let mut name = archive.as_os_str().to_owned();
    name.push(".sha256");
    name.into()
}
fn verify_sidecar(archive: &Path, file: &mut File, cancel: &Cancellation) -> Result<()> {
    let raw = match fs::read_to_string(sidecar_path(archive)) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let want = raw.split_whitespace().next().unwrap_or("");
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 << 10];
    loop {
        cancel.check()?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if hex(hash.finalize()) != want {
        return invalid("archive does not match its SHA-256 sidecar");
    }
    file.rewind()?;
    Ok(())
}
/// Reads the archive, reporting a short read anywhere as truncation.
fn scan(file: &mut File, stage: Option<&Dir>, cancel: &Cancellation) -> Result<Manifest> {
    scan_entries(file, stage, cancel).map_err(|error| match error {
        Error::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
            Error::Invalid("archive is truncated or has no manifest".into())
        }
        other => other,
    })
}
fn scan_entries(file: &mut File, stage: Option<&Dir>, cancel: &Cancellation) -> Result<Manifest> {
    file.rewind()?;
    let mut magic = vec![0; MAGIC.len()];
    file.read_exact(&mut magic)?;
    if magic != MAGIC {
        return invalid("unsupported backup archive format");
    }
    let mut seen = BTreeMap::new();
    loop {
        cancel.check()?;
        let mut length = [0u8; 4];
        file.read_exact(&mut length)?;
        let length = u32::from_le_bytes(length);
        if length == 0 || length > MAX_HEADER {
            return invalid("archive header is too large");
        }
        let mut bytes = vec![0; length as usize];
        file.read_exact(&mut bytes)?;
        match serde_json::from_slice::<Header>(&bytes)? {
            Header::File { path, size, mode } => {
                if !safe_relative(&path) || seen.contains_key(&path) || mode & !0o777 != 0 {
                    return invalid("unsafe, duplicate or invalid archive file");
                }
                let sha256 = if let Some(stage) = stage {
                    if let Some(parent) = Path::new(&path).parent() {
                        private_subdirs(stage, parent)?;
                    }
                    let mut options = OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use cap_std::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut out = stage.open_with(&path, &options)?;
                    let digest = copy_exact(file, &mut out, size, cancel)?;
                    out.sync_all()?;
                    #[cfg(unix)]
                    {
                        use cap_std::fs::PermissionsExt;
                        out.set_permissions(cap_std::fs::Permissions::from_mode(mode))?;
                    }
                    digest
                } else {
                    copy_exact(file, &mut io::sink(), size, cancel)?
                };
                seen.insert(
                    path.clone(),
                    ArchivedFile {
                        path,
                        size,
                        sha256,
                        mode,
                    },
                );
            }
            Header::Manifest { manifest } => {
                let mut byte = [0u8; 1];
                if file.read(&mut byte)? != 0 {
                    return invalid("archive contains data after its manifest");
                }
                if manifest.format != FORMAT_VERSION || manifest.files.len() != seen.len() {
                    return invalid("unsupported manifest or file count mismatch");
                }
                for entry in &manifest.files {
                    if seen.remove(&entry.path).as_ref() != Some(entry) {
                        return invalid(format!("{} does not match the manifest", entry.path));
                    }
                }
                if !seen.is_empty() {
                    return invalid("manifest omits archived files");
                }
                return Ok(manifest);
            }
        }
    }
}
/// Verifies the sidecar (when present) and every manifest entry without restoring.
pub fn verify(archive: impl AsRef<Path>, cancel: &Cancellation) -> Result<Manifest> {
    let archive = archive.as_ref();
    let mut f = File::open(archive)?;
    verify_sidecar(archive, &mut f, cancel)?;
    scan(&mut f, None, cancel)
}
fn empty_destination(home: &Path) -> Result<()> {
    match fs::symlink_metadata(home) {
        Ok(meta) if !meta.is_dir() || meta.is_symlink() => {
            invalid("restore destination must be a plain directory")
        }
        Ok(_) => {
            if fs::read_dir(home)?.next().is_some() {
                invalid("restore only into an empty home")
            } else {
                Ok(())
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
/// Extracts only plain relative files into a private staging directory. Failed
/// verification leaves the requested destination untouched and cleans staging.
pub fn restore(
    archive: impl AsRef<Path>,
    home: impl AsRef<Path>,
    cancel: &Cancellation,
) -> Result<Manifest> {
    cancel.check()?;
    let archive = archive.as_ref();
    let home = home.as_ref();
    empty_destination(home)?;
    let parent = home
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    private_dir(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".restore-")
        .tempdir_in(parent)?;
    private_temp(stage.path())?;
    let stage_root = root(stage.path())?;
    let mut file = File::open(archive)?;
    verify_sidecar(archive, &mut file, cancel)?;
    let manifest = scan(&mut file, Some(&stage_root), cancel)?;
    drop(stage_root);
    cancel.check()?;
    empty_destination(home)?;
    #[cfg(windows)]
    if home.exists() {
        fs::remove_dir(home)?;
    }
    fs::rename(stage.path(), home)?;
    let _ = stage.keep();
    Ok(manifest)
}
pub fn latest(dir: impl AsRef<Path>) -> Result<Option<(PathBuf, DateTime<Utc>)>> {
    let dir = dir.as_ref();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut latest = None;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(stamp) = name
            .strip_prefix(PREFIX)
            .and_then(|s| s.strip_suffix(ARCHIVE_EXTENSION))
        else {
            continue;
        };
        let Ok(at) = NaiveDateTime::parse_from_str(stamp, STAMP) else {
            continue;
        };
        let at = at.and_utc();
        if latest.as_ref().is_none_or(|(_, old)| at > *old) {
            latest = Some((entry.path(), at))
        }
    }
    Ok(latest)
}
#[derive(Debug)]
pub enum RunEvent {
    Created(PathBuf),
    Failed(String),
}
pub fn run(
    options: &Options,
    every: Duration,
    cancel: &Cancellation,
    mut notify: impl FnMut(RunEvent),
) -> Result<()> {
    if every.is_zero() {
        return invalid("backup interval must be positive");
    }
    let now: DateTime<Utc> = std::time::SystemTime::now().into();
    let mut wait = latest(&options.dir)?
        .map(|(_, at)| {
            every.saturating_sub(now.signed_duration_since(at).to_std().unwrap_or_default())
        })
        .unwrap_or_default();
    loop {
        let start = Instant::now();
        while start.elapsed() < wait {
            if cancel.is_cancelled() {
                return Ok(());
            }
            std::thread::sleep(
                wait.saturating_sub(start.elapsed())
                    .min(Duration::from_millis(100)),
            );
        }
        if cancel.is_cancelled() {
            return Ok(());
        }
        match create(options, cancel) {
            Ok(path) => notify(RunEvent::Created(path)),
            Err(Error::Cancelled) => return Ok(()),
            Err(e) => notify(RunEvent::Failed(e.to_string())),
        }
        wait = every;
    }
}
