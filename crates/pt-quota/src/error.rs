use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not logged in, not configured or not installed: there is no data to read.
    Unavailable(String),
    /// The stored login is past its expiry; its CLI must refresh it. Readers never do.
    Expired,
    Unauthorized,
    /// The login lacks the scope usage queries need.
    Scope,
    Http(u16),
    Timeout,
    Other(String),
}

impl Error {
    pub fn unavailable(detail: impl Into<String>) -> Error {
        Error::Unavailable(detail.into())
    }

    pub fn other(detail: impl Into<String>) -> Error {
        Error::Other(detail.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unavailable(d) => f.write_str(d),
            Error::Expired => f.write_str("login expired; log in again with the provider CLI"),
            Error::Unauthorized => f.write_str("login rejected (HTTP 401)"),
            Error::Scope => f.write_str("login lacks the scope required for usage queries"),
            Error::Http(s) => write!(f, "unexpected HTTP status {s}"),
            Error::Timeout => f.write_str("request timed out"),
            Error::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
