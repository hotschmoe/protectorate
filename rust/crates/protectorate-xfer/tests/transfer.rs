use protectorate_xfer::{Chunk, Header, Kind, Sink, Source, Trailer, CHUNK_SIZE};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn transfer(from: &Path, to: &Path, overwrite: bool) -> Trailer {
    let mut source = Source::open(from, true).unwrap();
    let mut sink = Sink::new(to, source.header(), overwrite).unwrap();
    while let Some(chunk) = source.next_chunk().unwrap() {
        sink.write(chunk).unwrap();
    }
    let trailer = source.trailer().unwrap();
    sink.commit(trailer.clone()).unwrap();
    trailer
}

#[test]
fn file_round_trip_is_bounded_and_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("from");
    let to = tmp.path().join("to");
    let content = vec![b'a'; CHUNK_SIZE + 107];
    fs::write(&from, &content).unwrap();
    fs::write(&to, b"old").unwrap();
    let trailer = transfer(&from, &to, true);
    assert_eq!(trailer.size, content.len() as u64);
    assert_eq!(fs::read(&to).unwrap(), content);

    let mut source = Source::open(&from, true).unwrap();
    let mut sink = Sink::new(&to, source.header(), true).unwrap();
    let chunk = source.next_chunk().unwrap().unwrap();
    assert!(chunk.compressed);
    assert_eq!(chunk.raw_size as usize, CHUNK_SIZE);
    sink.write(chunk).unwrap();
    while let Some(chunk) = source.next_chunk().unwrap() {
        sink.write(chunk).unwrap();
    }
    let mut wrong = source.trailer().unwrap();
    wrong.sha256 = "00".repeat(32);
    assert!(sink.commit(wrong).is_err());
    assert_eq!(fs::read(&to).unwrap(), content);
}

#[test]
fn directory_round_trip_skips_symlinks_and_preserves_modes() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("from");
    let to = tmp.path().join("to");
    fs::create_dir(&from).unwrap();
    fs::create_dir(from.join("nested")).unwrap();
    fs::write(from.join("nested/data"), b"payload").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        symlink("nested/data", from.join("link")).unwrap();
        fs::set_permissions(from.join("nested/data"), fs::Permissions::from_mode(0o400)).unwrap();
        fs::set_permissions(from.join("nested"), fs::Permissions::from_mode(0o500)).unwrap();
        fs::set_permissions(&from, fs::Permissions::from_mode(0o750)).unwrap();
    }
    let trailer = transfer(&from, &to, false);
    assert_eq!(fs::read(to.join("nested/data")).unwrap(), b"payload");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(trailer.skipped, 1);
        assert!(!to.join("link").exists());
        assert_eq!(
            fs::metadata(&to).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(to.join("nested"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        assert_eq!(
            fs::metadata(to.join("nested/data"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o400
        );
    }
}

#[test]
fn changed_source_and_bad_chunks_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("source");
    fs::write(&from, b"original").unwrap();
    let mut source = Source::open(&from, false).unwrap();
    assert!(source.next_chunk().unwrap().is_some());
    assert!(source.next_chunk().unwrap().is_none());
    fs::write(&from, b"changed to a different length").unwrap();
    assert!(source.trailer().is_err());

    let mut sink = Sink::new(
        tmp.path().join("dest"),
        Header {
            kind: Kind::File,
            mode: 0o600,
            size: 1,
        },
        false,
    )
    .unwrap();
    assert!(sink
        .write(Chunk {
            data: vec![0; CHUNK_SIZE + 1],
            compressed: false,
            raw_size: 1
        })
        .is_err());
    assert!(sink
        .write(Chunk {
            data: vec![1],
            compressed: false,
            raw_size: (CHUNK_SIZE + 1) as u32
        })
        .is_err());
    assert!(sink
        .write(Chunk {
            data: vec![1],
            compressed: true,
            raw_size: 1
        })
        .is_err());
}

#[test]
fn unknown_size_sink_requires_matching_trailer() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("dest");
    let mut sink = Sink::new_unknown_size(&dest, Kind::File, 0o600, false).unwrap();
    sink.write(Chunk {
        data: b"hello".to_vec(),
        compressed: false,
        raw_size: 5,
    })
    .unwrap();
    let digest = Sha256::digest(b"hello");
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    sink.commit(Trailer {
        size: 5,
        sha256,
        skipped: 0,
    })
    .unwrap();
    assert_eq!(fs::read(dest).unwrap(), b"hello");
}

fn archive(records: &[(u8, &str, u32, &[u8])]) -> Vec<u8> {
    let mut out = b"PTXDIR1\n".to_vec();
    for (kind, name, mode, data) in records {
        out.push(*kind);
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&mode.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out.push(0);
    out
}

fn receive_archive(bytes: &[u8], dest: &Path) -> protectorate_xfer::Result<()> {
    let mut sink = Sink::new(
        dest,
        Header {
            kind: Kind::Directory,
            mode: 0o700,
            size: 0,
        },
        false,
    )?;
    for piece in bytes.chunks(CHUNK_SIZE) {
        sink.write(Chunk {
            data: piece.to_vec(),
            compressed: false,
            raw_size: piece.len() as u32,
        })?;
    }
    let digest = Sha256::digest(bytes);
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    sink.commit(Trailer {
        size: bytes.len() as u64,
        sha256,
        skipped: 0,
    })
}

#[test]
fn archive_rejects_escape_devices_symlinks_and_out_of_order_entries() {
    let tmp = tempfile::tempdir().unwrap();
    for (i, name) in [
        "../escape",
        "/absolute",
        "C:/drive",
        "a\\b",
        "NUL",
        "COM1.txt",
        "a/../escape",
    ]
    .iter()
    .enumerate()
    {
        let dest = tmp.path().join(format!("dest{i}"));
        let bytes = archive(&[(2, name, 0o600, b"bad")]);
        assert!(receive_archive(&bytes, &dest).is_err(), "accepted {name}");
        assert!(!dest.exists());
    }
    assert!(!tmp.path().join("escape").exists());
    assert!(receive_archive(
        &archive(&[(3, "link", 0o777, b"")]),
        &tmp.path().join("symlink")
    )
    .is_err());
    assert!(receive_archive(
        &archive(&[(2, "missing/file", 0o600, b"x")]),
        &tmp.path().join("unordered")
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn source_root_symlink_is_followed_but_nested_link_is_skipped() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("actual"), b"yes").unwrap();
    symlink("actual", tmp.path().join("root-link")).unwrap();
    let trailer = transfer(
        &tmp.path().join("root-link"),
        &tmp.path().join("copied"),
        false,
    );
    assert_eq!(trailer.size, 3);
    assert_eq!(fs::read(tmp.path().join("copied")).unwrap(), b"yes");
}

#[cfg(unix)]
#[test]
fn non_utf8_destination_path_is_supported() {
    use std::os::unix::ffi::OsStringExt;
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::write(&source, b"value").unwrap();
    let dest = tmp
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'd', 0xff]));
    transfer(&source, &dest, false);
    assert_eq!(fs::read(dest).unwrap(), b"value");
}
