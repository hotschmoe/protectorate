//! SHA-1, SHA-256 and hex against the `sha1` and `sha2` crates, with random update chunking.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sha1::Digest;

fuzz_target!(|data: &[u8]| {
    let (seed, msg) = data.split_at(data.len().min(8));
    let mut x = seed.iter().fold(0x9e37_79b9_7f4a_7c15u64, |x, &b| (x ^ u64::from(b)).rotate_left(9));
    let (mut ours256, mut ours1) = (pt_hash::Sha256::new(), pt_hash::Sha1::new());
    let (mut ref256, mut ref1) = (sha2::Sha256::new(), sha1::Sha1::new());
    let mut rest = msg;
    while !rest.is_empty() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 130) as usize;
        let (chunk, tail) = rest.split_at(n.min(rest.len()));
        ours256.update(chunk);
        ours1.update(chunk);
        ref256.update(chunk);
        ref1.update(chunk);
        rest = tail;
        if x % 7 == 0 {
            assert_eq!(ours256.finalize()[..], ref256.clone().finalize()[..]);
        }
    }
    let digest = ours256.finalize();
    assert_eq!(digest[..], ref256.finalize()[..]);
    assert_eq!(ours1.finalize()[..], ref1.finalize()[..]);
    assert_eq!(pt_hash::sha256(msg)[..], sha2::Sha256::digest(msg)[..]);
    assert_eq!(pt_hash::sha1(msg)[..], sha1::Sha1::digest(msg)[..]);

    let hex = pt_hash::to_hex(msg);
    assert_eq!(pt_hash::from_hex(&hex).unwrap(), msg);
    assert_eq!(pt_hash::from_hex_array::<32>(&pt_hash::to_hex(&digest)).unwrap(), digest);
    if let Ok(s) = std::str::from_utf8(msg) {
        let lower = s.len() % 2 == 0 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
        assert_eq!(pt_hash::from_hex(s).is_ok(), lower);
    }
});
