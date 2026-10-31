use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pt_journal::{Cut, CutReason, HEADER_LEN, Journal, MAX_FRAME, Stop};

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
fn write_reopen_compare() {
    let (dir, _) = written("roundtrip");
    let p = payloads();
    let (mut j, rec) = Journal::open(&dir).expect("reopen");
    assert_eq!(rec.frames, p);
    assert_eq!(rec.cut, None);
    assert_eq!(j.segment(), 0);

    j.append_batch(&[]).expect("empty batch");
    j.append(b"after reopen").expect("append");
    drop(j);
    let (_j, rec) = Journal::open(&dir).expect("reopen again");
    let mut want = p.clone();
    want.push(b"after reopen".to_vec());
    assert_eq!(rec.frames, want);
    assert_eq!(rec.cut, None);
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

#[test]
fn flipped_bytes_in_last_frame() {
    let (dir, last) = written("flip");
    let p = payloads();
    let full = fs::read(segment(&dir, 0)).expect("read segment");
    for i in last..full.len() {
        for mask in [0x01u8, 0x80, 0xFF] {
            let mut bad = full.clone();
            bad[i] ^= mask;
            fs::write(segment(&dir, 0), &bad).expect("rewrite segment");
            let (_j, rec) = Journal::open(&dir).expect("open damaged");
            assert_eq!(rec.frames, p[..2], "byte {i} mask {mask:#x}");
            let cut = rec.cut.expect("damage reported");
            assert_eq!(cut.offset, last as u64);
            assert_eq!(cut.dropped, (full.len() - last) as u64);
            if i >= last + 8 {
                assert_eq!(cut.reason, CutReason::Frame(Stop::CrcMismatch));
            }
        }
    }
}

#[test]
fn middle_corruption_drops_everything_after_it() {
    let (dir, last) = written("middle");
    let p = payloads();
    let mut full = fs::read(segment(&dir, 0)).expect("read segment");
    full[last - 3] ^= 0x40;
    fs::write(segment(&dir, 0), &full).expect("rewrite segment");
    let (_j, rec) = Journal::open(&dir).expect("open");
    assert_eq!(rec.frames, p[..1]);
    let cut = rec.cut.expect("cut");
    assert_eq!(cut.reason, CutReason::Frame(Stop::CrcMismatch));
    assert_eq!(cut.offset, (HEADER_LEN + 8 + p[0].len()) as u64);
}

#[test]
fn snapshot_then_frames() {
    let dir = fresh_dir("snapshot");
    let (mut j, _) = Journal::open(&dir).expect("open");
    j.append_batch(&[b"a", b"b"]).expect("append");
    j.write_snapshot(b"state after a and b")
        .expect("snapshot 1");
    assert_eq!(j.segment(), 1);
    j.append(b"c").expect("append c");
    drop(j);

    let (mut j, rec) = Journal::open(&dir).expect("reopen");
    assert_eq!(rec.snapshot.as_deref(), Some(&b"state after a and b"[..]));
    assert_eq!(rec.frames, [b"c"]);
    assert_eq!(rec.cut, None);
    assert_eq!(j.segment(), 1);

    j.write_snapshot(b"").expect("empty snapshot 2");
    j.append(b"d").expect("append d");
    drop(j);
    let (_j, rec) = Journal::open(&dir).expect("reopen");
    assert_eq!(rec.snapshot.as_deref(), Some(&b""[..]));
    assert_eq!(rec.frames, [b"d"]);
    for seq in [0, 1, 2] {
        assert!(segment(&dir, seq).exists(), "segment {seq} kept");
    }

    // A crash between the snapshot rename and the new segment's creation.
    let crashed = fresh_dir("snapshot-crash");
    for name in [
        "segment-00000000000000000000",
        "snapshot-00000000000000000001",
    ] {
        fs::copy(dir.join(name), crashed.join(name)).expect("copy");
    }
    let (mut j, rec) = Journal::open(&crashed).expect("open crashed");
    assert_eq!(rec.snapshot.as_deref(), Some(&b"state after a and b"[..]));
    assert!(rec.frames.is_empty());
    assert_eq!(j.segment(), 1);
    j.append(b"e").expect("append");
    drop(j);
    let (j, rec) = Journal::open(&crashed).expect("reopen crashed");
    assert_eq!(rec.frames, [b"e"]);
    drop(j);

    // A damaged snapshot fails loudly.
    let snap = crashed.join("snapshot-00000000000000000001");
    let mut bytes = fs::read(&snap).expect("read snapshot");
    let n = bytes.len();
    bytes[n - 1] ^= 1;
    fs::write(&snap, &bytes).expect("rewrite snapshot");
    let err = Journal::open(&crashed)
        .err()
        .expect("damaged snapshot refused");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn refusals() {
    let dir = fresh_dir("refusals");
    let (mut j, _) = Journal::open(&dir).expect("open");
    let err = Journal::open(&dir).err().expect("second writer refused");
    assert_eq!(err.kind(), ErrorKind::WouldBlock);

    assert_eq!(
        j.append(b"").expect_err("empty").kind(),
        ErrorKind::InvalidInput
    );
    let big = vec![7u8; MAX_FRAME + 1];
    assert_eq!(
        j.append_batch(&[b"ok", &big]).expect_err("oversize").kind(),
        ErrorKind::InvalidInput
    );
    j.append(&big[..MAX_FRAME]).expect("max frame");
    drop(j);
    let (_j, rec) = Journal::open(&dir).expect("reopen");
    assert_eq!(rec.frames.len(), 1);
    assert_eq!(rec.frames[0].len(), MAX_FRAME);

    let foreign = fresh_dir("foreign");
    fs::write(segment(&foreign, 0), b"NOTOURS\0\x01\0\0\0").expect("write");
    let err = Journal::open(&foreign).err().expect("foreign file refused");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}
