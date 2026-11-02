use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use pt_json::Value;

use crate::event::{Entry, ErrorClass, Event, ToolKind};

pub(crate) const MAX_TOOL_OUTPUT: usize = 16 << 10;
pub(crate) const MAX_TOOL_INPUT: usize = 16 << 10;
/// The most text one assistant or reasoning block keeps; the rest is cut with a marker.
pub(crate) const MAX_BLOCK: usize = 1 << 20;
const TRUNCATED: &str = "\n[protectorate: block truncated at 1 MiB]";
/// Token counts above this are malformed (2^40, far past any context window or session).
const MAX_COUNT: u64 = 1 << 40;
const MAX_COST_USD: f64 = 1e6;
const SUMMARY: usize = 300;

/// Assigns seqs, coalesces text deltas into one final entry per block, and buffers what a
/// protocol step produced: entries for the hub and lines for the harness's stdin.
pub(crate) struct Out {
    g: u64,
    next_seq: u64,
    open: Option<OpenText>,
    /// Recently finalized blocks, so a late authoritative end does not repeat one.
    closed: VecDeque<Closed>,
    notes: HashSet<String>,
    /// Stdout lines that were not JSON objects.
    stray: u64,
    /// The harness refused its handshake: the session must end now.
    pub(crate) refused: bool,
    /// User messages whose line is queued but not yet written, by request id.
    unwritten: Vec<(String, Entry)>,
    pub(crate) events: Vec<Event>,
    pub(crate) writes: Vec<Write>,
}

/// A line for the harness's stdin; `request` names the user message it carries.
pub(crate) struct Write {
    pub(crate) line: Value,
    pub(crate) request: Option<String>,
}

/// A text block. Another entry in mid-stream (a steer's ack, a notice) closes it; deltas
/// that follow for the same harness id open part 2, emitted as `<id>~2`, so no two finals
/// share an id.
struct OpenText {
    seq: u64,
    reasoning: bool,
    /// The harness's id for the block.
    key: String,
    part: u32,
    text: String,
    truncated: bool,
}

struct Closed {
    reasoning: bool,
    key: String,
    parts: u32,
}

impl OpenText {
    fn id(&self) -> String {
        match self.part {
            1 => self.key.clone(),
            n => format!("{}~{n}", self.key),
        }
    }
}

impl Out {
    pub(crate) fn new(g: u64) -> Out {
        Out {
            g,
            next_seq: 1,
            open: None,
            closed: VecDeque::new(),
            notes: HashSet::new(),
            stray: 0,
            refused: false,
            unwritten: Vec::new(),
            events: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub(crate) fn write(&mut self, line: Value) {
        self.writes.push(Write {
            line,
            request: None,
        });
    }

    /// Holds the user message `entry` back until the line just written for `request_id` has
    /// left ([`Out::written`]), or the harness answers it, which proves it was read.
    pub(crate) fn after_write(&mut self, request_id: &str, entry: Entry) {
        match self.writes.last_mut() {
            Some(w) if w.request.is_none() => {
                w.request = Some(request_id.to_owned());
                self.unwritten.push((request_id.to_owned(), entry));
            }
            _ => self.emit(entry),
        }
    }

    /// The line carrying `request_id` is written: its user message enters the stream.
    pub(crate) fn written(&mut self, request_id: &str) {
        if let Some(i) = self.unwritten.iter().position(|(r, _)| r == request_id) {
            let (_, entry) = self.unwritten.remove(i);
            self.emit(entry);
        }
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

    /// Any entry other than a delta closes the open text block first. An answer to a user
    /// message is never ahead of the message itself.
    pub(crate) fn emit(&mut self, entry: Entry) {
        if let Entry::PromptAck { request_id, .. }
        | Entry::TurnStarted {
            request_id: Some(request_id),
            ..
        } = &entry
            && self.unwritten.iter().any(|(r, _)| r == request_id)
        {
            let request_id = request_id.clone();
            self.written(&request_id);
        }
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

    /// The harness answered its handshake with an error: the session cannot become ready, so
    /// the process is ended at once instead of leaving the hub to wait out its start timeout.
    pub(crate) fn refuse(&mut self, class: ErrorClass, text: impl Into<String>) {
        self.refused = true;
        self.error(class, text);
    }

    pub(crate) fn notice(&mut self, code: &str, text: impl Into<String>) {
        self.emit(Entry::Notice {
            code: Some(code.to_owned()),
            text: text.into(),
        });
    }

    /// A stdout line that is not a JSON object: output of the harness or of something it runs
    /// (an MCP server, an extension, an npm wrapper), not a breach of the protocol. It is
    /// counted and noticed at each power of two, so a chatty process cannot flood the stream.
    pub(crate) fn stray_line(&mut self, harness: &str, line: &str) {
        self.stray += 1;
        if self.stray.is_power_of_two() {
            let text = format!(
                "{harness} wrote {} stdout lines that are not JSON objects; latest: {}",
                self.stray,
                bounded(line, 120)
            );
            self.notice("stray_stdout", text);
        }
    }

    /// One notice per unmapped message type per session (NoMatch, not Desync).
    pub(crate) fn unmapped(&mut self, what: &str) {
        let what = bounded(what, 120);
        self.once(
            &format!("unmapped:{what}"),
            format!("unmapped message {what}"),
        );
    }

    fn once(&mut self, code: &str, text: String) {
        if self.notes.len() < 256 && self.notes.insert(code.to_owned()) {
            self.notice(code, text);
        }
    }

    /// A token count the harness reported: `None` when absent, and when absurd, which is
    /// noticed once per field as malformed.
    pub(crate) fn count(&mut self, what: &str, v: &Value) -> Option<u64> {
        let n = v.as_u64()?;
        if n > MAX_COUNT {
            self.once(
                &format!("malformed:{what}"),
                format!("ignored malformed {what} {n}"),
            );
            return None;
        }
        Some(n)
    }

    /// A cost in USD: finite, not negative and not absurd, else `None`.
    pub(crate) fn cost(&mut self, what: &str, v: &Value) -> Option<f64> {
        let c = v.as_f64()?;
        if !(0.0..=MAX_COST_USD).contains(&c) {
            self.once(
                &format!("malformed:{what}"),
                format!("ignored malformed {what} {}", bounded(&v.to_string(), 40)),
            );
            return None;
        }
        Some(c)
    }

    pub(crate) fn delta(&mut self, reasoning: bool, id: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        if self
            .open
            .as_ref()
            .is_some_and(|o| o.reasoning != reasoning || o.key != id)
        {
            self.flush();
        }
        if self.open.is_none() {
            let part = self
                .closed
                .iter()
                .find(|c| c.reasoning == reasoning && c.key == id)
                .map_or(1, |c| c.parts.saturating_add(1));
            let seq = self.take_seq();
            self.open = Some(OpenText {
                seq,
                reasoning,
                key: id.to_owned(),
                part,
                text: String::new(),
                truncated: false,
            });
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        // Past the cap the block keeps its marker and live deltas stop, so the live view
        // never shows more than the final keeps.
        if open.truncated {
            return;
        }
        let room = MAX_BLOCK - open.text.len();
        let text = if text.len() > room {
            open.truncated = true;
            bounded(text, room)
        } else {
            text
        };
        open.text.push_str(text);
        let (seq, block) = (open.seq, open.id());
        if !text.is_empty() {
            self.push(seq, text_entry(reasoning, &block, text, false));
        }
    }

    /// Closes the block `id` with the harness's authoritative text, or with the streamed text
    /// when that is empty (Claude redacts thinking) or the block was split, since earlier
    /// parts already hold the start. A block never streamed becomes one final; one already
    /// closed (Pi ends thinking after its text started) is not repeated.
    pub(crate) fn finish(&mut self, reasoning: bool, id: &str, text: &str) {
        let matches = self
            .open
            .as_ref()
            .is_some_and(|o| o.reasoning == reasoning && o.key == id);
        if matches {
            if let Some(mut open) = self.open.take() {
                if !text.is_empty() && open.part == 1 {
                    open.text = bounded(text, MAX_BLOCK).to_owned();
                    open.truncated = text.len() > MAX_BLOCK;
                }
                self.close(open);
            }
        } else if !text.is_empty()
            && !self
                .closed
                .iter()
                .any(|c| c.reasoning == reasoning && c.key == id)
        {
            self.flush();
            let seq = self.take_seq();
            self.close(OpenText {
                seq,
                reasoning,
                key: id.to_owned(),
                part: 1,
                text: bounded(text, MAX_BLOCK).to_owned(),
                truncated: text.len() > MAX_BLOCK,
            });
        }
    }

    pub(crate) fn flush(&mut self) {
        if let Some(open) = self.open.take() {
            self.close(open);
        }
    }

    fn close(&mut self, mut open: OpenText) {
        if open.truncated {
            open.text.push_str(TRUNCATED);
        }
        self.push(
            open.seq,
            text_entry(open.reasoning, &open.id(), &open.text, true),
        );
        if let Some(c) = self
            .closed
            .iter_mut()
            .find(|c| c.reasoning == open.reasoning && c.key == open.key)
        {
            c.parts = open.part;
            return;
        }
        if self.closed.len() == 64 {
            self.closed.pop_front();
        }
        self.closed.push_back(Closed {
            reasoning: open.reasoning,
            key: open.key,
            parts: open.part,
        });
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

/// A sum of token counts over a turn's calls, held to the same bound as one count.
pub(crate) fn add_count(a: u64, b: u64) -> u64 {
    a.saturating_add(b).min(MAX_COUNT)
}

/// A JSON-RPC id as a key that keeps its type: a number as its text, a string as `s:` and
/// the string, so `1` and `"1"` stay apart. `None` for any other JSON type.
pub(crate) fn id_key(id: &Value) -> Option<String> {
    match id {
        Value::Number(n) => Some(n.as_str().to_owned()),
        Value::String(s) => Some(format!("s:{s}")),
        _ => None,
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
