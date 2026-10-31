//! Crash recovery on a real file: a crash can leave any prefix of the last write on disk, so the
//! journal is cut at every byte of its last frame and reopened. The rest of the journal's
//! behavior is covered by the `recover`, `ops` and `crc_diff` fuzz targets.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pt_journal::{Cut, CutReason, HEADER_LEN, Journal, Stop};

fn fresh_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("pt-journal")
        .join(format!("{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create test dir");
    dir
}

fn segment(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("segment-{seq:020}"))
}

fn payloads() -> Vec<Vec<u8>> {
    vec![
        b"first".to_vec(),
        (0..=255u8).collect(),
        b"the last frame, which the tests damage".to_vec(),
    ]
}

/// Writes `payloads` to a fresh journal and returns the dir and the offset of the last frame.
fn written(name: &str) -> (PathBuf, usize) {
    let dir = fresh_dir(name);
    let p = payloads();
    let (mut j, rec) = Journal::open(&dir).expect("open");
    assert!(rec.snapshot.is_none() && rec.frames.is_empty() && rec.cut.is_none());
    j.append(&p[0]).expect("append");
    j.append_batch(&[&p[1], &p[2]]).expect("append batch");
    drop(j);
    let last = HEADER_LEN + 8 + p[0].len() + 8 + p[1].len();
    (dir, last)
}

#[test]
fn truncate_last_frame_at_every_offset() {
    let (dir, last) = written("truncate");
    let p = payloads();
    let full = fs::read(segment(&dir, 0)).expect("read segment");
    assert_eq!(full.len(), last + 8 + p[2].len());

    for end in last..full.len() {
        fs::write(segment(&dir, 0), &full[..end]).expect("rewrite segment");
        let (mut j, rec) = Journal::open(&dir).expect("open truncated");
        assert_eq!(rec.frames, p[..2], "cut at {end}");
        let want = if end == last {
            None
        } else {
            let stop = if end - last < 8 {
                Stop::ShortHeader
            } else {
                Stop::ShortPayload
            };
            Some(Cut {
                offset: last as u64,
                dropped: (end - last) as u64,
                reason: CutReason::Frame(stop),
            })
        };
        assert_eq!(rec.cut, want, "cut at {end}");
        assert_eq!(
            fs::metadata(segment(&dir, 0)).expect("stat").len(),
            last as u64,
            "file truncated to the last good frame (cut at {end})"
        );
        j.append(b"replacement").expect("append after cut");
        drop(j);
        let (_j, rec) = Journal::open(&dir).expect("reopen after append");
        assert_eq!(rec.frames, [&p[0][..], &p[1][..], b"replacement"]);
        assert_eq!(rec.cut, None);
    }

    for end in 0..HEADER_LEN {
        fs::write(segment(&dir, 0), &full[..end]).expect("rewrite segment");
        let (mut j, rec) = Journal::open(&dir).expect("open torn header");
        assert!(rec.frames.is_empty());
        assert_eq!(
            rec.cut,
            Some(Cut {
                offset: 0,
                dropped: end as u64,
                reason: CutReason::Header,
            })
        );
        j.append(b"fresh").expect("append");
        drop(j);
        let (_j, rec) = Journal::open(&dir).expect("reopen");
        assert_eq!(rec.frames, [b"fresh"]);
    }
}
