//! Parse, write, parse again: the values must be equal and the second write byte-identical.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(v) = pt_json::parse_bytes(data) else {
        return;
    };
    let first = v.to_string();
    let mut written = String::new();
    v.write(&mut written);
    assert_eq!(first, written, "Display and write differ");
    let again = pt_json::parse(&first).expect("written JSON parses");
    assert!(again == v, "write-then-parse differs: {first}");
    assert_eq!(again.to_string(), first, "second write differs");
});
