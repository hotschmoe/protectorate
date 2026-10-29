//! A real hub process is SIGKILLed while concurrent clients mutate every
//! durable store, then restarted on the same home: it must start, every
//! mutation a client saw succeed must be present, nothing may be half applied,
//! and the audit chain must verify.
//!
//! The kill delays come from a seed that is printed on every run; set
//! `PROTECTORATE_CRASH_SEED` to replay one and `PROTECTORATE_CRASH_ITERATIONS`
//! to run more kill and restart rounds than the default.
#![cfg(target_os = "linux")]
use crate::support;

use protectorate_connect_client::Client;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use support::{binary, Hub};

const MSG: &str = "/protectorate.v1.MessageService/";
const NOTE: &str = "/protectorate.v1.BacklogService/";
const TOKEN: &str = "/protectorate.v1.TokenService/";
const DEVICE: &str = "/protectorate.v1.DeviceService/";
const SLEEVE: &str = "/protectorate.v1.SleeveService/";
const AUDIT: &str = "/protectorate.v1.AuditService/";

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn seed() -> u64 {
    let seed = std::env::var("PROTECTORATE_CRASH_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        });
    eprintln!("crash_consistency: PROTECTORATE_CRASH_SEED={}", seed | 1);
    seed | 1
}

/// What clients were told succeeded; all of it must survive every kill.
#[derive(Default)]
struct Ledger {
    mails: Vec<(String, String, String, bool)>,
    notes: Vec<(String, Vec<String>, bool)>,
    deleted_notes: Vec<String>,
    tokens: Vec<(String, bool)>,
    devices: Vec<(String, String, String)>,
    sleeves: Vec<String>,
}

/// What a worker thread needs from the running hub.
#[derive(Clone)]
struct Ctx {
    client: Client,
    work: PathBuf,
}

/// Adds a device; the ledger keeps its id, name and token.
fn add_device(ctx: &Ctx, name: &str) -> Result<(String, String, String), String> {
    let added = ctx
        .client
        .call(&format!("{DEVICE}AddDevice"), &json!({"name": name}))
        .map_err(|e| e.to_string())?;
    Ok((
        text(&added["device"]["id"]).to_owned(),
        name.to_owned(),
        text(&added["token"]).to_owned(),
    ))
}

/// One mutation; the ledger learns of it only if the hub acknowledged it.
fn mutate(ctx: &Ctx, ledger: &Mutex<Ledger>, tag: &str, n: u64, op: u64) {
    let call = |path: &str, body: Value| ctx.client.call(path, &body);
    match op % 7 {
        0 | 1 => {
            let request = format!("crash-mail-{tag}");
            let sent = call(
                &format!("{MSG}Send"),
                json!({"to":"owner","body":format!("mail body {tag}"),"requestId":request,
                    "kind":"MESSAGE_KIND_REQUEST"}),
            );
            if let Ok(sent) = sent {
                let id = sent["message"]["id"].as_str().unwrap().to_owned();
                let thread = sent["message"]["threadId"].as_str().unwrap().to_owned();
                let acked = call(&format!("{MSG}Ack"), json!({"id":id})).is_ok();
                ledger
                    .lock()
                    .unwrap()
                    .mails
                    .push((id, thread, request, acked));
            }
        }
        2 | 3 => {
            let title = format!("note {tag}");
            let Ok(made) = call(
                &format!("{NOTE}CreateNote"),
                json!({"title":title,"body":"x".repeat(200)}),
            ) else {
                return;
            };
            let id = made["note"]["id"].as_str().unwrap().to_owned();
            // A failed call may still have been applied before the kill.
            let mut titles = vec![title];
            if n.is_multiple_of(3) {
                let renamed = format!("renamed {tag}");
                match call(
                    &format!("{NOTE}UpdateNote"),
                    json!({"id":id,"title":renamed}),
                ) {
                    Ok(_) => titles = vec![renamed],
                    Err(_) => titles.push(renamed),
                }
            }
            if n.is_multiple_of(5) {
                match call(&format!("{NOTE}DeleteNote"), json!({"id":id})) {
                    Ok(_) => ledger.lock().unwrap().deleted_notes.push(id),
                    Err(_) => ledger.lock().unwrap().notes.push((id, titles, true)),
                }
            } else {
                ledger.lock().unwrap().notes.push((id, titles, false));
            }
        }
        4 => {
            if let Ok(made) = call(
                &format!("{TOKEN}CreateToken"),
                json!({"name":format!("token {tag}"),"scopes":["read"]}),
            ) {
                let id = made["token"]["id"].as_str().unwrap().to_owned();
                let revoked = n.is_multiple_of(2)
                    && call(&format!("{TOKEN}RevokeToken"), json!({"id":id})).is_ok();
                ledger.lock().unwrap().tokens.push((id, revoked));
            }
        }
        5 => {
            if let Ok(device) = add_device(ctx, &format!("device {tag}")) {
                ledger.lock().unwrap().devices.push(device);
            }
        }
        6 => {
            let name = format!("sl-{tag}");
            let dir = ctx.work.join(&name);
            fs::create_dir_all(&dir).unwrap();
            let spawned = call(
                &format!("{SLEEVE}SpawnSleeve"),
                json!({"name":name,"harness":"HARNESS_SHELL","mode":"SLEEVE_MODE_TERMINAL",
                    "workspace":dir,"args":["/usr/bin/sh","-c","sleep 1"]}),
            );
            if spawned.is_ok() {
                ledger.lock().unwrap().sleeves.push(name);
            }
        }
        _ => unreachable!("op is reduced modulo 7"),
    }
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map_or(&[], Vec::as_slice)
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

/// Every durable file in `home` parses; temp files are reported, not fatal.
fn check_files(home: &Path, live: Option<u32>) {
    for name in [
        "identity.json",
        "sleeves.json",
        "directory.json",
        "notes.json",
    ] {
        if let Ok(bytes) = fs::read(home.join(name)) {
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|e| panic!("{name} is not valid JSON after restart: {e}"));
        }
    }
    // Only the running hub may have a replacement in flight.
    for entry in fs::read_dir(home).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let pid = name.strip_prefix('.').and_then(|rest| {
            let mut parts = rest.rsplitn(3, '.');
            parts.next()?.parse::<u64>().ok()?;
            parts.next()?.parse::<u32>().ok()
        });
        assert!(
            pid.is_none_or(|pid| Some(pid) == live) && !name.starts_with(".tmp"),
            "a dead writer's temp file survives: {}",
            entry.path().display()
        );
    }
    let mailbox = fs::read(home.join("mailbox.jsonl")).unwrap_or_default();
    assert!(
        mailbox.is_empty() || mailbox.ends_with(b"\n"),
        "mailbox.jsonl ends in a partial line"
    );
    for line in mailbox.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        serde_json::from_slice::<Value>(line).expect("mailbox.jsonl line parses");
    }
}

fn verify(hub: &Hub, ledger: &mut Ledger, context: &str) {
    let call = |path: &str, body: Value| {
        hub.client
            .call(path, &body)
            .unwrap_or_else(|e| panic!("{context}: {path}: {e}"))
    };
    check_files(&hub.home, hub.process.as_ref().map(|p| p.0.id()));
    let log = fs::read_to_string(hub.home.parent().unwrap().join("hub.log")).unwrap_or_default();
    assert!(!log.contains("panicked"), "{context}: hub panicked: {log}");

    for (id, thread, request, acked) in &ledger.mails {
        let thread = call(&format!("{MSG}Thread"), json!({"threadId":thread}));
        let m = array(&thread, "messages")
            .iter()
            .find(|m| text(&m["id"]) == id)
            .unwrap_or_else(|| panic!("{context}: acknowledged message {id} lost: {thread}"));
        assert!(
            text(&m["body"]).starts_with("mail body ") && !array(m, "deliveries").is_empty(),
            "{context}: half-applied message {m}"
        );
        if *acked {
            assert!(
                array(m, "deliveries")
                    .iter()
                    .any(|d| !d["ackedAt"].is_null()),
                "{context}: acknowledged ack of {id} lost: {m}"
            );
        }
        let again = call(
            &format!("{MSG}Send"),
            json!({"to":"owner","body":m["body"],"requestId":request,
                "kind":"MESSAGE_KIND_REQUEST"}),
        );
        assert!(
            again["duplicate"] == true && text(&again["message"]["id"]) == id,
            "{context}: request id of {id} forgotten: {again}"
        );
    }

    let listed = call(&format!("{NOTE}ListNotes"), json!({}));
    for note in array(&listed, "notes") {
        assert!(
            !text(&note["id"]).is_empty() && text(&note["title"]).contains("-w"),
            "{context}: half-applied note {note}"
        );
    }
    for (id, titles, may_be_gone) in &ledger.notes {
        match hub
            .client
            .call(&format!("{NOTE}GetNote"), &json!({"id":id}))
        {
            Ok(note) => assert!(
                titles.iter().any(|t| note["note"]["title"] == *t),
                "{context}: note {id} has {note}, expected one of {titles:?}"
            ),
            Err(e) => assert!(*may_be_gone, "{context}: acknowledged note {id} lost: {e}"),
        }
    }
    for id in &ledger.deleted_notes {
        assert!(
            hub.client
                .call(&format!("{NOTE}GetNote"), &json!({"id":id}))
                .is_err(),
            "{context}: deleted note {id} is back"
        );
    }

    let tokens = call(&format!("{TOKEN}ListTokens"), json!({}));
    for (id, revoked) in &ledger.tokens {
        let token = array(&tokens, "tokens")
            .iter()
            .find(|t| text(&t["id"]) == id)
            .unwrap_or_else(|| panic!("{context}: token {id} lost"));
        assert!(
            !*revoked || !token["revokedAt"].is_null(),
            "{context}: revoked token {id} is live again"
        );
    }
    let devices = call(&format!("{DEVICE}ListDevices"), json!({}));
    for (id, name, token) in &ledger.devices {
        let device = array(&devices, "devices")
            .iter()
            .find(|d| text(&d["id"]) == id)
            .unwrap_or_else(|| panic!("{context}: added device {id} lost"));
        assert_eq!(device["name"], *name, "{context}");
        Client::new(&hub.plain, token.as_str(), Duration::from_secs(20))
            .unwrap()
            .call("/protectorate.v1.HubService/GetStatus", &json!({}))
            .unwrap_or_else(|error| panic!("{context}: device {id} token refused: {error:?}"));
    }
    for name in &ledger.sleeves {
        let sleeve = call(&format!("{SLEEVE}GetSleeve"), json!({"name":name}));
        assert_eq!(text(&sleeve["sleeve"]["name"]), name, "{context}");
    }
    let audit = call(&format!("{AUDIT}VerifyAudit"), json!({}));
    assert_eq!(audit["ok"], true, "{context}: audit chain: {audit}");
}

/// Every archive the scheduled and manual backups left restores and starts.
fn check_backups(dir: &Path, scratch: &Path) -> usize {
    let mut restored = 0;
    let mut archives = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pbackup"))
        .collect::<Vec<_>>();
    archives.sort();
    for (i, archive) in archives.iter().enumerate() {
        let home = scratch.join(format!("restore-{i}"));
        let output = Command::new(binary())
            .args(["hub", "restore"])
            .arg(archive)
            .arg("--home")
            .arg(&home)
            .env_remove("PROTECTORATE_HOME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "backup {} does not restore: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        check_files(&home, None);
        restored += 1;
    }
    restored
}

#[test]
fn acknowledged_mutations_survive_hub_sigkill_at_random_points() {
    let seed = seed();
    let mut rng = Rng(seed);
    let iterations: u64 = std::env::var("PROTECTORATE_CRASH_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let root = tempfile::tempdir().unwrap();
    let backups = root.path().join("backups");
    let mut hub = Hub::start_with(
        root.path(),
        &[
            "--backup-dir",
            backups.to_str().unwrap(),
            "--backup-every",
            "1s",
        ],
    );
    let work = root.path().join("work");
    fs::create_dir_all(&work).unwrap();

    let ledger = Mutex::new(Ledger::default());
    // Spawning is slow enough that few land inside a kill window.
    let seeding = Ctx {
        client: hub.client.clone(),
        work: work.clone(),
    };
    for n in 0..3 {
        mutate(&seeding, &ledger, &format!("seed-n{n}"), n, 6);
    }
    for iteration in 0..iterations {
        let delay = Duration::from_millis(150 + rng.below(700));
        eprintln!("crash_consistency: iteration {iteration}: kill after {delay:?}");
        let ctx = Ctx {
            client: hub.client.clone(),
            work: work.clone(),
        };
        let stop = AtomicBool::new(false);
        let worker_seeds = (0..6).map(|_| rng.next() | 1).collect::<Vec<_>>();
        thread::scope(|scope| {
            for (worker, worker_seed) in worker_seeds.into_iter().enumerate() {
                let (ctx, ledger, stop) = (&ctx, &ledger, &stop);
                scope.spawn(move || {
                    let mut rng = Rng(worker_seed);
                    let mut n = 0;
                    while !stop.load(Ordering::Relaxed) {
                        let tag = format!("i{iteration}-w{worker}-n{n}");
                        mutate(ctx, ledger, &tag, n, rng.next());
                        n += 1;
                    }
                });
            }
            // A manual backup races the scheduled ones and the kill.
            let home = hub.home.clone();
            scope.spawn(|| {
                let home = home;
                let _ = Command::new(binary())
                    .args(["hub", "backup", "--to"])
                    .arg(&backups)
                    .arg("--home")
                    .arg(&home)
                    .env_remove("PROTECTORATE_HOME")
                    .output();
            });
            thread::sleep(delay);
            hub.stop();
            stop.store(true, Ordering::Relaxed);
        });
        hub.restart();
        let mut ledger = ledger.lock().unwrap();
        verify(
            &hub,
            &mut ledger,
            &format!("seed {seed} iteration {iteration}"),
        );
    }
    let ledger = ledger.lock().unwrap();
    eprintln!(
        "crash_consistency: {} mails ({} acked), {} notes, {} tokens, {} devices, {} sleeves acknowledged",
        ledger.mails.len(),
        ledger.mails.iter().filter(|m| m.3).count(),
        ledger.notes.len(),
        ledger.tokens.len(),
        ledger.devices.len(),
        ledger.sleeves.len()
    );
    hub.stop();
    let restored = check_backups(&backups, root.path());
    eprintln!("crash_consistency: {restored} backup archives restored");
}

/// What a power cut leaves behind that SIGKILL cannot: a half-written final
/// line in each append log, and temp files of writers that never finished.
#[test]
fn torn_log_tails_and_stray_temp_files_do_not_stop_the_hub() {
    let root = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(root.path());
    let mail = hub.call(
        &format!("{MSG}Send"),
        json!({"to":"owner","body":"mail body before the tear","requestId":"torn-tail-mail"}),
    );
    hub.call(&format!("{MSG}Ack"), json!({"id":mail["message"]["id"]}));
    let note = hub.call(
        &format!("{NOTE}CreateNote"),
        json!({"title":"note before the tear","body":"kept"}),
    );
    hub.stop();

    let append = |path: &Path, bytes: &[u8]| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(bytes).unwrap();
    };
    append(
        &hub.home.join("mailbox.jsonl"),
        br#"{"op":"send","at":"2026-10-25T00:00:00Z","msg":{"id":"m99","thr"#,
    );
    let audit = fs::read_dir(hub.home.join("audit"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .max()
        .expect("the hub audits its own mutations");
    append(&audit, br#"{"seq":99,"prev":"ab"#);
    // Writers that died mid-replacement, and one whose process still lives.
    let dead = {
        let mut child = Command::new("true").spawn().unwrap();
        child.wait().unwrap();
        child.id()
    };
    let dead_temps = [
        hub.home.join(format!(".identity.json.{dead}.3")),
        hub.home.join(format!(".notes.json.{dead}.4")),
    ];
    for temp in &dead_temps {
        fs::create_dir_all(temp.parent().unwrap()).unwrap();
        fs::write(temp, br#"{"half":"written"#).unwrap();
    }
    let live_temp = hub
        .home
        .join(format!(".notes.json.{}.6", std::process::id()));
    fs::write(&live_temp, b"a writer that is still running").unwrap();

    hub.restart();
    let thread = hub.call(
        &format!("{MSG}Thread"),
        json!({"threadId":mail["message"]["threadId"]}),
    );
    assert_eq!(thread["messages"][0]["body"], "mail body before the tear");
    assert!(
        !thread["messages"][0]["deliveries"][0]["ackedAt"].is_null(),
        "{thread}"
    );
    hub.call(&format!("{NOTE}GetNote"), json!({"id":note["note"]["id"]}));
    // Both logs take new records after the cut (the first audited write
    // drops the torn audit line) and survive another restart.
    let again = hub.call(
        &format!("{MSG}Send"),
        json!({"to":"owner","body":"mail body after the tear","requestId":"torn-tail-after"}),
    );
    hub.stop();
    hub.restart();
    hub.call(
        &format!("{MSG}Thread"),
        json!({"threadId":again["message"]["threadId"]}),
    );
    let audit_ok = hub.call(&format!("{AUDIT}VerifyAudit"), json!({}));
    assert_eq!(audit_ok["ok"], true, "{audit_ok}");
    check_files(&hub.home, Some(std::process::id()));

    for temp in &dead_temps {
        assert!(
            !temp.exists(),
            "a dead writer's temp file survives a restart: {}",
            temp.display()
        );
    }
    assert!(live_temp.exists(), "a live writer's temp file was removed");
}
