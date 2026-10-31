#![no_main]

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use pt_journal::codec::{Stop, decode_frames, encode_frame};
use pt_journal::{HEADER_LEN, Journal};

fn dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("pt-journal-fuzz-{}", std::process::id()));
        fs::create_dir_all(&d).expect("create fuzz dir");
        d
    })
}

fuzz_target!(|data: &[u8]| {
    let d = decode_frames(data);

    let mut again = Vec::new();
    for f in &d.frames {
        encode_frame(f, &mut again).expect("decoded frames re-encode");
    }
    assert_eq!(again, data[..d.valid_len], "frames are exactly the valid prefix");
    assert_eq!(d.stop.is_none(), d.valid_len == data.len());

    // A stop decided by bytes already present is final: later appends never revive the tail.
    if matches!(
        d.stop,
        Some(Stop::CrcMismatch | Stop::ZeroLength | Stop::TooLarge)
    ) {
        let mut more = data.to_vec();
        encode_frame(b"later", &mut more).expect("encode");
        let d2 = decode_frames(&more);
        assert_eq!(d2.frames, d.frames);
        assert_eq!(d2.valid_len, d.valid_len);
    }

    // The same bytes as a real segment file: open recovers the same frames and truncates.
    let seg = dir().join("segment-00000000000000000000");
    let mut file = b"PTJOURNL\x01\0\0\0".to_vec();
    file.extend_from_slice(data);
    fs::write(&seg, &file).expect("write segment");
    let (journal, rec) = Journal::open(dir()).expect("open");
    drop(journal);
    assert_eq!(rec.frames.len(), d.frames.len());
    assert!(rec.frames.iter().zip(&d.frames).all(|(a, b)| a == b));
    assert_eq!(rec.cut.is_some(), d.stop.is_some());
    let len = fs::metadata(&seg).expect("stat").len();
    assert_eq!(len, (HEADER_LEN + d.valid_len) as u64);
});
