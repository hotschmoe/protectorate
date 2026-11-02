//! Shared harness for the pt-core fuzz targets: a simulated hub (journal, group commit, executors,
//! crashes) around `pt_core::apply`, a simulated world (which incarnations are alive), the
//! invariant checks I1-I10 and the auto-continue rule with table-driven reference transitions,
//! and a scenario script runner
//! for the committed seeds.

#![forbid(unsafe_code)]

pub mod check;
pub mod gen_event;
pub mod script;
pub mod sim;

/// Reads fuzz bytes as choices; past the end every choice is 0.
pub struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Bytes { data, pos: 0 }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn u8(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let v = u16::from_le_bytes([self.u8(), self.u8()]) as u64;
        v % n
    }

    pub fn bool(&mut self) -> bool {
        self.u8() & 1 == 1
    }

    pub fn u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.u8();
        }
        u64::from_le_bytes(b)
    }

    pub fn string(&mut self) -> String {
        let n = self.u8() % 6;
        (0..n).map(|_| (b'a' + self.u8() % 26) as char).collect()
    }
}
