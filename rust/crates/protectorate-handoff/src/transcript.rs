use crate::{Error, Result};
use cap_std::{ambient_authority, fs::Dir};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
pub const MAX_TRANSCRIPT_READ: u64 = 16 << 20;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    pub role: String,
    pub text: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    pub path: String,
    pub turns: Vec<Turn>,
}
fn pattern_match(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            name.starts_with(prefix)
                && name.ends_with(suffix)
                && name.len() >= prefix.len() + suffix.len()
        }
        None => name == pattern,
    }
}
fn locate(root: &Dir, harness: &str, id: &str) -> Result<PathBuf> {
    let patterns = match harness {
        "claude" => vec![format!(".claude/projects/*/{id}.jsonl")],
        "codex" => vec![format!(".codex/sessions/*/*/*/rollout-*-{id}.jsonl")],
        "grok" => vec![format!(".grok/sessions/*/{id}/chat_history.jsonl")],
        "pi" => vec![
            format!(".pi/sessions/*_{id}.jsonl"),
            format!(".pi/sessions/*/*_{id}.jsonl"),
        ],
        _ => {
            return Err(Error::Invalid(format!(
                "no native transcript for harness {harness:?}"
            )))
        }
    };
    fn walk(root: &Dir, path: &Path, parts: &[&str], out: &mut Vec<PathBuf>) {
        if parts.is_empty() {
            if root.symlink_metadata(path).is_ok() {
                out.push(path.into())
            }
            return;
        }
        if parts[0].contains('*') {
            if let Ok(entries) = root.read_dir(path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if pattern_match(parts[0], &name.to_string_lossy()) {
                        walk(root, &path.join(name), &parts[1..], out)
                    }
                }
            }
        } else {
            walk(root, &path.join(parts[0]), &parts[1..], out)
        }
    }
    for pattern in patterns {
        let mut out = Vec::new();
        walk(
            root,
            Path::new("."),
            &pattern.split('/').collect::<Vec<_>>(),
            &mut out,
        );
        out.sort();
        if let Some(path) = out.pop() {
            return Ok(path);
        }
    }
    Err(Error::Invalid(format!("no {harness} session {id} found")))
}
pub fn read_transcript(home: impl AsRef<Path>, harness: &str, id: &str) -> Result<Transcript> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(Error::Invalid("invalid session id".into()));
    }
    let root = Dir::open_ambient_dir(home, ambient_authority())?;
    let path = locate(&root, harness, id)?;
    let mut file = root.open(&path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(Error::Invalid("transcript is not a regular file".into()));
    }
    let partial = meta.len() > MAX_TRANSCRIPT_READ;
    if partial {
        file.seek(SeekFrom::Start(meta.len() - MAX_TRANSCRIPT_READ))?;
    }
    let turns = parse_transcript(harness, file.take(MAX_TRANSCRIPT_READ), partial)?;
    Ok(Transcript {
        path: path
            .strip_prefix(".")
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/"),
        turns,
    })
}
pub fn parse_transcript(harness: &str, reader: impl Read, partial: bool) -> Result<Vec<Turn>> {
    if !["claude", "codex", "grok", "pi"].contains(&harness) {
        return Err(Error::Invalid("unsupported transcript harness".into()));
    }
    let mut reader = BufReader::new(reader.take(MAX_TRANSCRIPT_READ));
    let mut turns: Vec<Turn> = Vec::new();
    let mut last_id = String::new();
    let mut first = true;
    loop {
        let mut raw = Vec::new();
        if reader.read_until(b'\n', &mut raw)? == 0 {
            break;
        }
        if !(first && partial) {
            if let Some((turn, id)) = parse_line(harness, &raw) {
                if turn.role == "assistant"
                    && !id.is_empty()
                    && id == last_id
                    && turns.last().is_some_and(|t| t.role == "assistant")
                {
                    let last = turns.last_mut().unwrap();
                    last.text.push('\n');
                    last.text.push_str(&turn.text)
                } else {
                    turns.push(turn)
                }
                last_id = id;
            }
        }
        first = false;
    }
    Ok(turns)
}
fn content_text(v: &Value) -> (String, bool) {
    if let Some(s) = v.as_str() {
        return (s.into(), false);
    }
    let mut out = Vec::new();
    let mut tool = false;
    if let Some(parts) = v.as_array() {
        for p in parts {
            match p.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" | "input_text" | "output_text" => {
                    if let Some(s) = p
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        out.push(s)
                    }
                }
                "tool_result" | "toolResult" | "function_call_output" => tool = true,
                _ => {}
            }
        }
    }
    (out.join("\n"), tool)
}
fn wrapped(s: &str) -> bool {
    let s = s.trim();
    let Some((opening, rest)) = s.strip_prefix('<').and_then(|s| s.split_once('>')) else {
        return false;
    };
    let tag = |s: &str| {
        s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && s.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
    };
    if !tag(opening) {
        return false;
    }
    rest.rsplit_once("</")
        .and_then(|(_, s)| s.strip_suffix('>'))
        .is_some_and(tag)
}
fn parse_line(harness: &str, raw: &[u8]) -> Option<(Turn, String)> {
    let v: Value = serde_json::from_slice(raw).ok()?;
    let kind = v.get("type")?.as_str()?;
    let (role, id, content) = match harness {
        "claude" => {
            if !["user", "assistant"].contains(&kind)
                || ["isMeta", "isCompactSummary", "isSidechain"]
                    .iter()
                    .any(|k| v.get(k).and_then(Value::as_bool) == Some(true))
            {
                return None;
            }
            let m = v.get("message")?;
            (
                m.get("role")?.as_str()?,
                m.get("id").and_then(Value::as_str).unwrap_or(""),
                m.get("content")?,
            )
        }
        "pi" => {
            if kind != "message" {
                return None;
            }
            let m = v.get("message")?;
            (m.get("role")?.as_str()?, "", m.get("content")?)
        }
        "codex" => {
            if kind != "response_item" {
                return None;
            }
            let p = v.get("payload")?;
            if p.get("type")?.as_str()? != "message" {
                return None;
            }
            (p.get("role")?.as_str()?, "", p.get("content")?)
        }
        "grok" => {
            if !["user", "assistant"].contains(&kind)
                || kind == "user"
                    && (v.get("prompt_index").and_then(Value::as_i64).is_none()
                        || v.get("synthetic_reason")
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty()))
            {
                return None;
            }
            (kind, "", v.get("content")?)
        }
        _ => return None,
    };
    if role != "user" && role != "assistant" {
        return None;
    }
    let (text, tool) = content_text(content);
    let mut text = text.trim();
    if harness == "grok" && role == "user" {
        if let Some(inner) = text
            .strip_prefix("<user_query>")
            .and_then(|s| s.strip_suffix("</user_query>"))
        {
            text = inner.trim()
        }
    }
    if text.is_empty() || role == "user" && (tool || wrapped(text)) {
        return None;
    }
    Some((
        Turn {
            role: role.into(),
            text: text.into(),
        },
        id.into(),
    ))
}
