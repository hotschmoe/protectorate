#![no_main]

//! Random appends, group commits, snapshots, reopens, torn tails and bit flips on a real journal
//! directory, checked against a model of what must survive.
//!
//! Nothing is ever deleted, so one directory per process carries over between inputs: each input
//! starts from whatever the directory holds. Snapshots stop after 64 segments and appends after
//! 1 MiB per segment so the directory stays small.

use std::fs::{self, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use pt_journal::{HEADER_LEN, Journal, Recovered};

const MAX_SEGMENTS: u64 = 64;
const MAX_SEGMENT_BYTES: u64 = 1 << 20;

fn dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| std::env::temp_dir().join(format!("pt-journal-ops-{}", std::process::id())))
}

struct Model {
    snapshot: Option<Vec<u8>>,
    frames: Vec<Vec<u8>>,
}

fn seg_path(j: &Journal) -> PathBuf {
    dir().join(format!("segment-{:020}", j.segment()))
}

fn frames_len(frames: &[Vec<u8>]) -> u64 {
    frames.iter().map(|f| 8 + f.len() as u64).sum::<u64>() + HEADER_LEN as u64
}

fn check(rec: &Recovered, m: &Model) {
    assert_eq!(rec.snapshot, m.snapshot, "snapshot");
    assert_eq!(rec.frames, m.frames, "frames after the snapshot");
}

fuzz_target!(|data: &[u8]| {
    let mut b = data.iter().copied();
    let mut next = move || b.next();
    let (mut j, rec) = Journal::open(dir()).expect("open");
    let mut m = Model {
        snapshot: rec.snapshot,
        frames: rec.frames,
    };
    let payload = |next: &mut dyn FnMut() -> Option<u8>| -> Vec<u8> {
        let n = 1 + next().unwrap_or(0) as usize % 40;
        (0..n).map(|_| next().unwrap_or(0x5a)).collect()
    };
    while let Some(op) = next() {
        let full = fs::metadata(seg_path(&j)).expect("stat").len() > MAX_SEGMENT_BYTES;
        match op % 7 {
            0 if !full => {
                let p = payload(&mut next);
                j.append(&p).expect("append");
                m.frames.push(p);
            }
            1 if !full => {
                let n = 1 + next().unwrap_or(0) as usize % 4;
                let batch: Vec<Vec<u8>> = (0..n).map(|_| payload(&mut next)).collect();
                let refs: Vec<&[u8]> = batch.iter().map(|p| &p[..]).collect();
                j.append_batch(&refs).expect("append batch");
                m.frames.extend(batch);
            }
            2 if j.segment() < MAX_SEGMENTS => {
                let mut s = payload(&mut next);
                if next().unwrap_or(0) % 4 == 0 {
                    s.clear();
                }
                j.write_snapshot(&s).expect("snapshot");
                m.snapshot = Some(s);
                m.frames.clear();
            }
            3 => {
                drop(j);
                let (j2, rec) = Journal::open(dir()).expect("reopen");
                assert_eq!(rec.cut, None, "a clean reopen cuts nothing");
                check(&rec, &m);
                j = j2;
            }
            4 if !m.frames.is_empty() => {
                // A torn last write: any prefix of the last frame stays on disk.
                let path = seg_path(&j);
                drop(j);
                let last = m.frames.last().expect("frame").len() as u64 + 8;
                let len = frames_len(&m.frames);
                let keep = len - 1 - next().unwrap_or(0) as u64 % (last - 1);
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .expect("open")
                    .set_len(keep)
                    .expect("truncate");
                m.frames.pop();
                let (j2, rec) = Journal::open(dir()).expect("reopen torn");
                let cut = rec.cut.expect("torn tail reported");
                assert_eq!(cut.offset, frames_len(&m.frames));
                assert_eq!(cut.offset + cut.dropped, keep);
                check(&rec, &m);
                j = j2;
            }
            5 if !m.frames.is_empty() => {
                // A flipped bit in the last frame (header or payload).
                let path = seg_path(&j);
                drop(j);
                let last = m.frames.last().expect("frame").len() as u64 + 8;
                let len = frames_len(&m.frames);
                let at = len - last + next().unwrap_or(0) as u64 % last;
                let bit = 1u8 << (next().unwrap_or(0) % 8);
                let f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .expect("open");
                let mut byte = [0u8];
                f.read_exact_at(&mut byte, at).expect("read");
                byte[0] ^= bit;
                f.write_all_at(&byte, at).expect("write");
                drop(f);
                m.frames.pop();
                let (j2, rec) = Journal::open(dir()).expect("reopen damaged");
                let cut = rec.cut.expect("damage reported");
                assert_eq!(cut.offset, frames_len(&m.frames));
                check(&rec, &m);
                j = j2;
            }
            6 if m.frames.len() >= 2 => {
                // A flipped payload bit in an earlier frame: open refuses the segment and leaves
                // it untouched; with the bit restored it opens cleanly.
                let path = seg_path(&j);
                drop(j);
                let i = next().unwrap_or(0) as usize % (m.frames.len() - 1);
                let at = frames_len(&m.frames[..i])
                    + 8
                    + next().unwrap_or(0) as u64 % m.frames[i].len() as u64;
                let bit = 1u8 << (next().unwrap_or(0) % 8);
                let flip = || {
                    let f = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                        .expect("open");
                    let mut byte = [0u8];
                    f.read_exact_at(&mut byte, at).expect("read");
                    byte[0] ^= bit;
                    f.write_all_at(&byte, at).expect("write");
                };
                flip();
                let before = fs::read(&path).expect("read");
                let err = Journal::open(dir()).err().expect("open refuses corruption");
                assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
                assert_eq!(fs::read(&path).expect("read"), before, "segment untouched");
                flip();
                let (j2, rec) = Journal::open(dir()).expect("reopen restored");
                assert_eq!(rec.cut, None);
                check(&rec, &m);
                j = j2;
            }
            _ => {}
        }
    }
    drop(j);
    let (_j, rec) = Journal::open(dir()).expect("final reopen");
    assert_eq!(rec.cut, None);
    check(&rec, &m);
});
