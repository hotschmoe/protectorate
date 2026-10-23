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

/// The `path=` value of a pax extended header, when it has one.
fn pax_path(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let mut rest: &str = &text;
    let mut found = None;
    while !rest.is_empty() {
        let space = rest.find(' ')?;
        let Ok(length) = rest[..space].parse::<usize>() else {
            break;
        };
        if length == 0 || length > rest.len() {
            break;
        }
        let record = &rest[..length];
        rest = &rest[length..];
        if let Some(value) = record.get(space + 1..)?.strip_prefix("path=") {
            found = Some(value.trim_end_matches('\n').to_string());
        }
    }
    found
}

fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    fn header(name: &str, kind: u8, size: u64) -> [u8; BLOCK] {
        let mut block = [0u8; BLOCK];
        block[..name.len()].copy_from_slice(name.as_bytes());
        let mode = format!("{:07o}\0", 0o644);
        block[100..108].copy_from_slice(mode.as_bytes());
        let size_field = format!("{size:011o}\0");
        block[124..136].copy_from_slice(size_field.as_bytes());
        block[148..156].copy_from_slice(b"        ");
        block[156] = kind;
        block[257..263].copy_from_slice(b"ustar\0");
        block[263..265].copy_from_slice(b"00");
        let sum: u32 = block.iter().map(|b| *b as u32).sum();
        let checksum = format!("{sum:07o}\0");
        block[148..156].copy_from_slice(checksum.as_bytes());
        block
    }

    fn archive(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, kind, data) in entries {
            out.extend_from_slice(&header(name, *kind, data.len() as u64));
            out.extend_from_slice(data);
            out.extend(std::iter::repeat_n(
                0u8,
                (BLOCK - data.len() % BLOCK) % BLOCK,
            ));
        }
        out.extend(std::iter::repeat_n(0u8, 2 * BLOCK));
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&out).unwrap();
        encoder.finish().unwrap()
    }

    fn extract(dir: &Path, entries: &[(&str, u8, &[u8])]) -> Result<PathBuf> {
        let archive_path = dir.join("web.tar.gz");
        fs::write(&archive_path, archive(entries)).unwrap();
        let web = dir.join("web");
        extract_web(&archive_path, &web)?;
        Ok(web)
    }

    #[test]
    fn web_paths_stay_inside_the_root() {
        assert_eq!(
            safe_web_path("./index.html").unwrap(),
            PathBuf::from("index.html")
        );
        assert_eq!(safe_web_path("./").unwrap(), PathBuf::from("."));
        assert_eq!(safe_web_path("a/./b/../c").unwrap(), PathBuf::from("a/c"));
        for bad in [
            "../escape",
            "a/../../escape",
            "..",
            "/etc/passwd",
            "C:/Windows/system.ini",
            "C:relative",
            "assets/CON.txt",
            "assets/COM1",
            "assets/trailing. ",
            "web\\evil",
            "a/..\\b",
        ] {
            let err = safe_web_path(bad).unwrap_err().to_string();
            assert!(err.contains("unsafe web path"), "{bad}: {err}");
        }
    }

    #[test]
    fn extracts_directories_and_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let web = extract(
            dir.path(),
            &[
                ("./", b'5', b""),
                ("./index.html", b'0', b"<html>index</html>"),
                ("./assets/app.css", b'0', b"body{}"),
            ],
        )
        .unwrap();
        assert_eq!(
            fs::read(web.join("index.html")).unwrap(),
            b"<html>index</html>"
        );
        assert_eq!(fs::read(web.join("assets/app.css")).unwrap(), b"body{}");
    }

    #[test]
    fn pax_path_is_taken_from_the_extended_header() {
        let record = "28 path=assets/deep/app.css\n";
        assert_eq!(
            pax_path(record.as_bytes()).as_deref(),
            Some("assets/deep/app.css")
        );
        assert_eq!(pax_path(b"12 other=1\n"), None);
        assert_eq!(pax_path(b"garbage"), None);
    }

    #[test]
    fn header_numbers_accept_octal_and_base_256() {
        assert_eq!(parse_number(b"0000644\0"), Some(0o644));
        assert_eq!(parse_number(b"\0"), Some(0));
        assert_eq!(parse_number(b"        \0"), Some(0));
        assert_eq!(
            parse_number(&[0x80, 0, 0, 0, 0, 0x10, 0, 0]),
            Some(0x10_0000)
        );
        assert_eq!(
            parse_number(&[0x80, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Some(-1)
        );
        assert_eq!(parse_number(b"not octal\0"), None);
    }

    #[test]
    fn extraction_rejects_a_non_gzip_stream() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("web.tar.gz");
        fs::write(&archive_path, b"not gzip at all").unwrap();
        let err = extract_web(&archive_path, &dir.path().join("web"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("web"), "{err}");
    }

    #[test]
    fn extraction_rejects_a_file_truncated_at_a_block_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("web.tar.gz");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&header("index.html", b'0', 12)).unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();
        let err = extract_web(&archive_path, &dir.path().join("web"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("truncated"), "{err}");
    }

    #[test]
    fn extraction_refuses_to_overwrite_an_extracted_file() {
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("index.html"), b"existing").unwrap();
        let archive_path = dir.path().join("web.tar.gz");
        fs::write(
            &archive_path,
            archive(&[("index.html", b'0', b"<html>index</html>")]),
        )
        .unwrap();
        let err = extract_web(&archive_path, &web).unwrap_err().to_string();
        assert!(err.contains("create web file"), "{err}");
    }
}
