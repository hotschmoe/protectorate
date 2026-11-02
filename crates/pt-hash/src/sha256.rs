use crate::blocks::Blocks;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Incremental SHA-256: `Sha256::new().update(a).update(b).finalize()`.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    blocks: Blocks,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub const fn new() -> Self {
        Sha256 {
            state: H0,
            blocks: Blocks::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) -> &mut Self {
        let state = &mut self.state;
        self.blocks.update(data, |b| compress(state, b));
        self
    }

    /// The digest of everything fed so far; the hasher stays usable.
    pub fn finalize(&self) -> [u8; 32] {
        let mut state = self.state;
        self.blocks.clone().finish(|b| compress(&mut state, b));
        let mut out = [0; 32];
        for (o, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
            *o = s.to_be_bytes();
        }
        out
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::new().update(data).finalize()
}

/// Streams a file through SHA-256.
pub fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let ctx = |e: io::Error| io::Error::new(e.kind(), format!("sha256 {}: {e}", path.display()));
    let mut file = File::open(path).map_err(ctx)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 256 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(hasher.finalize()),
            Ok(n) => {
                hasher.update(&buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(ctx(e)),
        }
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 16];
    for (w, b) in w.iter_mut().zip(block.as_chunks::<4>().0) {
        *w = u32::from_be_bytes(*b);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    // One round with the eight working variables named in rotated order, so nothing is shuffled.
    macro_rules! round {
        ($a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident, $h:ident, $k:expr, $w:expr) => {
            let s1 = $e.rotate_right(6) ^ $e.rotate_right(11) ^ $e.rotate_right(25);
            let ch = ($e & $f) ^ (!$e & $g);
            let t1 = $h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add($k.wrapping_add($w));
            let s0 = $a.rotate_right(2) ^ $a.rotate_right(13) ^ $a.rotate_right(22);
            let maj = ($a & $b) ^ ($a & $c) ^ ($b & $c);
            $d = $d.wrapping_add(t1);
            $h = t1.wrapping_add(s0.wrapping_add(maj));
        };
    }
    macro_rules! eight {
        ($k:expr, $w:ident) => {
            round!(a, b, c, d, e, f, g, h, $k[0], $w[0]);
            round!(h, a, b, c, d, e, f, g, $k[1], $w[1]);
            round!(g, h, a, b, c, d, e, f, $k[2], $w[2]);
            round!(f, g, h, a, b, c, d, e, $k[3], $w[3]);
            round!(e, f, g, h, a, b, c, d, $k[4], $w[4]);
            round!(d, e, f, g, h, a, b, c, $k[5], $w[5]);
            round!(c, d, e, f, g, h, a, b, $k[6], $w[6]);
            round!(b, c, d, e, f, g, h, a, $k[7], $w[7]);
        };
    }
    for (i, k) in K.as_chunks::<16>().0.iter().enumerate() {
        if i > 0 {
            for j in 0..16 {
                let (w15, w2) = (w[(j + 1) & 15], w[(j + 14) & 15]);
                let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
                let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
                w[j] = w[j]
                    .wrapping_add(s0)
                    .wrapping_add(w[(j + 9) & 15])
                    .wrapping_add(s1);
            }
        }
        let (k0, k1) = k.split_at(8);
        let (w0, w1) = w.split_at(8);
        eight!(k0, w0);
        eight!(k1, w1);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}
