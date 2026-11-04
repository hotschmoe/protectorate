//! Codex subscription usage from the Codex CLI's login (`~/.codex/auth.json`) and the
//! chatgpt.com wham usage endpoint.

use std::path::PathBuf;
use std::time::Duration;

use pt_api::Subscription;
use pt_json::Value;

use crate::http::{self, Get};
use crate::login::{home_dir, read_login};
use crate::reader::{Reader, Snapshot, window, window_label};
use crate::time::secs_to_ms;
use crate::{Error, Result};

const DEFAULT_BASE: &str = "https://chatgpt.com/backend-api";

#[derive(Clone, Debug, Default)]
pub struct Codex {
    pub home: Option<PathBuf>,
    pub base_url: Option<String>,
}

pub struct Login {
    token: String,
    account_id: String,
    renewable: bool,
}

pub fn parse_login(raw: &[u8]) -> Result<Login> {
    let v = pt_json::parse_bytes(raw).map_err(|e| Error::other(format!("decode login: {e}")))?;
    let text = |k: &str| {
        v.pointer(&format!("/tokens/{k}"))
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let token = text("access_token");
    if token.is_empty() {
        return Err(Error::unavailable("not logged in (no OAuth access token)"));
    }
    Ok(Login {
        token: token.to_string(),
        account_id: text("account_id").to_string(),
        renewable: !text("refresh_token").is_empty(),
    })
}

/// `exp` of a JWT access token in epoch ms, if it is a JWT with one.
pub fn jwt_expiry_ms(token: &str) -> Option<u64> {
    let mut parts = token.split('.');
    let payload = parts.nth(1)?;
    parts.next()?;
    let claims = pt_json::parse_bytes(&b64url_decode(payload.trim_end_matches('='))?).ok()?;
    let exp = claims.get("exp")?.as_i64()?;
    if exp == 0 { None } else { secs_to_ms(exp) }
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let idx = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(idx);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The plan and windows of a `/wham/usage` response.
pub fn parse_usage(body: &[u8]) -> Result<Snapshot> {
    let v = pt_json::parse_bytes(body).map_err(|e| Error::other(format!("decode usage: {e}")))?;
    if v.as_object().is_none() {
        return Err(Error::other("usage response is not an object"));
    }
    let mut snap = Snapshot {
        plan: v
            .get("plan_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ..Snapshot::default()
    };
    for key in ["primary_window", "secondary_window"] {
        let Some(w) = v
            .pointer(&format!("/rate_limit/{key}"))
            .filter(|w| !w.is_null())
        else {
            continue;
        };
        let pct = w
            .get("used_percent")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or_else(|| Error::other(format!("usage response has invalid {key} usage")))?;
        let secs = w
            .get("limit_window_seconds")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::other(format!("usage response has invalid {key} length")))?;
        // `0` is not a reset time. Stored as epoch ms it is always in the past, and the
        // percent would then be dropped.
        let reset = w
            .get("reset_at")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0)
            .and_then(secs_to_ms);
        // The label keeps whole minutes (`119` s is `1m`). The pace uses this length.
        let mut w = window(window_label(secs / 60), pct, reset);
        w.length_ms = u64::try_from(secs).ok().and_then(|s| s.checked_mul(1_000));
        snap.windows.push(w);
    }
    if snap.windows.is_empty() {
        return Err(Error::other("usage response has no rate limit windows"));
    }
    Ok(snap)
}

impl Reader for Codex {
    fn subscription(&self) -> Subscription {
        Subscription::Codex
    }

    fn read(&self, now_ms: u64, timeout: Duration) -> Result<Snapshot> {
        let home = home_dir(self.home.as_deref())?;
        let login = parse_login(&read_login(&home.join(".codex").join("auth.json"))?)?;
        if jwt_expiry_ms(&login.token).is_some_and(|e| now_ms >= e) {
            return Err(Error::Expired {
                renewable: login.renewable,
            });
        }
        let base = self
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE)
            .trim_end_matches('/');
        let url = format!("{base}/wham/usage");
        let mut headers = vec![
            ("Authorization", format!("Bearer {}", login.token)),
            ("Accept", "application/json".to_string()),
            ("User-Agent", "protectorate".to_string()),
        ];
        if !login.account_id.is_empty() {
            headers.push(("ChatGPT-Account-Id", login.account_id.clone()));
        }
        parse_usage(&http::get(&Get {
            url: &url,
            headers: &headers,
            timeout,
        })?)
    }
}
