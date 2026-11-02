use std::fmt;

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
    /// A snapshot restore failed before replacing a live file. The trial release can stay.
    Restore,
    /// A snapshot restore replaced a live file and did not finish. Prestart must exit and
    /// run again; the trial must not serve the half-restored data.
    RestorePartial,
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
