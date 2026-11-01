//! End-to-end: the real `pt-quota-probe` binary, the real `curl`, real sockets and login files.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const PROBE: &str = env!("CARGO_BIN_EXE_pt-quota-probe");
const STATES: [&str; 4] = ["fresh", "stale", "unknown", "error"];

fn probe(args: &[&str]) -> String {
    probe_env(args, &[])
}

fn probe_env(args: &[&str], env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(PROBE);
    cmd.args(args).env_remove("PT_LOCAL_URL");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run probe");
    assert!(
        out.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("probe output is UTF-8")
}

fn line_of<'a>(out: &'a str, name: &str) -> &'a str {
    out.lines()
        .find(|l| l.split(' ').next() == Some(name))
        .unwrap_or_else(|| panic!("no {name} line in:\n{out}"))
}

fn state_of(line: &str) -> &str {
    let state = line.split(' ').nth(1).unwrap_or("");
    assert!(STATES.contains(&state), "bad state in {line:?}");
    state
}

/// This machine's own logins: every configured subscription yields a reading line, and a fresh
/// one carries windows. Unknown and error are allowed (offline, expired login) and printed.
#[test]
fn live_logins_each_return_a_reading() {
    let home = std::env::var("HOME").expect("HOME");
    let out = probe(&[]);
    println!("{out}");
    for (name, login) in [
        ("claude", ".claude/.credentials.json"),
        ("codex", ".codex/auth.json"),
        ("grok", ".grok/auth.json"),
    ] {
        let line = line_of(&out, name);
        let state = state_of(line);
        if Path::new(&home).join(login).exists() {
            if state == "fresh" {
                assert!(
                    line.contains("plan=") && line.contains(" ["),
                    "fresh without windows: {line}"
                );
            }
        } else {
            assert_eq!(state, "unknown", "{line}");
        }
    }
    state_of(line_of(&out, "local"));
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Serves canned bodies by path and records each request's Authorization header. A path under
/// a `guarded` prefix answers 401 unless the request carries that bearer token.
fn serve(routes: Vec<(&'static str, &'static str)>) -> (u16, Arc<Mutex<Vec<String>>>) {
    serve_guarded(routes, &[])
}

fn serve_guarded(
    routes: Vec<(&'static str, &'static str)>,
    guarded: &'static [(&'static str, &'static str)],
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            let text = String::from_utf8_lossy(&req).into_owned();
            let path = text.split(' ').nth(1).unwrap_or("").to_string();
            let mut auth = None;
            for l in text
                .lines()
                .filter(|l| l.to_lowercase().starts_with("authorization:"))
            {
                log.lock().unwrap().push(format!("{path} {l}"));
                auth = l.split_once(':').map(|(_, v)| v.trim().to_string());
            }
            let denied = guarded.iter().any(|(p, token)| {
                path.starts_with(p) && auth.as_deref() != Some(format!("Bearer {token}").as_str())
            });
            let body = routes
                .iter()
                .find(|(p, _)| path.starts_with(p))
                .map(|(_, b)| *b);
            let reply = match body {
                _ if denied => {
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                }
                Some(b) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                    b.len()
                ),
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string(),
            };
            let _ = s.write_all(reply.as_bytes());
        }
    });
    (port, seen)
}

/// Fixture logins and a fake provider on a real socket: each reader reads its login file, sends
/// the bearer token over curl, and parses the response; an expired login, a missing login and a
/// dead endpoint are reported as error and unknown; no token is ever printed.
#[test]
fn fixture_readers_over_real_curl() {
    let home = scratch("fixture-home");
    let far = "4102444800000"; // 2100-01-01
    write(
        &home.join(".claude/.credentials.json"),
        &format!(
            r#"{{"claudeAiOauth":{{"accessToken":"claude-secret-token","expiresAt":{far},"subscriptionType":"max"}}}}"#
        ),
    );
    write(
        &home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"codex-secret-token","account_id":"acct-1"}}"#,
    );
    // No Grok login: unknown.
    let (port, seen) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":30.5,"resets_at":"2100-01-01T00:00:00Z"},"seven_day":{"utilization":10,"resets_at":"2100-01-02T00:00:00.5+00:00"},"seven_day_sonnet":null}"#,
        ),
        (
            "/wham/usage",
            r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":7,"limit_window_seconds":18000,"reset_at":4102444800},"secondary_window":{"used_percent":12,"limit_window_seconds":604800,"reset_at":4102444800}}}"#,
        ),
        (
            "/v1/models",
            r#"{"data":[{"id":"model-a"},{"id":"llama"}]}"#,
        ),
    ]);
    let base = format!("http://127.0.0.1:{port}");
    let home_s = home.to_str().unwrap();
    let out = probe(&[
        "--home",
        home_s,
        "--claude-url",
        &base,
        "--codex-url",
        &base,
        "--grok-url",
        &base,
        "--local",
        &base,
    ]);
    println!("{out}");
    let claude = line_of(&out, "claude");
    assert_eq!(state_of(claude), "fresh");
    assert!(
        claude.contains("plan=max") && claude.contains("[5h 30%") && claude.contains("[wk 10%"),
        "{claude}"
    );
    let codex = line_of(&out, "codex");
    assert_eq!(state_of(codex), "fresh");
    assert!(
        codex.contains("plan=plus") && codex.contains("[5h 7%") && codex.contains("[wk 12%"),
        "{codex}"
    );
    let grok = line_of(&out, "grok");
    assert_eq!(state_of(grok), "unknown", "{grok}");
    let local = line_of(&out, "local");
    assert_eq!(state_of(local), "fresh");
    assert!(local.contains("model-a, llama"), "{local}");
    assert!(out.lines().any(|l| l.starts_with("target ")));

    assert!(
        !out.contains("secret-token"),
        "a token leaked into the output"
    );
    let seen = seen.lock().unwrap().join("\n");
    assert!(
        seen.contains("/api/oauth/usage authorization: Bearer claude-secret-token")
            || seen.contains("Authorization: Bearer claude-secret-token"),
        "{seen}"
    );
    assert!(seen.contains("codex-secret-token"), "{seen}");

    // Expired Claude login: the reader refuses and does not call out; a dead local endpoint errors.
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"claude-secret-token","expiresAt":1000,"subscriptionType":"max"}}"#,
    );
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_url = format!("http://127.0.0.1:{}", dead.local_addr().unwrap().port());
    drop(dead);
    let out = probe(&[
        "--home",
        home_s,
        "--claude-url",
        &base,
        "--codex-url",
        &base,
        "--local",
        &dead_url,
    ]);
    println!("{out}");
    let claude = line_of(&out, "claude");
    assert_eq!(state_of(claude), "error");
    assert!(claude.contains("expired"), "{claude}");
    assert_eq!(state_of(line_of(&out, "local")), "error");
    assert!(!out.contains("secret-token"));
}

/// The local endpoint behind a bearer token, as the owner's is: the reader finds the key and the
/// base URL in a fixture `~/.pi/agent/models.json` (provider named by `--pi-provider`), sends the
/// key over curl, and gets a fresh reading; a wrong key is a 401 error; an `apiKey` naming a set
/// environment variable sends that variable's value; a `!command` key is never run. No key is
/// ever printed.
#[test]
fn local_reader_sends_the_pi_provider_key() {
    let (port, seen) = serve_guarded(
        vec![("/v1/models", r#"{"data":[{"id":"qwen-local"}]}"#)],
        &[("/v1/models", "local-secret-key")],
    );
    let base = format!("http://127.0.0.1:{port}/v1");
    let models = |home: &Path, key: &str| {
        write(
            &home.join(".pi/agent/models.json"),
            &format!(
                r#"{{"providers":{{"other":{{"baseUrl":"http://127.0.0.1:1/v1","apiKey":"other-secret-key"}},"lab":{{"api":"openai-completions","baseUrl":"{base}","apiKey":"{key}","models":[{{"id":"qwen-local"}}]}}}}}}"#
            ),
        );
    };
    let run = |home: &Path, env: &[(&str, &str)]| {
        let out = probe_env(
            &["--home", home.to_str().unwrap(), "--pi-provider", "lab"],
            env,
        );
        println!("{out}");
        assert!(!out.contains("secret-key"), "a key leaked: {out}");
        line_of(&out, "local").to_string()
    };

    let home = scratch("local-key-home");
    models(&home, "local-secret-key");
    let local = run(&home, &[]);
    assert_eq!(state_of(&local), "fresh", "{local}");
    assert!(local.contains("qwen-local"), "{local}");

    models(&home, "wrong-secret-key");
    let local = run(&home, &[]);
    assert_eq!(state_of(&local), "error", "{local}");
    assert!(local.contains("401"), "{local}");

    models(&home, "PT_E2E_LOCAL_KEY");
    let local = run(&home, &[("PT_E2E_LOCAL_KEY", "local-secret-key")]);
    assert_eq!(state_of(&local), "fresh", "{local}");

    models(&home, "!cat /nonexistent");
    let local = run(&home, &[]);
    assert_eq!(state_of(&local), "unknown", "{local}");
    assert!(local.contains("command"), "{local}");

    let seen = seen.lock().unwrap().join("\n");
    assert!(
        seen.contains("Bearer local-secret-key") && seen.contains("Bearer wrong-secret-key"),
        "{seen}"
    );
    assert!(
        !seen.contains("other-secret-key"),
        "only the named provider's key is sent"
    );
}
