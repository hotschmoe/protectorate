//! The history index end to end: real Pi sleeves on the local model write their native session
//! logs, the hub tails them into its index, and the real CLI searches and reads it as the owner
//! and as a sleeve. Then the index is corrupted (in a copy) and the hub rebuilds it on restart.

mod common;

use std::fs;
use std::process::Command;
use std::time::Duration;

use common::*;
use pt_json::Value;

const ALPHA_WORD: &str = "CORMORANT-7731";
const BETA_WORD: &str = "PELICAN-ORBIT-42";

fn hits(v: &Value) -> Vec<Value> {
    v["hits"].as_array().map(<[_]>::to_vec).unwrap_or_default()
}

/// The owner's hits for `q`, once there are any.
fn found(hub: &Hub, q: &str, secs: u64) -> Vec<Value> {
    eventually(
        &format!("{q} in the history"),
        Duration::from_secs(secs),
        || {
            let h = hits(&hub.cli(&["--json", "history", "search", q]).json());
            (!h.is_empty()).then_some(h)
        },
    )
}

fn roles<'a>(hits: &'a [Value], role: &str) -> Vec<&'a Value> {
    hits.iter()
        .filter(|h| h["doc"]["role"].as_str() == Some(role))
        .collect()
}

#[test]
fn history_search_scope_and_rebuild() {
    if !have_runtime() {
        return;
    }
    let mut hub = Hub::start("history");
    let (repo_a, repo_b) = (temp_repo("hist-a"), temp_repo("hist-b"));
    let alpha = hub.spawn_pi("alpha", &repo_a);
    hub.spawn_pi("beta", &repo_b);
    let alpha_id = alpha["id"].as_str().unwrap().to_string();
    for text in [
        format!("Remember the code word {ALPHA_WORD} for later. Reply with exactly OK."),
        READY.to_string(),
        "What was the code word? Reply with it and nothing else.".to_string(),
    ] {
        hub.send("alpha", &text);
        hub.wait("alpha", "idle", 180);
    }
    hub.send(
        "beta",
        &format!("Remember the code word {BETA_WORD}. Reply with exactly OK."),
    );
    hub.wait("beta", "idle", 180);

    // The owner finds alpha's phrase, from Pi's own session log, once (the transcript copy of
    // the same session is not indexed again).
    let h = found(&hub, ALPHA_WORD, 30);
    assert!(
        h.iter()
            .all(|x| x["doc"]["sleeve"].as_str() == Some("alpha")),
        "{h:?}"
    );
    let users = roles(&h, "user");
    assert_eq!(users.len(), 1, "the prompt is indexed once: {h:?}");
    let doc = &users[0]["doc"];
    assert!(
        doc["path"]
            .as_str()
            .unwrap()
            .contains(&format!("sleeves/{alpha_id}/home/.pi/agent/sessions/")),
        "the native log is the source: {doc:?}"
    );
    assert_eq!(doc["harness"].as_str(), Some("pi_local"));
    assert_eq!(doc["turn"].as_u64(), Some(1));
    let human = hub.cli(&["history", "search", ALPHA_WORD]);
    assert!(
        human.ok().contains("**cormorant") || human.ok().contains("**CORMORANT"),
        "the human output marks the match: {}",
        human.stdout
    );

    // Read: the doc whole, and the session's first turn.
    let doc_id = doc["doc_id"].as_u64().unwrap().to_string();
    let session = doc["session"].as_str().unwrap().to_string();
    let read = hub
        .cli(&["--json", "history", "read", "--doc", &doc_id])
        .json();
    assert!(
        read["docs"][0]["text"]
            .as_str()
            .unwrap()
            .contains(ALPHA_WORD),
        "{read:?}"
    );
    let turn = hub
        .cli(&[
            "--json", "history", "read", &session, "--from", "1", "--to", "1",
        ])
        .json();
    let turn_docs = turn["docs"].as_array().unwrap();
    assert!(
        turn_docs
            .iter()
            .any(|d| d["doc"]["role"].as_str() == Some("assistant"))
            && turn_docs
                .iter()
                .all(|d| d["doc"]["turn"].as_u64() == Some(1)),
        "turn 1 has the prompt and the reply: {turn:?}"
    );

    // Scope: alpha's own capability reaches alpha's history and nothing of beta's.
    let beta_hits = found(&hub, BETA_WORD, 30);
    let beta_doc = beta_hits[0]["doc"]["doc_id"].as_u64().unwrap().to_string();
    let token = hub.sleeve_token(&alpha_id, 1);
    let as_alpha = |args: &[&str]| hub.cli_env(&[("PROTECTORATE_TOKEN", token.as_str())], args);
    assert!(!hits(&as_alpha(&["--json", "history", "search", ALPHA_WORD]).json()).is_empty());
    assert!(
        hits(&as_alpha(&["--json", "history", "search", BETA_WORD]).json()).is_empty(),
        "a sleeve does not see another sleeve's history"
    );
    let r = as_alpha(&["history", "search", "--sleeve", "beta", BETA_WORD]);
    assert_ne!(r.status, 0);
    assert!(r.stderr.contains("outside this capability"), "{}", r.stderr);
    let r = as_alpha(&["history", "read", "--doc", &beta_doc]);
    assert_ne!(r.status, 0);
    assert!(r.stderr.contains("no history doc"), "{}", r.stderr);
    let r = as_alpha(&["history", "search", "--mode", "any", BETA_WORD, "OK"]);
    assert!(!r.ok().contains("beta"), "{}", r.stdout);

    // D26: a parent sleeve does not see its child's runs.
    const CHILD_WORD: &str = "TERN-9144";
    let kid = hub
        .cli(&[
            "--json",
            "sleeve",
            "spawn",
            "tern",
            "--parent",
            "alpha",
            "--harness",
            "pi_local",
        ])
        .json();
    assert_eq!(kid["role"].as_str(), Some("child"), "{kid:?}");
    hub.wait("tern", "idle", 120);
    hub.send(
        "tern",
        &format!("Remember the code word {CHILD_WORD}. Reply with exactly OK."),
    );
    hub.wait("tern", "idle", 180);
    let child_hits = found(&hub, CHILD_WORD, 30);
    assert!(
        child_hits
            .iter()
            .all(|x| x["doc"]["sleeve"].as_str() == Some("tern")),
        "{child_hits:?}"
    );
    assert!(
        hits(&as_alpha(&["--json", "history", "search", CHILD_WORD]).json()).is_empty(),
        "a parent does not see its child's history"
    );
    let r = as_alpha(&["history", "search", "--sleeve", "tern", CHILD_WORD]);
    assert_ne!(r.status, 0);
    assert!(r.stderr.contains("outside this capability"), "{}", r.stderr);
    let child_doc = child_hits[0]["doc"]["doc_id"].as_u64().unwrap().to_string();
    let r = as_alpha(&["history", "read", "--doc", &child_doc]);
    assert_ne!(r.status, 0);
    assert!(r.stderr.contains("no history doc"), "{}", r.stderr);

    // Corrupt a copy of the index and point the hub at it: the hub starts, moves the copy aside
    // and rebuilds it from the sources. The original is left as it was.
    hub.stop();
    let index = hub.home.join("history");
    let original = fs::read(index.join("manifest")).expect("the index manifest");
    let copy = hub.home.with_file_name("history-copy");
    let cp = Command::new("cp")
        .arg("-a")
        .arg(&index)
        .arg(&copy)
        .status()
        .expect("cp");
    assert!(cp.success());
    fs::write(copy.join("manifest"), b"PTHISTMF 00000000\nnot json").unwrap();
    let config_path = hub.home.join("config.json");
    let mut config = pt_json::parse(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .insert("history_dir", Value::from(copy.to_str().unwrap()));
    fs::write(&config_path, config.to_string()).unwrap();
    hub.restart();

    let h = found(&hub, ALPHA_WORD, 60);
    assert_eq!(roles(&h, "user").len(), 1, "rebuilt once: {h:?}");
    assert!(
        hub.log_text().contains("rebuilding it from the sources"),
        "{}",
        tail(&hub.log_text(), 30)
    );
    let rebuilt = fs::read_to_string(copy.join("manifest")).unwrap();
    assert!(rebuilt.starts_with("PTHISTMF ") && rebuilt.contains("\"sources\""));
    let aside: Vec<String> = fs::read_dir(copy.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("history-copy.corrupt-"))
        .collect();
    assert_eq!(aside.len(), 1, "the corrupt copy is kept aside: {aside:?}");
    assert_eq!(
        fs::read(index.join("manifest")).unwrap(),
        original,
        "the original index is untouched"
    );
}

#[test]
#[cfg(debug_assertions)]
fn failed_history_commit_retries_without_new_transcript_data() {
    use std::io::Write;

    let arm = unique("history-arm");
    let hub = Hub::start_with(
        "history-retry",
        "pi_local",
        &[],
        &[
            ("PT_TEST_HISTORY_FAIL_COMMIT", "manifest"),
            ("PT_TEST_HISTORY_FAIL_ARM", arm.to_str().unwrap()),
        ],
    );
    let transcript = hub.home.join("sleeves/0/transcript.jsonl");
    eventually("envoy transcript", Duration::from_secs(10), || {
        transcript.is_file().then_some(())
    });
    std::thread::sleep(Duration::from_secs(2));
    fs::write(&arm, "armed").unwrap();
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap();
    writeln!(file, "{{\"seq\":10000,\"g\":1,\"time_ms\":1000,\"entry\":{{\"type\":\"user_message\",\"request_id\":\"fixture\",\"from\":\"owner\",\"text\":\"historyretryquartz\"}}}}").unwrap();
    drop(file);
    eventually(
        "the injected commit failure",
        Duration::from_secs(15),
        || {
            hub.log_text()
                .contains("history commit: injected history manifest commit failure")
                .then_some(())
        },
    );
    let len = fs::metadata(&transcript).unwrap().len();
    let h = found(&hub, "historyretryquartz", 15);
    assert_eq!(roles(&h, "user").len(), 1, "{h:?}");
    assert_eq!(
        fs::metadata(transcript).unwrap().len(),
        len,
        "retry did not need new transcript bytes"
    );
}

/// Indexed user text is clipped to 32 KiB, so the fixture is many lines.
const INDEXED_TEXT: usize = 32 << 10;

fn user_line(seq: u64, text: &str) -> String {
    format!(
        r#"{{"seq":{seq},"g":1,"time_ms":{seq},"entry":{{"type":"user_message","request_id":"fixture-{seq}","from":"owner","text":"{text}"}}}}"#
    )
}

#[test]
fn history_budget_under_a_mebibyte_is_refused() {
    let home = unique("hbudget").join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("config.json"), r#"{"history_budget_bytes":0}"#).unwrap();
    let out = Command::new(bin())
        .args(["--home", home.to_str().unwrap(), "serve", "--listen", "off"])
        .output()
        .expect("run serve");
    assert!(
        !out.status.success(),
        "serve started with history_budget_bytes 0"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    let floor = 1u64 << 20;
    assert!(
        err.contains(&format!(
            "history_budget_bytes must be at least {floor} bytes"
        )),
        "{err}"
    );
}

#[test]
fn history_budget_in_config_caps_the_sleeve() {
    let floor = 1u64 << 20;
    let hub = Hub::start_config(
        "hcap",
        "pi_local",
        &[
            ("history_budget_bytes", Value::from(floor)),
            ("scopes", Value::from(false)),
        ],
        &[],
        &[],
    );
    let id = eventually("envoy", Duration::from_secs(15), || {
        let sleeve = hub.sleeve("envoy");
        let id = sleeve["id"].as_str()?;
        (!id.is_empty()).then(|| id.to_string())
    });
    let transcript = hub.home.join(format!("sleeves/{id}/transcript.jsonl"));
    if let Some(dir) = transcript.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    let oldest = format!("quartzoldestline {}", "x".repeat(INDEXED_TEXT));
    let newest = format!("quartznewestline {}", "y".repeat(INDEXED_TEXT));
    let fill = "m".repeat(INDEXED_TEXT);
    // One more full clip than the floor can hold, plus overhead, so the oldest line is evicted.
    let n = (floor as usize / INDEXED_TEXT) + 8;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&transcript)
        .unwrap();
    use std::io::Write;
    writeln!(file, "{}", user_line(10_000, &oldest)).unwrap();
    for seq in 1..n - 1 {
        writeln!(file, "{}", user_line(10_000 + seq as u64, &fill)).unwrap();
    }
    writeln!(file, "{}", user_line(10_000 + n as u64, &newest)).unwrap();
    drop(file);
    found(&hub, "quartznewestline", 20);
    let gone = hits(
        &hub.cli(&["--json", "history", "search", "quartzoldestline"])
            .json(),
    );
    assert!(gone.is_empty(), "the oldest line stayed: {gone:?}");
    let info = hub.cli(&["hub", "info"]);
    assert!(
        info.ok().contains(&format!("history capped: {id}")),
        "{}",
        info.stdout
    );
}
