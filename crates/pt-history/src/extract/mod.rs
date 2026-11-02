//! Per-harness extractors: one native log line in, zero or more documents and usage facts out.
//! Unknown record kinds produce nothing. What is kept: user prompts, assistant text (not
//! thinking), tool calls as name, paths and command, failed tool results, and compaction
//! summaries.

mod claude;
mod codex;
mod grok;
mod hub;
mod item;
mod pi;
mod text;
mod time;

use pt_drivers::Harness;
use pt_json::Value;

use crate::source::SourceKind;

pub use item::{Draft, Item, State};
pub(crate) use text::take_files;
pub use text::{TEXT_MAX, TOOL_MAX};
pub use time::parse_time;

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
