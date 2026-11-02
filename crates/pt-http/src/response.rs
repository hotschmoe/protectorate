use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::headers::Headers;
use crate::request::Request;
use crate::ws::WebSocket;

pub(crate) type StreamFn = Box<dyn FnOnce(&mut dyn Write) -> io::Result<()> + Send>;
pub(crate) type WebSocketFn = Box<dyn FnOnce(WebSocket) + Send>;

pub(crate) enum Body {
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
    pub(crate) body: Body,
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
