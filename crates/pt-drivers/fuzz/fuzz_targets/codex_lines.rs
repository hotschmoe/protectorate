//! Arbitrary stdout lines and calls into the Codex protocol mapper: it must never panic, and
//! every entry it emits must survive a JSON round trip.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    pt_drivers::fuzz_feed(pt_drivers::Harness::Codex, data);
});
