#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_core::codec::{decode, encode};
use pt_core_fuzz::{Bytes, gen_event};

fuzz_target!(|data: &[u8]| {
    // Arbitrary bytes: decode never panics, and what decodes is canonical.
    if let Ok(e) = decode(data) {
        assert_eq!(encode(&e), data, "decode accepted a non-canonical encoding");
    }
    // Arbitrary events: encode then decode is the identity, and every strict prefix fails.
    let mut b = Bytes::new(data);
    while !b.done() {
        let e = gen_event::event(&mut b);
        let bytes = encode(&e);
        assert_eq!(decode(&bytes), Ok(e));
        let cut = data.len() % bytes.len();
        assert!(decode(&bytes[..cut]).is_err(), "a truncated event decoded");
    }
});
