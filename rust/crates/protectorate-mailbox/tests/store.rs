#[allow(dead_code)]
mod common;

use common::*;
use protectorate_mailbox::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

fn strs(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

fn ids(ms: &[Message]) -> Vec<String> {
    ms.iter().map(|m| m.id.clone()).collect()
}

fn note(from: &str, body: &str) -> NewMessage {
    NewMessage {
        from: from.to_string(),
        body: body.to_string(),
        ..Default::default()
    }
}

#[test]
fn assigns_ids_and_threads() {
    let path = tmp_path("ids");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let a = s.send(note("alice", "a")).unwrap();
    let b = s.send(note("alice", "b")).unwrap();
    assert_eq!(
        (a.message.id.as_str(), a.message.thread.as_str()),
        ("m1", "t1")
    );
    assert_eq!(
        (b.message.id.as_str(), b.message.thread.as_str()),
        ("m2", "t2")
    );
    let c = s
        .send(NewMessage {
            from: "alice".into(),
            body: "reply".into(),
            thread: "t1".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        (c.message.id.as_str(), c.message.thread.as_str()),
        ("m3", "t1")
    );
    s.close().unwrap();

    let s = Store::open_with_clock(&path, fixed_clock(at(2))).unwrap();
    let d = s.send(note("alice", "after replay")).unwrap();
    assert_eq!(
        (d.message.id.as_str(), d.message.thread.as_str()),
        ("m4", "t3")
    );
}

#[test]
fn request_ids_are_idempotent_per_sender() {
    let path = tmp_path("request_id");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let first = s
        .send(NewMessage {
            from: "alice".into(),
            body: "first".into(),
            request_id: "r1".into(),
            ..Default::default()
        })
        .unwrap();
    let retry = s
        .send(NewMessage {
            from: "alice".into(),
            body: "second".into(),
            request_id: "r1".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(!retry.created);
    assert_eq!(retry.message.id, first.message.id);
    assert_eq!(retry.message.body, "first");

    let other = s
        .send(NewMessage {
            from: "bob".into(),
            body: "same id, other sender".into(),
            request_id: "r1".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(other.created);
    assert_eq!(other.message.id, "m2");
    s.close().unwrap();

    let s = Store::open_with_clock(&path, fixed_clock(at(2))).unwrap();
    let after_replay = s
        .send(NewMessage {
            from: "alice".into(),
            body: "still the same".into(),
            request_id: "r1".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(!after_replay.created);
    assert_eq!(after_replay.message.id, "m1");
    assert_eq!(read_lines(&path).len(), 2);
}

#[test]
fn cuts_a_partial_last_line() {
    let path = tmp_path("partial");
    {
        let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
        s.send(note("alice", "kept")).unwrap();
        s.close().unwrap();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        use std::io::Write;
        let partial = b"{\"op\":\"ack\",\"at\":\"2026-10-19T12:00:05Z";
        f.write_all(partial).unwrap();
        f.flush().unwrap();
    }
    let before = std::fs::metadata(&path).unwrap().len();
    let s = Store::open_with_clock(&path, fixed_clock(at(2))).unwrap();
    let after = std::fs::metadata(&path).unwrap().len();
    assert_eq!(after + 38, before);
    assert_eq!(s.get("m1").unwrap().body, "kept");
    s.send(note("alice", "appended")).unwrap();
    s.close().unwrap();
    let lines = read_lines(&path);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("appended"));
}

#[test]
fn reports_a_corrupt_line() {
    let path = tmp_path("corrupt");
    std::fs::write(
        &path,
        b"{\"op\":\"send\",\"at\":\"2026-10-19T12:00:01Z\",\"msg\":{\"id\":\"m1\",\"thread\":\"t1\",\"from\":\"a\",\"to\":\"b\",\"kind\":\"note\",\"body\":\"ok\",\"created\":\"2026-10-19T12:00:01Z\"}}\n{\"op\":\"ack\",\"at\":oops}\n",
    )
    .unwrap();
    let err = match Store::open(&path) {
        Ok(_) => panic!("a corrupt log must not open"),
        Err(e) => e,
    };
    match err {
        Error::Corrupt {
            line, ref message, ..
        } => {
            assert_eq!(line, 2);
            assert!(!message.is_empty());
        }
        other => panic!("expected a corrupt error, got {:?}", other),
    }
    assert!(err.to_string().starts_with("mailbox "));
}

#[test]
fn rejects_a_send_record_without_a_message() {
    let path = tmp_path("no_msg");
    std::fs::write(
        &path,
        b"{\"op\":\"send\",\"at\":\"2026-10-19T12:00:01Z\"}\n",
    )
    .unwrap();
    let err = match Store::open(&path) {
        Ok(_) => panic!("a corrupt log must not open"),
        Err(e) => e,
    };
    assert!(matches!(err, Error::Corrupt { line: 1, .. }));
}

#[test]
fn resolve_and_arm_are_idempotent() {
    let path = tmp_path("resolve");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let m = s.send(note("alice", "queued")).unwrap();
    s.resolve(&m.message.id, &strs(&["bob", "carol"]), &strs(&["carol"]))
        .unwrap();
    s.resolve(&m.message.id, &strs(&["dave"]), &strs(&["dave"]))
        .unwrap();
    let got = s.get(&m.message.id).unwrap();
    assert_eq!(got.recipients, strs(&["bob", "carol"]));
    assert_eq!(got.deliveries["carol"].since, Some(at(1)));
    assert_eq!(got.deliveries["bob"], Delivery::default());
    s.arm(&m.message.id, "carol").unwrap();
    s.arm(&m.message.id, "bob").unwrap();
    assert_eq!(
        s.get(&m.message.id).unwrap().deliveries["bob"].since,
        Some(at(1))
    );
    assert_eq!(read_lines(&path).len(), 4);

    s.deliver(&m.message.id, "bob").unwrap();
    let after = s.get(&m.message.id).unwrap();
    assert_eq!(after.deliveries["bob"].delivered, Some(at(1)));
    assert_eq!(after.deliveries["bob"].since, Some(at(1)));
    assert!(!after.unread("bob"));

    s.arm(&m.message.id, "nobody").unwrap_err();
    s.arm("m9", "bob").unwrap_err();
}

#[test]
fn inbox_filters_and_marks_read() {
    let path = tmp_path("inbox");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let a = s
        .send(NewMessage {
            from: "alice".into(),
            body: "one".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .unwrap();
    let b = s
        .send(NewMessage {
            from: "alice".into(),
            body: "two".into(),
            thread: a.message.thread.clone(),
            recipients: strs(&["bob", "carol"]),
            ..Default::default()
        })
        .unwrap();
    s.send(NewMessage {
        from: "alice".into(),
        body: "other thread".into(),
        recipients: strs(&["bob"]),
        ..Default::default()
    })
    .unwrap();

    let peek = s.inbox("bob", "", true, true).unwrap();
    assert_eq!(ids(&peek), strs(&["m1", "m2", "m3"]));
    assert!(peek.iter().all(|m| m.deliveries["bob"].read.is_none()));

    let only_thread = s.inbox("bob", "t1", false, true).unwrap();
    assert_eq!(ids(&only_thread), strs(&["m1", "m2"]));

    let read = s.inbox("bob", "", true, false).unwrap();
    assert_eq!(ids(&read), strs(&["m1", "m2", "m3"]));
    assert!(read.iter().all(|m| m.deliveries["bob"].read.is_none()));
    assert!(s
        .get("m1")
        .unwrap()
        .deliveries
        .values()
        .all(|d| d.read == Some(at(1))));
    assert!(s.inbox("bob", "", true, false).unwrap().is_empty());
    assert_eq!(s.inbox("carol", "", true, false).unwrap().len(), 1);
    assert!(s.inbox("dave", "", true, false).unwrap().is_empty());
    assert_eq!(
        ids(&s.inbox("bob", "", false, true).unwrap()),
        strs(&["m1", "m2", "m3"])
    );
    assert_eq!(b.message.id, "m2");
}

#[test]
fn ack_and_escalate_are_once_only() {
    let path = tmp_path("ack");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let m = s
        .send(NewMessage {
            from: "alice".into(),
            body: "ack me".into(),
            ack_required: true,
            recipients: strs(&["bob", "carol"]),
            ..Default::default()
        })
        .unwrap();
    let id = m.message.id.clone();
    assert!(m.message.open());
    let (got, changed) = s.ack(&id, "bob").unwrap();
    assert!(changed);
    assert!(!got.open());
    let (_, changed) = s.ack(&id, "bob").unwrap();
    assert!(!changed);
    assert!(s.ack(&id, "zoe").is_err());
    s.escalate(&id).unwrap();
    s.escalate(&id).unwrap();
    assert_eq!(s.get(&id).unwrap().escalated, Some(at(1)));
    assert_eq!(read_lines(&path).len(), 3);
    assert_eq!(s.threads("bob")[0].escalated, 0);
}

#[test]
fn overdue_reminds_then_escalates_once_per_message() {
    let path = tmp_path("overdue");
    let clock = Clock::new(0);
    let s = Store::open_with_clock(&path, clock.clock()).unwrap();
    let a = s
        .send(NewMessage {
            from: "alice".into(),
            body: "first".into(),
            ack_required: true,
            recipients: strs(&["bob", "carol"]),
            ..Default::default()
        })
        .unwrap();
    let b = s
        .send(NewMessage {
            from: "alice".into(),
            body: "no ack needed".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .unwrap();
    s.arm(&a.message.id, "bob").unwrap();
    s.arm(&a.message.id, "carol").unwrap();
    s.close().unwrap();
    assert!(!b.message.ack_required);

    let later = Store::open_with_clock(&path, clock.clock()).unwrap();
    let due = later.overdue(Duration::from_secs(5));
    assert!(due.is_empty(), "nothing is overdue yet");

    clock.set(10);
    let due = later.overdue(Duration::from_secs(5));
    assert_eq!(
        due.iter()
            .map(|d| format!("{} {} {}", d.id, d.rcpt, d.action.as_str()))
            .collect::<Vec<_>>(),
        strs(&["m1 bob remind", "m1 carol remind"])
    );
    later.remind(&a.message.id, "bob").unwrap();
    clock.set(20);
    let due = later.overdue(Duration::from_secs(5));
    assert_eq!(
        due.iter()
            .map(|d| format!("{} {} {}", d.id, d.rcpt, d.action.as_str()))
            .collect::<Vec<_>>(),
        strs(&["m1 bob escalate", "m1 carol remind"])
    );
    later.remind(&a.message.id, "carol").unwrap();
    let due = later.overdue(Duration::from_secs(5));
    assert_eq!(
        due.iter()
            .map(|d| format!("{} {} {}", d.id, d.rcpt, d.action.as_str()))
            .collect::<Vec<_>>(),
        strs(&["m1 bob escalate"])
    );
    later.escalate(&a.message.id).unwrap();
    assert!(later.overdue(Duration::from_secs(5)).is_empty());
}

#[test]
fn overdue_ignores_acknowledged_and_unarmed_deliveries() {
    let path = tmp_path("overdue_skips");
    let s = Store::open_with_clock(&path, fixed_clock(at(10))).unwrap();
    let m = s
        .send(NewMessage {
            from: "alice".into(),
            body: "mixed".into(),
            ack_required: true,
            recipients: strs(&["bob", "carol"]),
            ..Default::default()
        })
        .unwrap();
    s.arm(&m.message.id, "bob").unwrap();
    s.ack(&m.message.id, "bob").unwrap();
    let due = s.overdue(Duration::from_secs(1));
    assert!(due.is_empty());
}

#[test]
fn wait_finds_replies_and_acked_outgoing_mail() {
    let path = tmp_path("wait");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let m = s
        .send(NewMessage {
            from: "alice".into(),
            to: "bob".into(),
            body: "question".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .unwrap();
    let err = s
        .wait("m1", "alice", Some(Duration::from_millis(50)))
        .unwrap_err();
    assert!(matches!(err, Error::Timeout { .. }));
    assert_eq!(m.message.id, "m1");
    s.close().unwrap();

    let store = Arc::new(Store::open_with_clock(&path, fixed_clock(at(2))).unwrap());
    let writer = {
        let store = store.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            store
                .send(NewMessage {
                    from: "bob".into(),
                    to: "alice".into(),
                    body: "answer".into(),
                    thread: "t1".into(),
                    reply_to: "m1".into(),
                    recipients: strs(&["alice"]),
                    ..Default::default()
                })
                .unwrap();
        })
    };
    let reply = store
        .wait("m1", "alice", Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(reply.id, "m2");
    assert_eq!(reply.body, "answer");
    writer.join().unwrap();

    let (_, changed) = store.ack("m2", "alice").unwrap();
    assert!(changed);
    let again = store
        .wait("m1", "alice", Some(Duration::from_millis(50)))
        .unwrap();
    assert_eq!(again.id, "m2");
}

#[test]
fn wait_returns_an_already_recorded_ack() {
    let path = tmp_path("wait_ack");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    s.send(NewMessage {
        from: "alice".into(),
        body: "needs an ack".into(),
        ack_required: true,
        recipients: strs(&["bob"]),
        ..Default::default()
    })
    .unwrap();
    s.ack("m1", "bob").unwrap();
    let got = s
        .wait("m1", "alice", Some(Duration::from_millis(50)))
        .unwrap();
    assert_eq!(got.id, "m1");
}

#[test]
fn cancelled_long_wait_releases_worker_and_preserves_replayed_reply() {
    let path = tmp_path("wait_cancel_restart");
    let store = Arc::new(Store::open_with_clock(&path, fixed_clock(at(1))).unwrap());
    store
        .send(NewMessage {
            from: "alice".into(),
            body: "question".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .unwrap();

    let cancelled = Arc::new(AtomicBool::new(false));
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker_store = Arc::clone(&store);
    let worker_flag = Arc::clone(&cancelled);
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        done_tx
            .send(worker_store.wait_with_cancel(
                "m1",
                "alice",
                Some(Duration::from_secs(3600)),
                &worker_flag,
            ))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        Err(Error::Cancelled)
    ));
    worker.join().unwrap();

    store
        .send(NewMessage {
            from: "bob".into(),
            body: "answer".into(),
            thread: "t1".into(),
            reply_to: "m1".into(),
            recipients: strs(&["alice"]),
            ..Default::default()
        })
        .unwrap();
    store.close().unwrap();
    drop(store);
    let reopened = Store::open_with_clock(&path, fixed_clock(at(2))).unwrap();
    assert_eq!(
        reopened
            .wait("m1", "alice", Some(Duration::from_millis(50)))
            .unwrap()
            .body,
        "answer"
    );
}

#[test]
fn sends_are_concurrent() {
    let path = tmp_path("concurrent");
    let store = Arc::new(Store::open_with_clock(&path, fixed_clock(at(1))).unwrap());
    let mut threads = Vec::new();
    for i in 0..8 {
        let store = store.clone();
        threads.push(std::thread::spawn(move || {
            for k in 0..5 {
                store
                    .send(NewMessage {
                        from: format!("sender{}", i),
                        body: format!("message {} {}", i, k),
                        request_id: format!("r{}-{}", i, k),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }));
    }
    for t in threads {
        t.join().unwrap();
    }
    let lines = read_lines(&path);
    assert_eq!(lines.len(), 40);
    let mut seen: Vec<String> = Vec::new();
    for i in 1..=40 {
        let id = format!("m{}", i);
        assert!(store.get(&id).is_some(), "missing {}", id);
        seen.push(id);
    }
    assert_eq!(seen.len(), 40);
    assert_eq!(store.threads("").len(), 40);
}

#[test]
fn a_closed_store_refuses_writes() {
    let path = tmp_path("closed");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    s.send(note("alice", "one")).unwrap();
    s.close().unwrap();
    s.close().unwrap();
    let err = s.send(note("alice", "two")).unwrap_err();
    assert!(matches!(err, Error::Closed));
    assert_eq!(read_lines(&path).len(), 1);
    assert_eq!(s.get("m1").unwrap().body, "one");
}

#[test]
fn ignores_unknown_fields_and_ops() {
    let path = tmp_path("unknown");
    std::fs::write(
        &path,
        concat!(
            "{\"op\":\"send\",\"at\":\"2026-10-19T12:00:01Z\",\"extra\":1,\"msg\":{\"id\":\"m1\",\"thread\":\"t1\",\"from\":\"a\",\"to\":\"b\",\"kind\":\"custom\",\"body\":\"hi\",\"created\":\"2026-10-19T12:00:01Z\",\"recipients\":[\"b\"],\"deliveries\":{\"b\":{\"since\":\"0001-01-01T00:00:00Z\"}},\"whatever\":null}}\n",
            "{\"op\":\"nonsense\",\"at\":\"2026-10-19T12:00:02Z\",\"id\":\"m1\",\"rcpt\":\"b\"}\n",
            "{\"op\":\"ack\",\"at\":\"2026-10-19T12:00:03Z\",\"id\":\"gone\",\"rcpt\":\"b\"}\n",
        ),
    )
    .unwrap();
    let s = Store::open(&path).unwrap();
    let m = s.get("m1").unwrap();
    assert_eq!(m.kind, Kind::new("custom"));
    assert_eq!(m.deliveries["b"], Delivery::default());
    assert!(m.open());
    assert_eq!(read_lines(&path).len(), 3);
}

#[test]
fn reads_timestamps_written_with_an_offset() {
    let path = tmp_path("offset");
    std::fs::write(
        &path,
        "{\"op\":\"send\",\"at\":\"2026-10-19T14:34:56.5+02:00\",\"msg\":{\"id\":\"m1\",\"thread\":\"t1\",\"from\":\"a\",\"to\":\"b\",\"kind\":\"note\",\"body\":\"hi\",\"created\":\"2026-10-19T14:34:56.5+02:00\",\"recipients\":[\"b\"],\"deliveries\":{\"b\":{\"since\":\"2026-10-19T14:34:56.500000000+02:00\"}}}}\n",
    )
    .unwrap();
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let m = s.get("m1").unwrap();
    assert_eq!(m.created, Time::parse("2026-10-19T12:34:56.5Z").unwrap());
    // Like Go, a send record rebuilds the deliveries from the recipients, so
    // the since written into the record is not kept.
    assert_eq!(m.deliveries["b"], Delivery::default());
    s.arm("m1", "b").unwrap();
    assert_eq!(s.get("m1").unwrap().deliveries["b"].since, Some(at(1)));
    let line = read_lines(&path).pop().unwrap();
    assert!(line.contains("\"at\":\"2026-10-19T12:00:01Z\""), "{}", line);
}

#[test]
fn escapes_bodies_the_way_go_does() {
    let path = tmp_path("escaping");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    s.send(note("a", "a <b> & \"c\"\t\ndel:\u{7f} e\u{1} é \u{2028}x"))
        .unwrap();
    let line = read_lines(&path).pop().unwrap();
    let want = format!(
        "{{\"op\":\"send\",\"at\":\"{}\",\"msg\":{{\"id\":\"m1\",\"thread\":\"t1\",\"from\":\"a\",\"to\":\"\",\"kind\":\"note\",\"body\":\"a \\u003cb\\u003e \\u0026 \\\"c\\\"\\t\\ndel:\u{7f} e\\u0001 é \\u2028x\",\"created\":\"{}\"}}}}",
        at(1).format(),
        at(1).format()
    );
    assert_eq!(line, want);
}

#[test]
fn pending_and_unresolved_follow_delivery_state() {
    let path = tmp_path("pending");
    let s = Store::open_with_clock(&path, fixed_clock(at(1))).unwrap();
    let open = s
        .send(NewMessage {
            from: "alice".into(),
            body: "no recipients yet".into(),
            project: "p1".into(),
            ..Default::default()
        })
        .unwrap();
    let armed = s
        .send(NewMessage {
            from: "alice".into(),
            body: "armed".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&s.unresolved()), strs(&["m1"]));
    s.resolve(&open.message.id, &strs(&["erin"]), &strs(&["erin"]))
        .unwrap();
    s.arm(&armed.message.id, "bob").unwrap();
    s.deliver(&armed.message.id, "bob").unwrap();
    s.inbox("bob", "", false, false).unwrap();

    assert!(s.unresolved().is_empty());
    let pending = s.pending();
    assert_eq!(pending.keys().cloned().collect::<Vec<_>>(), strs(&["erin"]));

    let threads = s.threads("bob");
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].id, "t2");
    assert_eq!(threads[0].participants, strs(&["alice", "bob"]));
    assert!(threads[0].projects.is_empty());
    assert_eq!(threads[1].id, "t1");
    assert_eq!(threads[1].participants, strs(&["alice", "erin"]));
    assert_eq!(threads[1].projects, strs(&["p1"]));
}
