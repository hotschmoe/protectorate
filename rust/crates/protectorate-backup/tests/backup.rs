use protectorate_backup::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
fn file(root: &Path, path: &str, data: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}
fn options(home: &Path, dir: &Path) -> Options {
    Options {
        home: home.into(),
        dir: dir.into(),
        version: "v1-test".into(),
        created_at: Some("2026-10-22T00:00:00Z".parse().unwrap()),
    }
}
fn sum(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn sidecar(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sha256", path.display()))
}
#[test]
fn roundtrip_excludes_transient_state_and_keeps_manifest_metadata() {
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    for path in [
        "token",
        "tokens.json",
        "identity.json",
        "nodes.json",
        "sleeves.json",
        "desk-policies.json",
        "desk-use.json",
        "leases.json",
        "notes.json",
        "directory.json",
        "audit/audit-2026-10.jsonl",
        "ca/ca.pem",
        "ca/ca-key.pem",
        "jobs/id.json",
        "jobs/id.log",
        "jobs/canceled-requests",
        "jobs/id.artifacts/result.txt",
        "jobs/id.artifacts/nested/trace.bin",
        "handoffs/alice/20261022T000000Z-pi-to-codex.md",
        "sleeves/alice/chat.jsonl",
        "sleeves/alice/chat.jsonl.request-ids",
    ] {
        file(home.path(), path, path.as_bytes())
    }
    file(
        home.path(),
        "jobs/id.json",
        br#"{"id":"id","job":{"artifacts":[{"path":"result.txt"},{"path":"nested/trace.bin"}]}}"#,
    );
    file(
        home.path(),
        "jobs/canceled-requests",
        br#"{"request-12345678":1792713600}"#,
    );
    for path in [
        "auth/credentials",
        "login/tokens",
        "bin/protectorate",
        "node-files/file",
        "local-jobs/job.json",
        "workspaces/repo/file",
        "sleeves/alice/home/secret",
        "sleeves/alice/workspace/file",
        "jobs/.id-artifacts-staging/file",
        "jobs/id.json.new",
        "handoffs/alice/private.tmp",
    ] {
        file(home.path(), path, b"excluded")
    }
    let cancel = Cancellation::default();
    let path = create(&options(home.path(), out.path()), &cancel).unwrap();
    let manifest = verify(&path, &cancel).unwrap();
    assert_eq!(manifest.hub_version, "v1-test");
    assert_eq!(manifest.files.len(), 21);
    assert!(manifest
        .files
        .iter()
        .all(|f| !f.path.contains("auth/") && !f.path.contains(".id-artifacts-staging/")));
    assert!(sidecar(&path).exists());
    let destination = out.path().join("restored");
    assert_eq!(restore(&path, &destination, &cancel).unwrap(), manifest);
    for f in &manifest.files {
        let data = fs::read(destination.join(&f.path)).unwrap();
        assert_eq!(sum(&data), f.sha256);
        assert_eq!(data.len() as u64, f.size)
    }
    assert!(!destination.join("handoffs/alice/private.tmp").exists());
    assert_eq!(
        fs::read(destination.join("jobs/canceled-requests")).unwrap(),
        br#"{"request-12345678":1792713600}"#
    );
    assert_eq!(latest(out.path()).unwrap().unwrap().0, path);
    assert!(create(&options(home.path(), out.path()), &cancel).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
fn archive(
    path: &Path,
    entries: &[(&str, &[u8])],
    manifest_files: Option<Vec<ArchivedFile>>,
    trailing: bool,
) {
    let mut f = fs::File::create(path).unwrap();
    f.write_all(b"PROTECTORATE-BACKUP\x01\n").unwrap();
    fn header(f: &mut fs::File, value: serde_json::Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        f.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&bytes).unwrap()
    }
    let mut files = Vec::new();
    for (name, data) in entries {
        header(
            &mut f,
            json!({"type":"file","path":name,"size":data.len(),"mode":384}),
        );
        f.write_all(data).unwrap();
        files.push(ArchivedFile {
            path: name.to_string(),
            size: data.len() as u64,
            sha256: sum(data),
            mode: 384,
        })
    }
    let manifest = Manifest {
        format: FORMAT_VERSION,
        created: "2026-10-22T00:00:00Z".parse().unwrap(),
        hub_version: "test".into(),
        home: "/source".into(),
        files: manifest_files.unwrap_or(files),
    };
    header(&mut f, json!({"type":"manifest","manifest":manifest}));
    if trailing {
        f.write_all(b"extra").unwrap()
    }
}
#[test]
fn invalid_archives_leave_empty_destination_untouched() {
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("home");
    fs::create_dir(&dest).unwrap();
    let path = root.path().join("bad.pbackup");
    let cancel = Cancellation::default();
    for entries in [
        vec![("../escape", &b"bad"[..])],
        vec![("/absolute", &b"bad"[..])],
        vec![("a\\b", &b"bad"[..])],
        vec![("C:stream", &b"bad"[..])],
        vec![("same", &b"1"[..]), ("same", &b"2"[..])],
    ] {
        archive(&path, &entries, None, false);
        assert!(restore(&path, &dest, &cancel).is_err());
        assert!(fs::read_dir(&dest).unwrap().next().is_none());
        assert!(!root.path().join("escape").exists());
    }
    archive(&path, &[("safe", b"one")], Some(vec![]), false);
    assert!(restore(&path, &dest, &cancel).is_err());
    archive(&path, &[("safe", b"one")], None, true);
    assert!(restore(&path, &dest, &cancel).is_err());
    archive(&path, &[("safe", b"one")], None, false);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() - 10);
    fs::write(&path, bytes).unwrap();
    assert!(restore(&path, &dest, &cancel).is_err());
    assert!(fs::read_dir(&dest).unwrap().next().is_none());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".restore-")));
}
#[test]
fn sidecar_and_per_file_hash_corruption_are_detected() {
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    file(home.path(), "token", b"abcdef");
    let path = create(&options(home.path(), out.path()), &Cancellation::default()).unwrap();
    let original = fs::read(&path).unwrap();
    let mut modified = original.clone();
    let offset = modified.windows(6).position(|w| w == b"abcdef").unwrap();
    modified[offset] = b'z';
    fs::write(&path, &modified).unwrap();
    assert!(verify(&path, &Cancellation::default()).is_err());
    fs::remove_file(sidecar(&path)).unwrap();
    assert!(verify(&path, &Cancellation::default()).is_err());
    fs::write(&path, original).unwrap();
    assert!(verify(&path, &Cancellation::default()).is_ok());
}
#[test]
fn backup_rejects_job_record_with_unarchived_artifact() {
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    file(
        home.path(),
        "jobs/id.json",
        br#"{"id":"id","job":{"artifacts":[{"path":"result.txt"}]}}"#,
    );
    let error = create(&options(home.path(), out.path()), &Cancellation::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("job artifact missing"), "{error}");
    assert!(latest(out.path()).unwrap().is_none());
    file(home.path(), "jobs/id.artifacts/result.txt", b"done");
    let archive = create(&options(home.path(), out.path()), &Cancellation::default()).unwrap();
    assert!(verify(&archive, &Cancellation::default())
        .unwrap()
        .files
        .iter()
        .any(|file| file.path == "jobs/id.artifacts/result.txt"));
}
#[test]
fn restore_rejects_a_verified_archive_with_a_missing_job_artifact() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("archive.pbackup");
    let record = br#"{"id":"id","job":{"artifacts":[{"path":"result.txt"}]}}"#;
    archive(&path, &[("jobs/id.json", record)], None, false);
    assert!(verify(&path, &Cancellation::default()).is_ok());
    let destination = root.path().join("restored");
    let error = restore(&path, &destination, &Cancellation::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing artifact"), "{error}");
    assert!(!destination.exists());

    archive(
        &path,
        &[
            ("jobs/id.json", record),
            ("jobs/id.artifacts/result.txt", b"result"),
        ],
        None,
        false,
    );
    restore(&path, &destination, &Cancellation::default()).unwrap();
    assert_eq!(
        fs::read(destination.join("jobs/id.artifacts/result.txt")).unwrap(),
        b"result"
    );
}
#[test]
fn restore_refuses_existing_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("archive.pbackup");
    archive(&path, &[("token", b"valid")], None, false);
    let home = root.path().join("home");
    file(&home, "existing", b"keep");
    assert!(restore(&path, &home, &Cancellation::default()).is_err());
    assert_eq!(fs::read(home.join("existing")).unwrap(), b"keep");
}
#[cfg(unix)]
#[test]
fn symlinks_cannot_leak_state_or_redirect_restore() {
    use std::os::unix::fs::symlink;
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    file(outside.path(), "secret", b"private");
    symlink(outside.path().join("secret"), home.path().join("token")).unwrap();
    symlink(outside.path(), home.path().join("audit")).unwrap();
    fs::create_dir_all(home.path().join("jobs/id.artifacts")).unwrap();
    symlink(
        outside.path().join("secret"),
        home.path().join("jobs/id.artifacts/secret"),
    )
    .unwrap();
    let path = create(&options(home.path(), out.path()), &Cancellation::default()).unwrap();
    assert!(verify(&path, &Cancellation::default())
        .unwrap()
        .files
        .is_empty());
    let dest = out.path().join("link");
    symlink(outside.path(), &dest).unwrap();
    assert!(restore(&path, &dest, &Cancellation::default()).is_err());
    assert_eq!(fs::read(outside.path().join("secret")).unwrap(), b"private");
}
#[test]
fn cancellation_and_scheduler_callbacks() {
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let options = options(home.path(), out.path());
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(matches!(
        create(&options, &cancelled),
        Err(Error::Cancelled)
    ));
    assert!(latest(out.path()).unwrap().is_none());
    let cancel = Cancellation::default();
    let stop = cancel.clone();
    let mut created = 0;
    run(&options, Duration::from_secs(1), &cancel, |event| {
        assert!(matches!(event, RunEvent::Created(_)));
        created += 1;
        stop.cancel();
    })
    .unwrap();
    assert_eq!(created, 1);
    assert!(run(&options, Duration::ZERO, &cancel, |_| {}).is_err());
}
