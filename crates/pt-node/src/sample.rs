use crate::sys;
use pt_frame::msg::Sample;
use std::time::Duration;

/// Cumulative CPU time counters; only ratios between two readings mean anything.
#[derive(Clone, Copy)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

pub struct Mem {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

/// Keeps the previous CPU reading so a long-running node gets a delta per sample.
#[derive(Default)]
pub struct Sampler {
    prev: Option<CpuTimes>,
}

impl Sampler {
    pub fn new() -> Sampler {
        Sampler::default()
    }

    pub fn sample(&mut self) -> Sample {
        let prev = match self.prev {
            Some(p) => Some(p),
            None => {
                let first = sys::cpu_times();
                std::thread::sleep(Duration::from_millis(250));
                first
            }
        };
        let now = sys::cpu_times();
        self.prev = now;
        let cpu_percent = match (prev, now) {
            (Some(a), Some(b)) if b.total > a.total => {
                let busy = b.busy.saturating_sub(a.busy) as f64;
                let total = (b.total - a.total) as f64;
                Some(round2((busy / total * 100.0).clamp(0.0, 100.0)))
            }
            _ => None,
        };
        let mem = sys::memory();
        Sample {
            taken_at: crate::unix_now(),
            cpu_percent,
            load: sys::load_avg().map(|l| l.iter().copied().map(round2).collect()),
            mem_total_bytes: mem.as_ref().map(|m| m.total),
            mem_available_bytes: mem.as_ref().map(|m| m.available),
            mem_used_bytes: mem.as_ref().map(|m| m.total.saturating_sub(m.available)),
            swap_total_bytes: mem.as_ref().map(|m| m.swap_total),
            swap_free_bytes: mem.as_ref().map(|m| m.swap_free),
            disks: sys::disks(),
            uptime_secs: sys::uptime(),
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
