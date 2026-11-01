//! JSON shapes of the API, matching `app/lib/model.dart` (snake_case keys, `*_ms` times, enums as
//! lowercase strings). Temporary: pt-api replaces this module with its generated types.

use pt_core::{
    Activity, GuardState, Harness, Layout, Life, Mode, ParkReason, Sleeve, SleeveId, State, Target,
};
use pt_drivers::{AckKind, Entry, Event, ToolStatus};
use pt_json::{Map, Value, obj};

use crate::home::Config;
use crate::hub::{Hub, View};

pub fn harness_wire(h: Harness) -> &'static str {
    match h {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
        Harness::Grok => "grok",
        Harness::Antigravity => "antigravity",
        Harness::PiVendor => "pi_vendor",
        Harness::PiLocal => "pi_local",
        Harness::PiDurable => "pi_durable",
    }
}

pub fn harness_from(s: &str) -> Option<Harness> {
    Some(match s {
        "claude" => Harness::Claude,
        "codex" => Harness::Codex,
        "grok" => Harness::Grok,
        "antigravity" => Harness::Antigravity,
        "pi_vendor" => Harness::PiVendor,
        "pi_local" => Harness::PiLocal,
        "pi_durable" => Harness::PiDurable,
        _ => return None,
    })
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
        Harness::PiLocal | Harness::PiDurable => (
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

pub fn life_wire(l: Life) -> &'static str {
    match l {
        Life::Provisioning => "provisioning",
        Life::Launching => "launching",
        Life::Running => "running",
        Life::Recovering => "recovering",
        Life::Resleeving => "resleeving",
        Life::Stopping { .. } => "stopping",
        Life::Stopped => "stopped",
        Life::Parked(_) => "parked",
        Life::Removed => "removed",
    }
}

pub fn activity_wire(a: &Activity) -> &'static str {
    match a {
        Activity::Booting => "booting",
        Activity::Idle { .. } => "idle",
        Activity::Delivering { .. } => "delivering",
        Activity::Working => "working",
        Activity::Blocked { .. } => "blocked",
        Activity::Interrupting => "interrupting",
        Activity::Compacting => "compacting",
        Activity::Unknown => "unknown",
        Activity::Ended => "ended",
    }
}

pub fn park_wire(r: ParkReason) -> String {
    match r {
        ParkReason::Provision => "provision".into(),
        ParkReason::NoTarget => "no_target".into(),
        ParkReason::Launch => "launch".into(),
        ParkReason::Flapping => "flapping".into(),
        ParkReason::BriefUnconfirmed => "brief_unconfirmed".into(),
        ParkReason::Guard(g) => format!("g{}", g.number()),
    }
}

fn guard_wire(g: GuardState) -> &'static str {
    match g {
        GuardState::Armed => "armed",
        GuardState::Interrupting => "interrupting",
        GuardState::Resuming => "resuming",
        GuardState::Quiescing { .. } => "quiescing",
        GuardState::Capturing { .. } => "capturing",
        GuardState::Selecting => "selecting",
        GuardState::Launching => "launching",
        GuardState::Briefing { .. } => "briefing",
        GuardState::Cooldown { .. } => "cooldown",
        GuardState::Parked => "parked",
    }
}

fn subscription(t: &Target) -> &'static str {
    match t.harness {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
        Harness::Grok => "grok",
        Harness::PiLocal => "local",
        _ => "unknown",
    }
}

pub fn root_of(state: &State, s: &Sleeve) -> SleeveId {
    s.parent
        .and_then(|p| state.sleeve(p))
        .map_or(s.id, |p| p.id)
}

pub fn sleeve(hub: &Hub, view: &View, s: &Sleeve) -> Value {
    let st = &view.state;
    let role = if s.parent.is_some() {
        "child"
    } else if Some(s.id) == Hub::envoy_id(st) {
        "envoy"
    } else {
        "root"
    };
    let stats = hub.transcripts.stats(s.id);
    let incarnation = (s.g.0 > 0).then(|| {
        let a = &s.activity;
        let seen = match a {
            Activity::Idle { seen } => *seen,
            _ => true,
        };
        let mail = match a {
            Activity::Delivering { effect } => st.effects.get(effect).map(|e| e.key.clone()),
            _ => None,
        };
        obj! {
            "g" => s.g.0,
            "mode" => match s.target.mode { Mode::Headless => "headless", Mode::Pane => "pane" },
            "harness" => harness_wire(s.target.harness),
            "subscription" => subscription(&s.target),
            "provider" => s.target.provider.as_str(),
            "model" => s.target.model.as_str(),
            "activity" => obj! {
                "state" => activity_wire(a),
                "since_ms" => view.since.get(&s.id).copied().unwrap_or(0),
                "seen" => seen,
                "mail_id" => mail,
            },
            "started_at_ms" => stats.started_ms,
            "turn" => stats.turns,
            "context_tokens" => stats.context_tokens,
            "context_window" => stats.context_window,
        }
    });
    let branch = (s.layout == Layout::Worktree).then(|| format!("pt/{}", s.name));
    obj! {
        "id" => s.id.0.to_string(),
        "name" => s.name.as_str(),
        "workspace_id" => format!("w{}", root_of(st, s).0),
        "parent_id" => s.parent.map(|p| p.0.to_string()),
        "role" => role,
        "lifecycle" => life_wire(s.life),
        "park_reason" => match s.life { Life::Parked(r) => Some(park_wire(r)), _ => None },
        "guard" => guard_wire(s.guard.state),
        "path" => s.claim.path.as_str(),
        "layout" => match s.layout { Layout::Root => "root", Layout::Worktree => "worktree", Layout::View => "view" },
        "resleeve_to" => s.resleeve_to.iter().map(|t| Value::from(harness_wire(t.harness))).collect::<Vec<_>>(),
        "branch" => branch,
        "queue" => s.queue.len(),
        "incarnation" => incarnation,
        "budget_used" => s.budget.map(|_| s.used),
        "budget_limit" => s.budget,
    }
}

pub fn approvals(view: &View, visible: &dyn Fn(SleeveId) -> bool) -> Vec<Value> {
    let mut out = Vec::new();
    for s in view.state.sleeves.values().filter(|s| visible(s.id)) {
        if let Activity::Blocked { approvals } = &s.activity {
            for a in approvals {
                out.push(obj! {
                    "id" => format!("{}:{a}", s.id.0),
                    "sleeve_id" => s.id.0.to_string(),
                    "kind" => "tool",
                    "title" => a.as_str(),
                    "summary" => "",
                    "detail" => "",
                    "requested_at_ms" => s.blocked_at,
                    "timeout_at_ms" => s.blocked_at + pt_core::limits::APPROVAL_TIMEOUT,
                    "state" => "pending",
                });
            }
        }
    }
    out
}

/// The full snapshot (`HubState` in the app), limited to the sleeves `visible` admits.
pub fn state(hub: &Hub, view: &View, visible: &dyn Fn(SleeveId) -> bool) -> Value {
    let st = &view.state;
    let shown: Vec<&Sleeve> = st
        .sleeves
        .values()
        .filter(|s| s.life != Life::Removed && visible(s.id))
        .collect();
    let workspaces: Vec<Value> = shown
        .iter()
        .filter(|s| s.parent.is_none())
        .map(|s| {
            let name = s.claim.path.rsplit('/').next().unwrap_or("");
            obj! { "id" => format!("w{}", s.id.0), "name" => name, "path" => s.claim.path.as_str() }
        })
        .collect();
    obj! {
        "hub" => obj! {
            "name" => "protectorate",
            "version" => option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or("dev"),
            "started_at_ms" => hub.started_ms,
        },
        "workspaces" => workspaces,
        "sleeves" => shown.iter().map(|s| sleeve(hub, view, s)).collect::<Vec<_>>(),
        "approvals" => approvals(view, visible),
        "nodes" => Vec::<Value>::new(),
        "jobs" => Vec::<Value>::new(),
        "quotas" => Vec::<Value>::new(),
        "events" => hub.bus.events(),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// One chat line (`ChatEntry` in the app) from a stored transcript entry. `type` keeps the
/// normalized entry kind for clients that want more than the chat shape.
pub fn chat_entry(e: &Event, streaming: bool) -> Value {
    let mut m = Map::new();
    m.insert("seq", e.seq);
    m.insert("at_ms", e.time_ms);
    m.insert("g", e.g);
    let mut put = |k: &str, v: Value| {
        m.insert(k, v);
    };
    let notice = |put: &mut dyn FnMut(&str, Value), level: &str, text: String| {
        put("kind", "notice".into());
        put("level", level.into());
        put("text", text.into());
    };
    match &e.entry {
        Entry::UserMessage {
            request_id,
            from,
            text,
            steer,
        } => {
            put("type", "user_message".into());
            put("kind", "user".into());
            put("from", from.as_str().into());
            put("text", text.as_str().into());
            put("request_id", request_id.as_str().into());
            put("steer", (*steer).into());
        }
        Entry::AssistantText { id, text, .. } => {
            put("type", "assistant_text".into());
            put("kind", "assistant".into());
            put("call_id", id.as_str().into());
            put("text", text.as_str().into());
            put("streaming", streaming.into());
        }
        Entry::Reasoning { id, text, .. } => {
            put("type", "reasoning".into());
            put("kind", "reasoning".into());
            put("call_id", id.as_str().into());
            put("text", text.as_str().into());
            put("streaming", streaming.into());
        }
        Entry::ToolCall {
            id,
            name,
            summary,
            input,
            ..
        } => {
            put("type", "tool_call".into());
            put("kind", "tool".into());
            put("call_id", id.as_str().into());
            put("tool", name.as_str().into());
            put("text", summary.as_str().into());
            put(
                "input",
                input
                    .as_ref()
                    .map(|v| clip(&v.to_string(), 2000))
                    .unwrap_or_default()
                    .into(),
            );
            put("status", "running".into());
        }
        Entry::ToolResult {
            id, status, output, ..
        } => {
            put("type", "tool_result".into());
            put("kind", "tool".into());
            put("call_id", id.as_str().into());
            put("output", output.as_str().into());
            let ok = *status == ToolStatus::Completed;
            put("status", if ok { "ok" } else { "error" }.into());
            put("ended_at_ms", e.time_ms.into());
        }
        Entry::ApprovalRequest {
            id, name, summary, ..
        } => {
            put("type", "approval_request".into());
            put("kind", "approval".into());
            put("approval_id", id.as_str().into());
            put("tool", name.as_str().into());
            put("text", summary.as_str().into());
        }
        Entry::ApprovalDecision { id, decision, .. } => {
            put("type", "approval_decision".into());
            put("approval_id", id.as_str().into());
            notice(&mut put, "info", format!("approval {id}: {decision}"));
        }
        Entry::SessionInfo {
            harness,
            native_id,
            model,
            ..
        } => {
            put("type", "session_info".into());
            let model = model.as_deref().unwrap_or("default model");
            notice(
                &mut put,
                "info",
                format!("{harness} {model}, g{}, session {native_id}", e.g),
            );
        }
        Entry::PromptAck {
            request_id, ack, ..
        } => {
            put("type", "prompt_ack".into());
            put("request_id", request_id.as_str().into());
            let level = if *ack == AckKind::Confirmed {
                "info"
            } else {
                "warn"
            };
            notice(&mut put, level, format!("prompt {request_id} {ack}"));
        }
        Entry::TurnStarted { .. } => {
            put("type", "turn_started".into());
            notice(&mut put, "info", "turn started".into());
        }
        Entry::TurnSettled { stop, usage, .. } => {
            put("type", "turn_settled".into());
            notice(
                &mut put,
                "info",
                format!(
                    "turn settled: {stop}, {} in / {} out tokens",
                    usage.input, usage.output
                ),
            );
        }
        Entry::Error { class, text } => {
            put("type", "error".into());
            put("class", class.as_str().into());
            notice(&mut put, "alarm", text.clone());
        }
        Entry::Compaction { tokens_before, .. } => {
            put("type", "compaction".into());
            notice(
                &mut put,
                "info",
                format!("compacted ({} tokens before)", tokens_before.unwrap_or(0)),
            );
        }
        Entry::FileChange { paths, .. } => {
            put("type", "file_change".into());
            let list: Vec<&str> = paths.iter().map(|p| p.path.as_str()).collect();
            notice(&mut put, "info", format!("changed {}", list.join(", ")));
        }
        Entry::Notice { code, text } => {
            put("type", "notice".into());
            put("code", code.clone().into());
            notice(&mut put, "info", text.clone());
        }
        Entry::Exited {
            code,
            signal,
            requested,
            ..
        } => {
            put("type", "exited".into());
            let how = match (code, signal) {
                (Some(c), _) => format!("exit code {c}"),
                (_, Some(s)) => format!("signal {s}"),
                _ => "unknown status".into(),
            };
            let level = if *requested { "info" } else { "warn" };
            notice(&mut put, level, format!("harness exited ({how})"));
        }
    }
    Value::Object(m)
}
