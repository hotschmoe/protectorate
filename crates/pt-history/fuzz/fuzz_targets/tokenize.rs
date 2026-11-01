//! The tokenizer against a char-by-char reference of its rules (see `pt_history::token`):
//! the same tokens in the same order with the same positions and spans, and every part is the
//! lowercased text of its span.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_history::token::{MAX_HUMPS, MAX_PART, MAX_RUN, is_sep, is_word, tokenize};

type Tok = (String, u32, usize, usize, bool, u32);

fn lower(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

fn reference(text: &str) -> (Vec<Tok>, u32) {
    let cs: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| cs.get(i).map_or(text.len(), |c| c.0);
    let (mut out, mut pos, mut run) = (Vec::new(), 0u32, 0u32);
    let mut i = 0;
    while i < cs.len() {
        if !is_word(cs[i].1) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < cs.len() && is_word(cs[j].1) {
            j += 1;
        }
        if at(j) - at(i) <= MAX_RUN {
            let mut k = i;
            while k < j {
                if is_sep(cs[k].1) {
                    k += 1;
                    continue;
                }
                let mut l = k;
                while l < j && !is_sep(cs[l].1) {
                    l += 1;
                }
                let mut bounds = vec![k];
                for m in k + 1..l {
                    let (c, p) = (cs[m].1, cs[m - 1].1);
                    let next_lower = m + 1 < l && cs[m + 1].1.is_lowercase();
                    if c.is_uppercase()
                        && (p.is_lowercase()
                            || p.is_ascii_digit()
                            || (p.is_uppercase() && next_lower))
                    {
                        bounds.push(m);
                    }
                }
                bounds.push(l);
                if bounds.len() - 1 > MAX_HUMPS {
                    bounds = vec![k, l];
                }
                let first = pos;
                let mut emitted = 0;
                for w in bounds.windows(2) {
                    let (s, e) = (at(w[0]), at(w[1]));
                    if e - s <= MAX_PART {
                        out.push((lower(&text[s..e]), pos, s, e, false, run));
                        pos += 1;
                        emitted += 1;
                    }
                }
                let (s, e) = (at(k), at(l));
                if bounds.len() > 2 && emitted > 0 && e - s <= MAX_PART {
                    out.push((lower(&text[s..e]), first, s, e, true, run));
                }
                k = l;
            }
        }
        run += 1;
        i = j;
    }
    (out, pos)
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut got: Vec<Tok> = Vec::new();
    let n = tokenize(&text, |t| {
        assert!(!t.term.is_empty(), "no empty term");
        assert!(text.is_char_boundary(t.start) && text.is_char_boundary(t.end));
        got.push((t.term.to_owned(), t.pos, t.start, t.end, t.whole, t.run));
    });
    let (want, positions) = reference(&text);
    assert_eq!(got, want, "tokens match the reference");
    assert_eq!(n, positions, "position count matches");
});
