//! Sockets, the HTTP/1.1 message grammar and body framing, shared by server and client.

use crate::{Headers, Request};
use std::io::{self, BufRead, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub(crate) enum Stream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

macro_rules! each {
    ($stream:expr, $s:ident => $e:expr) => {
        match $stream {
            Stream::Tcp($s) => $e,
            Stream::Unix($s) => $e,
        }
    };
}

impl Stream {
    pub(crate) fn try_clone(&self) -> io::Result<Stream> {
        Ok(match self {
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
        })
    }

    pub(crate) fn set_read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        each!(self, s => s.set_read_timeout(t))
    }

    pub(crate) fn set_write_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        each!(self, s => s.set_write_timeout(t))
    }

    pub(crate) fn shutdown(&self, how: Shutdown) {
        let _ = each!(self, s => s.shutdown(how));
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        each!(self, s => s.read(buf))
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        each!(self, s => s.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A stream whose reads and writes share one deadline: each gets only the time left, so a peer
/// trickling a byte at a time cannot stretch a timeout. Without a deadline, the socket's own
/// timeouts apply.
pub(crate) struct Timed {
    pub(crate) stream: Stream,
    pub(crate) deadline: Option<Instant>,
}

impl Timed {
    fn left(&self) -> io::Result<Option<Duration>> {
        let Some(deadline) = self.deadline else {
            return Ok(None);
        };
        match deadline.saturating_duration_since(Instant::now()) {
            left if left.is_zero() => Err(io::ErrorKind::TimedOut.into()),
            left => Ok(Some(left)),
        }
    }
}

impl Read for Timed {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(left) = self.left()? {
            self.stream.set_read_timeout(Some(left))?;
        }
        self.stream.read(buf)
    }
}

impl Write for Timed {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(left) = self.left()? {
            self.stream.set_write_timeout(Some(left))?;
        }
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// A malformed or refused message: the status to answer with and why.
#[derive(Debug)]
pub(crate) enum Error {
    Status(u16, &'static str),
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        if is_timeout(&e) {
            Error::Status(408, "request timeout")
        } else {
            Error::Io(e)
        }
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> io::Error {
        match e {
            Error::Status(408, why) => io::Error::new(io::ErrorKind::TimedOut, why),
            Error::Status(status, why) => {
                io::Error::new(io::ErrorKind::InvalidData, format!("http {status}: {why}"))
            }
            Error::Io(e) => e,
        }
    }
}

pub(crate) struct Head {
    pub(crate) start: String,
    pub(crate) headers: Headers,
}

const MAX_HEADERS: usize = 100;

/// Reads a line of at most `limit` bytes including its LF. False at EOF; `line` holds any partial.
fn read_line(
    r: &mut impl BufRead,
    line: &mut Vec<u8>,
    limit: usize,
    too_long: Error,
) -> Result<bool, Error> {
    loop {
        let buf = match r.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        if buf.is_empty() {
            return Ok(false);
        }
        let (n, done) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        if line.len() + n > limit {
            return Err(too_long);
        }
        line.extend_from_slice(&buf[..n]);
        r.consume(n);
        if done {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(true);
        }
    }
}

/// Reads a start line and header fields within `limit` bytes. `None` on EOF before the first byte.
pub(crate) fn read_head(r: &mut impl BufRead, limit: usize) -> Result<Option<Head>, Error> {
    let too_long = || Error::Status(431, "request header fields too large");
    let mut used = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        if !read_line(r, &mut line, limit.saturating_sub(used), too_long())? {
            return if used == 0 && line.is_empty() {
                Ok(None)
            } else {
                Err(Error::Status(400, "truncated head"))
            };
        }
        used += line.len() + 2;
        if !line.is_empty() {
            break;
        }
    }
    let start = field_text(&line)?;
    let mut headers = Headers::default();
    loop {
        line.clear();
        if !read_line(r, &mut line, limit.saturating_sub(used), too_long())? {
            return Err(Error::Status(400, "truncated head"));
        }
        used += line.len() + 2;
        if line.is_empty() {
            return Ok(Some(Head { start, headers }));
        }
        if headers.0.len() == MAX_HEADERS {
            return Err(too_long());
        }
        let colon = line
            .iter()
            .position(|&b| b == b':')
            .ok_or(Error::Status(400, "header without colon"))?;
        let name = std::str::from_utf8(&line[..colon])
            .ok()
            .filter(|n| is_token(n));
        let name = name.ok_or(Error::Status(400, "bad header name"))?;
        let value = field_text(line[colon + 1..].trim_ascii())?;
        headers.0.push((name.to_string(), value));
    }
}

/// UTF-8 with no control bytes except tab.
fn field_text(b: &[u8]) -> Result<String, Error> {
    if b.iter().any(|&c| (c < 0x20 && c != b'\t') || c == 0x7f) {
        return Err(Error::Status(400, "control byte in head"));
    }
    String::from_utf8(b.to_vec()).map_err(|_| Error::Status(400, "head is not UTF-8"))
}

pub(crate) fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

pub(crate) fn parse_request(head: Head) -> Result<Request, Error> {
    let mut parts = head.start.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Error::Status(400, "bad request line"));
    };
    if !is_token(method) {
        return Err(Error::Status(400, "bad method"));
    }
    if !target.starts_with('/') || target.contains(['\t', '#']) {
        return Err(Error::Status(400, "bad request target"));
    }
    let http10 = match version {
        "HTTP/1.1" => false,
        "HTTP/1.0" => true,
        v if v.starts_with("HTTP/") => {
            return Err(Error::Status(505, "HTTP version not supported"));
        }
        _ => return Err(Error::Status(400, "bad request line")),
    };
    // RFC 9112 3.2: HTTP/1.1 needs a Host, and no request may carry two.
    match head.headers.get_all("Host").count() {
        0 if !http10 => return Err(Error::Status(400, "missing Host")),
        0 | 1 => {}
        _ => return Err(Error::Status(400, "more than one Host")),
    }
    Ok(Request {
        method: method.to_string(),
        path_len: target.find('?').unwrap_or(target.len()),
        target: target.to_string(),
        http10,
        headers: head.headers,
        body: Vec::new(),
    })
}

/// The status code from a response's start line.
pub(crate) fn parse_status(start: &str) -> Result<u16, Error> {
    let mut parts = start.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    let code = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.")
        || code.len() != 3
        || !code.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::Status(502, "bad status line"));
    }
    code.parse()
        .map_err(|_| Error::Status(502, "bad status line"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Framing {
    Length(u64),
    Chunked,
    /// No framing headers: an empty request body, or a response body up to EOF.
    Unframed,
}

pub(crate) fn framing(h: &Headers) -> Result<Framing, Error> {
    let mut te = h
        .get_all("Transfer-Encoding")
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .peekable();
    let has_length = h.get("Content-Length").is_some();
    if te.peek().is_some() {
        if has_length {
            return Err(Error::Status(
                400,
                "both Transfer-Encoding and Content-Length",
            ));
        }
        let first = te.next().unwrap_or("");
        return if first.eq_ignore_ascii_case("chunked") && te.next().is_none() {
            Ok(Framing::Chunked)
        } else {
            Err(Error::Status(501, "unsupported transfer coding"))
        };
    }
    let mut length = None;
    for v in h
        .get_all("Content-Length")
        .flat_map(|v| v.split(','))
        .map(str::trim)
    {
        let n = Some(v)
            .filter(|v| !v.is_empty() && v.len() <= 19 && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(Error::Status(400, "bad Content-Length"))?;
        if length.is_some_and(|l| l != n) {
            return Err(Error::Status(400, "conflicting Content-Length"));
        }
        length = Some(n);
    }
    Ok(length.map_or(Framing::Unframed, Framing::Length))
}

/// Reads a body of at most `max` bytes. `trailer_limit` bounds chunked trailers.
pub(crate) fn read_body(
    r: &mut impl BufRead,
    framing: Framing,
    max: usize,
    trailer_limit: usize,
) -> Result<Vec<u8>, Error> {
    let too_large = Error::Status(413, "content too large");
    let truncated = || Error::Status(400, "truncated body");
    let mut body = Vec::new();
    match framing {
        Framing::Unframed => {}
        Framing::Length(n) => {
            if n > max as u64 {
                return Err(too_large);
            }
            r.take(n).read_to_end(&mut body)?;
            if body.len() as u64 != n {
                return Err(truncated());
            }
        }
        Framing::Chunked => {
            let mut line = Vec::new();
            loop {
                line.clear();
                if !read_line(
                    r,
                    &mut line,
                    1024,
                    Error::Status(400, "chunk size line too long"),
                )? {
                    return Err(truncated());
                }
                let digits = line
                    .split(|&b| b == b';')
                    .next()
                    .unwrap_or(&[])
                    .trim_ascii();
                let size = Some(digits)
                    .filter(|d| {
                        !d.is_empty() && d.len() <= 15 && d.iter().all(u8::is_ascii_hexdigit)
                    })
                    .and_then(|d| u64::from_str_radix(std::str::from_utf8(d).ok()?, 16).ok())
                    .ok_or(Error::Status(400, "bad chunk size"))?;
                if size == 0 {
                    break;
                }
                if body.len() as u64 + size > max as u64 {
                    return Err(too_large);
                }
                let before = body.len();
                r.take(size).read_to_end(&mut body)?;
                line.clear();
                if (body.len() - before) as u64 != size
                    || !read_line(r, &mut line, 2, truncated())?
                    || !line.is_empty()
                {
                    return Err(Error::Status(400, "bad chunk end"));
                }
            }
            let mut used = 0;
            loop {
                line.clear();
                if !read_line(
                    r,
                    &mut line,
                    trailer_limit.saturating_sub(used),
                    Error::Status(431, "trailers too large"),
                )? {
                    return Err(Error::Status(400, "truncated trailers"));
                }
                used += line.len() + 2;
                if line.is_empty() {
                    break;
                }
            }
        }
    }
    Ok(body)
}

/// Reads a response body until EOF, at most `max` bytes.
pub(crate) fn read_to_eof(r: &mut impl BufRead, max: usize) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    r.take(max as u64 + 1).read_to_end(&mut body)?;
    if body.len() > max {
        return Err(Error::Status(413, "content too large"));
    }
    Ok(body)
}

/// Sends each write as one chunk; `finish` sends the last chunk.
pub(crate) struct ChunkedWriter<'a>(pub(crate) &'a mut dyn Write);

impl Write for ChunkedWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !buf.is_empty() {
            let mut chunk = format!("{:x}\r\n", buf.len()).into_bytes();
            chunk.extend_from_slice(buf);
            chunk.extend_from_slice(b"\r\n");
            self.0.write_all(&chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl ChunkedWriter<'_> {
    pub(crate) fn finish(self) -> io::Result<()> {
        self.0.write_all(b"0\r\n\r\n")
    }
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        417 => "Expectation Failed",
        426 => "Upgrade Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        _ => "",
    }
}

/// Parses pipelined requests from `data`, fed in small reads, and checks the limits hold.
pub fn fuzz_request(data: &[u8]) {
    const HEAD: usize = 2048;
    const BODY: usize = 4096;
    struct Trickle<'a>(&'a [u8], usize);
    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.0.len()).min(1 + self.1 % 61);
            self.1 = self.1.wrapping_mul(31).wrapping_add(7);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let seed = data.first().copied().unwrap_or(0) as usize;
    let mut r = io::BufReader::with_capacity(97, Trickle(data, seed));
    for _ in 0..64 {
        let Ok(Some(head)) = read_head(&mut r, HEAD) else {
            return;
        };
        assert!(head.start.len() <= HEAD);
        assert!(head.headers.0.len() <= MAX_HEADERS);
        let Ok(req) = parse_request(head) else { return };
        assert!(req.path().len() <= HEAD);
        let hosts = req.headers.get_all("Host").count();
        assert!(
            hosts == 1 || (req.http10 && hosts == 0),
            "{hosts} Host fields"
        );
        let _ = req.query_param("a");
        if crate::ws::check_handshake(&req).is_ok() {
            assert_canonical_key(req.header("Sec-WebSocket-Key").unwrap_or(""));
        }
        let Ok(framing) = framing(&req.headers) else {
            return;
        };
        match read_body(&mut r, framing, BODY, HEAD) {
            Ok(body) => assert!(body.len() <= BODY),
            Err(_) => return,
        }
    }
}

/// An accepted Sec-WebSocket-Key decodes to 16 bytes that encode back to the same text.
fn assert_canonical_key(key: &str) {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut bytes, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in key.trim_end_matches('=').bytes() {
        let v = A.iter().position(|&a| a == c).expect("base64 alphabet");
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    assert_eq!(bytes.len(), 16, "{key}");
    assert_eq!(crate::ws::base64(&bytes), key);
}
