//! SHA-1 (WebSocket handshake only), SHA-256 and lowercase hex, implemented here.

#![forbid(unsafe_code)]

mod sha1;
mod sha256;

pub use sha1::{Sha1, sha1};
pub use sha256::{Sha256, sha256, sha256_file};

use std::fmt;

/// The block buffering and big-endian length padding shared by SHA-1 and SHA-256.
#[derive(Clone)]
struct Blocks {
    buf: [u8; 64],
    used: usize,
    total: u64,
}

impl Blocks {
    const fn new() -> Self {
        Blocks {
            buf: [0; 64],
            used: 0,
            total: 0,
        }
    }

    fn update(&mut self, mut data: &[u8], mut compress: impl FnMut(&[u8; 64])) {
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

    fn finish(mut self, mut compress: impl FnMut(&[u8; 64])) {
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

/// Lowercase hex of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[usize::from(b >> 4)] as char);
        s.push(DIGITS[usize::from(b & 15)] as char);
    }
    s
}

/// Decodes lowercase hex. Uppercase is rejected so every byte string has one spelling.
pub fn from_hex(s: &str) -> Result<Vec<u8>, HexError> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(2) {
        return Err(HexError::OddLength(s.len()));
    }
    let digit = |i: usize| match s[i] {
        c @ b'0'..=b'9' => Ok(c - b'0'),
        c @ b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(HexError::BadDigit(i)),
    };
    (0..s.len() / 2)
        .map(|i| Ok(digit(2 * i)? << 4 | digit(2 * i + 1)?))
        .collect()
}

/// Decodes lowercase hex into a fixed-size array, such as a 32-byte SHA-256 digest.
pub fn from_hex_array<const N: usize>(s: &str) -> Result<[u8; N], HexError> {
    let v = from_hex(s)?;
    v.try_into().map_err(|v: Vec<u8>| HexError::Length {
        want: N,
        got: v.len(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexError {
    OddLength(usize),
    BadDigit(usize),
    Length { want: usize, got: usize },
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::OddLength(n) => write!(f, "hex: odd length {n}"),
            HexError::BadDigit(i) => write!(f, "hex: not a lowercase hex digit at offset {i}"),
            HexError::Length { want, got } => write!(f, "hex: want {want} bytes, got {got}"),
        }
    }
}

impl std::error::Error for HexError {}
