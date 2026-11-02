use std::path::PathBuf;

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
