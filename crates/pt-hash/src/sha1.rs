use crate::Blocks;

const H0: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

/// Incremental SHA-1. Only for the WebSocket handshake; never for integrity.
#[derive(Clone)]
pub struct Sha1 {
    state: [u32; 5],
    blocks: Blocks,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1 {
    pub const fn new() -> Self {
        Sha1 {
            state: H0,
            blocks: Blocks::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) -> &mut Self {
        let state = &mut self.state;
        self.blocks.update(data, |b| compress(state, b));
        self
    }

    pub fn finalize(&self) -> [u8; 20] {
        let mut state = self.state;
        self.blocks.clone().finish(|b| compress(&mut state, b));
        let mut out = [0; 20];
        for (o, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
            *o = s.to_be_bytes();
        }
        out
    }
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
    Sha1::new().update(data).finalize()
}

fn compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for (w, b) in w.iter_mut().zip(block.as_chunks::<4>().0) {
        *w = u32::from_be_bytes(*b);
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (i, wi) in w.iter().enumerate() {
        let (f, k) = match i {
            0..20 => ((b & c) | (!b & d), 0x5a827999),
            20..40 => (b ^ c ^ d, 0x6ed9eba1),
            40..60 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6),
        };
        let t = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*wi);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = t;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}
