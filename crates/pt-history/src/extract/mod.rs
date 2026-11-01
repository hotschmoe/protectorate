//! Per-harness extractors: one native log line in, zero or more documents and usage facts out.
//! Unknown record kinds produce nothing. What is kept: user prompts, assistant text (not
//! thinking), tool calls as name, paths and command, failed tool results, and compaction
//! summaries.

mod claude;
mod codex;
mod grok;
mod hub;
mod pi;

use pt_drivers::Harness;
use pt_json::Value;

use crate::tail::clip;
use crate::{Role, SourceKind};

/// Text kept per document.
pub const TEXT_MAX: usize = 32 << 10;
/// Text kept per tool call and per tool error.
pub const TOOL_MAX: usize = 512;

pt_json::record! {
    /// Extractor state carried across lines (and across ingest calls, in the manifest).
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct State {
        pub turn: u32 = 0,
        pub time_ms: u64 = 0,
        pub session: String = String::new(),
        pub harness: Option<Harness>,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub role: Role,
    pub turn: u32,
    pub time_ms: u64,
    pub session: String,
    pub harness: Option<Harness>,
    pub tool: Option<String>,
    pub files: Vec<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Doc(Draft),
    /// Prompt tokens of the latest model call, and the window when the log states it.
    Usage {
        session: String,
        harness: Option<Harness>,
        tokens: u64,
        window: Option<u64>,
        at_ms: u64,
    },
    Compacted {
        session: String,
    },
}

impl Item {
    pub fn session(&self) -> &str {
        match self {
            Item::Doc(d) => &d.session,
            Item::Usage { session, .. } | Item::Compacted { session } => session,
        }
    }
}

/// Extracts one line. Returns false when the line is not a typed JSON record.
pub fn line(kind: SourceKind, bytes: &[u8], st: &mut State, out: &mut Vec<Item>) -> bool {
    if kind == SourceKind::Hub {
        return hub::line(bytes, st, out);
    }
    let Ok(v) = pt_json::parse_bytes(bytes) else {
        return false;
    };
    if !matches!(v, Value::Object(_)) {
        return false;
    }
    match kind {
        SourceKind::Claude => claude::line(&v, st, out),
        SourceKind::Codex => codex::line(&v, st, out),
        SourceKind::Grok => grok::line(&v, st, out),
        SourceKind::Pi => pi::line(&v, st, out),
        SourceKind::Hub => false,
    }
}

/// The harness a native source belongs to.
pub fn harness_of(kind: SourceKind) -> Option<Harness> {
    match kind {
        SourceKind::Claude => Some(Harness::Claude),
        SourceKind::Codex => Some(Harness::Codex),
        SourceKind::Grok => Some(Harness::Grok),
        SourceKind::Pi => Some(Harness::Pi),
        SourceKind::Hub => None,
    }
}

impl State {
    fn push(&mut self, out: &mut Vec<Item>, role: Role, text: &str, tool: Option<&str>) {
        self.push_files(out, role, text, tool, Vec::new());
    }

    fn push_files(
        &mut self,
        out: &mut Vec<Item>,
        role: Role,
        text: &str,
        tool: Option<&str>,
        files: Vec<String>,
    ) {
        let text = text.trim();
        if text.is_empty() && files.is_empty() {
            return;
        }
        if role == Role::User {
            self.turn += 1;
        }
        let max = match role {
            Role::ToolCall | Role::ToolError => TOOL_MAX,
            _ => TEXT_MAX,
        };
        out.push(Item::Doc(Draft {
            role,
            turn: self.turn,
            time_ms: self.time_ms,
            session: self.session.clone(),
            harness: self.harness,
            tool: tool.map(|t| clip(t, 128).to_owned()),
            files,
            text: clip(text, max).to_owned(),
        }));
    }

    fn usage(&self, out: &mut Vec<Item>, tokens: u64, window: Option<u64>) {
        if tokens > 0 {
            out.push(Item::Usage {
                session: self.session.clone(),
                harness: self.harness,
                tokens,
                window,
                at_ms: self.time_ms,
            });
        }
    }

    fn compacted(&self, out: &mut Vec<Item>) {
        out.push(Item::Compacted {
            session: self.session.clone(),
        });
    }

    /// Takes an ISO 8601 timestamp (or epoch milliseconds) when the record has one.
    fn stamp(&mut self, v: &Value) {
        let t = match v {
            Value::String(s) => parse_time(s),
            Value::Number(n) => n.as_u64(),
            _ => None,
        };
        if let Some(t) = t {
            self.time_ms = t;
        }
    }
}

/// Milliseconds since the epoch for `YYYY-MM-DDTHH:MM:SS[.frac][Z|+HH:MM|-HH:MM]`.
pub fn parse_time(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let d = b.get(r)?;
        if d.is_empty() || !d.iter().all(u8::is_ascii_digit) {
            return None;
        }
        d.iter()
            .try_fold(0i64, |acc, c| Some(acc * 10 + i64::from(c - b'0')))
    };
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut ms = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            if i - start < 3 {
                ms = ms * 10 + i64::from(b[i] - b'0');
            }
            i += 1;
        }
        if i == start {
            return None;
        }
        for _ in (i - start)..3 {
            ms *= 10;
        }
    }
    let offset_min = match b.get(i) {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(&sign @ (b'+' | b'-')) => {
            let oh = num(i + 1..i + 3)?;
            let om = match b.get(i + 3) {
                Some(b':') => num(i + 4..i + 6)?,
                _ => num(i + 3..i + 5)?,
            };
            let m = oh * 60 + om;
            if sign == b'+' { m } else { -m }
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    let total = secs.checked_mul(1000)?.checked_add(ms)?;
    u64::try_from(total).ok()
}

/// Text of a string, or of the text blocks of an array (`{"type": "text"|"input_text"|
/// "output_text", "text": ...}`), joined by newlines.
fn text_of(v: &Value) -> String {
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
fn is_wrapped(text: &str) -> bool {
    text.trim_start().starts_with('<')
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

/// A tool call as one line of text (name, paths, command or other salient arguments) and the
/// paths it names. `input` is an object, or a string holding JSON (Codex, Grok) or raw text.
fn tool_call(name: &str, input: &Value) -> (String, Vec<String>) {
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
