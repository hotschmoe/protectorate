//! The history search index (SPEC 3.7): native harness session logs and the hub's per-sleeve
//! transcripts, read only and incrementally, into our own on-disk keyword index with BM25.
//!
//! The index is derived: the logs stay authoritative, and a fresh directory rebuilt from the
//! same sources gives the same results. Only the sources the caller ingests are indexed (D21:
//! the hub passes the sessions it launched).
//!
//! ```text
//! let mut ix = Index::open(dir, pt_history::DEFAULT_BUDGET)?;
//! ix.ingest(&Source::new(SourceKind::Claude, path, session_id).sleeve("api"))?;
//! ix.commit()?;                                  // ingested docs become searchable
//! let hits = ix.search(&Query::all("journal torn tail"), &Filters::default(), 8)?;
//! let pre = ix.search(&pt_history::brief_query(task), &filters, 3)?; // a brief's pre-run hits
//! let (turns, _more) = ix.read(&Selector::Turns { session, from: 3, to: 5, after: None }, None, READ_DOCS)?;
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
mod doc;
mod extract;
mod index;
mod query;
mod safe_open;
mod search;
#[doc(hidden)]
pub mod segment;
mod source;
pub mod tail;
pub mod token;

pub use discover::discover;
pub use doc::{ContextUsage, Doc, DocId, Role, Selector};
pub use extract::{TEXT_MAX, TOOL_MAX, parse_time};
pub use index::{DEFAULT_BUDGET, Index, Ingested, MIN_BUDGET, READ_BYTES, READ_DOCS, Stats};
pub use pt_drivers::Harness;
pub use query::{Filters, Hit, Mode, Query, Snippet};
pub use source::{Source, SourceKind};

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
            assert!(
                d.files.iter().map(String::len).sum::<usize>() <= TOOL_MAX,
                "doc paths over the tool budget"
            );
            assert!(!d.text.is_empty() || !d.files.is_empty(), "empty doc");
            docs += 1;
        }
    }
    docs
}
