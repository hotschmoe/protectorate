//! The append-only journal the hub's durability rests on.
//!
//! A journal is a directory holding numbered snapshots and segments:
//!
//! ```text
//! lock                       held with an exclusive file lock while a Journal is open
//! snapshot-<seq>             magic, version, u64 LE length, u32 LE CRC-32C, payload
//! segment-<seq>              magic, version, then frames (see `codec`)
//! ```
//!
//! `segment-<seq>` holds the frames written after `snapshot-<seq>` (segment 0 has no snapshot).
//! Only the newest snapshot and its segment are read; older ones are kept on disk.

#![forbid(unsafe_code)]

pub mod codec;
pub mod crc32c;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

pub use codec::{MAX_FRAME, Stop};

const SEGMENT_MAGIC: [u8; 8] = *b"PTJOURNL";
const SNAPSHOT_MAGIC: [u8; 8] = *b"PTSNAPSH";
const VERSION: u32 = 1;
/// Bytes of magic plus version at the start of every segment and snapshot.
pub const HEADER_LEN: usize = 12;
const SNAPSHOT_PREFIX: usize = HEADER_LEN + 8 + 4;

/// Why the tail of the live segment was cut off at open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutReason {
    /// The segment was shorter than its header (a torn creation); the header was rewritten.
    Header,
    /// A frame failed to decode.
    Frame(Stop),
}

/// A torn or corrupt tail removed at open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cut {
    /// File offset the segment was truncated to.
    pub offset: u64,
    /// Bytes removed.
    pub dropped: u64,
    pub reason: CutReason,
}

/// What [`Journal::open`] found on disk.
#[derive(Debug)]
pub struct Recovered {
    /// The newest snapshot, if any.
    pub snapshot: Option<Vec<u8>>,
    /// Payloads appended after that snapshot, in order.
    pub frames: Vec<Vec<u8>>,
    pub cut: Option<Cut>,
}

/// An open journal. One writer at a time, enforced by a lock file.
pub struct Journal {
    dir: PathBuf,
    seq: u64,
    file: File,
    len: u64,
    poisoned: bool,
    _lock: File,
}

impl Journal {
    /// Opens or creates the journal in `dir`, truncating a torn tail of the live segment to its
    /// last good frame. A frame that fails its CRC with an intact frame after it is corruption,
    /// not a torn append: open fails with `InvalidData` and leaves the file as it is.
    pub fn open(dir: &Path) -> io::Result<(Journal, Recovered)> {
        create_dir_durable(dir)?;
        let lock_path = dir.join("lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| ctx(e, "open", &lock_path))?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("{}: journal is open elsewhere", lock_path.display()),
            ),
            fs::TryLockError::Error(e) => ctx(e, "lock", &lock_path),
        })?;

        let (snapshots, segments) = list(dir)?;
        let seq = snapshots.iter().copied().max().unwrap_or(0);
        if let Some(&newer) = segments.iter().find(|&&s| s > seq) {
            return Err(invalid(
                dir,
                format!("segment {newer} has no snapshot (newest snapshot is {seq})"),
            ));
        }
        let snapshot = if snapshots.contains(&seq) {
            Some(read_snapshot(&dir.join(snapshot_name(seq)))?)
        } else {
            None
        };

        let path = dir.join(segment_name(seq));
        let (file, len, frames, cut) = if segments.contains(&seq) {
            open_segment(&path)?
        } else {
            let file = create_segment(dir, &path)?;
            (file, HEADER_LEN as u64, Vec::new(), None)
        };
        let journal = Journal {
            dir: dir.to_path_buf(),
            seq,
            file,
            len,
            poisoned: false,
            _lock: lock,
        };
        Ok((
            journal,
            Recovered {
                snapshot,
                frames,
                cut,
            },
        ))
    }

    /// The live segment's sequence number (the newest snapshot's, or 0).
    pub fn segment(&self) -> u64 {
        self.seq
    }

    /// Appends one frame and fdatasyncs.
    pub fn append(&mut self, payload: &[u8]) -> io::Result<()> {
        self.append_batch(&[payload])
    }

    /// Appends every payload in one write and fdatasyncs once. When this returns `Ok`, all the
    /// frames are durable. After any write or sync error the journal refuses further writes: the
    /// file's state is unknown until it is reopened.
    pub fn append_batch(&mut self, payloads: &[&[u8]]) -> io::Result<()> {
        self.check_poisoned()?;
        if payloads.is_empty() {
            return Ok(());
        }
        let mut buf = Vec::with_capacity(payloads.iter().map(|p| p.len() + 8).sum());
        for (i, p) in payloads.iter().enumerate() {
            codec::encode_frame(p, &mut buf).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("journal frame {i} of {}: {e:?}", payloads.len()),
                )
            })?;
        }
        let path = self.dir.join(segment_name(self.seq));
        let res = self
            .file
            .write_all_at(&buf, self.len)
            .map_err(|e| ctx(e, "append to", &path))
            .and_then(|()| {
                self.file
                    .sync_data()
                    .map_err(|e| ctx(e, "fdatasync", &path))
            });
        match res {
            Ok(()) => {
                self.len += buf.len() as u64;
                Ok(())
            }
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    /// Writes `bytes` as the next snapshot (tmp file, fsync, rename, dir fsync) and starts a new
    /// segment after it. `bytes` must describe the state after every frame appended so far.
    pub fn write_snapshot(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.check_poisoned()?;
        let next = self.seq + 1;
        let tmp = self.dir.join(format!("{}.tmp", snapshot_name(next)));
        let dst = self.dir.join(snapshot_name(next));
        let mut buf = Vec::with_capacity(SNAPSHOT_PREFIX + bytes.len());
        buf.extend_from_slice(&SNAPSHOT_MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(&crc32c::crc32c(bytes).to_le_bytes());
        buf.extend_from_slice(bytes);
        {
            let mut f = File::create(&tmp).map_err(|e| ctx(e, "create", &tmp))?;
            f.write_all(&buf).map_err(|e| ctx(e, "write", &tmp))?;
            f.sync_all().map_err(|e| ctx(e, "fsync", &tmp))?;
        }
        if let Err(e) = fs::rename(&tmp, &dst) {
            return Err(ctx(e, "rename snapshot to", &dst));
        }
        // From here the snapshot may be the newest on disk, so frames written to the old segment
        // would be ignored at the next open: any failure poisons the journal.
        let switched = sync_dir(&self.dir).and_then(|()| {
            let path = self.dir.join(segment_name(next));
            create_segment(&self.dir, &path)
        });
        match switched {
            Ok(file) => {
                self.file = file;
                self.seq = next;
                self.len = HEADER_LEN as u64;
                Ok(())
            }
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    fn check_poisoned(&self) -> io::Result<()> {
        if self.poisoned {
            return Err(io::Error::other(format!(
                "{}: journal refused a write after an earlier write error; reopen it",
                self.dir.display()
            )));
        }
        Ok(())
    }
}

fn header(magic: &[u8; 8]) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..8].copy_from_slice(magic);
    h[8..].copy_from_slice(&VERSION.to_le_bytes());
    h
}

fn segment_name(seq: u64) -> String {
    format!("segment-{seq:020}")
}

fn snapshot_name(seq: u64) -> String {
    format!("snapshot-{seq:020}")
}

fn parse_seq(name: &str, prefix: &str) -> Option<u64> {
    let digits = name.strip_prefix(prefix)?;
    if digits.len() != 20 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn list(dir: &Path) -> io::Result<(Vec<u64>, Vec<u64>)> {
    let mut snapshots = Vec::new();
    let mut segments = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| ctx(e, "read dir", dir))? {
        let entry = entry.map_err(|e| ctx(e, "read dir", dir))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(s) = parse_seq(name, "snapshot-") {
            snapshots.push(s);
        } else if let Some(s) = parse_seq(name, "segment-") {
            segments.push(s);
        }
    }
    Ok((snapshots, segments))
}

fn read_all(path: &Path) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path)
        .and_then(|mut f| f.read_to_end(&mut data))
        .map_err(|e| ctx(e, "read", path))?;
    Ok(data)
}

fn read_snapshot(path: &Path) -> io::Result<Vec<u8>> {
    let data = read_all(path)?;
    if data.len() < SNAPSHOT_PREFIX {
        return Err(invalid(path, "snapshot shorter than its header".into()));
    }
    check_header(path, &data[..HEADER_LEN], &SNAPSHOT_MAGIC)?;
    let len = u64::from_le_bytes(data[12..20].try_into().expect("8 bytes"));
    let crc = u32::from_le_bytes(data[20..24].try_into().expect("4 bytes"));
    let payload = &data[SNAPSHOT_PREFIX..];
    if len != payload.len() as u64 {
        return Err(invalid(
            path,
            format!("snapshot declares {len} bytes but holds {}", payload.len()),
        ));
    }
    if crc32c::crc32c(payload) != crc {
        return Err(invalid(path, "snapshot CRC mismatch".into()));
    }
    Ok(payload.to_vec())
}

fn check_header(path: &Path, got: &[u8], magic: &[u8; 8]) -> io::Result<()> {
    if got[..8] != magic[..] {
        return Err(invalid(
            path,
            "bad magic: not a protectorate journal file".into(),
        ));
    }
    let version = u32::from_le_bytes(got[8..12].try_into().expect("4 bytes"));
    if version != VERSION {
        return Err(invalid(
            path,
            format!("unsupported version {version} (this build reads {VERSION})"),
        ));
    }
    Ok(())
}

type Segment = (File, u64, Vec<Vec<u8>>, Option<Cut>);

fn open_segment(path: &Path) -> io::Result<Segment> {
    let data = read_all(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| ctx(e, "open", path))?;
    let want = header(&SEGMENT_MAGIC);
    if data.len() < HEADER_LEN {
        if data[..] != want[..data.len()] {
            return Err(invalid(
                path,
                "bad magic: not a protectorate journal file".into(),
            ));
        }
        file.set_len(0).map_err(|e| ctx(e, "truncate", path))?;
        file.write_all_at(&want, 0)
            .map_err(|e| ctx(e, "write header to", path))?;
        file.sync_all().map_err(|e| ctx(e, "fsync", path))?;
        let cut = Cut {
            offset: 0,
            dropped: data.len() as u64,
            reason: CutReason::Header,
        };
        return Ok((file, HEADER_LEN as u64, Vec::new(), Some(cut)));
    }
    check_header(path, &data[..HEADER_LEN], &SEGMENT_MAGIC)?;
    let decoded = codec::decode_frames(&data[HEADER_LEN..]);
    let good = (HEADER_LEN + decoded.valid_len) as u64;
    if decoded.stop == Some(Stop::CrcMismatch) {
        // A torn append damages only the last complete frame. A damaged frame with an intact
        // one after it is corruption of committed data: cutting would lose that frame.
        let bad = &data[HEADER_LEN + decoded.valid_len..];
        let end = codec::complete_len(bad).expect("a CRC mismatch is a complete frame");
        if !codec::decode_frames(&bad[end..]).frames.is_empty() {
            return Err(invalid(
                path,
                format!(
                    "frame at offset {good} fails its CRC and intact frames follow it; \
                     the segment is left as it is"
                ),
            ));
        }
    }
    let frames = decoded.frames.iter().map(|f| f.to_vec()).collect();
    let cut = match decoded.stop {
        None => None,
        Some(stop) => {
            file.set_len(good).map_err(|e| ctx(e, "truncate", path))?;
            file.sync_all().map_err(|e| ctx(e, "fsync", path))?;
            Some(Cut {
                offset: good,
                dropped: data.len() as u64 - good,
                reason: CutReason::Frame(stop),
            })
        }
    };
    Ok((file, good, frames, cut))
}

fn create_segment(dir: &Path, path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| ctx(e, "create", path))?;
    file.write_all(&header(&SEGMENT_MAGIC))
        .map_err(|e| ctx(e, "write header to", path))?;
    file.sync_all().map_err(|e| ctx(e, "fsync", path))?;
    sync_dir(dir)?;
    Ok(file)
}

/// Creates `dir` and any missing ancestors, then fsyncs the parent of every directory created
/// and of `dir` itself, so the journal's own entry survives a power loss. The parent is synced
/// even when `dir` exists: an earlier open may have created it and crashed before its sync.
fn create_dir_durable(dir: &Path) -> io::Result<()> {
    let parent = |d: &Path| {
        d.parent()
            .filter(|up| !up.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    };
    let mut dirs = vec![dir];
    while let Some(up) = dirs
        .last()
        .copied()
        .and_then(Path::parent)
        .filter(|up| !up.as_os_str().is_empty() && !up.exists())
    {
        dirs.push(up);
    }
    fs::create_dir_all(dir).map_err(|e| ctx(e, "create journal dir", dir))?;
    for d in dirs {
        sync_dir(&parent(d))?;
    }
    Ok(())
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| ctx(e, "fsync dir", dir))
}

fn ctx(e: io::Error, what: &str, path: &Path) -> io::Error {
    io::Error::new(e.kind(), format!("{what} {}: {e}", path.display()))
}

fn invalid(path: &Path, msg: String) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {msg}", path.display()),
    )
}
