//! The one place for OS FFI. Each unsafe block is tiny, sits behind a safe fn and states its
//! invariant. Linux first; elsewhere the functions exist and return `Unsupported`.

mod cred;
#[cfg(not(target_os = "linux"))]
mod error;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

pub use cred::PeerCred;
#[cfg(not(target_os = "linux"))]
pub(crate) use error::unsupported;

#[cfg(not(unix))]
mod other;
#[cfg(not(unix))]
pub use other::*;

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;

/// Installs the termination handler (idempotent) and blocks until SIGTERM, SIGINT or SIGHUP.
/// Returns the signal number.
pub fn wait_term() -> std::io::Result<i32> {
    let flag = install_term_handler()?;
    flag.wait();
    Ok(flag.signal().unwrap_or(SIGTERM))
}
