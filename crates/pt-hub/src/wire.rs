//! pt-core state and stored transcript lines as pt-api's types.

use pt_api as api;
use pt_api::EpochMs;
use pt_core::{Activity, Harness, Layout, Life, Mode, ParkReason, Sleeve, SleeveId, State, Target};
use pt_drivers as drv;

use crate::envoy::envoy_id;
use crate::home::Config;
use crate::transcript::{Line, Transcripts};
use crate::view::View;

pub fn harness_from(s: &str) -> Option<Harness> {
    Some(match s {
        "claude" => Harness::Claude,
        "codex" => Harness::Codex,
        "grok" => Harness::Grok,
        "antigravity" => Harness::Antigravity,
        "pi_vendor" => Harness::PiVendor,
        "pi_local" => Harness::PiLocal,
        _ => return None,
    })
}

pub fn api_harness(h: Harness) -> api::Harness {
    match h {
        Harness::Claude => api::Harness::Claude,
        Harness::Codex => api::Harness::Codex,
        Harness::Grok => api::Harness::Grok,
        Harness::Antigravity => api::Harness::Antigravity,
        Harness::PiVendor => api::Harness::PiVendor,
        Harness::PiLocal => api::Harness::PiLocal,
    }
}

pub fn core_harness(h: api::Harness) -> Harness {
    match h {
        api::Harness::Claude => Harness::Claude,
        api::Harness::Codex => Harness::Codex,
        api::Harness::Grok => Harness::Grok,
        api::Harness::Antigravity => Harness::Antigravity,
        api::Harness::PiVendor => Harness::PiVendor,
        api::Harness::PiLocal => Harness::PiLocal,
    }
}

/// A headless target, with provider and model defaulted from the config.
pub fn target(config: &Config, harness: Harness, provider: &str, model: &str) -> Target {
    let (dp, dm) = match harness {
        Harness::Claude => ("", config.claude_model.as_str()),
        Harness::Codex => ("", config.codex_model.as_str()),
        Harness::Grok => ("", config.grok_model.as_str()),
        Harness::PiVendor => (
            config.pi_vendor_provider.as_str(),
            config.pi_vendor_model.as_str(),
        ),
        Harness::PiLocal => (
            config.pi_local_provider.as_str(),
            config.pi_local_model.as_str(),
        ),
        Harness::Antigravity => ("", ""),
    };
    let pick = |given: &str, d: &str| {
        if given.is_empty() {
            d.to_string()
        } else {
            given.to_string()
        }
    };
    Target {
        harness,
        mode: Mode::Headless,
        provider: pick(provider, dp),
        model: pick(model, dm),
    }
}

pub fn lifecycle(l: Life) -> api::Lifecycle {
    use api::Lifecycle as L;
    match l {
        Life::Provisioning => L::Provisioning,
        Life::Launching => L::Launching,
        Life::Running => L::Running,
        Life::Recovering => L::Recovering,
        Life::Resleeving => L::Resleeving,
        Life::Stopping { .. } => L::Stopping,
        Life::Stopped => L::Stopped,
        Life::Parked(_) => L::Parked,
        Life::Removed => L::Removed,
    }
}

pub fn activity(a: &Activity) -> api::ActivityState {
    use api::ActivityState as A;
    match a {
        Activity::Booting => A::Booting,
        Activity::Idle { .. } => A::Idle,
        Activity::Delivering { .. } => A::Delivering,
        Activity::Working => A::Working,
        Activity::Blocked { .. } => A::Blocked,
        Activity::Interrupting => A::Interrupting,
        Activity::Compacting => A::Compacting,
        Activity::Unknown => A::Unknown,
        Activity::Ended => A::Ended,
    }
}

pub fn park_wire(r: ParkReason) -> String {
    match r {
        ParkReason::Provision => "provision".into(),
        ParkReason::NoTarget => "no_target".into(),
        ParkReason::Launch => "launch".into(),
        ParkReason::Flapping(Some(h)) => format!("flapping ({} login rejected)", h.as_str()),
        ParkReason::Flapping(None) => "flapping".into(),
        ParkReason::BriefUnconfirmed => "brief_unconfirmed".into(),
        ParkReason::Guard(g) => format!("g{}", g.number()),
    }
}

fn subscription(t: &Target) -> api::Subscription {
    match t.harness {
        Harness::Claude => api::Subscription::Claude,
        Harness::Codex => api::Subscription::Codex,
        Harness::Grok => api::Subscription::Grok,
        Harness::PiLocal => api::Subscription::Local,
        Harness::PiVendor | Harness::Antigravity => api::Subscription::Api,
    }
}

pub fn root_of(state: &State, s: &Sleeve) -> SleeveId {
    s.parent
        .and_then(|p| state.sleeve(p))
        .map_or(s.id, |p| p.id)
}

pub fn role(state: &State, s: &Sleeve) -> api::SleeveRole {
    if s.parent.is_some() {
        api::SleeveRole::Child
    } else if Some(s.id) == envoy_id(state) {
        api::SleeveRole::Envoy
    } else {
        api::SleeveRole::Root
    }
}

pub fn sleeve(transcripts: &Transcripts, view: &View, s: &Sleeve) -> api::Sleeve {
    let st = &view.state;
    let stats = transcripts.stats(s.id);
    let incarnation = (s.g.0 > 0).then(|| api::Incarnation {
        g: s.g.0,
        mode: match s.target.mode {
            Mode::Headless => api::Mode::Headless,
            Mode::Pane => api::Mode::Pane,
        },
        harness: api_harness(s.target.harness),
        subscription: subscription(&s.target),
        provider: s.target.provider.clone(),
        model: s.target.model.clone(),
        effort: (stats.g == s.g.0).then(|| stats.effort.clone()).flatten(),
        native_id: s.session.clone(),
        activity: api::Activity {
            state: activity(&s.activity),
            since_ms: EpochMs(view.since.get(&s.id).copied().unwrap_or(0)),
            seen: match s.activity {
                Activity::Idle { seen } => seen,
                _ => true,
            },
            request_id: match &s.activity {
                Activity::Delivering { effect } => st.effects.get(effect).map(|e| e.key.clone()),
                _ => None,
            },
        },
        started_at_ms: EpochMs(stats.started_ms),
        turn: stats.turns,
        context_tokens: stats.context_tokens,
        context_window: stats.context_window,
        cache_ratio: None,
    });
    api::Sleeve {
        id: s.id.0.to_string(),
        name: s.name.clone(),
        workspace_id: format!("w{}", root_of(st, s).0),
        parent_id: s.parent.map(|p| p.0.to_string()),
        role: role(st, s),
        lifecycle: lifecycle(s.life),
        park_reason: match s.life {
            Life::Parked(r) => Some(park_wire(r)),
            _ => None,
        },
        path: s.claim.path.clone(),
        resleeve_to: s
            .resleeve_to
            .iter()
            .map(|t| api_harness(t.harness))
            .collect(),
        branch: (s.layout == Layout::Worktree).then(|| format!("pt/{}", s.name)),
        view: match s.layout {
            Layout::Root => None,
            Layout::Worktree => Some(api::WorkspaceView::Worktree),
            Layout::View => Some(api::WorkspaceView::Readonly),
        },
        incarnation,
        queued: s.queue.len() as u64,
        budget_used: s.budget.map(|_| s.used),
        budget_limit: s.budget,
        d9: s.d9,
    }
}

pub fn workspace(s: &Sleeve) -> api::Workspace {
    api::Workspace {
        id: format!("w{}", s.id.0),
        name: s.claim.path.rsplit('/').next().unwrap_or("").to_string(),
        path: s.claim.path.clone(),
    }
}

fn tool_kind(k: drv::ToolKind) -> api::ToolKind {
    use api::ToolKind as K;
    match k {
        drv::ToolKind::Command => K::Command,
        drv::ToolKind::Read => K::Read,
        drv::ToolKind::Edit => K::Edit,
        drv::ToolKind::Search => K::Search,
        drv::ToolKind::Web => K::Web,
        drv::ToolKind::Mcp => K::Mcp,
        drv::ToolKind::Subagent => K::Subagent,
        drv::ToolKind::Other => K::Other,
    }
}

pub fn decision(d: drv::Decision) -> api::Decision {
    match d {
        drv::Decision::AllowOnce => api::Decision::AllowOnce,
        drv::Decision::AllowSession => api::Decision::AllowSession,
        drv::Decision::Deny | drv::Decision::Cancel => api::Decision::Deny,
    }
}

pub fn decided_by(by: Option<&str>) -> api::DecidedBy {
    match by {
        Some("owner") => api::DecidedBy::Owner,
        Some("envoy") => api::DecidedBy::Envoy,
        Some("rule") => api::DecidedBy::Rule,
        _ => api::DecidedBy::Autonomous,
    }
}

fn stop(s: drv::Stop) -> api::Stop {
    match s {
        drv::Stop::EndTurn => api::Stop::EndTurn,
        drv::Stop::Cancelled => api::Stop::Cancelled,
        drv::Stop::Error => api::Stop::Error,
        drv::Stop::MaxTurns => api::Stop::MaxTurns,
        drv::Stop::Other => api::Stop::Other,
    }
}

fn error_class(c: drv::ErrorClass) -> api::ErrorClass {
    match c {
        drv::ErrorClass::Auth => api::ErrorClass::Auth,
        drv::ErrorClass::Quota => api::ErrorClass::Quota,
        drv::ErrorClass::RateLimit => api::ErrorClass::RateLimit,
        drv::ErrorClass::Protocol => api::ErrorClass::Protocol,
        drv::ErrorClass::Crash => api::ErrorClass::Crash,
        drv::ErrorClass::Other => api::ErrorClass::Other,
    }
}

/// The approval id an entry names: action approvals carry theirs, tool approvals are the
/// incarnation's.
fn approval_id(sleeve: SleeveId, g: u64, id: &str) -> String {
    if id.starts_with(pt_core::ACTION_PREFIX) {
        id.to_string()
    } else {
        pt_core::tool_approval_id(sleeve, pt_core::Gen(g), id)
    }
}

/// The API entry of a stored line; `delta` for a live streaming piece.
pub fn chat_entry(sleeve: SleeveId, l: &Line, delta: bool) -> api::ChatEntry {
    use api::Entry as A;
    use drv::Entry as D;
    let entry = match &l.entry {
        D::SessionInfo {
            harness,
            native_id,
            model,
            provider,
            effort,
            ..
        } => A::SessionInfo {
            harness: l.harness.unwrap_or(match harness {
                drv::Harness::Claude => api::Harness::Claude,
                drv::Harness::Codex => api::Harness::Codex,
                drv::Harness::Grok => api::Harness::Grok,
                drv::Harness::Pi => api::Harness::PiVendor,
            }),
            mode: api::Mode::Headless,
            native_id: native_id.clone(),
            model: model.clone(),
            provider: provider.clone(),
            effort: effort.clone(),
        },
        D::UserMessage {
            request_id,
            from,
            text,
            steer,
        } => A::UserMessage {
            request_id: request_id.clone(),
            from: from.clone(),
            text: text.clone(),
            steer: *steer,
        },
        D::PromptAck {
            request_id,
            ack,
            detail,
            ..
        } => A::PromptAck {
            request_id: request_id.clone(),
            ack: match ack {
                drv::AckKind::Confirmed => api::AckKind::Confirmed,
                drv::AckKind::Rejected => api::AckKind::Rejected,
            },
            detail: detail.clone(),
        },
        D::AssistantText { id, text, .. } => A::AssistantText {
            id: id.clone(),
            text: text.clone(),
            delta,
        },
        D::Reasoning { id, text, .. } => A::Reasoning {
            id: id.clone(),
            text: text.clone(),
            delta,
        },
        D::ToolCall {
            id,
            tool,
            name,
            summary,
            input,
            truncated,
        } => A::ToolCall {
            id: id.clone(),
            tool: tool_kind(*tool),
            name: name.clone(),
            summary: summary.clone(),
            input: input.clone(),
            truncated: *truncated,
        },
        D::ToolResult {
            id,
            status,
            output,
            truncated,
            duration_ms,
        } => A::ToolResult {
            id: id.clone(),
            status: match status {
                drv::ToolStatus::Completed => api::ToolStatus::Completed,
                drv::ToolStatus::Failed => api::ToolStatus::Failed,
                drv::ToolStatus::Declined => api::ToolStatus::Declined,
                drv::ToolStatus::Cancelled => api::ToolStatus::Cancelled,
            },
            output: output.clone(),
            truncated: *truncated,
            duration_ms: *duration_ms,
        },
        D::FileChange { tool_id, paths } => A::FileChange {
            tool_id: tool_id.clone(),
            files: paths
                .iter()
                .map(|p| api::FileDelta {
                    path: p.path.clone(),
                    op: p.op.clone(),
                    added: None,
                    removed: None,
                })
                .collect(),
            diff_ref: None,
        },
        D::ApprovalRequest {
            id, name, summary, ..
        } => A::ApprovalRequest {
            approval_id: approval_id(sleeve, l.g, id),
            title: name.clone(),
            summary: summary.clone(),
        },
        D::ApprovalDecision {
            id,
            decision: d,
            by,
            ..
        } => A::ApprovalDecision {
            approval_id: approval_id(sleeve, l.g, id),
            decision: decision(*d),
            by: decided_by(by.as_deref()),
        },
        D::TurnStarted { request_id, .. } => A::TurnStarted {
            request_id: request_id.clone(),
        },
        D::TurnSettled {
            stop: s,
            native_stop,
            usage,
            duration_ms,
        } => A::TurnSettled {
            stop: stop(*s),
            native_stop: native_stop.clone(),
            usage: api::Usage {
                input: usage.input,
                output: usage.output,
                cache_read: usage.cache_read,
                cache_write: usage.cache_write,
                cost_usd: usage.cost_usd,
                context_tokens: usage.context_tokens,
                context_window: usage.context_window,
            },
            duration_ms: *duration_ms,
        },
        D::Error { class, text } => A::Error {
            kind: error_class(*class),
            text: text.clone(),
        },
        D::Compaction {
            trigger,
            tokens_before,
            tokens_after,
        } => A::Compaction {
            trigger: trigger.clone(),
            tokens_before: *tokens_before,
            tokens_after: *tokens_after,
        },
        D::Notice { code, text } => A::Notice {
            level: l.level.unwrap_or(api::Level::Info),
            code: code.clone(),
            text: text.clone(),
        },
        D::Exited {
            code,
            signal,
            requested,
            stderr_tail,
        } => A::Exited {
            code: *code,
            signal: *signal,
            requested: *requested,
            stderr_tail: stderr_tail.clone(),
        },
    };
    api::ChatEntry {
        seq: l.seq,
        g: l.g,
        turn: l.turn,
        at_ms: EpochMs(l.time_ms),
        entry,
    }
}
