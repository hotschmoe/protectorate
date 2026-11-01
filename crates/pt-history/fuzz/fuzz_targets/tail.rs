//! The tail reader against a reference. The input's first bytes pick the max line length, the
//! read window and up to eight growth points; the rest is the file. The file is "written" up to
//! each growth point and read from the previous cursor each time. The lines seen across all
//! reads must equal the reference split of the final prefix: complete, non-empty lines no longer
//! than the max, never a partial last line, never one twice. Clipping every line at arbitrary
//! points must land on a char boundary and be the longest such prefix (or suffix).

#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_history::tail::{advance, clip, clip_tail};

fn reference(data: &[u8], max_line: usize) -> Vec<(u64, Vec<u8>)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in data.iter().enumerate() {
        if *b == b'\n' {
            let l = &data[start..i];
            if !l.is_empty() && l.len() <= max_line {
                out.push((start as u64, l.to_vec()));
            }
            start = i + 1;
        }
    }
    out
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let max_line = 1 + data[0] as usize % 48;
    let window = 1 + data[1] as usize % 96;
    let ncuts = data[2] as usize % 9;
    if data.len() < 3 + ncuts {
        return;
    }
    let file = &data[3 + ncuts..];
    let mut cuts: Vec<usize> = data[3..3 + ncuts]
        .iter()
        .map(|c| (*c as usize * file.len()) / 255)
        .collect();
    cuts.push(file.len());
    cuts.sort_unstable();

    let mut seen: Vec<(u64, Vec<u8>)> = Vec::new();
    let (mut offset, mut skipping) = (0u64, false);
    for &cut in &cuts {
        let visible = &file[..cut];
        let a = advance(
            |off, buf| {
                let off = off as usize;
                let n = buf.len().min(visible.len().saturating_sub(off));
                buf[..n].copy_from_slice(&visible[off..off + n]);
                Ok(n)
            },
            offset,
            cut as u64,
            skipping,
            window,
            max_line,
            |off, l| seen.push((off, l.to_vec())),
        )
        .expect("in-memory reads do not fail");
        assert!(
            a.offset >= offset && a.offset <= cut as u64,
            "cursor stays in range"
        );
        let consumed = &visible[..a.offset as usize];
        assert!(
            a.skipping || consumed.is_empty() || consumed.ends_with(b"\n"),
            "a cursor outside a skipped line sits after a newline"
        );
        offset = a.offset;
        skipping = a.skipping;
    }
    assert_eq!(seen, reference(file, max_line), "lines match the reference");

    let text = String::from_utf8_lossy(file);
    for (i, &c) in data[3..3 + ncuts].iter().enumerate() {
        let at = (c as usize * 7 + i) % (text.len() + 2);
        let head = clip(&text, at);
        assert!(head.len() <= at && text.starts_with(head));
        assert!(
            head.len() == text.len()
                || text[head.len()..]
                    .chars()
                    .next()
                    .is_some_and(|ch| head.len() + ch.len_utf8() > at),
            "clip keeps the longest prefix"
        );
        let tail = clip_tail(&text, at);
        assert!(tail.len() <= at && text.ends_with(tail));
        assert!(
            tail.len() == text.len()
                || text[..text.len() - tail.len()]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| tail.len() + ch.len_utf8() > at),
            "clip_tail keeps the longest suffix"
        );
    }
    if std::str::from_utf8(file).is_ok() {
        for (_, l) in &seen {
            assert!(
                std::str::from_utf8(l).is_ok(),
                "lines never split a UTF-8 char"
            );
        }
    }
});
