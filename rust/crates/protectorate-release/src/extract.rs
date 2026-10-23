//! Safe, bounded extraction of the release web archive.
//!
//! Mirrors the Go extractor: gzip inside a tar, only directories and regular
//! files, no absolute or escaping paths, a hard cap on unpacked bytes and a
//! required index.html.
use crate::{invalid, Error, Result};
use flate2::read::GzDecoder;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_WEB_BYTES: u64 = 1 << 30;
const MAX_ENTRIES: u64 = 100_000;
const MAX_META_ENTRY: u64 = 8 << 20;
const BLOCK: usize = 512;

/// Unpack `archive` into `dir`, refusing anything a release archive should not
/// contain.
pub fn extract_web(archive: &Path, dir: &Path) -> Result<()> {
    let file = File::open(archive).map_err(|e| invalid(format!("open web archive: {e}")))?;
    let mut tar = TarReader::new(GzDecoder::new(file));
    fs::create_dir_all(dir)?;
    let mut total: u64 = 0;
    while let Some(entry) = tar.next()? {
        let rel = safe_web_path(&entry.name)?;
        let path = dir.join(&rel);
        match entry.kind {
            b'5' => create_dir(&path, 0o755)?,
            b'0' | b'\0' => {
                if rel == Path::new(".") {
                    return Err(invalid(format!(
                        "web archive contains unnamed file entry {:?}",
                        entry.name
                    )));
                }
                total += entry.size;
                if total > MAX_WEB_BYTES {
                    return Err(invalid("web archive exceeds size limit"));
                }
                if let Some(parent) = path.parent() {
                    create_dir(parent, 0o755)?;
                }
                let mut out = create_new(&path, 0o644)
                    .map_err(|e| invalid(format!("create web file: {e}")))?;
                tar.copy_file(entry.size, &mut out)?;
                out.flush()?;
            }
            _ => {
                return Err(invalid(format!(
                    "web archive contains unsupported entry {:?}",
                    entry.name
                )))
            }
        }
    }
    if !dir.join("index.html").is_file() {
        return Err(invalid("web archive has no index.html"));
    }
    Ok(())
}

/// The archive entry name as a path below the extraction root, or an error when
/// it is absolute, escapes the root, or uses a Windows device name.
pub fn safe_web_path(name: &str) -> Result<PathBuf> {
    let bad = || invalid(format!("unsafe web path {name:?}"));
    if name.contains('\\') || name.contains(':') {
        return Err(bad());
    }
    if name.starts_with('/') {
        return Err(bad());
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.pop().is_none() {
                return Err(bad());
            }
            continue;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
        if device
            || part.trim_end_matches(['.', ' ']) != part
            || part
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(bad());
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Ok(PathBuf::from("."));
    }
    Ok(parts.iter().collect())
}

fn create_dir(path: &Path, mode: u32) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

fn create_new(path: &Path, mode: u32) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    opts.open(path)
}

/// Stops a decompression bomb from producing more than `limit` bytes.
struct Limited<R> {
    inner: R,
    used: u64,
    limit: u64,
}

impl<R: Read> Read for Limited<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.used >= self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "web archive exceeds size limit",
            ));
        }
        let room = (self.limit - self.used).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..room])?;
        self.used += n as u64;
        Ok(n)
    }
}

struct Header {
    name: String,
    kind: u8,
    size: u64,
}

struct Entry {
    name: String,
    kind: u8,
    size: u64,
}

struct TarReader<R: Read> {
    inner: Limited<R>,
    pending_name: Option<String>,
    entries: u64,
}

impl<R: Read> TarReader<R> {
    fn new(inner: R) -> Self {
        TarReader {
            inner: Limited {
                inner,
                used: 0,
                limit: MAX_WEB_BYTES + (1 << 20),
            },
            pending_name: None,
            entries: 0,
        }
    }

    fn next(&mut self) -> Result<Option<Entry>> {
        loop {
            let mut block = [0u8; BLOCK];
            if !self.read_block(&mut block)? {
                return Ok(None);
            }
            if block.iter().all(|b| *b == 0) {
                return Ok(None);
            }
            let header = parse_header(&block)?;
            let mut name = header.name;
            if let Some(long) = self.pending_name.take() {
                name = long;
            }
            match header.kind {
                b'L' => {
                    let data = self.read_data(header.size)?;
                    self.pending_name = Some(bytes_to_string(&data));
                    continue;
                }
                b'K' | b'x' | b'X' => {
                    let data = self.read_data(header.size)?;
                    if header.kind == b'x' || header.kind == b'X' {
                        if let Some(path) = pax_path(&data) {
                            self.pending_name = Some(path);
                        }
                    }
                    continue;
                }
                _ => {}
            }
            self.entries += 1;
            if self.entries > MAX_ENTRIES {
                return Err(invalid("web archive has too many entries"));
            }
            return Ok(Some(Entry {
                name,
                kind: header.kind,
                size: header.size,
            }));
        }
    }

    fn read_block(&mut self, block: &mut [u8; BLOCK]) -> Result<bool> {
        read_exact_or_eof(&mut self.inner, block).map_err(|e| {
            if e.kind() == io::ErrorKind::InvalidData {
                Error::Invalid(format!("read web tar: {e}"))
            } else {
                invalid(format!("read web tar: {e}"))
            }
        })
    }

    fn read_data(&mut self, size: u64) -> Result<Vec<u8>> {
        if size > MAX_META_ENTRY {
            return Err(invalid("web archive metadata entry is too large"));
        }
        let mut data = vec![0u8; size as usize];
        if size > 0 {
            read_required(&mut self.inner, &mut data)
                .map_err(|e| invalid(format!("read web tar: {e}")))?;
        }
        self.skip_padding(size)?;
        while data.last() == Some(&0) {
            data.pop();
        }
        Ok(data)
    }

    fn skip_padding(&mut self, size: u64) -> Result<()> {
        let mut left = (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64;
        let mut block = [0u8; BLOCK];
        while left > 0 {
            let n = (left as usize).min(BLOCK);
            read_required(&mut self.inner, &mut block[..n])
                .map_err(|e| invalid(format!("read web tar: {e}")))?;
            left -= n as u64;
        }
        Ok(())
    }

    fn copy_file(&mut self, size: u64, out: &mut File) -> Result<()> {
        let mut left = size;
        let mut buf = [0u8; 16 * 1024];
        while left > 0 {
            let n = (left as usize).min(buf.len());
            read_required(&mut self.inner, &mut buf[..n])
                .map_err(|e| invalid(format!("read web file: {e}")))?;
            out.write_all(&buf[..n])?;
            left -= n as u64;
        }
        self.skip_padding(size)
    }
}

fn read_exact_or_eof<R: Read>(inner: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = inner.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    if filled == 0 {
        return Ok(false);
    }
    if filled != buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "web tar is truncated",
        ));
    }
    Ok(true)
}

fn read_required<R: Read>(inner: &mut R, buf: &mut [u8]) -> io::Result<()> {
    if !read_exact_or_eof(inner, buf)? {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "web tar is truncated",
        ));
    }
    Ok(())
}

fn parse_header(block: &[u8; BLOCK]) -> Result<Header> {
    let sum: i64 = block
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                b' ' as i64
            } else {
                *b as i64
            }
        })
        .sum();
    let stored = parse_number(&block[148..156])
        .filter(|v| *v >= 0)
        .ok_or_else(|| invalid("read web tar: bad header checksum"))?;
    if stored != sum {
        return Err(invalid("read web tar: header checksum mismatch"));
    }
    let size =
        parse_number(&block[124..136]).ok_or_else(|| invalid("read web tar: bad header size"))?;
    if size < 0 {
        return Err(invalid("web archive exceeds size limit"));
    }
    let name = field_text(&block[0..100]);
    let name = if block[257..263].starts_with(b"ustar") {
        let prefix = field_text(&block[345..500]);
        match (prefix.is_empty(), name.is_empty()) {
            (true, _) => name,
            (false, true) => prefix,
            (false, false) => format!("{prefix}/{name}"),
        }
    } else {
        name
    };
    Ok(Header {
        name,
        kind: block[156],
        size: size as u64,
    })
}

fn field_text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn parse_number(field: &[u8]) -> Option<i64> {
    if field.is_empty() {
        return None;
    }
    if field[0] == 0x80 || field[0] == 0xff {
        let rest = &field[1..];
        if rest.is_empty() {
            return None;
        }
        let mut value: i64 = 0;
        for b in rest {
            value = (value << 8) | *b as i64;
        }
        let bits = rest.len() * 8;
        if bits < 64 && (value >> (bits - 1)) & 1 == 1 {
            value -= 1i64 << bits;
        }
        return Some(value);
    }
    let mut start = 0usize;
    let mut end = field.len();
    while start < end && (field[start] == b' ' || field[start] == 0) {
        start += 1;
    }
    while end > start && (field[end - 1] == b' ' || field[end - 1] == 0) {
        end -= 1;
    }
    if start == end {
        return Some(0);
    }
    let text = String::from_utf8_lossy(&field[start..end]);
    i64::from_str_radix(text.trim(), 8).ok()
}

/// The `path=` value of a pax extended header, when it has one. Record
/// lengths count bytes, so the records are split before any UTF-8 decoding.
fn pax_path(data: &[u8]) -> Option<String> {
    let mut rest = data;
    let mut found = None;
    while !rest.is_empty() {
        let space = rest.iter().position(|&b| b == b' ')?;
        let Some(length) = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|digits| digits.parse::<usize>().ok())
        else {
            break;
        };
        if length == 0 || length > rest.len() {
            break;
        }
        let (record, tail) = rest.split_at(length);
        rest = tail;
        if let Some(value) = record.get(space + 1..)?.strip_prefix(b"path=") {
            found = Some(
                String::from_utf8_lossy(value)
                    .trim_end_matches('\n')
                    .to_string(),
            );
        }
    }
    found
}

fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}
