/// The block buffering and big-endian length padding shared by SHA-1 and SHA-256.
#[derive(Clone)]
pub(crate) struct Blocks {
    buf: [u8; 64],
    used: usize,
    total: u64,
}

impl Blocks {
    pub(crate) const fn new() -> Self {
        Blocks {
            buf: [0; 64],
            used: 0,
            total: 0,
        }
    }

    pub(crate) fn update(&mut self, mut data: &[u8], mut compress: impl FnMut(&[u8; 64])) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.used > 0 {
            let n = data.len().min(64 - self.used);
            self.buf[self.used..self.used + n].copy_from_slice(&data[..n]);
            self.used += n;
            data = &data[n..];
            if self.used < 64 {
                return;
            }
            compress(&self.buf);
            self.used = 0;
        }
        let (blocks, rest) = data.as_chunks::<64>();
        for block in blocks {
            compress(block);
        }
        self.buf[..rest.len()].copy_from_slice(rest);
        self.used = rest.len();
    }

    pub(crate) fn finish(mut self, mut compress: impl FnMut(&[u8; 64])) {
        let bits = self.total.wrapping_mul(8);
        self.buf[self.used] = 0x80;
        self.buf[self.used + 1..].fill(0);
        if self.used >= 56 {
            compress(&self.buf);
            self.buf = [0; 64];
        }
        self.buf[56..].copy_from_slice(&bits.to_be_bytes());
        compress(&self.buf);
    }
}
