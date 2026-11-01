//! SHA256SUMS: whatever parses is printed back in sha256sum's format and parses to the same
//! entries, and only plain file names are ever accepted.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_update::Sums;

fuzz_target!(|data: &[u8]| {
    let Ok(sums) = Sums::parse(data) else {
        return;
    };
    let mut text = String::new();
    for name in sums.names() {
        assert!(!name.is_empty() && !name.contains(['/', '\\', '\n']), "{name:?}");
        assert!(name != "." && name != "..");
        for b in sums.get(name).expect("listed name") {
            text.push_str(&format!("{b:02x}"));
        }
        text.push_str("  ");
        text.push_str(name);
        text.push('\n');
    }
    let again = Sums::parse(text.as_bytes()).expect("printed sums parse");
    assert_eq!(again, sums);
    assert!(sums.verify_bytes("not/listed", b"").is_err());
});
