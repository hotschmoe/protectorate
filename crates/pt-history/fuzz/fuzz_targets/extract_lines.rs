//! Arbitrary lines into every extractor (the first byte picks the source kind): it must never
//! panic, and every document must be non-empty and within its text budget.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pt_history::fuzz_extract(data);
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    let body: String = rest
        .iter()
        .take(512)
        .map(|b| char::from(b'a' + b % 26))
        .collect();
    if body.is_empty() {
        return;
    }
    let (kind, visible, wrapped) = if mode & 1 == 0 {
        (
            0,
            format!("<div>{body}</div>"),
            format!("<system-reminder>{body}</system-reminder>"),
        )
    } else {
        (
            1,
            format!("<3 {body}"),
            format!("<environment_context>{body}</environment_context>"),
        )
    };
    let line = |text: &str| {
        if kind == 0 {
            format!(r#"{{"type":"user","message":{{"content":"{text}"}}}}"#)
        } else {
            format!(
                r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"{text}"}}]}}}}"#
            )
        }
    };
    let mut stream = vec![kind];
    stream.extend_from_slice(line(&visible).as_bytes());
    stream.push(b'\n');
    stream.extend_from_slice(line(&wrapped).as_bytes());
    assert_eq!(pt_history::fuzz_extract(&stream), vec![visible]);
});
