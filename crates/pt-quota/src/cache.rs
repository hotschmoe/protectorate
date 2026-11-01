//! The last good reading per subscription, with a TTL and error backoff.

use std::time::Duration;

use pt_api::{EpochMs, Quota, QuotaReading};

use crate::{Error, Reader, Snapshot, UNKNOWN_PREFIX, fmt_age};

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// A good reading younger than this is not re-read.
    pub ttl: Duration,
    /// A good reading older than this is stale.
    pub stale_after: Duration,
    /// Wait after the first failure; doubles per consecutive failure.
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    /// Bound on one read.
    pub read_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            ttl: Duration::from_secs(300),
            stale_after: Duration::from_secs(600),
            backoff_base: Duration::from_secs(30),
            backoff_max: Duration::from_secs(3600),
            read_timeout: Duration::from_secs(45),
        }
    }
}

impl Options {
    fn backoff(&self, failures: u32) -> Duration {
        let shift = failures.saturating_sub(1).min(16);
        self.backoff_base
            .saturating_mul(1 << shift)
            .min(self.backoff_max)
    }
}

struct Slot {
    reader: Box<dyn Reader>,
    good: Option<(Snapshot, u64)>,
    last_attempt_ms: Option<u64>,
    failures: u32,
    last_err: Option<Error>,
}

pub struct Cache {
    opts: Options,
    slots: Vec<Slot>,
}

impl Cache {
    pub fn new(readers: Vec<Box<dyn Reader>>, opts: Options) -> Cache {
        let slots = readers
            .into_iter()
            .map(|reader| Slot {
                reader,
                good: None,
                last_attempt_ms: None,
                failures: 0,
                last_err: None,
            })
            .collect();
        Cache { opts, slots }
    }

    /// Reads, in parallel, every subscription that is due: never read, past its TTL, or past its
    /// backoff after failures. `force` skips the TTL but never the backoff.
    pub fn refresh(&mut self, now_ms: u64, force: bool) {
        let opts = self.opts;
        std::thread::scope(|scope| {
            for slot in &mut self.slots {
                if !slot.due(&opts, now_ms, force) {
                    continue;
                }
                scope.spawn(move || {
                    let sub = slot.reader.subscription();
                    let res = std::thread::scope(|inner| {
                        inner
                            .spawn(|| slot.reader.read(now_ms, opts.read_timeout))
                            .join()
                            .unwrap_or_else(|_| {
                                Err(Error::other(format!("{sub:?} reader panicked")))
                            })
                    });
                    slot.last_attempt_ms = Some(now_ms);
                    match res {
                        Ok(snap) => {
                            slot.good = Some((snap, now_ms));
                            slot.failures = 0;
                            slot.last_err = None;
                        }
                        Err(e) => {
                            slot.failures = slot.failures.saturating_add(1);
                            slot.last_err = Some(e);
                        }
                    }
                });
            }
        });
    }

    /// Every subscription's quota with its reading state computed at `now_ms`.
    pub fn quotas(&self, now_ms: u64) -> Vec<Quota> {
        self.slots
            .iter()
            .map(|s| s.quota(&self.opts, now_ms))
            .collect()
    }
}

impl Slot {
    fn due(&self, opts: &Options, now_ms: u64, force: bool) -> bool {
        let Some(last) = self.last_attempt_ms else {
            return true;
        };
        let since = Duration::from_millis(now_ms.saturating_sub(last));
        if self.failures > 0 {
            return since >= opts.backoff(self.failures);
        }
        force && since >= opts.backoff_base || since >= opts.ttl
    }

    fn quota(&self, opts: &Options, now_ms: u64) -> Quota {
        let subscription = self.reader.subscription();
        let err_note = self.last_err.as_ref().map(ToString::to_string);
        let Some((snap, fetched)) = &self.good else {
            let (reading, note) = match (&self.last_err, self.last_attempt_ms) {
                (Some(Error::Unavailable(d)), _) => {
                    (QuotaReading::Error, format!("{UNKNOWN_PREFIX}{d}"))
                }
                (Some(e), _) => (QuotaReading::Error, e.to_string()),
                (None, _) => (QuotaReading::Error, format!("{UNKNOWN_PREFIX}not read yet")),
            };
            return Quota {
                subscription,
                plan: String::new(),
                reading,
                fetched_at_ms: EpochMs(0),
                windows: Vec::new(),
                note,
            };
        };
        let age = now_ms.saturating_sub(*fetched);
        let stale = Duration::from_millis(age) > opts.stale_after;
        let mut note = snap.note.clone();
        if stale {
            note = format!("stale ({} old)", fmt_age(age));
        }
        if let Some(e) = err_note {
            note = if note.is_empty() {
                format!("last read failed: {e}")
            } else {
                format!("{note}; last read failed: {e}")
            };
        }
        Quota {
            subscription,
            plan: snap.plan.clone(),
            reading: if stale {
                QuotaReading::Stale
            } else {
                QuotaReading::Ok
            },
            fetched_at_ms: EpochMs(*fetched),
            windows: snap.windows.clone(),
            note,
        }
    }
}
