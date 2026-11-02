//! Updates from GitHub releases (SPEC 6a), for the hub and the nodes.
//!
//! - [`Source`] asks the GitHub releases API for a channel's newest release through the system
//!   curl (SPEC 15) and downloads its assets.
//! - [`Store`] is `<root>/releases/`: [`Store::stage`] checks every asset against the release's
//!   `SHA256SUMS` before anything lands in `releases/<version>/`, and the `current` and
//!   `previous` symlinks switch by rename (on Windows by renaming the old link aside first, a
//!   change [`Store::open`] finishes after a crash), so a crash leaves one or the other.
//! - [`Store::switch_on_probation`] puts a new release on trial: [`Store::confirm`] keeps it
//!   (a node: the hub made contact; the hub: its own health check passed), and
//!   [`Store::check_probation`] or [`Store::start_attempt`] (run by the service before each
//!   start, from the previous release) rolls it back when the window passes or it keeps failing
//!   to start.
//!
//! There is no signing key (D36): the sums catch corruption and truncation, not someone who can
//! replace release assets.

#![forbid(unsafe_code)]

mod fetch;
mod store;
mod sums;
mod version;

pub use fetch::{Asset, Release, Source};
pub use store::{Probation, Staged, Store, Switch, Verdict};
pub use sums::Sums;
pub use version::{Channel, Version};

use std::fmt;
use std::time::Duration;

/// How long a new hub has to pass its own health check before it switches back.
pub const HUB_HEALTH_WINDOW: Duration = Duration::from_secs(120);
/// How long a node waits for the hub's first contact after an update before it reverts.
pub const NODE_CONTACT_WINDOW: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// curl failed, or a body was over its limit.
    Fetch,
    /// The release metadata is unusable: missing asset, bad JSON, no release on the channel.
    Release,
    /// A size or sha256 did not match `SHA256SUMS`, or `SHA256SUMS` itself is malformed.
    Verify,
    /// The local `releases/` directory: I/O, a pointer that is not a symlink, nothing to roll
    /// back to.
    Store,
    /// A new release failed its health check (and was rolled back where possible).
    Health,
}

#[derive(Debug, Clone)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn io(what: impl fmt::Display, e: std::io::Error) -> Error {
        Error::new(ErrorKind::Store, format!("{what}: {e}"))
    }

    pub(crate) fn context(mut self, what: impl fmt::Display) -> Error {
        self.message = format!("{what}: {}", self.message);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One release asset and the name it gets inside `releases/<version>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub asset: String,
    pub install_as: String,
    pub executable: bool,
}

fn exe(asset: String, install_as: String) -> Pick {
    Pick {
        asset,
        install_as,
        executable: true,
    }
}

/// This machine's architecture in release asset names (`amd64`, `arm64`).
pub fn arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// How many starts a release on trial gets before [`Store::start_attempt`] rolls it back.
pub const MAX_TRIAL_STARTS: u64 = 3;

/// The hub's asset on Linux `arch`; it embeds headscale and member.
pub fn hub_picks(arch: &str) -> Vec<Pick> {
    vec![exe(
        format!("protectorate-linux-{arch}"),
        "protectorate".to_string(),
    )]
}

/// A node's asset; it embeds member. `os` is `linux` or `windows`.
pub fn node_picks(os: &str, arch: &str) -> Vec<Pick> {
    let ext = if os == "windows" { ".exe" } else { "" };
    vec![exe(
        format!("protectorate-node-{os}-{arch}{ext}"),
        format!("protectorate-node{ext}"),
    )]
}
