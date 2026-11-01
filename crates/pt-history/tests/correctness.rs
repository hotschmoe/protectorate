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
