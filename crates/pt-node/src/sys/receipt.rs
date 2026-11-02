/// What the node records about a spawned job, enough to find its processes after a restart.
#[derive(Clone, Debug)]
pub struct Receipt {
    pub pid: u32,
    /// Linux `/proc/<pid>/stat` start time, so a reused pid is never mistaken for the job.
    pub start_time: Option<u64>,
    pub scope: Option<String>,
}

/// A job found unfinished in the journal at startup.
pub enum Orphan {
    /// Its processes were still running and have been killed (never on Windows: kill-on-close).
    #[cfg_attr(windows, allow(dead_code))]
    Killed,
    /// Nothing of it runs any more.
    Gone,
}
