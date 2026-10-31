//! Arbitrary bytes as a stream of pipelined requests, fed in small reads: never panics, and
//! every accepted head and body stays within its limit.
//! Seeds: `cargo +nightly fuzz run request corpus/request seeds/request`.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pt_http::fuzzing::request(data));
