//! The tokenizer, shared by indexing, queries and snippets.
//!
//! - A run is a maximal sequence of alphanumerics and `_ . / -` (paths, identifiers, versions).
//!   Runs longer than [`MAX_RUN`] bytes (base64, hex dumps) are dropped.
//! - A run splits on those separators into segments, and a segment splits into camelCase parts
//!   (`parseBytes` is `parse`, `bytes`; `HTTPServer` is `http`, `server`; `sha256Sum` is
//!   `sha256`, `sum`) unless it has more than [`MAX_HUMPS`] of them.
//! - Every part is lowercased and takes the next position, so `crates/pt-history` is the
//!   adjacent parts `crates pt history` and a phrase query finds it inside any longer path.
//! - A segment of several case parts is also emitted whole (`parsebytes`) at the position of
//!   its first part, as an extra term that takes no position.
//! - Parts and segments longer than [`MAX_PART`] bytes are dropped.

pub const MAX_RUN: usize = 200;
pub const MAX_PART: usize = 48;
pub const MAX_HUMPS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tok<'a> {
    pub term: &'a str,
    pub pos: u32,
    pub start: usize,
    pub end: usize,
    /// The whole-segment extra term, not a part.
    pub whole: bool,
    /// Index of the run the token came from.
    pub run: u32,
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || is_sep(c)
}

pub fn is_sep(c: char) -> bool {
    matches!(c, '_' | '.' | '/' | '-')
}

/// Calls `f` for every token of `text` in order and returns the number of positions used.
pub fn tokenize(text: &str, mut f: impl FnMut(Tok<'_>)) -> u32 {
    let mut pos = 0u32;
    let mut run = 0u32;
    let mut buf = String::new();
    let mut parts = [(0, 0); MAX_HUMPS];
    let mut chars = text.char_indices().peekable();
    while let Some(&(s, c)) = chars.peek() {
        if !is_word(c) {
            chars.next();
            continue;
        }
        let mut e = s;
        while let Some(&(i, c)) = chars.peek() {
            if !is_word(c) {
                break;
            }
            e = i + c.len_utf8();
            chars.next();
        }
        if e - s <= MAX_RUN {
            let r = &text[s..e];
            let mut seg_start = None;
            for (i, c) in r.char_indices().chain(std::iter::once((r.len(), '/'))) {
                match (is_sep(c), seg_start) {
                    (true, Some(a)) => {
                        segment(
                            text,
                            s + a,
                            s + i,
                            run,
                            &mut pos,
                            &mut buf,
                            &mut parts,
                            &mut f,
                        );
                        seg_start = None;
                    }
                    (false, None) => seg_start = Some(i),
                    _ => {}
                }
            }
        }
        run += 1;
    }
    pos
}

#[allow(clippy::too_many_arguments)]
fn segment(
    text: &str,
    a: usize,
    b: usize,
    run: u32,
    pos: &mut u32,
    buf: &mut String,
    parts: &mut [(usize, usize); MAX_HUMPS],
    f: &mut impl FnMut(Tok<'_>),
) {
    let seg = &text[a..b];
    let count = humps(seg, parts);
    let first = *pos;
    let mut emitted = 0;
    for &(ps, pe) in &parts[..count] {
        if pe - ps > MAX_PART {
            continue;
        }
        lower(&seg[ps..pe], buf);
        f(Tok {
            term: buf,
            pos: *pos,
            start: a + ps,
            end: a + pe,
            whole: false,
            run,
        });
        *pos += 1;
        emitted += 1;
    }
    if count > 1 && emitted > 0 && seg.len() <= MAX_PART {
        lower(seg, buf);
        f(Tok {
            term: buf,
            pos: first,
            start: a,
            end: b,
            whole: true,
            run,
        });
    }
}

/// Splits a separator-free segment into camelCase parts (byte ranges within `seg`).
fn humps(seg: &str, out: &mut [(usize, usize); MAX_HUMPS]) -> usize {
    let mut chars = seg.char_indices().peekable();
    let Some((_, mut prev)) = chars.next() else {
        return 0;
    };
    let mut start = 0;
    let mut count = 0;
    while let Some((at, c)) = chars.next() {
        let next_lower = chars.peek().is_some_and(|(_, n)| n.is_lowercase());
        let boundary = c.is_uppercase()
            && (prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next_lower));
        if boundary {
            if count == MAX_HUMPS - 1 {
                out[0] = (0, seg.len());
                return 1;
            }
            out[count] = (start, at);
            count += 1;
            start = at;
        }
        prev = c;
    }
    out[count] = (start, seg.len());
    count + 1
}

fn lower(s: &str, buf: &mut String) {
    buf.clear();
    if s.is_ascii() {
        buf.push_str(s);
        buf.make_ascii_lowercase();
    } else {
        for c in s.chars() {
            buf.extend(c.to_lowercase());
        }
    }
}
