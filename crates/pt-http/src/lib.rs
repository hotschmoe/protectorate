//! HTTP/1.1 server and a small client on blocking std sockets, with RFC 6455 WebSockets.
//!
//! One thread per connection, no async runtime. The server never interprets proxy headers such as
//! X-Forwarded-For: it exposes the headers and the [`Peer`], and the caller decides trust.

#![forbid(unsafe_code)]

mod client;
mod server;
mod wire;
mod ws;

pub use client::{Client, ClientResponse};
pub use pt_sys::PeerCred;
pub use server::{Config, Handler, Server, ShutdownHandle};
pub use ws::{Message, WebSocket, WsWriter};

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Header fields in arrival order. Lookups ignore ASCII case; a name may repeat.
#[derive(Debug, Clone, Default)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    /// The first value of `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.get_all(name).next()
    }

    pub fn get_all<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a str> {
        self.0
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }

    /// True when a comma-separated list header such as Connection holds `token` (any case).
    pub fn has_token(&self, name: &str, token: &str) -> bool {
        self.get_all(name)
            .flat_map(|v| v.split(','))
            .any(|t| t.trim().eq_ignore_ascii_case(token))
    }

    /// Appends a field. A field whose name is not a token, or whose value holds CR, LF or NUL,
    /// is dropped, so a handler cannot split the response.
    pub fn add(&mut self, name: &str, value: &str) {
        if wire::is_token(name) && !value.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
            self.0.push((name.to_string(), value.to_string()));
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.0.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    }
}

/// Who is on the other end of a connection.
#[derive(Debug, Clone)]
pub enum Peer {
    Tcp(SocketAddr),
    /// Credentials of the connecting process (SO_PEERCRED).
    Unix(PeerCred),
}

/// A parsed request with its whole body.
#[derive(Debug)]
pub struct Request {
    method: String,
    target: String,
    path_len: usize,
    http10: bool,
    headers: Headers,
    body: Vec<u8>,
}

impl Request {
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The path as sent, without the query and without percent-decoding.
    pub fn path(&self) -> &str {
        &self.target[..self.path_len]
    }

    /// The raw query after `?`, or "".
    pub fn query(&self) -> &str {
        self.target.get(self.path_len + 1..).unwrap_or("")
    }

    /// The first `name=value` query pair with that name, both percent-decoded (`+` is a space).
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query().split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (wire::percent_decode(k) == name).then(|| wire::percent_decode(v))
        })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

type StreamFn = Box<dyn FnOnce(&mut dyn Write) -> io::Result<()> + Send>;
type WebSocketFn = Box<dyn FnOnce(WebSocket) + Send>;

enum Body {
    Bytes(Cow<'static, [u8]>),
    File(File, u64),
    Stream(StreamFn),
    WebSocket(WebSocketFn),
}

/// A status, headers and a body. The server owns the framing headers (Content-Length,
/// Transfer-Encoding, Connection) and drops any the handler sets.
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    body: Body,
}

impl Response {
    fn with(status: u16, body: Body) -> Response {
        Response {
            status,
            headers: Headers::default(),
            body,
        }
    }

    /// An empty body.
    pub fn new(status: u16) -> Response {
        Response::with(status, Body::Bytes(Cow::Borrowed(b"")))
    }

    /// A byte body; `&'static [u8]` (such as an embedded asset) is not copied.
    pub fn bytes(status: u16, content_type: &str, body: impl Into<Cow<'static, [u8]>>) -> Response {
        Response::with(status, Body::Bytes(body.into())).header("Content-Type", content_type)
    }

    pub fn text(status: u16, body: impl Into<String>) -> Response {
        Response::bytes(
            status,
            "text/plain; charset=utf-8",
            body.into().into_bytes(),
        )
    }

    pub fn json(status: u16, body: impl Into<String>) -> Response {
        Response::bytes(status, "application/json", body.into().into_bytes())
    }

    /// Streams a file from disk, with a content type from its extension and an ETag from its
    /// length and modification time (304 when the request already has it).
    pub fn file(req: &Request, path: &Path) -> io::Result<Response> {
        let ctx = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        let file = File::open(path).map_err(ctx)?;
        let meta = file.metadata().map_err(ctx)?;
        if !meta.is_file() {
            return Err(ctx(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            )));
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok());
        let etag = format!("{:x}-{:x}", meta.len(), mtime.map_or(0, |d| d.as_nanos()));
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        Ok(Response::with(200, Body::File(file, meta.len()))
            .header("Content-Type", content_type(name))
            .etag(req, &etag))
    }

    /// A chunked body written by `f` on the connection thread; each `write` is sent as a chunk.
    pub fn stream(
        status: u16,
        content_type: &str,
        f: impl FnOnce(&mut dyn Write) -> io::Result<()> + Send + 'static,
    ) -> Response {
        Response::with(status, Body::Stream(Box::new(f))).header("Content-Type", content_type)
    }

    /// Upgrades to a WebSocket and runs `f` on the connection thread after the 101. The server
    /// validates the handshake and answers 400 or 426 itself when it is not one. Check Origin
    /// before returning this for browser clients.
    pub fn websocket(f: impl FnOnce(WebSocket) + Send + 'static) -> Response {
        Response::with(101, Body::WebSocket(Box::new(f)))
    }

    pub fn not_found() -> Response {
        Response::text(404, "not found\n")
    }

    pub fn method_not_allowed(allow: &str) -> Response {
        Response::text(405, "method not allowed\n").header("Allow", allow)
    }

    pub fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.add(name, value);
        self
    }

    /// Sets the ETag `"tag"` and turns the response into a bodiless 304 when the request's
    /// If-None-Match already names it.
    pub fn etag(mut self, req: &Request, tag: &str) -> Response {
        let quoted = format!("\"{tag}\"");
        self.headers.remove("ETag");
        let hit = req
            .headers
            .get_all("If-None-Match")
            .flat_map(|v| v.split(','))
            .any(|t| {
                let t = t.trim();
                t == "*" || t.strip_prefix("W/").unwrap_or(t) == quoted
            });
        if hit && matches!(self.status, 200..300) {
            self.status = 304;
            self.body = Body::Bytes(Cow::Borrowed(b""));
        }
        self.header("ETag", &quoted)
    }
}

/// The content type for a file name, by extension, covering a Flutter web build.
pub fn content_type(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Binds a unix socket at `path`, however long: see [`unix_addr`].
pub(crate) fn unix_bind(path: &Path) -> io::Result<UnixListener> {
    let (_dir, addr) = unix_addr(path)?;
    UnixListener::bind(addr)
}

/// Connects to a unix socket at `path`, however long: see [`unix_addr`].
pub(crate) fn unix_connect(path: &Path) -> io::Result<UnixStream> {
    let (_dir, addr) = unix_addr(path)?;
    UnixStream::connect(addr)
}

/// An address for `path` that fits `sun_path` (107 bytes and a NUL). A longer path is reached
/// through its directory's descriptor under /proc/self/fd, which the returned file keeps open.
fn unix_addr(path: &Path) -> io::Result<(Option<File>, Cow<'_, Path>)> {
    const SUN_PATH_MAX: usize = 107;
    if path.as_os_str().len() <= SUN_PATH_MAX || !cfg!(target_os = "linux") {
        return Ok((None, Cow::Borrowed(path)));
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok((None, Cow::Borrowed(path)));
    };
    let dir = File::open(dir)
        .map_err(|e| io::Error::new(e.kind(), format!("open {}: {e}", dir.display())))?;
    let short = Path::new("/proc/self/fd")
        .join(dir.as_raw_fd().to_string())
        .join(name);
    Ok((Some(dir), Cow::Owned(short)))
}

/// Entry points for the cargo-fuzz targets in `fuzz/`; not a stable API.
#[doc(hidden)]
pub mod fuzzing {
    pub use crate::wire::fuzz_request as request;
    pub use crate::ws::fuzz_frames as websocket;
}
