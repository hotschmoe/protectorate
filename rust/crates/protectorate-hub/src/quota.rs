use super::{boolean, input, respond, timestamp, EventBus, Hub};
use chrono::Utc;
use protectorate_quota::{
    self as quotas, Freshness, Harness, HarnessSuggestion, ProviderQuota, QuotaCache, Reader,
    TaskSize,
};
use protectorate_wire::{Code, Result, RpcError, UnaryServer};
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) fn default_readers() -> Vec<Arc<dyn Reader>> {
    vec![
        Arc::new(quotas::Claude::new()),
        Arc::new(quotas::Codex::new()),
        Arc::new(quotas::Grok::new()),
        Arc::new(quotas::Antigravity::new()),
    ]
}

pub(crate) fn publish_updates(cache: &QuotaCache, events: Arc<EventBus>) {
    cache.set_on_update(move |quota| {
        events.publish("quota.updated", &quota.provider, quota_json(quota));
    });
}

pub(crate) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let get_hub = Arc::clone(&hub);
    server.register("/protectorate.v1.QuotaService/GetQuota", move |request| {
        let hub = Arc::clone(&get_hub);
        async move {
            let refresh = boolean(&input(&request)?, "refresh");
            if refresh {
                refresh_cache(Arc::clone(&hub)).await?;
            }
            let quotas = hub.quota.snapshot(Utc::now());
            respond(
                &request,
                json!({"quotas": quotas.iter().map(quota_json).collect::<Vec<_>>()}),
            )
        }
    })?;
    server.register("/protectorate.v1.QuotaService/Suggest", move |request| {
        let hub = Arc::clone(&hub);
        async move {
            let body = input(&request)?;
            let size = task_size(body.get("taskSize"))?;
            if boolean(&body, "refresh") {
                refresh_cache(Arc::clone(&hub)).await?;
            }
            let now = Utc::now();
            let quotas = hub.quota.snapshot(now);
            let suggestions = quotas::rank_harnesses(&quotas, size, now);
            respond(
                &request,
                json!({
                    "suggestions": suggestions.iter().map(suggestion_json).collect::<Vec<_>>(),
                    "generatedAt": timestamp(now),
                }),
            )
        }
    })?;
    Ok(())
}

async fn refresh_cache(hub: Arc<Hub>) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        hub.quota
            .refresh(&quotas::Ctx::now(), hub.quota_transport.as_ref(), true);
    })
    .await
    .map_err(|error| {
        RpcError::new(
            Code::Internal,
            format!("quota refresh task failed: {error}"),
        )
    })
}

fn task_size(value: Option<&Value>) -> Result<TaskSize> {
    match value {
        None | Some(Value::Null) => Ok(TaskSize::Medium),
        Some(Value::String(name)) => match name.as_str() {
            "TASK_SIZE_UNSPECIFIED" | "TASK_SIZE_MEDIUM" => Ok(TaskSize::Medium),
            "TASK_SIZE_SMALL" => Ok(TaskSize::Small),
            "TASK_SIZE_LARGE" => Ok(TaskSize::Large),
            _ => Err(RpcError::new(Code::InvalidArgument, "invalid task size")),
        },
        Some(Value::Number(number)) => match number.as_i64() {
            Some(0 | 2) => Ok(TaskSize::Medium),
            Some(1) => Ok(TaskSize::Small),
            Some(3) => Ok(TaskSize::Large),
            _ => Err(RpcError::new(Code::InvalidArgument, "invalid task size")),
        },
        _ => Err(RpcError::new(Code::InvalidArgument, "invalid task size")),
    }
}

fn freshness(value: Freshness) -> &'static str {
    match value {
        Freshness::Fresh => "FRESHNESS_FRESH",
        Freshness::Stale => "FRESHNESS_STALE",
        Freshness::Unknown => "FRESHNESS_UNKNOWN",
    }
}

fn harness(value: Harness) -> &'static str {
    match value {
        Harness::Claude => "HARNESS_CLAUDE",
        Harness::Codex => "HARNESS_CODEX",
        Harness::Grok => "HARNESS_GROK",
        Harness::Pi => "HARNESS_PI",
    }
}

fn duration_seconds(value: u64) -> String {
    format!("{value}s")
}

fn duration(value: std::time::Duration) -> String {
    let seconds = value.as_secs();
    let nanos = value.subsec_nanos();
    if nanos == 0 {
        return duration_seconds(seconds);
    }
    if nanos.is_multiple_of(1_000_000) {
        format!("{seconds}.{:03}s", nanos / 1_000_000)
    } else if nanos.is_multiple_of(1_000) {
        format!("{seconds}.{:06}s", nanos / 1_000)
    } else {
        format!("{seconds}.{nanos:09}s")
    }
}

fn duration_millis(value: u64) -> String {
    let seconds = value / 1_000;
    let millis = value % 1_000;
    if millis == 0 {
        duration_seconds(seconds)
    } else {
        format!("{seconds}.{millis:03}s")
    }
}

pub(crate) fn quota_json(quota: &ProviderQuota) -> Value {
    let mut out = json!({
        "freshness": freshness(quota.freshness),
    });
    if !quota.provider.is_empty() {
        out["provider"] = json!(quota.provider);
    }
    if !quota.windows.is_empty() {
        out["windows"] = json!(quota
            .windows
            .iter()
            .map(|window| {
                let mut value = json!({});
                if !window.name.is_empty() {
                    value["name"] = json!(window.name);
                }
                if window.used_percent != 0.0 {
                    value["usedPercent"] = json!(window.used_percent);
                }
                if window.window_seconds != 0 {
                    value["windowSeconds"] = json!(window.window_seconds.to_string());
                }
                if let Some(at) = window.resets_at {
                    value["resetsAt"] = json!(timestamp(at));
                }
                if let Some(estimated) = window.estimated_unused_percent_at_reset {
                    value["estimatedUnusedPercentAtReset"] = json!(estimated);
                }
                value
            })
            .collect::<Vec<_>>());
    }
    if !quota.resets.is_empty() {
        out["resets"] = json!(quota
            .resets
            .iter()
            .map(|credit| {
                let mut value = json!({});
                if !credit.kind.is_empty() {
                    value["kind"] = json!(credit.kind);
                }
                if credit.count != 0 {
                    value["count"] = json!(credit.count);
                }
                if let Some(at) = credit.expires_at {
                    value["expiresAt"] = json!(timestamp(at));
                }
                value
            })
            .collect::<Vec<_>>());
    }
    if !quota.plan.is_empty() {
        out["plan"] = json!(quota.plan);
    }
    if !quota.source.is_empty() {
        out["source"] = json!(quota.source);
    }
    if !quota.error.is_empty() {
        out["error"] = json!(quota.error);
    }
    if !quota.models.is_empty() {
        out["models"] = json!(quota.models);
    }
    if let Some(at) = quota.fetched_at {
        out["fetchedAt"] = json!(timestamp(at));
    }
    if let Some(age) = quota.age {
        out["age"] = json!(duration(age));
    }
    if let Some(at) = quota.checked_at {
        out["checkedAt"] = json!(timestamp(at));
    }
    if let Some(latency) = quota.latency.filter(|latency| *latency > 0) {
        out["latency"] = json!(duration_millis(latency));
    }
    out
}

fn suggestion_json(suggestion: &HarnessSuggestion) -> Value {
    let mut out = json!({
        "harness": harness(suggestion.harness),
        "freshness": freshness(suggestion.freshness),
    });
    if suggestion.rank != 0 {
        out["rank"] = json!(suggestion.rank);
    }
    if !suggestion.provider.is_empty() {
        out["provider"] = json!(suggestion.provider);
    }
    if suggestion.recommended {
        out["recommended"] = json!(true);
    }
    if suggestion.score != 0.0 {
        out["score"] = json!(suggestion.score);
    }
    if !suggestion.window.is_empty() {
        out["window"] = json!(suggestion.window);
    }
    if !suggestion.reason.is_empty() {
        out["reason"] = json!(suggestion.reason);
    }
    if let Some(value) = suggestion.usable_percent {
        out["usablePercent"] = json!(value);
    }
    if let Some(value) = suggestion.expiring_percent {
        out["expiringPercent"] = json!(value);
    }
    if let Some(seconds) = suggestion.resets_in {
        out["resetsIn"] = json!(duration_seconds(seconds));
    }
    out
}
