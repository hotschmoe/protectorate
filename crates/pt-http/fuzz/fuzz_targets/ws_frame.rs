//! Arbitrary bytes as WebSocket frames from a client (or server, by the first byte): never
//! panics, messages stay within the limit, and decoded frames survive an encode round trip.
//! Seeds: `cargo +nightly fuzz run ws_frame corpus/ws_frame seeds/ws_frame`.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pt_http::fuzzing::websocket(data));
