use chrono::{DateTime, Utc};
use protectorate_quota::{
    rank_harnesses, window_length, Ctx, Error, Freshness, Harness, HarnessSuggestion,
    ProviderQuota, QuotaCache, QuotaOptions, QuotaWindow, Reader, Reset, ResetCredit, Snapshot,
    TaskSize, Transport, Window,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct NullTransport;
impl Transport for NullTransport {
    fn send(
        &self,
        _req: &protectorate_quota::Request,
    ) -> Result<protectorate_quota::Response, Error> {
        Err(Error::Other("no network in this test".to_string()))
    }
}

fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

struct CountingReader {
    name: &'static str,
    reads: Arc<AtomicUsize>,
    good: bool,
}

impl CountingReader {
    fn new(name: &'static str, good: bool) -> Self {
        Self {
            name,
            reads: Arc::new(AtomicUsize::new(0)),
            good,
        }
    }
}

impl Reader for CountingReader {
    fn provider(&self) -> &str {
        self.name
    }
    fn read(&self, ctx: &Ctx, _http: &dyn Transport) -> Result<Snapshot, Error> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if !self.good {
            return Err(Error::unavailable(format!(
                "{} is not installed",
                self.name
            )));
        }
        Ok(Snapshot {
            provider: self.name.to_string(),
            plan: "pro".to_string(),
            source: "test".to_string(),
            windows: vec![Window {
                name: "5h".to_string(),
                used_percent: 20.0,
                resets_at: Some(ctx.now + chrono::Duration::hours(2)),
            }],
            fetched_at: Some(ctx.now),
            ..Snapshot::default()
        })
    }
}

fn opts() -> QuotaOptions {
    QuotaOptions {
        interval: Duration::from_millis(20),
        stale_after: Duration::from_secs(60),
        min_refresh: Duration::from_secs(30),
        read_timeout: Duration::from_secs(5),
    }
}

#[test]
fn cache_reports_fresh_then_stale() {
    let reader = Arc::new(CountingReader::new("claude", true));
    let cache = QuotaCache::new(vec![reader], opts());
    let http = NullTransport;

    cache.refresh(&Ctx::at(time("2026-01-01T00:00:00Z")), &http, true);
    let q = cache.snapshot(time("2026-01-01T00:00:10Z"));
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].provider, "claude");
    assert_eq!(q[0].freshness, Freshness::Fresh);
    assert_eq!(q[0].plan, "pro");
    assert_eq!(q[0].age, Some(Duration::from_secs(10)));
    assert_eq!(q[0].windows[0].name, "5h");
    assert_eq!(q[0].windows[0].window_seconds, 5 * 3600);
    assert_eq!(
        q[0].windows[0].resets_at,
        Some(time("2026-01-01T02:00:00Z"))
    );

    let q = cache.snapshot(time("2026-01-01T00:02:00Z"));
    assert_eq!(q[0].freshness, Freshness::Stale);
    assert_eq!(q[0].age, Some(Duration::from_secs(120)));

    let q = cache.snapshot(time("2026-01-01T00:01:00.001Z"));
    assert_eq!(q[0].freshness, Freshness::Stale);
    assert_eq!(q[0].age, Some(Duration::from_millis(60_001)));
}

#[test]
fn forced_refreshes_collapse_within_the_min_refresh() {
    let reader = Arc::new(CountingReader::new("codex", true));
    let reads = reader.reads.clone();
    let cache = QuotaCache::new(vec![reader], opts());
    let http = NullTransport;

    let first = Ctx::at(time("2026-01-01T00:00:00Z"));
    cache.refresh(&first, &http, true);
    cache.refresh(&Ctx::at(time("2026-01-01T00:00:05Z")), &http, true);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "read too recently: keep the cache"
    );

    cache.refresh(&Ctx::at(time("2026-01-01T00:00:40Z")), &http, true);
    assert_eq!(reads.load(Ordering::SeqCst), 2);

    cache.refresh(&Ctx::at(time("2026-01-01T00:00:41Z")), &http, false);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        3,
        "polls are never rate limited"
    );
}

#[test]
fn a_failed_read_keeps_the_last_good_data_and_shows_the_error() {
    let reader = Arc::new(CountingReader::new("grok", false));
    let cache = QuotaCache::new(vec![reader], opts());
    cache.refresh(&Ctx::at(time("2026-01-01T00:00:00Z")), &NullTransport, true);
    let q = cache.snapshot(time("2026-01-01T00:00:01Z"));
    assert_eq!(q[0].freshness, Freshness::Unknown);
    assert_eq!(q[0].fetched_at, None);
    assert_eq!(q[0].checked_at, Some(time("2026-01-01T00:00:00Z")));
    assert_eq!(q[0].error, "source unavailable: grok is not installed");
}

#[test]
fn updates_are_announced_once_per_read() {
    let cache = QuotaCache::new(vec![Arc::new(CountingReader::new("claude", true))], opts());
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = seen.clone();
    cache.set_on_update(move |q: &ProviderQuota| {
        assert_eq!(q.freshness, Freshness::Fresh);
        counter.fetch_add(1, Ordering::SeqCst);
    });
    cache.refresh(&Ctx::at(time("2026-01-01T00:00:00Z")), &NullTransport, true);
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

fn window(
    name: &str,
    used: f64,
    resets_at: Option<DateTime<Utc>>,
    est: Option<f64>,
) -> QuotaWindow {
    QuotaWindow {
        name: name.to_string(),
        used_percent: used,
        resets_at,
        window_seconds: window_length(name).as_secs() as i64,
        estimated_unused_percent_at_reset: est,
    }
}

fn quota(provider: &str, freshness: Freshness, windows: Vec<QuotaWindow>) -> ProviderQuota {
    ProviderQuota {
        provider: provider.to_string(),
        plan: "pro".to_string(),
        source: "test".to_string(),
        freshness,
        fetched_at: Some(time("2026-01-01T00:00:00Z")),
        age: Some(Duration::ZERO),
        error: String::new(),
        checked_at: Some(time("2026-01-01T00:00:00Z")),
        windows,
        resets: vec![ResetCredit {
            kind: "usage-limit".to_string(),
            count: 1,
            expires_at: Some(time("2026-01-02T00:00:00Z")),
        }],
        models: Vec::new(),
        latency: None,
    }
}

fn ranks(out: &[HarnessSuggestion]) -> Vec<&str> {
    out.iter().map(|s| s.harness.name()).collect()
}

#[test]
fn spare_capacity_that_expires_soonest_wins() {
    let now = time("2026-01-01T00:00:00Z");
    let quotas = vec![
        quota(
            "claude",
            Freshness::Fresh,
            vec![
                window(
                    "5h",
                    40.0,
                    Some(now + chrono::Duration::hours(1)),
                    Some(60.0),
                ),
                window(
                    "7d",
                    30.0,
                    Some(now + chrono::Duration::days(3)),
                    Some(40.0),
                ),
            ],
        ),
        quota(
            "codex",
            Freshness::Fresh,
            vec![window(
                "5h",
                97.0,
                Some(now + chrono::Duration::hours(1)),
                None,
            )],
        ),
        quota(
            "grok",
            Freshness::Stale,
            vec![window("credits", 5.0, None, None)],
        ),
    ];
    let out = rank_harnesses(&quotas, TaskSize::Medium, now);

    assert_eq!(ranks(&out), ["claude", "pi", "codex", "grok"]);
    assert_eq!(out[0].rank, 1);
    assert!(out[0].recommended);
    assert_eq!(out[0].window, "5h");
    assert_eq!(out[0].usable_percent, Some(60.0));
    assert_eq!(out[0].expiring_percent, Some(60.0));
    assert_eq!(out[0].resets_in, Some(3600));
    // 60% expiring at a reset one hour away, weighted by 48h urgency.
    assert!(
        (out[0].score - 58.75).abs() < 0.05,
        "score {}",
        out[0].score
    );
    assert_eq!(
        out[0].reason,
        "5h window resets in 1h00m, ~60% expiring unused (projected); 60% usable now (5h)"
    );

    assert_eq!(out[1].harness, Harness::Pi);
    assert!(out[1].recommended);
    assert_eq!(out[1].score, 1.0);
    assert_eq!(
        out[1].reason,
        "local LLM via pi: no quota, always available fallback (no local endpoint monitored)"
    );

    assert!(!out[2].recommended);
    assert!(out[2]
        .reason
        .starts_with("only 3% usable now (5h window), below the 15% a medium task needs"));
    assert!(!out[3].recommended);
    assert_eq!(
        out[3].reason,
        "quota data is stale (0m old); not counted as spare"
    );
}

#[test]
fn a_task_size_raises_the_bar_and_unknown_data_explains_itself() {
    let now = time("2026-01-01T00:00:00Z");
    let mut claude_q = quota(
        "claude",
        Freshness::Fresh,
        vec![window(
            "5h",
            80.0,
            Some(now + chrono::Duration::hours(2)),
            None,
        )],
    );
    claude_q.resets.clear();
    let mut codex_q = quota("codex", Freshness::Unknown, Vec::new());
    codex_q.error = "source unavailable: codex is not installed".to_string();
    let mut grok_q = quota("grok", Freshness::Fresh, Vec::new());
    grok_q.resets.clear();
    let quotas = vec![claude_q, codex_q, grok_q];

    let out = rank_harnesses(&quotas, TaskSize::Large, now);
    let claude = out.iter().find(|s| s.harness == Harness::Claude).unwrap();
    assert!(!claude.recommended);
    assert!(claude
        .reason
        .starts_with("only 20% usable now (5h window), below the 30% a large task needs"));

    let out_small = rank_harnesses(&quotas, TaskSize::Small, now);
    let claude = out_small
        .iter()
        .find(|s| s.harness == Harness::Claude)
        .unwrap();
    assert!(claude.recommended);
    assert_eq!(claude.window, "5h");
    assert_eq!(claude.expiring_percent, Some(20.0));
    assert!(claude.reason.contains("remaining, no pace estimate"));

    let codex = out.iter().find(|s| s.harness == Harness::Codex).unwrap();
    assert_eq!(codex.freshness, Freshness::Unknown);
    assert_eq!(
        codex.reason,
        "no quota data yet; not counted as spare (source unavailable: codex is not installed)"
    );

    let grok = out.iter().find(|s| s.harness == Harness::Grok).unwrap();
    assert!(!grok.recommended);
    assert_eq!(
        grok.reason,
        "no usable windows reported (none, or all already reset); wait for the next quota read"
    );
}

#[test]
fn local_is_ranked_below_any_subscription_with_real_capacity() {
    let now = time("2026-01-01T00:00:00Z");
    let mut local = quota("local", Freshness::Fresh, Vec::new());
    local.models = vec!["pi".to_string(), "llama3".to_string()];
    let quotas = vec![
        quota(
            "claude",
            Freshness::Fresh,
            vec![window(
                "5h",
                10.0,
                Some(now + chrono::Duration::hours(4)),
                None,
            )],
        ),
        local,
    ];
    let out = rank_harnesses(&quotas, TaskSize::Medium, now);
    assert_eq!(ranks(&out), ["claude", "pi", "codex", "grok"]);
    let pi = out.iter().find(|s| s.harness == Harness::Pi).unwrap();
    assert_eq!(
        pi.reason,
        "local LLM via pi: no quota, reachable (pi, llama3)"
    );

    let mut local = quota("local", Freshness::Stale, Vec::new());
    local.age = Some(Duration::from_secs(3600));
    let out = rank_harnesses(&[local], TaskSize::Medium, now);
    let pi = out.iter().find(|s| s.harness == Harness::Pi).unwrap();
    assert!(pi.recommended, "pi is always the fallback");
    assert!(pi.reason.contains("endpoint not confirmed reachable"));
}

#[test]
fn window_lengths_come_from_the_name() {
    assert_eq!(window_length("5h"), Duration::from_secs(5 * 3600));
    assert_eq!(
        window_length("secondary 7d"),
        Duration::from_secs(7 * 24 * 3600)
    );
    assert_eq!(
        window_length("credits weekly"),
        Duration::from_secs(7 * 24 * 3600)
    );
    assert_eq!(window_length("credits"), Duration::ZERO);
    assert_eq!(window_length("45m"), Duration::ZERO);
    assert_eq!(window_length(""), Duration::ZERO);
}

#[test]
fn snapshots_serialize_without_empty_noise() {
    let s = Snapshot {
        provider: "claude".to_string(),
        windows: vec![Window {
            name: "5h".to_string(),
            used_percent: 1.0,
            resets_at: None,
        }],
        resets: vec![Reset {
            kind: "usage-limit".to_string(),
            count: 2,
            expires_at: Some(time("2026-01-02T00:00:00Z")),
        }],
        fetched_at: Some(time("2026-01-01T00:00:00Z")),
        ..Snapshot::default()
    };
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(
        json,
        r#"{"provider":"claude","windows":[{"name":"5h","used_percent":1.0}],"resets":[{"kind":"usage-limit","count":2,"expires_at":"2026-01-02T00:00:00Z"}],"fetched_at":"2026-01-01T00:00:00Z"}"#
    );
    let back: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}
