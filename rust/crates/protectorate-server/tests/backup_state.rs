#![cfg(target_os = "linux")]
//! `hub backup` and `hub restore` with the real binary: a restored home brings
//! back what a running hub wrote, and a damaged or hostile archive changes
//! nothing on disk.
mod support;

use protectorate_connect_client::Client;
use protectorate_wire::Code;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};
use support::{binary, Hub};

fn hub_cli(args: &[&str]) -> Output {
    Command::new(binary())
        .args(["hub"])
        .args(args)
        .env_remove("PROTECTORATE_HOME")
        .output()
        .unwrap()
}

fn backup(home: &Path, to: &Path) -> PathBuf {
    let output = hub_cli(&[
        "backup",
        "--home",
        home.to_str().unwrap(),
        "--to",
        to.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
}

fn restore(archive: &Path, home: &Path) -> Output {
    hub_cli(&[
        "restore",
        archive.to_str().unwrap(),
        "--home",
        home.to_str().unwrap(),
    ])
}

#[test]
fn restored_home_serves_the_state_a_running_hub_wrote() {
    let first = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(first.path());
    let note = hub.call(
        "/protectorate.v1.BacklogService/CreateNote",
        json!({"title":"survives restore","body":"kept"}),
    );
    hub.call(
        "/protectorate.v1.NodeService/AddNode",
        json!({"node":{"name":"desk","address":"127.0.0.1:9","tier":"care"}}),
    );
    let mail = hub.call(
        "/protectorate.v1.MessageService/Send",
        json!({"to":"orchestrator","body":"read me after restore"}),
    );
    let token = |name: &str| {
        hub.call(
            "/protectorate.v1.TokenService/CreateToken",
            json!({"name":name,"scopes":["read"]}),
        )
    };
    let (live, revoked) = (token("live"), token("revoked"));
    hub.call(
        "/protectorate.v1.TokenService/RevokeToken",
        json!({"id":revoked["token"]["id"]}),
    );
    let head = hub.call("/protectorate.v1.AuditService/VerifyAudit", json!({}));
    assert_eq!(head["ok"], true, "{head}");
    hub.stop();

    let archive = backup(&hub.home, &first.path().join("archives"));
    let second = tempfile::tempdir().unwrap();
    let output = restore(&archive, &second.path().join("hub"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let restored = Hub::start(second.path());
    let notes = restored.call("/protectorate.v1.BacklogService/ListNotes", json!({}));
    assert_eq!(notes["notes"][0]["id"], note["note"]["id"], "{notes}");
    let node = restored.status("desk");
    assert_eq!(node["node"]["tier"], "care", "{node}");
    let thread = restored.call(
        "/protectorate.v1.MessageService/Thread",
        json!({"threadId":mail["message"]["threadId"]}),
    );
    assert_eq!(
        thread["messages"][0]["body"], "read me after restore",
        "{thread}"
    );
    let reader = |secret: &serde_json::Value| {
        Client::new(
            &restored.plain,
            secret["secret"].as_str().unwrap(),
            Duration::from_secs(10),
        )
        .unwrap()
        .call("/protectorate.v1.BacklogService/ListNotes", &json!({}))
    };
    reader(&live).expect("a live capability still authenticates");
    assert_eq!(reader(&revoked).unwrap_err().code, Code::Unauthenticated);
    assert_eq!(
        fs::read(hub.home.join("ca/ca.pem")).unwrap(),
        fs::read(restored.home.join("ca/ca.pem")).unwrap()
    );
    let verified = restored.call("/protectorate.v1.AuditService/VerifyAudit", json!({}));
    assert_eq!(verified["ok"], true, "{verified}");
    assert!(
        verified["entries"].as_u64() >= head["entries"].as_u64(),
        "{verified} {head}"
    );
    let created = restored.call(
        "/protectorate.v1.AuditService/ListAuditEntries",
        json!({"procedure":"BacklogService/CreateNote"}),
    );
    assert_eq!(created["entries"][0]["outcome"], "ok", "{created}");
}

/// Writes a one-file backup archive by hand: the format magic, then each
/// header as a little-endian length and JSON, the file bytes after its header.
fn crafted(path: &Path, name: &str, body: &[u8]) {
    use sha2::{Digest, Sha256};
    let mut bytes = b"PROTECTORATE-BACKUP\x01\n".to_vec();
    let entry = json!({"path":name,"size":body.len(),"mode":0o600});
    let manifest = json!({"type":"manifest","manifest":{
        "format":1,"created":"2026-10-24T00:00:00Z","hub_version":"test","home":"/hub",
        "files":[{"path":name,"size":body.len(),"sha256":format!("{:x}", Sha256::digest(body)),"mode":0o600}]
    }});
    let mut file = entry.clone();
    file["type"] = json!("file");
    for (header, data) in [(file, body), (manifest, &[][..])] {
        let encoded = header.to_string().into_bytes();
        bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&encoded);
        bytes.extend_from_slice(data);
    }
    fs::write(path, bytes).unwrap();
}

#[test]
fn restore_refuses_damaged_and_hostile_archives_without_touching_disk() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let outside = root.path().join("outside-secret");
    fs::create_dir_all(home.join("audit")).unwrap();
    fs::write(home.join("notes.json"), br#"{"notes":[]}"#).unwrap();
    fs::write(home.join("audit/audit-2026-10.jsonl"), b"entry\n").unwrap();
    fs::write(&outside, b"credential outside the hub home").unwrap();
    symlink(&outside, home.join("nodes.json")).unwrap();
    let archives = root.path().join("archives");
    let good = backup(&home, &archives);
    let sidecar = PathBuf::from(format!("{}.sha256", good.display()));
    let original = fs::read(&good).unwrap();
    assert!(!original
        .windows(b"credential outside".len())
        .any(|window| window == b"credential outside"));

    let cases = root.path().join("cases");
    fs::create_dir(&cases).unwrap();
    let flipped = |name: &str, keep_sidecar: bool| {
        let path = cases.join(name);
        let mut bytes = original.clone();
        let at = bytes.windows(5).position(|w| w == b"entry").unwrap();
        bytes[at] ^= 0x20;
        fs::write(&path, bytes).unwrap();
        if keep_sidecar {
            fs::copy(&sidecar, format!("{}.sha256", path.display())).unwrap();
        }
        path
    };
    let truncated = cases.join("truncated.pbackup");
    fs::write(&truncated, &original[..original.len() - 10]).unwrap();
    let traversal = cases.join("traversal.pbackup");
    crafted(&traversal, "../escaped", b"planted");
    let absolute = cases.join("absolute.pbackup");
    let escaped = root.path().join("escaped-absolute");
    crafted(&absolute, escaped.to_str().unwrap(), b"planted");
    let empty = root.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let linked = root.path().join("linked-home");
    symlink(&empty, &linked).unwrap();
    let restores = root.path().join("restores");
    fs::create_dir(&restores).unwrap();
    for (archive, destination, refusal) in [
        (
            flipped("sidecar.pbackup", true),
            restores.join("a"),
            "does not match its SHA-256 sidecar",
        ),
        (
            flipped("manifest.pbackup", false),
            restores.join("b"),
            "does not match the manifest",
        ),
        (truncated, restores.join("c"), "truncated"),
        (traversal, restores.join("d"), "unsafe"),
        (absolute, restores.join("e"), "unsafe"),
        (good.clone(), linked.clone(), "plain directory"),
    ] {
        let output = restore(&archive, &destination);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{}", archive.display());
        assert!(stderr.contains(refusal), "{}: {stderr}", archive.display());
        assert!(
            !destination.exists() || destination == linked,
            "{}",
            archive.display()
        );
    }
    assert!(fs::read_dir(&restores).unwrap().next().is_none());
    assert!(fs::read_dir(&empty).unwrap().next().is_none());
    assert!(!root.path().join("escaped").exists());
    assert!(!escaped.exists());

    let restored = restores.join("good");
    let output = restore(&good, &restored);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(restored.join("audit/audit-2026-10.jsonl")).unwrap(),
        b"entry\n"
    );
    assert!(fs::symlink_metadata(restored.join("nodes.json")).is_err());
}
