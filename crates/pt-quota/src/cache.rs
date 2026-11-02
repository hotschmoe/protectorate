//! The last good reading per subscription, with a monotonic TTL and error backoff.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use pt_api::{EpochMs, Quota, QuotaReading, Subscription};

use crate::reader::{Reader, Snapshot};
use crate::{Error, fmt_age};

/// How long a reading stays fresh. A passed reset blocks for this long too.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(600);

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
            stale_after: STALE_AFTER,
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
    pending: Option<mpsc::Receiver<(u64, Instant, crate::Result<Snapshot>)>>,
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
                        Ok((finished_ms, finished_at, result)) => {
                            // The read missed the previous deadline. Stamp and age it from when
                            // it finished, and do not count a second failure on top of the timeout.
                            slot.settle(finished_ms, finished_at, result, false);
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
                        let result = reader.read(now_ms, opts.read_timeout);
                        let _ = tx.send((crate::now_ms(), Instant::now(), result));
                    });
                    // This refresh's clock is the stamp. A result that arrives after the
                    // deadline is stamped later, when the read actually returned.
                    let result = match rx.recv_timeout(opts.read_timeout) {
                        Ok((_, _, result)) => result,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            slot.pending = Some(rx);
                            Err(Error::Timeout)
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::other(format!(
                            "{:?} reader panicked",
                            slot.reader.subscription()
                        ))),
                    };
                    slot.settle(now_ms, Instant::now(), result, true);
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
    fn settle(
        &mut self,
        now_ms: u64,
        at: Instant,
        result: crate::Result<Snapshot>,
        count_failure: bool,
    ) {
        match result {
            Ok(snap) => {
                self.last_attempt = Some(at);
                self.good = Some((snap, now_ms, at));
                self.failures = 0;
                self.last_err = None;
            }
            Err(e) => {
                if count_failure {
                    self.last_attempt = Some(at);
                    self.failures = self.failures.saturating_add(1);
                }
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
            // Expired and missing logins are not a failed window. A rejected login or a
            // missing usage scope is the same only for the vendor OAuth CLIs, which refresh
            // the login once a sleeve launches. A wrong key on the local endpoint stays a
            // failed read.
            let vendor = matches!(
                subscription,
                Subscription::Claude | Subscription::Codex | Subscription::Grok
            );
            let (reading, note) = match &self.last_err {
                Some(Error::Unavailable(d)) => (QuotaReading::Unknown, d.to_string()),
                Some(Error::Expired) => (QuotaReading::Unknown, Error::Expired.to_string()),
                Some(Error::Unauthorized) if vendor => {
                    (QuotaReading::Unknown, Error::Unauthorized.to_string())
                }
                Some(Error::Scope) if vendor => (QuotaReading::Unknown, Error::Scope.to_string()),
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
