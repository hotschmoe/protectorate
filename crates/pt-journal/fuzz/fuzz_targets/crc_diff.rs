#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_journal::crc32c::{crc32c, crc32c_append};

fuzz_target!(|data: &[u8]| {
    let want = crc32c::crc32c(data);
    assert_eq!(crc32c(data), want);
    if let Some(&first) = data.first() {
        let split = first as usize % data.len();
        let head = crc32c(&data[..split]);
        assert_eq!(head, crc32c::crc32c(&data[..split]));
        assert_eq!(crc32c_append(head, &data[split..]), want);
    }
});
