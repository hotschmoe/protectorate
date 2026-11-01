//! The history search index (SPEC 3.7): native harness session logs and the hub's per-sleeve
//! transcripts, read only and incrementally, into our own on-disk keyword index with BM25.
//!
//! The index is derived: the logs stay authoritative, and a fresh directory rebuilt from the
//! same sources gives the same results. Only the sources the caller ingests are indexed (D21:
//! the hub passes the sessions it launched).
//!
//! ```text
//! let mut ix = Index::open(dir)?;
//! ix.ingest(&Source::new(SourceKind::Claude, path, session_id).sleeve("api"))?;
//! ix.commit()?;                                  // ingested docs become searchable
//! let hits = ix.search(&Query::all("journal torn tail"), &Filters::default(), 8)?;
//! let pre = ix.search(&pt_history::brief_query(task), &filters, 3)?; // a brief's pre-run hits
//! let turns = ix.read(&Selector::Turns { session, from: 3, to: 5 })?;
//! ```
//!
//! Directory layout:
//!
//! ```text
//! lock            held with an exclusive file lock while an Index is open
//! manifest        "PTHISTMF <crc32c hex>\n" + JSON: segments, source cursors, drops, usage
//! seg-<n>         immutable segments (see `segment`), each written to a temp name and renamed
//! ```

#![forbid(unsafe_code)]

mod discover;
mod extract;
mod index;
mod search;
#[doc(hidden)]
pub mod segment;
pub mod tail;
pub mod token;

use std::path::PathBuf;

pub use discover::discover;
pub use extract::{TEXT_MAX, TOOL_MAX, parse_time};
pub use index::{Index, Ingested, Stats};
pub use pt_drivers::Harness;

pt_json::string_enum! {
    /// Where a log comes from. `Hub` is the hub's own transcript of normalized entries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum SourceKind {
        Claude = "claude",
        Codex = "codex",
        Grok = "grok",
        Pi = "pi",
        Hub = "hub",
    }

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
    fn from_code(c: u64) -> Option<Role> {
        Role::ALL.get(usize::try_from(c).ok()?).copied()
    }
}

/// One log file to index, from the caller's allowlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    pub path: PathBuf,
    /// The native session id (the sleeve id for a hub transcript, until its `session_info`).
    pub session: String,
    pub sleeve: Option<String>,
    /// Read no further than this byte offset (a frozen view of a live file).
    pub limit: Option<u64>,
    /// Sessions whose docs and usage this source does not emit, because another source
    /// indexes them (a hub transcript repeats every headless session's native log).
    pub skip: Vec<String>,
}

impl Source {
    pub fn new(kind: SourceKind, path: impl Into<PathBuf>, session: impl Into<String>) -> Source {
        Source {
            kind,
            path: path.into(),
            session: session.into(),
            sleeve: None,
            limit: None,
            skip: Vec::new(),
        }
    }

    pub fn sleeve(mut self, sleeve: impl Into<String>) -> Source {
        self.sleeve = Some(sleeve.into());
        self
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Every word (or quoted phrase) must match.
    #[default]
    All,
    /// Any word may match; BM25 ranks.
    Any,
    /// The whole query is one phrase.
    Phrase,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub mode: Mode,
}

impl Query {
    pub fn all(text: impl Into<String>) -> Query {
        Query {
            text: text.into(),
            mode: Mode::All,
        }
    }

    pub fn any(text: impl Into<String>) -> Query {
        Query {
            text: text.into(),
            mode: Mode::Any,
        }
    }

    pub fn phrase(text: impl Into<String>) -> Query {
        Query {
            text: text.into(),
            mode: Mode::Phrase,
        }
    }
}

/// Every set field must match. `sleeves` and `roles` empty mean any; times are inclusive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    /// Any of these sleeves.
    pub sleeves: Vec<String>,
    pub harness: Option<Harness>,
    pub session: Option<String>,
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub roles: Vec<Role>,
}

/// Up to [`SNIPPET`] bytes around the best cluster of matches; `highlights` are byte
/// ranges within `text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    pub highlights: Vec<(usize, usize)>,
}

impl Snippet {
    /// The text with every highlight wrapped in `open` and `close`.
    pub fn marked(&self, open: &str, close: &str) -> String {
        let mut out = String::with_capacity(self.text.len() + 8 * self.highlights.len());
        let mut at = 0;
        for &(s, e) in &self.highlights {
            out.push_str(&self.text[at..s]);
            out.push_str(open);
            out.push_str(&self.text[s..e]);
            out.push_str(close);
            at = e;
        }
        out.push_str(&self.text[at..]);
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub doc: Doc,
    pub score: f64,
    pub snippet: Snippet,
}

/// What [`Index::read`] returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Doc(DocId),
    /// Docs of `session` whose turn is in `from..=to`, in order.
    Turns {
        session: String,
        from: u32,
        to: u32,
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

pub use search::{SNIPPET, brief_query};

/// Drives one extractor over arbitrary lines for the fuzz targets: the first byte picks the
/// source kind, the rest is split on newlines. Returns the number of docs. Not a stable API.
#[doc(hidden)]
pub fn fuzz_extract(data: &[u8]) -> usize {
    let Some((&k, rest)) = data.split_first() else {
        return 0;
    };
    let kind = SourceKind::ALL[k as usize % SourceKind::ALL.len()];
    let mut st = extract::State::default();
    let mut out = Vec::new();
    for l in rest.split(|b| *b == b'\n') {
        extract::line(kind, l, &mut st, &mut out);
    }
    let mut docs = 0;
    for item in &out {
        if let extract::Item::Doc(d) = item {
            let max = match d.role {
                Role::ToolCall | Role::ToolError => TOOL_MAX,
                _ => TEXT_MAX,
            };
            assert!(d.text.len() <= max, "doc text over its budget");
            assert!(!d.text.is_empty() || !d.files.is_empty(), "empty doc");
            docs += 1;
        }
    }
    docs
}
