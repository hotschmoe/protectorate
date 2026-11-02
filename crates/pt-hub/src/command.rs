//! A handler's way to the core: a command whose refusal or timeout is the API's error, and a
//! wait for the published view to reach a condition.

use std::time::{Duration, Instant};

use pt_api::{ApiError, ErrorCode};
use pt_core::{Code, EventKind, Reply};

use crate::hub::{CommandError, Hub};
use crate::view::View;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

pub fn err(code: ErrorCode, m: impl Into<String>) -> ApiError {
    ApiError::new(code, m)
}

/// A command to the core, answered once it is journaled.
pub fn command(hub: &Hub, kind: EventKind) -> Result<Reply, ApiError> {
    let r = hub.command(kind, COMMAND_TIMEOUT).map_err(|e| match e {
        CommandError::Timeout => err(ErrorCode::Timeout, "the core did not answer"),
        CommandError::Down => err(ErrorCode::Unavailable, "the core is stopped"),
    })?;
    if let Reply::Refused(code, why) = r {
        let code = match code {
            Code::InvalidArgument => ErrorCode::Invalid,
            Code::NotFound => ErrorCode::NotFound,
            Code::AlreadyExists | Code::FailedPrecondition => ErrorCode::Conflict,
        };
        return Err(err(code, why));
    }
    Ok(r)
}

/// Re-evaluates `check` on every change until it answers or `deadline` passes. The bus position
/// is read before the view, so a change between the two wakes the wait at once.
pub fn wait_for<T>(
    hub: &Hub,
    deadline: Instant,
    mut check: impl FnMut(&View) -> Option<T>,
) -> Option<T> {
    loop {
        let pos = hub.bus.pos();
        let view = hub.view();
        if let Some(x) = check(&view) {
            return Some(x);
        }
        if Instant::now() >= deadline {
            return None;
        }
        hub.bus.wait_after(pos, deadline);
    }
}
