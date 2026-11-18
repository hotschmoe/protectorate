#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pt_quota::codex::parse_usage(data);
    let _ = pt_quota::codex::parse_login(data);
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = pt_quota::codex::jwt_expiry_ms(s);
    }
    if let Some(&n) = data.first() {
        let seconds = 1 + u64::from(n) * 2;
        let usage = format!(
            r#"{{"rate_limit":{{"primary_window":{{"used_percent":50,"limit_window_seconds":{seconds},"reset_at":1800000090}}}}}}"#
        );
        let snapshot = pt_quota::codex::parse_usage(usage.as_bytes()).expect("valid usage");
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].length_ms, Some(seconds * 1000));
    }
});
