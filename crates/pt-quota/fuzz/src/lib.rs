//! Input generation for the pt-quota fuzz targets.

use pt_api::{EpochMs, Harness, Quota, QuotaReading, QuotaWindow, Subscription};

/// Reads fuzz bytes as choices; past the end every choice is 0.
pub struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Bytes { data, pos: 0 }
    }

    pub fn u8(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }

    pub fn bool(&mut self) -> bool {
        self.u8() & 1 == 1
    }

    pub fn below(&mut self, n: usize) -> usize {
        usize::from(self.u8()) % n.max(1)
    }

    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }
}

pub const HARNESSES: [Harness; 6] = [
    Harness::Claude,
    Harness::Codex,
    Harness::Grok,
    Harness::PiVendor,
    Harness::PiLocal,
    Harness::Antigravity,
];

pub const SUBSCRIPTIONS: [Subscription; 5] = [
    Subscription::Claude,
    Subscription::Codex,
    Subscription::Grok,
    Subscription::Local,
    Subscription::Api,
];

const LABELS: [&str; 10] = [
    "5h",
    "wk",
    "day",
    "mo",
    "3d",
    "wk sonnet",
    "credits",
    "window",
    "wk opus",
    "wk sonnet 4.5",
];

fn pct(b: &mut Bytes) -> Option<f64> {
    match b.below(8) {
        0 => None,
        1 => Some(f64::NAN),
        2 => Some(f64::INFINITY),
        3 => Some(-5.0),
        4 => Some(250.0),
        _ => Some(f64::from(b.u8()) * 100.0 / 255.0),
    }
}

pub fn quota(b: &mut Bytes, now_ms: u64) -> Quota {
    let reading = match b.below(4) {
        0 | 1 => QuotaReading::Ok,
        2 => QuotaReading::Stale,
        3 if b.bool() => QuotaReading::Unknown,
        _ => QuotaReading::Error,
    };
    let windows = (0..b.below(4))
        .map(|_| QuotaWindow {
            label: LABELS[b.below(LABELS.len())].to_string(),
            used_pct: pct(b),
            resets_at_ms: match b.below(5) {
                0 => None,
                // A stamp at the epoch is "no reset"; the percent still counts.
                1 => Some(EpochMs(0)),
                2 => Some(EpochMs(now_ms.saturating_sub(u64::from(b.u32())))),
                _ => Some(EpochMs(now_ms.saturating_add(u64::from(b.u32()) * 1000))),
            },
            length_ms: match b.below(4) {
                0 => None,
                1 => Some(0),
                _ => Some(u64::from(b.u32()).saturating_mul(1000)),
            },
        })
        .collect();
    Quota {
        subscription: SUBSCRIPTIONS[b.below(SUBSCRIPTIONS.len())],
        plan: String::new(),
        reading,
        fetched_at_ms: EpochMs(now_ms.saturating_sub(u64::from(b.u32()) * 10)),
        windows,
        note: String::new(),
    }
}
