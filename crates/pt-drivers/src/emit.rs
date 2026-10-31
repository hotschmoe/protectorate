use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use pt_json::Value;

use crate::event::{Entry, ErrorClass, Event, ToolKind};

pub(crate) const MAX_TOOL_OUTPUT: usize = 16 << 10;
pub(crate) const MAX_TOOL_INPUT: usize = 16 << 10;
const SUMMARY: usize = 300;

/// Assigns seqs, coalesces text deltas into one final entry per block, and buffers what a
/// protocol step produced: entries for the hub and lines for the harness's stdin.
pub(crate) struct Out {
    g: u64,
    next_seq: u64,
    open: Option<OpenText>,
    /// Recently finalized blocks, so a late authoritative end does not repeat one.
    closed: VecDeque<(bool, String)>,
    unmapped: HashSet<String>,
    pub(crate) events: Vec<Event>,
    pub(crate) writes: Vec<Value>,
}

struct OpenText {
    seq: u64,
    reasoning: bool,
    id: String,
    text: String,
}

impl Out {
    pub(crate) fn new(g: u64) -> Out {
        Out {
            g,
            next_seq: 1,
            open: None,
            closed: VecDeque::new(),
            unmapped: HashSet::new(),
            events: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub(crate) fn write(&mut self, line: Value) {
        self.writes.push(line);
    }

    fn push(&mut self, seq: u64, entry: Entry) {
        self.events.push(Event {
            seq,
            g: self.g,
            time_ms: now_ms(),
            entry,
        });
    }

    fn take_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Any entry other than a delta closes the open text block first.
    pub(crate) fn emit(&mut self, entry: Entry) {
        self.flush();
        let seq = self.take_seq();
        self.push(seq, entry);
    }

    pub(crate) fn error(&mut self, class: ErrorClass, text: impl Into<String>) {
        self.emit(Entry::Error {
            class,
            text: text.into(),
        });
    }

    pub(crate) fn notice(&mut self, code: &str, text: impl Into<String>) {
        self.emit(Entry::Notice {
            code: Some(code.to_owned()),
            text: text.into(),
        });
    }

    /// One notice per unmapped message type per session (NoMatch, not Desync).
    pub(crate) fn unmapped(&mut self, what: &str) {
        if self.unmapped.len() < 256 && self.unmapped.insert(what.to_owned()) {
            let what = bounded(what, 120);
            self.notice(
                &format!("unmapped:{what}"),
                format!("unmapped message {what}"),
            );
        }
    }

    pub(crate) fn delta(&mut self, reasoning: bool, id: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        if self
            .open
            .as_ref()
            .is_some_and(|o| o.reasoning != reasoning || o.id != id)
        {
            self.flush();
        }
        if self.open.is_none() {
            let seq = self.take_seq();
            self.open = Some(OpenText {
                seq,
                reasoning,
                id: id.to_owned(),
                text: String::new(),
            });
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.text.push_str(text);
        let seq = open.seq;
        self.push(seq, text_entry(reasoning, id, text, false));
    }

    /// Closes the block `id` with the harness's authoritative text, or with the streamed text
    /// when that is empty (Claude redacts thinking). A block never streamed becomes one final;
    /// one already closed (Pi ends thinking after its text started) is not repeated.
    pub(crate) fn finish(&mut self, reasoning: bool, id: &str, text: &str) {
        let matches = self
            .open
            .as_ref()
            .is_some_and(|o| o.reasoning == reasoning && o.id == id);
        if matches {
            if let Some(mut open) = self.open.take() {
                if !text.is_empty() {
                    open.text = text.to_owned();
                }
                self.close(open);
            }
        } else if !text.is_empty() && !self.closed.iter().any(|(r, i)| *r == reasoning && i == id) {
            self.flush();
            let seq = self.take_seq();
            self.close(OpenText {
                seq,
                reasoning,
                id: id.to_owned(),
                text: text.to_owned(),
            });
        }
    }

    pub(crate) fn flush(&mut self) {
        if let Some(open) = self.open.take() {
            self.close(open);
        }
    }

    fn close(&mut self, open: OpenText) {
        self.push(
            open.seq,
            text_entry(open.reasoning, &open.id, &open.text, true),
        );
        if self.closed.len() == 64 {
            self.closed.pop_front();
        }
        self.closed.push_back((open.reasoning, open.id));
    }

    pub(crate) fn tool_call(&mut self, id: &str, tool: ToolKind, name: &str, input: &Value) {
        let (input, truncated) = bounded_input(input);
        self.emit(Entry::ToolCall {
            id: id.to_owned(),
            tool,
            name: name.to_owned(),
            summary: summarize(name, input.as_ref()),
            input,
            truncated,
        });
    }
}

fn text_entry(reasoning: bool, id: &str, text: &str, r#final: bool) -> Entry {
    let (id, text) = (id.to_owned(), text.to_owned());
    if reasoning {
        Entry::Reasoning { id, text, r#final }
    } else {
        Entry::AssistantText { id, text, r#final }
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The longest prefix of `text` within `max` bytes, on a char boundary.
pub(crate) fn bounded(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub(crate) fn bounded_output(text: &str) -> (String, bool) {
    let cut = bounded(text, MAX_TOOL_OUTPUT);
    (cut.to_owned(), cut.len() < text.len())
}

/// Small inputs pass as JSON; large ones become their truncated JSON text.
pub(crate) fn bounded_input(input: &Value) -> (Option<Value>, bool) {
    if input.is_null() {
        return (None, false);
    }
    let text = input.to_string();
    if text.len() <= MAX_TOOL_INPUT {
        (Some(input.clone()), false)
    } else {
        (
            Some(Value::from(bounded(&text, MAX_TOOL_INPUT).to_owned())),
            true,
        )
    }
}

/// `name: detail` from the first well-known input field, capped at 300 bytes.
pub(crate) fn summarize(name: &str, input: Option<&Value>) -> String {
    let detail = input.and_then(|input| {
        [
            "command",
            "cmd",
            "file_path",
            "path",
            "url",
            "pattern",
            "query",
            "description",
        ]
        .iter()
        .find_map(|key| input[*key].as_str())
    });
    let text = match (detail, input) {
        (Some(detail), _) => format!("{name}: {detail}"),
        (None, Some(Value::String(s))) => format!("{name}: {s}"),
        (None, Some(input)) if !input.is_null() => format!("{name} {input}"),
        _ => name.to_owned(),
    };
    if text.len() <= SUMMARY {
        text
    } else {
        format!("{}...", bounded(&text, SUMMARY))
    }
}

/// Joined `text` fields of an array of content parts, or the value itself as text.
pub(crate) fn content_text(content: &Value) -> String {
    match content {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            let texts: Vec<&str> = parts
                .iter()
                .filter_map(|part| {
                    part["text"]
                        .as_str()
                        .or_else(|| part["content"]["text"].as_str())
                })
                .collect();
            if texts.is_empty() && !parts.is_empty() {
                content.to_string()
            } else {
                texts.join("\n")
            }
        }
        other => other.to_string(),
    }
}

/// A JSON-RPC id as an approval or map key.
pub(crate) fn id_key(id: &Value) -> String {
    match id {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn opt_str(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// A random RFC 4122 v4 UUID from the kernel.
pub(crate) fn new_uuid() -> std::io::Result<String> {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}
