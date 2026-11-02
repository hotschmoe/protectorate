//! Quota readings (SPEC 8): a `pt_quota::Cache` over the readers of the installed harnesses,
//! refreshed on a background thread every `quota_poll_secs`. The API, the snapshot and
//! `SelectTarget` read the last values; a changed reading goes out on the event stream.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use pt_api::{self as api, HubStreamMessage};
use pt_quota::Options;

use crate::bus::Bus;
use crate::lock::lock;

pub struct Quotas {
    last: Mutex<Published>,
}

struct Published {
    rows: Vec<(api::Quota, Option<Duration>)>,
    at: Instant,
    stale_after: Duration,
}

impl Default for Quotas {
    fn default() -> Self {
        Quotas {
            last: Mutex::new(Published {
                rows: Vec::new(),
                at: Instant::now(),
                stale_after: Options::default().stale_after,
            }),
        }
    }
}

impl Quotas {
    pub fn list(&self) -> Vec<api::Quota> {
        let last = lock(&self.last);
        last.rows
            .iter()
            .map(|(q, age)| {
                let mut q = q.clone();
                if q.reading == api::QuotaReading::Ok
                    && age
                        .is_some_and(|age| age.saturating_add(last.at.elapsed()) > last.stale_after)
                {
                    let age = age.unwrap_or_default().saturating_add(last.at.elapsed());
                    q.reading = api::QuotaReading::Stale;
                    let stale = format!(
                        "stale ({} old)",
                        pt_quota::fmt_age(age.as_millis().min(u64::MAX as u128) as u64)
                    );
                    q.note = if q.note.is_empty() {
                        stale
                    } else {
                        format!("{stale}; {}", q.note)
                    };
                }
                q
            })
            .collect()
    }

    /// Takes a round of readings; a changed one goes out on the stream.
    pub fn publish(
        &self,
        bus: &Bus,
        now: Vec<(api::Quota, Option<Duration>)>,
        stale_after: Duration,
    ) {
        let mut last = lock(&self.last);
        for (q, _) in &now {
            if !last.rows.iter().any(|(old, _)| old == q) {
                let quota = q.clone();
                bus.push(|seq| HubStreamMessage::Quota { seq, quota });
            }
        }
        *last = Published {
            rows: now,
            at: Instant::now(),
            stale_after,
        };
    }
}
