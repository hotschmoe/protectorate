//! D9 in code (SPEC 3.1): when Envoy's current turn was started by a vendor agent's message,
//! the actions listed here wait for the owner's approval whatever Envoy decides. The origin is
//! recorded per Envoy turn by the executor that writes the prompt (`EnvoyTurn`).
//!
//! The list: removing a sleeve, running a node job, and spawning more than
//! `config.d9_spawn_limit` sleeves in one such turn. Pushes join it with the push method
//! (REVISIT). A node maintenance job from any sleeve, Envoy included, always waits for the
//! owner (SPEC 3.1).

use std::time::{Duration, Instant};

use pt_api::{ApiError, ApprovalState, DecidedBy, Decision, ErrorCode, EventKind, Level};
use pt_core::SleeveId;
use pt_drivers::{Entry, ToolKind};

use crate::api::{Principal, err};
use crate::home::now_ms;
use crate::hub::Hub;
use crate::log::info;
use crate::transcript::{Note, lock};

pub enum Guarded<'a> {
    Remove(&'a str),
    /// The command line and where it would run.
    RunJob(String),
    Spawn(&'a str),
}

/// A node maintenance job (`what`: the command line and where it would run): from any sleeve,
/// Envoy included, it waits for the owner whatever started the turn.
pub fn gate_maintenance(
    hub: &Hub,
    who: &Principal,
    what: &str,
) -> Result<Option<String>, ApiError> {
    let (Principal::Envoy(sleeve) | Principal::Sleeve(sleeve)) = *who else {
        return Ok(None);
    };
    ask(
        hub,
        sleeve,
        format!("run node maintenance job {what}"),
        "A sleeve asked for a node maintenance job. Code requires the owner's approval for it."
            .into(),
    )
}

/// Lets the call through (`Ok(None)`), lets it through after the owner allowed it
/// (`Ok(Some(approval_id))`), or fails it on a denial or timeout.
pub fn gate(hub: &Hub, who: &Principal, action: Guarded<'_>) -> Result<Option<String>, ApiError> {
    let Principal::Envoy(envoy) = *who else {
        return Ok(None);
    };
    let (origin, title) = {
        let mut t = lock(&hub.envoy_turn);
        let Some(origin) = t.origin.clone() else {
            return Ok(None);
        };
        let title = match action {
            Guarded::Remove(name) => format!("remove sleeve {name}"),
            Guarded::RunJob(what) => format!("run node job {what}"),
            Guarded::Spawn(name) => {
                t.spawns += 1;
                let limit = hub.config.d9_spawn_limit;
                if t.spawns <= limit {
                    return Ok(None);
                }
                format!(
                    "spawn sleeve {name} ({} this turn, over the limit of {limit})",
                    t.spawns
                )
            }
        };
        (origin, title)
    };
    let summary = format!(
        "Envoy asked for this in a turn started by {origin}. Code requires the owner's \
         approval for it (D9)."
    );
    ask(hub, envoy, title, summary)
}

/// Opens an action approval for `sleeve`, records it in its transcript and on the stream, and
/// waits for the owner.
fn ask(
    hub: &Hub,
    sleeve: SleeveId,
    title: String,
    summary: String,
) -> Result<Option<String>, ApiError> {
    let wait = Duration::from_secs(hub.config.d9_approval_timeout_secs);
    let a = hub.approvals.open_action(
        &hub.bus,
        sleeve,
        title.clone(),
        summary.clone(),
        wait.as_millis() as u64,
    );
    info!("{} waits for the owner: {title}", a.id);
    let (g, source) = hub
        .view()
        .state
        .sleeve(sleeve)
        .map_or((0, String::new()), |s| (s.g.0, s.name.clone()));
    hub.record(
        sleeve,
        g,
        now_ms(),
        Entry::ApprovalRequest {
            id: a.id.clone(),
            tool: ToolKind::Other,
            name: title.clone(),
            summary,
            input: None,
            options: Vec::new(),
        },
        Note::default(),
    );
    hub.event(
        EventKind::Approval,
        Level::Warn,
        &source,
        "owner",
        &format!("approval {} requested: {title}", a.id),
    );
    let decided = hub.approvals.wait(&a.id, Instant::now() + wait);
    match decided {
        Some(d) if d.state == ApprovalState::Approved => Ok(Some(a.id)),
        Some(d) => Err(err(
            ErrorCode::Forbidden,
            format!("the owner did not allow {title} ({}: {})", a.id, d.state),
        )),
        None => {
            hub.approvals.resolve(
                &hub.bus,
                &a.id,
                ApprovalState::Expired,
                Some(Decision::Deny),
                DecidedBy::Rule,
            );
            Err(err(
                ErrorCode::Timeout,
                format!(
                    "no owner decision on {title} within {}s ({})",
                    wait.as_secs(),
                    a.id
                ),
            ))
        }
    }
}
