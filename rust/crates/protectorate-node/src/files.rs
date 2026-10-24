use protectorate_fsutil::unix_seconds;
use protectorate_wire::{self as wire, v1, Code, ReflectMessage, RpcError};
#[cfg(any(unix, windows))]
use protectorate_xfer::directory_digest;
use protectorate_xfer::{Chunk, Error as XferError, Kind, Sink, Source, Trailer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    time::{Duration, Instant},
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
    reply: v1::PutFileResponse,
}

struct Completed {
    replay: Replay,
    finished: Instant,
}

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    offset: u64,
    request_hash: [u8; 32],
    /// The PutFileResponse as protobuf JSON.
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
    expires: i64,
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
                        .filter(|(_, receipt)| receipt.expires > unix_seconds())
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

    pub(crate) fn put(
        &self,
        mut request: v1::PutFileRequest,
    ) -> Result<v1::PutFileResponse, RpcError> {
        let request_hash: [u8; 32] = Sha256::digest(wire::encode(&request)?).into();
        let chunk = request.chunk.take();
        let id = request.transfer_id.as_str();
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
        let offset = request.offset;
        let mut map = self
            .puts
            .lock()
            .map_err(|_| err(Code::Internal, "transfer lock poisoned"))?;
        map.completed
            .retain(|_, entry| entry.finished.elapsed() <= COMPLETED_TTL);
        map.durable
            .retain(|_, entry| entry.expires > unix_seconds());
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
                return stored_reply(&entry.reply);
            }
            return Err(err(
                Code::FailedPrecondition,
                "transfer publication outcome is ambiguous",
            ));
        }
        let (put, existing) =
            if offset == 0 {
                let kind = match request.kind() {
                    v1::FileKind::File => Kind::File,
                    v1::FileKind::Directory => Kind::Directory,
                    _ => return Err(err(Code::InvalidArgument, "kind must be file or directory")),
                };
                let dest = self.resolve(&request.path)?;
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
                        if request.mode == 0 {
                            0o600
                        } else {
                            request.mode
                        }
                    };
                    let sink = Sink::new_unknown_size(&dest, kind, mode, request.overwrite)
                        .map_err(|e| match &e {
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
        let is_final = request.last;
        let result = self.apply(&mut p, &request, chunk, id, request_hash);
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
        request: &v1::PutFileRequest,
        chunk: Option<v1::FileChunk>,
        id: &str,
        request_hash: [u8; 32],
    ) -> Result<v1::PutFileResponse, RpcError> {
        let offset = request.offset;
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
        if let Some(chunk) = chunk {
            sink.write(Chunk {
                data: chunk.data,
                compressed: chunk.compressed,
                raw_size: chunk.raw_size,
            })
            .map_err(|e| err(Code::InvalidArgument, e))?;
        }
        let received = sink.received();
        if !request.last {
            return Ok(v1::PutFileResponse {
                received,
                ..Default::default()
            });
        }
        let trailer = request
            .trailer
            .as_ref()
            .ok_or_else(|| err(Code::InvalidArgument, "last chunk needs a trailer"))?;
        let trailer = Trailer {
            size: trailer.size,
            sha256: trailer.sha256.clone(),
            skipped: trailer.skipped,
        };
        let reply = v1::PutFileResponse {
            path: p.dest.to_string_lossy().into_owned(),
            received,
            done: true,
        };
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
                reply: reply_json(&reply)?,
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
                expires: unix_seconds() + COMPLETED_TTL.as_secs() as i64,
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
                    reply: reply_json(&reply)?,
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
                    expires: unix_seconds() + COMPLETED_TTL.as_secs() as i64,
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
        entries.retain(|_, entry| entry.expires > unix_seconds());
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

#[cfg(any(unix, windows))]
fn reply_json(reply: &v1::PutFileResponse) -> Result<Value, RpcError> {
    serde_json::to_value(reply.transcode_to_dynamic()).map_err(|e| err(Code::Internal, e))
}
fn stored_reply(reply: &Value) -> Result<v1::PutFileResponse, RpcError> {
    wire::decode(&serde_json::to_vec(reply).map_err(|e| err(Code::Internal, e))?)
        .map_err(|e| err(Code::Internal, format!("file receipt reply: {}", e.message)))
}
fn err(code: Code, message: impl std::fmt::Display) -> RpcError {
    RpcError::new(code, message.to_string())
}
