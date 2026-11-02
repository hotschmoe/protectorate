use pt_api::{Quota, QuotaReading};

/// The four reading states of a quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Fresh,
    Stale,
    Unknown,
    Error,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Fresh => "fresh",
            State::Stale => "stale",
            State::Unknown => "unknown",
            State::Error => "error",
        }
    }
}

pub fn state(q: &Quota) -> State {
    match q.reading {
        QuotaReading::Ok => State::Fresh,
        QuotaReading::Stale => State::Stale,
        QuotaReading::Unknown => State::Unknown,
        QuotaReading::Error => State::Error,
    }
}
