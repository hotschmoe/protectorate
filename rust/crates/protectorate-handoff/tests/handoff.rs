use protectorate_handoff::*;
use std::{fs, io::Write, path::Path, process::Command};
fn file(root: &Path, path: &str, text: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap()
}
#[test]
fn reads_four_native_harness_formats() {
    let root = tempfile::tempdir().unwrap();
    file(
        root.path(),
        ".claude/projects/work/sid.jsonl",
        r#"{"type":"user","message":{"role":"user","content":"Original task"}}
{"type":"user","isMeta":true,"message":{"role":"user","content":"ignore meta"}}
{"type":"user","message":{"role":"user","content":"<environment>generated</environment>"}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"ignore tool"},{"type":"tool_result","content":"result"}]}}
{"type":"assistant","message":{"id":"a","role":"assistant","content":[{"type":"text","text":"first chunk"}]}}
{"type":"assistant","message":{"id":"a","role":"assistant","content":"second chunk"}}
{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":"ignore sidechain"}}
"#,
    );
    let t = read_transcript(root.path(), "claude", "sid").unwrap();
    assert_eq!(t.turns.len(), 2);
    assert_eq!(t.turns[0].text, "Original task");
    assert_eq!(t.turns[1].text, "first chunk\nsecond chunk");
    file(
        root.path(),
        ".codex/sessions/2026/10/22/rollout-time-sid.jsonl",
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"codex final"}]}}
{"type":"event_msg","payload":{"type":"agent_message","message":"duplicate"}}
"#,
    );
    assert_eq!(
        read_transcript(root.path(), "codex", "sid").unwrap().turns[0].text,
        "codex final"
    );
    file(
        root.path(),
        ".grok/sessions/work/sid/chat_history.jsonl",
        r#"{"type":"user","prompt_index":0,"content":"<user_query>Actual question</user_query>"}
{"type":"user","synthetic_reason":"tool","prompt_index":1,"content":"generated"}
{"type":"assistant","content":"grok answer"}
"#,
    );
    let t = read_transcript(root.path(), "grok", "sid").unwrap();
    assert_eq!(t.turns.len(), 2);
    assert_eq!(t.turns[0].text, "Actual question");
    file(
        root.path(),
        ".pi/sessions/work/2026_sid.jsonl",
        r#"{"type":"message","message":{"role":"user","content":"pi task"}}
{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"pi answer"}]}}
"#,
    );
    assert_eq!(
        read_transcript(root.path(), "pi", "sid")
            .unwrap()
            .turns
            .len(),
        2
    );
    assert!(read_transcript(root.path(), "claude", "../sid").is_err());
    assert!(read_transcript(root.path(), "unknown", "sid").is_err());
}
#[test]
fn transcript_tail_skips_partial_first_line_and_keeps_unterminated_last() {
    let data=b"partial garbage\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":\"final\"}}";
    let t = parse_transcript("pi", &data[..], true).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].text, "final");
}
#[test]
fn budget_prioritizes_final_context_and_preserves_utf8() {
    for budget in [0, 1, 20, 100, 400] {
        let c = clip(&"界".repeat(1000), budget);
        assert!(c.len() <= budget);
        assert!(std::str::from_utf8(c.as_bytes()).is_ok())
    }
    let mut f = Facts {
        sleeve: "alice".into(),
        harness: "codex".into(),
        to: "claude".into(),
        workspace: "/workspace".into(),
        ..Default::default()
    };
    for i in 0..20 {
        f.transcript.turns.push(Turn {
            role: "user".into(),
            text: format!("request {i}: {}", "界".repeat(800)),
        })
    }
    f.transcript.turns.push(Turn {
        role: "assistant".into(),
        text: "FINAL RESULT: continue the remaining implementation".into(),
    });
    let md = f.markdown(2500);
    assert!(md.len() <= 2500);
    assert!(md.contains("FINAL RESULT"));
    assert!(md.contains("other messages omitted"));
    assert!(md.contains("Historical context"));
    let summary = Summary {
        text: "# Goal\nDo the work\n## Next\nFinish it".into(),
        harness: "local".into(),
        local: true,
        ..Default::default()
    };
    let doc = document(&f, &summary, 2500);
    assert!(doc.contains("### Goal"));
    assert!(doc.contains("### Next"));
    assert!(doc.contains("local model"));
    let prompt = summary_prompt(&f, &md);
    for heading in [
        "## Goal",
        "## State",
        "## Unfinished",
        "## Next",
        "## Gotchas",
    ] {
        assert!(prompt.contains(heading))
    }
    assert!(prompt.contains("Never include credentials"));
    assert!(document(&f, &Summary::default(), 2500).contains("No summary"));
}
#[test]
fn git_facts_use_caller_runner_and_report_failures() {
    let mut f = Facts::default();
    let mut seen = Vec::new();
    f.read_git(|_, args| {
        seen.push(args[0].to_owned());
        Ok(match args[0] {
            "status" => "## main\n",
            "symbolic-ref" => "main\n",
            "rev-parse" => "abcdef\n",
            "diff" => "1 file changed\n",
            "log" => "abcdef initial\n",
            _ => "",
        }
        .into())
    });
    assert_eq!(f.head, "abcdef");
    assert_eq!(f.branch, "main");
    assert_eq!(seen, ["status", "symbolic-ref", "rev-parse", "diff", "log"]);
    let mut failed = Facts::default();
    failed.read_git(|_, _| Err(Error::Invalid("sandbox unavailable".into())));
    assert!(failed.git_err.contains("sandbox unavailable"));
}
fn git(root: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().into()
}
#[test]
fn writes_excludes_and_archives_without_collisions() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-b", "main"]);
    git(root.path(), &["config", "user.name", "Test"]);
    git(
        root.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    file(root.path(), "tracked", "x");
    git(root.path(), &["add", "tracked"]);
    git(root.path(), &["commit", "-m", "initial"]);
    assert!(exclude(root.path()).unwrap());
    assert!(!exclude(root.path()).unwrap());
    let path = write(root.path(), b"first handoff").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"first handoff");
    write(root.path(), b"second handoff").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second handoff");
    assert!(git(root.path(), &["status", "--porcelain"]).is_empty());
    assert!(!has_notes(root.path()));
    file(root.path(), ".protectorate/NOTES.md", "standing notes");
    assert!(has_notes(root.path()));
    let archives = tempfile::tempdir().unwrap();
    let at = "2026-10-22T00:00:00Z".parse().unwrap();
    let one = archive(archives.path(), "alice", "codex", "claude", at, b"one").unwrap();
    let two = archive(archives.path(), "alice", "codex", "claude", at, b"two").unwrap();
    assert_ne!(one, two);
    assert_eq!(fs::read(&one).unwrap(), b"one");
    assert!(archive(archives.path(), "../escape", "a", "b", at, b"bad").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(one).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let linked = root.path().join("linked");
    git(
        root.path(),
        &["worktree", "add", "-b", "other", linked.to_str().unwrap()],
    );
    assert!(!exclude(&linked).unwrap());
}
#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_reads_or_writes() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join(".protectorate")).unwrap();
    assert!(write(root.path(), b"bad").is_err());
    assert!(!outside.path().join("HANDOFF.md").exists());
    fs::remove_file(root.path().join(".protectorate")).unwrap();
    file(root.path(), ".protectorate/NOTES.md", "safe");
    fs::remove_file(root.path().join(".protectorate/NOTES.md")).unwrap();
    file(outside.path(), "secret", "private");
    symlink(
        outside.path().join("secret"),
        root.path().join(".protectorate/NOTES.md"),
    )
    .unwrap();
    assert!(!has_notes(root.path()));
    file(root.path(), ".pi/sessions/time_id.jsonl", "{}");
    fs::remove_file(root.path().join(".pi/sessions/time_id.jsonl")).unwrap();
    symlink(
        outside.path().join("secret"),
        root.path().join(".pi/sessions/time_id.jsonl"),
    )
    .unwrap();
    assert!(read_transcript(root.path(), "pi", "id").is_err());
}
#[test]
fn transcript_read_is_bounded_to_tail() {
    let root = tempfile::tempdir().unwrap();
    let p = root.path().join(".pi/sessions/time_id.jsonl");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(&vec![b'x'; MAX_TRANSCRIPT_READ as usize])
        .unwrap();
    f.write_all(
        b"\n{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"latest\"}}\n",
    )
    .unwrap();
    let t = read_transcript(root.path(), "pi", "id").unwrap();
    assert_eq!(t.turns.len(), 1);
    assert_eq!(t.turns[0].text, "latest");
}
