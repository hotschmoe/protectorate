//! The session-log tail reader: complete lines from a growing file, by byte offset. Nothing is
//! carried in memory between calls; a partial last line is simply not consumed and is read again
//! next time. Pure over a `read_at` closure, so the fuzz target drives it without a file.

use std::fs::File;
use std::io;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;

use pt_journal::crc32c::crc32c;

/// Longer lines are skipped and counted (a log line with an inlined image or a huge tool dump).
pub const MAX_LINE: usize = 8 << 20;
/// Bytes read per call to `read_at`; always more than `MAX_LINE`.
pub const WINDOW: usize = 16 << 20;
/// Bytes before the cursor whose CRC detects a rewritten file.
const CHECK: u64 = 64;

pt_json::record! {
    /// Where the next read starts. `skipping` is set inside an overlong line; `dev`, `ino` and
    /// `check` (CRC-32C of the bytes just before `offset`) detect a replaced or rewritten file.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Cursor {
        pub offset: u64 = 0,
        pub skipping: bool = false,
        pub dev: u64 = 0,
        pub ino: u64 = 0,
        pub check: u32 = 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    /// Bytes of `buf` fully handled; the rest is a partial line to read again.
    pub consumed: usize,
    pub skipping: bool,
    pub overlong: u32,
}

/// Calls `line(start, bytes)` for every complete, non-empty line of `buf` no longer than
/// `max_line` (without its `\n`). A partial tail longer than `max_line` is consumed and the
/// scan continues in skipping mode, so the lines seen never depend on how a file was split
/// into reads, as long as every read is longer than `max_line` or reaches the end.
pub fn scan(
    buf: &[u8],
    skipping: bool,
    max_line: usize,
    mut line: impl FnMut(usize, &[u8]),
) -> Scan {
    let mut pos = 0;
    let mut overlong = 0;
    if skipping {
        match buf.iter().position(|b| *b == b'\n') {
            None => {
                return Scan {
                    consumed: buf.len(),
                    skipping: true,
                    overlong: 0,
                };
            }
            Some(i) => {
                pos = i + 1;
                overlong += 1;
            }
        }
    }
    while let Some(i) = buf[pos..].iter().position(|b| *b == b'\n') {
        let l = &buf[pos..pos + i];
        if l.len() > max_line {
            overlong += 1;
        } else if !l.is_empty() {
            line(pos, l);
        }
        pos += i + 1;
    }
    if buf.len() - pos > max_line {
        return Scan {
            consumed: buf.len(),
            skipping: true,
            overlong,
        };
    }
    Scan {
        consumed: pos,
        skipping: false,
        overlong,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advance {
    pub offset: u64,
    pub skipping: bool,
    pub overlong: u32,
}

/// Reads `[start, end)` through `read_at` in windows and hands every complete line to `line`
/// with its absolute offset. Stops at a partial line at the end. A short read is the end.
pub fn advance(
    mut read_at: impl FnMut(u64, &mut [u8]) -> io::Result<usize>,
    start: u64,
    mut end: u64,
    mut skipping: bool,
    window: usize,
    max_line: usize,
    mut line: impl FnMut(u64, &[u8]),
) -> io::Result<Advance> {
    let window = window.max(max_line + 1);
    let mut buf = Vec::new();
    let mut pos = start;
    let mut overlong = 0;
    while pos < end {
        let want = (end - pos).min(window as u64) as usize;
        buf.resize(want, 0);
        let mut got = 0;
        while got < want {
            match read_at(pos + got as u64, &mut buf[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if got < want {
            buf.truncate(got);
            end = pos + got as u64;
        }
        let s = scan(&buf, skipping, max_line, |o, l| line(pos + o as u64, l));
        skipping = s.skipping;
        overlong += s.overlong;
        let at_end = pos + buf.len() as u64 == end;
        pos += s.consumed as u64;
        if s.consumed < buf.len() && at_end {
            break;
        }
    }
    Ok(Advance {
        offset: pos,
        skipping,
        overlong,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tail {
    pub cursor: Cursor,
    /// The file was replaced, shrunk or rewritten before the cursor; it was read from 0.
    pub reset: bool,
    pub bytes: u64,
    pub overlong: u32,
}

/// A log file opened at a cursor. `reset` is set when the file was replaced, shrunk or
/// rewritten before the cursor; reading then starts again at 0.
pub struct Opened {
    file: File,
    path: std::path::PathBuf,
    pub start: Cursor,
    pub reset: bool,
    pub len: u64,
    dev: u64,
    ino: u64,
}

pub fn open(path: &Path, cursor: &Cursor) -> io::Result<Opened> {
    let ctx = |e: io::Error| io::Error::new(e.kind(), format!("tail {}: {e}", path.display()));
    let file = File::open(path).map_err(ctx)?;
    let meta = file.metadata().map_err(ctx)?;
    let len = meta.len();
    let reset = cursor.offset > 0
        && (cursor.dev != meta.dev()
            || cursor.ino != meta.ino()
            || len < cursor.offset
            || check(&file, cursor.offset).map_err(ctx)? != cursor.check);
    Ok(Opened {
        file,
        path: path.to_owned(),
        start: if reset {
            Cursor::default()
        } else {
            cursor.clone()
        },
        reset,
        len,
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

impl Opened {
    /// Reads the complete lines after the start, up to `limit` bytes into the file.
    pub fn read(self, limit: Option<u64>, line: impl FnMut(u64, &[u8])) -> io::Result<Tail> {
        let path = &self.path;
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("tail {}: {e}", path.display()));
        let start = &self.start;
        let end = limit
            .map_or(self.len, |l| l.min(self.len))
            .max(start.offset);
        let file = &self.file;
        let a = advance(
            |off, buf| file.read_at(buf, off),
            start.offset,
            end,
            start.skipping,
            WINDOW,
            MAX_LINE,
            line,
        )
        .map_err(ctx)?;
        Ok(Tail {
            cursor: Cursor {
                offset: a.offset,
                skipping: a.skipping,
                dev: self.dev,
                ino: self.ino,
                check: check(file, a.offset).map_err(ctx)?,
            },
            reset: self.reset,
            bytes: a.offset - start.offset,
            overlong: a.overlong,
        })
    }
}

fn check(file: &File, offset: u64) -> io::Result<u32> {
    let from = offset.saturating_sub(CHECK);
    let mut buf = vec![0u8; (offset - from) as usize];
    file.read_exact_at(&mut buf, from)?;
    Ok(crc32c(&buf))
}

/// The longest prefix of `s` at most `max` bytes long that ends on a char boundary.
pub fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// The longest suffix of `s` at most `max` bytes long that starts on a char boundary.
pub fn clip_tail(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}
