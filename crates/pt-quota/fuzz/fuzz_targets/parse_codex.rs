#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pt_quota::codex::parse_usage(data);
    let _ = pt_quota::codex::parse_login(data);
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = pt_quota::codex::jwt_expiry_ms(s);
    }
});
