use crate::fields;
use protectorate_quota::{
    rank_harnesses, Antigravity, Claude, Codex, Ctx, Grok, Local, QuotaCache, QuotaOptions, Reader,
    Request, Response, Result, TaskSize, Transport,
};
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

const BASE: &str = "http://quota.invalid";

/// Routes each provider endpoint to its own NUL-separated input field, so the
/// concurrent readers see the same bodies on every run. A field starting with
/// `@NNN ` answers with that status.
struct Bodies(Vec<Vec<u8>>);

impl Transport for Bodies {
    fn send(&self, req: &Request) -> Result<Response> {
        let path = req.url.strip_prefix(BASE).unwrap_or(&req.url);
        let route = [
            "/api/oauth/usage",
            "/wham/usage",
            "/wham/rate-limit-reset-credits",
            "/v1/billing",
            "/v1/settings",
            "/prod_mc_billing",
            "/v1/models",
        ]
        .iter()
        .position(|p| path.starts_with(p))
        .unwrap_or(0);
        let body = &self.0[route];
        let (status, body) = match body.strip_prefix(b"@") {
            Some(rest) if rest.len() >= 4 && rest[3] == b' ' => (
                std::str::from_utf8(&rest[..3])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(200),
                rest[4..].to_vec(),
            ),
            _ => (200, body.clone()),
        };
        Ok(Response { status, body })
    }
}

/// Provider responses (Claude, Codex, Grok, local OpenAI-compatible and the
/// agy CLI report) through the hub's quota cache and harness ranking.
pub fn quota_responses(data: &[u8]) {
    let parts = fields(data, 8);
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    for (path, body) in [
        (
            ".claude/.credentials.json",
            &br#"{"claudeAiOauth":{"accessToken":"t","subscriptionType":"max"}}"#[..],
        ),
        (
            ".codex/auth.json",
            br#"{"tokens":{"access_token":"t","account_id":"a"}}"#,
        ),
        (
            ".grok/auth.json",
            br#"{"https://auth.x.ai::fuzz":{"key":"k","expires_at":""}}"#,
        ),
    ] {
        fs::create_dir_all(h.join(path).parent().unwrap()).unwrap();
        fs::write(h.join(path), body).unwrap();
    }
    let report = h.join("agy-report");
    fs::write(&report, parts[7]).unwrap();
    let agy = h.join("agy");
    fs::write(
        &agy,
        format!("#!/bin/sh\nexec cat '{}'\n", report.display()),
    )
    .unwrap();
    fs::set_permissions(&agy, fs::Permissions::from_mode(0o755)).unwrap();
    let readers: Vec<Arc<dyn Reader>> = vec![
        Arc::new(Claude::new().with_home(h.into()).with_base_url(BASE)),
        Arc::new(
            Codex::new()
                .with_home(h.into())
                .with_bin("/nonexistent/codex")
                .with_base_url(BASE),
        ),
        Arc::new(
            Grok::new()
                .with_home(h.into())
                .with_base_url(BASE)
                .with_web_url(BASE),
        ),
        Arc::new(Local::new(BASE)),
        Arc::new(Antigravity::new().with_bin(&agy.to_string_lossy())),
    ];
    let transport = Bodies(parts[..7].iter().map(|p| p.to_vec()).collect());
    let cache = QuotaCache::new(readers, QuotaOptions::default());
    let ctx = Ctx::now();
    cache.refresh(&ctx, &transport, true);
    let quotas = cache.snapshot(ctx.now);
    for quota in &quotas {
        for window in &quota.windows {
            assert!(
                window.used_percent.is_finite() && window.used_percent >= 0.0,
                "{} window {:?} used {}",
                quota.provider,
                window.name,
                window.used_percent
            );
        }
        serde_json::to_string(quota).expect("quota serializes");
    }
    for size in [TaskSize::Small, TaskSize::Medium, TaskSize::Large] {
        let ranked = rank_harnesses(&quotas, size, ctx.now);
        for (i, s) in ranked.iter().enumerate() {
            assert_eq!(s.rank, i as i32 + 1);
            assert!(!s.score.is_nan(), "{} scored NaN", s.provider);
        }
    }
}
