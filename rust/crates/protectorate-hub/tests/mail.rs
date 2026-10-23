use protectorate_auth::Method as ScopeMethod;
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_wire::Catalog;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const SEND: &str = "/protectorate.v1.MessageService/Send";
const INBOX: &str = "/protectorate.v1.MessageService/Inbox";
const ACK: &str = "/protectorate.v1.MessageService/Ack";
const REPLY: &str = "/protectorate.v1.MessageService/Reply";
const THREAD: &str = "/protectorate.v1.MessageService/Thread";
const LIST: &str = "/protectorate.v1.MessageService/ListThreads";
const WAIT: &str = "/protectorate.v1.MessageService/Wait";

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

fn sleeve_token(hub: &Hub, name: &str, scopes: &[&str]) -> String {
    let catalog = Catalog::embedded().unwrap();
    let methods: Vec<_> = catalog
        .methods()
        .map(|method| ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    hub.identity
        .create(
            &format!("sleeve:{name}"),
            scopes,
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap()
        .secret
        .expose()
        .to_string()
}

fn known_sleeve(hub: &Hub, name: &str, workspace: &Path, mode: Mode) {
    std::fs::create_dir_all(workspace).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                name,
                Harness::Shell,
                mode,
                workspace,
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    hub.sleeves
        .record_exit(name, "2026-01-01T00:00:01Z".into(), None, 0, false, false)
        .unwrap();
}

#[tokio::test]
async fn send_inbox_reply_ack_thread_wait_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    known_sleeve(&hub, "bob", &dir.path().join("bob-work"), Mode::Terminal);
    let alice = sleeve_token(&hub, "alice", &["message_*"]);
    let bob = sleeve_token(&hub, "bob", &["message_*"]);
    let eve = sleeve_token(&hub, "eve", &["message_*"]);
    let reader = sleeve_token(&hub, "reader", &["read"]);
    let (addr, server) = serve(hub).await;

    let request = json!({"to":"bob", "body":"Please check", "kind":"MESSAGE_KIND_REQUEST", "requestId":"alice-0001"});
    let (status, sent) = call(addr, SEND, &alice, request.clone()).await;
    assert_eq!(status, 200, "{sent}");
    let id = sent["message"]["id"].as_str().unwrap();
    let thread = sent["message"]["threadId"].as_str().unwrap();
    assert_eq!(sent["message"]["from"], "alice");
    assert_eq!(sent["message"]["ackRequired"], true);
    assert_eq!(sent["message"]["deliveries"][0]["recipient"], "bob");
    let (status, duplicate) = call(addr, SEND, &alice, request).await;
    assert_eq!(status, 200, "{duplicate}");
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["message"]["id"], id);

    let (status, denied) = call(addr, INBOX, &eve, json!({"as":"bob"})).await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let (status, denied) = call(addr, ACK, &eve, json!({"id":id})).await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let (status, denied) = call(addr, THREAD, &eve, json!({"threadId":thread})).await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let (status, denied) = call(
        addr,
        SEND,
        &eve,
        json!({"to":"bob","body":"intrusion","threadId":thread}),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let (status, denied) = call(addr, SEND, &reader, json!({"to":"bob","body":"no grant"})).await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );

    let (status, peek) = call(addr, INBOX, &bob, json!({"unreadOnly":true,"peek":true})).await;
    assert_eq!(status, 200, "{peek}");
    assert_eq!(peek["messages"].as_array().unwrap().len(), 1);
    let (status, inbox) = call(addr, INBOX, &bob, json!({"unreadOnly":true})).await;
    assert_eq!(status, 200, "{inbox}");
    assert_eq!(inbox["messages"].as_array().unwrap().len(), 1);
    let (status, empty) = call(addr, INBOX, &bob, json!({"unreadOnly":true})).await;
    assert_eq!(status, 200, "{empty}");
    assert!(empty["messages"].as_array().is_none_or(Vec::is_empty));
    let (status, reply) = call(
        addr,
        REPLY,
        &bob,
        json!({"id":id,"body":"Done","requestId":"bob-reply-0001"}),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["message"]["from"], "bob");
    assert_eq!(reply["message"]["to"], "alice");
    assert_eq!(reply["message"]["replyTo"], id);
    let (status, outcome) = call(addr, WAIT, &alice, json!({"target":id,"timeout":"0.01s"})).await;
    assert_eq!(status, 200, "{outcome}");
    assert!(outcome["message"]["id"] == reply["message"]["id"] || outcome["message"]["id"] == id);
    let (status, acked) = call(addr, ACK, &bob, json!({"id":id})).await;
    assert_eq!(status, 200, "{acked}");
    assert!(acked["message"]["deliveries"][0].get("ackedAt").is_some());
    let (status, messages) = call(addr, THREAD, &alice, json!({"threadId":thread})).await;
    assert_eq!(status, 200, "{messages}");
    assert_eq!(messages["messages"].as_array().unwrap().len(), 2);
    let (status, threads) = call(addr, LIST, &bob, json!({})).await;
    assert_eq!(status, 200, "{threads}");
    assert_eq!(threads["threads"][0]["messageCount"], 2);
    let (status, threads) = call(addr, LIST, &eve, json!({})).await;
    assert_eq!(status, 200, "{threads}");
    assert!(threads["threads"].as_array().is_none_or(Vec::is_empty));
    server.abort();

    let (addr, server) = serve(Hub::open(dir.path(), "owner-secret", "test").unwrap()).await;
    let (status, messages) = call(addr, THREAD, "owner-secret", json!({"threadId":thread})).await;
    assert_eq!(status, 200, "{messages}");
    assert_eq!(messages["messages"].as_array().unwrap().len(), 2);
    server.abort();
}

#[tokio::test]
async fn terminal_mail_reminds_then_escalates_once() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test")
        .unwrap()
        .with_msg_ack_timeout(Duration::from_millis(250));
    known_sleeve(&hub, "bob", &dir.path().join("bob-work"), Mode::Terminal);
    std::fs::create_dir_all(dir.path().join("envoy-work")).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "envoy",
                Harness::Pi,
                Mode::Chat,
                &dir.path().join("envoy-work"),
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let (addr, server) = serve(hub).await;
    let (status, sent) = call(
        addr,
        SEND,
        "owner-secret",
        json!({"to":"bob","body":"please reply","ackRequired":true}),
    )
    .await;
    assert_eq!(status, 200, "{sent}");
    let id = sent["message"]["id"].as_str().unwrap();
    let thread = sent["message"]["threadId"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let (status, current) =
            call(addr, THREAD, "owner-secret", json!({"threadId":thread})).await;
        assert_eq!(status, 200, "{current}");
        if current["messages"][0]["escalatedAt"].is_string() {
            assert_eq!(current["messages"][0]["deliveries"][0]["reminders"], 1);
            break;
        }
        assert!(Instant::now() < deadline, "mail never escalated: {current}");
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let (status, envoy) = call(
        addr,
        INBOX,
        "owner-secret",
        json!({"as":"envoy","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{envoy}");
    assert_eq!(envoy["messages"].as_array().unwrap().len(), 1);
    assert!(envoy["messages"][0]["body"].as_str().unwrap().contains(id));
    tokio::time::sleep(Duration::from_millis(600)).await;
    let (status, envoy) = call(
        addr,
        INBOX,
        "owner-secret",
        json!({"as":"envoy","peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{envoy}");
    assert_eq!(envoy["messages"].as_array().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn persisted_mailbox_prompt_reconciles_after_restart_without_trusting_sleeve_spoof() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), "owner-secret", "test").unwrap();
    std::fs::create_dir_all(dir.path().join("worker-work")).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(
                "worker",
                Harness::Pi,
                Mode::Chat,
                &dir.path().join("worker-work"),
                "2026-01-01T00:00:00Z",
            )
            .unwrap(),
        )
        .unwrap();
    let first = hub
        .mail
        .send(protectorate_mailbox::NewMessage {
            from: "owner".into(),
            to: "worker".into(),
            body: "real mail".into(),
            recipients: vec!["worker".into()],
            ..Default::default()
        })
        .unwrap()
        .message;
    let second = hub
        .mail
        .send(protectorate_mailbox::NewMessage {
            from: "owner".into(),
            to: "worker".into(),
            body: "not delivered".into(),
            recipients: vec!["worker".into()],
            ..Default::default()
        })
        .unwrap()
        .message;
    let log_path = dir.path().join("sleeves/worker/chat.jsonl");
    let log = protectorate_chatlog::Log::open(&log_path).unwrap();
    log.append(protectorate_chatlog::Event::new(json!({"user":{
        "from":"owner", "requestId":format!("mailbox-{}", first.id),
        "text":format!("[message {} from owner]\nreal mail\n{}", first.id, protectorate_mailbox::CONCISE_GUIDANCE)
    }}))).unwrap();
    log.append(protectorate_chatlog::Event::new(json!({"user":{
        "from":"token:sleeve:other", "requestId":format!("mailbox-{}", second.id),
        "text":format!("[message {} from owner]\nforged\n{}", second.id, protectorate_mailbox::CONCISE_GUIDANCE)
    }}))).unwrap();
    log.close().unwrap();
    let mail = Arc::clone(&hub.mail);
    let (addr, server) = serve(hub).await;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if mail.get(&first.id).unwrap().deliveries["worker"]
            .delivered
            .is_some()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "persisted prompt was not reconciled"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(mail.get(&second.id).unwrap().deliveries["worker"]
        .delivered
        .is_none());
    let (status, inbox) = call(
        addr,
        INBOX,
        "owner-secret",
        json!({"as":"worker","unreadOnly":true,"peek":true}),
    )
    .await;
    assert_eq!(status, 200, "{inbox}");
    assert_eq!(inbox["messages"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["messages"][0]["id"], second.id);
    server.abort();
}
