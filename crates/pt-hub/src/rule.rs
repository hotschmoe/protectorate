//! D9 in code (SPEC 3.1): when Envoy's current turn was started by a vendor agent's message,
//! or by mail from a D9-marked sleeve, the actions listed here wait for the owner's approval
//! whatever Envoy decides. The origin is recorded per Envoy turn by the executor that writes
//! the prompt (`EnvoyTurn`).
//!
//! The list: removing a sleeve, running a node job, and spawning more than
//! `config.d9_spawn_limit` sleeves in one such turn. Pushes join it with the push method
//! (REVISIT). Outside such a turn nothing a sleeve does waits for the owner (SPEC 2, threat
//! model).

use pt_api::ApiError;
use pt_core::SleeveId;

use crate::api::Principal;
use crate::hub::Hub;
use crate::lock::lock;

pub enum Guarded<'a> {
    Remove(&'a str),
    /// The command line and where it would run.
    RunJob(String),
    Spawn(&'a str),
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

/// Opens an action approval for `sleeve` and waits for the owner.
fn ask(
    hub: &Hub,
    sleeve: SleeveId,
    title: String,
    summary: String,
) -> Result<Option<String>, ApiError> {
    crate::approvals::ask_owner(hub, sleeve, title, summary).map(Some)
}
