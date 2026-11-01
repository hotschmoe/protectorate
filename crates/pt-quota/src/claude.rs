//! Claude subscription usage from the Claude CLI's own OAuth login (`~/.claude/.credentials.json`).

use std::path::PathBuf;
use std::time::Duration;

use pt_api::Subscription;
use pt_json::Value;

use crate::http::{self, Get};
use crate::{Error, Reader, Result, Snapshot, window};
use crate::{home_dir, parse_rfc3339, read_login};

const DEFAULT_BASE: &str = "https://api.anthropic.com";

#[derive(Clone, Debug, Default)]
pub struct Claude {
    pub home: Option<PathBuf>,
    pub base_url: Option<String>,
}

pub struct Login {
    token: String,
    pub expires_at_ms: Option<u64>,
    pub plan: String,
}

pub fn parse_login(raw: &[u8]) -> Result<Login> {
    let v = pt_json::parse_bytes(raw).map_err(|e| Error::other(format!("decode login: {e}")))?;
    let oauth = v.get("claudeAiOauth");
    let text = |k: &str| {
        oauth
            .and_then(|o| o.get(k))
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let token = text("accessToken");
    if token.is_empty() {
        return Err(Error::unavailable("not logged in (no OAuth access token)"));
    }
    Ok(Login {
        token: token.to_string(),
        expires_at_ms: oauth
            .and_then(|o| o.get("expiresAt"))
            .and_then(Value::as_u64)
            .filter(|&t| t > 0),
        plan: text("subscriptionType").to_string(),
    })
}

/// The windows of a `/api/oauth/usage` response.
pub fn parse_usage(body: &[u8]) -> Result<Snapshot> {
    let v = pt_json::parse_bytes(body).map_err(|e| Error::other(format!("decode usage: {e}")))?;
    if v.as_object().is_none() {
        return Err(Error::other("usage response is not an object"));
    }
    let mut snap = Snapshot::default();
    let mut add = |label: String, w: Option<&Value>, pct_key: &str| {
        let Some(w) = w else { return };
        let Some(pct) = w.get(pct_key).and_then(Value::as_f64) else {
            return;
        };
        if !pct.is_finite() || snap.windows.iter().any(|x| x.label == label) {
            return;
        }
        let reset = w
            .get("resets_at")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339);
        snap.windows.push(window(label, pct, reset));
    };
    add("5h".into(), v.get("five_hour"), "utilization");
    add("wk".into(), v.get("seven_day"), "utilization");
    add("wk sonnet".into(), v.get("seven_day_sonnet"), "utilization");
    add("wk opus".into(), v.get("seven_day_opus"), "utilization");
    if let Some(limits) = v.get("limits").and_then(Value::as_array) {
        for l in limits {
            if l.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
                continue;
            }
            let name = l
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !name.is_empty() {
                add(format!("wk {}", name.to_lowercase()), Some(l), "percent");
            }
        }
    }
    Ok(snap)
}

impl Reader for Claude {
    fn subscription(&self) -> Subscription {
        Subscription::Claude
    }

    fn read(&self, now_ms: u64, timeout: Duration) -> Result<Snapshot> {
        let home = home_dir(self.home.as_deref())?;
        let login = parse_login(&read_login(
            &home.join(".claude").join(".credentials.json"),
        )?)?;
        if login.expires_at_ms.is_some_and(|e| now_ms >= e) {
            return Err(Error::Expired);
        }
        let base = self
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE)
            .trim_end_matches('/');
        let url = format!("{base}/api/oauth/usage");
        let headers = [
            ("Authorization", format!("Bearer {}", login.token)),
            ("anthropic-beta", "oauth-2025-04-20".to_string()),
            ("Accept", "application/json".to_string()),
        ];
        let body = http::get(&Get {
            url: &url,
            headers: &headers,
            timeout,
        })?;
        let mut snap = parse_usage(&body)?;
        snap.plan = login.plan;
        Ok(snap)
    }
}
