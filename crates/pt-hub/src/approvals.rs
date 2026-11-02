//! Approvals live in the core's state (`pt_core::State::approvals`), journaled with every other
//! change. This is their API view; the owner decides in `api.rs`, and the D9 wait is in
//! `rule.rs`.

use pt_api::{self as api, EpochMs};
use pt_core::{Approval, ApprovalState, Choice, Decider, Source};

/// Beyond this a JSON number loses precision in the web app.
const MAX_SAFE_MS: u64 = (1 << 53) - 1;

pub fn choice(c: Choice) -> api::Decision {
    match c {
        Choice::AllowOnce => api::Decision::AllowOnce,
        Choice::AllowSession => api::Decision::AllowSession,
        Choice::Deny => api::Decision::Deny,
    }
}

pub fn state(a: &Approval) -> api::ApprovalState {
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
