//! The owner's view of the mailbox over HTTP: its own inbox, reading and
//! acking as a recipient, and the thread list that only the owner sees whole.
#[path = "support/capability.rs"]
mod capability;
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const OWNER: &str = "owner-secret";

async fn call(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /protectorate.v1.MessageService/{method} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

async fn ok(addr: std::net::SocketAddr, method: &str, token: &str, body: Value) -> Value {
    let (status, out) = call(addr, method, token, body).await;
    assert_eq!(status, 200, "{method}: {out}");
    out
}

fn ids(list: &Value, key: &str, field: &str) -> Vec<String> {
    list[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item[field].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn owner_reads_and_acks_as_a_recipient_and_lists_every_thread() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path(), OWNER, "test").unwrap();
    for name in ["bob", "carol"] {
        let work = dir.path().join(name);
        std::fs::create_dir_all(&work).unwrap();
        hub.sleeves
            .record_spawned(
                Record::new(
                    name,
                    Harness::Shell,
                    Mode::Terminal,
                    &work,
                    "2026-01-01T00:00:00Z",
                )
                .unwrap(),
            )
            .unwrap();
        hub.sleeves
            .record_exit(name, "2026-01-01T00:00:01Z".into(), None, 0, false, false)
            .unwrap();
    }
    let token = |name: &str| capability::mint(&hub, &format!("sleeve:{name}"), &["message_*"]).1;
    let (alice, bob) = (token("alice"), token("bob"));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });

    let hello = ok(addr, "Send", OWNER, json!({"to":"bob","body":"hello bob"})).await;
    let request = ok(
        addr,
        "Send",
        &alice,
        json!({"to":"carol","body":"please review","kind":"MESSAGE_KIND_REQUEST"}),
    )
    .await;
    let note = ok(
        addr,
        "Send",
        &bob,
        json!({"to":"owner","body":"bob is done"}),
    )
    .await;
    let thread = |sent: &Value| sent["message"]["threadId"].as_str().unwrap().to_owned();
    let (t_hello, t_request, t_note) = (thread(&hello), thread(&request), thread(&note));

    let all = ok(addr, "ListThreads", OWNER, json!({})).await;
    assert_eq!(
        ids(&all, "threads", "threadId"),
        [&*t_note, &t_request, &t_hello]
    );
    assert_eq!(all["threads"][0]["unread"], 1, "{all}");
    assert_eq!(all["threads"][1]["openAcks"], 1, "{all}");
    assert_eq!(
        all["threads"][1]["participants"],
        json!(["alice", "carol"]),
        "{all}"
    );
    let carol = ok(addr, "ListThreads", OWNER, json!({"sleeve":"carol"})).await;
    assert_eq!(ids(&carol, "threads", "threadId"), [&*t_request]);
    let newest = ok(addr, "ListThreads", OWNER, json!({"limit":1})).await;
    assert_eq!(ids(&newest, "threads", "threadId"), [&*t_note]);
    let (status, bad) = call(addr, "ListThreads", OWNER, json!({"limit":-1})).await;
    assert_eq!((status, &bad["code"]), (400, &json!("invalid_argument")));
    let mine = ok(addr, "ListThreads", &bob, json!({})).await;
    assert_eq!(ids(&mine, "threads", "threadId"), [&*t_note, &t_hello]);

    let inbox = ok(addr, "Inbox", OWNER, json!({})).await;
    assert_eq!(
        ids(&inbox, "messages", "id"),
        [note["message"]["id"].as_str().unwrap()]
    );
    let all = ok(addr, "ListThreads", OWNER, json!({})).await;
    assert!(all["threads"][0].get("unread").is_none(), "{all}");

    let peeked = ok(addr, "Inbox", OWNER, json!({"as":"carol","peek":true})).await;
    assert_eq!(
        ids(&peeked, "messages", "id"),
        [request["message"]["id"].as_str().unwrap()]
    );
    let unread = ok(
        addr,
        "Inbox",
        OWNER,
        json!({"as":"carol","unreadOnly":true}),
    )
    .await;
    assert_eq!(unread["messages"].as_array().unwrap().len(), 1, "{unread}");
    let (status, denied) = call(addr, "Inbox", &bob, json!({"as":"carol"})).await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let (status, denied) = call(
        addr,
        "Ack",
        &bob,
        json!({"id":request["message"]["id"],"as":"carol"}),
    )
    .await;
    assert_eq!(
        (status, &denied["code"]),
        (403, &json!("permission_denied"))
    );
    let acked = ok(
        addr,
        "Ack",
        OWNER,
        json!({"id":request["message"]["id"],"as":"carol"}),
    )
    .await;
    assert!(
        acked["message"]["deliveries"][0]["ackedAt"].is_string(),
        "{acked}"
    );
    let carol = ok(addr, "ListThreads", OWNER, json!({"sleeve":"carol"})).await;
    assert!(carol["threads"][0].get("openAcks").is_none(), "{carol}");

    let reply = ok(
        addr,
        "Reply",
        OWNER,
        json!({"id":note["message"]["id"],"body":"thanks"}),
    )
    .await;
    assert_eq!(reply["message"]["to"], "bob");
    let bob_inbox = ok(addr, "Inbox", &bob, json!({"unreadOnly":true,"peek":true})).await;
    assert_eq!(
        ids(&bob_inbox, "messages", "id"),
        [
            hello["message"]["id"].as_str().unwrap(),
            reply["message"]["id"].as_str().unwrap()
        ]
    );
    let all = ok(addr, "ListThreads", OWNER, json!({})).await;
    assert_eq!(all["threads"][0]["threadId"], t_note.as_str());
    assert_eq!(all["threads"][0]["messageCount"], 2, "{all}");
    let (status, missing) = call(addr, "Send", OWNER, json!({"to":"nobody","body":"hi"})).await;
    assert_eq!((status, &missing["code"]), (404, &json!("not_found")));
    server.abort();
}
