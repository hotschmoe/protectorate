//! The node side: inventory, samples, presence, job priority and the job runner. The wire types
//! are pt-frame's.

mod artifact;
mod clock;
mod exec;
mod id;
pub mod inventory;
mod job_error;
pub mod jobs;
pub mod presence;
pub mod priority;
mod probe;
pub mod sample;
mod sys;
mod text;

/// The Windows service: run under the SCM, log stdio to a file, keep children in a
/// kill-on-close Job Object.
#[cfg(windows)]
pub mod windows {
    pub use crate::sys::{Job, redirect_stdio, run_service, spawn_in_job};
}

pub use clock::unix_now;
pub use id::random_id;
pub use jobs::Runner;
pub use presence::{Detector, Gate, job_gate};
pub use priority::lower_to_idle;
pub use sample::Sampler;

pub fn hostname() -> String {
    text::clean(&sys::hostname(), 128)
}
