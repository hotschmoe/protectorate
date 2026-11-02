use pt_drivers::Harness;

use crate::doc::{Doc, Role};

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
