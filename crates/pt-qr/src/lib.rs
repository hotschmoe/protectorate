//! QR codes for a terminal: byte mode, error correction level L or M, drawn with Unicode half
//! blocks. A trimmed port of the Rust version of Project Nayuki's qrcodegen 1.8.0 (MIT; the notice
//! is in LICENSE). Kept from upstream: the version search, Reed-Solomon codewords, interleaving,
//! the function patterns and the penalty-scored mask choice. Dropped: numeric, alphanumeric,
//! kanji and ECI segments, levels Q and H, the level boost, forced masks and version ranges.
//!
//! Copyright (c) Project Nayuki. (MIT License)
//! https://www.nayuki.io/page/qr-code-generator-library

#![forbid(unsafe_code)]

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ecc {
    /// Recovers about 7% of the codewords.
    Low,
    /// Recovers about 15% of the codewords.
    Medium,
}

impl Ecc {
    fn ordinal(self) -> usize {
        match self {
            Ecc::Low => 0,
            Ecc::Medium => 1,
        }
    }

    fn format_bits(self) -> u32 {
        match self {
            Ecc::Low => 1,
            Ecc::Medium => 0,
        }
    }
}

/// The data does not fit version 40 at the chosen level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooLong {
    pub bytes: usize,
    pub max: usize,
}

impl fmt::Display for TooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bytes do not fit a QR code (at most {})",
            self.bytes, self.max
        )
    }
}

impl std::error::Error for TooLong {}

pub const MIN_VERSION: u8 = 1;
pub const MAX_VERSION: u8 = 40;

/// The most bytes one code holds at `ecc` (version 40).
pub fn max_bytes(ecc: Ecc) -> usize {
    (data_codewords(MAX_VERSION, ecc) * 8 - 4 - 16) / 8
}

#[derive(Clone, PartialEq, Eq)]
pub struct QrCode {
    version: u8,
    size: i32,
    mask: u8,
    modules: Vec<bool>,
}

impl fmt::Debug for QrCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QrCode(v{}, mask {})", self.version, self.mask)
    }
}

/// Encodes `data` as one byte-mode segment in the smallest version that holds it.
pub fn encode(data: &[u8], ecc: Ecc) -> Result<QrCode, TooLong> {
    let mut version = MIN_VERSION;
    let used_bits = loop {
        let count_bits = byte_count_bits(version);
        let fits_count = data.len() < (1usize << count_bits);
        let used = 4 + count_bits as usize + data.len().saturating_mul(8);
        if fits_count && used <= data_codewords(version, ecc) * 8 {
            break used;
        }
        if version == MAX_VERSION {
            return Err(TooLong {
                bytes: data.len(),
                max: max_bytes(ecc),
            });
        }
        version += 1;
    };

    let capacity = data_codewords(version, ecc) * 8;
    let mut bits = Bits::default();
    bits.push(0b0100, 4);
    bits.push(data.len() as u32, byte_count_bits(version));
    for &b in data {
        bits.push(u32::from(b), 8);
    }
    debug_assert_eq!(bits.len, used_bits);
    bits.push(0, (capacity - bits.len).min(4) as u8);
    bits.push(0, (bits.len.wrapping_neg() & 7) as u8);
    for &pad in [0xEC, 0x11].iter().cycle() {
        if bits.len >= capacity {
            break;
        }
        bits.push(pad, 8);
    }
    Ok(QrCode::from_codewords(version, ecc, &bits.bytes))
}

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    len: usize,
}

impl Bits {
    fn push(&mut self, val: u32, n: u8) {
        debug_assert!(n <= 31 && val >> n == 0);
        for i in (0..n).rev() {
            if self.len.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (val >> i) & 1 != 0 {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 0x80 >> (self.len % 8);
            }
            self.len += 1;
        }
    }
}

fn byte_count_bits(version: u8) -> u8 {
    if version <= 9 { 8 } else { 16 }
}

struct Builder {
    size: i32,
    modules: Vec<bool>,
    function: Vec<bool>,
}

impl QrCode {
    fn from_codewords(version: u8, ecc: Ecc, data: &[u8]) -> QrCode {
        let size = i32::from(version) * 4 + 17;
        let cells = (size * size) as usize;
        let mut b = Builder {
            size,
            modules: vec![false; cells],
            function: vec![false; cells],
        };
        b.draw_function_patterns(version, ecc);
        let all = add_ecc_and_interleave(version, ecc, data);
        b.draw_codewords(&all);

        let mut best = (i32::MAX, 0u8);
        for mask in 0..8u8 {
            b.apply_mask(mask);
            b.draw_format_bits(ecc, mask);
            let penalty = b.penalty_score();
            if penalty < best.0 {
                best = (penalty, mask);
            }
            b.apply_mask(mask);
        }
        let mask = best.1;
        b.apply_mask(mask);
        b.draw_format_bits(ecc, mask);
        QrCode {
            version,
            size,
            mask,
            modules: b.modules,
        }
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    /// Modules per side, without a quiet zone.
    pub fn size(&self) -> usize {
        self.size as usize
    }

    pub fn mask(&self) -> u8 {
        self.mask
    }

    /// Whether the module at column `x`, row `y` is dark; outside the code is light.
    pub fn dark(&self, x: i32, y: i32) -> bool {
        (0..self.size).contains(&x)
            && (0..self.size).contains(&y)
            && self.modules[(y * self.size + x) as usize]
    }

    /// The code with a `quiet`-module light border, two module rows per text line. Light modules
    /// are drawn as blocks, dark ones as spaces. With `ansi` each line forces a white foreground on
    /// a black background, so any terminal theme shows dark-on-light; without it, the terminal's
    /// own colors must be light text on a dark background.
    pub fn to_terminal(&self, quiet: u8, ansi: bool) -> String {
        let q = i32::from(quiet);
        let (lo, hi) = (-q, self.size + q);
        let light = |x, y| y < hi && !self.dark(x, y);
        let mut out = String::new();
        let mut y = lo;
        while y < hi {
            if ansi {
                out.push_str("\x1b[97;40m");
            }
            for x in lo..hi {
                out.push(match (light(x, y), light(x, y + 1)) {
                    (true, true) => '\u{2588}',
                    (true, false) => '\u{2580}',
                    (false, true) => '\u{2584}',
                    (false, false) => ' ',
                });
            }
            if ansi {
                out.push_str("\x1b[0m");
            }
            out.push('\n');
            y += 2;
        }
        out
    }
}

impl Builder {
    fn set_function(&mut self, x: i32, y: i32, dark: bool) {
        let i = (y * self.size + x) as usize;
        self.modules[i] = dark;
        self.function[i] = true;
    }

    fn draw_function_patterns(&mut self, version: u8, ecc: Ecc) {
        let size = self.size;
        for i in 0..size {
            self.set_function(6, i, i % 2 == 0);
            self.set_function(i, 6, i % 2 == 0);
        }
        self.draw_finder(3, 3);
        self.draw_finder(size - 4, 3);
        self.draw_finder(3, size - 4);
        let pos = alignment_positions(version, size);
        let n = pos.len();
        for i in 0..n {
            for j in 0..n {
                let corner = (i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0);
                if !corner {
                    self.draw_alignment(pos[i], pos[j]);
                }
            }
        }
        self.draw_format_bits(ecc, 0);
        self.draw_version(version);
    }

    fn draw_format_bits(&mut self, ecc: Ecc, mask: u8) {
        let data = ecc.format_bits() << 3 | u32::from(mask);
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = (data << 10 | rem) ^ 0x5412;
        for i in 0..6 {
            self.set_function(8, i, bit(bits, i));
        }
        self.set_function(8, 7, bit(bits, 6));
        self.set_function(8, 8, bit(bits, 7));
        self.set_function(7, 8, bit(bits, 8));
        for i in 9..15 {
            self.set_function(14 - i, 8, bit(bits, i));
        }
        let size = self.size;
        for i in 0..8 {
            self.set_function(size - 1 - i, 8, bit(bits, i));
        }
        for i in 8..15 {
            self.set_function(8, size - 15 + i, bit(bits, i));
        }
        self.set_function(8, size - 8, true);
    }

    fn draw_version(&mut self, version: u8) {
        if version < 7 {
            return;
        }
        let data = u32::from(version);
        let mut rem = data;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = data << 12 | rem;
        for i in 0..18 {
            let dark = bit(bits, i);
            let a = self.size - 11 + i % 3;
            let b = i / 3;
            self.set_function(a, b, dark);
            self.set_function(b, a, dark);
        }
    }

    fn draw_finder(&mut self, x: i32, y: i32) {
        for dy in -4..=4 {
            for dx in -4..=4 {
                let (xx, yy) = (x + dx, y + dy);
                if (0..self.size).contains(&xx) && (0..self.size).contains(&yy) {
                    let dist = dx.abs().max(dy.abs());
                    self.set_function(xx, yy, dist != 2 && dist != 4);
                }
            }
        }
    }

    fn draw_alignment(&mut self, x: i32, y: i32) {
        for dy in -2..=2 {
            for dx in -2..=2 {
                self.set_function(x + dx, y + dy, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    fn draw_codewords(&mut self, data: &[u8]) {
        let size = self.size;
        let mut i = 0usize;
        let mut right = size - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..size {
                for j in 0..2 {
                    let x = right - j;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { size - 1 - vert } else { vert };
                    let idx = (y * size + x) as usize;
                    if !self.function[idx] && i < data.len() * 8 {
                        self.modules[idx] = bit(u32::from(data[i >> 3]), 7 - (i as i32 & 7));
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
        debug_assert_eq!(i, data.len() * 8);
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let invert = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => x * y % 2 + x * y % 3 == 0,
                    6 => (x * y % 2 + x * y % 3) % 2 == 0,
                    _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
                };
                let idx = (y * self.size + x) as usize;
                self.modules[idx] ^= invert & !self.function[idx];
            }
        }
    }

    fn module(&self, x: i32, y: i32) -> bool {
        self.modules[(y * self.size + x) as usize]
    }

    fn penalty_score(&self) -> i32 {
        let size = self.size;
        let mut result = 0;
        for transpose in [false, true] {
            for a in 0..size {
                let mut color = false;
                let mut run = 0;
                let mut history = FinderPenalty::new(size);
                for b in 0..size {
                    let m = if transpose {
                        self.module(a, b)
                    } else {
                        self.module(b, a)
                    };
                    if m == color {
                        run += 1;
                        if run == 5 {
                            result += PENALTY_N1;
                        } else if run > 5 {
                            result += 1;
                        }
                    } else {
                        history.add(run);
                        if !color {
                            result += history.count_patterns() * PENALTY_N3;
                        }
                        color = m;
                        run = 1;
                    }
                }
                result += history.terminate_and_count(color, run) * PENALTY_N3;
            }
        }
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let c = self.module(x, y);
                if c == self.module(x + 1, y)
                    && c == self.module(x, y + 1)
                    && c == self.module(x + 1, y + 1)
                {
                    result += PENALTY_N2;
                }
            }
        }
        let dark: i32 = self.modules.iter().map(|&m| i32::from(m)).sum();
        let total = size * size;
        let k = ((dark * 20 - total * 10).abs() + total - 1) / total - 1;
        result + k * PENALTY_N4
    }
}

struct FinderPenalty {
    size: i32,
    history: [i32; 7],
}

impl FinderPenalty {
    fn new(size: i32) -> FinderPenalty {
        FinderPenalty {
            size,
            history: [0; 7],
        }
    }

    fn add(&mut self, mut run: i32) {
        if self.history[0] == 0 {
            run += self.size;
        }
        self.history.copy_within(0..6, 1);
        self.history[0] = run;
    }

    fn count_patterns(&self) -> i32 {
        let h = &self.history;
        let n = h[1];
        let core = n > 0 && h[2] == n && h[3] == n * 3 && h[4] == n && h[5] == n;
        i32::from(core && h[0] >= n * 4 && h[6] >= n)
            + i32::from(core && h[6] >= n * 4 && h[0] >= n)
    }

    fn terminate_and_count(mut self, color: bool, mut run: i32) -> i32 {
        if color {
            self.add(run);
            run = 0;
        }
        run += self.size;
        self.add(run);
        self.count_patterns()
    }
}

fn bit(x: u32, i: i32) -> bool {
    (x >> i) & 1 != 0
}

fn alignment_positions(version: u8, size: i32) -> Vec<i32> {
    if version == 1 {
        return Vec::new();
    }
    let n = i32::from(version) / 7 + 2;
    let step = if version == 32 {
        26
    } else {
        (i32::from(version) * 4 + n * 2 + 1) / (n * 2 - 2) * 2
    };
    let mut out: Vec<i32> = (0..n - 1).map(|i| size - 7 - i * step).collect();
    out.push(6);
    out.reverse();
    out
}

fn raw_data_modules(version: u8) -> usize {
    let v = usize::from(version);
    let mut result = (16 * v + 128) * v + 64;
    if v >= 2 {
        let n = v / 7 + 2;
        result -= (25 * n - 10) * n - 55;
        if v >= 7 {
            result -= 36;
        }
    }
    result
}

fn table(t: &[[u8; 41]; 2], version: u8, ecc: Ecc) -> usize {
    usize::from(t[ecc.ordinal()][usize::from(version)])
}

fn data_codewords(version: u8, ecc: Ecc) -> usize {
    raw_data_modules(version) / 8
        - table(&ECC_CODEWORDS_PER_BLOCK, version, ecc) * table(&ECC_BLOCKS, version, ecc)
}

fn add_ecc_and_interleave(version: u8, ecc: Ecc, data: &[u8]) -> Vec<u8> {
    debug_assert_eq!(data.len(), data_codewords(version, ecc));
    let blocks_n = table(&ECC_BLOCKS, version, ecc);
    let ecc_len = table(&ECC_CODEWORDS_PER_BLOCK, version, ecc);
    let raw = raw_data_modules(version) / 8;
    let short_n = blocks_n - raw % blocks_n;
    let short_len = raw / blocks_n;
    let divisor = rs_divisor(ecc_len);
    let mut blocks = Vec::with_capacity(blocks_n);
    let mut k = 0;
    for i in 0..blocks_n {
        let len = short_len - ecc_len + usize::from(i >= short_n);
        let mut block = data[k..k + len].to_vec();
        k += len;
        let rem = rs_remainder(&block, &divisor);
        if i < short_n {
            block.push(0);
        }
        block.extend_from_slice(&rem);
        blocks.push(block);
    }
    let mut out = Vec::with_capacity(raw);
    for i in 0..=short_len {
        for (j, block) in blocks.iter().enumerate() {
            if i != short_len - ecc_len || j >= short_n {
                out.push(block[i]);
            }
        }
    }
    out
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree - 1];
    result.push(1);
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = rs_multiply(result[j], root);
            if j + 1 < result.len() {
                result[j] ^= result[j + 1];
            }
        }
        root = rs_multiply(root, 0x02);
    }
    result
}

fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &b in data {
        let factor = b ^ result[0];
        result.rotate_left(1);
        let last = result.len() - 1;
        result[last] = 0;
        for (x, &y) in result.iter_mut().zip(divisor) {
            *x ^= rs_multiply(y, factor);
        }
    }
    result
}

fn rs_multiply(x: u8, y: u8) -> u8 {
    let mut z = 0u8;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x1D);
        z ^= ((y >> i) & 1) * x;
    }
    z
}

const PENALTY_N1: i32 = 3;
const PENALTY_N2: i32 = 3;
const PENALTY_N3: i32 = 40;
const PENALTY_N4: i32 = 10;

// Index 0 is unused; rows are levels L and M.
static ECC_CODEWORDS_PER_BLOCK: [[u8; 41]; 2] = [
    [
        0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28,
        30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28,
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    ],
];

static ECC_BLOCKS: [[u8; 41]; 2] = [
    [
        0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13,
        14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
    ],
    [
        0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21,
        23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
    ],
];
