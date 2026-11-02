//! The frame codec, pure and allocation-bounded.
//!
//! A frame is `u32 LE payload length`, `u32 LE CRC-32C of the payload`, then the payload.
//! Payloads are 1 to [`MAX_FRAME`] bytes. Empty payloads are refused so that a zero-filled tail,
//! which a filesystem can leave after a crash, never reads as a run of valid frames.

use crate::crc32c::crc32c;

/// Largest payload a frame may carry: 16 MiB.
pub const MAX_FRAME: usize = 16_777_216;
/// Bytes before the payload: length and CRC.
pub const FRAME_HEADER: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    Empty,
    TooLarge(usize),
}

/// Why decoding stopped before the end of the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Fewer than [`FRAME_HEADER`] bytes remain.
    ShortHeader,
    /// The declared length is zero.
    ZeroLength,
    /// The declared length exceeds [`MAX_FRAME`].
    TooLarge,
    /// The declared payload runs past the end of the buffer.
    ShortPayload,
    /// The payload does not match its CRC.
    CrcMismatch,
}

#[derive(Clone, Copy)]
pub(crate) enum Recovery {
    Clean(usize),
    Torn(usize, Stop),
    Corrupt(usize),
}

/// The open decision for a decoded segment. A torn append damages only the last complete frame,
/// so a CRC failure is corruption only when an intact frame follows it (`intact_after`); every
/// other stop is a torn tail and is cut.
pub(crate) fn recovery(valid_len: usize, stop: Option<Stop>, intact_after: bool) -> Recovery {
    match stop {
        None => Recovery::Clean(valid_len),
        Some(Stop::CrcMismatch) if intact_after => Recovery::Corrupt(valid_len),
        Some(stop) => Recovery::Torn(valid_len, stop),
    }
}

/// The result of [`decode_frames`]: the valid prefix and where it ends.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    pub frames: Vec<&'a [u8]>,
    /// Length of the valid prefix: the offset of the first byte not in a valid frame.
    pub valid_len: usize,
    /// Why decoding stopped at `valid_len`, or `None` if the whole buffer was valid.
    pub stop: Option<Stop>,
}

fn read_u32(buf: &[u8]) -> u32 {
    (buf[0] as u32) | ((buf[1] as u32) << 8) | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24)
}

/// Appends one frame carrying `payload` to `out`.
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if payload.is_empty() {
        return Err(EncodeError::Empty);
    }
    if payload.len() > MAX_FRAME {
        return Err(EncodeError::TooLarge(payload.len()));
    }
    out.reserve(FRAME_HEADER + payload.len());
    let len = payload.len() as u32;
    out.push(len as u8);
    out.push((len >> 8) as u8);
    out.push((len >> 16) as u8);
    out.push((len >> 24) as u8);
    let crc = crc32c(payload);
    out.push(crc as u8);
    out.push((crc >> 8) as u8);
    out.push((crc >> 16) as u8);
    out.push((crc >> 24) as u8);
    out.extend_from_slice(payload);
    Ok(())
}

/// The total length of the frame at the start of `buf` when its declared length is valid and
/// its payload is all present, whatever its CRC.
pub fn complete_len(buf: &[u8]) -> Option<usize> {
    if buf.len() < FRAME_HEADER {
        return None;
    }
    let len = read_u32(buf) as usize;
    if len != 0 && len <= MAX_FRAME && buf.len() - FRAME_HEADER >= len {
        Some(FRAME_HEADER + len)
    } else {
        None
    }
}

/// Decodes frames from the start of `buf` up to the first invalid one. Nothing after the first
/// invalid frame is examined: a damaged frame in the middle ends the valid prefix.
pub fn decode_frames(buf: &[u8]) -> Decoded<'_> {
    let mut frames = Vec::new();
    let mut pos = 0usize;
    let stop = loop {
        let rest = &buf[pos..];
        if rest.is_empty() {
            break None;
        }
        if rest.len() < FRAME_HEADER {
            break Some(Stop::ShortHeader);
        }
        let len = read_u32(rest) as usize;
        let crc = read_u32(&rest[4..]);
        if len == 0 {
            break Some(Stop::ZeroLength);
        }
        if len > MAX_FRAME {
            break Some(Stop::TooLarge);
        }
        if rest.len() - FRAME_HEADER < len {
            break Some(Stop::ShortPayload);
        }
        let payload = &rest[FRAME_HEADER..FRAME_HEADER + len];
        if crc32c(payload) != crc {
            break Some(Stop::CrcMismatch);
        }
        frames.push(payload);
        pos += FRAME_HEADER + len;
    };
    Decoded {
        frames,
        valid_len: pos,
        stop,
    }
}
