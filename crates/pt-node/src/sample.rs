use crate::json::{Obj, array};
use crate::sys;
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

pub struct Sample {
    pub taken_at: u64,
    pub cpu_percent: Option<f64>,
    pub load: Option<[f64; 3]>,
    pub mem: Option<Mem>,
    pub disks: Vec<crate::inventory::Disk>,
    pub uptime_secs: Option<u64>,
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
                Some((busy / total * 100.0).clamp(0.0, 100.0))
            }
            _ => None,
        };
        Sample {
            taken_at: crate::unix_now(),
            cpu_percent,
            load: sys::load_avg(),
            mem: sys::memory(),
            disks: sys::disks(),
            uptime_secs: sys::uptime(),
        }
    }
}

impl Sample {
    pub fn to_json(&self) -> String {
        let mut o = Obj::new().uint("taken_at", self.taken_at);
        o = match self.cpu_percent {
            Some(p) => o.float("cpu_percent", p),
            None => o.raw("cpu_percent", "null"),
        };
        o = match self.load {
            Some(l) => o.raw("load", &array(l.iter().map(|v| format!("{v:.2}")))),
            None => o.raw("load", "null"),
        };
        match &self.mem {
            Some(m) => {
                o = o
                    .uint("mem_total_bytes", m.total)
                    .uint("mem_available_bytes", m.available)
                    .uint("mem_used_bytes", m.total.saturating_sub(m.available))
                    .uint("swap_total_bytes", m.swap_total)
                    .uint("swap_free_bytes", m.swap_free);
            }
            None => {
                o = o
                    .raw("mem_total_bytes", "null")
                    .raw("mem_available_bytes", "null")
                    .raw("mem_used_bytes", "null")
                    .raw("swap_total_bytes", "null")
                    .raw("swap_free_bytes", "null");
            }
        }
        o.raw("disks", &array(self.disks.iter().map(|d| d.to_json())))
            .opt_uint("uptime_secs", self.uptime_secs)
            .finish()
    }
}
