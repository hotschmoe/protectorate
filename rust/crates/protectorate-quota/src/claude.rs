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
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }
    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ctx, time, Fake};

    fn home(dir: &std::path::Path, creds: &str) {
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(dir.join(".claude").join(".credentials.json"), creds).unwrap();
    }

    const USAGE: &str = r#"{
      "five_hour": {"utilization": 42.5, "resets_at": "2026-01-01T04:00:00Z"},
      "seven_day": {"utilization": 10},
      "seven_day_sonnet": null,
      "seven_day_opus": {"utilization": -3, "resets_at": "2026-01-05T00:00:00Z"},
      "limits": [
        {"kind": "weekly_scoped", "percent": 55, "resets_at": "2026-01-04T00:00:00Z",
         "scope": {"model": {"display_name": "Fable"}}},
        {"kind": "weekly_scoped", "percent": 61, "resets_at": "2026-01-04T00:00:00Z",
         "scope": {"model": {"display_name": "opus"}}},
        {"kind": "other", "percent": 99, "scope": {"model": {"display_name": "Ignored"}}}
      ],
      "extra_usage": {"is_enabled": true, "utilization": 7},
      "cedar_ember": {"eligible": true, "grants": [
        {"resets_left": 2, "starts_at": "2025-12-31T00:00:00Z", "ends_at": "2026-01-03T00:00:00Z"},
        {"resets_left": 0, "ends_at": "2026-01-03T00:00:00Z"},
        {"resets_left": 1, "starts_at": "2026-02-01T00:00:00Z", "ends_at": "2026-01-03T00:00:00Z"},
        {"resets_left": 1, "ends_at": "2025-12-01T00:00:00Z"},
        {"resets_left": 1, "paused": true, "ends_at": "2026-01-03T00:00:00Z"}
      ]}
    }"#;

    fn creds_json(exp_millis: i64) -> String {
        format!(
            r#"{{"claudeAiOauth": {{"accessToken": "tok", "expiresAt": {exp_millis}, "subscriptionType": "max"}}}}"#
        )
    }

    #[test]
    fn windows_resets_and_plan() {
        let dir = tempfile::tempdir().unwrap();
        home(
            dir.path(),
            &creds_json(time("2026-01-01T05:00:00Z").timestamp_millis()),
        );
        let http = Fake::new().route(
            "GET",
            "https://api.anthropic.com/api/oauth/usage?cedar_ember=1",
            200,
            USAGE,
        );
        let snap = Claude::new()
            .with_home(dir.path().to_path_buf())
            .read(&ctx("2026-01-01T00:00:00Z"), &http)
            .unwrap();

        assert_eq!(snap.provider, "claude");
        assert_eq!(snap.source, "oauth-usage");
        assert_eq!(snap.plan, "max");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["5h", "7d", "7d opus", "7d Fable", "extra"]);
        assert_eq!(snap.windows[0].used_percent, 42.5);
        assert_eq!(
            snap.windows[0].resets_at,
            Some(time("2026-01-01T04:00:00Z"))
        );
        assert_eq!(snap.windows[1].resets_at, None);
        assert_eq!(
            snap.windows[2].used_percent, 0.0,
            "negative utilization clamps"
        );
        assert_eq!(snap.windows[4].name, "extra");
        assert_eq!(snap.resets.len(), 1, "only live, unpaused grants count");
        assert_eq!(snap.resets[0].kind, "usage-limit");
        assert_eq!(snap.resets[0].count, 2);
        assert_eq!(
            snap.resets[0].expires_at,
            Some(time("2026-01-03T00:00:00Z"))
        );

        let seen = http.requests();
        assert_eq!(seen.len(), 1);
        assert!(seen[0]
            .2
            .contains(&("anthropic-beta".to_string(), "oauth-2025-04-20".to_string())));
        assert!(seen[0]
            .2
            .contains(&("Authorization".to_string(), "Bearer tok".to_string())));
    }

    #[test]
    fn scoped_windows_are_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        home(
            dir.path(),
            &creds_json(time("2026-01-01T05:00:00Z").timestamp_millis()),
        );
        let http = Fake::new().route(
            "GET",
            "https://api.anthropic.com/api/oauth/usage?cedar_ember=1",
            200,
            r#"{"seven_day_opus": {"utilization": 1},
                "limits": [{"kind": "weekly_scoped", "percent": 2,
                 "scope": {"model": {"display_name": "opus"}}}]}"#,
        );
        let snap = Claude::new()
            .with_home(dir.path().to_path_buf())
            .read(&ctx("2026-01-01T00:00:00Z"), &http)
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].used_percent, 1.0);
    }

    #[test]
    fn expired_credentials_are_reported_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        home(
            dir.path(),
            &creds_json(time("2025-12-31T23:00:00Z").timestamp_millis()),
        );
        let http = Fake::new();
        let err = Claude::new()
            .with_home(dir.path().to_path_buf())
            .read(&ctx("2026-01-01T00:00:00Z"), &http)
            .unwrap_err();
        assert_eq!(err, Error::Expired);
        assert!(http.requests().is_empty());
    }

    #[test]
    fn missing_credentials_and_token_are_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let http = Fake::new();
        let c = Claude::new().with_home(dir.path().to_path_buf());
        let err = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap_err();
        assert!(matches!(err, Error::Unavailable(_)));
        assert_eq!(
            err.to_string(),
            "source unavailable: credentials file not found"
        );

        home(dir.path(), r#"{"claudeAiOauth": {"accessToken": ""}}"#);
        let err = c.read(&ctx("2026-01-01T00:00:00Z"), &http).unwrap_err();
        assert_eq!(err.to_string(), "source unavailable: no OAuth access token");
    }

    #[test]
    fn http_status_maps_to_credential_errors() {
        let dir = tempfile::tempdir().unwrap();
        home(
            dir.path(),
            &creds_json(time("2026-01-01T05:00:00Z").timestamp_millis()),
        );
        let url = "https://api.anthropic.com/api/oauth/usage?cedar_ember=1";
        let c = Claude::new().with_home(dir.path().to_path_buf());
        let now = ctx("2026-01-01T00:00:00Z");

        let http = Fake::new().route("GET", url, 401, "{}");
        assert_eq!(c.read(&now, &http).unwrap_err(), Error::Unauthorized);

        let http = Fake::new().route("GET", url, 403, r#"{"error":"missing scope"}}"#);
        assert_eq!(c.read(&now, &http).unwrap_err(), Error::Scope);

        let http = Fake::new().route("GET", url, 403, "nope");
        assert_eq!(c.read(&now, &http).unwrap_err(), Error::Http(403));

        let http = Fake::new().route("GET", url, 503, "nope");
        assert_eq!(c.read(&now, &http).unwrap_err(), Error::Http(503));
    }

    #[test]
    fn base_url_override_is_used() {
        let dir = tempfile::tempdir().unwrap();
        home(
            dir.path(),
            &creds_json(time("2026-01-01T05:00:00Z").timestamp_millis()),
        );
        let http = Fake::new().route(
            "GET",
            "https://proxy.invalid/base/api/oauth/usage?cedar_ember=1",
            200,
            r#"{"five_hour": {"utilization": 1}}"#,
        );
        let snap = Claude::new()
            .with_home(dir.path().to_path_buf())
            .with_base_url("https://proxy.invalid/base/")
            .read(&ctx("2026-01-01T00:00:00Z"), &http)
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
    }
}
