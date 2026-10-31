//! The node side: inventory, samples, presence, job priority and the job runner. The wire types
//! are pt-frame's.

mod exec;
pub mod inventory;
pub mod presence;
pub mod priority;
pub mod sample;
mod sys;

pub use presence::{Detector, Gate, job_gate};
pub use priority::lower_to_idle;
pub use sample::Sampler;

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
