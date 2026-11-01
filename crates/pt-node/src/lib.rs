//! The node side: inventory, samples, presence, job priority and the job runner. The wire types
//! are pt-frame's.

mod artifact;
mod exec;
pub mod inventory;
pub mod jobs;
pub mod presence;
pub mod priority;
pub mod sample;
mod sys;

/// The Windows service: run under the SCM, log stdio to a file, keep children in a
/// kill-on-close Job Object.
#[cfg(windows)]
pub mod windows {
    pub use crate::sys::{Job, redirect_stdio, run_service, spawn_in_job};
}

pub use jobs::Runner;
pub use presence::{Detector, Gate, job_gate};
pub use priority::lower_to_idle;
pub use sample::Sampler;

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn hostname() -> String {
    clean(&sys::hostname(), 128)
}

/// Strings from this machine are bounded and stripped of control characters before they leave it.
pub(crate) fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = if c.is_control() { ' ' } else { c };
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

/// 128 bits as 32 hex digits, from std's randomly keyed SipHash (no CSPRNG crate): unique enough
/// for node ids and journal epochs, never for secrets.
pub fn random_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut out = String::with_capacity(32);
    for i in 0..2u8 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u8(i);
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}
