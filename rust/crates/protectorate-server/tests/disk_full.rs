//! A real hub whose home sits on a tiny tmpfs, in a user and mount namespace
//! the test owns, so the disk really fills (ENOSPC) without any privilege.
//! Writes must fail with a clear error and leave memory and disk in step,
//! reads must keep working, and the hub must take writes again once space
//! returns, without a restart.
#![cfg(target_os = "linux")]
use crate::support;

use protectorate_connect_client::Client;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use support::{binary, bound, die_with_parent, until, Process, OWNER};

const MSG: &str = "/protectorate.v1.MessageService/";
const NOTE: &str = "/protectorate.v1.BacklogService/";
const TOKEN: &str = "/protectorate.v1.TokenService/";
const SLEEVE: &str = "/protectorate.v1.SleeveService/";
const AUDIT: &str = "/protectorate.v1.AuditService/";
const HUB: &str = "/protectorate.v1.HubService/GetStatus";

/// Keeps a private mount namespace alive with a size-limited tmpfs mounted at
/// `mount`; hub processes join it with nsenter.
struct SmallDisk {
    holder: Process,
    mount: PathBuf,
}

impl SmallDisk {
    fn new(mount: &Path, size: &str) -> Self {
        fs::create_dir_all(mount).unwrap();
        let mut child = die_with_parent(Command::new("unshare").args([
            "--user",
            "--map-root-user",
            "--mount",
            "sh",
            "-c",
        ]))
            .arg(r#"mount -t tmpfs -o "size=$2,mode=0755" tmpfs "$1" && echo mounted && exec sleep 3600"#)
            .args(["sh"])
            .arg(mount)
            .arg(size)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("unshare is required for the disk-full scenarios");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            line.trim(),
            "mounted",
            "a size-limited tmpfs needs unprivileged user namespaces"
        );
        Self {
            holder: Process(child),
            mount: mount.to_owned(),
        }
    }

    fn pid(&self) -> u32 {
        self.holder.0.id()
    }

    /// `path` as seen from outside, inside the namespace's mount tree.
    fn outside(&self, path: &Path) -> PathBuf {
        PathBuf::from(format!("/proc/{}/root", self.pid())).join(path.strip_prefix("/").unwrap())
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new("nsenter");
        command
            .arg("-t")
            .arg(self.pid().to_string())
            .args(["-U", "-m", "--preserve-credentials", "--"])
            .arg(program);
        command
    }

    fn free_bytes(&self) -> u64 {
        let stat = nix_statvfs(&self.outside(&self.mount));
        stat.0 * stat.1
    }

    /// Writes a file until the filesystem refuses; returns the filler's path.
    fn fill(&self) -> PathBuf {
        let filler = self.outside(&self.mount.join("filler"));
        let mut file = fs::File::create(&filler).unwrap();
        let block = vec![0xa5u8; 4096];
        while file.write_all(&block).is_ok() {}
        // Whatever is left is under a page, or nothing.
        assert!(self.free_bytes() < 4096, "the disk did not fill");
        filler
    }
}

/// (available blocks, block size).
fn nix_statvfs(path: &Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::statvfs(c.as_ptr(), &mut stat) }, 0);
    (stat.f_bavail as u64, stat.f_frsize as u64)
}

struct DiskHub {
    process: Option<Process>,
    tmux: String,
    client: Client,
    log: PathBuf,
}

fn start_hub(disk: &SmallDisk, root: &Path, run: u32) -> DiskHub {
    let user_home = root.join("user");
    fs::create_dir_all(&user_home).unwrap();
    let token = root.join("owner-token");
    fs::write(&token, OWNER).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let log = root.join(format!("hub-{run}.log"));
    let tmux = format!(
        "p-full-{}-{}",
        std::process::id(),
        root.file_name().unwrap().to_string_lossy()
    );
    let mut command = disk.command(binary());
    command
        .args(["serve", "--home"])
        .arg(&disk.mount)
        .arg("--owner-token-file")
        .arg(&token)
        .args(["--listen", "127.0.0.1:0", "--tls-listen", "127.0.0.1:0"])
        .args(["--release-api-url", "http://127.0.0.1:1"])
        .args(["--tmux-socket", &tmux])
        .env("HOME", &user_home)
        .env("PROTECTORATE_HUB_GUARD_CHILD", "1")
        .env_remove("PROTECTORATE_HOME")
        .env_remove("PROTECTORATE_TOKEN")
        .env_remove("PROTECTORATE_TOKEN_FILE")
        .env_remove("PROTECTORATE_HUB")
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&log).unwrap()));
    let mut process = Process(die_with_parent(&mut command).spawn().unwrap());
    until("hub start on the small disk", &log, || {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "hub exited: {}",
            fs::read_to_string(&log).unwrap_or_default()
        );
        bound(&log, "serving TLS").is_some()
    });
    let url = format!("http://{}", bound(&log, "serving").unwrap());
    // Short timeouts: a write that hangs on a full disk fails the test.
    let client = Client::new(&url, OWNER, Duration::from_secs(20)).unwrap();
    until("hub status", &log, || client.call(HUB, &json!({})).is_ok());
    DiskHub {
        process: Some(process),
        tmux,
        client,
        log,
    }
}

/// A killed hub leaves its sleeves' tmux servers running; stop them by socket.
impl Drop for DiskHub {
    fn drop(&mut self) {
        self.process = None;
        // The hub runs as uid 0 inside its namespace, so tmux keeps the
        // socket in /tmp/tmux-0, which support::kill_tmux does not look in.
        let socket = Path::new("/tmp/tmux-0").join(&self.tmux);
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(&socket)
            .arg("kill-server")
            .stderr(Stdio::null())
            .status();
        let _ = fs::remove_file(socket);
    }
}

impl DiskHub {
    fn call(&self, path: &str, body: Value) -> Result<Value, String> {
        let started = Instant::now();
        let result = self.client.call(path, &body).map_err(|e| e.to_string());
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "{path} hung for {:?}",
            started.elapsed()
        );
        result
    }

    fn ok(&self, path: &str, body: Value) -> Value {
        self.call(path, body)
            .unwrap_or_else(|e| panic!("{path}: {e}\n{}", self.log_text()))
    }

    fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn alive(&mut self) {
        assert!(
            self.process
                .as_mut()
                .unwrap()
                .0
                .try_wait()
                .unwrap()
                .is_none(),
            "hub died: {}",
            self.log_text()
        );
        self.ok(HUB, json!({}));
        assert!(
            !self.log_text().contains("panicked"),
            "hub panicked: {}",
            self.log_text()
        );
    }
}

#[derive(Default)]
struct Acked {
    /// The first message, note and token get changed in place later;
    /// these are what a failed or acknowledged change leaves them as.
    first_acked: bool,
    first_revoked: bool,
    mails: Vec<(String, String)>,
    notes: Vec<(String, String)>,
    tokens: Vec<String>,
}

/// A full disk reads as one, and says which write failed.
fn assert_clear(error: &str) {
    assert!(
        error.contains("No space left on device")
            && ["write ", "sync ", "create ", "stat "]
                .iter()
                .any(|what| error.contains(what)),
        "an error for a full disk must name the disk and the failed write: {error}"
    );
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

/// One mutation of each kind with a body that crosses a page boundary.
/// Returns the failures' messages; successes join `acked`.
fn mutate_all(hub: &DiskHub, acked: &mut Acked, tag: &str, size: usize) -> Vec<String> {
    let mut errors = Vec::new();
    let body = format!("{tag} {}", "m".repeat(size));
    match hub.call(
        &format!("{MSG}Send"),
        json!({"to":"owner","body":body,"requestId":format!("full-{tag}-mail")}),
    ) {
        Ok(sent) => acked.mails.push((
            text(&sent["message"]["id"]).to_owned(),
            text(&sent["message"]["threadId"]).to_owned(),
        )),
        Err(e) => errors.push(format!("send: {e}")),
    }
    match hub.call(
        &format!("{NOTE}CreateNote"),
        json!({"title":format!("note {tag}"),"body":"n".repeat(size)}),
    ) {
        Ok(made) => acked
            .notes
            .push((text(&made["note"]["id"]).to_owned(), format!("note {tag}"))),
        Err(e) => errors.push(format!("create note: {e}")),
    }
    match hub.call(
        &format!("{TOKEN}CreateToken"),
        json!({"name":format!("token {tag}"),"scopes":["read"]}),
    ) {
        Ok(made) => acked.tokens.push(text(&made["token"]["id"]).to_owned()),
        Err(e) => errors.push(format!("create token: {e}")),
    }
    errors
}

/// Changes the first message, note and token in place; a change the
/// hub refuses must leave the old value, one it accepts must stick.
fn change_first(hub: &DiskHub, acked: &mut Acked, tag: &str) -> usize {
    let mut failed = 0;
    let mut outcome = |what: &str, result: Result<Value, String>| match result {
        Ok(_) => true,
        Err(e) => {
            assert_clear(&format!("{what}: {e}"));
            failed += 1;
            false
        }
    };
    let (id, _) = acked.mails[0].clone();
    if outcome("ack", hub.call(&format!("{MSG}Ack"), json!({"id":id}))) {
        acked.first_acked = true;
    }
    let (note, _) = acked.notes[0].clone();
    let title = format!("changed {tag}");
    if outcome(
        "update note",
        hub.call(
            &format!("{NOTE}UpdateNote"),
            json!({"id":note,"title":title}),
        ),
    ) {
        acked.notes[0].1 = title;
    }
    let token = acked.tokens[0].clone();
    if outcome(
        "revoke",
        hub.call(&format!("{TOKEN}RevokeToken"), json!({"id":token})),
    ) {
        acked.first_revoked = true;
    }
    failed
}

/// Spawns a sleeve; it either fails with a clear error and leaves no trace,
/// or succeeds.
fn heavy(hub: &DiskHub, work: &Path, tag: &str, must_succeed: bool) {
    let sleeve = format!("sl-{tag}");
    let dir = work.join(&sleeve);
    fs::create_dir_all(&dir).unwrap();
    let spawned = hub.call(
        &format!("{SLEEVE}SpawnSleeve"),
        json!({"name":sleeve,"harness":"HARNESS_SHELL","mode":"SLEEVE_MODE_TERMINAL",
            "workspace":dir,"args":["/usr/bin/sh","-c","sleep 1"]}),
    );
    if must_succeed {
        spawned.as_ref().expect("spawn after recovery");
    }
    if let Err(e) = &spawned {
        assert_clear(&format!("spawn: {e}"));
    }
    let got = hub.call(&format!("{SLEEVE}GetSleeve"), json!({"name":sleeve}));
    assert_eq!(
        got.is_ok(),
        spawned.is_ok(),
        "sleeve record vs spawn result: {got:?}"
    );
}

/// Reads through the API: exactly the acknowledged state is visible.
fn check_reads(hub: &DiskHub, acked: &Acked, context: &str) {
    for (id, thread) in &acked.mails {
        let thread = hub.ok(&format!("{MSG}Thread"), json!({"threadId":thread}));
        assert!(
            thread["messages"]
                .as_array()
                .is_some_and(|m| m.iter().any(|m| text(&m["id"]) == id)),
            "{context}: message {id} missing: {thread}"
        );
    }
    if let Some((id, thread)) = acked.mails.first() {
        let thread = hub.ok(&format!("{MSG}Thread"), json!({"threadId":thread}));
        let message = thread["messages"]
            .as_array()
            .and_then(|m| m.iter().find(|m| text(&m["id"]) == id))
            .unwrap();
        let is_acked = message["deliveries"]
            .as_array()
            .is_some_and(|d| d.iter().any(|d| !d["ackedAt"].is_null()));
        assert_eq!(is_acked, acked.first_acked, "{context}: ack state of {id}");
    }
    let listed = hub.ok(&format!("{NOTE}ListNotes"), json!({}));
    let notes = listed["notes"].as_array().cloned().unwrap_or_default();
    for (id, title) in &acked.notes {
        assert!(
            notes
                .iter()
                .any(|n| text(&n["id"]) == id && text(&n["title"]) == title),
            "{context}: note {id} missing"
        );
    }
    assert_eq!(
        notes.len(),
        acked.notes.len(),
        "{context}: a failed create is visible in memory: {listed}"
    );
    let tokens = hub.ok(&format!("{TOKEN}ListTokens"), json!({}));
    let tokens = tokens["tokens"].as_array().cloned().unwrap_or_default();
    for id in &acked.tokens {
        assert!(
            tokens.iter().any(|t| text(&t["id"]) == id),
            "{context}: token {id} missing"
        );
    }
    if let Some(id) = acked.tokens.first() {
        let token = tokens.iter().find(|t| text(&t["id"]) == id).unwrap();
        assert_eq!(
            !token["revokedAt"].is_null(),
            acked.first_revoked,
            "{context}: revocation state of {id}"
        );
    }
    assert_eq!(
        tokens
            .iter()
            .filter(|t| text(&t["name"]).starts_with("token "))
            .count(),
        acked.tokens.len(),
        "{context}: a failed token is visible in memory"
    );
}

/// Every durable file parses and no store file is truncated or torn.
fn check_disk(disk: &SmallDisk, context: &str) {
    let home = disk.outside(&disk.mount);
    for name in [
        "identity.json",
        "sleeves.json",
        "directory.json",
        "notes.json",
    ] {
        if let Ok(bytes) = fs::read(home.join(name)) {
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|e| panic!("{context}: {name} is corrupt: {e}"));
        }
    }
    let mailbox = fs::read(home.join("mailbox.jsonl")).unwrap_or_default();
    assert!(
        mailbox.is_empty() || mailbox.ends_with(b"\n"),
        "{context}: mailbox.jsonl ends in a torn line"
    );
    for (n, line) in mailbox.split(|b| *b == b'\n').enumerate() {
        if !line.is_empty() {
            serde_json::from_slice::<Value>(line)
                .unwrap_or_else(|e| panic!("{context}: mailbox.jsonl line {}: {e}", n + 1));
        }
    }
    // The audit log may end in one torn line (the next write cuts it); a
    // torn line with entries after it is a break.
    for entry in fs::read_dir(home.join("audit"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "jsonl") {
            let bytes = fs::read(&path).unwrap();
            let complete = bytes
                .rsplit(|b| *b == b'\n')
                .nth(1)
                .map_or(&[][..], |_| &bytes[..]);
            let body = complete.rsplitn(2, |b| *b == b'\n').nth(1).unwrap_or(&[]);
            for line in body.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
                serde_json::from_slice::<Value>(line).unwrap_or_else(|e| {
                    panic!(
                        "{context}: {} has a torn line in the middle: {e}",
                        path.display()
                    )
                });
            }
        }
    }
}

#[test]
fn full_disk_fails_writes_cleanly_keeps_reads_and_recovers_without_restart() {
    let root = tempfile::tempdir().unwrap();
    let disk = SmallDisk::new(&root.path().join("small"), "3m");
    let mut hub = start_hub(&disk, root.path(), 0);
    let mut acked = Acked::default();

    let errors = mutate_all(&hub, &mut acked, "base", 300);
    assert!(errors.is_empty(), "{errors:?}");
    let baseline = disk.outside(&disk.mount).join("mailbox.jsonl");
    let baseline_bytes = fs::read(&baseline).unwrap();
    check_reads(&hub, &acked, "baseline");

    let work = root.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let filler = disk.fill();
    let mut failures = 0;
    for round in 0..40 {
        // Odd sizes so appends straddle page boundaries.
        let size = 700 + (round * 331) % 5000;
        if round % 8 == 0 {
            heavy(&hub, &work, &format!("full{round}"), false);
        }
        failures += change_first(&hub, &mut acked, &format!("full{round}"));
        for error in mutate_all(&hub, &mut acked, &format!("full{round}"), size) {
            failures += 1;
            assert!(
                error.contains("space"),
                "unclear error for a full disk: {error}"
            );
            assert_clear(&error);
        }
    }
    assert!(
        failures > 100,
        "writes did not fail on a full disk: {failures}"
    );
    hub.alive();
    check_reads(&hub, &acked, "full");
    check_disk(&disk, "while full");
    assert!(
        fs::read(&baseline).unwrap().starts_with(&baseline_bytes),
        "old mailbox content was damaged"
    );

    // A backup into the full disk fails cleanly and leaves no archive.
    let backups = disk.mount.join("backups");
    fs::create_dir_all(disk.outside(&backups)).unwrap();
    let output = disk
        .command(binary())
        .args(["hub", "backup", "--home"])
        .arg(&disk.mount)
        .arg("--to")
        .arg(&backups)
        .env_remove("PROTECTORATE_HOME")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "a backup into a full disk succeeded"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("space"),
        "unclear backup error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let left = fs::read_dir(disk.outside(&backups)).unwrap().count();
    assert_eq!(left, 0, "a failed backup left files behind");

    // A restart on the still-full disk comes up and serves what it has.
    hub.process = None;
    let mut hub = start_hub(&disk, root.path(), 1);
    hub.alive();
    check_reads(&hub, &acked, "restarted full");
    check_disk(&disk, "restarted full");
    let refused = hub.call(
        &format!("{NOTE}CreateNote"),
        json!({"title":"too late","body":"n".repeat(6000)}),
    );
    assert_clear(&refused.expect_err("a write on a full disk"));

    // Space returns: the hub takes writes again on the same process.
    fs::File::create(&filler).unwrap();
    assert!(disk.free_bytes() > 1 << 20);
    let errors = mutate_all(&hub, &mut acked, "after", 300);
    assert!(errors.is_empty(), "the hub did not recover: {errors:?}");
    // The names that failed on the full disk must be free again.
    heavy(&hub, &work, "full0", true);
    assert_eq!(change_first(&hub, &mut acked, "after"), 0);
    assert!(acked.first_acked && acked.first_revoked);
    hub.alive();
    check_reads(&hub, &acked, "recovered");
    check_disk(&disk, "after recovery");
    let audit = hub.ok(&format!("{AUDIT}VerifyAudit"), json!({}));
    assert_eq!(audit["ok"], true, "{audit}");

    // And a kill and restart on the same disk loses nothing.
    hub.process = None;
    let mut hub = start_hub(&disk, root.path(), 2);
    check_reads(&hub, &acked, "restarted");
    check_disk(&disk, "after restart");
    let audit = hub.ok(&format!("{AUDIT}VerifyAudit"), json!({}));
    assert_eq!(audit["ok"], true, "{audit}");
    hub.alive();
}
