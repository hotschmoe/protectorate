#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pt_quota::grok::parse_billing(data);
    let _ = pt_quota::grok::pick_login(data);
});
