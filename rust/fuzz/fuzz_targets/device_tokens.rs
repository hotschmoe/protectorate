#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| protectorate_fuzz::device_tokens(data));
