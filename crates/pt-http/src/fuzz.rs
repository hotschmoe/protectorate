use std::io::{self, Read};

use crate::handshake::check_handshake;
use crate::wire::{MAX_HEADERS, framing, parse_request, read_body, read_head};
use crate::ws::base64;

/// Parses pipelined requests from `data`, fed in small reads, and checks the limits hold.
pub fn fuzz_request(data: &[u8]) {
    const HEAD: usize = 2048;
    const BODY: usize = 4096;
    struct Trickle<'a>(&'a [u8], usize);
    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.0.len()).min(1 + self.1 % 61);
            self.1 = self.1.wrapping_mul(31).wrapping_add(7);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let seed = data.first().copied().unwrap_or(0) as usize;
    let mut r = io::BufReader::with_capacity(97, Trickle(data, seed));
    for _ in 0..64 {
        let Ok(Some(head)) = read_head(&mut r, HEAD) else {
            return;
        };
        assert!(head.start.len() <= HEAD);
        assert!(head.headers.0.len() <= MAX_HEADERS);
        let Ok(req) = parse_request(head) else { return };
        assert!(req.path().len() <= HEAD);
        let hosts = req.headers.get_all("Host").count();
        assert!(
            hosts == 1 || (req.http10 && hosts == 0),
            "{hosts} Host fields"
        );
        let _ = req.query_param("a");
        if check_handshake(&req).is_ok() {
            assert_canonical_key(req.header("Sec-WebSocket-Key").unwrap_or(""));
        }
        let Ok(framing) = framing(&req.headers) else {
            return;
        };
        match read_body(&mut r, framing, BODY, HEAD) {
            Ok(body) => assert!(body.len() <= BODY),
            Err(_) => return,
        }
    }
}

/// An accepted Sec-WebSocket-Key decodes to 16 bytes that encode back to the same text.
fn assert_canonical_key(key: &str) {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut bytes, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in key.trim_end_matches('=').bytes() {
        let v = A.iter().position(|&a| a == c).expect("base64 alphabet");
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    assert_eq!(bytes.len(), 16, "{key}");
    assert_eq!(base64(&bytes), key);
}
