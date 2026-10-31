//! The frame codec, pure and allocation-bounded (Verus phase 1).
//!
//! A frame is `u32 LE payload length`, `u32 LE CRC-32C of the payload`, then the payload.
//! Payloads are 1 to [`MAX_FRAME`] bytes. Empty payloads are refused so that a zero-filled tail,
//! which a filesystem can leave after a crash, never reads as a run of valid frames.

use crate::crc32c::crc32c;

/// Largest payload a frame may carry: 16 MiB.
pub const MAX_FRAME: usize = 16 << 20;
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

/// The result of [`decode_frames`]: the valid prefix and where it ends.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    pub frames: Vec<&'a [u8]>,
    /// Length of the valid prefix: the offset of the first byte not in a valid frame.
    pub valid_len: usize,
    /// Why decoding stopped at `valid_len`, or `None` if the whole buffer was valid.
    pub stop: Option<Stop>,
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
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32c(payload).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
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
        let len = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let crc = u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]);
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
