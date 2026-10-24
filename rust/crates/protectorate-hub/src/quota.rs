use super::{proto_time, EventBus, Hub};
use chrono::Utc;
use prost_reflect::ReflectMessage;
use protectorate_quota::{
    self as quotas, Freshness, Harness, HarnessSuggestion, ProviderQuota, QuotaCache, Reader,
    TaskSize,
};
use protectorate_wire::{
    self as wire,
    v1::{self, quota_service},
    Call, Code, Result, RpcError, UnaryServer,
};
use std::{sync::Arc, time::Duration};

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
        match serde_json::to_value(provider_quota(quota).transcode_to_dynamic()) {
            Ok(payload) => events.publish("quota.updated", &quota.provider, payload),
            Err(error) => eprintln!("quota {}: encode update: {error}", quota.provider),
        }
    });
}

pub(crate) fn register(server: &mut UnaryServer, hub: Arc<Hub>) -> Result<()> {
    let get_hub = Arc::clone(&hub);
    server.unary(
        quota_service::GET_QUOTA,
        move |call: Call<v1::GetQuotaRequest>| {
            let hub = Arc::clone(&get_hub);
            async move {
                if call.request.refresh {
                    refresh_cache(Arc::clone(&hub)).await?;
                }
                Ok(v1::GetQuotaResponse {
                    quotas: hub
                        .quota
                        .snapshot(Utc::now())
                        .iter()
                        .map(provider_quota)
                        .collect(),
                })
            }
        },
    )?;
    server.unary(
        quota_service::SUGGEST,
        move |call: Call<v1::SuggestRequest>| {
            let hub = Arc::clone(&hub);
            async move {
                let size = task_size(call.request.task_size)?;
                if call.request.refresh {
                    refresh_cache(Arc::clone(&hub)).await?;
                }
                let now = Utc::now();
                let quotas = hub.quota.snapshot(now);
                Ok(v1::SuggestResponse {
                    suggestions: quotas::rank_harnesses(&quotas, size, now)
                        .iter()
                        .map(suggestion)
                        .collect(),
                    generated_at: Some(proto_time(now)),
                })
            }
        },
    )?;
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

fn task_size(value: i32) -> Result<TaskSize> {
    match v1::TaskSize::try_from(value) {
        Ok(v1::TaskSize::Unspecified | v1::TaskSize::Medium) => Ok(TaskSize::Medium),
        Ok(v1::TaskSize::Small) => Ok(TaskSize::Small),
        Ok(v1::TaskSize::Large) => Ok(TaskSize::Large),
        Err(_) => Err(RpcError::new(Code::InvalidArgument, "invalid task size")),
    }
}

fn freshness(value: Freshness) -> i32 {
    let value = match value {
        Freshness::Fresh => v1::Freshness::Fresh,
        Freshness::Stale => v1::Freshness::Stale,
        Freshness::Unknown => v1::Freshness::Unknown,
    };
    value as i32
}

fn harness(value: Harness) -> i32 {
    let value = match value {
        Harness::Claude => v1::Harness::Claude,
        Harness::Codex => v1::Harness::Codex,
        Harness::Grok => v1::Harness::Grok,
        Harness::Pi => v1::Harness::Pi,
    };
    value as i32
}

fn provider_quota(quota: &ProviderQuota) -> v1::ProviderQuota {
    v1::ProviderQuota {
        provider: quota.provider.clone(),
        plan: quota.plan.clone(),
        source: quota.source.clone(),
        freshness: freshness(quota.freshness),
        fetched_at: quota.fetched_at.map(proto_time),
        age: quota.age.map(wire::proto_duration),
        error: quota.error.clone(),
        checked_at: quota.checked_at.map(proto_time),
        windows: quota
            .windows
            .iter()
            .map(|window| v1::QuotaWindow {
                name: window.name.clone(),
                used_percent: window.used_percent,
                resets_at: window.resets_at.map(proto_time),
                window_seconds: window.window_seconds,
                estimated_unused_percent_at_reset: window.estimated_unused_percent_at_reset,
            })
            .collect(),
        resets: quota
            .resets
            .iter()
            .map(|credit| v1::ResetCredit {
                kind: credit.kind.clone(),
                count: credit.count,
                expires_at: credit.expires_at.map(proto_time),
            })
            .collect(),
        models: quota.models.clone(),
        latency: quota
            .latency
            .filter(|latency| *latency > 0)
            .map(|millis| wire::proto_duration(Duration::from_millis(millis))),
    }
}

fn suggestion(suggestion: &HarnessSuggestion) -> v1::HarnessSuggestion {
    v1::HarnessSuggestion {
        rank: suggestion.rank,
        harness: harness(suggestion.harness),
        provider: suggestion.provider.clone(),
        recommended: suggestion.recommended,
        score: suggestion.score,
        freshness: freshness(suggestion.freshness),
        usable_percent: suggestion.usable_percent,
        expiring_percent: suggestion.expiring_percent,
        window: suggestion.window.clone(),
        resets_in: suggestion
            .resets_in
            .map(|seconds| wire::proto_duration(Duration::from_secs(seconds))),
        reason: suggestion.reason.clone(),
    }
}
