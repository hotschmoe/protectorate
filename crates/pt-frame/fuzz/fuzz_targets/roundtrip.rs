//! Frames built from the input, encoded back to back, must decode to the same frames; every
//! strict prefix of the stream yields only the frames wholly inside it and ends as Truncated.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_frame::codec::{self, Decoder, Frame, FrameError, Kind, MAX_PAYLOAD};

fn frames(mut data: &[u8]) -> Vec<Frame> {
    let mut out = Vec::new();
    while data.len() >= 11 {
        let kind = Kind::from_u8(data[0] % 5 + 1).expect("kinds 1 to 5 exist");
        let id = u64::from_le_bytes(data[1..9].try_into().expect("8 bytes"));
        let len = usize::from(u16::from_le_bytes([data[9], data[10]])).min(data.len() - 11);
        out.push(Frame {
            kind,
            id,
            payload: data[11..11 + len].to_vec(),
        });
        data = &data[11 + len..];
    }
    out
}

fn decode_all(stream: &[u8], step: usize) -> (Vec<Frame>, Result<(), FrameError>) {
    let mut dec = Decoder::new();
    let mut got = Vec::new();
    for chunk in stream.chunks(step.max(1)) {
        dec.feed(chunk);
        while let Some(f) = dec.next_frame().expect("our own encoding decodes") {
            got.push(f);
        }
    }
    (got, dec.finish())
}

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    if flags == 0xFF {
        let big = vec![0u8; MAX_PAYLOAD + 1];
        assert!(codec::encode(Kind::Chunk, 1, &big).is_err());
        assert!(codec::encode(Kind::Chunk, 1, &big[..MAX_PAYLOAD]).is_ok());
    }
    let want = frames(rest);
    let mut stream = Vec::new();
    let mut ends = Vec::new();
    for f in &want {
        codec::encode_into(f.kind, f.id, &f.payload, &mut stream).expect("small payloads encode");
        ends.push(stream.len());
    }
    let (got, fin) = decode_all(&stream, usize::from(flags % 32) + 1);
    assert_eq!(got, want);
    assert_eq!(fin, Ok(()));

    for cut in [stream.len().saturating_sub(1), stream.len() / 2, 1] {
        if cut == 0 || cut >= stream.len() {
            continue;
        }
        let whole = ends.iter().filter(|&&e| e <= cut).count();
        let last_end = if whole == 0 { 0 } else { ends[whole - 1] };
        let (got, fin) = decode_all(&stream[..cut], 7);
        assert_eq!(got[..], want[..whole]);
        if cut == last_end {
            assert_eq!(fin, Ok(()));
        } else {
            assert_eq!(fin, Err(FrameError::Truncated(cut - last_end)));
        }
    }
});
