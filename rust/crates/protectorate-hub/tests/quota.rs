use chrono::Duration as ChronoDuration;
use protectorate_hub::Hub;
use protectorate_quota::{
    Ctx, Error, QuotaCache, QuotaOptions, Reader, Request, Response, Snapshot, Transport, Window,
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct FakeReader {
    name: &'static str,
    reads: Arc<AtomicUsize>,
}

impl Reader for FakeReader {
    fn provider(&self) -> &str {
        self.name
    }

    fn read(&self, ctx: &Ctx, _http: &dyn Transport) -> protectorate_quota::Result<Snapshot> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(Snapshot {
            provider: self.name.into(),
            plan: if self.name == "claude" {
                "pro"
            } else {
                "local"
            }
            .into(),
            source: "fake".into(),
            fetched_at: Some(ctx.now),
            windows: if self.name == "claude" {
                vec![Window {
                    name: "5h".into(),
                    used_percent: 80.0,
                    resets_at: Some(ctx.now + ChronoDuration::minutes(30)),
                }]
            } else {
                Vec::new()
            },
            models: if self.name == "local" {
                vec!["tiny".into()]
            } else {
                Vec::new()
            },
            latency: Some(12),
            ..Snapshot::default()
        })
    }
}

struct FakeTransport;

impl Transport for FakeTransport {
    fn send(&self, _request: &Request) -> protectorate_quota::Result<Response> {
        Err(Error::unavailable("network disabled in test"))
    }
}

async fn call(addr: std::net::SocketAddr, path: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}

async fn read_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            return String::from_utf8(line).unwrap();
        }
    }
}

async fn read_event(stream: &mut TcpStream) -> Value {
    let line = read_line(stream).await;
    let size = usize::from_str_radix(line.trim(), 16).unwrap();
    let mut frame = vec![0; size];
    stream.read_exact(&mut frame).await.unwrap();
    assert_eq!(read_line(stream).await, "\r\n");
    assert_eq!(frame[0], 0);
    assert_eq!(
        u32::from_be_bytes(frame[1..5].try_into().unwrap()) as usize,
        size - 5
    );
    serde_json::from_slice(&frame[5..]).unwrap()
}

#[tokio::test]
async fn quota_http_refresh_rank_and_updated_event() {
    let dir = tempfile::tempdir().unwrap();
    let claude_reads = Arc::new(AtomicUsize::new(0));
    let local_reads = Arc::new(AtomicUsize::new(0));
    let readers: Vec<Arc<dyn Reader>> = vec![
        Arc::new(FakeReader {
            name: "claude",
            reads: claude_reads.clone(),
        }),
        Arc::new(FakeReader {
            name: "local",
            reads: local_reads.clone(),
        }),
    ];
    let cache = Arc::new(QuotaCache::new(readers, QuotaOptions::default()));
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_quota(cache, Arc::new(FakeTransport));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let get = "/protectorate.v1.QuotaService/GetQuota";
    let suggest = "/protectorate.v1.QuotaService/Suggest";
    let (code, initial) = call(addr, get, json!({})).await;
    assert_eq!(code, 200, "{initial}");
    assert_eq!(initial["quotas"][0]["freshness"], "FRESHNESS_UNKNOWN");
    for quota in initial["quotas"].as_array().unwrap() {
        let fields = quota.as_object().unwrap();
        for absent in ["plan", "source", "error", "windows", "resets", "models"] {
            assert!(!fields.contains_key(absent), "unexpected {absent}: {quota}");
        }
    }

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let frame = [0, 0, 0, 0, 2, b'{', b'}'];
    let headers = format!("POST /protectorate.v1.EventService/Subscribe HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/connect+json\r\nAuthorization: Bearer owner-secret\r\nContent-Length: {}\r\n\r\n", frame.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&frame).await.unwrap();
    assert!(read_line(&mut stream).await.contains("200"));
    loop {
        if read_line(&mut stream).await == "\r\n" {
            break;
        }
    }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), read_event(&mut stream))
            .await
            .unwrap()["kind"],
        "heartbeat"
    );

    let (code, refreshed) = call(addr, get, json!({"refresh":true})).await;
    assert_eq!(code, 200, "{refreshed}");
    assert_eq!(refreshed["quotas"][0]["provider"], "claude");
    assert_eq!(refreshed["quotas"][0]["freshness"], "FRESHNESS_FRESH");
    assert_eq!(
        refreshed["quotas"][0]["windows"][0]["windowSeconds"],
        "18000"
    );
    assert!(refreshed["quotas"][0]["windows"][0]["resetsAt"].is_string());
    assert_eq!(refreshed["quotas"][0]["latency"], "0.012s");
    assert!(refreshed["quotas"][0].get("error").is_none());
    assert!(refreshed["quotas"][0].get("resets").is_none());
    assert!(refreshed["quotas"][0].get("models").is_none());
    assert_eq!(refreshed["quotas"][1]["models"][0], "tiny");
    for _ in 0..2 {
        let event = tokio::time::timeout(Duration::from_secs(2), read_event(&mut stream))
            .await
            .unwrap();
        assert_eq!(event["kind"], "quota.updated");
        assert_eq!(event["payload"]["freshness"], "FRESHNESS_FRESH");
    }
    let (code, small) = call(addr, suggest, json!({"taskSize":"TASK_SIZE_SMALL"})).await;
    assert_eq!(code, 200, "{small}");
    assert_eq!(small["suggestions"][0]["harness"], "HARNESS_CLAUDE");
    assert_eq!(small["suggestions"][0]["usablePercent"], 20.0);
    assert_eq!(small["suggestions"][0]["freshness"], "FRESHNESS_FRESH");
    assert!(small["suggestions"][0]["resetsIn"].is_string());
    assert!(small["generatedAt"].is_string());

    let (code, large) = call(
        addr,
        suggest,
        json!({"taskSize":"TASK_SIZE_LARGE","refresh":true}),
    )
    .await;
    assert_eq!(code, 200, "{large}");
    assert_eq!(large["suggestions"][0]["harness"], "HARNESS_PI");
    let claude = large["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| suggestion["harness"] == "HARNESS_CLAUDE")
        .unwrap();
    for absent in ["recommended", "score"] {
        assert!(
            claude.get(absent).is_none(),
            "unexpected {absent}: {claude}"
        );
    }
    assert_eq!(claude_reads.load(Ordering::SeqCst), 1);
    assert_eq!(local_reads.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn default_quota_lists_four_providers_before_polling() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    let (code, body) = call(addr, "/protectorate.v1.QuotaService/GetQuota", json!({})).await;
    assert_eq!(code, 200, "{body}");
    let providers: Vec<_> = body["quotas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["provider"].as_str().unwrap())
        .collect();
    assert_eq!(providers, ["claude", "codex", "grok", "antigravity"]);
    server.abort();
}

#[test]
fn configured_local_model_readers_keep_the_hub_poll_interval() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "v-rust")
        .unwrap()
        .with_quota_sources(
            Duration::from_secs(90),
            &[
                "http://127.0.0.1:1234".into(),
                "http://127.0.0.1:5678/v1".into(),
            ],
        );
    assert_eq!(hub.quota.options().interval, Duration::from_secs(90));
    let providers: Vec<_> = hub
        .quota
        .snapshot(chrono::Utc::now())
        .into_iter()
        .map(|quota| quota.provider)
        .collect();
    assert_eq!(
        providers,
        ["claude", "codex", "grok", "antigravity", "local", "local"]
    );
}
