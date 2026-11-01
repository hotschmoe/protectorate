//! The frame codec, pure and allocation-bounded (Verus phase 1).
//!
//! A frame is `u32 LE payload length`, `u32 LE CRC-32C of the payload`, then the payload.
//! Payloads are 1 to [`MAX_FRAME`] bytes. Empty payloads are refused so that a zero-filled tail,
//! which a filesystem can leave after a crash, never reads as a run of valid frames.

use crate::crc32c::crc32c;

#[cfg(verus_keep_ghost)]
use crate::crc32c::spec_crc32c;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Largest payload a frame may carry: 16 MiB.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures MAX_FRAME == 16_777_216))]
pub const MAX_FRAME: usize = 16_777_216;
/// Bytes before the payload: length and CRC.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures FRAME_HEADER == 8))]
pub const FRAME_HEADER: usize = 8;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    Empty,
    TooLarge(usize),
}

/// Why decoding stopped before the end of the buffer.
#[cfg_attr(verus_keep_ghost, verus_verify)]
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

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) enum Recovery {
    Clean(usize),
    Torn(usize, Stop),
    Corrupt(usize),
}

/// The open decision for a decoded segment. A torn append damages only the last complete frame,
/// so a CRC failure is corruption only when an intact frame follows it (`intact_after`); every
/// other stop is a torn tail and is cut.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == match stop {
        None => Recovery::Clean(valid_len),
        Some(s) => if s == Stop::CrcMismatch && intact_after {
            Recovery::Corrupt(valid_len)
        } else {
            Recovery::Torn(valid_len, s)
        },
    }
))]
pub(crate) fn recovery(valid_len: usize, stop: Option<Stop>, intact_after: bool) -> Recovery {
    match stop {
        None => Recovery::Clean(valid_len),
        Some(Stop::CrcMismatch) if intact_after => Recovery::Corrupt(valid_len),
        Some(stop) => Recovery::Torn(valid_len, stop),
    }
}

/// The result of [`decode_frames`]: the valid prefix and where it ends.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    pub frames: Vec<&'a [u8]>,
    /// Length of the valid prefix: the offset of the first byte not in a valid frame.
    pub valid_len: usize,
    /// Why decoding stopped at `valid_len`, or `None` if the whole buffer was valid.
    pub stop: Option<Stop>,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn u32_at(buf: Seq<u8>, at: int) -> u32
        recommends 0 <= at && at + 4 <= buf.len()
    {
        (buf[at] as u32) | ((buf[at + 1] as u32) << 8)
            | ((buf[at + 2] as u32) << 16) | ((buf[at + 3] as u32) << 24)
    }

    pub open spec fn frame_bytes(payload: Seq<u8>, crc: u32) -> Seq<u8> {
        let len = payload.len() as u32;
        seq![
            len as u8, (len >> 8) as u8, (len >> 16) as u8, (len >> 24) as u8,
            crc as u8, (crc >> 8) as u8, (crc >> 16) as u8, (crc >> 24) as u8,
        ] + payload
    }

    pub open spec fn payload_views<'a>(frames: Seq<&'a [u8]>) -> Seq<Seq<u8>> {
        frames.map_values(|frame: &'a [u8]| frame@)
    }

    #[verifier::opaque]
    pub open spec fn model_decode(buf: Seq<u8>) -> Seq<Seq<u8>>
        decreases buf.len()
    {
        if buf.len() < 8 {
            Seq::empty()
        } else {
            let len = u32_at(buf, 0) as nat;
            if len == 0 || len > 16_777_216 || buf.len() - 8 < len {
                Seq::empty()
            } else {
                let payload = buf.subrange(8, (8 + len) as int);
                if spec_crc32c(payload) != u32_at(buf, 4) {
                    Seq::empty()
                } else {
                    seq![payload] + model_decode(buf.subrange((8 + len) as int, buf.len() as int))
                }
            }
        }
    }

    pub open spec fn valid_first(buf: Seq<u8>) -> bool {
        buf.len() >= 8 && {
            let len = u32_at(buf, 0) as nat;
            0 < len <= 16_777_216 && len <= buf.len() - 8
                && spec_crc32c(buf.subrange(8, (8 + len) as int)) == u32_at(buf, 4)
        }
    }

    pub open spec fn valid_payloads(frames: Seq<Seq<u8>>) -> bool {
        forall|i: int| 0 <= i < frames.len() ==> 0 < #[trigger] frames[i].len() <= 16_777_216
    }

    #[verifier::opaque]
    pub open spec fn model_encode(frames: Seq<Seq<u8>>) -> Seq<u8>
        decreases frames.len()
    {
        if frames.len() == 0 {
            Seq::empty()
        } else {
            frame_bytes(frames.first(), spec_crc32c(frames.first()))
                + model_encode(frames.drop_first())
        }
    }

    proof fn lemma_frame_bytes(payload: Seq<u8>)
        requires 0 < payload.len() <= 16_777_216
        ensures
            frame_bytes(payload, spec_crc32c(payload)).len() == payload.len() + 8,
            u32_at(frame_bytes(payload, spec_crc32c(payload)), 0) == payload.len(),
            u32_at(frame_bytes(payload, spec_crc32c(payload)), 4) == spec_crc32c(payload),
            frame_bytes(payload, spec_crc32c(payload)).subrange(8, (8 + payload.len()) as int)
                == payload,
    {
        let len = payload.len() as u32;
        let crc = spec_crc32c(payload);
        assert(
            len == (len as u8 as u32) | (((len >> 8) as u8 as u32) << 8)
                | (((len >> 16) as u8 as u32) << 16)
                | (((len >> 24) as u8 as u32) << 24)
        ) by (bit_vector);
        assert(
            crc == (crc as u8 as u32) | (((crc >> 8) as u8 as u32) << 8)
                | (((crc >> 16) as u8 as u32) << 16)
                | (((crc >> 24) as u8 as u32) << 24)
        ) by (bit_vector);
    }

    proof fn lemma_model_stops(buf: Seq<u8>)
        requires !valid_first(buf)
        ensures model_decode(buf) == Seq::<Seq<u8>>::empty()
    {
        reveal(model_decode);
    }

    proof fn lemma_model_next(buf: Seq<u8>)
        requires valid_first(buf)
        ensures model_decode(buf) == seq![buf.subrange(8, (8 + u32_at(buf, 0)) as int)]
            + model_decode(buf.subrange((8 + u32_at(buf, 0)) as int, buf.len() as int))
    {
        reveal(model_decode);
    }

    pub proof fn lemma_model_roundtrip(frames: Seq<Seq<u8>>)
        requires valid_payloads(frames)
        ensures model_decode(model_encode(frames)) == frames
        decreases frames.len()
    {
        reveal(model_encode);
        if frames.len() == 0 {
            assert(frames =~= Seq::<Seq<u8>>::empty());
            lemma_model_stops(model_encode(frames));
        } else {
            let first = frames.first();
            let tail = frames.drop_first();
            assert(0 < first.len() <= 16_777_216);
            assert(valid_payloads(tail)) by {
                assert forall|i: int| 0 <= i < tail.len()
                    implies 0 < #[trigger] tail[i].len() <= 16_777_216 by {
                    assert(tail[i] == frames[i + 1]);
                }
            }
            lemma_frame_bytes(first);
            lemma_model_roundtrip(tail);
            let head = frame_bytes(first, spec_crc32c(first));
            let bytes = head + model_encode(tail);
            assert(bytes == model_encode(frames));
            assert(bytes.subrange(0, head.len() as int) =~= head);
            assert(bytes.subrange(8, (8 + first.len()) as int) =~= first);
            assert(u32_at(bytes, 0) == first.len());
            assert(u32_at(bytes, 4) == spec_crc32c(first));
            assert(valid_first(bytes));
            lemma_model_next(bytes);
            assert(bytes.subrange((8 + first.len()) as int, bytes.len() as int)
                =~= model_encode(tail));
            assert(frames =~= seq![first] + tail);
        }
    }

    proof fn lemma_extend_model<'a>(
        previous: Seq<&'a [u8]>, payload: &'a [u8], buf: Seq<u8>, rest: Seq<u8>,
        pos: usize, len: usize,
    )
        requires
            pos <= buf.len(),
            rest == buf.subrange(pos as int, buf.len() as int),
            valid_first(rest),
            len == u32_at(rest, 0),
            payload@ == rest.subrange(8, (8 + len) as int),
            payload_views(previous) + model_decode(rest) == model_decode(buf),
        ensures
            payload_views(previous.push(payload))
                + model_decode(buf.subrange((pos + 8 + len) as int, buf.len() as int))
                == model_decode(buf),
    {
        lemma_model_next(rest);
        assert(payload_views(previous.push(payload)) =~= payload_views(previous).push(payload@));
        assert(rest.subrange((8 + len) as int, rest.len() as int)
            =~= buf.subrange((pos + 8 + len) as int, buf.len() as int));
    }

    pub open spec fn prefix_len<'a>(frames: Seq<&'a [u8]>) -> nat
        decreases frames.len()
    {
        if frames.len() == 0 {
            0
        } else {
            prefix_len(frames.drop_last()) + 8 + frames.last()@.len()
        }
    }

    pub open spec fn prefix_payloads<'a>(frames: Seq<&'a [u8]>, buf: Seq<u8>) -> bool {
        forall|i: int| 0 <= i < frames.len() ==> {
            let header = prefix_len(frames.take(i));
            let start = header + 8;
            let end = start + frames[i]@.len();
            &&& end <= buf.len()
            &&& 0 < frames[i]@.len() <= 16_777_216
            &&& u32_at(buf, header as int) == frames[i]@.len()
            &&& u32_at(buf, (header + 4) as int) == spec_crc32c(frames[i]@)
            &&& #[trigger] frames[i]@ == buf.subrange(start as int, end as int)
        }
    }

    proof fn lemma_extend_prefix<'a>(
        previous: Seq<&'a [u8]>, payload: &'a [u8], buf: Seq<u8>, pos: usize, len: usize,
    )
        requires
            prefix_payloads(previous, buf),
            pos == prefix_len(previous),
            0 < len <= 16_777_216,
            pos + 8 + len <= buf.len(),
            payload@ == buf.subrange((pos + 8) as int, (pos + 8 + len) as int),
            payload@.len() == len,
            u32_at(buf, pos as int) == len,
            u32_at(buf, (pos + 4) as int) == spec_crc32c(payload@),
        ensures prefix_payloads(previous.push(payload), buf),
    {
        let frames = previous.push(payload);
        assert forall|i: int| 0 <= i < frames.len() implies {
            let header = prefix_len(frames.take(i));
            let start = header + 8;
            let end = start + frames[i]@.len();
            &&& end <= buf.len()
            &&& 0 < frames[i]@.len() <= 16_777_216
            &&& u32_at(buf, header as int) == frames[i]@.len()
            &&& u32_at(buf, (header + 4) as int) == spec_crc32c(frames[i]@)
            &&& #[trigger] frames[i]@ == buf.subrange(start as int, end as int)
        } by {
            if i < previous.len() {
                assert(frames.take(i) =~= previous.take(i));
                assert(frames[i] == previous[i]);
            } else {
                assert(i == previous.len());
                assert(frames.take(i) =~= previous);
                assert(frames[i] == payload);
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(n =>
    requires buf.len() >= 4,
    ensures n == u32_at(buf@, 0),
))]
fn read_u32(buf: &[u8]) -> u32 {
    (buf[0] as u32) | ((buf[1] as u32) << 8) | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24)
}

/// Appends one frame carrying `payload` to `out`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Ok(()) => 0 < payload.len() <= 16_777_216
            && final(out)@ == old(out)@ + frame_bytes(payload@, spec_crc32c(payload@)),
        Err(EncodeError::Empty) => payload.len() == 0 && final(out)@ == old(out)@,
        Err(EncodeError::TooLarge(n)) => payload.len() > 16_777_216
            && n == payload.len() && final(out)@ == old(out)@,
    }
))]
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if payload.is_empty() {
        return Err(EncodeError::Empty);
    }
    if payload.len() > MAX_FRAME {
        return Err(EncodeError::TooLarge(payload.len()));
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost before = out@; }
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
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(out@ == before + frame_bytes(payload@, crc));
    }
    Ok(())
}

/// The total length of the frame at the start of `buf` when its declared length is valid and
/// its payload is all present, whatever its CRC.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(n) => 8 < n && n <= buf.len() && n <= 16_777_224,
        None => true,
    }
))]
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
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result.valid_len <= buf.len(),
        result.valid_len == prefix_len(result.frames@),
        prefix_payloads(result.frames@, buf@),
        payload_views(result.frames@) == model_decode(buf@),
        match result.stop {
            None => result.valid_len == buf.len(),
            Some(Stop::ShortHeader) =>
                0 < buf.len() - result.valid_len < 8,
            Some(Stop::ZeroLength) => buf.len() - result.valid_len >= 8
                && u32_at(buf@, result.valid_len as int) == 0,
            Some(Stop::TooLarge) => buf.len() - result.valid_len >= 8
                && u32_at(buf@, result.valid_len as int) > 16_777_216,
            Some(Stop::ShortPayload) => buf.len() - result.valid_len >= 8
                && 0 < u32_at(buf@, result.valid_len as int) <= 16_777_216
                && buf.len() - result.valid_len - 8 < u32_at(buf@, result.valid_len as int),
            Some(Stop::CrcMismatch) => buf.len() - result.valid_len >= 8
                && 0 < u32_at(buf@, result.valid_len as int) <= 16_777_216
                && buf.len() - result.valid_len - 8 >= u32_at(buf@, result.valid_len as int)
                && u32_at(buf@, (result.valid_len + 4) as int)
                    != spec_crc32c(buf@.subrange(
                        (result.valid_len + 8) as int,
                        (result.valid_len + 8 + u32_at(buf@, result.valid_len as int)) as int,
                    )),
        },
))]
pub fn decode_frames(buf: &[u8]) -> Decoded<'_> {
    let mut frames = Vec::new();
    let mut pos = 0usize;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(payload_views(frames@) =~= Seq::<Seq<u8>>::empty());
        assert(buf@.subrange(0, buf.len() as int) =~= buf@);
    }
    let stop = {
        #[cfg_attr(verus_keep_ghost, verifier::loop_isolation(false))]
        #[cfg_attr(verus_keep_ghost, verus_spec(invariant
            pos <= buf.len(),
            pos == prefix_len(frames@),
            prefix_payloads(frames@, buf@),
            payload_views(frames@) + model_decode(buf@.subrange(pos as int, buf.len() as int))
                == model_decode(buf@),
            decreases buf.len() - pos,
        ))]
        loop {
            let rest = &buf[pos..];
            if rest.is_empty() {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break None;
            }
            if rest.len() < FRAME_HEADER {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break Some(Stop::ShortHeader);
            }
            let len = read_u32(rest) as usize;
            let crc = read_u32(&rest[4..]);
            if len == 0 {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break Some(Stop::ZeroLength);
            }
            if len > MAX_FRAME {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break Some(Stop::TooLarge);
            }
            if rest.len() - FRAME_HEADER < len {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break Some(Stop::ShortPayload);
            }
            let payload = &rest[FRAME_HEADER..FRAME_HEADER + len];
            let actual_crc = crc32c(payload);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(payload@ =~= buf@.subrange((pos + 8) as int, (pos + 8 + len) as int));
                assert(crc == u32_at(buf@, (pos + 4) as int));
            }
            if actual_crc != crc {
                #[cfg(verus_keep_ghost)]
                proof! { lemma_model_stops(rest@); }
                break Some(Stop::CrcMismatch);
            }
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost previous = frames@; }
            frames.push(payload);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(frames@.drop_last() =~= previous);
                assert(frames@.last() == payload);
                assert(prefix_len(frames@) == prefix_len(previous) + 8 + payload@.len());
                assert(payload@ =~= buf@.subrange((pos + 8) as int, (pos + 8 + len) as int));
                assert(u32_at(buf@, pos as int) == len);
                lemma_extend_prefix(previous, payload, buf@, pos, len);
                lemma_extend_model(previous, payload, buf@, rest@, pos, len);
            }
            pos += FRAME_HEADER + len;
        }
    };
    Decoded {
        frames,
        valid_len: pos,
        stop,
    }
}
