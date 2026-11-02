//! The hub-node frame, the only parser fed by hostile machines:
//!
//! ```text
//! [len: u32 LE][kind: u8][id: u64 LE][payload: len - 9 bytes]
//! ```
//!
//! `len` counts the kind, id and payload, and lies in `HEAD..=MAX_FRAME`. A length outside that
//! range or an unknown kind is an error as soon as its bytes arrive, before any payload is
//! buffered. Pure: no I/O, and nothing is allocated beyond the bytes actually received.

use std::fmt;

/// Largest `len`: kind, id and payload together.
pub const MAX_FRAME: usize = 1_048_576;
/// Bytes of kind and id that every frame carries after its length.
pub const HEAD: usize = 9;
pub const MAX_PAYLOAD: usize = MAX_FRAME - HEAD;
/// Bytes on the wire before the payload.
pub const PREFIX: usize = 4 + HEAD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Request,
    Response,
    Chunk,
    End,
    Error,
}

impl Kind {
    pub fn to_u8(self) -> u8 {
        match self {
            Kind::Request => 1,
            Kind::Response => 2,
            Kind::Chunk => 3,
            Kind::End => 4,
            Kind::Error => 5,
        }
    }

    pub fn from_u8(b: u8) -> Option<Kind> {
        match b {
            1 => Some(Kind::Request),
            2 => Some(Kind::Response),
            3 => Some(Kind::Chunk),
            4 => Some(Kind::End),
            5 => Some(Kind::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind,
    pub id: u64,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A declared length, or a payload to encode, over the limit.
    TooLarge(u64),
    /// A declared length too small to hold kind and id.
    TooShort(u32),
    BadKind(u8),
    /// The stream ended inside a frame, with this many bytes of it received.
    Truncated(usize),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::TooLarge(n) => write!(f, "frame length {n} is over {MAX_FRAME}"),
            FrameError::TooShort(n) => write!(f, "frame length {n} is under {HEAD}"),
            FrameError::BadKind(k) => write!(f, "unknown frame kind {k}"),
            FrameError::Truncated(n) => write!(f, "stream ended {n} bytes into a frame"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Appends one encoded frame to `out`.
pub fn encode_into(
    kind: Kind,
    id: u64,
    payload: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), FrameError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(FrameError::TooLarge(
            (payload.len() as u64).saturating_add(HEAD as u64),
        ));
    }
    let len = (payload.len() + HEAD) as u32;
    out.reserve(PREFIX + payload.len());
    out.push(len as u8);
    out.push((len >> 8) as u8);
    out.push((len >> 16) as u8);
    out.push((len >> 24) as u8);
    out.push(kind.to_u8());
    out.push(id as u8);
    out.push((id >> 8) as u8);
    out.push((id >> 16) as u8);
    out.push((id >> 24) as u8);
    out.push((id >> 32) as u8);
    out.push((id >> 40) as u8);
    out.push((id >> 48) as u8);
    out.push((id >> 56) as u8);
    out.extend_from_slice(payload);
    Ok(())
}

pub fn encode(kind: Kind, id: u64, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    encode_into(kind, id, payload, &mut out)?;
    Ok(out)
}

fn read_u32(buf: &[u8]) -> u32 {
    (buf[0] as u32) | ((buf[1] as u32) << 8) | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24)
}

fn read_u64(buf: &[u8]) -> u64 {
    (buf[0] as u64)
        | ((buf[1] as u64) << 8)
        | ((buf[2] as u64) << 16)
        | ((buf[3] as u64) << 24)
        | ((buf[4] as u64) << 32)
        | ((buf[5] as u64) << 40)
        | ((buf[6] as u64) << 48)
        | ((buf[7] as u64) << 56)
}

/// Decodes the frame at the front of `buf`: `Ok(None)` when more bytes are needed, else the
/// frame and the bytes it took. A strict prefix of a valid frame always gives `Ok(None)`.
pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, FrameError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = read_u32(buf);
    if len as u64 > MAX_FRAME as u64 {
        return Err(FrameError::TooLarge(len as u64));
    }
    if (len as usize) < HEAD {
        return Err(FrameError::TooShort(len));
    }
    if buf.len() < 5 {
        return Ok(None);
    }
    let kind = match Kind::from_u8(buf[4]) {
        Some(k) => k,
        None => return Err(FrameError::BadKind(buf[4])),
    };
    let total = 4 + len as usize;
    if buf.len() < total {
        return Ok(None);
    }
    let id = read_u64(&buf[5..]);
    let mut payload = Vec::new();
    payload.extend_from_slice(&buf[PREFIX..total]);
    Ok(Some((Frame { kind, id, payload }, total)))
}

/// An incremental decoder. Call `next_frame` until it gives `Ok(None)` before feeding more, so the
/// buffer never holds more than one partial frame plus the last bytes fed. After an error the
/// stream is never trusted again: every later call returns the same error.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    failed: Option<FrameError>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buf.extend_from_slice(bytes);
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        match decode(&self.buf) {
            Ok(Some((frame, used))) => {
                let remaining = self.buf.len() - used;
                self.buf.copy_within(used.., 0);
                self.buf.truncate(remaining);
                Ok(Some(frame))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.failed = Some(e);
                self.buf = Vec::new();
                Err(e)
            }
        }
    }

    /// Bytes received but not yet returned as a frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// At end of stream: an error if a frame was left incomplete.
    pub fn finish(&self) -> Result<(), FrameError> {
        match (self.failed, self.buf.len()) {
            (Some(e), _) => Err(e),
            (None, 0) => Ok(()),
            (None, n) => Err(FrameError::Truncated(n)),
        }
    }
}
