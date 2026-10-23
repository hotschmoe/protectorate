#[allow(dead_code)]
mod common;

use common::*;
use protectorate_mailbox::*;
use std::time::Duration;

fn strs(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// Replays the same script the Go store ran to produce tests/fixtures/go_mailbox.jsonl
/// and compares the resulting log byte for byte.
#[test]
fn writes_the_same_log_as_go() {
    let path = tmp_path("go_compat");
    let ticker = Ticker::new(T12);
    let s = Store::open_with_clock(&path, ticker.clock()).expect("open");

    let m1 = s
        .send(NewMessage {
            from: "alice".into(),
            to: "bob".into(),
            kind: Kind::REQUEST,
            ack_required: true,
            body: "fix <the> bug & ship \"it\"\t\ntoday".into(),
            request_id: "r1".into(),
            from_project: "p1".into(),
            project: "p2".into(),
            recipients: strs(&["bob", "carol"]),
            ..Default::default()
        })
        .expect("send m1");
    assert!(m1.created);
    assert_eq!(m1.message.id, "m1");
    assert_eq!(m1.message.thread, "t1");

    let retry = s
        .send(NewMessage {
            from: "alice".into(),
            to: "bob".into(),
            kind: Kind::REQUEST,
            ack_required: true,
            body: "duplicate".into(),
            request_id: "r1".into(),
            recipients: strs(&["bob"]),
            ..Default::default()
        })
        .expect("retry");
    assert!(!retry.created);
    assert_eq!(retry.message.id, "m1");
    assert_eq!(retry.message.body, m1.message.body);

    let m2 = s
        .send(NewMessage {
            from: "bob".into(),
            to: "alice".into(),
            kind: Kind::NOTICE,
            body: "done <all>".into(),
            thread: "t1".into(),
            reply_to: "m1".into(),
            footer: Some(Box::new(|t| format!("thread {}", t))),
            ..Default::default()
        })
        .expect("send m2");
    assert_eq!(m2.message.id, "m2");
    assert_eq!(m2.message.body, "done <all>\n\nthread t1");

    let m3 = s
        .send(NewMessage {
            from: "carol".into(),
            to: "proj:p2".into(),
            kind: Kind::REQUEST,
            ack_required: true,
            body: "queued".into(),
            project: "p2".into(),
            ..Default::default()
        })
        .expect("send m3");
    assert_eq!(m3.message.thread, "t2");
    assert!(m3.message.recipients.is_empty());

    s.resolve("m3", &strs(&["dave", "erin"]), &strs(&["erin"]))
        .expect("resolve");
    let unread = s.inbox("bob", "", true, false).expect("inbox");
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].id, "m1");
    assert!(unread[0].deliveries["bob"].read.is_none());
    let peeked = s.inbox("bob", "", false, true).expect("inbox peek");
    assert_eq!(peeked.len(), 1);
    assert!(peeked[0].deliveries["bob"].read.is_some());
    s.deliver("m1", "bob").expect("deliver");
    s.arm("m1", "carol").expect("arm");
    let (acked, changed) = s.ack("m1", "bob").expect("ack");
    assert!(changed);
    assert!(acked.deliveries["bob"].acked.is_some());
    let (_, changed) = s.ack("m1", "bob").expect("re-ack");
    assert!(!changed);
    s.remind("m1", "carol").expect("remind");
    s.escalate("m1").expect("escalate");
    s.escalate("m1").expect("escalate again");
    s.set_note_id("m3", "note-7").expect("note");
    s.close().expect("close");

    assert_eq!(ticker.calls(), 12);
    let got = std::fs::read(&path).expect("read log");
    let want = std::fs::read(fixture("go_mailbox.jsonl")).expect("read fixture");
    assert_eq!(
        String::from_utf8_lossy(&got),
        String::from_utf8_lossy(&want)
    );
}

#[test]
fn replays_the_go_log() {
    let path = copy_fixture("go_mailbox.jsonl", "replay");
    let s = Store::open_with_clock(&path, fixed_clock(at(300))).expect("open");

    let m1 = s.get("m1").expect("m1");
    assert_eq!(m1.thread, "t1");
    assert_eq!(m1.kind, Kind::REQUEST);
    assert!(m1.ack_required);
    assert_eq!(m1.body, "fix <the> bug & ship \"it\"\t\ntoday");
    assert_eq!(m1.created, at(1));
    assert_eq!(m1.recipients, strs(&["bob", "carol"]));
    let bob = &m1.deliveries["bob"];
    assert_eq!(bob.since, Some(at(7)));
    assert_eq!(bob.delivered, Some(at(7)));
    assert_eq!(bob.read, Some(at(6)));
    assert_eq!(bob.acked, Some(at(9)));
    assert_eq!(bob.reminders, 0);
    let carol = &m1.deliveries["carol"];
    assert_eq!(carol.since, Some(at(10)));
    assert_eq!(carol.delivered, Some(at(10)));
    assert_eq!(carol.read, None);
    assert_eq!(carol.acked, None);
    assert_eq!(carol.reminders, 1);
    assert_eq!(m1.escalated, Some(at(11)));
    assert!(!m1.open());
    assert!(!m1.unread("carol"));

    let m2 = s.get("m2").expect("m2");
    assert_eq!(m2.body, "done <all>\n\nthread t1");
    assert_eq!(m2.reply_to, "m1");
    assert!(m2.deliveries.is_empty());

    let m3 = s.get("m3").expect("m3");
    assert_eq!(m3.recipients, strs(&["dave", "erin"]));
    assert_eq!(m3.deliveries["dave"], Delivery::default());
    assert_eq!(m3.deliveries["erin"].since, Some(at(5)));
    assert_eq!(m3.note_id, "note-7");
    assert!(m3.open());

    assert_eq!(s.thread("t1").len(), 2);
    assert_eq!(s.thread("t2").len(), 1);
    assert_eq!(s.thread("nope").len(), 0);

    let due = s.overdue(Duration::from_secs(1));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "m3");
    assert_eq!(due[0].rcpt, "erin");
    assert_eq!(due[0].action, Action::Remind);
    assert_eq!(due[0].action.as_str(), "remind");

    let pending = s.pending();
    assert_eq!(
        pending.keys().cloned().collect::<Vec<_>>(),
        strs(&["dave", "erin"])
    );
    assert_eq!(pending["dave"][0].id, "m3");

    assert_eq!(
        s.involving("carol")
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        strs(&["m1", "m3"])
    );
    assert!(s.involving("alice").is_empty());
    assert!(s.open_requests("p2", "p1").is_empty());
    assert_eq!(s.open_requests("p2", "").len(), 1);
    assert_eq!(
        s.unresolved()
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        strs(&["m2"])
    );

    let threads = s.threads("bob");
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].id, "t2");
    assert_eq!(threads[0].messages, 1);
    assert_eq!(threads[0].participants, strs(&["carol", "dave", "erin"]));
    assert_eq!(threads[0].projects, strs(&["p2"]));
    assert_eq!(threads[0].unread, 0);
    assert_eq!(threads[0].open_acks, 1);
    assert_eq!(threads[0].escalated, 0);
    assert_eq!(threads[0].last.id, "m3");
    assert_eq!(threads[1].id, "t1");
    assert_eq!(threads[1].messages, 2);
    assert_eq!(threads[1].participants, strs(&["alice", "bob", "carol"]));
    assert_eq!(threads[1].projects, strs(&["p2", "p1"]));
    assert_eq!(threads[1].open_acks, 0);
    assert_eq!(threads[1].escalated, 0);
    assert_eq!(threads[1].last.id, "m2");

    let err = s.deliver("m99", "bob").expect_err("missing message");
    assert_eq!(err.to_string(), "no such message: m99");
    let err = s.ack("m1", "zoe").expect_err("not a recipient");
    assert_eq!(
        err.to_string(),
        "not a recipient of this message: zoe, message m1"
    );
}

/// A log written by Go must survive being opened, extended and reopened here.
#[test]
fn appends_to_a_go_written_log() {
    let path = copy_fixture("go_mailbox.jsonl", "append");
    {
        let s = Store::open_with_clock(&path, fixed_clock(at(400))).expect("open");
        s.arm("m3", "dave").expect("arm");
        s.close().expect("close");
    }
    let lines = read_lines(&path);
    assert_eq!(lines.len(), 13);
    assert_eq!(
        lines[12],
        format!(
            "{{\"op\":\"arm\",\"at\":\"{}\",\"id\":\"m3\",\"rcpt\":\"dave\"}}",
            at(400).format()
        )
    );
    let s = Store::open_with_clock(&path, fixed_clock(at(400))).expect("reopen");
    assert_eq!(
        s.get("m3").expect("m3").deliveries["dave"].since,
        Some(at(400))
    );
}
