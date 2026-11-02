//! The last good reading per subscription, with a monotonic TTL and error backoff.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use pt_api::{EpochMs, Quota, QuotaReading};

use crate::{Error, Reader, Snapshot, fmt_age};

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub ttl: Duration,
    pub stale_after: Duration,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
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
    reader: Arc<dyn Reader>,
    good: Option<(Snapshot, u64, Instant)>,
    last_attempt: Option<Instant>,
    failures: u32,
    last_err: Option<Error>,
    pending: Option<mpsc::Receiver<crate::Result<Snapshot>>>,
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
                reader: Arc::from(reader),
                good: None,
                last_attempt: None,
                failures: 0,
                last_err: None,
                pending: None,
            })
            .collect();
        Cache { opts, slots }
    }

    /// Reads due subscriptions concurrently, with a deadline for the entire reader.
    pub fn refresh(&mut self, now_ms: u64, force: bool) {
        let opts = self.opts;
        std::thread::scope(|scope| {
            for slot in &mut self.slots {
                if let Some(pending) = slot.pending.take() {
                    match pending.try_recv() {
                        Err(mpsc::TryRecvError::Empty) => {
                            slot.pending = Some(pending);
                            continue;
                        }
                        Ok(result) => {
                            // A read that missed the previous deadline still replaces the row.
                            slot.settle(now_ms, result);
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {}
                    }
                }
                if !slot.due(&opts, force) {
                    continue;
                }
                scope.spawn(move || {
                    let reader = slot.reader.clone();
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(reader.read(now_ms, opts.read_timeout));
                    });
                    let result = match rx.recv_timeout(opts.read_timeout) {
                        Ok(result) => result,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            slot.pending = Some(rx);
                            Err(Error::Timeout)
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::other(format!(
                            "{:?} reader panicked",
                            slot.reader.subscription()
                        ))),
                    };
                    slot.settle(now_ms, result);
                });
            }
        });
    }

    pub fn quotas(&self) -> Vec<Quota> {
        self.quotas_with_age().into_iter().map(|(q, _)| q).collect()
    }

    /// The reading and the monotonic age of its last successful fetch.
    pub fn quotas_with_age(&self) -> Vec<(Quota, Option<Duration>)> {
        self.slots.iter().map(|s| s.quota(&self.opts)).collect()
    }
}

impl Slot {
    fn settle(&mut self, now_ms: u64, result: crate::Result<Snapshot>) {
        let now = Instant::now();
        self.last_attempt = Some(now);
        match result {
            Ok(snap) => {
                self.good = Some((snap, now_ms, now));
                self.failures = 0;
                self.last_err = None;
            }
            Err(e) => {
                self.failures = self.failures.saturating_add(1);
                self.last_err = Some(e);
            }
        }
    }

    fn due(&self, opts: &Options, force: bool) -> bool {
        let Some(last) = self.last_attempt else {
            return true;
        };
        let since = last.elapsed();
        if self.failures > 0 {
            return since >= opts.backoff(self.failures);
        }
        force && since >= opts.backoff_base || since >= opts.ttl
    }

    fn quota(&self, opts: &Options) -> (Quota, Option<Duration>) {
        let subscription = self.reader.subscription();
        let Some((snap, fetched, instant)) = &self.good else {
            let (reading, note) = match &self.last_err {
                Some(Error::Unavailable(d)) => (QuotaReading::Unknown, d.to_string()),
                Some(e) => (QuotaReading::Error, e.to_string()),
                None => (QuotaReading::Unknown, "not read yet".to_string()),
            };
            return (
                Quota {
                    subscription,
                    plan: String::new(),
                    reading,
                    fetched_at_ms: EpochMs(0),
                    windows: Vec::new(),
                    note,
                },
                None,
            );
        };
        let age = instant.elapsed();
        let stale = age > opts.stale_after;
        let mut note = if stale {
            format!(
                "stale ({} old)",
                fmt_age(age.as_millis().min(u64::MAX as u128) as u64)
            )
        } else {
            snap.note.clone()
        };
        if let Some(e) = &self.last_err {
            note = if note.is_empty() {
                format!("last read failed: {e}")
            } else {
                format!("{note}; last read failed: {e}")
            };
        }
        (
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
            },
            Some(age),
        )
    }
}
