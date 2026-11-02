use pt_drivers::Harness;
use pt_json::Value;

use super::text::{TEXT_MAX, TOOL_MAX, take_files};
use super::time::parse_time;
use crate::doc::Role;
use crate::tail::clip;

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

impl State {
    pub(super) fn push(&mut self, out: &mut Vec<Item>, role: Role, text: &str, tool: Option<&str>) {
        self.push_files(out, role, text, tool, Vec::new());
    }

    pub(super) fn push_files(
        &mut self,
        out: &mut Vec<Item>,
        role: Role,
        text: &str,
        tool: Option<&str>,
        files: Vec<String>,
    ) {
        let text = text.trim();
        let files = take_files(files.iter().map(String::as_str));
        if text.is_empty() && files.is_empty() {
            return;
        }
        if role == Role::User {
            self.turn = self.turn.saturating_add(1);
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

    pub(super) fn usage(&self, out: &mut Vec<Item>, tokens: u64, window: Option<u64>) {
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

    pub(super) fn compacted(&self, out: &mut Vec<Item>) {
        out.push(Item::Compacted {
            session: self.session.clone(),
        });
    }

    /// Takes an ISO 8601 timestamp (or epoch milliseconds) when the record has one.
    pub(super) fn stamp(&mut self, v: &Value) {
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
