//! Arbitrary bytes into the decoder, fed in chunks whose size comes from the first byte: never
//! a panic, never more buffered than one partial frame, every frame re-encodes to exactly the
//! bytes it came from, and chunked decoding agrees with one-shot `decode`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_frame::codec::{self, Decoder, FrameError, MAX_FRAME, MAX_PAYLOAD};

fuzz_target!(|data: &[u8]| {
    let Some((&step, input)) = data.split_first() else {
        return;
    };
    let step = usize::from(step % 64) + 1;

    let mut oneshot = Vec::new();
    let mut at = 0;
    let oneshot_err = loop {
        match codec::decode(&input[at..]) {
            Ok(Some((f, used))) => {
                assert!(used <= input.len() - at);
                oneshot.push((f, at, used));
                at += used;
            }
            Ok(None) => break None,
            Err(e) => break Some(e),
        }
    };

    let mut dec = Decoder::new();
    let mut frames = Vec::new();
    let mut err = None;
    'feed: for chunk in input.chunks(step) {
        dec.feed(chunk);
        loop {
            match dec.next_frame() {
                Ok(Some(f)) => frames.push(f),
                Ok(None) => break,
                Err(e) => {
                    assert_eq!(dec.next_frame(), Err(e), "a failed decoder must stay failed");
                    err = Some(e);
                    break 'feed;
                }
            }
        }
        assert!(
            dec.buffered() < 4 + MAX_FRAME,
            "more than one partial frame buffered"
        );
    }

    assert_eq!(err, oneshot_err, "chunked and one-shot errors differ");
    assert_eq!(
        frames.len(),
        oneshot.len(),
        "chunked and one-shot frame counts differ"
    );
    for (f, (g, start, used)) in frames.iter().zip(&oneshot) {
        assert_eq!(f, g);
        assert!(f.payload.len() <= MAX_PAYLOAD);
        let bytes = codec::encode(f.kind, f.id, &f.payload).expect("a decoded frame encodes");
        assert_eq!(
            &bytes[..],
            &input[*start..start + used],
            "re-encoding differs from input"
        );
    }
    match (err, dec.finish()) {
        (Some(e), r) => assert_eq!(r, Err(e)),
        (None, Ok(())) => assert_eq!(at, input.len()),
        (None, Err(FrameError::Truncated(n))) => assert_eq!(n, input.len() - at),
        (None, Err(e)) => panic!("finish gave {e} without a decode error"),
    }
});
