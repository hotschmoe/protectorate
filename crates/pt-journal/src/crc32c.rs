//! CRC-32C (Castagnoli): reflected polynomial 0x82F63B78, init and final xor 0xFFFFFFFF.

const POLY: u32 = 0x82F6_3B78;

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ POLY } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// CRC-32C of `data`.
pub fn crc32c(data: &[u8]) -> u32 {
    crc32c_append(0, data)
}

/// Continues `crc` (a finished CRC-32C of earlier bytes) over `data`.
pub fn crc32c_append(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}
