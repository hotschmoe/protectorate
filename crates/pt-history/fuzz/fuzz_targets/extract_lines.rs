//! Arbitrary lines into every extractor (the first byte picks the source kind): it must never
//! panic, and every document must be non-empty and within its text budget.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    pt_history::fuzz_extract(data);
});
