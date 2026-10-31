//! Node-side collectors: inventory, samples, presence and job priority.
//!
//! JSON output is hand-written (`to_json`) until pt-json lands; see REVISIT.md.

mod exec;
pub mod inventory;
mod json;
pub mod presence;
pub mod priority;
pub mod sample;
mod sys;

pub use inventory::Inventory;
pub use presence::{Detector, Gate, Presence, State, job_gate};
pub use priority::lower_to_idle;
pub use sample::{Sample, Sampler};

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
