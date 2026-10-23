//! Claude subscription usage from the Claude CLI's own OAuth credentials.
use crate::http::get_json;
use crate::{
    clamp_percent, home_dir, parse_time, read_cred_file, Ctx, Error, Reader, Reset, Result,
    Snapshot, Transport, Window,
};
use chrono::DateTime;
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_BASE: &str = "https://api.anthropic.com";

#[derive(Clone, Debug)]
pub struct Claude {
    pub home: Option<PathBuf>,
    pub base_url: String,
}

impl Default for Claude {
    fn default() -> Self {
        Self {
            home: None,
            base_url: DEFAULT_BASE.to_string(),
        }
    }
}

impl Claude {
    pub fn new() -> Self {
        Self::default()
    }
    fn base(&self) -> String {
        if self.base_url.is_empty() {
            DEFAULT_BASE.to_string()
        } else {
            self.base_url.clone()
        }
    }
}

impl Reader for Claude {
    fn provider(&self) -> &str {
        "claude"
    }

    fn read(&self, ctx: &Ctx, http: &dyn Transport) -> Result<Snapshot> {
        let mut snap = Snapshot {
            provider: "claude".to_string(),
            source: "oauth-usage".to_string(),
            ..Snapshot::default()
        };
        let home = home_dir(self.home.as_deref())?;
        let raw = read_cred_file(&home.join(".claude").join(".credentials.json"))?;
        let creds: ClaudeCreds = serde_json::from_slice(&raw)
            .map_err(|e| Error::Other(format!("decode credentials: {e}")))?;
        if creds.oauth.access_token.is_empty() {
            return Err(Error::unavailable("no OAuth access token"));
        }
        if creds.oauth.expires_at > 0 {
            let exp = DateTime::from_timestamp_millis(creds.oauth.expires_at);
            if exp.map(|e| ctx.now >= e).unwrap_or(true) {
                return Err(Error::Expired);
            }
        }
        snap.plan = creds.oauth.subscription_type;

        let url = self.base() + "/api/oauth/usage?cedar_ember=1";
        let headers = vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", creds.oauth.access_token),
            ),
            ("anthropic-beta".to_string(), "oauth-2025-04-20".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        let u: ClaudeUsage = get_json(http, ctx, &url, headers)?;
        snap.fetched_at = Some(ctx.now);

        let mut seen: Vec<String> = Vec::new();
        let mut add = |name: &str, w: Option<&ClaudeWindow>, snap: &mut Snapshot| {
            let Some(w) = w else { return };
            let Some(util) = w.utilization else { return };
            if seen.iter().any(|s| s == name) {
                return;
            }
            seen.push(name.to_string());
            snap.windows.push(Window {
                name: name.to_string(),
                used_percent: clamp_percent(util),
                resets_at: parse_time(&w.resets_at),
            });
        };
        add("5h", u.five_hour.as_ref(), &mut snap);
        add("7d", u.seven_day.as_ref(), &mut snap);
        add("7d sonnet", u.seven_day_sonnet.as_ref(), &mut snap);
        add("7d opus", u.seven_day_opus.as_ref(), &mut snap);
        for l in &u.limits {
            if l.kind != "weekly_scoped" {
                continue;
            }
            let Some(display) = l
                .scope
                .as_ref()
                .and_then(|s| s.model.as_ref())
                .map(|m| m.display_name.clone())
            else {
                continue;
            };
            if display.is_empty() {
                continue;
            }
            let w = ClaudeWindow {
                utilization: l.percent,
                resets_at: l.resets_at.clone(),
            };
            add(&format!("7d {display}"), Some(&w), &mut snap);
        }
        if let Some(x) = u.extra_usage.as_ref() {
            if x.is_enabled {
                let w = ClaudeWindow {
                    utilization: x.utilization,
                    resets_at: String::new(),
                };
                add("extra", Some(&w), &mut snap);
            }
        }
        if let Some(ce) = u.cedar_ember.as_ref() {
            if ce.eligible {
                for g in &ce.grants {
                    if g.paused || g.resets_left <= 0 {
                        continue;
                    }
                    if let Some(s) = parse_time(&g.starts_at) {
                        if s > ctx.now {
                            continue;
                        }
                    }
                    let end = parse_time(&g.ends_at);
                    if let Some(e) = end {
                        if e <= ctx.now {
                            continue;
                        }
                    }
                    snap.resets.push(Reset {
                        kind: "usage-limit".to_string(),
                        count: g.resets_left,
                        expires_at: end,
                    });
                }
            }
        }
        Ok(snap)
    }
}

#[derive(Deserialize, Default)]
struct ClaudeCreds {
    #[serde(rename = "claudeAiOauth", default)]
    oauth: ClaudeOauth,
}

#[derive(Deserialize, Default)]
struct ClaudeOauth {
    #[serde(rename = "accessToken", default)]
    access_token: String,
    #[serde(rename = "expiresAt", default)]
    expires_at: i64,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: String,
}

#[derive(Deserialize, Default, Clone)]
struct ClaudeWindow {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    resets_at: String,
}

#[derive(Deserialize, Default)]
struct ClaudeUsage {
    #[serde(rename = "five_hour", default)]
    five_hour: Option<ClaudeWindow>,
    #[serde(rename = "seven_day", default)]
    seven_day: Option<ClaudeWindow>,
    #[serde(rename = "seven_day_sonnet", default)]
    seven_day_sonnet: Option<ClaudeWindow>,
    #[serde(rename = "seven_day_opus", default)]
    seven_day_opus: Option<ClaudeWindow>,
    #[serde(rename = "extra_usage", default)]
    extra_usage: Option<ClaudeExtra>,
    #[serde(default)]
    limits: Vec<ClaudeLimit>,
    #[serde(rename = "cedar_ember", default)]
    cedar_ember: Option<ClaudeCedar>,
}

#[derive(Deserialize, Default)]
struct ClaudeExtra {
    #[serde(rename = "is_enabled", default)]
    is_enabled: bool,
    #[serde(default)]
    utilization: Option<f64>,
}

#[derive(Deserialize, Default)]
struct ClaudeLimit {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    percent: Option<f64>,
    #[serde(default)]
    resets_at: String,
    #[serde(default)]
    scope: Option<ClaudeScope>,
}

#[derive(Deserialize, Default)]
struct ClaudeScope {
    #[serde(default)]
    model: Option<ClaudeModel>,
}

#[derive(Deserialize, Default)]
struct ClaudeModel {
    #[serde(rename = "display_name", default)]
    display_name: String,
}

#[derive(Deserialize, Default)]
struct ClaudeCedar {
    #[serde(default)]
    eligible: bool,
    #[serde(default)]
    grants: Vec<ClaudeGrant>,
}

#[derive(Deserialize, Default)]
struct ClaudeGrant {
    #[serde(rename = "resets_left", default)]
    resets_left: i32,
    #[serde(rename = "starts_at", default)]
    starts_at: String,
    #[serde(rename = "ends_at", default)]
    ends_at: String,
    #[serde(default)]
    paused: bool,
}
