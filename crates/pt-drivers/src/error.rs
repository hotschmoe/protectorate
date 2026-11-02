use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum DriverError {
    /// The harness has not reported its session yet.
    NotReady,
    /// A turn is running and this harness cannot take another message now.
    Busy,
    /// The process has exited or is shutting down; nothing was written.
    Ended,
    /// The turn a steer was meant for is not the running one; nothing was written.
    TurnEnded,
    Unsupported(&'static str),
    UnknownApproval(String),
    Invalid(&'static str),
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NotReady => f.write_str("harness session is not ready"),
            DriverError::Busy => f.write_str("harness is in a turn and cannot take a message"),
            DriverError::Ended => f.write_str("harness session has ended"),
            DriverError::TurnEnded => f.write_str("the turn to steer has ended"),
            DriverError::Unsupported(what) => write!(f, "unsupported: {what}"),
            DriverError::UnknownApproval(id) => write!(f, "approval {id:?} is not pending"),
            DriverError::Invalid(what) => write!(f, "invalid: {what}"),
            DriverError::Spawn { program, source } => {
                write!(f, "spawn {}: {source}", program.display())
            }
        }
    }
}

impl std::error::Error for DriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DriverError::Spawn { source, .. } => Some(source),
            _ => None,
        }
    }
}
