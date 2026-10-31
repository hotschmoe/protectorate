//! A small blocking client for loopback TCP and unix sockets, with a WebSocket client.

use crate::Headers;
use crate::wire::{self, Error, Framing, Stream, Timed};
use crate::ws::{self, WebSocket};
use std::fmt;
use std::io::{self, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const MAX_HEAD: usize = 64 << 10;

#[derive(Debug, Clone)]
enum Target {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Tcp(a) => write!(f, "{a}"),
            Target::Unix(p) => write!(f, "{}", p.display()),
        }
    }
}

/// Opens one connection per request (`Connection: close`).
#[derive(Debug, Clone)]
pub struct Client {
    target: Target,
    connect_timeout: Duration,
    read_timeout: Option<Duration>,
    max_body: usize,
}

#[derive(Debug)]
pub struct ClientResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Client {
    pub fn tcp(addr: SocketAddr) -> Client {
        Client::new(Target::Tcp(addr))
    }

    pub fn unix(path: impl Into<PathBuf>) -> Client {
        Client::new(Target::Unix(path.into()))
    }

    fn new(target: Target) -> Client {
        Client {
            target,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Some(Duration::from_secs(30)),
            max_body: 16 << 20,
        }
    }

    /// TCP only; a unix connect is local and does not wait on a network.
    pub fn connect_timeout(mut self, t: Duration) -> Client {
        self.connect_timeout = t;
        self
    }

    /// Per read. For a WebSocket it is the idle time before a ping, then before giving up.
    pub fn read_timeout(mut self, t: Option<Duration>) -> Client {
        self.read_timeout = t;
        self
    }

    /// Largest response body or WebSocket message accepted.
    pub fn max_body(mut self, n: usize) -> Client {
        self.max_body = n;
        self
    }

    /// Sends a request with a Content-Length body and reads the whole response, whichever
    /// framing it uses.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> io::Result<ClientResponse> {
        let ctx = |e| self.context(&format!("{method} {path}"), e);
        let has_body = !body.is_empty() || matches!(method, "POST" | "PUT" | "PATCH");
        let length = if has_body {
            format!("Content-Length: {}\r\n", body.len())
        } else {
            String::new()
        };
        let mut reader = self
            .send_head(
                method,
                path,
                headers,
                &format!("Connection: close\r\n{length}"),
            )
            .map_err(ctx)?;
        reader.get_mut().stream.write_all(body).map_err(ctx)?;
        read_response(&mut reader, method == "HEAD", self.max_body).map_err(|e| ctx(e.into()))
    }

    /// Opens a WebSocket at `path`; outgoing frames are masked.
    pub fn websocket(&self, path: &str, headers: &[(&str, &str)]) -> io::Result<WebSocket> {
        let ctx = |e| self.context(&format!("websocket {path}"), e);
        let mut nonce = ws::random_u64().to_le_bytes().to_vec();
        nonce.extend_from_slice(&ws::random_u64().to_le_bytes());
        let key = ws::base64(&nonce);
        let upgrade = format!(
            "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n"
        );
        let mut reader = self
            .send_head("GET", path, headers, &upgrade)
            .map_err(ctx)?;
        let bad = |why: String| ctx(io::Error::new(io::ErrorKind::InvalidData, why));
        let head = wire::read_head(&mut reader, MAX_HEAD).map_err(|e| ctx(e.into()))?;
        let head = head.ok_or_else(|| bad("closed before the handshake answer".into()))?;
        let status = wire::parse_status(&head.start).map_err(|e| ctx(e.into()))?;
        if status != 101 {
            return Err(bad(format!("handshake answered {status}")));
        }
        if head.headers.get("Sec-WebSocket-Accept") != Some(ws::accept_key(&key).as_str()) {
            return Err(bad("wrong Sec-WebSocket-Accept".into()));
        }
        WebSocket::new(reader, true, self.max_body, self.read_timeout).map_err(ctx)
    }

    fn context(&self, what: &str, e: io::Error) -> io::Error {
        io::Error::new(e.kind(), format!("{what} on {}: {e}", self.target))
    }

    fn send_head(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        extra: &str,
    ) -> io::Result<BufReader<Timed>> {
        let invalid =
            |what: &str| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid {what}"));
        if !wire::is_token(method) {
            return Err(invalid("method"));
        }
        if !path.starts_with('/') || path.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Err(invalid("path"));
        }
        let host = match &self.target {
            Target::Tcp(a) => a.to_string(),
            Target::Unix(_) => "localhost".into(),
        };
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n{extra}");
        for (n, v) in headers {
            if !wire::is_token(n) || v.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
                return Err(invalid("header"));
            }
            head.push_str(&format!("{n}: {v}\r\n"));
        }
        head.push_str("\r\n");
        let mut stream = match &self.target {
            Target::Tcp(a) => {
                let s = TcpStream::connect_timeout(a, self.connect_timeout)?;
                s.set_nodelay(true)?;
                Stream::Tcp(s)
            }
            Target::Unix(p) => Stream::Unix(UnixStream::connect(p)?),
        };
        stream.set_read_timeout(self.read_timeout)?;
        stream.set_write_timeout(self.read_timeout)?;
        stream.write_all(head.as_bytes())?;
        Ok(BufReader::new(Timed {
            stream,
            deadline: None,
        }))
    }
}

fn read_response(
    r: &mut BufReader<Timed>,
    head_request: bool,
    max: usize,
) -> Result<ClientResponse, Error> {
    loop {
        let head = wire::read_head(r, MAX_HEAD)?;
        let head = head.ok_or_else(|| {
            Error::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "closed before a response",
            ))
        })?;
        let status = wire::parse_status(&head.start)?;
        if (100..200).contains(&status) && status != 101 {
            continue;
        }
        let body = if head_request || matches!(status, 101 | 204 | 304) {
            Vec::new()
        } else {
            match wire::framing(&head.headers)? {
                Framing::Unframed => wire::read_to_eof(r, max)?,
                framing => wire::read_body(r, framing, max, MAX_HEAD)?,
            }
        };
        return Ok(ClientResponse {
            status,
            headers: head.headers,
            body,
        });
    }
}
