use protectorate_auth::{self as auth, Method};
use protectorate_identity::*;
use std::{fs, time::Duration};
const METHODS: &[Method<'static>] = &[
    Method {
        tool: "job_run",
        service: "JobService",
        read_only: false,
        owner_only: false,
    },
    Method {
        tool: "job_list",
        service: "JobService",
        read_only: true,
        owner_only: false,
    },
    Method {
        tool: "token_create",
        service: "TokenService",
        read_only: false,
        owner_only: true,
    },
];
#[test]
fn token_lifecycle_keeps_secrets_out_of_disk_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private/tokens.json");
    let s = Store::open(&path).unwrap();
    let issued = s
        .create(
            "agent",
            &["job_*", "read", "read"],
            Duration::from_secs(10),
            100,
            METHODS,
        )
        .unwrap();
    assert!(issued.secret.expose().starts_with(SECRET_PREFIX));
    assert_eq!(issued.token.scopes, ["job_*", "read"]);
    assert_eq!(issued.token.expires, Some(110));
    assert!(!format!("{issued:?}").contains(issued.secret.expose()));
    let bytes = fs::read_to_string(&path).unwrap();
    assert!(!bytes.contains(issued.secret.expose()));
    assert!(!serde_json::to_string(&issued.token)
        .unwrap()
        .contains("hash"));
    assert_eq!(
        s.lookup(issued.secret.expose(), 109).unwrap().last_used,
        Some(109)
    );
    assert!(matches!(
        s.lookup(issued.secret.expose(), 110),
        Err(Error::Expired)
    ));
    assert!(matches!(s.lookup("bogus", 100), Err(Error::UnknownToken)));
    let s = Store::open(&path).unwrap();
    let revoked = s.revoke(&issued.token.id, 105).unwrap();
    assert_eq!(
        s.revoke(&issued.token.id, 106).unwrap().revoked_at,
        Some(105)
    );
    assert!(matches!(
        s.lookup(issued.secret.expose(), 106),
        Err(Error::Revoked)
    ));
    assert_eq!(revoked.id, issued.token.id);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn scope_updates_renewal_and_task_grants() {
    let s = Store::memory();
    assert!(s
        .create("", &["read"], Duration::ZERO, 10, METHODS)
        .is_err());
    assert!(s
        .create("owner", &["full"], Duration::ZERO, 10, METHODS)
        .is_err());
    assert!(s
        .create("owner", &["node_admin:*"], Duration::ZERO, 10, METHODS)
        .is_err());
    let original = s
        .create("agent", &["read"], Duration::ZERO, 10, METHODS)
        .unwrap();
    let (t, before) = s
        .update_scopes("agent", &["job_*"], &["read"], 11, METHODS)
        .unwrap();
    assert_eq!(before, ["read"]);
    assert_eq!(t.scopes, ["job_*"]);
    assert_eq!(
        s.lookup(original.secret.expose(), 12).unwrap().scopes,
        ["job_*"]
    );
    let renew = s.renew(&t.id, Duration::from_secs(5), 12).unwrap();
    assert_ne!(renew.secret.expose(), original.secret.expose());
    assert_eq!(renew.token.expires, Some(17));
    s.create("agent", &["read"], Duration::ZERO, 12, METHODS)
        .unwrap();
    assert!(matches!(
        s.update_scopes("agent", &[], &[], 13, METHODS),
        Err(Error::Ambiguous(_))
    ));
    let grant = s
        .create_grant(
            "node-*",
            "  compile project  ",
            "owner",
            Duration::from_secs(3600),
            20,
            METHODS,
        )
        .unwrap();
    assert_eq!(grant.token.task, "compile project");
    assert!(auth::admin_on(
        &grant
            .token
            .scopes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "node-linux"
    ));
    assert!(!auth::admin_on(
        &grant
            .token
            .scopes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "other"
    ));
    assert!(s
        .create_grant("*", "task", "owner", Duration::ZERO, 0, METHODS)
        .is_err());
    assert!(s.renew(&grant.token.id, Duration::ZERO, 21).is_err());
    assert!(s
        .create_grant("*", "", "owner", Duration::from_secs(10), 0, METHODS)
        .is_err());
}
#[test]
fn device_approval_is_durable_and_delivered_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    let s = Store::open(&path).unwrap();
    let started = s
        .start_device("phone", "android", "127.0.0.1", 100)
        .unwrap();
    let secret = started.device_code.expose().to_owned();
    let code = started.request.user_code.clone();
    assert!(!fs::read_to_string(&path).unwrap().contains(&secret));
    assert_eq!(
        s.poll_device(&secret, 100).unwrap().status,
        PollStatus::Pending
    );
    let slow = s.poll_device(&secret, 101).unwrap();
    assert_eq!(slow.status, PollStatus::SlowDown);
    assert_eq!(slow.interval, 10);
    let r = s
        .decide_device(&code.replace('-', "").to_lowercase(), "owner", true, 102)
        .unwrap();
    assert_eq!(r.status, DeviceStatus::Approved);
    assert!(s.pending_devices(102).is_empty());
    drop(s);
    let s = Store::open(&path).unwrap();
    let result = s.poll_device(&secret, 111).unwrap();
    assert_eq!(result.status, PollStatus::Approved);
    let issued = result.issued.unwrap();
    assert!(issued.token.device);
    assert_eq!(issued.token.scopes, ["full"]);
    assert_eq!(issued.token.approved_by, "owner");
    assert_eq!(issued.token.approved_at, Some(102));
    assert!(s.lookup(issued.secret.expose(), 100_000).is_ok());
    assert_eq!(
        s.poll_device(&secret, 112).unwrap().status,
        PollStatus::Consumed
    );
    assert_eq!(
        Store::open(&path)
            .unwrap()
            .poll_device(&secret, 113)
            .unwrap()
            .status,
        PollStatus::Consumed
    );
    assert!(s
        .update_scopes(&issued.token.id, &[], &["full"], 114, METHODS)
        .is_err());
    assert_eq!(
        s.rename_device(&issued.token.id, "tablet").unwrap().name,
        "tablet"
    );
    assert!(s.rename_device(&issued.token.id, "sleeve:bad").is_err());
    assert_eq!(
        s.poll_device("unknown", 114).unwrap().status,
        PollStatus::Expired
    );
}
#[test]
fn device_denial_expiry_labels_and_capacity() {
    let s = Store::memory();
    for name in ["envoy", "orchestrator-envoy", "sleeve:x", "bad\nname", ""] {
        assert!(s.start_device(name, "web", "", 0).is_err())
    }
    let start = s.start_device("phone", "web", "", 0).unwrap();
    s.decide_device(&start.request.user_code, "owner", false, 1)
        .unwrap();
    assert_eq!(
        s.poll_device(start.device_code.expose(), 2).unwrap().status,
        PollStatus::Denied
    );
    assert_eq!(
        s.poll_device(start.device_code.expose(), 600)
            .unwrap()
            .status,
        PollStatus::Expired
    );
    for i in 0..MAX_PENDING_DEVICES {
        s.start_device(&format!("device{i}"), "web", "", 1).unwrap();
    }
    assert!(matches!(
        s.start_device("overflow", "web", "", 1),
        Err(Error::DeviceCapacity)
    ));
    s.sweep_devices(601).unwrap();
    assert!(s.pending_devices(601).is_empty());
    assert!(s.start_device("again", "web", "", 601).is_ok());
}
#[test]
fn failed_persistence_keeps_approval_and_tokens_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("state");
    let path = parent.join("tokens.json");
    let s = Store::open(&path).unwrap();
    let pending = s.start_device("phone", "web", "", 1).unwrap();
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&parent).unwrap();
    fs::write(&parent, b"blocks directory").unwrap();
    assert!(s
        .decide_device(&pending.request.user_code, "owner", true, 2)
        .is_err());
    assert_eq!(s.pending_devices(2).len(), 1);
    assert!(s
        .create("x", &["read"], Duration::ZERO, 2, METHODS)
        .is_err());
    assert!(s.list().is_empty());
    fs::remove_file(&parent).unwrap();
    let issued = s
        .create("x", &["read"], Duration::ZERO, 3, METHODS)
        .unwrap();
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&parent).unwrap();
    fs::write(&parent, b"blocks directory").unwrap();
    let watch = s.watch_token(&issued.token.id, 3);
    assert!(s.revoke(&issued.token.id, 4).is_err());
    assert!(!watch.is_cancelled());
    assert!(s.lookup(issued.secret.expose(), 5).is_ok());
}
#[test]
fn revocation_watches_cover_admission_races() {
    let s = Store::memory();
    let issued = s
        .create("x", &["read"], Duration::ZERO, 0, METHODS)
        .unwrap();
    let watch = s.watch_token(&issued.token.id, 0);
    std::thread::scope(|scope| {
        let waiter = scope.spawn(|| watch.wait(Duration::from_secs(2)));
        s.revoke(&issued.token.id, 1).unwrap();
        assert!(waiter.join().unwrap())
    });
    assert!(s.watch_token(&issued.token.id, 2).is_cancelled());
    assert!(s.watch_token("unknown", 2).is_cancelled());
    let expiring = s
        .create("expires", &["read"], Duration::from_secs(1), 0, METHODS)
        .unwrap();
    assert!(s.watch_token(&expiring.token.id, 1).is_cancelled());
}
