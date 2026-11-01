//! Approvals live in the core's state (`pt_core::State::approvals`), journaled with every other
//! change. This is their API view, the owner's decision, and the D9 wait: an action approval is
//! opened through the core, and the API call waits for the core to end it.

use std::time::{Duration, Instant};

use pt_api::{self as api, ApiError, EpochMs, ErrorCode};
use pt_core::{Approval, ApprovalState, Ask, Choice, Decider, EventKind, Reply, SleeveId, Source};
use pt_drivers::{Entry, ToolKind};

use crate::api::{command, err, wait_for};
use crate::home::now_ms;
use crate::hub::Hub;
use crate::log::info;
use crate::transcript::Note;

/// The core times an action approval out at the first Tick past its deadline.
const TICK_SLACK: Duration = Duration::from_secs(15);
/// Beyond this a JSON number loses precision in the web app.
const MAX_SAFE_MS: u64 = (1 << 53) - 1;

pub fn choice(c: Choice) -> api::Decision {
    match c {
        Choice::AllowOnce => api::Decision::AllowOnce,
        Choice::AllowSession => api::Decision::AllowSession,
        Choice::Deny => api::Decision::Deny,
    }
}

fn state(a: &Approval) -> api::ApprovalState {
    use api::ApprovalState as S;
    match a.state {
        ApprovalState::Pending => S::Pending,
        ApprovalState::Decided if a.decision.is_some_and(|d| d.allow) => S::Approved,
        ApprovalState::Decided => S::Denied,
        ApprovalState::Expired => S::Expired,
        ApprovalState::Cancelled => S::Cancelled,
        ApprovalState::Fenced => S::Fenced,
        ApprovalState::Abandoned => S::Abandoned,
    }
}

/// Why a decision did, or will, do nothing.
fn note(a: &Approval) -> String {
    let g = match a.source {
        Source::Tool { g, .. } => g.0,
        Source::Action => 0,
    };
    let call_gone = "the call waiting on this approval ended when the hub restarted";
    match (a.state, &a.source, a.orphan) {
        (ApprovalState::Pending, Source::Tool { .. }, Some(_)) => format!(
            "incarnation g{g} ended before a decision; a decision now is recorded and sent nowhere"
        ),
        (ApprovalState::Pending, Source::Action, Some(_)) => {
            format!("{call_gone}; a decision now is recorded and does nothing")
        }
        (ApprovalState::Fenced, _, Some(_)) => {
            format!("incarnation g{g} had ended; nothing was sent to the harness")
        }
        (ApprovalState::Fenced, _, None) => format!(
            "incarnation g{g} took no answer (no live session after a hub restart); nothing was \
             applied"
        ),
        (ApprovalState::Abandoned, _, _) => format!("{call_gone}; nothing was done"),
        (ApprovalState::Expired, Source::Tool { .. }, Some(_)) => {
            format!("timed out after incarnation g{g} ended; nothing was sent")
        }
        _ => String::new(),
    }
}

/// The API view of a core approval.
pub fn view(a: &Approval) -> api::Approval {
    let (title, summary, detail, options) = match (&a.ask, &a.source) {
        (Some(k), _) => (
            k.title.clone(),
            k.summary.clone(),
            k.detail.clone(),
            k.options.iter().map(|c| choice(*c)).collect(),
        ),
        (None, Source::Tool { native, .. }) => (
            format!("approval {native}"),
            String::new(),
            String::new(),
            Vec::new(),
        ),
        (None, Source::Action) => (a.id.clone(), String::new(), String::new(), Vec::new()),
    };
    api::Approval {
        id: a.id.clone(),
        sleeve_id: a.sleeve.0.to_string(),
        kind: match a.source {
            Source::Tool { .. } => api::ApprovalKind::Tool,
            Source::Action => api::ApprovalKind::Action,
        },
        title,
        summary,
        detail,
        options,
        requested_at_ms: EpochMs(a.requested_at),
        timeout_at_ms: (a.deadline != u64::MAX).then(|| EpochMs(a.deadline.min(MAX_SAFE_MS))),
        state: state(a),
        decision: a.decision.map(|d| {
            if d.allow {
                api::Decision::AllowOnce
            } else {
                api::Decision::Deny
            }
        }),
        decided_by: a.decision.map(|d| match d.by {
            Decider::Owner => api::DecidedBy::Owner,
            Decider::Rule => api::DecidedBy::Rule,
        }),
        decided_at_ms: a.decision.map(|d| EpochMs(d.at)),
        note: note(a),
    }
}

/// The owner decides approval `id`. A tool decision reaches the incarnation that asked only
/// while it is the sleeve's live one; the core records any other as fenced.
pub fn decide(hub: &Hub, id: &str, d: api::Decision) -> Result<api::Approval, ApiError> {
    let view = hub.view();
    let a = view
        .state
        .approvals
        .get(id)
        .ok_or_else(|| err(ErrorCode::NotFound, format!("no approval {id}")))?;
    if a.state != ApprovalState::Pending {
        return Err(err(
            ErrorCode::Conflict,
            format!("approval {id} is already {}", state(a)),
        ));
    }
    if a.source == Source::Action && d == api::Decision::AllowSession {
        return Err(err(
            ErrorCode::Invalid,
            "an action approval is allow_once or deny",
        ));
    }
    let allow = d != api::Decision::Deny;
    let sleeve = a.sleeve;
    command(
        hub,
        EventKind::RespondApproval {
            sleeve,
            approval: id.to_string(),
            allow,
            note: String::new(),
        },
    )?;
    let view = hub.view();
    let a = view
        .state
        .approvals
        .get(id)
        .ok_or_else(|| err(ErrorCode::Internal, format!("approval {id} vanished")))?;
    if a.source == Source::Action && a.state == ApprovalState::Decided {
        let g = view.state.sleeve(sleeve).map_or(0, |s| s.g.0);
        hub.record(
            sleeve,
            g,
            now_ms(),
            Entry::ApprovalDecision {
                id: id.to_string(),
                decision: if allow {
                    pt_drivers::Decision::AllowOnce
                } else {
                    pt_drivers::Decision::Deny
                },
                by: Some("owner".into()),
                confirmed: true,
            },
            Note::default(),
        );
    }
    Ok(self::view(a))
}

/// D9: opens an action approval for `sleeve`, shows it in its chat, and waits until the core
/// ends it: `Ok(id)` once the owner allowed it, else a denial or timeout error. The timeout runs
/// in the core from the journaled request time, so a restart neither resets nor loses it.
pub fn ask_owner(
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
            format!("the owner did not allow {title} ({id}: {})", state(&a)),
        )),
        None => Err(timed_out()),
    }
}
