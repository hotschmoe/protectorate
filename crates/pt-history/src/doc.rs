use std::path::PathBuf;

use pt_drivers::Harness;

pt_json::string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Role {
        User = "user",
        Assistant = "assistant",
        ToolCall = "tool_call",
        ToolError = "tool_error",
        Compaction = "compaction",
    }
}

impl Role {
    pub(crate) fn from_code(c: u64) -> Option<Role> {
        Role::ALL.get(usize::try_from(c).ok()?).copied()
    }
}

/// An index-wide document id; ids grow in ingest order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocId(pub u64);

/// One indexed unit: a prompt, an assistant message, a tool call, a failed tool result or a
/// compaction summary, with its provenance in the source file.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub id: DocId,
    pub session: String,
    pub harness: Option<Harness>,
    pub sleeve: Option<String>,
    /// The user prompt count in the session up to and including this doc (0 before the first).
    pub turn: u32,
    pub role: Role,
    pub time_ms: u64,
    pub tool: Option<String>,
    pub files: Vec<String>,
    pub text: String,
    pub path: PathBuf,
    /// Byte offset of the source line in `path`.
    pub offset: u64,
}

/// What [`Index::read`] returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Doc(DocId),
    /// Docs of `session` whose turn is in `from..=to`, in order. `after` skips that doc id
    /// and every earlier one, so a turn with more docs than one page can continue.
    Turns {
        session: String,
        from: u32,
        to: u32,
        after: Option<u64>,
    },
}

pt_json::record! {
    /// A session's context fill from its own log: prompt tokens of the latest model call, the
    /// window when the log states it, and how often the harness compacted.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct ContextUsage {
        pub session: String,
        pub harness: Option<Harness>,
        pub tokens: u64 = 0,
        pub window: Option<u64>,
        pub at_ms: u64 = 0,
        pub compactions: u32 = 0,
        pub turns: u32 = 0,
    }
}
