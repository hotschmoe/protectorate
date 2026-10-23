use base64::{engine::general_purpose::STANDARD, Engine as _};
use protectorate_wire::{Code, RpcError};
#[cfg(any(unix, windows))]
use protectorate_xfer::directory_digest;
use protectorate_xfer::{Chunk, Error as XferError, Kind, Sink, Source, Trailer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(any(unix, windows))]
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::{
    ffi::OsStrExt,
    io::{AsRawHandle, FromRawHandle},
};
use std::{
    collections::HashMap,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(windows)]
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

const MAX_TRANSFERS: usize = 16;
const IDLE: Duration = Duration::from_secs(3600);
const MAX_COMPLETED: usize = 64;
const COMPLETED_TTL: Duration = Duration::from_secs(300);

pub(crate) struct Files {
    root: PathBuf,
    unconfined: bool,
    receipts_path: Option<PathBuf>,
    puts: Mutex<PutState>,
}
struct PutState {
    active: HashMap<String, Arc<Mutex<Put>>>,
    completed: HashMap<String, Completed>,
    durable: HashMap<String, Receipt>,
}
struct Put {
    sink: Option<Sink>,
    last: Instant,
    dest: PathBuf,
    replay: Option<Replay>,
}

#[derive(Clone)]
struct Replay {
    offset: u64,
    request_hash: [u8; 32],
    reply: Value,
}

struct Completed {
    replay: Replay,
    finished: Instant,
}

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    offset: u64,
    request_hash: [u8; 32],
    reply: Value,
    dest: PathBuf,
    dev: u64,
    ino: u64,
    len: u64,
    mode: u32,
    mtime: i64,
    mtime_nsec: i64,
    sha256: String,
    #[serde(default)]
    directory: bool,
    expires: u64,
}

impl Receipt {
    #[cfg(unix)]
    fn same_identity(&self, meta: &fs::Metadata) -> bool {
        (if self.directory {
            meta.is_dir()
        } else {
            meta.is_file()
        }) && meta.dev() == self.dev
            && meta.ino() == self.ino
            && meta.len() == self.len
            && meta.mode() & 0o777 == self.mode
            && meta.mtime() == self.mtime
            && meta.mtime_nsec() == self.mtime_nsec
    }

    fn matches_destination(&self) -> bool {
        #[cfg(unix)]
        {
            let Ok(meta) = fs::symlink_metadata(&self.dest) else {
                return false;
            };
            if !self.same_identity(&meta) {
                return false;
            }
            if self.directory {
                return directory_digest(&self.dest).is_ok_and(|digest| digest == self.sha256)
                    && fs::symlink_metadata(&self.dest)
                        .is_ok_and(|meta| self.same_identity(&meta));
            }
            let Ok(mut file) = fs::File::open(&self.dest) else {
                return false;
            };
            if !file.metadata().is_ok_and(|meta| self.same_identity(&meta)) {
                return false;
            }
            let mut digest = Sha256::new();
            let mut buf = [0_u8; 64 * 1024];
            loop {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => digest.update(&buf[..n]),
                    Err(_) => return false,
                }
            }
            if format!("{:x}", digest.finalize()) != self.sha256 {
                return false;
            }
            file.metadata().is_ok_and(|meta| self.same_identity(&meta))
                && fs::symlink_metadata(&self.dest).is_ok_and(|meta| self.same_identity(&meta))
        }
        #[cfg(not(unix))]
        {
            #[cfg(windows)]
            {
                let right_kind = fs::symlink_metadata(&self.dest).is_ok_and(|meta| {
                    !meta.file_type().is_symlink()
                        && if self.directory {
                            meta.is_dir()
                        } else {
                            meta.is_file()
                        }
                });
                if !right_kind {
                    return false;
                }
                if self.directory {
                    return windows_directory_identity(&self.dest)
                        .is_ok_and(|identity| self.same_windows_identity(identity))
                        && directory_digest(&self.dest).is_ok_and(|digest| digest == self.sha256)
                        && windows_directory_identity(&self.dest)
                            .is_ok_and(|identity| self.same_windows_identity(identity));
                }
                let Ok(mut file) = fs::File::open(&self.dest) else {
                    return false;
                };
                if !self.same_windows_file(&file) {
                    return false;
                }
                let mut digest = Sha256::new();
                let mut buf = [0_u8; 64 * 1024];
                loop {
                    match file.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => digest.update(&buf[..n]),
                        Err(_) => return false,
                    }
                }
                format!("{:x}", digest.finalize()) == self.sha256
                    && self.same_windows_file(&file)
                    && fs::File::open(&self.dest)
                        .is_ok_and(|at_path| self.same_windows_file(&at_path))
            }
            #[cfg(not(windows))]
            {
                false
            }
        }
    }

    #[cfg(windows)]
    fn same_windows_file(&self, file: &fs::File) -> bool {
        windows_file_identity(file).is_ok_and(|identity| self.same_windows_identity(identity))
    }

    #[cfg(windows)]
    fn same_windows_identity(&self, (dev, ino, len, mtime): (u64, u64, u64, i64)) -> bool {
        dev == self.dev && ino == self.ino && len == self.len && mtime == self.mtime
    }
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> io::Result<(u64, u64, u64, i64)> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the file handle is live and info points to writable storage.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("file is a reparse point"));
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    let len = (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow);
    let write_time = (u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
        | u64::from(info.ftLastWriteTime.dwLowDateTime);
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        index,
        len,
        i64::try_from(write_time).map_err(io::Error::other)?,
    ))
}

#[cfg(windows)]
fn windows_directory_identity(path: &Path) -> io::Result<(u64, u64, u64, i64)> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: wide is NUL-terminated and all remaining pointers are null.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned a unique owned handle.
    let file = unsafe { fs::File::from_raw_handle(handle as _) };
    windows_file_identity(&file)
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Files {
    pub(crate) fn new(root: impl AsRef<Path>, unconfined: bool) -> io::Result<Self> {
        Self::open_store(root, unconfined, None)
    }

    pub(crate) fn new_with_receipts(
        root: impl AsRef<Path>,
        unconfined: bool,
        home: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let dir = home.as_ref().join("file-receipts");
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Self::open_store(root, unconfined, Some(dir.join("receipts.json")))
    }

    fn open_store(
        root: impl AsRef<Path>,
        unconfined: bool,
        receipts_path: Option<PathBuf>,
    ) -> io::Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        let durable = if let Some(path) = &receipts_path {
            match fs::read(path) {
                Ok(bytes) if bytes.len() <= 1 << 20 => {
                    let entries: HashMap<String, Receipt> =
                        serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                    if entries.len() > MAX_COMPLETED {
                        return Err(io::Error::other("too many file receipts"));
                    }
                    entries
                        .into_iter()
                        .filter(|(_, receipt)| receipt.expires > now_epoch())
                        .collect()
                }
                Ok(_) => return Err(io::Error::other("file receipts exceed 1 MiB")),
                Err(error) if error.kind() == io::ErrorKind::NotFound => HashMap::new(),
                Err(error) => return Err(error),
            }
        } else {
            HashMap::new()
        };
        Ok(Self {
            root: fs::canonicalize(root)?,
            unconfined,
            receipts_path,
            puts: Mutex::new(PutState {
                active: HashMap::new(),
                completed: HashMap::new(),
                durable,
            }),
        })
    }

    pub(crate) fn resolve(&self, name: &str) -> Result<PathBuf, RpcError> {
        if name.is_empty() || name.contains('\0') {
            return Err(err(Code::InvalidArgument, "path is empty or contains NUL"));
        }
        let input = Path::new(name);
        let joined = if input.is_absolute() {
            input.to_path_buf()
        } else {
            self.root.join(input)
        };
        let mut clean = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                    clean.push(component)
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    clean.pop();
                }
            }
        }
        if self.unconfined {
            return Ok(clean);
        }
        if !clean.starts_with(&self.root) || clean == self.root {
            return Err(err(
                Code::PermissionDenied,
                "path is outside the node file root",
            ));
        }
        let mut probe = clean.as_path();
        loop {
            match fs::canonicalize(probe) {
                Ok(real) => {
                    if !real.starts_with(&self.root) {
                        return Err(err(
                            Code::PermissionDenied,
                            "path resolves outside the node file root",
                        ));
                    }
                    return Ok(clean);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    probe = probe.parent().ok_or_else(|| {
                        err(Code::InvalidArgument, "path has no existing ancestor")
                    })?;
                }
                Err(e) => return Err(err(Code::InvalidArgument, format!("resolve path: {e}"))),
            }
        }
    }

    pub(crate) fn open(&self, path: &str, compress: bool) -> Result<(PathBuf, Source), RpcError> {
        let path = self.resolve(path)?;
        let source = Source::open(&path, compress).map_err(|e| match &e {
            XferError::Io(io) if io.kind() == io::ErrorKind::NotFound => err(Code::NotFound, e),
            _ => err(Code::FailedPrecondition, e),
        })?;
        Ok((path, source))
    }

    pub(crate) fn put(&self, request: &Value) -> Result<Value, RpcError> {
        let id = field(request, "transferId")?;
        if id.len() < 8
            || id.len() > 80
            || !id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(err(
                Code::InvalidArgument,
                "transfer_id must be 8 to 80 characters of [A-Za-z0-9._-]",
            ));
        }
        let offset = number(request, "offset").unwrap_or(0);
        let request_hash: [u8; 32] = Sha256::digest(
            serde_json::to_vec(request).map_err(|error| err(Code::Internal, error))?,
        )
        .into();
        let mut map = self
            .puts
            .lock()
            .map_err(|_| err(Code::Internal, "transfer lock poisoned"))?;
        map.completed
            .retain(|_, entry| entry.finished.elapsed() <= COMPLETED_TTL);
        map.durable.retain(|_, entry| entry.expires > now_epoch());
        if let Some(entry) = map.completed.get(id) {
            if entry.replay.offset == offset && entry.replay.request_hash == request_hash {
                return Ok(entry.replay.reply.clone());
            }
            return Err(err(
                Code::InvalidArgument,
                "completed transfer id cannot be reused with a different request",
            ));
        }
        if let Some(entry) = map
            .durable
            .get(id)
            .filter(|_| !map.active.contains_key(id))
            .cloned()
        {
            drop(map);
            if entry.offset != offset || entry.request_hash != request_hash {
                return Err(err(
                    Code::InvalidArgument,
                    "completed transfer id cannot be reused with a different request",
                ));
            }
            if entry.matches_destination() {
                return Ok(entry.reply.clone());
            }
            return Err(err(
                Code::FailedPrecondition,
                "transfer publication outcome is ambiguous",
            ));
        }
        let (put, existing) = if offset == 0 {
            let kind = match field(request, "kind")? {
                "FILE_KIND_FILE" => Kind::File,
                "FILE_KIND_DIRECTORY" => Kind::Directory,
                _ => return Err(err(Code::InvalidArgument, "kind must be file or directory")),
            };
            let dest = self.resolve(field(request, "path")?)?;
            map.active.retain(|_, transfer| {
                transfer
                    .try_lock()
                    .map_or(true, |p| p.last.elapsed() <= IDLE)
            });
            if let Some(put) = map.active.get(id) {
                (Arc::clone(put), true)
            } else {
                if map.active.len() >= MAX_TRANSFERS {
                    return Err(err(
                        Code::ResourceExhausted,
                        "too many transfers in progress",
                    ));
                }
                let mode = if kind == Kind::Directory {
                    0o700
                } else {
                    let requested = number(request, "mode").unwrap_or(0) as u32;
                    if requested == 0 {
                        0o600
                    } else {
                        requested
                    }
                };
                let overwrite = request
                    .get("overwrite")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let sink =
                    Sink::new_unknown_size(&dest, kind, mode, overwrite).map_err(|e| match &e {
                        XferError::Invalid(message) if message == "destination exists" => {
                            err(Code::AlreadyExists, e)
                        }
                        _ => err(Code::FailedPrecondition, e),
                    })?;
                let put = Arc::new(Mutex::new(Put {
                    sink: Some(sink),
                    last: Instant::now(),
                    dest,
                    replay: None,
                }));
                map.active.insert(id.into(), Arc::clone(&put));
                (put, false)
            }
        } else {
            let put = map
                .active
                .get(id)
                .cloned()
                .ok_or_else(|| err(Code::NotFound, "unknown or expired transfer"))?;
            (put, true)
        };
        drop(map);
        let mut p = put
            .lock()
            .map_err(|_| err(Code::Internal, "transfer lock poisoned"))?;
        if let Some(replay) = &p.replay {
            if replay.offset == offset {
                if replay.request_hash == request_hash {
                    let reply = replay.reply.clone();
                    p.last = Instant::now();
                    return Ok(reply);
                }
                return Err(err(
                    Code::InvalidArgument,
                    "retried chunk differs from accepted chunk",
                ));
            }
        }
        if existing && offset == 0 {
            return Err(err(Code::AlreadyExists, "transfer id is in use"));
        }
        let is_final = request.get("last").and_then(Value::as_bool) == Some(true);
        let result = self.apply(&mut p, request, offset, id, request_hash);
        if let Ok(reply) = &result {
            let replay = Replay {
                offset,
                request_hash,
                reply: reply.clone(),
            };
            p.replay = Some(replay);
        }
        if result.is_err() || is_final {
            let mut map = self.puts.lock().unwrap_or_else(|e| e.into_inner());
            if is_final && result.is_ok() {
                if let Some(replay) = &p.replay {
                    map.completed
                        .retain(|_, entry| entry.finished.elapsed() <= COMPLETED_TTL);
                    if map.completed.len() >= MAX_COMPLETED {
                        if let Some(oldest) = map
                            .completed
                            .iter()
                            .min_by_key(|(_, entry)| entry.finished)
                            .map(|(id, _)| id.clone())
                        {
                            map.completed.remove(&oldest);
                        }
                    }
                    map.completed.insert(
                        id.to_owned(),
                        Completed {
                            replay: replay.clone(),
                            finished: Instant::now(),
                        },
                    );
                }
            }
            map.active.remove(id);
        }
        result
    }

    fn apply(
        &self,
        p: &mut Put,
        request: &Value,
        offset: u64,
        id: &str,
        request_hash: [u8; 32],
    ) -> Result<Value, RpcError> {
        let sink = p
            .sink
            .as_mut()
            .ok_or_else(|| err(Code::NotFound, "transfer already finished"))?;
        if sink.received() != offset {
            return Err(err(
                Code::InvalidArgument,
                format!("offset {offset}, expected {}", sink.received()),
            ));
        }
        p.last = Instant::now();
        if let Some(chunk) = request.get("chunk") {
            let data = STANDARD
                .decode(field(chunk, "data")?)
                .map_err(|e| err(Code::InvalidArgument, e))?;
            let raw_size = number(chunk, "rawSize").unwrap_or(0);
            let raw_size = u32::try_from(raw_size)
                .map_err(|_| err(Code::InvalidArgument, "rawSize is too large"))?;
            let compressed = chunk
                .get("compressed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            sink.write(Chunk {
                data,
                compressed,
                raw_size,
            })
            .map_err(|e| err(Code::InvalidArgument, e))?;
        }
        let received = sink.received();
        if request.get("last").and_then(Value::as_bool) != Some(true) {
            return Ok(json!({"received": received.to_string()}));
        }
        let trailer = request
            .get("trailer")
            .ok_or_else(|| err(Code::InvalidArgument, "last chunk needs a trailer"))?;
        let trailer = Trailer {
            size: number(trailer, "size").unwrap_or(0),
            sha256: field(trailer, "sha256")?.into(),
            skipped: number(trailer, "skipped").unwrap_or(0) as u32,
        };
        let reply = json!({"path": p.dest.to_string_lossy(), "received": received.to_string(), "done": true});
        let sink = p
            .sink
            .take()
            .ok_or_else(|| err(Code::Internal, "missing sink"))?;
        #[cfg(any(unix, windows))]
        if self.receipts_path.is_some() && sink.kind() == Kind::File {
            let prepared = sink
                .prepare_file(trailer.clone())
                .map_err(|e| err(Code::DataLoss, e))?;
            #[cfg(unix)]
            let meta = fs::metadata(prepared.path()).map_err(|e| err(Code::Internal, e))?;
            #[cfg(windows)]
            let (dev, ino, len, mtime) = {
                let file = fs::File::open(prepared.path()).map_err(|e| err(Code::Internal, e))?;
                windows_file_identity(&file).map_err(|e| err(Code::Internal, e))?
            };
            let receipt = Receipt {
                offset,
                request_hash,
                reply: reply.clone(),
                dest: p.dest.clone(),
                #[cfg(unix)]
                dev: meta.dev(),
                #[cfg(windows)]
                dev,
                #[cfg(unix)]
                ino: meta.ino(),
                #[cfg(windows)]
                ino,
                #[cfg(unix)]
                len: meta.len(),
                #[cfg(windows)]
                len,
                #[cfg(unix)]
                mode: meta.mode() & 0o777,
                #[cfg(windows)]
                mode: 0,
                #[cfg(unix)]
                mtime: meta.mtime(),
                #[cfg(windows)]
                mtime,
                #[cfg(unix)]
                mtime_nsec: meta.mtime_nsec(),
                #[cfg(windows)]
                mtime_nsec: 0,
                sha256: trailer.sha256.to_ascii_lowercase(),
                directory: false,
                expires: now_epoch() + COMPLETED_TTL.as_secs(),
            };
            self.record_receipt(id, receipt)?;
            prepared.publish().map_err(|e| err(Code::DataLoss, e))?;
            return Ok(reply);
        }
        #[cfg(any(unix, windows))]
        if self.receipts_path.is_some() && sink.kind() == Kind::Directory {
            let prepared = sink
                .prepare_directory(trailer)
                .map_err(|e| err(Code::DataLoss, e))?;
            let digest = match directory_digest(prepared.path()) {
                Ok(digest) => Some(digest),
                Err(XferError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied => {
                    None
                }
                Err(error) => return Err(err(Code::DataLoss, error)),
            };
            if let Some(digest) = digest {
                #[cfg(unix)]
                let meta =
                    fs::symlink_metadata(prepared.path()).map_err(|e| err(Code::Internal, e))?;
                #[cfg(windows)]
                let (dev, ino, len, mtime) = windows_directory_identity(prepared.path())
                    .map_err(|e| err(Code::Internal, e))?;
                let receipt = Receipt {
                    offset,
                    request_hash,
                    reply: reply.clone(),
                    dest: p.dest.clone(),
                    #[cfg(unix)]
                    dev: meta.dev(),
                    #[cfg(windows)]
                    dev,
                    #[cfg(unix)]
                    ino: meta.ino(),
                    #[cfg(windows)]
                    ino,
                    #[cfg(unix)]
                    len: meta.len(),
                    #[cfg(windows)]
                    len,
                    #[cfg(unix)]
                    mode: meta.mode() & 0o777,
                    #[cfg(windows)]
                    mode: 0,
                    #[cfg(unix)]
                    mtime: meta.mtime(),
                    #[cfg(windows)]
                    mtime,
                    #[cfg(unix)]
                    mtime_nsec: meta.mtime_nsec(),
                    #[cfg(windows)]
                    mtime_nsec: 0,
                    sha256: digest,
                    directory: true,
                    expires: now_epoch() + COMPLETED_TTL.as_secs(),
                };
                self.record_receipt(id, receipt)?;
            }
            prepared.publish().map_err(|e| err(Code::DataLoss, e))?;
            return Ok(reply);
        }
        sink.commit(trailer).map_err(|e| err(Code::DataLoss, e))?;
        Ok(reply)
    }

    #[cfg(any(unix, windows))]
    fn record_receipt(&self, id: &str, receipt: Receipt) -> Result<(), RpcError> {
        let path = self
            .receipts_path
            .as_ref()
            .ok_or_else(|| err(Code::Internal, "file receipt store missing"))?;
        let mut map = self
            .puts
            .lock()
            .map_err(|_| err(Code::Internal, "transfer lock poisoned"))?;
        let mut entries = map.durable.clone();
        entries.retain(|_, entry| entry.expires > now_epoch());
        if entries.len() >= MAX_COMPLETED {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.expires)
                .map(|(id, _)| id.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(id.to_owned(), receipt);
        let bytes = serde_json::to_vec(&entries).map_err(|e| err(Code::Internal, e))?;
        if bytes.len() > 1 << 20 {
            return Err(err(
                Code::ResourceExhausted,
                "file receipt store exceeds 1 MiB",
            ));
        }
        protectorate_fsutil::write_file(path, &bytes, 0o600).map_err(|e| err(Code::Internal, e))?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| err(Code::Internal, e))?;
        }
        map.durable = entries;
        Ok(())
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| err(Code::InvalidArgument, format!("missing {key}")))
}
fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}
fn err(code: Code, message: impl std::fmt::Display) -> RpcError {
    RpcError::new(code, message.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn completed_file_replays_after_store_reopen_without_republishing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        fs::write(root.join("output"), b"old").unwrap();
        let old = fs::metadata(root.join("output")).unwrap();
        let first = json!({"transferId":"transfer-123", "path":"output", "kind":"FILE_KIND_FILE", "overwrite":true, "offset":"0", "chunk":{"data":"eA==","rawSize":1}});
        files.put(&first).unwrap();
        let final_request = json!({"transferId":"transfer-123", "offset":"1", "last":true, "trailer":{"size":"1", "sha256":"2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"}});
        let reply = files.put(&final_request).unwrap();
        let dest = root.join("output");
        let before = fs::metadata(&dest).unwrap();
        assert_ne!((old.dev(), old.ino()), (before.dev(), before.ino()));
        drop(files);

        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(reopened.put(&final_request).unwrap(), reply);
        let after = fs::metadata(&dest).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(&dest).unwrap(), b"x");

        let mut altered = final_request.clone();
        altered["trailer"]["sha256"] = json!("00".repeat(32));
        assert_eq!(
            reopened.put(&altered).unwrap_err().code,
            Code::InvalidArgument
        );
        altered = final_request;
        altered["transferId"] = json!("transfer-456");
        assert_eq!(reopened.put(&altered).unwrap_err().code, Code::NotFound);
        fs::write(&dest, b"y").unwrap();
        let original_final = json!({"transferId":"transfer-123", "offset":"1", "last":true, "trailer":{"size":"1", "sha256":"2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"}});
        assert_eq!(
            reopened.put(&original_final).unwrap_err().code,
            Code::FailedPrecondition
        );
    }

    #[test]
    fn intent_without_publication_never_replays_success() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::write(&dest, b"x").unwrap();
        let original = fs::metadata(&dest).unwrap();
        let mut sink = Sink::new_unknown_size(&dest, Kind::File, 0o600, true).unwrap();
        sink.write(Chunk {
            data: b"x".to_vec(),
            compressed: false,
            raw_size: 1,
        })
        .unwrap();
        let sha = "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881";
        let prepared = sink
            .prepare_file(Trailer {
                size: 1,
                sha256: sha.into(),
                skipped: 0,
            })
            .unwrap();
        let meta = fs::metadata(prepared.path()).unwrap();
        let request = json!({"transferId":"transfer-123", "offset":"1", "last":true, "trailer":{"size":"1", "sha256":sha}});
        let hash = Sha256::digest(serde_json::to_vec(&request).unwrap());
        files
            .record_receipt(
                "transfer-123",
                Receipt {
                    offset: 1,
                    request_hash: hash.into(),
                    reply: json!({"done":true}),
                    dest,
                    dev: meta.dev(),
                    ino: meta.ino(),
                    len: meta.len(),
                    mode: meta.mode() & 0o777,
                    mtime: meta.mtime(),
                    mtime_nsec: meta.mtime_nsec(),
                    sha256: sha.into(),
                    directory: false,
                    expires: now_epoch() + 300,
                },
            )
            .unwrap();
        drop(prepared);
        drop(files);
        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(
            reopened.put(&request).unwrap_err().code,
            Code::FailedPrecondition
        );
        let after = fs::metadata(root.join("output")).unwrap();
        assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
    }

    #[test]
    fn completed_directory_replays_after_store_reopen_only_while_tree_matches() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let source_dir = temp.path().join("source");
        fs::create_dir_all(source_dir.join("nested")).unwrap();
        fs::write(source_dir.join("nested/item"), b"new").unwrap();
        let mut source = Source::open(&source_dir, false).unwrap();
        let chunk = source.next_chunk().unwrap().unwrap();
        assert!(source.next_chunk().unwrap().is_none());
        let trailer = source.trailer().unwrap();

        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old"), b"old").unwrap();
        let old = fs::metadata(&dest).unwrap();
        let first = json!({
            "transferId":"directory-123", "path":"output", "kind":"FILE_KIND_DIRECTORY",
            "overwrite":true, "offset":"0",
            "chunk":{"data":STANDARD.encode(&chunk.data),"rawSize":chunk.raw_size}
        });
        files.put(&first).unwrap();
        let final_request = json!({
            "transferId":"directory-123", "offset":chunk.raw_size.to_string(), "last":true,
            "trailer":{"size":trailer.size.to_string(), "sha256":trailer.sha256}
        });
        let reply = files.put(&final_request).unwrap();
        let published = fs::metadata(&dest).unwrap();
        assert_ne!((old.dev(), old.ino()), (published.dev(), published.ino()));
        assert_eq!(fs::read(dest.join("nested/item")).unwrap(), b"new");
        drop(files);

        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(reopened.put(&final_request).unwrap(), reply);
        let replayed = fs::metadata(&dest).unwrap();
        assert_eq!(
            (published.dev(), published.ino()),
            (replayed.dev(), replayed.ino())
        );
        fs::write(dest.join("nested/item"), b"bad").unwrap();
        assert_eq!(
            reopened.put(&final_request).unwrap_err().code,
            Code::FailedPrecondition
        );
    }

    #[test]
    fn directory_intent_before_publication_never_acknowledges_old_tree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("item"), b"new").unwrap();
        let mut source = Source::open(&source_dir, false).unwrap();
        let chunk = source.next_chunk().unwrap().unwrap();
        assert!(source.next_chunk().unwrap().is_none());
        let trailer = source.trailer().unwrap();

        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old"), b"old").unwrap();
        let original = fs::metadata(&dest).unwrap();
        let mut sink = Sink::new_unknown_size(&dest, Kind::Directory, 0o700, true).unwrap();
        sink.write(chunk.clone()).unwrap();
        let prepared = sink.prepare_directory(trailer.clone()).unwrap();
        let meta = fs::metadata(prepared.path()).unwrap();
        let request = json!({
            "transferId":"directory-123", "offset":chunk.raw_size.to_string(), "last":true,
            "trailer":{"size":trailer.size.to_string(), "sha256":trailer.sha256}
        });
        let hash = Sha256::digest(serde_json::to_vec(&request).unwrap());
        files
            .record_receipt(
                "directory-123",
                Receipt {
                    offset: u64::from(chunk.raw_size),
                    request_hash: hash.into(),
                    reply: json!({"done":true}),
                    dest: dest.clone(),
                    dev: meta.dev(),
                    ino: meta.ino(),
                    len: meta.len(),
                    mode: meta.mode() & 0o777,
                    mtime: meta.mtime(),
                    mtime_nsec: meta.mtime_nsec(),
                    sha256: directory_digest(prepared.path()).unwrap(),
                    directory: true,
                    expires: now_epoch() + 300,
                },
            )
            .unwrap();
        drop(prepared);
        drop(files);
        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(
            reopened.put(&request).unwrap_err().code,
            Code::FailedPrecondition
        );
        let after = fs::metadata(&dest).unwrap();
        assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(dest.join("old")).unwrap(), b"old");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn completed_file_replays_after_store_reopen_without_republishing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::write(&dest, b"old").unwrap();
        let old = windows_file_identity(&fs::File::open(&dest).unwrap()).unwrap();
        let first = json!({
            "transferId":"transfer-123", "path":"output", "kind":"FILE_KIND_FILE",
            "overwrite":true, "offset":"0", "chunk":{"data":"eA==","rawSize":1}
        });
        files.put(&first).unwrap();
        let final_request = json!({
            "transferId":"transfer-123", "offset":"1", "last":true,
            "trailer":{"size":"1", "sha256":"2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"}
        });
        let reply = files.put(&final_request).unwrap();
        let published = windows_file_identity(&fs::File::open(&dest).unwrap()).unwrap();
        assert_ne!((old.0, old.1), (published.0, published.1));
        drop(files);

        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(reopened.put(&final_request).unwrap(), reply);
        let replayed = windows_file_identity(&fs::File::open(&dest).unwrap()).unwrap();
        assert_eq!((published.0, published.1), (replayed.0, replayed.1));
        assert_eq!(fs::read(&dest).unwrap(), b"x");
        let mut second_first = first.clone();
        second_first["transferId"] = json!("transfer-456");
        second_first["path"] = json!("second");
        second_first["overwrite"] = json!(false);
        reopened.put(&second_first).unwrap();
        let mut second_final = final_request.clone();
        second_final["transferId"] = json!("transfer-456");
        let second_reply = reopened.put(&second_final).unwrap();
        drop(reopened);
        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(reopened.put(&final_request).unwrap(), reply);
        assert_eq!(reopened.put(&second_final).unwrap(), second_reply);
        let mut altered = final_request.clone();
        altered["trailer"]["sha256"] = json!("00".repeat(32));
        assert_eq!(
            reopened.put(&altered).unwrap_err().code,
            Code::InvalidArgument
        );
        fs::write(&dest, b"y").unwrap();
        assert_eq!(
            reopened.put(&final_request).unwrap_err().code,
            Code::FailedPrecondition
        );
    }

    #[test]
    fn intent_before_publication_never_acknowledges_old_file() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::write(&dest, b"old").unwrap();
        let original = windows_file_identity(&fs::File::open(&dest).unwrap()).unwrap();
        let mut sink = Sink::new_unknown_size(&dest, Kind::File, 0o600, true).unwrap();
        sink.write(Chunk {
            data: b"x".to_vec(),
            compressed: false,
            raw_size: 1,
        })
        .unwrap();
        let sha = "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881";
        let prepared = sink
            .prepare_file(Trailer {
                size: 1,
                sha256: sha.into(),
                skipped: 0,
            })
            .unwrap();
        let (dev, ino, len, mtime) =
            windows_file_identity(&fs::File::open(prepared.path()).unwrap()).unwrap();
        let request = json!({
            "transferId":"transfer-123", "offset":"1", "last":true,
            "trailer":{"size":"1", "sha256":sha}
        });
        let hash = Sha256::digest(serde_json::to_vec(&request).unwrap());
        files
            .record_receipt(
                "transfer-123",
                Receipt {
                    offset: 1,
                    request_hash: hash.into(),
                    reply: json!({"done":true}),
                    dest: dest.clone(),
                    dev,
                    ino,
                    len,
                    mode: 0,
                    mtime,
                    mtime_nsec: 0,
                    sha256: sha.into(),
                    directory: false,
                    expires: now_epoch() + 300,
                },
            )
            .unwrap();
        drop(prepared);
        drop(files);

        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(
            reopened.put(&request).unwrap_err().code,
            Code::FailedPrecondition
        );
        let after = windows_file_identity(&fs::File::open(&dest).unwrap()).unwrap();
        assert_eq!((original.0, original.1), (after.0, after.1));
        assert_eq!(fs::read(&dest).unwrap(), b"old");
    }

    #[test]
    fn completed_directory_replays_after_store_reopen_only_while_tree_matches() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let source_dir = temp.path().join("source");
        fs::create_dir_all(source_dir.join("nested")).unwrap();
        fs::write(source_dir.join("nested/item"), b"new").unwrap();
        let mut source = Source::open(&source_dir, false).unwrap();
        let chunk = source.next_chunk().unwrap().unwrap();
        assert!(source.next_chunk().unwrap().is_none());
        let trailer = source.trailer().unwrap();

        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old"), b"old").unwrap();
        let old = windows_directory_identity(&dest).unwrap();
        let first = json!({
            "transferId":"directory-123", "path":"output", "kind":"FILE_KIND_DIRECTORY",
            "overwrite":true, "offset":"0",
            "chunk":{"data":STANDARD.encode(&chunk.data),"rawSize":chunk.raw_size}
        });
        files.put(&first).unwrap();
        let final_request = json!({
            "transferId":"directory-123", "offset":chunk.raw_size.to_string(), "last":true,
            "trailer":{"size":trailer.size.to_string(), "sha256":trailer.sha256}
        });
        let reply = files.put(&final_request).unwrap();
        let published = windows_directory_identity(&dest).unwrap();
        assert_ne!((old.0, old.1), (published.0, published.1));
        drop(files);

        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(reopened.put(&final_request).unwrap(), reply);
        assert_eq!(fs::read(dest.join("nested/item")).unwrap(), b"new");
        fs::write(dest.join("nested/item"), b"bad").unwrap();
        assert_eq!(
            reopened.put(&final_request).unwrap_err().code,
            Code::FailedPrecondition
        );
    }

    #[test]
    fn directory_intent_before_publication_never_acknowledges_old_tree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("files");
        let home = temp.path().join("state");
        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("item"), b"new").unwrap();
        let mut source = Source::open(&source_dir, false).unwrap();
        let chunk = source.next_chunk().unwrap().unwrap();
        assert!(source.next_chunk().unwrap().is_none());
        let trailer = source.trailer().unwrap();

        let files = Files::new_with_receipts(&root, false, &home).unwrap();
        let dest = root.join("output");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old"), b"old").unwrap();
        let original = windows_directory_identity(&dest).unwrap();
        let mut sink = Sink::new_unknown_size(&dest, Kind::Directory, 0o700, true).unwrap();
        sink.write(chunk.clone()).unwrap();
        let prepared = sink.prepare_directory(trailer.clone()).unwrap();
        let (dev, ino, len, mtime) = windows_directory_identity(prepared.path()).unwrap();
        let request = json!({
            "transferId":"directory-123", "offset":chunk.raw_size.to_string(), "last":true,
            "trailer":{"size":trailer.size.to_string(), "sha256":trailer.sha256}
        });
        let hash = Sha256::digest(serde_json::to_vec(&request).unwrap());
        files
            .record_receipt(
                "directory-123",
                Receipt {
                    offset: u64::from(chunk.raw_size),
                    request_hash: hash.into(),
                    reply: json!({"done":true}),
                    dest: dest.clone(),
                    dev,
                    ino,
                    len,
                    mode: 0,
                    mtime,
                    mtime_nsec: 0,
                    sha256: directory_digest(prepared.path()).unwrap(),
                    directory: true,
                    expires: now_epoch() + 300,
                },
            )
            .unwrap();
        drop(prepared);
        drop(files);
        let reopened = Files::new_with_receipts(&root, false, &home).unwrap();
        assert_eq!(
            reopened.put(&request).unwrap_err().code,
            Code::FailedPrecondition
        );
        let after = windows_directory_identity(&dest).unwrap();
        assert_eq!((original.0, original.1), (after.0, after.1));
        assert_eq!(fs::read(dest.join("old")).unwrap(), b"old");
    }
}
