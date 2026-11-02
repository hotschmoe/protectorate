//! D9 in code (SPEC 3.1): when Envoy's current turn was started by a vendor agent's message,
//! or by mail from a D9-marked sleeve, the actions listed here wait for the owner's approval
//! whatever Envoy decides. The origin is recorded per Envoy turn by the executor that writes
//! the prompt (`EnvoyTurn`).
//!
//! The list: removing a sleeve, running a node job, and spawning more than
//! `config.d9_spawn_limit` sleeves in one such turn. Pushes join it with the push method
//! (REVISIT). Outside such a turn nothing a sleeve does waits for the owner (SPEC 2, threat
//! model).

use std::time::{Duration, Instant};

use pt_api::{ApiError, ErrorCode};
use pt_core::{ApprovalState, Ask, Choice, EventKind, Reply, SleeveId};
use pt_drivers::{Entry, ToolKind};

use crate::approvals;
use crate::command::{command, err, wait_for};
use crate::home::now_ms;
use crate::hub::Hub;
use crate::lock::lock;
use crate::log::info;
use crate::principal::Principal;
use crate::transcript::Note;

/// The core times an action approval out at the first Tick past its deadline.
const TICK_SLACK: Duration = Duration::from_secs(15);

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
    ask_owner(hub, envoy, title, summary).map(Some)
}

/// D9: opens an action approval for `sleeve`, shows it in its chat, and waits until the core
/// ends it: `Ok(id)` once the owner allowed it, else a denial or timeout error. The timeout runs
/// in the core from the journaled request time, so a restart neither resets nor loses it.
fn ask_owner(
    hub: &Hub,
    sleeve: SleeveId,
    title: String,
    summary: String,
) -> Result<String, ApiError> {
    let secs = hub.config.d9_approval_timeout_secs;
    let timeout = secs.saturating_mul(1000);
    let ask = Ask {
        title: title.clone(),
        summary: summary.clone(),
        detail: String::new(),
        options: vec![Choice::AllowOnce, Choice::Deny],
    };
    let id = match command(
        hub,
        EventKind::RequestApproval {
            sleeve,
            ask,
            timeout,
        },
    )? {
        Reply::ApprovalOpened(id) => id,
        other => {
            return Err(err(
                ErrorCode::Internal,
                format!("RequestApproval answered {other:?}"),
            ));
        }
    };
    info!("{id} waits for the owner: {title}");
    let g = hub.view().state.sleeve(sleeve).map_or(0, |s| s.g.0);
    hub.record(
        sleeve,
        g,
        now_ms(),
        Entry::ApprovalRequest {
            id: id.clone(),
            tool: ToolKind::Other,
            name: title.clone(),
            summary,
            input: None,
            options: Vec::new(),
        },
        Note::default(),
    );
    // Ten years stands in for "forever" and keeps the Instant arithmetic in range.
    let wait = Duration::from_millis(timeout)
        .saturating_add(TICK_SLACK)
        .min(Duration::from_secs(10 * 365 * 24 * 3600));
    let ended = wait_for(hub, Instant::now() + wait, |v| {
        v.state
            .approvals
            .get(&id)
            .filter(|a| a.state != ApprovalState::Pending)
            .cloned()
    });
    let timed_out = || {
        err(
            ErrorCode::Timeout,
            format!("no owner decision on {title} within {secs}s ({id})"),
        )
    };
    match ended {
        Some(a) if a.state == ApprovalState::Decided && a.decision.is_some_and(|d| d.allow) => {
            Ok(id)
        }
        Some(a) if a.state == ApprovalState::Expired => Err(timed_out()),
        Some(a) => Err(err(
            ErrorCode::Forbidden,
            format!(
                "the owner did not allow {title} ({id}: {})",
                approvals::state(&a)
            ),
        )),
        None => Err(timed_out()),
    }
}
