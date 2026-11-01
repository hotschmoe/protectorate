//! The hub-node frame, the only parser fed by hostile machines (Verus phase 1, SPEC 10):
//!
//! ```text
//! [len: u32 LE][kind: u8][id: u64 LE][payload: len - 9 bytes]
//! ```
//!
//! `len` counts the kind, id and payload, and lies in `HEAD..=MAX_FRAME`. A length outside that
//! range or an unknown kind is an error as soon as its bytes arrive, before any payload is
//! buffered. Pure: no I/O, and nothing is allocated beyond the bytes actually received.

use std::fmt;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Largest `len`: kind, id and payload together.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures MAX_FRAME == 1_048_576))]
pub const MAX_FRAME: usize = 1_048_576;
/// Bytes of kind and id that every frame carries after its length.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures HEAD == 9))]
pub const HEAD: usize = 9;
#[cfg_attr(verus_keep_ghost, verus_spec(ensures MAX_PAYLOAD == 1_048_567))]
pub const MAX_PAYLOAD: usize = MAX_FRAME - HEAD;
/// Bytes on the wire before the payload.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures PREFIX == 13))]
pub const PREFIX: usize = 4 + HEAD;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Request,
    Response,
    Chunk,
    End,
    Error,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Kind {
    #[cfg_attr(verus_keep_ghost, verus_spec(b => ensures b == kind_byte(self)))]
    pub fn to_u8(self) -> u8 {
        match self {
            Kind::Request => 1,
            Kind::Response => 2,
            Kind::Chunk => 3,
            Kind::End => 4,
            Kind::Error => 5,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == match b {
            1 => Some(Kind::Request),
            2 => Some(Kind::Response),
            3 => Some(Kind::Chunk),
            4 => Some(Kind::End),
            5 => Some(Kind::Error),
            _ => None,
        }
    ))]
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

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn kind_byte(kind: Kind) -> u8 {
        match kind {
            Kind::Request => 1,
            Kind::Response => 2,
            Kind::Chunk => 3,
            Kind::End => 4,
            Kind::Error => 5,
        }
    }

    pub open spec fn wire_bytes(kind: Kind, id: u64, payload: Seq<u8>) -> Seq<u8> {
        let len = (payload.len() + 9) as u32;
        seq![
            len as u8, (len >> 8) as u8, (len >> 16) as u8, (len >> 24) as u8,
            kind_byte(kind),
            id as u8, (id >> 8) as u8, (id >> 16) as u8, (id >> 24) as u8,
            (id >> 32) as u8, (id >> 40) as u8, (id >> 48) as u8, (id >> 56) as u8,
        ] + payload
    }

    pub open spec fn u32_at(buf: Seq<u8>, at: int) -> u32
        recommends 0 <= at && at + 4 <= buf.len()
    {
        (buf[at] as u32) | ((buf[at + 1] as u32) << 8)
            | ((buf[at + 2] as u32) << 16) | ((buf[at + 3] as u32) << 24)
    }

    pub open spec fn u64_at(buf: Seq<u8>, at: int) -> u64
        recommends 0 <= at && at + 8 <= buf.len()
    {
        (buf[at] as u64) | ((buf[at + 1] as u64) << 8)
            | ((buf[at + 2] as u64) << 16) | ((buf[at + 3] as u64) << 24)
            | ((buf[at + 4] as u64) << 32) | ((buf[at + 5] as u64) << 40)
            | ((buf[at + 6] as u64) << 48) | ((buf[at + 7] as u64) << 56)
    }

    pub proof fn lemma_wire_bytes(kind: Kind, id: u64, payload: Seq<u8>)
        requires payload.len() <= 1_048_567,
        ensures
            wire_bytes(kind, id, payload).len() == 13 + payload.len(),
            u32_at(wire_bytes(kind, id, payload), 0) == 9 + payload.len(),
            wire_bytes(kind, id, payload)[4] == kind_byte(kind),
            1 <= kind_byte(kind) <= 5,
            u64_at(wire_bytes(kind, id, payload), 5) == id,
            wire_bytes(kind, id, payload).subrange(13, 13 + payload.len() as int) == payload,
    {
        let len = (payload.len() + 9) as u32;
        assert(
            len == (len as u8 as u32) | (((len >> 8) as u8 as u32) << 8)
                | (((len >> 16) as u8 as u32) << 16)
                | (((len >> 24) as u8 as u32) << 24)
        ) by (bit_vector);
        assert(
            id == (id as u8 as u64) | (((id >> 8) as u8 as u64) << 8)
                | (((id >> 16) as u8 as u64) << 16)
                | (((id >> 24) as u8 as u64) << 24)
                | (((id >> 32) as u8 as u64) << 32)
                | (((id >> 40) as u8 as u64) << 40)
                | (((id >> 48) as u8 as u64) << 48)
                | (((id >> 56) as u8 as u64) << 56)
        ) by (bit_vector);
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(verus_keep_ghost, verifier::allow(autoderive_clone_without_spec))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind,
    pub id: u64,
    pub payload: Vec<u8>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
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

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn decode_contract(
        buf: Seq<u8>, result: Result<Option<(Frame, usize)>, FrameError>,
    ) -> bool {
        &&& match result {
            Ok(Some((frame, used))) =>
                13 <= used && used <= buf.len()
                && frame.payload@.len() <= 1_048_567
                && frame.payload@.len() + 13 == used
                && used == 4 + u32_at(buf, 0)
                && kind_byte(frame.kind) == buf[4]
                && frame.id == u64_at(buf, 5)
                && frame.payload@ == buf.subrange(13, used as int),
            _ => true,
        }
        &&& (buf.len() >= 5 && 9 <= u32_at(buf, 0) <= 1_048_576
            && 1 <= buf[4] <= 5 && buf.len() >= 4 + u32_at(buf, 0)
            ==> match result { Ok(Some(_)) => true, _ => false })
        &&& (buf.len() < 4 ==> match result { Ok(None) => true, _ => false })
        &&& (buf.len() >= 4 && u32_at(buf, 0) > 1_048_576 ==>
            match result {
                Err(FrameError::TooLarge(n)) => n == u32_at(buf, 0),
                _ => false,
            })
        &&& (buf.len() >= 4 && u32_at(buf, 0) < 9 ==>
            match result {
                Err(FrameError::TooShort(n)) => n == u32_at(buf, 0),
                _ => false,
            })
        &&& (buf.len() == 4 && 9 <= u32_at(buf, 0) <= 1_048_576 ==>
            match result { Ok(None) => true, _ => false })
        &&& (buf.len() >= 5 && 9 <= u32_at(buf, 0) <= 1_048_576
            && (buf[4] < 1 || buf[4] > 5) ==>
            match result {
                Err(FrameError::BadKind(b)) => b == buf[4],
                _ => false,
            })
        &&& (buf.len() >= 5 && 9 <= u32_at(buf, 0) <= 1_048_576
            && 1 <= buf[4] <= 5 && buf.len() < 4 + u32_at(buf, 0) ==>
            match result { Ok(None) => true, _ => false })
    }

    pub proof fn lemma_frame_roundtrip(
        kind: Kind, id: u64, payload: Seq<u8>,
        result: Result<Option<(Frame, usize)>, FrameError>,
    )
        requires
            payload.len() <= 1_048_567,
            decode_contract(wire_bytes(kind, id, payload), result),
        ensures
            match result {
                Ok(Some((frame, used))) =>
                    frame.kind == kind && frame.id == id && frame.payload@ == payload
                    && used == 13 + payload.len(),
                _ => false,
            },
    {
        lemma_wire_bytes(kind, id, payload);
    }
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
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Ok(()) => payload.len() <= 1_048_567
            && final(out)@ == old(out)@ + wire_bytes(kind, id, payload@),
        Err(_) => payload.len() > 1_048_567 && final(out)@ == old(out)@,
    }
))]
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

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Ok(bytes) => payload.len() <= 1_048_567 && bytes@ == wire_bytes(kind, id, payload@),
        Err(_) => payload.len() > 1_048_567,
    }
))]
pub fn encode(kind: Kind, id: u64, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    encode_into(kind, id, payload, &mut out)?;
    Ok(out)
}

#[cfg_attr(verus_keep_ghost, verus_spec(n =>
    requires buf.len() >= 4,
    ensures n == u32_at(buf@, 0),
))]
fn read_u32(buf: &[u8]) -> u32 {
    (buf[0] as u32) | ((buf[1] as u32) << 8) | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24)
}

#[cfg_attr(verus_keep_ghost, verus_spec(n =>
    requires buf.len() >= 8,
    ensures n == u64_at(buf@, 0),
))]
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
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures decode_contract(buf@, result),
))]
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
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(payload@ =~= buf@.subrange(13, total as int));
    }
    Ok(Some((Frame { kind, id, payload }, total)))
}

/// An incremental decoder. Call `next_frame` until it gives `Ok(None)` before feeding more, so the
/// buffer never holds more than one partial frame plus the last bytes fed. After an error the
/// stream is never trusted again: every later call returns the same error.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    failed: Option<FrameError>,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub closed spec fn decoder_buffer(decoder: &Decoder) -> Seq<u8> { decoder.buf@ }
    pub closed spec fn decoder_error(decoder: &Decoder) -> Option<FrameError> { decoder.failed }
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            decoder_error(final(self)) == decoder_error(old(self)),
            match decoder_error(old(self)) {
                Some(_) => decoder_buffer(final(self)) == decoder_buffer(old(self)),
                None => decoder_buffer(final(self)) == decoder_buffer(old(self)) + bytes@,
            },
    ))]
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buf.extend_from_slice(bytes);
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures
            match decoder_error(old(self)) {
                Some(e) => result == Err(e) && decoder_buffer(final(self)) == decoder_buffer(old(self)),
                None => true,
            },
            match result {
                Err(e) => decoder_error(final(self)) == Some(e),
                Ok(_) => decoder_error(final(self)) == None,
            },
            match result {
                Ok(Some(frame)) => exists|used: usize|
                    13 <= used <= decoder_buffer(old(self)).len()
                    && decoder_buffer(final(self)) == decoder_buffer(old(self)).subrange(used as int, decoder_buffer(old(self)).len() as int)
                    && decode_contract(decoder_buffer(old(self)), Ok(Some((frame, used)))),
                Ok(None) => decoder_buffer(final(self)) == decoder_buffer(old(self)),
                Err(_) => true,
            },
    ))]
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before = self.buf@; }
        match decode(&self.buf) {
            Ok(Some((frame, used))) => {
                let remaining = self.buf.len() - used;
                self.buf.copy_within(used.., 0);
                self.buf.truncate(remaining);
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(self.buf@ =~= before.subrange(used as int, before.len() as int));
                    assert(decode_contract(before, Ok(Some((frame, used)))));
                    assert(decoder_buffer(old(self)) == before);
                    assert(decoder_buffer(self) == before.subrange(used as int, before.len() as int));
                    assert(exists|n: usize|
                        13 <= n <= before.len()
                        && decoder_buffer(self) == before.subrange(n as int, before.len() as int)
                        && decode_contract(before, Ok(Some((frame, n)))));
                }
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
