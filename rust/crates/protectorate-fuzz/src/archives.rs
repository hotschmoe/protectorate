use crate::{fields, text};
use protectorate_backup::{restore, verify, Cancellation};
use protectorate_release::{
    extract_web, parse_checksums, resolve_url, resolved_version, safe_web_path, valid_version,
    validate_api, validate_channel, validate_repo, validate_url, verify_checksum, Info,
};
use protectorate_xfer::{validate_entry_name, Chunk, Kind, Sink, Trailer, CHUNK_SIZE};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Makes every directory below root traversable, then asserts the tree holds
/// only regular files and directories that resolve inside root.
fn check_tree(root: &Path) {
    let real_root = fs::canonicalize(root).unwrap();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(
                meta.is_dir() || meta.is_file(),
                "{} is neither a file nor a directory",
                path.display()
            );
            assert!(
                fs::canonicalize(&path).unwrap().starts_with(&real_root),
                "{} escaped {}",
                path.display(),
                root.display()
            );
            if meta.is_dir() {
                stack.push(path);
            }
        }
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// A directory transfer archive (the PTXDIR1 format) sent as verified chunks:
/// whatever it contains, nothing lands outside the destination and a failed
/// extraction leaves nothing behind.
pub fn xfer_archive(data: &[u8]) {
    let scratch = tempfile::tempdir().unwrap();
    let parent = scratch.path().join("parent");
    let dest = parent.join("dest");
    let mut sink = Sink::new_unknown_size(&dest, Kind::Directory, 0o755, false).unwrap();
    for piece in data.chunks(CHUNK_SIZE) {
        sink.write(Chunk {
            data: piece.to_vec(),
            compressed: false,
            raw_size: piece.len() as u32,
        })
        .unwrap();
    }
    let trailer = Trailer {
        size: data.len() as u64,
        sha256: hex(&Sha256::digest(data)),
        skipped: 0,
    };
    match sink.prepare_directory(trailer) {
        Ok(prepared) => {
            check_tree(prepared.path());
            prepared.publish().expect("publish a verified directory");
            check_tree(&dest);
            assert_eq!(names(&parent), ["dest"], "extraction left residue");
        }
        Err(_) => {
            check_tree(&parent);
            assert!(names(&parent).is_empty(), "failed extraction left residue");
        }
    }
    assert_eq!(names(scratch.path()), ["parent"]);
    for part in String::from_utf8_lossy(data).split(['\0', '\n']) {
        if let Ok(path) = validate_entry_name(part) {
            assert!(
                path.components().all(|c| matches!(c, Component::Normal(_))),
                "entry name {part:?} is not a plain relative path"
            );
        }
    }
}

/// An uncompressed tar stream, gzipped and unpacked as the release web
/// bundle: nothing escapes the target and success implies an index.html.
pub fn release_web_archive(data: &[u8]) {
    let scratch = tempfile::tempdir().unwrap();
    let archive = scratch.path().join("web.tar.gz");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(data).unwrap();
    fs::write(&archive, gz.finish().unwrap()).unwrap();
    let out = scratch.path().join("web");
    let result = extract_web(&archive, &out);
    if out.exists() {
        check_tree(&out);
    }
    if result.is_ok() {
        assert!(out.join("index.html").is_file());
    }
    assert!(names(scratch.path())
        .iter()
        .all(|n| n == "web" || n == "web.tar.gz"));
}

/// NUL-separated: release JSON, SHA256SUMS, VERSION, channel, base URL,
/// redirect location, archive entry name.
pub fn release_manifest(data: &[u8]) {
    let parts = fields(data, 7);
    let (sums, version) = (parts[1], parts[2]);
    let channel = text(parts[3]);
    let _ = validate_channel(&channel);
    let _ = validate_repo(&channel);
    let _ = validate_api(&text(parts[4]));
    if let Ok(map) = parse_checksums(sums) {
        let actual = hex(&Sha256::digest(version));
        for (name, want) in &map {
            let verified = verify_checksum(name, version, sums).is_ok();
            assert_eq!(
                verified,
                want.eq_ignore_ascii_case(&actual),
                "checksum verdict for {name} disagrees with SHA-256"
            );
        }
    }
    if let Ok(info) = serde_json::from_slice::<Info>(parts[0]) {
        let _ = info.matches_channel(&channel, "");
        for asset in &info.assets {
            if let Ok(url) = info.asset_url(&asset.name) {
                let same: Vec<_> = info
                    .assets
                    .iter()
                    .filter(|a| a.name == asset.name)
                    .collect();
                assert_eq!(same.len(), 1, "duplicate asset {:?} resolved", asset.name);
                assert_eq!(url, asset.url);
            }
        }
        let version_asset = (!version.is_empty()).then_some(version);
        if let Ok(v) = resolved_version(&info, Some(sums), version_asset) {
            assert!(valid_version(&v), "resolved invalid version {v:?}");
        }
    }
    let (base, location) = (text(parts[4]), text(parts[5]));
    // resolve_url drops the fragment and trims before deciding what is relative.
    let relative = location.split('#').next().unwrap_or("").trim();
    if validate_url(&base, true).is_ok() && !relative.contains("://") && !relative.starts_with("//")
    {
        if let Ok(next) = resolve_url(&base, &location) {
            let (scheme, rest) = base.split_once("://").unwrap();
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let origin = format!("{scheme}://{authority}");
            assert!(
                next.strip_prefix(&origin)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(['/', '?'])),
                "relative redirect {location:?} from {base:?} left the origin: {next:?}"
            );
            assert!(validate_url(&next, true).is_ok());
        }
    }
    let name = text(parts[6]);
    if let Ok(path) = safe_web_path(&name) {
        assert!(
            path == Path::new(".") || path.components().all(|c| matches!(c, Component::Normal(_))),
            "web path {name:?} became {}",
            path.display()
        );
    }
}

/// A backup archive file, verified and restored: both agree on the manifest,
/// a restore holds exactly the manifest files, and a refusal leaves nothing
/// beside the destination.
pub fn backup_archive(data: &[u8]) {
    let scratch = tempfile::tempdir().unwrap();
    let parent = scratch.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let archive = parent.join("fuzz.pbackup");
    fs::write(&archive, data).unwrap();
    let cancel = Cancellation::default();
    let verified = verify(&archive, &cancel);
    let home = parent.join("home");
    match restore(&archive, &home, &cancel) {
        Ok(manifest) => {
            assert_eq!(
                verified.expect("restore succeeded where verify failed"),
                manifest
            );
            check_tree(&home);
            for file in &manifest.files {
                let bytes = fs::read(home.join(&file.path)).expect("manifest file restored");
                assert_eq!(bytes.len() as u64, file.size);
                assert_eq!(hex(&Sha256::digest(&bytes)), file.sha256);
            }
            assert_eq!(names(&parent), ["fuzz.pbackup", "home"]);
        }
        Err(_) => {
            assert!(!home.exists(), "failed restore published a destination");
            assert_eq!(
                names(&parent),
                ["fuzz.pbackup"],
                "failed restore left residue"
            );
        }
    }
}
