use crate::{fields, text};
use protectorate_backlog::{Filter, NewNote, Patch};
use protectorate_chatlog::{read_all, Event, Log};
use protectorate_directory::{normalize_remote, read_manifests, Directory, RefState};
use protectorate_handoff::parse_transcript;
use protectorate_mailbox::{Message, NewMessage, Store as Mailbox};
use std::{fs, time::Duration};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().expect("scratch directory")
}

fn mailbox_state(store: &Mailbox) -> Vec<Option<Message>> {
    let mut ids: Vec<String> = store
        .unresolved()
        .into_iter()
        .chain(store.pending().into_values().flatten())
        .chain(store.inbox("peer", "", false, true).unwrap_or_default())
        .chain(store.pending_for("peer"))
        .map(|m| m.id)
        .collect();
    ids.sort();
    ids.dedup();
    let threads: Vec<_> = store
        .threads("peer")
        .into_iter()
        .map(|t| (t.id, t.messages, t.unread, t.open_acks, t.escalated))
        .collect();
    let mut state: Vec<Option<Message>> = ids.iter().map(|id| store.get(id)).collect();
    for (id, _, _, _, _) in &threads {
        state.extend(store.thread(id).into_iter().map(Some));
    }
    state
}

/// A replayed log accepts a new message, and reopening the log it wrote
/// reproduces the same state.
pub fn mailbox_log(data: &[u8]) {
    let dir = scratch();
    let path = dir.path().join("mailbox.jsonl");
    fs::write(&path, data).unwrap();
    let Ok(store) = Mailbox::open(&path) else {
        return;
    };
    let sent = store
        .send(NewMessage {
            from: "fuzz".into(),
            to: "peer".into(),
            body: "probe".into(),
            ack_required: true,
            recipients: vec!["peer".into()],
            ..NewMessage::default()
        })
        .expect("send to a replayed mailbox");
    assert!(sent.created, "a fresh request id created nothing");
    let _ = store.ack(&sent.message.id, "peer");
    let before = mailbox_state(&store);
    store.close().unwrap();
    let reopened = Mailbox::open(&path).expect("reopen a mailbox log this store wrote");
    assert_eq!(
        reopened.get(&sent.message.id).map(|m| m.body),
        Some("probe".to_string())
    );
    assert_eq!(
        mailbox_state(&reopened),
        before,
        "replay changed the mailbox"
    );
}

/// A log that opens can be extended, and both readers then agree on every
/// event with gap-free sequence numbers.
pub fn chatlog(data: &[u8]) {
    let dir = scratch();
    let path = dir.path().join("chat.jsonl");
    fs::write(&path, data).unwrap();
    let Ok(log) = Log::open(&path) else {
        return;
    };
    let last = log.last();
    log.append_text("fuzz", false, "partial ").unwrap();
    log.append_text("fuzz", false, "reply").unwrap();
    let seq = log
        .append(Event::new(
            serde_json::json!({"state":{"state":"CHAT_STATE_IDLE"}}),
        ))
        .unwrap();
    assert_eq!(
        seq,
        last + 2,
        "the text message and the event each take one seq"
    );
    log.close().unwrap();
    let events = read_all(&path).expect("read a log that opened");
    assert_eq!(events.len() as u64, seq);
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, i as u64 + 1, "sequence gap");
    }
    assert_eq!(
        events[seq as usize - 2].payload["text"]["delta"],
        "partial reply"
    );
    let reopened = Log::open(&path).expect("reopen a log this writer closed");
    assert_eq!(reopened.last(), seq);
}

/// notes.json loads, a new note survives a write, and an update changes only
/// that note.
pub fn backlog_notes(data: &[u8]) {
    let dir = scratch();
    let store = protectorate_backlog::Store::new(dir.path());
    fs::write(store.path(), data).unwrap();
    let all = Filter {
        state: "all".into(),
        ..Filter::default()
    };
    let Ok(before) = store.list(all.clone()) else {
        return;
    };
    for note in &before {
        assert!(!note.state.is_empty(), "a loaded note has no state");
        assert!(
            store.get(&note.id).is_ok(),
            "listed note {:?} not found",
            note.id
        );
    }
    let created = match store.create(NewNote {
        title: "probe".into(),
        body: "fuzz".into(),
        ..NewNote::default()
    }) {
        Ok(note) => note,
        Err(_) => return,
    };
    assert!(!before.iter().any(|n| n.id == created.id), "id reused");
    let (updated, changed) = store
        .update(
            &created.id,
            Patch {
                state: Some("done".into()),
                resolution: Some("fixed".into()),
                done_by: "fuzz".into(),
                ..Patch::default()
            },
        )
        .expect("update the note just created");
    assert!(changed && updated.state == "done");
    let after = store.list(all).expect("list after write");
    assert_eq!(after.len(), before.len() + 1);
    for note in &before {
        let kept = after.iter().filter(|n| n.id == note.id).count();
        assert!(kept >= 1, "write dropped note {:?}", note.id);
    }
}

/// An identity file loads; a capability minted into it verifies after a
/// reopen and after renewal, no other input verifies as it, and a probe
/// device token never panics the store.
pub fn identity_store(data: &[u8]) {
    let [file, probe] = fields(data, 2)[..] else {
        unreachable!()
    };
    let dir = scratch();
    let path = dir.path().join("identity.json");
    fs::write(&path, file).unwrap();
    let Ok(store) = protectorate_identity::Store::open(&path) else {
        return;
    };
    let now = 1_800_000_000;
    let probe = text(probe);
    let _ = store.verify(&probe, now);
    let _ = store.verify_device(&probe, now);
    let before = store.capabilities().len();
    let Ok((record, secret)) = store.mint(
        protectorate_identity::Mint {
            principal: "fuzz",
            scopes: vec!["read".into()],
            task: "",
            minted_by: "owner",
            ttl: Duration::from_secs(3600),
            file: None,
        },
        now,
    ) else {
        return;
    };
    let reopened =
        protectorate_identity::Store::open(&path).expect("reopen a written identity file");
    assert_eq!(reopened.capabilities().len(), before + 1);
    let (found, _) = reopened
        .verify(secret.expose(), now + 1)
        .expect("minted capability after reopen");
    assert_eq!(found.id, record.id);
    let (_, renewed) = reopened
        .renew(secret.expose(), now + 2)
        .expect("renew a live capability");
    assert_eq!(
        reopened.verify(renewed.expose(), now + 3).unwrap().0.id,
        record.id
    );
    if probe != secret.expose() && probe != renewed.expose() {
        assert!(
            reopened
                .verify(&probe, now + 1)
                .map_or(true, |(r, _)| r.id != record.id),
            "another input verified as the new capability"
        );
    }
}

/// directory.json, four manifests and a remote, separated by NUL.
pub fn directory_state(data: &[u8]) {
    let parts = fields(data, 6);
    let dir = scratch();
    let path = dir.path().join("directory.json");
    fs::write(&path, parts[0]).unwrap();
    for (name, src) in [
        ("go.mod", parts[1]),
        ("package.json", parts[2]),
        ("Cargo.toml", parts[3]),
        ("pyproject.toml", parts[4]),
    ] {
        fs::write(dir.path().join(name), src).unwrap();
    }
    for manifest in read_manifests(dir.path()) {
        let prefix = format!("{}:", manifest.kind);
        assert!(
            manifest.deps.windows(2).all(|w| w[0] < w[1]),
            "deps unsorted"
        );
        assert!(manifest.deps.iter().all(|d| d.starts_with(&prefix)));
    }
    let _ = normalize_remote(&text(parts[5]));
    let Ok(directory) = Directory::open(&path) else {
        return;
    };
    let views = directory.list();
    for view in &views {
        let _ = directory.get(&view.project.id);
        let _ = directory.get(&view.project.name);
        let _ = directory.projects_of(&view.project.id);
    }
    let Some(first) = views.first() else {
        return;
    };
    let refs = RefState {
        default: "main".into(),
        default_tip: "0".repeat(40),
        tags: Vec::new(),
    };
    directory
        .set_refs(&first.project.id, refs.clone())
        .expect("persist refs");
    let reopened = Directory::open(&path).expect("reopen a written directory");
    let projects = |d: &Directory| d.list().into_iter().map(|v| v.project).collect::<Vec<_>>();
    assert_eq!(projects(&reopened), projects(&directory));
    let stored = projects(&reopened)
        .into_iter()
        .find(|p| p.id == first.project.id)
        .and_then(|p| p.refs);
    assert_eq!(stored, Some(refs));
}

/// Any audit directory verifies or reports a break without panicking. A
/// write always extends the chain, a chain that verified still verifies
/// afterwards, and then changing any one byte of the log breaks it.
pub fn audit_log(data: &[u8]) {
    let [log, head, flip] = fields(data, 3)[..] else {
        unreachable!()
    };
    let dir = scratch();
    let file = dir.path().join("audit-2026-01.jsonl");
    fs::write(&file, log).unwrap();
    if !head.is_empty() {
        fs::write(dir.path().join("head.json"), head).unwrap();
    }
    let audit = protectorate_audit::Audit::new(dir.path());
    let before = audit.verify();
    let _ = audit.page(protectorate_audit::Query {
        limit: 3,
        ..Default::default()
    });
    let Ok(written) = audit.try_write(protectorate_audit::Entry {
        actor: "fuzz".into(),
        procedure: "probe".into(),
        ..Default::default()
    }) else {
        return;
    };
    let Ok(before) = before else {
        return;
    };
    let after = audit.verify().expect("a write extends a verified chain");
    assert_eq!(after.entries, before.entries + 1);
    assert_eq!(after.head, written);
    assert_eq!(after.beyond_head, 0);
    let target = if fs::metadata(&file).unwrap().len() > 0 {
        file
    } else {
        fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p != &file && p.extension().is_some_and(|x| x == "jsonl"))
            .unwrap()
    };
    let mut bytes = fs::read(&target).unwrap();
    let at = flip
        .iter()
        .fold(0usize, |n, b| n.wrapping_mul(31).wrapping_add(*b as usize))
        % bytes.len();
    bytes[at] ^= flip.first().copied().filter(|b| *b != 0).unwrap_or(1);
    fs::write(&target, &bytes).unwrap();
    assert!(
        audit.verify().is_err(),
        "a changed byte at {at} still verified"
    );
}

/// The first byte picks the harness and whether the read starts mid-file; the
/// rest is a native session transcript. Turns are non-empty, trimmed
/// user/assistant text.
pub fn handoff_transcript(data: &[u8]) {
    let (head, body) = data.split_first().map_or((0, &[][..]), |(h, b)| (*h, b));
    let harness = ["claude", "codex", "grok", "pi"][usize::from(head & 3)];
    let turns = parse_transcript(harness, body, head & 4 != 0).expect("in-memory read cannot fail");
    for turn in turns {
        assert!(turn.role == "user" || turn.role == "assistant");
        assert!(!turn.text.is_empty() && turn.text == turn.text.trim());
    }
}
