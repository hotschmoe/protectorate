//! Hashes real files and compares with the `sha256sum` and `sha1sum` binaries.

use pt_hash::{Sha1, Sha256, from_hex, from_hex_array, sha1, sha256, sha256_file, to_hex};
use std::path::{Path, PathBuf};
use std::process::Command;

fn bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn coreutils(tool: &str, path: &Path) -> String {
    let out = Command::new(tool)
        .arg(path)
        .output()
        .expect("run coreutils hasher");
    assert!(out.status.success(), "{tool} failed");
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn dir() -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pt-hash-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn matches_coreutils() {
    let dir = dir();
    let sizes = [
        0, 1, 3, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1000, 65_536, 3_000_017,
    ];
    for (i, len) in sizes.into_iter().enumerate() {
        let data = bytes(len, i as u64 + 7);
        let path = dir.join(format!("f{len}"));
        std::fs::write(&path, &data).unwrap();

        let want256 = coreutils("sha256sum", &path);
        let want1 = coreutils("sha1sum", &path);
        assert_eq!(
            to_hex(&sha256_file(&path).unwrap()),
            want256,
            "sha256_file len {len}"
        );
        assert_eq!(to_hex(&sha256(&data)), want256, "sha256 len {len}");
        assert_eq!(to_hex(&sha1(&data)), want1, "sha1 len {len}");

        let (mut h256, mut h1) = (Sha256::new(), Sha1::new());
        let mut rest = &data[..];
        let mut step = 1;
        while !rest.is_empty() {
            let n = step.min(rest.len());
            h256.update(&rest[..n]);
            h1.update(&rest[..n]);
            rest = &rest[n..];
            step = step * 3 % 701 + 1;
        }
        assert_eq!(
            to_hex(&h256.finalize()),
            want256,
            "incremental sha256 len {len}"
        );
        assert_eq!(to_hex(&h1.finalize()), want1, "incremental sha1 len {len}");

        assert_eq!(from_hex(&want256).unwrap(), sha256(&data));
        assert_eq!(from_hex_array::<20>(&want1).unwrap(), sha1(&data));
    }
}
