//! The one place for OS FFI. Each unsafe block is tiny, sits behind a safe fn and states its
//! invariant. Linux first; elsewhere the functions exist and return `Unsupported`.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(not(unix))]
mod other;
#[cfg(not(unix))]
pub use other::*;

/// The credentials of the process on the other end of a unix socket, taken at connect time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCred {
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
}

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

#[cfg(not(target_os = "linux"))]
fn unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("{what} is not supported on {}", std::env::consts::OS),
    )
}
