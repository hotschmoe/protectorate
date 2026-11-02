use pt_json::Value;

use crate::tail::clip;

/// Text kept per document.
pub const TEXT_MAX: usize = 32 << 10;
/// Text kept per tool call and per tool error.
pub const TOOL_MAX: usize = 512;

/// Text of a string, or of the text blocks of an array (`{"type": "text"|"input_text"|
/// "output_text", "text": ...}`), joined by newlines.
pub(super) fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let mut out = String::new();
            for b in blocks {
                let t = match b["type"].as_str() {
                    Some("text" | "input_text" | "output_text") => b["text"].as_str(),
                    None => b.as_str(),
                    _ => None,
                };
                if let Some(t) = t {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// User text the harness wrapped in a tag (`<system-reminder>`, `<environment_context>`,
/// command output) is context, not a prompt.
pub(super) fn is_wrapped(text: &str) -> bool {
    let text = text.trim_start();
    [
        "<system-reminder>",
        "<environment_context>",
        "<local-command-caveat>",
        "<local-command-stdout>",
        "<local-command-stderr>",
        "<command-name>",
        "<command-message>",
        "<command-args>",
    ]
    .iter()
    .any(|tag| text.starts_with(tag))
}

const PATH_KEYS: &[&str] = &[
    "file_path",
    "path",
    "notebook_path",
    "target_file",
    "filePath",
];
const ARG_KEYS: &[&str] = &[
    "command",
    "cmd",
    "pattern",
    "query",
    "url",
    "description",
    "old_string",
    "prompt",
];

/// Paths kept with a document: each clipped to [`TOOL_MAX`], and together no larger than that.
pub(crate) fn take_files<'a>(files: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for f in files {
        let f = clip(f.trim(), TOOL_MAX);
        if f.is_empty() {
            continue;
        }
        if !out.is_empty() && used.saturating_add(f.len()) > TOOL_MAX {
            break;
        }
        used = used.saturating_add(f.len());
        out.push(f.to_owned());
    }
    out
}

/// A tool call as one line of text (name, paths, command or other salient arguments) and the
/// paths it names. `input` is an object, or a string holding JSON (Codex, Grok) or raw text.
pub(super) fn tool_call(name: &str, input: &Value) -> (String, Vec<String>) {
    let parsed;
    let input = match input {
        Value::String(s) => match pt_json::parse(s) {
            Ok(v @ Value::Object(_)) => {
                parsed = v;
                &parsed
            }
            _ => {
                let files = patch_files(s);
                return (format!("{name} {}", clip(s, TOOL_MAX)), files);
            }
        },
        v => v,
    };
    let mut text = String::from(name);
    let mut files = Vec::new();
    for k in PATH_KEYS {
        if let Some(p) = input[*k].as_str() {
            text.push(' ');
            text.push_str(p);
            files.push(p.to_owned());
        }
    }
    for k in ARG_KEYS {
        match &input[*k] {
            Value::String(s) => {
                text.push(' ');
                text.push_str(s);
            }
            Value::Array(a) => {
                for s in a.iter().filter_map(Value::as_str) {
                    text.push(' ');
                    text.push_str(s);
                }
            }
            _ => {}
        }
        if text.len() > TOOL_MAX {
            break;
        }
    }
    if let Some(p) = input["patch"].as_str().or(input["input"].as_str()) {
        files.extend(patch_files(p));
    }
    for f in &files {
        if text.len() > TOOL_MAX {
            break;
        }
        if !text.contains(f.as_str()) {
            text.push(' ');
            text.push_str(f);
        }
    }
    (text, files)
}

/// Paths named by an `apply_patch` envelope.
fn patch_files(patch: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in patch.lines() {
        for p in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
            if let Some(f) = l.strip_prefix(p) {
                let f = f.trim();
                if !f.is_empty() && !out.iter().any(|o| o == f) {
                    out.push(f.to_owned());
                }
            }
        }
    }
    out
}
