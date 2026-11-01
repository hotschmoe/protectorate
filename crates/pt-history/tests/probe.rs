//! End to end through the built `pt-history-probe`: a synthetic home with one log per harness
//! in its real on-disk layout, and a 50 MB slice of this machine's real logs. Each run indexes,
//! queries, checks recall, rebuilds a second index with a different commit pattern and checks
//! the results are identical.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh dir under the target tmp dir (left in place; cargo clean removes it).
fn temp(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pt-history-{name}-{}-{n}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn probe(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_pt-history-probe"))
        .args(args)
        .output()
        .expect("run pt-history-probe");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    eprintln!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "probe failed: {:?}", out.status);
    stdout
}

/// The `top N` count printed for `query`.
fn top(out: &str, query: &str) -> usize {
    let prefix = format!("query {query:?}: top ");
    let line = out
        .lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no result line for {query:?}"));
    line.split(',').next().unwrap().parse().unwrap()
}

/// The first hit line printed under `query`.
fn first_hit<'a>(out: &'a str, query: &str) -> &'a str {
    let prefix = format!("query {query:?}: top ");
    let mut lines = out.lines().skip_while(|l| !l.starts_with(&prefix));
    lines.next();
    lines.next().expect("a hit line").trim()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn synthetic_home() -> PathBuf {
    let home = temp("home");
    write(
        &home.join(".claude/projects/-work/aaaaaaaa-0000-4000-8000-000000000001.jsonl"),
        concat!(
            r#"{"type":"permission-mode","permissionMode":"default"}"#,
            "\n",
            r#"{"type":"user","isSidechain":false,"timestamp":"2026-10-30T10:00:00.000Z","message":{"role":"user","content":"Fix the journal codec torn tail recovery in crates/pt-journal"}}"#,
            "\n",
            r#"{"type":"user","isMeta":true,"timestamp":"2026-10-30T10:00:00.500Z","message":{"role":"user","content":"metamarker caveat"}}"#,
            "\n",
            r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-10-30T10:00:05.000Z","message":{"role":"assistant","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000},"content":[{"type":"thinking","thinking":"thinkingmarker"},{"type":"text","text":"Reading the codec first."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/work/crates/pt-journal/src/codec.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","isSidechain":true,"timestamp":"2026-10-30T10:00:06.000Z","message":{"role":"user","content":"sidechainmarker words"}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-10-30T10:00:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"Exit code 1\ncodec.rs: torn tail at byte 4096"}]}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-10-30T10:00:08.000Z","message":{"role":"user","content":"<system-reminder>wrappedmarker</system-reminder>"}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-10-30T10:00:09.000Z","message":{"role":"user","content":"tornmarker half writ"#,
        ),
    );
    write(
        &home.join(".codex/sessions/2026/10/30/rollout-2026-10-30T11-00-00-bbbbbbbb-0000-4000-8000-000000000002.jsonl"),
        concat!(
            r#"{"timestamp":"2026-10-30T11:00:00.000Z","type":"session_meta","payload":{"id":"bbbbbbbb-0000-4000-8000-000000000002","cwd":"/work"}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>envmarker</environment_context>"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:02.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Run flutter analyze in app/ and fix what it reports"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:03.000Z","type":"response_item","payload":{"type":"reasoning","summary":[],"encrypted_content":"gAAAA"}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:04.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"flutter analyze\",\"workdir\":\"/work/app\"}","call_id":"c1"}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:09.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"Chunk ID: 1\nProcess exited with code 1\nOutput:\n3 issues found"}}"#,
            "\n",
            r#"{"timestamp":"2026-10-30T11:00:10.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":5000},"model_context_window":258400}}}"#,
            "\n",
        ),
    );
    let grok = home.join(".grok/sessions/%2Fwork/cccccccc-0000-4000-8000-000000000003");
    write(
        &grok.join("summary.json"),
        r#"{"info":{"id":"cccccccc-0000-4000-8000-000000000003","cwd":"/work"},"created_at":"2026-10-30T12:00:00.000Z"}"#,
    );
    write(
        &grok.join("chat_history.jsonl"),
        concat!(
            r#"{"type":"system","content":"systemmarker prompt"}"#,
            "\n",
            r#"{"type":"user","content":[{"type":"text","text":"<user_info>contextmarker</user_info>"}]}"#,
            "\n",
            r#"{"type":"user","content":[{"type":"text","text":"<user_query>\nHow does headscale register a node?\n</user_query>"}],"prompt_index":0}"#,
            "\n",
            r#"{"type":"assistant","content":"Reading the headscale auth code.","tool_calls":[{"id":"g1","name":"grep","arguments":"{\"pattern\":\"func register\",\"path\":\"/go/headscale\"}"}]}"#,
            "\n",
            r#"{"type":"tool_result","tool_call_id":"g1","content":"resultmarker auth.go:12"}"#,
            "\n",
        ),
    );
    write(
        &home.join(".pi/agent/sessions/--work--/2026-10-30T13-00-00-000Z_dddddddd-0000-4000-8000-000000000004.jsonl"),
        concat!(
            r#"{"type":"session","version":3,"id":"dddddddd-0000-4000-8000-000000000004","timestamp":"2026-10-30T13:00:00.000Z","cwd":"/work"}"#,
            "\n",
            r#"{"type":"message","id":"1","timestamp":"2026-10-30T13:00:01.000Z","message":{"role":"user","content":[{"type":"text","text":"Check the sandbox, then reply with exactly the word READY."}]}}"#,
            "\n",
            r#"{"type":"message","id":"2","timestamp":"2026-10-30T13:00:02.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Checking."},{"type":"toolCall","id":"p1","name":"bash","arguments":{"command":"bwrap --version"}}],"usage":{"input":2000,"output":5,"cacheRead":500,"cacheWrite":0}}}"#,
            "\n",
            r#"{"type":"message","id":"3","timestamp":"2026-10-30T13:00:03.000Z","message":{"role":"toolResult","toolCallId":"p1","toolName":"bash","isError":true,"content":[{"type":"text","text":"bwrap: setting up uid map: Permission denied"}]}}"#,
            "\n",
            r#"{"type":"message","id":"4","timestamp":"2026-10-30T13:00:04.000Z","message":{"role":"assistant","content":[{"type":"text","text":"READY"}],"usage":{"input":2600,"output":2,"cacheRead":0,"cacheWrite":0}}}"#,
            "\n",
            r#"{"type":"compaction","id":"5","timestamp":"2026-10-30T13:00:05.000Z","summary":"Compacted: the sandbox check failed.","tokensBefore":2600}"#,
            "\n",
        ),
    );
    home
}

#[test]
fn synthetic_home_every_harness() {
    let home = synthetic_home();
    let index = temp("synthetic-index");
    let hits = [
        ("journal codec torn tail", "claude"),
        ("flutter analyze", "codex"),
        ("headscale register", "grok"),
        ("bwrap", "pi"),
        ("READY", "pi"),
        ("crates/pt-journal", "claude"),
        ("\"uid map\"", "pi"),
        ("\"at byte 4096\"", "claude"),
        ("\"3 issues found\"", "codex"),
        ("\"sandbox check failed\"", "pi"),
    ];
    let misses = [
        "tornmarker",
        "sidechainmarker",
        "metamarker",
        "thinkingmarker",
        "wrappedmarker",
        "envmarker",
        "systemmarker",
        "contextmarker",
        "resultmarker",
        "journal flutter",
    ];
    let mut args = vec![
        "--index",
        index.to_str().unwrap(),
        "--home",
        home.to_str().unwrap(),
        "--runs",
        "2",
    ];
    for q in hits.iter().map(|(q, _)| *q).chain(misses) {
        args.push("--query");
        args.push(q);
    }
    let out = probe(&args);
    assert!(out.contains("selected: 4 files"), "every log selected");
    for (q, harness) in hits {
        assert!(top(&out, q) > 0, "{q:?} finds something");
        let hit = first_hit(&out, q);
        assert!(
            hit.contains(&format!(" {harness} ")),
            "{q:?} top hit is {harness}: {hit}"
        );
    }
    for q in misses {
        assert_eq!(top(&out, q), 0, "{q:?} is not indexed");
    }
    assert!(
        out.contains("context: codex bbbbbbbb tokens 5000 window 258400 turns 1 compactions 0"),
        "codex context usage"
    );
    assert!(
        out.contains("context: pi dddddddd tokens 2600 window - turns 1 compactions 1"),
        "pi context usage"
    );
    assert!(out.contains("rebuild: identical"), "rebuild matches");
    assert!(
        out.contains("recall=") && !out.contains("recall=0/"),
        "recall sampled"
    );
}

#[test]
fn real_logs_50mb() {
    let Some(home) = std::env::var_os("HOME") else {
        eprintln!("SKIP: no HOME");
        return;
    };
    let home = PathBuf::from(home);
    let any = [
        ".claude/projects",
        ".codex/sessions",
        ".grok/sessions",
        ".pi/agent/sessions",
    ]
    .iter()
    .any(|r| home.join(r).is_dir());
    if !any {
        eprintln!("SKIP: no harness logs under {}", home.display());
        return;
    }
    let index = temp("real-index");
    let out = probe(&[
        "--index",
        index.to_str().unwrap(),
        "--cap",
        "50M",
        "--runs",
        "3",
    ]);
    let summary = out
        .lines()
        .find(|l| l.starts_with("summary: "))
        .expect("summary line");
    if summary.contains("files=0") {
        eprintln!("SKIP: no logs found");
        return;
    }
    let field = |k: &str| {
        summary
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .unwrap_or_else(|| panic!("no {k} in {summary}"))
    };
    let (found, sampled) = field("recall=").split_once('/').unwrap();
    assert_eq!(
        found, sampled,
        "every sampled doc is found by its own words"
    );
    assert_ne!(sampled, "0", "docs were sampled");
    let (with_hits, _) = field("queries_with_hits=").split_once('/').unwrap();
    assert!(
        with_hits.parse::<u32>().unwrap() >= 1,
        "some query finds hits"
    );
    assert_eq!(field("rebuild="), "identical");
}
