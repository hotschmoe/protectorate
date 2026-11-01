//! The segment codec. Arbitrary bytes must decode to an error or a segment, never a panic.
//! Docs built from the input must encode, decode back exactly, and have every token the
//! tokenizer gives in their postings; flipping any byte or cutting the file must be rejected.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_history::segment::{DocIn, decode, encode, walk};
use pt_history::token::tokenize;
use pt_history::{Harness, Role};

fuzz_target!(|data: &[u8]| {
    let _ = decode(data);
    if data.len() < 2 {
        return;
    }
    let flip = u16::from_le_bytes([data[0], data[1]]) as usize;
    let text = String::from_utf8_lossy(&data[2..]);
    let pieces: Vec<&str> = text.split('\n').take(64).collect();
    let files = vec!["src/lib.rs".to_owned(), "Cargo.toml".to_owned()];
    let harnesses = [
        None,
        Some(Harness::Claude),
        Some(Harness::Codex),
        Some(Harness::Grok),
        Some(Harness::Pi),
    ];
    let docs: Vec<DocIn<'_>> = pieces
        .iter()
        .enumerate()
        .map(|(i, t)| DocIn {
            id: i as u64 * 3 + u64::from(data[0]),
            time_ms: i as u64 * 1000,
            offset: i as u64 * 17,
            turn: i as u32 / 2,
            role: Role::ALL[i % Role::ALL.len()],
            harness: harnesses[i % harnesses.len()],
            session: if i % 2 == 0 { "s1" } else { "s2" },
            sleeve: (i % 3 == 0).then_some("api"),
            source: "claude:/logs/s1.jsonl",
            tool: (i % 4 == 0).then_some("Bash"),
            files: if i % 2 == 0 { &files[..] } else { &[] },
            text: t,
        })
        .collect();
    let bytes = encode(&docs);
    let d = decode(&bytes).expect("an encoded segment decodes");
    assert_eq!(d.docs.len(), docs.len());
    let mut scratch = Vec::new();
    for (local, (m, doc)) in d.docs.iter().zip(&docs).enumerate() {
        assert_eq!(m.id, doc.id);
        assert_eq!(
            (m.time_ms, m.offset, m.turn),
            (doc.time_ms, doc.offset, doc.turn)
        );
        assert_eq!((m.role, m.harness), (doc.role, doc.harness));
        assert_eq!(d.strings[m.session as usize], doc.session);
        assert_eq!(d.strings[m.source as usize], doc.source);
        let s = (d.texts.start + m.text_off) as usize;
        assert_eq!(&bytes[s..s + m.text_len as usize], doc.text.as_bytes());
        let files: Vec<&str> = m
            .files
            .iter()
            .map(|f| d.strings[*f as usize].as_str())
            .collect();
        assert_eq!(
            files,
            doc.files.iter().map(String::as_str).collect::<Vec<_>>()
        );
        let mut toks = Vec::new();
        let n = tokenize(doc.text, |t| toks.push((t.term.to_owned(), t.pos)));
        assert_eq!(m.len, n);
        for (term, pos) in toks {
            let p = d
                .dict
                .find(&term)
                .expect("every token is in the dictionary");
            let s = (d.postings.start + p.off) as usize;
            let mut found = false;
            walk(
                &bytes[s..s + p.len as usize],
                d.docs.len() as u32,
                &mut scratch,
                |doc, ps| {
                    found |= doc == local as u32 && ps.binary_search(&pos).is_ok();
                },
            )
            .expect("postings walk");
            assert!(found, "token {term:?} at {pos} is posted");
        }
    }
    let mut bad = bytes.clone();
    let i = flip % bad.len();
    bad[i] ^= 0xa5;
    assert!(decode(&bad).is_err(), "a flipped byte at {i} is rejected");
    assert!(
        decode(&bytes[..bytes.len() - 1 - flip % (bytes.len() - 1)]).is_err(),
        "a cut file is rejected"
    );
});
