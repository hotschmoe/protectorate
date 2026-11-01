//! Our inflate against miniz_oxide's. The first byte picks the case:
//! - arbitrary bytes as a raw DEFLATE stream: both decoders agree on the output, or both fail;
//! - the rest compressed by miniz_oxide at a level from the first byte: it inflates back exactly;
//! - the rest wrapped as one gzip member (stored, then miniz_oxide's deflate): `gunzip` returns
//!   it, and a flipped CRC, a cut trailer or a trailing byte is refused.
//!
//! Never a panic, and never more output than the limit.
#![no_main]

use libfuzzer_sys::fuzz_target;
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec_with_limit;

const LIMIT: usize = 1 << 20;

fn gzip(deflated: &[u8], data: &[u8]) -> Vec<u8> {
    let mut g = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
    g.extend_from_slice(deflated);
    g.extend_from_slice(&pt_embed::crc32(data).to_le_bytes());
    g.extend_from_slice(&(data.len() as u32).to_le_bytes());
    g
}

fuzz_target!(|input: &[u8]| {
    let Some((&mode, data)) = input.split_first() else {
        return;
    };
    match mode % 3 {
        0 => {
            let ours = pt_embed::inflate(data, LIMIT);
            let theirs = decompress_to_vec_with_limit(data, LIMIT);
            if let Ok((out, used)) = &ours {
                assert!(out.len() <= LIMIT && *used <= data.len());
            }
            match (ours, theirs) {
                (Ok((a, _)), Ok(b)) => assert_eq!(a, b, "the decoders disagree on the output"),
                (Err(_), Err(_)) => {}
                (a, b) => panic!(
                    "one decoder accepts: ours {:?}, miniz_oxide {:?}",
                    a.map(|(o, _)| o.len()),
                    b.map(|o| o.len()).map_err(|e| e.status)
                ),
            }
        }
        1 => {
            let packed = compress_to_vec(data, mode / 3 % 11);
            let (out, used) = pt_embed::inflate(&packed, LIMIT).expect("miniz_oxide's output inflates");
            assert_eq!(out, data);
            assert_eq!(used, packed.len());
        }
        _ => {
            let mut stored = Vec::new();
            for (i, c) in data.chunks(1000).enumerate() {
                let last = (i + 1) * 1000 >= data.len();
                stored.push(u8::from(last));
                stored.extend_from_slice(&(c.len() as u16).to_le_bytes());
                stored.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
                stored.extend_from_slice(c);
            }
            if data.is_empty() {
                stored = vec![1, 0, 0, 0xff, 0xff];
            }
            for deflated in [stored, compress_to_vec(data, 6)] {
                let g = gzip(&deflated, data);
                assert_eq!(pt_embed::gunzip(&g, LIMIT).expect("a gzip member"), data);
                let mut bad = g.clone();
                let n = bad.len();
                bad[n - 8] ^= 1;
                assert!(pt_embed::gunzip(&bad, LIMIT).is_err(), "a bad CRC passed");
                assert!(pt_embed::gunzip(&g[..n - 1], LIMIT).is_err(), "a cut trailer passed");
                let mut more = g.clone();
                more.push(0);
                assert!(pt_embed::gunzip(&more, LIMIT).is_err(), "trailing data passed");
            }
        }
    }
});
