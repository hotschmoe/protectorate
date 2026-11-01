//! Query evaluation: terms and phrases over the segments' postings, BM25 over all live docs,
//! filters, and snippets with highlights.

use std::collections::{HashMap, HashSet};
use std::io;

use crate::index::{Index, Seg};
use crate::segment::{self, Meta};
use crate::token::tokenize;
use crate::{Filters, Hit, Mode, Query, Snippet};

/// Snippet length in bytes.
pub const SNIPPET: usize = 240;
const K1: f64 = 1.2;
const B: f64 = 0.75;
/// Terms the brief derives from a task text.
const BRIEF_TERMS: usize = 12;

/// A query unit: one term, or parts that must sit at consecutive positions.
type QTerm = Vec<String>;

/// Words become terms (or phrases of their parts: `pt-history`, `parseBytes`); a quoted span
/// is one phrase. `Mode::Phrase` joins everything into one phrase.
pub(crate) fn parse(q: &Query) -> Vec<QTerm> {
    let mut out: Vec<QTerm> = Vec::new();
    for (i, chunk) in q.text.split('"').enumerate() {
        let quoted = i % 2 == 1;
        let mut runs: Vec<QTerm> = Vec::new();
        tokenize(chunk, |t| {
            if t.whole {
                return;
            }
            let r = t.run as usize;
            if runs.len() <= r {
                runs.resize(r + 1, Vec::new());
            }
            runs[r].push(t.term.to_owned());
        });
        runs.retain(|r| !r.is_empty());
        if quoted {
            let phrase: QTerm = runs.into_iter().flatten().collect();
            if !phrase.is_empty() {
                out.push(phrase);
            }
        } else {
            out.extend(runs);
        }
    }
    if q.mode == Mode::Phrase {
        let all: QTerm = out.into_iter().flatten().collect();
        return if all.is_empty() {
            Vec::new()
        } else {
            vec![all]
        };
    }
    let mut seen = HashSet::new();
    out.retain(|t| seen.insert(t.clone()));
    out
}

struct Resolved {
    /// Empty means any sleeve.
    sleeves: Vec<u32>,
    session: Option<u32>,
}

/// The filters' strings as symbols; `None` when no doc can match.
fn resolve(ix: &Index, f: &Filters) -> Option<Resolved> {
    let sleeves: Vec<u32> = f.sleeves.iter().filter_map(|s| ix.syms.get(s)).collect();
    if !f.sleeves.is_empty() && sleeves.is_empty() {
        return None;
    }
    Some(Resolved {
        sleeves,
        session: match &f.session {
            Some(s) => Some(ix.syms.get(s)?),
            None => None,
        },
    })
}

fn pass(m: &Meta, f: &Filters, r: &Resolved) -> bool {
    (r.sleeves.is_empty() || r.sleeves.contains(&m.sleeve))
        && r.session.is_none_or(|s| m.session == s)
        && f.harness.is_none_or(|h| m.harness == Some(h))
        && f.since_ms.is_none_or(|t| m.time_ms >= t)
        && f.until_ms.is_none_or(|t| m.time_ms <= t)
        && (f.roles.is_empty() || f.roles.contains(&m.role))
}

/// Live docs of one segment matching `q`, with the match count per doc.
fn matches(ix: &Index, seg: &Seg, q: &QTerm) -> io::Result<Vec<(u32, u32)>> {
    let ndocs = seg.docs.len() as u32;
    let bad = |e: segment::Corrupt| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("history {}: {e}", seg.name),
        )
    };
    let mut scratch = Vec::new();
    let mut out = Vec::new();
    if q.len() == 1 {
        let Some(p) = seg.dict.find(&q[0]) else {
            return Ok(out);
        };
        let bytes = seg.postings(p)?;
        segment::walk(&bytes, ndocs, &mut scratch, |d, pos| {
            if ix.live(&seg.docs[d as usize]) {
                out.push((d, pos.len() as u32));
            }
        })
        .map_err(bad)?;
        return Ok(out);
    }
    // Phrase: positions of every part per doc, intersected from the rarest part outward.
    let mut parts: Vec<HashMap<u32, Vec<u32>>> = Vec::with_capacity(q.len());
    let mut order: Vec<(u32, usize)> = Vec::with_capacity(q.len());
    for (i, t) in q.iter().enumerate() {
        let Some(p) = seg.dict.find(t) else {
            return Ok(out);
        };
        order.push((p.df, i));
    }
    order.sort_unstable();
    parts.resize(q.len(), HashMap::new());
    let mut allowed: Option<HashSet<u32>> = None;
    for &(_, i) in &order {
        let p = seg.dict.find(&q[i]).expect("found above");
        let bytes = seg.postings(p)?;
        let mut map = HashMap::new();
        segment::walk(&bytes, ndocs, &mut scratch, |d, pos| {
            if allowed.as_ref().is_none_or(|a| a.contains(&d)) {
                map.insert(d, pos.to_vec());
            }
        })
        .map_err(bad)?;
        allowed = Some(map.keys().copied().collect());
        parts[i] = map;
        if allowed.as_ref().is_some_and(HashSet::is_empty) {
            return Ok(out);
        }
    }
    let mut docs: Vec<u32> = allowed.unwrap_or_default().into_iter().collect();
    docs.sort_unstable();
    for d in docs {
        if !ix.live(&seg.docs[d as usize]) {
            continue;
        }
        let first = &parts[0][&d];
        let n = first
            .iter()
            .filter(|&&p| {
                (1..q.len()).all(|k| {
                    p.checked_add(k as u32)
                        .is_some_and(|want| parts[k][&d].binary_search(&want).is_ok())
                })
            })
            .count() as u32;
        if n > 0 {
            out.push((d, n));
        }
    }
    Ok(out)
}

impl Index {
    /// Ranked committed docs for `query`. Ties go to the newer doc.
    pub fn search(&self, query: &Query, filters: &Filters, limit: usize) -> io::Result<Vec<Hit>> {
        let qterms = parse(query);
        if qterms.is_empty() || limit == 0 || self.live_docs == 0 {
            return Ok(Vec::new());
        }
        let Some(res) = resolve(self, filters) else {
            return Ok(Vec::new());
        };
        // Per qterm, per segment: matching docs. df counts every live doc, filtered or not.
        let mut per: Vec<Vec<Vec<(u32, u32)>>> = Vec::with_capacity(qterms.len());
        let mut df = Vec::with_capacity(qterms.len());
        for q in &qterms {
            let mut segs = Vec::with_capacity(self.segs.len());
            let mut n = 0u64;
            for s in &self.segs {
                let m = matches(self, s, q)?;
                n += m.len() as u64;
                segs.push(m);
            }
            per.push(segs);
            df.push(n);
        }
        let need = match query.mode {
            Mode::Any => 1,
            Mode::All | Mode::Phrase => qterms.len() as u32,
        };
        let n = self.live_docs as f64;
        let avgdl = (self.live_len as f64 / n).max(1.0);
        let mut ranked: Vec<(f64, u64, usize, u32)> = Vec::new();
        for (si, s) in self.segs.iter().enumerate() {
            let mut acc: HashMap<u32, (f64, u32)> = HashMap::new();
            for (qi, segs) in per.iter().enumerate() {
                let idf = (1.0 + (n - df[qi] as f64 + 0.5) / (df[qi] as f64 + 0.5)).ln();
                for &(d, tf) in &segs[si] {
                    let m = &s.docs[d as usize];
                    if !pass(m, filters, &res) {
                        continue;
                    }
                    let tf = f64::from(tf);
                    let norm = K1 * (1.0 - B + B * f64::from(m.len) / avgdl);
                    let e = acc.entry(d).or_insert((0.0, 0));
                    e.0 += idf * tf * (K1 + 1.0) / (tf + norm);
                    e.1 += 1;
                }
            }
            for (d, (score, hits)) in acc {
                if hits >= need {
                    ranked.push((score, s.docs[d as usize].id, si, d));
                }
            }
        }
        ranked.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
        ranked.truncate(limit);
        let words: HashSet<&str> = qterms.iter().flatten().map(String::as_str).collect();
        let mut hits = Vec::with_capacity(ranked.len());
        for (score, _, si, d) in ranked {
            let s = &self.segs[si];
            let doc = self.doc(s, &s.docs[d as usize])?;
            let snippet = snippet(&doc.text, &words);
            hits.push(Hit {
                doc,
                score,
                snippet,
            });
        }
        Ok(hits)
    }
}

/// The query a continuation brief runs before the new session starts: the task's identifiers,
/// paths, quoted strings and longer words, any of which may match.
pub fn brief_query(task_text: &str) -> Query {
    let terms = parse(&Query::any(task_text));
    let mut scored: Vec<(u8, usize, String)> = terms
        .into_iter()
        .filter_map(|t| {
            let joined = t.join(" ");
            let ident = t.len() > 1 || joined.bytes().any(|b| b.is_ascii_digit());
            let rank = if ident {
                0
            } else if joined.len() >= 4 {
                1
            } else {
                return None;
            };
            let text = if t.len() > 1 {
                format!("\"{joined}\"")
            } else {
                joined
            };
            Some((rank, usize::MAX - text.len(), text))
        })
        .collect();
    scored.sort();
    scored.truncate(BRIEF_TERMS);
    let text: Vec<String> = scored.into_iter().map(|(_, _, t)| t).collect();
    Query::any(text.join(" "))
}

/// The window of `text` with the most matched words, whitespace flattened to spaces.
pub(crate) fn snippet(text: &str, words: &HashSet<&str>) -> Snippet {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    tokenize(text, |t| {
        if words.contains(t.term) {
            spans.push((t.start, t.end));
        }
    });
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let lead = SNIPPET / 6;
    let (mut start, mut best) = (0, 0);
    for (i, &(s, _)) in merged.iter().enumerate() {
        let from = s.saturating_sub(lead);
        let n = merged[i..]
            .iter()
            .take_while(|&&(_, e)| e <= from + SNIPPET)
            .count();
        if n > best {
            best = n;
            start = from;
        }
    }
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    if start > 0 {
        let first = merged.iter().find(|m| m.0 >= start).map_or(start, |m| m.0);
        if let Some(ws) = text[start..first].find(char::is_whitespace) {
            let skip = ws + text[start + ws..].chars().next().map_or(1, char::len_utf8);
            if ws < lead / 2 {
                start += skip;
            }
        }
    }
    let mut end = (start + SNIPPET).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let window = &text[start..end];
    let flat: String = window
        .chars()
        .map(|c| {
            if c.is_whitespace() || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Whitespace and control chars map to one byte each only if they were one byte; keep
    // highlights correct by mapping through char boundaries.
    let mut offsets = Vec::with_capacity(window.len() + 1);
    let mut acc = 0;
    for (i, c) in window.char_indices() {
        while offsets.len() < i {
            offsets.push(acc);
        }
        offsets.push(acc);
        acc += if c.is_whitespace() || c.is_control() {
            1
        } else {
            c.len_utf8()
        };
    }
    while offsets.len() <= window.len() {
        offsets.push(acc);
    }
    let highlights = merged
        .iter()
        .filter(|&&(s, e)| s >= start && e <= end)
        .map(|&(s, e)| (offsets[s - start], offsets[e - start]))
        .collect();
    Snippet {
        text: flat,
        highlights,
    }
}
