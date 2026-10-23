use crate::{fields, text};
use protectorate_backlog::{Filter, NewNote, Patch};
use protectorate_chatlog::{read_all, Event, Log};
use protectorate_directory::{normalize_remote, read_manifests, Directory, RefState};
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

/// A token file loads, a token issued into it is found by its secret after a
/// reopen, and no other input secret matches.
pub fn identity_tokens(data: &[u8]) {
    let [file, probe] = fields(data, 2)[..] else {
        unreachable!()
    };
    let dir = scratch();
    let path = dir.path().join("tokens.json");
    fs::write(&path, file).unwrap();
    let Ok(store) = protectorate_identity::Store::open(&path) else {
        return;
    };
    let now = 1_800_000_000;
    let probe = text(probe);
    let _ = store.lookup(&probe, now);
    let _ = store.poll_device(&probe, now);
    let _ = store.pending_devices(now);
    let before = store.list().len();
    let Ok(issued) = store.create("fuzz", &["read"], Duration::from_secs(3600), now, &[]) else {
        return;
    };
    let reopened = protectorate_identity::Store::open(&path).expect("reopen a written token file");
    assert_eq!(reopened.list().len(), before + 1);
    let found = reopened
        .lookup(issued.secret.expose(), now + 1)
        .expect("issued secret after reopen");
    assert_eq!(found.id, issued.token.id);
    if probe != issued.secret.expose() {
        assert!(
            reopened
                .lookup(&probe, now + 1)
                .map_or(true, |t| t.id != issued.token.id),
            "another secret matched the new token"
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
