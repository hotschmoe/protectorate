use protectorate_lease::*;
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
fn ttl(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn wait(holder: &str, resource: &str, priority: i32) -> Wait {
    Wait {
        holder: holder.into(),
        notify: holder.into(),
        resource: resource.into(),
        request: resource.into(),
        ttl: ttl(300),
        priority,
    }
}
fn active(entries: &[Entry], holder: &str) -> Lease {
    entries
        .iter()
        .find_map(|e| match e {
            Entry::Active(l) if l.holder == holder => Some(l.clone()),
            _ => None,
        })
        .unwrap()
}
#[test]
fn whole_node_gpu_conflicts_and_expiration() {
    let t = LeaseTable::memory();
    let a = t.acquire("alice", ttl(10), "node:gpu0", 0).unwrap();
    let b = t.acquire("bob", ttl(10), "node:gpu1", 0).unwrap();
    assert!(matches!(
        t.acquire("third", ttl(10), "node", 0),
        Err(Error::Busy(_))
    ));
    assert!(matches!(
        t.acquire("third", ttl(10), "node:gpu0", 0),
        Err(Error::Busy(_))
    ));
    assert!(t.node_lease("node", 0).unwrap().is_none());
    assert!(t.acquire("desktop", ttl(10), "desk:node", 0).is_ok());
    assert!(t.acquire("other", ttl(10), "other", 0).is_ok());
    t.release(&a.id, 100).unwrap();
    assert!(t
        .blocked(&parse_resource("node:gpu0").unwrap(), 100)
        .unwrap()
        .is_none());
    assert!(t
        .blocked(&parse_resource("node").unwrap(), 100)
        .unwrap()
        .is_some());
    assert!(matches!(t.get(&b.id, 10_000), Err(Error::NotFound(_))));
    let node = t.acquire("node holder", ttl(10), "node", 10_000).unwrap();
    assert_eq!(t.node_lease("node", 10_000).unwrap().unwrap(), node);
    assert!(t.acquire("a", ttl(10), "node:gpu2", 10_000).is_err());
    assert!(t.acquire("", ttl(1), "empty", 0).is_err());
    assert!(t.acquire("a", Duration::ZERO, "empty", 0).is_err());
    assert!(t.acquire("a", MAX_LEASE_TTL + ttl(1), "empty", 0).is_err());
}
#[test]
fn priority_fifo_claim_windows_and_notifications() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let grants = Arc::new(Mutex::new(Vec::new()));
    let event_seen = events.clone();
    let grant_seen = grants.clone();
    let t = LeaseTable::memory()
        .with_claim_window(ttl(2))
        .unwrap()
        .with_on_event(move |e| event_seen.lock().unwrap().push(e.kind))
        .with_on_grant(move |q, l| {
            grant_seen
                .lock()
                .unwrap()
                .push((q.holder.clone(), l.id.clone()))
        });
    let first = t.acquire("initial", ttl(100), "node", 0).unwrap();
    let low = t.acquire_wait(wait("low", "node", 0), 0).unwrap();
    let high1 = t.acquire_wait(wait("high1", "node", 10), 0).unwrap();
    let high2 = t.acquire_wait(wait("high2", "node", 10), 0).unwrap();
    let all = t.list(0).unwrap();
    let queued: Vec<_> = all
        .iter()
        .filter_map(|e| match e {
            Entry::Queued { ticket, position } => Some((ticket.holder.as_str(), *position)),
            _ => None,
        })
        .collect();
    assert_eq!(queued, [("high1", 1), ("high2", 2), ("low", 3)]);
    assert!(matches!(
        t.get(high1.id(), 0).unwrap(),
        Entry::Queued { position: 1, .. }
    ));
    t.release(&first.id, 1000).unwrap();
    let granted = active(&t.list(1000).unwrap(), "high1");
    assert_eq!(granted.ttl, ttl(2));
    assert_eq!(granted.expires_at, 3000);
    assert!(t.get(high1.id(), 1000).is_err());
    t.sweep(3000).unwrap();
    let second = active(&t.list(3000).unwrap(), "high2");
    let claimed = t.renew(&second.id, None, 3500).unwrap();
    assert_eq!(claimed.ttl, ttl(300));
    assert_eq!(claimed.expires_at, 303500);
    assert!(claimed.wanted_ttl.is_none());
    t.release(low.id(), 4000).unwrap();
    assert!(t.get(low.id(), 4000).is_err());
    assert!(t.get(high2.id(), 4000).is_err());
    assert_eq!(
        grants
            .lock()
            .unwrap()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["high1", "high2"]
    );
    assert!(events.lock().unwrap().contains(&EventKind::Expired));
    assert!(events.lock().unwrap().contains(&EventKind::Dropped));
}
#[test]
fn restart_recovers_custom_selectors_queue_and_claims() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("leases/state.json");
    let chooser: Chooser = Arc::new(|request, blocked| {
        let node = request
            .strip_prefix("choose:")
            .ok_or_else(|| Error::Invalid("bad selector".into()))?;
        let r = parse_resource(node)?;
        if blocked(&r).is_some() {
            return Err(Error::Busy("selected resource".into()));
        }
        Ok(r)
    });
    let t = LeaseTable::open(&path)
        .unwrap()
        .with_chooser(chooser.clone())
        .with_claim_window(ttl(2))
        .unwrap();
    t.acquire("initial", ttl(1), "choose:node", 0).unwrap();
    let mut w = wait("queued", "node", 7);
    w.request = "choose:node".into();
    let ticket = t.acquire_wait(w, 0).unwrap();
    assert!(matches!(ticket, Entry::Queued { .. }));
    drop(t);
    let t = LeaseTable::open(&path)
        .unwrap()
        .with_chooser(chooser.clone())
        .with_claim_window(ttl(2))
        .unwrap();
    let granted = active(&t.list(1000).unwrap(), "queued");
    assert_eq!(granted.wanted_ttl, Some(ttl(300)));
    drop(t);
    let t = LeaseTable::open(&path).unwrap().with_chooser(chooser);
    let claimed = t.renew(&granted.id, None, 1500).unwrap();
    assert_eq!(claimed.expires_at, 301500);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn busy_queue_does_not_block_unrelated_resources_and_old_tickets_drop() {
    let t = LeaseTable::memory();
    t.acquire("holder", MAX_LEASE_TTL, "busy", 0).unwrap();
    let q = t.acquire_wait(wait("old", "busy", 0), 0).unwrap();
    assert!(matches!(
        t.acquire_wait(wait("unrelated", "free", 0), 0).unwrap(),
        Entry::Active(_)
    ));
    t.sweep(MAX_QUEUE_WAIT.as_millis() as i64 + 1).unwrap();
    assert!(t
        .get(q.id(), MAX_QUEUE_WAIT.as_millis() as i64 + 1)
        .is_err());
    assert!(!t
        .list(MAX_QUEUE_WAIT.as_millis() as i64 + 1)
        .unwrap()
        .iter()
        .any(|e| match e {
            Entry::Active(l) => l.holder == "old",
            Entry::Queued { ticket, .. } => ticket.holder == "old",
        }));
    let unavailable = LeaseTable::memory()
        .with_chooser(Arc::new(|_, _| Err(Error::Unavailable("offline".into()))));
    assert!(matches!(
        unavailable.acquire_wait(wait("x", "node", 0), 0),
        Err(Error::Unavailable(_))
    ));
    assert!(unavailable.list(0).unwrap().is_empty());
}
#[test]
fn chooser_cannot_bypass_conflicts_and_concurrent_acquisition_is_serial() {
    let t = LeaseTable::memory().with_chooser(Arc::new(|request, _| parse_resource(request)));
    t.acquire("a", ttl(10), "node", 0).unwrap();
    assert!(matches!(
        t.acquire("b", ttl(10), "node", 0),
        Err(Error::Busy(_))
    ));
    let t = LeaseTable::memory();
    let successes = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let t = &t;
                scope.spawn(move || t.acquire(&n.to_string(), ttl(10), "node:gpu0", 0).is_ok())
            })
            .collect();
        threads
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>()
    });
    assert_eq!(successes, 1);
}
#[test]
fn failed_write_rolls_back_and_emits_no_success() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("state");
    let path = parent.join("leases.json");
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let t = LeaseTable::open(&path)
        .unwrap()
        .with_on_event(move |e| seen.lock().unwrap().push(e.kind));
    let l = t.acquire("a", ttl(100), "node", 0).unwrap();
    events.lock().unwrap().clear();
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&parent).unwrap();
    fs::write(&parent, b"block writes").unwrap();
    assert!(t.release(&l.id, 1).is_err());
    assert!(t.acquire("b", ttl(100), "other", 1).is_err());
    assert!(t.renew(&l.id, Some(ttl(200)), 1).is_err());
    assert!(t.acquire_wait(wait("queued", "node", 0), 1).is_err());
    assert!(events.lock().unwrap().is_empty());
    let all = t.list(1).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(active(&all, "a"), l);
}
#[test]
fn corrupt_state_is_not_overwritten_and_resource_parser_is_strict() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("state.json");
    fs::write(&p, b"broken").unwrap();
    assert!(LeaseTable::open(&p).is_err());
    assert_eq!(fs::read(&p).unwrap(), b"broken");
    for name in [
        "",
        "node:gpu-1",
        "node:gpu1:extra",
        "../node",
        "node:gpu",
        "node:gpu4294967296",
    ] {
        assert!(parse_resource(name).is_err(), "{name}")
    }
    assert_eq!(
        parse_resource(" NODE:GPU2 ").unwrap(),
        Resource {
            node: "node".into(),
            gpu: Some(2)
        }
    );
}
