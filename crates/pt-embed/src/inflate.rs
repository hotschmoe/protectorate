//! gzip (RFC 1952) around DEFLATE (RFC 1951), decode only, after zlib's `puff`: canonical
//! Huffman codes decoded a bit at a time. Code sets follow zlib's rules: the code-length code
//! must be complete; a literal/length or distance code may be incomplete only when it has a
//! single code of length 1. Output never grows past the caller's limit.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InflateError(pub &'static str);

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for InflateError {}

type Result<T> = std::result::Result<T, InflateError>;

const MAXBITS: usize = 15;
const MAXLCODES: usize = 286;
const MAXDCODES: usize = 30;
const FIXLCODES: usize = 288;

const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl Bits<'_> {
    /// `need` bits, at most 16, least significant first.
    fn bits(&mut self, need: u32) -> Result<u32> {
        while self.cnt < need {
            let b = *self
                .data
                .get(self.pos)
                .ok_or(InflateError("the stream ends early"))?;
            self.pos += 1;
            self.buf |= u32::from(b) << self.cnt;
            self.cnt += 8;
        }
        let v = self.buf & ((1u32 << need) - 1);
        self.buf >>= need;
        self.cnt -= need;
        Ok(v)
    }

    /// Drops the rest of the current byte.
    fn align(&mut self) {
        self.buf = 0;
        self.cnt = 0;
    }
}

struct Huffman {
    /// Codes of each length.
    count: [u16; MAXBITS + 1],
    /// Symbols ordered by code.
    symbol: Vec<u16>,
}

impl Huffman {
    /// The code for `lengths`, and how many codes it leaves unused: negative when
    /// over-subscribed, 0 when complete.
    fn new(lengths: &[u8]) -> (Huffman, i32) {
        let mut count = [0u16; MAXBITS + 1];
        for &l in lengths {
            count[usize::from(l)] += 1;
        }
        let mut h = Huffman {
            count,
            symbol: vec![0; lengths.len()],
        };
        if usize::from(count[0]) == lengths.len() {
            return (h, 0);
        }
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left <<= 1;
            left -= i32::from(c);
            if left < 0 {
                return (h, left);
            }
        }
        let mut offs = [0usize; MAXBITS + 1];
        for len in 1..MAXBITS {
            offs[len + 1] = offs[len] + usize::from(count[len]);
        }
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                h.symbol[offs[usize::from(l)]] = sym as u16;
                offs[usize::from(l)] += 1;
            }
        }
        (h, left)
    }

    /// zlib's rule for literal/length and distance codes.
    fn usable(lengths: &[u8]) -> Result<Huffman> {
        let (h, left) = Huffman::new(lengths);
        let only_ones = usize::from(h.count[0]) + usize::from(h.count[1]) == lengths.len();
        if left < 0 || (left > 0 && !only_ones) {
            return Err(InflateError("an over-subscribed or incomplete code"));
        }
        Ok(h)
    }

    fn decode(&self, s: &mut Bits<'_>) -> Result<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..=MAXBITS {
            code |= s.bits(1)? as i32;
            let count = i32::from(self.count[len]);
            if code - count < first {
                return Ok(self.symbol[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError("a code that the block does not define"))
    }
}

fn push(out: &mut Vec<u8>, limit: usize, b: u8) -> Result<()> {
    if out.len() >= limit {
        return Err(InflateError("the output is over the limit"));
    }
    out.push(b);
    Ok(())
}

fn stored(s: &mut Bits<'_>, out: &mut Vec<u8>, limit: usize) -> Result<()> {
    s.align();
    let head = s
        .data
        .get(s.pos..s.pos + 4)
        .ok_or(InflateError("the stream ends early"))?;
    let len = u16::from_le_bytes([head[0], head[1]]);
    let nlen = u16::from_le_bytes([head[2], head[3]]);
    if len != !nlen {
        return Err(InflateError(
            "a stored block's length does not match its complement",
        ));
    }
    s.pos += 4;
    let len = usize::from(len);
    let bytes = s
        .data
        .get(s.pos..s.pos + len)
        .ok_or(InflateError("the stream ends early"))?;
    if out.len() + len > limit {
        return Err(InflateError("the output is over the limit"));
    }
    out.extend_from_slice(bytes);
    s.pos += len;
    Ok(())
}

fn codes(
    s: &mut Bits<'_>,
    out: &mut Vec<u8>,
    limit: usize,
    lencode: &Huffman,
    distcode: &Huffman,
) -> Result<()> {
    loop {
        let sym = usize::from(lencode.decode(s)?);
        if sym < 256 {
            push(out, limit, sym as u8)?;
            continue;
        }
        if sym == 256 {
            return Ok(());
        }
        let sym = sym - 257;
        if sym >= LBASE.len() {
            return Err(InflateError("an invalid length code"));
        }
        let len = usize::from(LBASE[sym]) + s.bits(u32::from(LEXT[sym]))? as usize;
        let dsym = usize::from(distcode.decode(s)?);
        if dsym >= DBASE.len() {
            return Err(InflateError("an invalid distance code"));
        }
        let dist = usize::from(DBASE[dsym]) + s.bits(u32::from(DEXT[dsym]))? as usize;
        if dist > out.len() {
            return Err(InflateError("a distance before the start of the output"));
        }
        if out.len() + len > limit {
            return Err(InflateError("the output is over the limit"));
        }
        let from = out.len() - dist;
        for i in 0..len {
            let b = out[from + i];
            out.push(b);
        }
    }
}

fn fixed() -> (Huffman, Huffman) {
    let mut lengths = [0u8; FIXLCODES];
    for (i, l) in lengths.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let (lencode, _) = Huffman::new(&lengths);
    let (distcode, _) = Huffman::new(&[5u8; MAXDCODES]);
    (lencode, distcode)
}

fn dynamic(s: &mut Bits<'_>) -> Result<(Huffman, Huffman)> {
    let nlen = s.bits(5)? as usize + 257;
    let ndist = s.bits(5)? as usize + 1;
    let ncode = s.bits(4)? as usize + 4;
    if nlen > MAXLCODES || ndist > MAXDCODES {
        return Err(InflateError("too many length or distance codes"));
    }
    let mut lengths = [0u8; MAXLCODES + MAXDCODES];
    for &o in &ORDER[..ncode] {
        lengths[o] = s.bits(3)? as u8;
    }
    let (lencode, left) = Huffman::new(&lengths[..19]);
    if left != 0 {
        return Err(InflateError("an incomplete code-length code"));
    }
    let mut index = 0;
    while index < nlen + ndist {
        let sym = lencode.decode(s)?;
        if sym < 16 {
            lengths[index] = sym as u8;
            index += 1;
            continue;
        }
        let (len, repeat) = match sym {
            16 => {
                if index == 0 {
                    return Err(InflateError("a repeat with no length before it"));
                }
                (lengths[index - 1], 3 + s.bits(2)? as usize)
            }
            17 => (0, 3 + s.bits(3)? as usize),
            _ => (0, 11 + s.bits(7)? as usize),
        };
        if index + repeat > nlen + ndist {
            return Err(InflateError("code lengths past the declared count"));
        }
        lengths[index..index + repeat].fill(len);
        index += repeat;
    }
    if lengths[256] == 0 {
        return Err(InflateError("no end-of-block code"));
    }
    let lencode = Huffman::usable(&lengths[..nlen])?;
    let distcode = Huffman::usable(&lengths[nlen..nlen + ndist])?;
    Ok((lencode, distcode))
}

/// A raw DEFLATE stream at the front of `data`: its output, and the bytes it took.
pub fn inflate(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize)> {
    let mut s = Bits {
        data,
        pos: 0,
        buf: 0,
        cnt: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = s.bits(1)? == 1;
        match s.bits(2)? {
            0 => stored(&mut s, &mut out, limit)?,
            1 => {
                let (l, d) = fixed();
                codes(&mut s, &mut out, limit, &l, &d)?;
            }
            2 => {
                let (l, d) = dynamic(&mut s)?;
                codes(&mut s, &mut out, limit, &l, &d)?;
            }
            _ => return Err(InflateError("block type 3")),
        }
        if last {
            return Ok((out, s.pos));
        }
    }
}

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

const FHCRC: u8 = 2;
const FEXTRA: u8 = 4;
const FNAME: u8 = 8;
const FCOMMENT: u8 = 16;

/// One gzip member, which must be all of `data`, with its CRC-32 and length checked.
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let short = InflateError("the gzip header ends early");
    if data.len() < 10 || data[0] != 0x1f || data[1] != 0x8b {
        return Err(InflateError("not gzip"));
    }
    if data[2] != 8 {
        return Err(InflateError("a gzip method other than deflate"));
    }
    let flags = data[3];
    if flags & 0xe0 != 0 {
        return Err(InflateError("reserved gzip flags"));
    }
    let mut pos = 10;
    if flags & FEXTRA != 0 {
        let x = data.get(pos..pos + 2).ok_or(short)?;
        pos += 2 + usize::from(u16::from_le_bytes([x[0], x[1]]));
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            let rest = data.get(pos..).ok_or(short)?;
            let nul = rest.iter().position(|&b| b == 0).ok_or(short)?;
            pos += nul + 1;
        }
    }
    if flags & FHCRC != 0 {
        pos += 2;
    }
    let body = data.get(pos..).ok_or(short)?;
    let (out, used) = inflate(body, limit)?;
    let trailer = body
        .get(used..)
        .filter(|t| t.len() == 8)
        .ok_or(InflateError(
            "the gzip trailer is missing, short or followed by data",
        ))?;
    let crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    if crc != crc32(&out) {
        return Err(InflateError("the gzip CRC-32 does not match"));
    }
    if size != out.len() as u32 {
        return Err(InflateError("the gzip length does not match"));
    }
    Ok(out)
}
