//! Parses real harness session logs from this machine (Claude, Codex, Pi JSONL), checks that
//! write-then-parse gives an equal value, and reports throughput (`--nocapture`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SOURCE_BUDGET: usize = 20 << 20;
const FILE_CAP: usize = 2 << 20;

fn jsonl_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                jsonl_files(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

/// Complete lines from the first `FILE_CAP` bytes of each file (the line crossing the cap is
/// kept whole), up to `budget` per source. Files may be appended to while we read.
fn sample(files: &mut [PathBuf], budget: usize, out: &mut String) -> usize {
    files.sort();
    let start = out.len();
    let mut used = 0;
    for f in files.iter() {
        if out.len() - start >= budget {
            break;
        }
        let Ok(bytes) = fs::read(f) else { continue };
        let Some(last_nl) = bytes.iter().rposition(|&b| b == b'\n') else {
            continue;
        };
        let end = match bytes[FILE_CAP.min(last_nl)..]
            .iter()
            .position(|&b| b == b'\n')
        {
            Some(p) => FILE_CAP.min(last_nl) + p + 1,
            None => last_nl + 1,
        };
        let text = std::str::from_utf8(&bytes[..end])
            .unwrap_or_else(|e| panic!("{}: not UTF-8: {e}", f.display()));
        out.push_str(text);
        used += 1;
    }
    used
}

#[test]
fn real_session_logs() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        println!("real logs: HOME unset, skipped");
        return;
    };
    let sources: [(&str, PathBuf, usize, usize); 3] = [
        ("claude", home.join(".claude/projects"), 1, SOURCE_BUDGET),
        ("codex", home.join(".codex/sessions"), 8, SOURCE_BUDGET),
        ("pi", home.join(".pi/agent/sessions"), 8, SOURCE_BUDGET / 2),
    ];
    let mut text = String::new();
    let mut summary = Vec::new();
    for (name, dir, depth, budget) in sources {
        let mut files = Vec::new();
        jsonl_files(&dir, depth, &mut files);
        let before = text.len();
        let used = sample(&mut files, budget, &mut text);
        summary.push(format!(
            "{name} {used} files {:.1} MB",
            mb(text.len() - before)
        ));
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        println!("real logs: no session logs on this machine, skipped");
        return;
    }

    let mut parse_time = Duration::ZERO;
    let mut write_time = Duration::ZERO;
    let mut written = String::new();
    for (n, line) in lines.iter().enumerate() {
        let t = Instant::now();
        let v = pt_json::parse(line).unwrap_or_else(|e| panic!("line {n}: {e}: {line:.200}"));
        parse_time += t.elapsed();

        written.clear();
        let t = Instant::now();
        v.write(&mut written);
        write_time += t.elapsed();

        let again = pt_json::parse(&written).unwrap_or_else(|e| panic!("line {n} rewritten: {e}"));
        assert!(again == v, "line {n}: write-then-parse differs");
    }
    let bytes: usize = lines.iter().map(|l| l.len()).sum();
    println!(
        "real logs: {}; {} lines, {:.1} MB; parse {:.0} MB/s, write {:.0} MB/s",
        summary.join(", "),
        lines.len(),
        mb(bytes),
        mb(bytes) / parse_time.as_secs_f64(),
        mb(bytes) / write_time.as_secs_f64(),
    );
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1 << 20) as f64
}
