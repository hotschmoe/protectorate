//! Real probe over synthetic logs and an on-disk index.

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const PROBE: &str = env!("CARGO_BIN_EXE_pt-history-probe");
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "history-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn run(home: &Path, index: &Path, queries: &[&str], failure: Option<&str>) -> Output {
    let mut command = Command::new(PROBE);
    command.args([
        "--home",
        home.to_str().unwrap(),
        "--index",
        index.to_str().unwrap(),
        "--runs",
        "1",
    ]);
    for query in queries {
        command.args(["--query", query]);
    }
    if let Some(phase) = failure {
        command.env("PT_TEST_HISTORY_FAIL_COMMIT", phase);
        if phase == "manifest" {
            command.env("PT_TEST_HISTORY_FLUSH_BYTES", "1048576");
        }
    }
    let out = command.output().expect("run history probe");
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[test]
fn prompts_and_discovery_stay_inside_the_root() {
    let dir = scratch("prompts");
    let home = dir.join("home");
    let claude = home.join(".claude/projects/project");
    write(
        &claude.join("real.jsonl"),
        concat!(
            "{\"type\":\"user\",\"message\":{\"content\":\"<div>visiblemarker</div>\"}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":\"<system-reminder>wrappermarker</system-reminder>\"}}\n"
        ),
    );
    write(
        &home.join(".codex/sessions/rollout-aaaaaaaa-0000-4000-8000-000000000001.jsonl"),
        concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"<3 secondmarker\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"<environment_context>wrappermarker</environment_context>\"}]}}\n"
        ),
    );
    let outside = dir.join("outside");
    write(
        &outside.join("stolen.jsonl"),
        "{\"type\":\"user\",\"message\":{\"content\":\"linktargetmarker\"}}\n",
    );
    symlink(outside.join("stolen.jsonl"), claude.join("file-link.jsonl")).unwrap();
    symlink(&outside, claude.join("dir-link")).unwrap();
    let out = run(
        &home,
        &dir.join("index"),
        &[
            "visiblemarker",
            "secondmarker",
            "wrappermarker",
            "linktargetmarker",
        ],
        None,
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("selected: 2 files"), "{stdout}");
    for query in ["visiblemarker", "secondmarker"] {
        assert!(
            stdout.contains(&format!("query \"{query}\": top 1")),
            "{stdout}"
        );
    }
    for query in ["wrappermarker", "linktargetmarker"] {
        assert!(
            stdout.contains(&format!("query \"{query}\": top 0")),
            "{stdout}"
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn replacement_with_a_symlink_is_not_opened() {
    struct Reap(Option<std::process::Child>);
    impl Drop for Reap {
        fn drop(&mut self) {
            if let Some(child) = &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let dir = scratch("replaced");
    let home = dir.join("home");
    let path = home.join(".claude/projects/project/session.jsonl");
    write(
        &path,
        "{\"type\":\"user\",\"message\":{\"content\":\"original\"}}\n",
    );
    let outside = dir.join("outside.jsonl");
    write(
        &outside,
        "{\"type\":\"user\",\"message\":{\"content\":\"outsidemarker\"}}\n",
    );
    let barrier = dir.join("discovered");
    let mut child = Reap(Some(
        Command::new(PROBE)
            .args([
                "--home",
                home.to_str().unwrap(),
                "--index",
                dir.join("index").to_str().unwrap(),
            ])
            .env("PT_TEST_HISTORY_DISCOVERY_BARRIER", &barrier)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !barrier.exists() {
        assert!(
            std::time::Instant::now() < until,
            "probe did not discover the file"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    fs::rename(&path, dir.join("original.jsonl")).unwrap();
    symlink(&outside, &path).unwrap();
    fs::write(barrier.with_extension("continue"), b"go").unwrap();
    let out = child.0.take().unwrap().wait_with_output().unwrap();
    assert!(!out.status.success(), "replaced link was followed");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("tail"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[cfg(debug_assertions)]
fn commit_retries_preserve_documents_and_cursors() {
    for phase in ["segment", "manifest"] {
        let dir = scratch(phase);
        let home = dir.join("home");
        let path = home.join(".claude/projects/project/session.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = File::create(path).unwrap();
        let count = if phase == "manifest" { 600 } else { 2 };
        let padding = if phase == "manifest" {
            "x".repeat(32_640)
        } else {
            String::new()
        };
        let line = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":\"batchmarker {padding}\"}}}}\n"
        );
        for _ in 0..count {
            file.write_all(line.as_bytes()).unwrap();
        }
        drop(file);
        let out = run(&home, &dir.join("index"), &["batchmarker"], Some(phase));
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(
            stderr.contains(&format!("retrying injected history {phase} commit failure")),
            "{stderr}"
        );
        assert!(
            stdout.contains(&format!("summary: files=1 docs={count}")),
            "{stdout}"
        );
        assert!(stdout.contains("rebuild=identical"), "{stdout}");
    }
}

#[test]
fn open_sweeps_unreferenced_segments() {
    let dir = scratch("sweep");
    let home = dir.join("home");
    write(
        &home.join(".claude/projects/project/session.jsonl"),
        "{\"type\":\"user\",\"message\":{\"content\":\"sweepmarker\"}}\n",
    );
    let index = dir.join("index");
    let a = index.join("a");
    fs::create_dir_all(&a).unwrap();
    fs::write(a.join("seg-00000099"), b"orphan").unwrap();
    fs::write(a.join("seg-00000001.tmp"), b"partial").unwrap();
    fs::write(a.join("sidecar"), b"leave").unwrap();
    let out = run(&home, &index, &["sweepmarker"], None);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("query \"sweepmarker\": top 1"), "{stdout}");
    assert!(stderr.contains("warning: history: no manifest"), "{stderr}");
    assert!(!a.join("seg-00000099").exists());
    assert!(!a.join("seg-00000001.tmp").exists());
    assert_eq!(fs::read(a.join("sidecar")).unwrap(), b"leave");
    assert!(a.join("manifest").is_file());
}

/// The indexed-byte charge of one user doc: source text plus the per-doc overhead.
fn doc_charge(text: &str, session: &str, path: &Path, sleeve: &str) -> u64 {
    let path = path.to_str().unwrap();
    let stored = format!("claude:{sleeve}:{path}");
    (text.len() + session.len() + stored.len() + sleeve.len() + 128) as u64
}

fn claude_user(text: &str) -> String {
    format!("{{\"type\":\"user\",\"message\":{{\"content\":\"{text}\"}}}}\n")
}

#[test]
fn small_budget_keeps_the_newest_lines_across_two_sessions() {
    let dir = scratch("budget");
    let home = dir.join("home");
    let root = home.join(".claude/projects/project");
    let sleeve = "4";
    let old_path = root.join("a-old.jsonl");
    let new_path = root.join("b-new.jsonl");
    let tail_path = root.join("c-tail.jsonl");
    let old_text = format!("{} oldsessiondoc", "x".repeat(4000));
    let new_text = "newsessiondoc";
    let tail_text = "tailsessiondoc";
    let budget = doc_charge(&old_text, "a-old", &old_path, sleeve);
    let later = doc_charge(new_text, "b-new", &new_path, sleeve)
        + doc_charge(tail_text, "c-tail", &tail_path, sleeve);
    assert!(
        later < budget,
        "the two newest docs have to fit under the old one"
    );
    write(&old_path, &claude_user(&old_text));
    write(&new_path, &claude_user(new_text));
    write(&tail_path, &claude_user(tail_text));
    let index = dir.join("index");
    let out = Command::new(PROBE)
        .args([
            "--home",
            home.to_str().unwrap(),
            "--index",
            index.to_str().unwrap(),
            "--runs",
            "1",
            "--budget",
            &budget.to_string(),
            "--sleeve",
            sleeve,
            "--query",
            "oldsessiondoc",
            "--query",
            "newsessiondoc",
            "--query",
            "tailsessiondoc",
        ])
        .output()
        .expect("run history probe");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("summary: files=3 docs=2"), "{stdout}");
    assert!(
        stdout.contains("query \"oldsessiondoc\": top 0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("query \"newsessiondoc\": top 1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("query \"tailsessiondoc\": top 1"),
        "{stdout}"
    );
    let named = format!("capped: {sleeve}");
    let at = stdout.find(&named).unwrap_or_else(|| panic!("{stdout}"));
    assert!(
        stdout[at + named.len()..].contains("capped: none"),
        "{stdout}"
    );
    let b = index.join("b");
    assert!(
        !b.join("seg-00000000").exists(),
        "the dead segment stays on disk"
    );
    let live: Vec<String> = fs::read_dir(&b)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("seg-") && !n.ends_with(".tmp"))
        .collect();
    assert!(!live.is_empty(), "a live segment remains: {live:?}");
}

/// One page of `session` from the probe: doc ids, their turns, and whether another page remains.
fn read_page(index: &Path, session: &str, after: Option<u64>) -> (Vec<u64>, Vec<u32>, bool) {
    let mut cmd = Command::new(PROBE);
    cmd.args(["--index", index.to_str().unwrap(), "--read", session]);
    let after_text = after.map(|id| id.to_string());
    if let Some(id) = &after_text {
        cmd.args(["--after", id]);
    }
    let out = cmd.output().expect("run history probe");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    let mut ids = Vec::new();
    let mut turns = Vec::new();
    let mut more = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("doc ") {
            let mut parts = rest.split_whitespace();
            ids.push(parts.next().unwrap().parse().unwrap());
            turns.push(parts.next().unwrap().parse().unwrap());
        } else if let Some(rest) = line.strip_prefix("more ") {
            more = Some(rest == "true");
        }
    }
    (ids, turns, more.expect("probe printed more"))
}

#[test]
fn one_turn_pages_past_200_docs_with_after() {
    let dir = scratch("page");
    let home = dir.join("home");
    let path = home.join(".claude/projects/project/page.jsonl");
    let mut body = String::from("{\"type\":\"user\",\"message\":{\"content\":\"pageuser\"}}\n");
    let tools = 250;
    for i in 0..tools {
        body.push_str(&format!(
            "{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{{\"file_path\":\"p{i}\"}}}}]}}}}\n"
        ));
    }
    write(&path, &body);
    let index = dir.join("index");
    let out = run(&home, &index, &["pageuser"], None);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("summary: files=1 docs=251"), "{stdout}");
    let (first, turns, more) = read_page(&index.join("a"), "page", None);
    assert_eq!(first.len(), 200, "{first:?}");
    assert!(more);
    assert!(turns.iter().all(|t| *t == 1), "{turns:?}");
    let (second, turns2, more2) = read_page(&index.join("a"), "page", Some(*first.last().unwrap()));
    assert!(!more2);
    assert!(turns2.iter().all(|t| *t == 1), "{turns2:?}");
    let mut all = first.clone();
    all.extend(&second);
    assert_eq!(all.len(), 251, "page lens {} {}", first.len(), second.len());
    for pair in all.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "gap or repeat at {pair:?}");
    }
}
