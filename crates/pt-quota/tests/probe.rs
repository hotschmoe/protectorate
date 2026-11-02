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
fn serve<S: Into<String>>(routes: Vec<(&'static str, S)>) -> (u16, Arc<Mutex<Vec<String>>>) {
    serve_guarded(routes, &[])
}

fn serve_guarded<S: Into<String>>(
    routes: Vec<(&'static str, S)>,
    guarded: &'static [(&'static str, &'static str)],
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let routes: Vec<(&'static str, String)> = routes
        .into_iter()
        .map(|(path, body)| (path, body.into()))
        .collect();
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
                .map(|(_, b)| b.clone());
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
/// the bearer token over curl, and parses the response; an expired login is unknown, a missing
/// login is unknown, and a dead endpoint is an error; no token is ever printed.
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
    assert_eq!(state_of(claude), "unknown", "{claude}");
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

#[test]
fn partial_responses_and_exhausted_selection() {
    let home = scratch(&format!("partial-quota-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture-claude"}}"#,
    );
    write(
        &home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"fixture-codex"}}"#,
    );
    write(
        &home.join(".grok/auth.json"),
        r#"{"https://auth.x.ai::fixture":{"key":"fixture-grok"}}"#,
    );
    let (full_port, _) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":99},"seven_day":{"utilization":10}}"#,
        ),
        (
            "/wham/usage",
            r#"{"rate_limit":{"primary_window":{"used_percent":98,"limit_window_seconds":18000},"secondary_window":{"used_percent":10,"limit_window_seconds":604800}}}"#,
        ),
        ("/v1/billing", r#"{"config":{"creditUsagePercent":100}}"#),
    ]);
    let full = format!("http://127.0.0.1:{full_port}");
    let home_s = home.to_str().unwrap();
    let args = |base: &str| {
        vec![
            "--home".to_string(),
            home_s.to_string(),
            "--claude-url".into(),
            base.into(),
            "--codex-url".into(),
            base.into(),
            "--grok-url".into(),
            base.into(),
            "--local".into(),
            base.into(),
            "--allow".into(),
            "claude,codex,grok".into(),
        ]
    };
    let run = |args: Vec<String>| probe(&args.iter().map(String::as_str).collect::<Vec<_>>());
    let out = run(args(&full));
    for name in ["claude", "codex", "grok"] {
        assert_eq!(state_of(line_of(&out, name)), "fresh", "{out}");
    }
    assert!(
        out.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{out}"
    );

    let (mixed_port, _) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":99},"seven_day":{"utilization":10}}"#,
        ),
        (
            "/wham/usage",
            r#"{"rate_limit":{"primary_window":{"used_percent":"n/a","limit_window_seconds":18000}}}"#,
        ),
        ("/v1/billing", r#"{"config":{"creditUsagePercent":100}}"#),
    ]);
    let mixed = format!("http://127.0.0.1:{mixed_port}");
    let out = run(args(&mixed));
    assert_eq!(state_of(line_of(&out, "codex")), "error", "{out}");
    assert!(
        out.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{out}"
    );

    let (partial_port, _) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":"n/a"},"seven_day":{"utilization":10}}"#,
        ),
        (
            "/wham/usage",
            r#"{"rate_limit":{"primary_window":{"used_percent":99,"limit_window_seconds":18000},"secondary_window":{"used_percent":-1,"limit_window_seconds":604800}}}"#,
        ),
        ("/v1/billing", r#"{"config":{}}"#),
        ("/v1/models", r#"{"data":[]}"#),
    ]);
    let partial = format!("http://127.0.0.1:{partial_port}");
    let out = run(args(&partial));
    for (name, reason) in [
        ("claude", "5h"),
        ("codex", "secondary_window"),
        ("grok", "credit usage"),
        ("local", "model ids"),
    ] {
        let line = line_of(&out, name);
        assert_eq!(state_of(line), "error", "{line}");
        assert!(line.contains(reason), "{line}");
    }
    assert!(
        out.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{out}"
    );
}

#[test]
fn local_endpoint_rejects_non_loopback_http_and_notes_an_unlisted_model() {
    let home = scratch(&format!("unsafe-local-{}", std::process::id()));
    write(
        &home.join(".pi/agent/models.json"),
        r#"{"providers":{"lab":{"apiKey":"fixture-secret"}}}"#,
    );
    let home_s = home.to_str().unwrap();
    let out = probe(&[
        "--home",
        home_s,
        "--pi-provider",
        "lab",
        "--local",
        "http://192.0.2.1:1234",
    ]);
    let local = line_of(&out, "local");
    assert_eq!(state_of(local), "error", "{local}");
    assert!(
        local.contains("misconfigured") && local.contains("loopback"),
        "{local}"
    );
    assert!(!out.contains("fixture-secret"));
    let (port, _) = serve(vec![("/v1/models", r#"{"data":[{"id":"other"}]}"#)]);
    let base = format!("http://localhost:{port}");
    let out = probe(&[
        "--home",
        home_s,
        "--pi-provider",
        "lab",
        "--local",
        &base,
        "--pi-model",
        "wanted",
    ]);
    let local = line_of(&out, "local");
    assert_eq!(state_of(local), "fresh", "{local}");
    assert!(local.contains("\"wanted\" is not listed"), "{local}");
}

#[test]
fn fifo_login_does_not_block_other_readers() {
    let home = scratch(&format!("fifo-quota-{}", std::process::id()));
    let login = home.join(".claude/.credentials.json");
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&login)
            .status()
            .unwrap()
            .success()
    );
    let (port, _) = serve(vec![("/v1/models", r#"{"data":[{"id":"ready"}]}"#)]);
    let base = format!("http://127.0.0.1:{port}");
    let start = std::time::Instant::now();
    let out = probe(&["--home", home.to_str().unwrap(), "--local", &base]);
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "{out}");
    assert!(
        line_of(&out, "claude").contains("not a regular file"),
        "{out}"
    );
    assert_eq!(state_of(line_of(&out, "local")), "fresh", "{out}");
}

#[test]
#[cfg(debug_assertions)]
fn cache_uses_monotonic_time_for_ttl_backoff_and_age() {
    let home = scratch(&format!("cache-clock-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"clock-claude"}}"#,
    );
    write(
        &home.join(".grok/auth.json"),
        r#"{"https://auth.x.ai::fixture":{"key":"clock-grok"}}"#,
    );
    let (port, seen) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":10},"seven_day":{"utilization":20}}"#,
        ),
        ("/v1/billing", r#"{"config":{}}"#),
    ]);
    let base = format!("http://127.0.0.1:{port}");
    let out = probe(&[
        "--home",
        home.to_str().unwrap(),
        "--claude-url",
        &base,
        "--grok-url",
        &base,
        "--test-cache-ms",
        "150",
    ]);
    assert_eq!(state_of(line_of(&out, "claude")), "fresh", "{out}");
    assert_eq!(state_of(line_of(&out, "grok")), "error", "{out}");
    assert!(out.contains("aged claude stale"), "{out}");
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.iter()
            .filter(|s| s.contains("/api/oauth/usage"))
            .count(),
        2,
        "{seen:?}"
    );
    assert_eq!(
        seen.iter().filter(|s| s.contains("/v1/billing")).count(),
        2,
        "{seen:?}"
    );
}

/// A stale row keeps the windows of its last good read. Under the usable minimum the fallback
/// does not spend it; at or above the minimum it still may, instead of parking.
#[test]
#[cfg(debug_assertions)]
fn stale_windows_still_gate_the_fallback() {
    let home = scratch(&format!("stale-windows-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"stale-claude"}}"#,
    );
    let home_s = home.to_str().unwrap();
    let run = |body: &'static str| {
        let (port, _) = serve(vec![("/api/oauth/usage", body)]);
        let base = format!("http://127.0.0.1:{port}");
        probe(&[
            "--home",
            home_s,
            "--claude-url",
            &base,
            "--allow",
            "claude",
            "--test-cache-ms",
            "150",
        ])
    };
    let exhausted = run(
        r#"{"five_hour":{"utilization":99,"resets_at":"2100-01-01T00:00:00Z"},"seven_day":{"utilization":10,"resets_at":"2100-01-08T00:00:00Z"}}"#,
    );
    assert!(
        exhausted.contains("aged claude stale") && exhausted.contains("99%"),
        "{exhausted}"
    );
    assert!(
        exhausted.contains(
            "aged target none: all eligible resleeve targets have insufficient fresh quota"
        ),
        "{exhausted}"
    );
    assert!(
        !exhausted.contains("without a fresh quota reading"),
        "{exhausted}"
    );

    let spare = run(
        r#"{"five_hour":{"utilization":0,"resets_at":"2100-01-01T00:00:00Z"},"seven_day":{"utilization":10,"resets_at":"2100-01-08T00:00:00Z"}}"#,
    );
    assert!(spare.contains("aged claude stale"), "{spare}");
    assert!(
        spare.contains(
            "aged target Claude: first eligible in resleeve_to whose last reading still has spare quota"
        ),
        "{spare}"
    );
}

/// A Codex `reset_at` of 0 is not a reset, so a full window still blocks. A reset a few
/// seconds behind the clock blocks on a fresh read, with no backdated cache. A stamp from
/// 1970 was already long past at fetch time, so the window counts as reset. A stamp that
/// has passed with room left is the same.
#[test]
fn a_reset_just_behind_blocks_and_an_ancient_one_counts_as_reset() {
    let home = scratch(&format!("passed-reset-{}", std::process::id()));
    write(
        &home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"passed-codex"}}"#,
    );
    let home_s = home.to_str().unwrap();
    let run = |body: &str| {
        let (port, _) = serve(vec![("/wham/usage", body.to_string())]);
        let base = format!("http://127.0.0.1:{port}");
        probe(&["--home", home_s, "--codex-url", &base, "--allow", "codex"])
    };
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let behind = run(&format!(
        r#"{{"plan_type":"plus","rate_limit":{{"primary_window":{{"used_percent":100,"limit_window_seconds":18000,"reset_at":{secs}}}}}}}"#,
        secs = now_s.saturating_sub(5)
    ));
    let line = line_of(&behind, "codex");
    assert_eq!(state_of(line), "fresh", "{behind}");
    assert!(line.contains("reset passed"), "{line}");
    assert!(
        behind.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{behind}"
    );
    assert!(
        !behind.contains("whose quota windows have reset"),
        "{behind}"
    );

    let zero = run(
        r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":100,"limit_window_seconds":18000,"reset_at":0}}}"#,
    );
    let line = line_of(&zero, "codex");
    assert_eq!(state_of(line), "fresh", "{zero}");
    assert!(
        line.contains("[5h 100%]") && !line.contains("reset passed"),
        "{line}"
    );
    assert!(
        zero.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{zero}"
    );

    let passed = run(
        r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":100,"limit_window_seconds":18000,"reset_at":1}}}"#,
    );
    let line = line_of(&passed, "codex");
    assert!(line.contains("reset passed"), "{line}");
    assert!(
        passed
            .contains("target Codex: first eligible in resleeve_to whose quota windows have reset"),
        "{passed}"
    );

    let room = run(
        r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":0,"limit_window_seconds":18000,"reset_at":1}}}"#,
    );
    assert!(
        room.contains("target Codex: first eligible in resleeve_to whose quota windows have reset"),
        "{room}"
    );
    assert!(!room.contains("without a fresh quota reading"), "{room}");
}

/// A stale row fetched before the reset still blocks inside the ten-minute grace, and counts
/// as reset once that grace has passed.
#[test]
#[cfg(debug_assertions)]
fn a_passed_reset_blocks_only_inside_the_grace() {
    let home = scratch(&format!("reset-grace-{}", std::process::id()));
    write(
        &home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"grace-codex"}}"#,
    );
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let home_s = home.to_str().unwrap();
    let run = |reset_at: u64| {
        let body = format!(
            r#"{{"plan_type":"plus","rate_limit":{{"primary_window":{{"used_percent":100,"limit_window_seconds":18000,"reset_at":{reset_at}}}}}}}"#
        );
        let (port, _) = serve(vec![("/wham/usage", body)]);
        let base = format!("http://127.0.0.1:{port}");
        probe(&[
            "--home",
            home_s,
            "--codex-url",
            &base,
            "--allow",
            "codex",
            "--test-cache-ms",
            "150",
        ])
    };
    let inside = run(now_s.saturating_sub(120));
    assert!(
        inside.contains(
            "aged target none: all eligible resleeve targets have insufficient fresh quota"
        ),
        "{inside}"
    );
    let outside = run(now_s.saturating_sub(30 * 60));
    assert!(
        outside.contains(
            "aged target Codex: first eligible in resleeve_to whose quota windows have reset"
        ),
        "{outside}"
    );
}

/// A local endpoint that answered, then went stale, is not spent. It has no percent windows
/// to fall back on.
#[test]
#[cfg(debug_assertions)]
fn a_stale_local_reading_is_not_spent() {
    let home = scratch(&format!("stale-local-{}", std::process::id()));
    let (port, _) = serve(vec![("/v1/models", r#"{"data":[{"id":"local-model"}]}"#)]);
    let base = format!("http://127.0.0.1:{port}");
    let out = probe(&[
        "--home",
        home.to_str().unwrap(),
        "--local",
        &base,
        "--allow",
        "pi_local",
        "--test-cache-ms",
        "150",
    ]);
    assert!(
        out.contains("local fresh") && out.contains("local-model"),
        "{out}"
    );
    assert!(out.contains("target PiLocal:"), "{out}");
    assert!(out.contains("aged local stale"), "{out}");
    assert!(
        out.contains(
            "aged target none: all eligible resleeve targets have insufficient fresh quota"
        ),
        "{out}"
    );
    assert!(!out.contains("without a fresh quota reading"), "{out}");
}

/// A weekly Sonnet cap is shown on the row and does not exhaust Claude when the account
/// window still has room. The local model is not chosen ahead of that account window.
#[test]
fn a_model_scoped_window_does_not_exhaust_the_subscription() {
    let home = scratch(&format!("scoped-window-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"scoped-claude"}}"#,
    );
    let (port, _) = serve(vec![
        (
            "/api/oauth/usage",
            r#"{"five_hour":{"utilization":0,"resets_at":"2100-01-01T00:00:00Z"},"seven_day_sonnet":{"utilization":100,"resets_at":"2100-01-08T00:00:00Z"}}"#,
        ),
        ("/v1/models", r#"{"data":[{"id":"local-model"}]}"#),
    ]);
    let base = format!("http://127.0.0.1:{port}");
    let home_s = home.to_str().unwrap();
    let run = |allow: &str| {
        probe(&[
            "--home",
            home_s,
            "--claude-url",
            &base,
            "--local",
            &base,
            "--allow",
            allow,
        ])
    };
    let both = run("claude,pi_local");
    let claude = line_of(&both, "claude");
    assert_eq!(state_of(claude), "fresh", "{both}");
    assert!(
        claude.contains("[5h 0%") && claude.contains("sonnet") && claude.contains("100%"),
        "{claude}"
    );
    assert!(
        both.contains("target Claude:") && both.contains("5h"),
        "{both}"
    );
    assert!(!both.contains("target PiLocal"), "{both}");

    let only = run("claude");
    assert!(
        only.contains("target Claude:") && only.contains("5h"),
        "{only}"
    );
    assert!(!only.contains("target none"), "{only}");
}

/// The Opus weekly cap gates a Claude candidate whose model is Opus, and does not gate Sonnet.
/// A row that only has that cap is not spent as if it had no reading.
#[test]
fn a_model_cap_gates_the_candidate_that_runs_that_model() {
    let home = scratch(&format!("model-cap-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"model-cap-claude"}}"#,
    );
    let (port, _) = serve(vec![(
        "/api/oauth/usage",
        r#"{"five_hour":{"utilization":0,"resets_at":"2100-01-01T00:00:00Z"},"seven_day_opus":{"utilization":100,"resets_at":"2100-01-08T00:00:00Z"}}"#,
    )]);
    let base = format!("http://127.0.0.1:{port}");
    let home_s = home.to_str().unwrap();
    let run = |allow: &str| probe(&["--home", home_s, "--claude-url", &base, "--allow", allow]);
    let opus = run("claude:claude-opus-4-6");
    assert!(
        opus.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{opus}"
    );
    assert!(!opus.contains("target Claude:"), "{opus}");
    let line = line_of(&opus, "claude");
    assert!(line.contains("opus") && line.contains("100%"), "{line}");

    let sonnet = run("claude:sonnet");
    assert!(
        sonnet.contains("target Claude:") && sonnet.contains("5h"),
        "{sonnet}"
    );

    let (port, _) = serve(vec![(
        "/api/oauth/usage",
        r#"{"seven_day_opus":{"utilization":100,"resets_at":"2100-01-08T00:00:00Z"}}"#,
    )]);
    let base = format!("http://127.0.0.1:{port}");
    let only = probe(&[
        "--home",
        home_s,
        "--claude-url",
        &base,
        "--allow",
        "claude:sonnet",
    ]);
    assert!(
        only.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{only}"
    );
    assert!(!only.contains("without a fresh quota reading"), "{only}");
}

/// Pi with a vendor login has no quota reader. A missing reading is not "unknown", so the
/// fallback does not spend it once the subscribed account is under the usable minimum.
#[test]
fn an_unmetered_harness_is_not_spent_when_its_reading_is_missing() {
    let home = scratch(&format!("unmetered-{}", std::process::id()));
    write(
        &home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"unmetered-claude"}}"#,
    );
    let (port, _) = serve(vec![(
        "/api/oauth/usage",
        r#"{"five_hour":{"utilization":99,"resets_at":"2100-01-01T00:00:00Z"}}"#,
    )]);
    let base = format!("http://127.0.0.1:{port}");
    let home_s = home.to_str().unwrap();
    let out = probe(&[
        "--home",
        home_s,
        "--claude-url",
        &base,
        "--allow",
        "claude,pi_vendor",
    ]);
    assert_eq!(state_of(line_of(&out, "claude")), "fresh", "{out}");
    assert!(
        out.contains("target none: all eligible resleeve targets have insufficient fresh quota"),
        "{out}"
    );
    assert!(!out.contains("target PiVendor"), "{out}");

    let alone = probe(&["--home", home_s, "--allow", "pi_vendor"]);
    assert!(
        alone.contains("target none: no eligible resleeve target has a quota meter"),
        "{alone}"
    );
    assert!(!alone.contains("insufficient fresh quota"), "{alone}");
}

/// Codex reports a window length in seconds. The label keeps whole minutes, so 119 seconds
/// is shown as `1m`, and the pace uses the stored length rather than that minute.
#[test]
fn a_codex_window_keeps_its_length_for_the_pace() {
    let home = scratch(&format!("window-length-{}", std::process::id()));
    write(
        &home.join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"length-codex"}}"#,
    );
    let reset_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 90;
    let body = format!(
        r#"{{"plan_type":"plus","rate_limit":{{"primary_window":{{"used_percent":50,"limit_window_seconds":119,"reset_at":{reset_at}}}}}}}"#
    );
    let (port, _) = serve(vec![("/wham/usage", body)]);
    let base = format!("http://127.0.0.1:{port}");
    let out = probe(&[
        "--home",
        home.to_str().unwrap(),
        "--codex-url",
        &base,
        "--allow",
        "codex",
    ]);
    let line = line_of(&out, "codex");
    assert_eq!(state_of(line), "fresh", "{out}");
    assert!(line.contains("[1m 50%"), "{line}");
    assert!(
        out.contains("target Codex:") && out.contains("~0% expiring unused, 50% usable now"),
        "{out}"
    );
}
