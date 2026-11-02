pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel: String,
}

pub struct CpuInfo {
    pub model: String,
    pub cores: u32,
    pub threads: u32,
}

pub fn parallelism() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

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
