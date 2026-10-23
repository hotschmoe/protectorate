use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Limits, Mode, ReadOnlyMount, Record};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn serve(hub: Hub) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (addr, task)
}
async fn call(addr: std::net::SocketAddr, path: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

#[tokio::test]
async fn sleeve_reads_preserve_user_visible_metadata_and_reconcile_restart() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    let mut chat = Record::new(
        "coder",
        Harness::Codex,
        Mode::Chat,
        dir.path(),
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    chat.parent = "envoy".into();
    chat.parent_token_id = "secret-token-id".into();
    chat.limits = Limits {
        cpu_percent: 200,
        memory_bytes: 4_294_967_296,
    };
    chat.nested = true;
    chat.supervised = true;
    chat.args = vec!["--model".into(), "test".into()];
    chat.read_only = vec![ReadOnlyMount {
        source: "/source/reference".into(),
        target: "/references".into(),
    }];
    chat.exit_after_idle = Some("300s".into());
    chat.base_repo = "https://example.test/repo".into();
    chat.base_branch = "main".into();
    chat.base_tip = "abc123".into();
    hub.sleeves.record_spawned(chat).unwrap();
    hub.sleeves.set_chat("coder", json!({"state":"CHAT_STATE_IDLE","harnessSessionId":"native-session","model":"gpt-test"})).unwrap();
    hub.sleeves
        .set_result(
            "coder",
            json!({"status":"REPORT_STATUS_PARTIAL","summary":"still working"}),
        )
        .unwrap();
    let mut terminal = Record::new(
        "shell",
        Harness::Shell,
        Mode::Terminal,
        dir.path(),
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    terminal.parent = "owner".into();
    hub.sleeves.record_spawned(terminal).unwrap();
    let mut helper = Record::new(
        "helper",
        Harness::Pi,
        Mode::Chat,
        dir.path(),
        "2026-10-22T00:00:00Z",
    )
    .unwrap();
    helper.helper = true;
    hub.sleeves.record_spawned(helper).unwrap();
    let (addr, server) = serve(hub).await;
    let list = "/protectorate.v1.SleeveService/ListSleeves";
    let get = "/protectorate.v1.SleeveService/GetSleeve";
    let (status, unauthorized) = call(addr, list, "wrong", json!({})).await;
    assert_eq!(
        (status, &unauthorized["code"]),
        (401, &json!("unauthenticated"))
    );
    let (status, listed) = call(addr, list, "owner-secret", json!({})).await;
    assert_eq!(status, 200, "{listed}");
    let rows = listed["sleeves"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "coder");
    assert_eq!(rows[1]["name"], "shell");
    assert_eq!(rows[0]["mode"], "SLEEVE_MODE_CHAT");
    assert_eq!(rows[1]["mode"], "SLEEVE_MODE_TERMINAL");
    assert_eq!(rows[0]["harness"], "HARNESS_CODEX");
    assert_eq!(rows[0]["parent"], "envoy");
    assert_eq!(rows[0]["nested"], true);
    assert_eq!(rows[0]["supervised"], true);
    assert_eq!(rows[0]["readOnly"][0]["source"], "/source/reference");
    assert_eq!(rows[0]["limits"]["memoryBytes"], "4294967296");
    assert_eq!(rows[0]["chat"]["harnessSessionId"], "native-session");
    assert_eq!(rows[0]["result"]["summary"], "still working");
    assert_eq!(rows[0]["exitAfterIdle"], "300s");
    assert!(rows[0].get("parentTokenId").is_none());
    let (status, missing) = call(addr, get, "owner-secret", json!({"name":"missing"})).await;
    assert_eq!((status, &missing["code"]), (404, &json!("not_found")));
    let (status, hidden) = call(addr, get, "owner-secret", json!({"name":"helper"})).await;
    assert_eq!((status, &hidden["code"]), (404, &json!("not_found")));
    server.abort();
    let (addr, server) = serve(Hub::open(dir.path(), "owner-secret", "test").unwrap()).await;
    let (status, restarted) = call(addr, get, "owner-secret", json!({"name":"coder"})).await;
    assert_eq!(status, 200, "{restarted}");
    assert_eq!(restarted["sleeve"]["state"], "SLEEVE_STATE_EXITED");
    assert!(
        restarted["sleeve"].get("exitCode").is_none() || restarted["sleeve"]["exitCode"].is_null()
    );
    assert!(restarted["sleeve"]["exitedAt"].as_str().is_some());
    assert_eq!(restarted["sleeve"]["chat"]["model"], "gpt-test");
    server.abort();
}
