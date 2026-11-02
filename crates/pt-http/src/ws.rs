//! RFC 6455 WebSockets: handshake keys, the frame codec, message reassembly and both halves.

use crate::wire::{Stream, Timed, is_timeout};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CONT: u8 = 0;
const TEXT: u8 = 1;
const BINARY: u8 = 2;
const CLOSE: u8 = 8;
const PING: u8 = 9;
const PONG: u8 = 10;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// A complete message. Pings are answered inside `recv` and not returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Pong(Vec<u8>),
    /// The peer's close code (1005 when it sent none) and reason. The reply is already sent.
    Close(u16, String),
}

pub(crate) fn accept_key(key: &str) -> String {
    let mut h = pt_hash::Sha1::new();
    h.update(key.as_bytes()).update(GUID.as_bytes());
    base64(&h.finalize())
}

pub(crate) fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = u32::from(c[0]) << 16
            | u32::from(*c.get(1).unwrap_or(&0)) << 8
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            s.push(if i <= c.len() {
                A[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}

/// Unpredictable enough for masking keys and handshake nonces: std seeds RandomState from the OS.
pub(crate) fn random_u64() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
    );
    h.finish()
}

#[derive(Debug)]
pub(crate) enum WsError {
    /// Close the connection with this code.
    Protocol(u16, &'static str),
    Io(io::Error),
}

impl From<io::Error> for WsError {
    fn from(e: io::Error) -> WsError {
        WsError::Io(e)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

/// Decodes one frame. `masked` is what the peer must send: true for frames from a client.
pub(crate) fn read_frame(r: &mut impl Read, masked: bool, max: usize) -> Result<Frame, WsError> {
    let mut h = [0u8; 2];
    r.read_exact(&mut h)?;
    let (fin, opcode) = (h[0] & 0x80 != 0, h[0] & 0x0f);
    if h[0] & 0x70 != 0 {
        return Err(WsError::Protocol(1002, "reserved bits set"));
    }
    if !matches!(opcode, CONT | TEXT | BINARY | CLOSE | PING | PONG) {
        return Err(WsError::Protocol(1002, "unknown opcode"));
    }
    if (h[1] & 0x80 != 0) != masked {
        return Err(WsError::Protocol(
            1002,
            if masked {
                "client frame not masked"
            } else {
                "server frame masked"
            },
        ));
    }
    let len = match h[1] & 0x7f {
        126 => {
            let mut b = [0; 2];
            r.read_exact(&mut b)?;
            u64::from(u16::from_be_bytes(b))
        }
        127 => {
            let mut b = [0; 8];
            r.read_exact(&mut b)?;
            u64::from_be_bytes(b)
        }
        n => u64::from(n),
    };
    if opcode >= CLOSE && (!fin || len > 125) {
        return Err(WsError::Protocol(1002, "bad control frame"));
    }
    if len > max as u64 {
        return Err(WsError::Protocol(1009, "message too big"));
    }
    let mut key = [0; 4];
    if masked {
        r.read_exact(&mut key)?;
    }
    let mut payload = Vec::new();
    r.take(len).read_to_end(&mut payload)?;
    if payload.len() as u64 != len {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
    }
    if masked {
        payload
            .iter_mut()
            .zip(key.iter().cycle())
            .for_each(|(b, k)| *b ^= k);
    }
    Ok(Frame {
        fin,
        opcode,
        payload,
    })
}

pub(crate) fn encode_frame(
    fin: bool,
    opcode: u8,
    payload: &[u8],
    mask: Option<[u8; 4]>,
) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 14);
    f.push(u8::from(fin) << 7 | opcode);
    let m = if mask.is_some() { 0x80 } else { 0 };
    match payload.len() {
        n @ 0..126 => f.push(m | n as u8),
        n @ 126..65536 => {
            f.push(m | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(m | 127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(key) => {
            f.extend_from_slice(&key);
            f.extend(payload.iter().zip(key.iter().cycle()).map(|(b, k)| b ^ k));
        }
        None => f.extend_from_slice(payload),
    }
    f
}

pub(crate) enum Event {
    Message(Message),
    Ping(Vec<u8>),
}

/// Reassembles fragmented messages and validates control frames.
#[derive(Default)]
pub(crate) struct Assembler {
    partial: Option<(u8, Vec<u8>)>,
}

impl Assembler {
    pub(crate) fn push(&mut self, f: Frame, max: usize) -> Result<Option<Event>, WsError> {
        let (opcode, payload) = match (f.opcode, self.partial.take()) {
            (PING, partial) => {
                self.partial = partial;
                return Ok(Some(Event::Ping(f.payload)));
            }
            (PONG, partial) => {
                self.partial = partial;
                return Ok(Some(Event::Message(Message::Pong(f.payload))));
            }
            (CLOSE, _) => {
                return close_payload(&f.payload)
                    .map(|(c, r)| Some(Event::Message(Message::Close(c, r))));
            }
            (TEXT | BINARY, Some(_)) => {
                return Err(WsError::Protocol(
                    1002,
                    "new message inside a fragmented one",
                ));
            }
            (TEXT | BINARY, None) => (f.opcode, f.payload),
            (_, None) => return Err(WsError::Protocol(1002, "continuation without a message")),
            (_, Some((opcode, mut buf))) => {
                if buf.len() + f.payload.len() > max {
                    return Err(WsError::Protocol(1009, "message too big"));
                }
                buf.extend_from_slice(&f.payload);
                (opcode, buf)
            }
        };
        if !f.fin {
            self.partial = Some((opcode, payload));
            return Ok(None);
        }
        Ok(Some(Event::Message(if opcode == TEXT {
            Message::Text(
                String::from_utf8(payload)
                    .map_err(|_| WsError::Protocol(1007, "text is not UTF-8"))?,
            )
        } else {
            Message::Binary(payload)
        })))
    }
}

fn close_payload(p: &[u8]) -> Result<(u16, String), WsError> {
    match p {
        [] => Ok((1005, String::new())),
        [_] => Err(WsError::Protocol(1002, "one-byte close payload")),
        [a, b, reason @ ..] => {
            let code = u16::from_be_bytes([*a, *b]);
            if !matches!(code, 1000..=1003 | 1007..=1011 | 3000..=4999) {
                return Err(WsError::Protocol(1002, "bad close code"));
            }
            let reason = String::from_utf8(reason.to_vec())
                .map_err(|_| WsError::Protocol(1007, "close reason is not UTF-8"))?;
            Ok((code, reason))
        }
    }
}

struct WriterState {
    stream: Stream,
    client: bool,
    closed: bool,
}

/// The sending half: Clone + Send, so another thread can push messages while the owner reads.
/// Each frame is written whole under a lock.
#[derive(Clone)]
pub struct WsWriter(Arc<Mutex<WriterState>>);

impl WsWriter {
    pub fn send_text(&self, text: &str) -> io::Result<()> {
        self.frame(TEXT, text.as_bytes())
    }

    pub fn send_binary(&self, data: &[u8]) -> io::Result<()> {
        self.frame(BINARY, data)
    }

    /// Sends a ping with at most 125 bytes of payload; the answer arrives as `Message::Pong`.
    pub fn ping(&self, data: &[u8]) -> io::Result<()> {
        self.frame(PING, &data[..data.len().min(125)])
    }

    /// Sends a close frame (once); later sends fail with NotConnected.
    pub fn close(&self, code: u16, reason: &str) -> io::Result<()> {
        let mut end = reason.len().min(123);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        let mut payload = code.to_be_bytes().to_vec();
        payload.extend_from_slice(&reason.as_bytes()[..end]);
        self.frame(CLOSE, &payload)
    }

    fn frame(&self, opcode: u8, payload: &[u8]) -> io::Result<()> {
        let mut w = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if w.closed {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "websocket closed",
            ));
        }
        w.closed = opcode == CLOSE;
        let mask = w
            .client
            .then(|| random_u64().to_le_bytes()[..4].try_into().unwrap_or([0; 4]));
        let bytes = encode_frame(true, opcode, payload, mask);
        w.stream.write_all(&bytes)
    }

    fn close_sent(&self) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).closed
    }

    pub(crate) fn shutdown(&self) {
        let mut w = self.0.lock().unwrap_or_else(|e| e.into_inner());
        w.closed = true;
        w.stream.shutdown(Shutdown::Both);
    }
}

/// One WebSocket connection, server or client side. `recv` reads; the [`WsWriter`] sends.
pub struct WebSocket {
    reader: BufReader<Timed>,
    writer: WsWriter,
    assembler: Assembler,
    max: usize,
    ping: Option<Duration>,
    /// When our keepalive ping went out; only a pong clears it.
    ping_sent: Option<Instant>,
    client: bool,
}

impl WebSocket {
    /// `ping`: after this long without a frame, send a ping; without a pong within as long again,
    /// fail TimedOut. It also bounds how long one frame may take once its first byte is in.
    pub(crate) fn new(
        mut reader: BufReader<Timed>,
        client: bool,
        max: usize,
        ping: Option<Duration>,
    ) -> io::Result<WebSocket> {
        reader.get_mut().deadline = None;
        reader.get_ref().stream.set_read_timeout(ping)?;
        let stream = reader.get_ref().stream.try_clone()?;
        let writer = WsWriter(Arc::new(Mutex::new(WriterState {
            stream,
            client,
            closed: false,
        })));
        Ok(WebSocket {
            reader,
            writer,
            assembler: Assembler::default(),
            max,
            ping,
            ping_sent: None,
            client,
        })
    }

    pub fn writer(&self) -> WsWriter {
        self.writer.clone()
    }

    pub fn send_text(&self, text: &str) -> io::Result<()> {
        self.writer.send_text(text)
    }

    pub fn send_binary(&self, data: &[u8]) -> io::Result<()> {
        self.writer.send_binary(data)
    }

    /// The next message. A protocol violation sends the matching close code and fails with
    /// InvalidData. A received close is answered before `Message::Close` is returned.
    pub fn recv(&mut self) -> io::Result<Message> {
        let client = self.client;
        loop {
            self.wait_frame()?;
            self.reader.get_mut().deadline = self.ping.map(|p| Instant::now() + p);
            let event = read_frame(&mut self.reader, !client, self.max)
                .and_then(|f| self.assembler.push(f, self.max));
            match event {
                Ok(None) => {}
                Ok(Some(Event::Message(Message::Pong(p)))) => {
                    self.ping_sent = None;
                    return Ok(Message::Pong(p));
                }
                Ok(Some(Event::Ping(p))) => match self.writer.frame(PONG, &p) {
                    Err(e) if e.kind() != io::ErrorKind::NotConnected => return Err(e),
                    _ => {}
                },
                Ok(Some(Event::Message(Message::Close(code, reason)))) => {
                    if !self.writer.close_sent() {
                        let _ = self
                            .writer
                            .close(if code == 1005 { 1000 } else { code }, "");
                    }
                    return Ok(Message::Close(code, reason));
                }
                Ok(Some(Event::Message(m))) => return Ok(m),
                Err(WsError::Protocol(code, why)) => {
                    let _ = self.writer.close(code, why);
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("websocket: {why}"),
                    ));
                }
                Err(WsError::Io(e)) => {
                    // A frame broke off midway, so the stream cannot be read further.
                    self.writer.shutdown();
                    return Err(if is_timeout(&e) {
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            "websocket: frame arrived too slowly",
                        )
                    } else {
                        e
                    });
                }
            }
        }
    }

    /// Sends a close and waits for the peer's close, discarding messages in between.
    pub fn close(&mut self, code: u16, reason: &str) -> io::Result<(u16, String)> {
        self.writer.close(code, reason)?;
        loop {
            if let Message::Close(code, reason) = self.recv()? {
                return Ok((code, reason));
            }
        }
    }

    fn wait_frame(&mut self) -> io::Result<()> {
        let stopped =
            || io::Error::new(io::ErrorKind::TimedOut, "websocket: peer stopped answering");
        loop {
            let now = Instant::now();
            let deadline = self.ping.map(|p| self.ping_sent.unwrap_or(now) + p);
            // Checked here too, since a peer that keeps sending never lets the read time out.
            if self.ping_sent.is_some() && deadline.is_some_and(|d| now >= d) {
                return Err(stopped());
            }
            self.reader.get_mut().deadline = deadline;
            match self.reader.fill_buf() {
                Ok([]) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "websocket: connection closed",
                    ));
                }
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if is_timeout(&e) && self.ping.is_some() && self.ping_sent.is_none() => {
                    self.writer.ping(b"")?;
                    self.ping_sent = Some(Instant::now());
                }
                Err(e) if is_timeout(&e) => return Err(stopped()),
                Err(e) => return Err(e),
            }
        }
    }
}

/// Decodes frames from `data` as a server would, reassembles messages, checks sizes and that
/// every decoded frame survives an encode and decode round trip.
pub fn fuzz_frames(data: &[u8]) {
    const MAX: usize = 4096;
    let (masked, mut rest) = match data.split_first() {
        Some((b, rest)) => (b & 1 == 1, rest),
        None => return,
    };
    let mut asm = Assembler::default();
    while let Ok(frame) = read_frame(&mut rest, masked, MAX) {
        assert!(frame.payload.len() <= MAX);
        let again = encode_frame(
            frame.fin,
            frame.opcode,
            &frame.payload,
            masked.then_some([1, 2, 3, 4]),
        );
        assert_eq!(
            read_frame(&mut &again[..], masked, MAX).ok().as_ref(),
            Some(&frame)
        );
        match asm.push(frame, MAX) {
            Ok(Some(Event::Message(Message::Text(t)))) => assert!(t.len() <= MAX),
            Ok(Some(Event::Message(Message::Binary(b)))) => assert!(b.len() <= MAX),
            Ok(_) => {}
            Err(_) => return,
        }
    }
}
