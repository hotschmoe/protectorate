//! Paging the audit log over HTTP: newest first across monthly files, with
//! actor, outcome and run filters, and cursors that cannot leave the log.
use crate::support::capability;
use chrono::{TimeZone, Utc};
use protectorate_audit::Entry;
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";
const CREATE: &str = "BacklogService/CreateNote";

async fn call(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

/// Every entry the filter matches, one page of `limit` at a time, with the
/// number of pages it took.
async fn page_all(addr: std::net::SocketAddr, filter: Value, limit: i64) -> (Vec<Value>, usize) {
    let mut entries = Vec::new();
    let mut token = String::new();
    for pages in 1.. {
        let mut request = filter.clone();
        request["limit"] = json!(limit);
        request["pageToken"] = json!(token);
        let (status, page) = call(addr, "AuditService/ListAuditEntries", OWNER, request).await;
        assert_eq!(status, 200, "{page}");
        let batch = page["entries"].as_array().cloned().unwrap_or_default();
        assert!(batch.len() <= limit as usize, "{page}");
        entries.extend(batch);
        token = page["nextPageToken"].as_str().unwrap_or("").to_owned();
        if token.is_empty() {
            return (entries, pages);
        }
        assert!(pages < 100, "paging does not end: {page}");
    }
    unreachable!()
}

fn actors(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .map(|e| {
            format!(
                "{} {}",
                e["actor"].as_str().unwrap(),
                e["outcome"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test]
async fn owner_pages_the_audit_log_across_months_with_filters() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), OWNER, "test").unwrap();
    for day in 1..=3 {
        hub.audit
            .try_write(Entry {
                time: Utc.with_ymd_and_hms(2025, 12, day, 12, 0, 0).unwrap(),
                actor: "owner".into(),
                procedure: format!("/protectorate.v1.{CREATE}"),
                subject: format!("december-{day}"),
                outcome: "ok".into(),
                ..Entry::default()
            })
            .unwrap();
    }
    for (name, run) in [("w1", "run-of-worker-one"), ("w2", "run-of-worker-two")] {
        let work = dir.path().join(name);
        std::fs::create_dir_all(&work).unwrap();
        let mut record = Record::new(
            name,
            Harness::Shell,
            Mode::Terminal,
            &work,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        record.run_id = run.into();
        hub.sleeves.record_spawned(record).unwrap();
        hub.sleeves
            .record_exit(name, "2026-01-01T00:00:01Z".into(), None, 0, false, false)
            .unwrap();
    }
    let (_, reader) = capability::mint(&hub, "reader", &["read"]);
    let (_, w1) = capability::mint(&hub, "sleeve:w1", &["backlog_*"]);
    let (_, w2) = capability::mint(&hub, "sleeve:w2", &["backlog_*"]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    for (token, status) in [
        (OWNER, 200),
        (OWNER, 200),
        (OWNER, 200),
        (reader.as_str(), 403),
        (reader.as_str(), 403),
        (w1.as_str(), 200),
        (w1.as_str(), 200),
        (w2.as_str(), 200),
    ] {
        let (got, body) = call(addr, CREATE, token, json!({"title":"audited"})).await;
        assert_eq!(got, status, "{body}");
    }
    let mut expected = vec![
        "token:sleeve:w2 ok",
        "token:sleeve:w1 ok",
        "token:sleeve:w1 ok",
    ];
    expected.extend(["token:reader denied"; 2]);
    expected.extend(["owner ok"; 6]);

    let (whole, pages) = page_all(addr, json!({"procedure":CREATE}), 500).await;
    assert_eq!(actors(&whole), expected);
    assert_eq!(pages, 1);
    let (paged, pages) = page_all(addr, json!({"procedure":CREATE}), 3).await;
    assert_eq!(actors(&paged), expected);
    assert!(pages >= 4, "{pages} pages");
    assert_eq!(paged, whole);
    let december = paged[8..]
        .iter()
        .map(|e| e["subject"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(december, ["december-3", "december-2", "december-1"]);
    let seqs = paged
        .iter()
        .map(|e| e["seq"].as_str().unwrap().parse::<u64>().unwrap())
        .collect::<Vec<_>>();
    assert!(seqs.windows(2).all(|w| w[0] > w[1]), "{seqs:?}");

    let (w1_run, pages) = page_all(addr, json!({"runId":"run-of-worker-one"}), 1).await;
    assert_eq!(actors(&w1_run), ["token:sleeve:w1 ok"; 2]);
    assert!(pages >= 2, "{pages} pages");
    assert!(w1_run.iter().all(|e| e["runId"] == "run-of-worker-one"));
    let (none, _) = page_all(addr, json!({"runId":"run-of-worker"}), 50).await;
    assert!(none.is_empty(), "run ids match exactly: {none:?}");
    let (denied, _) = page_all(addr, json!({"outcome":"denied","actor":"READER"}), 50).await;
    assert_eq!(actors(&denied), ["token:reader denied"; 2]);

    for cursor in [
        "nonsense",
        "../audit-2025-12.jsonl:1",
        "audit-2025-12.jsonl:x",
    ] {
        let (status, bad) = call(
            addr,
            "AuditService/ListAuditEntries",
            OWNER,
            json!({"pageToken":cursor}),
        )
        .await;
        assert_eq!(
            (status, &bad["code"]),
            (400, &json!("invalid_argument")),
            "{cursor}"
        );
    }
    let (status, denied) = call(addr, "AuditService/ListAuditEntries", &reader, json!({})).await;
    assert_eq!(status, 403, "{denied}");
    let (status, verified) = call(addr, "AuditService/VerifyAudit", OWNER, json!({})).await;
    assert_eq!(status, 200, "{verified}");
    assert_eq!(verified["ok"], true, "{verified}");
    server.abort();
}
