//! Grok subscription usage from the Grok CLI's login (`~/.grok/auth.json`) and the CLI chat proxy
//! billing endpoint.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pt_api::Subscription;
use pt_json::Value;

use crate::http::{self, Get};
use crate::login::{home_dir, read_login};
use crate::reader::{Reader, Snapshot, window};
use crate::{Error, Result, parse_rfc3339};

const DEFAULT_PROXY: &str = "https://cli-chat-proxy.grok.com";

#[derive(Clone, Debug, Default)]
pub struct Grok {
    pub home: Option<PathBuf>,
    pub base_url: Option<String>,
}

pub struct Login {
    key: String,
    pub expires_at_ms: Option<u64>,
}

/// Picks the entry the Grok CLI wrote, preferring the auth.x.ai account.
pub fn pick_login(raw: &[u8]) -> Result<Login> {
    let v = pt_json::parse_bytes(raw).map_err(|e| Error::other(format!("decode login: {e}")))?;
    let all = v
        .as_object()
        .ok_or_else(|| Error::other("login file is not an object"))?;
    for prefix in ["https://auth.x.ai::", "https://accounts.x.ai/sign-in"] {
        for (k, entry) in all.iter() {
            if !k.starts_with(prefix) {
                continue;
            }
            let key = entry.get("key").and_then(Value::as_str).unwrap_or("");
            if !key.is_empty() {
                return Ok(Login {
                    key: key.to_string(),
                    expires_at_ms: entry
                        .get("expires_at")
                        .and_then(Value::as_str)
                        .and_then(parse_rfc3339),
                });
            }
        }
    }
    Err(Error::unavailable("not logged in (no Grok login entry)"))
}

fn amount(v: Option<&Value>) -> Option<f64> {
    let val = v?.get("val")?;
    match val {
        Value::String(s) => s.parse::<f64>().ok(),
        other => other.as_f64(),
    }
    .filter(|f| f.is_finite())
}

fn label(start: Option<u64>, end: Option<u64>) -> &'static str {
    let (Some(s), Some(e)) = (start, end) else {
        return "credits";
    };
    if e <= s {
        return "credits";
    }
    let days = (e - s) as f64 / 86_400_000.0;
    if (6.0..=8.0).contains(&days) {
        "wk"
    } else if (27.0..=32.0).contains(&days) {
        "mo"
    } else {
        "credits"
    }
}

/// The plan and credit window of a `/v1/billing?format=credits` response.
pub fn parse_billing(body: &[u8]) -> Result<Snapshot> {
    let v = pt_json::parse_bytes(body).map_err(|e| Error::other(format!("decode billing: {e}")))?;
    if v.as_object().is_none() {
        return Err(Error::other("billing response is not an object"));
    }
    let text = |p: &str| v.pointer(p).and_then(Value::as_str).unwrap_or("");
    let mut snap = Snapshot {
        plan: text("/subscriptionTier").to_string(),
        ..Snapshot::default()
    };
    let cfg = v
        .get("config")
        .ok_or_else(|| Error::other("billing response missing config"))?;
    if snap.plan.is_empty() {
        snap.plan = text("/config/subscriptionTier").to_string();
    }
    let time = |p: &str| v.pointer(p).and_then(Value::as_str).and_then(parse_rfc3339);
    let mut start = time("/config/billingPeriodStart");
    let mut end = time("/config/billingPeriodEnd");
    if let Some(e) = time("/config/currentPeriod/end") {
        start = time("/config/currentPeriod/start");
        end = Some(e);
    }
    let pct = cfg
        .get("creditUsagePercent")
        .and_then(Value::as_f64)
        .or_else(|| {
            let used = amount(cfg.get("onDemandUsed"))?;
            let cap = amount(cfg.get("onDemandCap"))?;
            (cap > 0.0).then(|| used / cap * 100.0)
        });
    let pct = pct
        .filter(|p| p.is_finite() && *p >= 0.0)
        .ok_or_else(|| Error::other("billing response missing valid credit usage"))?;
    snap.windows.push(window(label(start, end), pct, end));
    Ok(snap)
}

impl Reader for Grok {
    fn subscription(&self) -> Subscription {
        Subscription::Grok
    }

    fn read(&self, now_ms: u64, timeout: Duration) -> Result<Snapshot> {
        let started = Instant::now();
        let left = || timeout.saturating_sub(started.elapsed());
        let home = home_dir(self.home.as_deref())?;
        let login = pick_login(&read_login(&home.join(".grok").join("auth.json"))?)?;
        if login.expires_at_ms.is_some_and(|e| now_ms >= e) {
            return Err(Error::Expired);
        }
        let base = self
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_PROXY)
            .trim_end_matches('/');
        let headers = [
            ("Authorization", format!("Bearer {}", login.key)),
            ("x-xai-token-auth", "xai-grok-cli".to_string()),
            ("Accept", "application/json".to_string()),
        ];
        let url = format!("{base}/v1/billing?format=credits");
        let mut snap = parse_billing(&http::get(&Get {
            url: &url,
            headers: &headers,
            timeout: left().max(Duration::from_millis(1)),
        })?)?;
        // The settings call gets only the time left. A full second budget here used to
        // outlive the cache deadline and drop the billing body that had already arrived.
        if snap.plan.is_empty() {
            let remain = left();
            if remain >= Duration::from_millis(1)
                && let Ok(body) = http::get(&Get {
                    url: &format!("{base}/v1/settings"),
                    headers: &headers,
                    timeout: remain,
                })
                && let Ok(v) = pt_json::parse_bytes(&body)
            {
                snap.plan = v
                    .get("subscription_tier_display")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
        }
        Ok(snap)
    }
}
