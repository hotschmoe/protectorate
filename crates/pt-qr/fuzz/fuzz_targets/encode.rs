//! Differential against upstream qrcodegen: for every input up to past the version 40 capacity,
//! the port never panics, fails exactly when upstream does (and only past `max_bytes`), and
//! otherwise picks the same version and mask and draws the same modules. Byte 0 picks the level
//! (bit 0) and, with bit 1 set, stretches the rest to the length in bytes 1 and 2 so large versions
//! are reached from short inputs.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_qr::Ecc;
use qrcodegen::{QrCode, QrCodeEcc, QrSegment, Version};

fuzz_target!(|input: &[u8]| {
    let Some((&flags, rest)) = input.split_first() else {
        return;
    };
    let (ecc, reference_ecc) = if flags & 1 == 0 {
        (Ecc::Low, QrCodeEcc::Low)
    } else {
        (Ecc::Medium, QrCodeEcc::Medium)
    };
    let max = pt_qr::max_bytes(ecc);
    let data: Vec<u8> = if flags & 2 != 0 && rest.len() >= 2 {
        let len = usize::from(u16::from_le_bytes([rest[0], rest[1]])) % (max + 16);
        let fill = if rest.len() > 2 { &rest[2..] } else { &[0u8][..] };
        fill.iter().copied().cycle().take(len).collect()
    } else {
        rest.to_vec()
    };

    let ours = pt_qr::encode(&data, ecc);
    let theirs = QrCode::encode_segments_advanced(
        &[QrSegment::make_bytes(&data)],
        reference_ecc,
        Version::MIN,
        Version::MAX,
        None,
        false,
    );
    match (ours, theirs) {
        (Err(e), Err(_)) => {
            assert!(data.len() > max, "refused {} bytes, max {max}", data.len());
            assert_eq!(e.bytes, data.len());
        }
        (Ok(q), Ok(r)) => {
            assert!(data.len() <= max);
            assert_eq!(q.version(), r.version().value());
            assert_eq!(q.mask(), r.mask().value());
            let size = q.size() as i32;
            assert_eq!(size, r.size());
            for y in -1..=size {
                for x in -1..=size {
                    assert_eq!(q.dark(x, y), r.get_module(x, y), "module {x},{y}");
                }
            }
            let quiet = flags >> 4;
            let text = q.to_terminal(quiet, flags & 4 != 0);
            let width = q.size() + 2 * usize::from(quiet);
            assert_eq!(text.lines().count(), width.div_ceil(2));
            if flags & 4 == 0 {
                assert!(text.lines().all(|l| l.chars().count() == width));
            }
        }
        (ours, theirs) => panic!(
            "{} bytes: port {:?}, upstream {}",
            data.len(),
            ours,
            theirs.map_or("err".to_string(), |r| format!("v{}", r.version().value()))
        ),
    }
});
