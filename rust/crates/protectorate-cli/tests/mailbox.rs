#![cfg(unix)]

//! The `msg` commands against a real hub: the real mailbox log, the real
//! sleeve registry, the real backlog store and the real Connect JSON router,
//! driven only through the CLI dispatcher with a sleeve's own token or the
//! owner token.
use protectorate_auth::Method as ScopeMethod;
use protectorate_cli::{dispatch_endpoint, parse, transport_timeout, Command, MessageKind, Result};
use protectorate_directory::{Scan, Workspace};
use protectorate_hub::Hub;
use protectorate_sleeve_registry::{Harness, Mode, Record};
use protectorate_wire::{Catalog, Code};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;

const OWNER: &str = "owner-secret";

/// A token that names a sleeve, the way the hub issues one to a sleeve.
fn sleeve_token(hub: &Hub, name: &str) -> String {
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
            &["message_*"],
            Duration::from_secs(3600),
            now,
            &methods,
        )
        .unwrap()
        .secret
        .expose()
        .to_string()
}

/// A sleeve the hub knows about. The fake chat sleeve has no runtime process,
/// so the hub keeps its registry record while the CLI exercises the mailbox.
fn running_sleeve(hub: &Hub, name: &str, workspace: &Path, mode: Mode) {
    std::fs::create_dir_all(workspace).unwrap();
    hub.sleeves
        .record_spawned(
            Record::new(name, Harness::Pi, mode, workspace, "2026-01-01T00:00:00Z").unwrap(),
        )
        .unwrap();
}

async fn serve(hub: Hub) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, hub.router().unwrap()).await.unwrap() });
    (url, task)
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

/// A protobuf JSON response omits a flag the hub left at its default, so a
/// test reads an absent one as false.
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn code(result: Result<String>) -> std::result::Result<String, Code> {
    result.map_err(|error| error.code)
}

type Calls = std::vec::IntoIter<std::result::Result<String, Code>>;

fn ok(calls: &mut Calls, label: &str) -> String {
    match calls.next().unwrap() {
        Ok(text) => text,
        Err(code) => panic!("expected {label} to succeed, got {code:?}"),
    }
}

fn fail(calls: &mut Calls, label: &str) -> Code {
    match calls.next().unwrap() {
        Ok(text) => panic!("expected {label} to fail, got {text}"),
        Err(code) => code,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_send_inbox_reply_ack_thread_and_threads_drive_a_real_hub() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), OWNER, "test").unwrap();
    running_sleeve(&hub, "bob", &home.path().join("bob-work"), Mode::Chat);
    let alice = sleeve_token(&hub, "alice");
    let bob = sleeve_token(&hub, "bob");
    let eve = sleeve_token(&hub, "eve");
    let (url, server) = serve(hub).await;

    let (calls, sent) = tokio::task::spawn_blocking(move || {
        let sent = dispatch_endpoint(
            &url,
            &alice,
            [
                "--json",
                "msg",
                "send",
                "bob",
                "Please",
                "check",
                "the",
                "hub",
                "--kind",
                "request",
                "--request-id",
                "alice-0001",
            ],
        )
        .expect("msg send");
        let id = json(&sent)["message"]["id"].as_str().unwrap().to_owned();
        let thread = json(&sent)["message"]["threadId"]
            .as_str()
            .unwrap()
            .to_owned();
        let commands: Vec<(String, Vec<&str>)> = vec![
            // The same request id sends once.
            (
                alice.clone(),
                vec![
                    "--json",
                    "msg",
                    "send",
                    "bob",
                    "Please",
                    "check",
                    "the",
                    "hub",
                    "--kind",
                    "request",
                    "--request-id",
                    "alice-0001",
                ],
            ),
            // A note in the same thread, and one that asks for no ack.
            (
                alice.clone(),
                vec![
                    "msg",
                    "send",
                    "bob",
                    "One",
                    "more",
                    "thing",
                    "--thread",
                    thread.as_str(),
                ],
            ),
            (
                alice.clone(),
                vec![
                    "msg",
                    "send",
                    "bob",
                    "FYI",
                    "--kind",
                    "notice",
                    "--ack=false",
                ],
            ),
            (bob.clone(), vec!["msg", "inbox", "--unread", "--peek"]),
            (bob.clone(), vec!["msg", "inbox", "--unread", "--peek"]),
            (bob.clone(), vec!["msg", "inbox", "--unread"]),
            (bob.clone(), vec!["msg", "inbox", "--unread"]),
            (bob.clone(), vec!["--json", "msg", "inbox", "--unread"]),
            (
                bob.clone(),
                vec!["--json", "msg", "inbox", "--thread", thread.as_str()],
            ),
            (eve.clone(), vec!["msg", "inbox", "--as", "bob"]),
            (eve.clone(), vec!["msg", "ack", id.as_str()]),
            (eve.clone(), vec!["msg", "thread", thread.as_str()]),
            (
                bob.clone(),
                vec!["msg", "reply", id.as_str(), "Tests", "pass"],
            ),
            (
                bob.clone(),
                vec!["msg", "reply", id.as_str(), "Tests", "pass"],
            ),
            (alice.clone(), vec!["msg", "thread", thread.as_str()]),
            (alice.clone(), vec!["--json", "msg", "threads"]),
            (
                alice.clone(),
                vec!["msg", "threads", "--sleeve", "bob", "--limit", "5"],
            ),
            (
                OWNER.to_owned(),
                vec!["msg", "inbox", "--as", "bob", "--peek"],
            ),
            (
                OWNER.to_owned(),
                vec!["msg", "ack", id.as_str(), "--as", "bob"],
            ),
            (alice.clone(), vec!["msg", "send", "ghost", "hello"]),
            (alice.clone(), vec!["msg", "ack", "m9999"]),
            (alice.clone(), vec!["msg", "thread", "t9999"]),
            (alice.clone(), vec!["msg", "wait", "nothing-here"]),
        ];
        let calls = commands
            .iter()
            .map(|(token, args)| code(dispatch_endpoint(&url, token, args.iter())))
            .collect::<Vec<_>>();
        (calls, sent)
    })
    .await
    .unwrap();
    server.abort();

    let sent: Value = json(&sent);
    let id = sent["message"]["id"].as_str().unwrap().to_owned();
    let thread = sent["message"]["threadId"].as_str().unwrap().to_owned();
    assert_eq!(sent["message"]["from"], "alice", "{sent}");
    assert_eq!(sent["message"]["to"], "bob", "{sent}");
    assert_eq!(sent["message"]["kind"], "MESSAGE_KIND_REQUEST", "{sent}");
    assert_eq!(sent["message"]["ackRequired"], true, "{sent}");
    assert_eq!(
        sent["message"]["deliveries"][0]["recipient"], "bob",
        "{sent}"
    );
    assert!(!flag(&sent, "duplicate"), "{sent}");
    assert!(!flag(&sent, "waiting"), "{sent}");

    let mut calls = calls.into_iter();

    let duplicate: Value = json(&ok(&mut calls, "msg send retry"));
    assert!(flag(&duplicate, "duplicate"), "{duplicate}");
    assert_eq!(duplicate["message"]["id"], id, "{duplicate}");

    let threaded = ok(&mut calls, "msg send --thread");
    assert!(threaded.contains(&thread), "{threaded}");
    assert!(!threaded.contains("waiting"), "{threaded}");

    let notice = ok(&mut calls, "msg send --kind notice --ack=false");
    assert!(notice.contains("message"), "{notice}");

    let peeked = ok(&mut calls, "msg inbox --unread --peek");
    assert!(
        peeked.starts_with(protectorate_mailbox::CONCISE_GUIDANCE),
        "{peeked}"
    );
    assert!(peeked.contains("alice -> bob"), "{peeked}");
    assert!(peeked.contains("request ack-required"), "{peeked}");
    assert!(peeked.contains("check the hub"), "{peeked}");
    assert_eq!(
        peeked.lines().filter(|line| line.starts_with('m')).count(),
        3,
        "peek does not mark the mail read: {peeked}"
    );

    let peeked_again = ok(&mut calls, "msg inbox --unread --peek again");
    assert_eq!(peeked_again, peeked, "peeking twice changes nothing");

    let read = ok(&mut calls, "msg inbox --unread");
    assert!(read.contains("check the hub"), "{read}");
    assert_eq!(
        ok(&mut calls, "msg inbox --unread after reading"),
        "no messages\n",
        "inbox marked the mail read"
    );
    let empty = json(&ok(&mut calls, "json msg inbox --unread after reading"));
    assert!(empty["messages"].is_null() || empty["messages"].as_array().is_some_and(Vec::is_empty));

    let response = json(&ok(&mut calls, "json msg inbox --thread"));
    let inbox = response["messages"].as_array().unwrap();
    assert_eq!(inbox.len(), 2, "{inbox:?}");
    assert_eq!(inbox[0]["id"], id, "{inbox:?}");
    assert!(
        inbox[0]["deliveries"][0].get("readAt").is_some(),
        "{inbox:?}"
    );
    assert_eq!(inbox[1]["kind"], "MESSAGE_KIND_NOTE", "{inbox:?}");

    assert_eq!(
        fail(&mut calls, "inbox as another sleeve"),
        Code::PermissionDenied
    );
    assert_eq!(
        fail(&mut calls, "ack as a non-recipient"),
        Code::PermissionDenied
    );
    assert_eq!(
        fail(&mut calls, "thread as an outsider"),
        Code::PermissionDenied
    );

    let reply = ok(&mut calls, "msg reply");
    assert!(reply.contains("message"), "{reply}");
    assert!(reply.contains(&thread), "{reply}");
    assert!(reply.contains("to       alice"), "{reply}");

    let duplicate_reply = ok(&mut calls, "msg reply without a request id");
    assert_ne!(duplicate_reply, reply, "a reply without an id sends again");

    let thread_text = ok(&mut calls, "msg thread");
    assert!(
        thread_text.contains(&format!("{id}  {thread}  alice -> bob")),
        "{thread_text}"
    );
    assert!(thread_text.contains("acked:bob"), "{thread_text}");
    assert!(thread_text.contains("bob -> alice"), "{thread_text}");

    let threads: Vec<Value> = ok(&mut calls, "json msg threads")
        .lines()
        .map(json)
        .collect();
    assert_eq!(threads.len(), 2, "{threads:?}");
    assert_eq!(threads[0]["threadId"], thread, "{threads:?}");
    assert_eq!(threads[0]["messageCount"], 4, "{threads:?}");
    assert_eq!(
        threads[0]["participants"],
        serde_json::json!(["alice", "bob"]),
        "{threads:?}"
    );
    assert_eq!(threads[1]["messageCount"], 1, "{threads:?}");

    let rows = ok(&mut calls, "msg threads --sleeve bob");
    assert!(rows.contains(&thread), "{rows}");
    assert!(rows.contains("4 msgs"), "{rows}");
    assert!(rows.contains("alice,bob"), "{rows}");

    let proxy = ok(&mut calls, "owner msg inbox --as bob");
    assert!(proxy.contains("alice -> bob"), "{proxy}");

    let owner_ack = ok(&mut calls, "owner msg ack --as bob");
    assert_eq!(owner_ack, format!("acked  {id}\n"), "{owner_ack}");

    assert_eq!(fail(&mut calls, "send to no sleeve"), Code::NotFound);
    assert_eq!(fail(&mut calls, "ack no message"), Code::NotFound);
    assert_eq!(fail(&mut calls, "thread no thread"), Code::NotFound);
    assert_eq!(fail(&mut calls, "wait for nothing"), Code::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_wait_blocks_for_an_ack_and_reports_a_timeout() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), OWNER, "test").unwrap();
    running_sleeve(&hub, "bob", &home.path().join("bob-work"), Mode::Chat);
    let alice = sleeve_token(&hub, "alice");
    let bob = sleeve_token(&hub, "bob");
    let (url, server) = serve(hub).await;

    let (calls, sent) = tokio::task::spawn_blocking(move || {
        let sent = dispatch_endpoint(
            &url,
            &alice,
            [
                "--json", "msg", "send", "bob", "Ack", "this", "request", "--kind", "request",
            ],
        )
        .expect("msg send");
        let id = json(&sent)["message"]["id"].as_str().unwrap().to_owned();
        let thread = json(&sent)["message"]["threadId"]
            .as_str()
            .unwrap()
            .to_owned();
        let commands: Vec<(String, Vec<&str>)> = vec![
            (
                alice.clone(),
                vec!["msg", "wait", id.as_str(), "--timeout", "250ms"],
            ),
            (
                alice.clone(),
                vec![
                    "--json",
                    "msg",
                    "wait",
                    thread.as_str(),
                    "--timeout",
                    "250ms",
                ],
            ),
            (bob.clone(), vec!["msg", "ack", id.as_str()]),
            (alice.clone(), vec!["msg", "wait", id.as_str()]),
            (
                alice.clone(),
                vec!["--json", "msg", "wait", thread.as_str()],
            ),
        ];
        let calls = commands
            .iter()
            .map(|(token, args)| code(dispatch_endpoint(&url, token, args.iter())))
            .collect::<Vec<_>>();
        (calls, sent)
    })
    .await
    .unwrap();
    server.abort();

    let id = json(&sent)["message"]["id"].as_str().unwrap().to_owned();
    let mut calls = calls.into_iter();

    assert_eq!(ok(&mut calls, "msg wait before the ack"), "timed out\n");
    assert_eq!(
        json(&ok(&mut calls, "json msg wait before the ack"))["timedOut"],
        true
    );
    let acked = ok(&mut calls, "msg ack");
    assert_eq!(acked, format!("acked  {id}\n"), "{acked}");

    let waited = ok(&mut calls, "msg wait after the ack");
    assert!(
        waited.contains(&format!("{id}  t1  alice -> bob")),
        "{waited}"
    );
    assert!(
        waited.contains("request ack-required acked:bob"),
        "{waited}"
    );

    let waited: Value = json(&ok(&mut calls, "json msg wait by thread"));
    assert_ne!(waited["timedOut"], true, "{waited}");
    assert_eq!(waited["message"]["id"], id, "{waited}");
}

#[tokio::test(flavor = "multi_thread")]
async fn project_mail_waits_for_a_sleeve_and_files_one_note() {
    let home = tempfile::tempdir().unwrap();
    let hub = Hub::open(home.path(), OWNER, "test").unwrap();
    hub.directory
        .update(&[Scan {
            sleeve: "worker".into(),
            workspace: Workspace {
                path: home.path().join("repo"),
                remote: "github.com/acme/repo".into(),
                ..Default::default()
            },
        }])
        .unwrap();
    let (url, server) = serve(hub).await;

    let calls = tokio::task::spawn_blocking(move || {
        let commands: Vec<Vec<&str>> = vec![
            vec![
                "msg",
                "send",
                "project:acme/repo",
                "Inspect",
                "the",
                "build",
                "--kind",
                "request",
                "--request-id",
                "owner-0001",
            ],
            vec![
                "--json",
                "msg",
                "send",
                "project:acme/repo",
                "Inspect",
                "the",
                "build",
                "--kind",
                "request",
                "--request-id",
                "owner-0001",
            ],
            vec!["backlog", "ls", "--project", "acme/repo"],
            vec!["msg", "threads"],
            vec!["msg", "send", "project:no-such-project", "hello"],
        ];
        commands
            .iter()
            .map(|args| code(dispatch_endpoint(&url, OWNER, args.iter())))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap();
    server.abort();

    let mut calls = calls.into_iter();

    let sent = ok(&mut calls, "msg send to a project");
    assert!(
        sent.contains("waiting  no live recipient yet; it is delivered when one appears"),
        "{sent}"
    );

    let again: Value = json(&ok(&mut calls, "msg send to a project again"));
    assert!(flag(&again, "duplicate"), "{again}");
    assert!(flag(&again, "waiting"), "{again}");
    let note_id = again["noteId"].as_str().unwrap();
    assert!(!note_id.is_empty(), "{again}");
    assert_eq!(again["message"]["noteId"], note_id, "{again}");

    let notes = ok(&mut calls, "backlog ls for the project");
    assert!(notes.contains("owner asks: Inspect the build"), "{notes}");
    assert!(notes.contains(note_id), "{notes}");

    let threads = ok(&mut calls, "msg threads");
    assert!(threads.contains("1 msgs"), "{threads}");
    assert!(threads.contains("1 open-acks"), "{threads}");
    assert!(threads.contains("owner"), "{threads}");

    assert_eq!(fail(&mut calls, "send to no project"), Code::NotFound);
}

#[test]
fn msg_usage_lists_the_mailbox_commands() {
    let help = protectorate_cli::run(["--help"]).unwrap();
    for line in [
        "msg send <to> <body>",
        "msg inbox",
        "msg reply <id> <body>",
        "msg ack <id>",
        "msg thread <thread-id>",
        "msg threads",
        "msg wait <message-id|thread>",
    ] {
        assert!(help.contains(line), "{help}");
    }
}

#[test]
fn msg_wait_extends_the_transport_past_its_own_deadline() {
    let invocation = parse(["msg", "wait", "m1", "--timeout", "1h30m"]).unwrap();
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(5405));

    let invocation = parse(["msg", "wait", "m1"]).unwrap();
    assert!(
        matches!(invocation.command, Command::MsgWait { timeout, .. } if timeout == Duration::from_secs(600)),
        "{:?}",
        invocation.command
    );
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(605));

    let invocation = parse(["--timeout", "30", "msg", "inbox"]).unwrap();
    assert_eq!(transport_timeout(&invocation), Duration::from_secs(30));

    assert_eq!(
        parse(["msg", "send", "bob", "hi", "--kind", "notice"])
            .unwrap()
            .command,
        Command::MsgSend {
            to: "bob".into(),
            body: "hi".into(),
            thread: String::new(),
            kind: MessageKind::Notice,
            ack: None,
            request_id: String::new(),
        }
    );
}
