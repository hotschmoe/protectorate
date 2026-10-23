use protectorate_audit::*;
use std::{
    fs,
    io::Write,
    sync::{Arc, Mutex},
};
fn entry(time: &str, actor: &str, subject: &str, outcome: &str) -> Entry {
    Entry {
        time: time.parse().unwrap(),
        actor: actor.into(),
        subject: subject.into(),
        procedure: "/protectorate.v1.JobService/Run".into(),
        outcome: outcome.into(),
        token_id: "token-id".into(),
        tailnet: "node.example".into(),
        code: if outcome == OUTCOME_ERROR {
            "internal"
        } else {
            ""
        }
        .into(),
        task: "compile app".into(),
        detail: "linux sandbox".into(),
    }
}
#[test]
fn rotation_pagination_and_field_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let audit = Audit::new(dir.path());
    let a = entry("2026-09-30T23:59:59Z", "alice", "one", OUTCOME_OK);
    let b = entry(
        "2026-10-01T00:00:00.123456789Z",
        "bob",
        "two",
        OUTCOME_DENIED,
    );
    let c = entry("2026-10-02T00:00:00Z", "alice", "three", OUTCOME_ERROR);
    let d = entry("2026-10-03T00:00:00Z", "alice", "four", OUTCOME_OK);
    for e in [&a, &b, &c, &d] {
        audit.try_write(e).unwrap()
    }
    assert!(dir.path().join("audit-2026-09.jsonl").exists());
    assert!(dir.path().join("audit-2026-10.jsonl").exists());
    let first = audit
        .page(Query {
            limit: 2,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.entries, [d.clone(), c.clone()]);
    assert!(!first.next.is_empty());
    let second = audit
        .page(Query {
            limit: 2,
            before: first.next,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second.entries, [b, a]);
    assert!(second.next.is_empty());
    assert_eq!(audit.tail(1).unwrap(), [d]);
    audit.close().unwrap();
    audit.close().unwrap();
    audit.try_write(&c).unwrap();
    assert_eq!(audit.tail(1).unwrap(), [c]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.path().join("audit-2026-10.jsonl"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn filtering_spans_months_and_uses_exact_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let audit = Audit::new(dir.path());
    for (time, actor, subject, outcome) in [
        ("2026-09-01T00:00:00Z", "ALICE", "Project-A", OUTCOME_OK),
        ("2026-10-01T00:00:00Z", "alice", "project-a", OUTCOME_DENIED),
        ("2026-10-02T00:00:00Z", "bob", "other", OUTCOME_OK),
    ] {
        audit
            .try_write(&entry(time, actor, subject, outcome))
            .unwrap()
    }
    let q = Query {
        actor: "ali".into(),
        procedure: "jobservice".into(),
        subject: "PROJECT-A".into(),
        outcome: OUTCOME_OK.into(),
        ..Default::default()
    };
    let p = audit.page(q).unwrap();
    assert_eq!(p.entries.len(), 1);
    assert_eq!(p.entries[0].actor, "ALICE");
    assert!(p.next.is_empty());
    assert!(audit
        .page(Query {
            outcome: "OK".into(),
            ..Default::default()
        })
        .unwrap()
        .entries
        .is_empty());
}
#[test]
fn partial_and_malformed_lines_do_not_hide_complete_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit-2026-10.jsonl");
    let e = entry("2026-10-01T00:00:00Z", "a", "first", OUTCOME_OK);
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(&marshal_entry(&e).unwrap()).unwrap();
    file.write_all(b"\nbad JSON\n{\"time\":").unwrap();
    let before = fs::read(&path).unwrap();
    let p = read_page(dir.path(), Query::default()).unwrap();
    assert_eq!(p.entries, [e]);
    assert_eq!(fs::read(path).unwrap(), before);
}
#[test]
fn invalid_cursors_and_io_failures_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let audit = Audit::new(dir.path());
    audit
        .try_write(&entry("2026-10-01T00:00:00Z", "a", "x", OUTCOME_OK))
        .unwrap();
    for before in [
        "unknown:0",
        "../audit-2026-10.jsonl:0",
        "audit-2026-10.jsonl:-1",
        "audit-2026-10.jsonl:no",
        "audit-2026-10.jsonl:0:0",
        "audit-2026-10.jsonl:",
    ] {
        assert!(
            matches!(
                audit.page(Query {
                    before: before.into(),
                    ..Default::default()
                }),
                Err(Error::Cursor(_))
            ),
            "{before}"
        )
    }
    assert!(audit
        .page(Query {
            limit: 0,
            before: "bad".into(),
            ..Default::default()
        })
        .unwrap()
        .entries
        .is_empty());
    assert!(read_page(dir.path().join("missing"), Query::default())
        .unwrap()
        .entries
        .is_empty());
    let file = dir.path().join("regular-file");
    fs::write(&file, b"x").unwrap();
    assert!(matches!(
        read_page(&file, Query::default()),
        Err(Error::Io(_))
    ));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let seen = failures.clone();
    let broken = Audit::new(file.join("bad"))
        .with_on_error(move |e| seen.lock().unwrap().push(e.to_string()));
    broken.write(Entry::default());
    assert_eq!(failures.lock().unwrap().len(), 1);
    assert!(broken.try_write(&Entry::default()).is_err());
}
#[test]
fn concurrent_appends_remain_complete_and_ordered() {
    let dir = tempfile::tempdir().unwrap();
    let audit = Arc::new(Audit::new(dir.path()));
    std::thread::scope(|scope| {
        for actor in 0..8 {
            let audit = audit.clone();
            scope.spawn(move || {
                for n in 0..25 {
                    audit
                        .try_write(&entry(
                            "2026-10-01T00:00:00Z",
                            &format!("worker-{actor}"),
                            &n.to_string(),
                            OUTCOME_OK,
                        ))
                        .unwrap()
                }
            });
        }
    });
    let p = audit
        .page(Query {
            limit: 1000,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(p.entries.len(), 200);
    for actor in 0..8 {
        let subjects: Vec<_> = p
            .entries
            .iter()
            .filter(|e| e.actor == format!("worker-{actor}"))
            .map(|e| e.subject.parse::<usize>().unwrap())
            .collect();
        assert_eq!(subjects, (0..25).rev().collect::<Vec<_>>());
    }
}
#[test]
fn sparse_queries_continue_at_scan_budget() {
    let dir = tempfile::tempdir().unwrap();
    let audit = Audit::new(dir.path());
    let old = entry("2026-09-01T00:00:00Z", "wanted", "found", OUTCOME_OK);
    audit.try_write(&old).unwrap();
    fs::write(
        dir.path().join("audit-2026-10.jsonl"),
        b"bad\n".repeat(PAGE_SCAN_BUDGET),
    )
    .unwrap();
    let first = audit
        .page(Query {
            actor: "wanted".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(first.entries.is_empty());
    assert_eq!(first.next, "audit-2026-09.jsonl:*");
    let second = audit
        .page(Query {
            actor: "wanted".into(),
            before: first.next,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second.entries, [old]);
    assert!(second.next.is_empty());
}
