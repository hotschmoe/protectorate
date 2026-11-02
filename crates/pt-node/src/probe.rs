#[derive(Clone, Copy)]
pub struct Config {
    /// Idle seconds at or above which a person counts as away.
    pub away_after_secs: u64,
    /// A locked screen counts as away once idle this long (a person who just locked is still there).
    pub locked_grace_secs: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            away_after_secs: 600,
            locked_grace_secs: 60,
        }
    }
}

pub(crate) enum Session {
    Failed,
    Absent,
    Present,
}

/// What a platform probe saw, before the threshold policy is applied.
pub(crate) struct Observation {
    pub session: Session,
    /// Seconds since the last local input; a lower bound when the probe only watched for a while.
    pub idle_secs: Option<u64>,
    pub locked: Option<bool>,
    pub source: &'static str,
    pub detail: String,
    pub alt_idle_secs: Option<u64>,
}
