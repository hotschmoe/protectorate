mod common;

use common::{release_json, sums_for, tar_header, Release, Server};
use protectorate_release::{valid_version, Error, Info, Source, BINARIES, MAX_RELEASE_JSON};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const VERSION: &str = "1.2.3";
const TAG: &str = "v1.2.3";

fn web_name() -> String {
    format!("protectorate-web-{VERSION}.tar.gz")
}

fn web_archive() -> Vec<u8> {
    common::tar_gz(&[
        ("./", b'5', b""),
        ("./index.html", b'0', b"<html>index</html>"),
        ("./main.dart.js", b'0', b"// dart output"),
        ("./assets", b'5', b""),
        ("./assets/app.css", b'0', b"body{}"),
    ])
}

/// Every asset a complete release carries, without SHA256SUMS.
fn payload() -> BTreeMap<String, Vec<u8>> {
    let mut map: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for name in BINARIES {
        map.insert(name.to_string(), format!("binary {name}").into_bytes());
    }
    map.insert("VERSION".to_string(), b"1.2.3\n".to_vec());
    map.insert(web_name(), web_archive());
    map
}

fn sums_for_payload(map: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let entries: Vec<(&str, &[u8])> = map
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_slice()))
        .collect();
    sums_for(&entries).into_bytes()
}

/// (served bodies including SHA256SUMS, the sums the client must match)
fn bodies_with(mut map: BTreeMap<String, Vec<u8>>) -> (BTreeMap<String, Vec<u8>>, Vec<u8>) {
    let sums = sums_for_payload(&map);
    map.insert("SHA256SUMS".to_string(), sums.clone());
    (map, sums)
}

fn standard() -> (BTreeMap<String, Vec<u8>>, Vec<u8>) {
    bodies_with(payload())
}

/// The standard release with a replaced web archive.
fn with_web(archive: Vec<u8>) -> (BTreeMap<String, Vec<u8>>, Vec<u8>) {
    let mut map = payload();
    map.insert(web_name(), archive);
    bodies_with(map)
}

fn release_with(map: &BTreeMap<String, Vec<u8>>) -> Release {
    let mut release = Release::new(TAG);
    for (name, body) in map {
        release.asset(name, body, &format!("/assets/{name}"));
    }
    release
}

struct Fixture {
    dir: TempDir,
    server: Server,
    source: Source,
    sums: Vec<u8>,
    bodies: BTreeMap<String, Vec<u8>>,
}

fn make_fixture(release: Release, bodies: BTreeMap<String, Vec<u8>>, sums: Vec<u8>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let server = release.serve();
    let source = Source::new(
        "owner/repo",
        "stable",
        &server.base,
        dir.path().to_str().unwrap(),
    )
    .unwrap();
    Fixture {
        dir,
        server,
        source,
        sums,
        bodies,
    }
}

fn standard_fixture() -> Fixture {
    let (bodies, sums) = standard();
    make_fixture(release_with(&bodies), bodies, sums)
}

#[test]
fn fetch_rust_binary_requires_a_verified_release_asset() {
    let mut map = payload();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-amd64"),
            component.as_bytes().to_vec(),
        );
    }
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    assert!(fixture
        .source
        .fetch_rust_binary(&info, "linux", "amd64")
        .is_err());
    fixture.source.fetch(&info, noop).unwrap();
    let path = fixture
        .source
        .fetch_rust_binary(&info, "linux", "amd64")
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"protectorate-rs");
    for component in protectorate_release::RUST_BUNDLE {
        let path = fixture
            .source
            .fetch_rust_component(&info, component, "linux", "amd64")
            .unwrap();
        assert_eq!(fs::read(path).unwrap(), component.as_bytes());
    }
    assert_eq!(
        fixture
            .source
            .fetch_rust_binary(&info, "linux", "amd64")
            .unwrap(),
        path
    );
    fs::write(&path, b"tampered").unwrap();
    assert!(fixture
        .source
        .fetch_rust_binary(&info, "linux", "amd64")
        .is_err());
    assert!(fixture
        .source
        .fetch_rust_binary(&info, "android", "arm64")
        .is_err());
}

#[test]
fn fetch_termux_binary_from_verified_release() {
    let mut map = payload();
    map.insert(
        "protectorate-rs-android-arm64".into(),
        b"termux node".to_vec(),
    );
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture.source.fetch(&info, noop).unwrap();
    let path = fixture
        .source
        .fetch_rust_binary(&info, "android", "arm64")
        .unwrap();
    assert_eq!(fs::read(path).unwrap(), b"termux node");
}

#[test]
fn rust_release_fetches_bundle_and_web_without_go_assets() {
    let mut map = BTreeMap::new();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    map.insert("VERSION".into(), b"1.2.3\n".to_vec());
    map.insert(web_name(), web_archive());
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap();
    let server = fixture
        .source
        .verify_rust_bundle(&info.version, "linux", "arm64")
        .unwrap();
    assert_eq!(fs::read(server).unwrap(), b"protectorate-rs");
    assert!(fixture
        .source
        .release_dir(&info.version)
        .join("web/index.html")
        .is_file());
}

#[test]
fn rust_bundle_completes_a_verified_release_with_one_cached_binary() {
    let mut map = payload();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture.source.fetch(&info, noop).unwrap();
    let cached = fixture
        .source
        .fetch_rust_binary(&info, "linux", "arm64")
        .unwrap();
    let cached_bytes = fs::read(&cached).unwrap();

    let bundle = fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap();
    assert_eq!(fs::read(&cached).unwrap(), cached_bytes);
    assert!(bundle.join("protectorate-node-sandbox-helper").is_file());
    assert!(bundle.join("web/index.html").is_file());
    fixture
        .source
        .verify_rust_bundle(&info.version, "linux", "arm64")
        .unwrap();
}

#[test]
fn rust_bundle_refuses_to_extend_a_tampered_partial_release() {
    let mut map = payload();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture.source.fetch(&info, noop).unwrap();
    let cached = fixture
        .source
        .fetch_rust_binary(&info, "linux", "arm64")
        .unwrap();
    fs::write(&cached, b"tampered").unwrap();

    assert!(fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .is_err());
    assert_eq!(fs::read(&cached).unwrap(), b"tampered");
    assert!(!fixture
        .source
        .release_dir(&info.version)
        .join("protectorate-node-sandbox-helper-linux-arm64")
        .exists());
}

#[test]
fn rust_bundle_refuses_to_extend_a_release_with_tampered_web_assets() {
    let mut map = payload();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture.source.fetch(&info, noop).unwrap();
    fixture
        .source
        .fetch_rust_binary(&info, "linux", "arm64")
        .unwrap();
    fs::write(
        fixture
            .source
            .release_dir(&info.version)
            .join("web/index.html"),
        b"tampered",
    )
    .unwrap();

    assert!(fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .is_err());
    assert!(!fixture
        .source
        .release_dir(&info.version)
        .join("protectorate-node-sandbox-helper-linux-arm64")
        .exists());
}

#[test]
fn rust_bundle_refuses_unlisted_cached_web_assets() {
    let mut map = payload();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    fixture.source.fetch(&info, noop).unwrap();
    fixture
        .source
        .fetch_rust_binary(&info, "linux", "arm64")
        .unwrap();
    fs::write(
        fixture
            .source
            .release_dir(&info.version)
            .join("web/unlisted.js"),
        b"unverified script",
    )
    .unwrap();

    assert!(fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .is_err());
    assert!(!fixture
        .source
        .release_dir(&info.version)
        .join("protectorate-node-sandbox-helper-linux-arm64")
        .exists());
}

#[test]
fn shared_release_cache_completes_node_and_hub_architectures_in_either_order() {
    let mut map = payload();
    for arch in ["amd64", "arm64"] {
        for component in protectorate_release::RUST_BUNDLE {
            map.insert(
                format!("{component}-linux-{arch}"),
                format!("{component} {arch}").into_bytes(),
            );
        }
    }
    for node_first in [false, true] {
        let (bodies, sums) = bodies_with(map.clone());
        let fixture = make_fixture(release_with(&bodies), bodies, sums);
        let info = fixture.source.check("stable", "").unwrap();
        if node_first {
            let dir = fixture
                .source
                .fetch_rust_node_bundle(&info, "linux", "arm64", noop)
                .unwrap();
            assert!(!dir.join("protectorate-rs").exists());
        } else {
            fixture
                .source
                .fetch_rust_release(&info, "linux", "amd64", noop)
                .unwrap();
        }
        let dir = fixture
            .source
            .fetch_rust_node_bundle(&info, "linux", "arm64", noop)
            .unwrap();
        assert_eq!(
            fs::read(dir.join("protectorate-rs-linux-arm64")).unwrap(),
            b"protectorate-rs arm64"
        );
        protectorate_release::verify_rust_assets_dir(&dir, "linux", "arm64").unwrap();
        fixture
            .source
            .fetch_rust_release(&info, "linux", "amd64", noop)
            .unwrap();
        assert_eq!(
            fs::read(dir.join("protectorate-rs")).unwrap(),
            b"protectorate-rs amd64"
        );
        fixture
            .source
            .verify_rust_bundle(&info.version, "linux", "amd64")
            .unwrap();
    }
}

#[cfg(unix)]
#[test]
fn rust_release_keeps_the_names_an_installed_unit_executes() {
    use std::os::unix::fs::MetadataExt;
    let mut map = BTreeMap::new();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    map.insert("VERSION".into(), b"1.2.3\n".to_vec());
    map.insert(web_name(), web_archive());
    let (bodies, _sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, Vec::new());
    let info = fixture.source.check("stable", "").unwrap();
    let published = fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap();
    for component in protectorate_release::RUST_BUNDLE {
        let stable = published.join(component);
        let versioned = published.join(format!("{component}-linux-arm64"));
        let stable_meta = fs::metadata(&stable).unwrap();
        let versioned_meta = fs::metadata(&versioned).unwrap();
        assert_eq!(stable_meta.ino(), versioned_meta.ino());
        assert_eq!(fs::read(&stable).unwrap(), component.as_bytes());
    }
    fixture.source.activate(&info.version).unwrap();
    let unit_exe = fixture.source.dir().join("current/protectorate-rs");
    assert!(unit_exe.is_file(), "missing {}", unit_exe.display());
    assert_eq!(fs::read(unit_exe).unwrap(), b"protectorate-rs");
    fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap();
}

#[cfg(unix)]
#[test]
fn cached_rust_release_rejects_non_executable_component() {
    use std::os::unix::fs::PermissionsExt;
    let mut map = BTreeMap::new();
    for component in protectorate_release::RUST_BUNDLE {
        map.insert(
            format!("{component}-linux-arm64"),
            component.as_bytes().to_vec(),
        );
    }
    map.insert("VERSION".into(), b"1.2.3\n".to_vec());
    map.insert(web_name(), web_archive());
    let (bodies, _sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, Vec::new());
    let info = fixture.source.check("stable", "").unwrap();
    let published = fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap();
    let binary = published.join("protectorate-rs-linux-arm64");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o644)).unwrap();
    let error = fixture
        .source
        .fetch_rust_release(&info, "linux", "arm64", noop)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not executable"), "{error}");
    assert!(fixture
        .source
        .verify_rust_bundle(&info.version, "linux", "arm64")
        .is_err());
}

fn noop(_: &str, _: u64, _: u64) -> Result<(), Error> {
    Ok(())
}

#[test]
fn check_fetches_release_and_reuses_the_etag() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    assert_eq!(info.tag, TAG);
    assert_eq!(info.version, VERSION);
    assert_eq!(info.channel, "stable");
    assert_eq!(
        info.notes_url,
        format!("{}/releases/tag/{TAG}", fixture.server.base)
    );

    let again = fixture.source.check("stable", "").unwrap();
    assert_eq!(again, info);
    let headers = fixture
        .server
        .headers_for("/repos/owner/repo/releases/latest");
    assert_eq!(headers.len(), 2);
    assert!(!headers[0].iter().any(|(k, _)| k == "if-none-match"));
    assert!(headers[1]
        .iter()
        .any(|(k, v)| k == "if-none-match" && v == "W/\"etag1\""));
    assert_eq!(fixture.source.available("stable").version, VERSION);
    assert!(fixture.server.saw("/assets/SHA256SUMS"));
    assert!(fixture.server.saw("/assets/VERSION"));
}

#[test]
fn check_selects_a_version_tag_path() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", VERSION).unwrap();
    assert_eq!(info.version, VERSION);
    assert!(fixture.server.saw("/repos/owner/repo/releases/tags/1.2.3"));
    assert!(!fixture.server.saw("/repos/owner/repo/releases/latest"));
    assert!(fixture.source.check("stable", "not a version").is_err());
    assert!(fixture.source.check("nightly", "").is_err());
}

#[test]
fn check_rejects_releases_that_do_not_match_the_channel() {
    let (bodies, sums) = standard();
    let mut prerelease = release_with(&bodies);
    prerelease.prerelease = true;
    let fixture = make_fixture(prerelease, bodies.clone(), sums.clone());
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(err.contains("does not match channel stable"), "{err}");

    let mut draft = release_with(&bodies);
    draft.draft = true;
    let fixture = make_fixture(draft, bodies, sums.clone());
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(err.contains("does not match channel stable"), "{err}");
}

#[test]
fn check_requires_a_version_asset_for_dev_releases() {
    let map = {
        let mut map = payload();
        map.remove("VERSION");
        map
    };
    let sums = sums_for_payload(&map);
    let mut bodies = map;
    bodies.insert("SHA256SUMS".to_string(), sums.clone());
    let mut dev = release_with(&bodies);
    dev.tag = "dev".to_string();
    dev.prerelease = true;
    dev.release_paths = vec!["/repos/owner/repo/releases/tags/dev".to_string()];
    let dir = tempfile::tempdir().unwrap();
    let server = dev.serve();
    let source = Source::new(
        "owner/repo",
        "dev",
        &server.base,
        dir.path().to_str().unwrap(),
    )
    .unwrap();
    let err = source.check("dev", "").unwrap_err().to_string();
    assert!(err.contains("release dev has no VERSION asset"), "{err}");
}

#[test]
fn check_bounds_the_release_document() {
    let (bodies, sums) = standard();
    let mut padded = release_with(&bodies);
    padded.pad = MAX_RELEASE_JSON as usize + 1;
    let fixture = make_fixture(padded, bodies, sums);
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(
        err.contains(&format!("asset exceeds {MAX_RELEASE_JSON} bytes")),
        "{err}"
    );
}

#[test]
fn check_reports_http_errors() {
    let (bodies, sums) = standard();
    let mut missing = release_with(&bodies);
    missing.release_paths = vec!["/repos/owner/repo/releases/other".to_string()];
    let fixture = make_fixture(missing, bodies, sums);
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(err.contains("HTTP 404"), "{err}");
}

#[test]
fn check_rejects_a_broken_checksums_asset() {
    let (mut bodies, sums) = standard();
    bodies.insert("SHA256SUMS".to_string(), b"deadbeef  VERSION\n".to_vec());
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(
        err.contains("missing or invalid checksum for VERSION"),
        "{err}"
    );
}

#[test]
fn check_falls_back_to_the_immutable_tag_when_there_is_no_version_asset() {
    let mut map: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for name in BINARIES {
        map.insert(name.to_string(), format!("binary {name}").into_bytes());
    }
    map.insert(format!("protectorate-web-{TAG}.tar.gz"), web_archive());
    let (bodies, sums) = bodies_with(map);
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    assert_eq!(info.version, TAG);
    let dest = fixture.source.fetch(&info, noop).unwrap();
    assert_eq!(dest, fixture.source.release_dir(TAG));
    assert!(!dest.join("VERSION").exists());
    assert!(dest.join("web/index.html").is_file());
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn check_and_fetch_a_dev_release_by_its_version_asset() {
    let (bodies, _sums) = standard();
    let mut dev = release_with(&bodies);
    dev.tag = "dev".to_string();
    dev.prerelease = true;
    dev.release_paths = vec!["/repos/owner/repo/releases/tags/dev".to_string()];
    let dir = tempfile::tempdir().unwrap();
    let server = dev.serve();
    let source = Source::new(
        "owner/repo",
        "dev",
        &server.base,
        dir.path().to_str().unwrap(),
    )
    .unwrap();
    let info = source.check("dev", "").unwrap();
    assert_eq!(info.tag, "dev");
    assert_eq!(info.version, VERSION);
    assert_eq!(info.channel, "dev");
    let dest = source.fetch(&info, noop).unwrap();
    assert_eq!(dest, source.release_dir(VERSION));
    assert!(dest.join("web/index.html").is_file());
    assert!(source.cached("dev").is_some());
}

#[test]
fn fetch_downloads_verifies_and_publishes_once() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    let mut events: Vec<(String, u64, u64)> = Vec::new();
    let dest = fixture
        .source
        .fetch(&info, |name, done, total| {
            events.push((name.to_string(), done, total));
            Ok(())
        })
        .unwrap();

    assert_eq!(dest, fixture.source.release_dir(VERSION));
    let mut expected: Vec<String> = BINARIES.iter().map(|name| name.to_string()).collect();
    expected.push(web_name());
    expected.push("VERSION".to_string());
    assert_eq!(
        events,
        expected
            .iter()
            .flat_map(|name| {
                let len = fixture.bodies[name].len() as u64;
                vec![(name.clone(), 0, 0), (name.clone(), len, len)]
            })
            .collect::<Vec<_>>()
    );

    for name in &expected {
        assert_eq!(fs::read(dest.join(name)).unwrap(), fixture.bodies[name]);
    }
    assert_eq!(fs::read(dest.join("SHA256SUMS")).unwrap(), fixture.sums);
    let stored: Info =
        serde_json::from_slice(&fs::read(dest.join("release.json")).unwrap()).unwrap();
    assert_eq!(stored, info);
    assert_eq!(
        fs::read_to_string(dest.join("web/index.html")).unwrap(),
        "<html>index</html>"
    );
    assert_eq!(
        fs::read_to_string(dest.join("web/assets/app.css")).unwrap(),
        "body{}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dest.join(BINARIES[0]))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(dest.join("VERSION"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_follows_relative_redirects() {
    let (bodies, sums) = standard();
    let mut redirect = release_with(&bodies);
    redirect.urls.insert(
        "VERSION".to_string(),
        "/redirect/assets/VERSION".to_string(),
    );
    let fixture = make_fixture(redirect, bodies.clone(), sums);
    let info = fixture.source.check("stable", "").unwrap();
    let dest = fixture.source.fetch(&info, noop).unwrap();
    assert!(fixture.server.saw("/redirect/assets/VERSION"));
    assert!(fixture.server.saw("/assets/VERSION"));
    assert_eq!(fs::read(dest.join("VERSION")).unwrap(), bodies["VERSION"]);
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_rejects_a_redirect_loop() {
    let (bodies, sums) = standard();
    let mut looped = release_with(&bodies);
    looped
        .urls
        .insert("VERSION".to_string(), "/loop".to_string());
    let fixture = make_fixture(looped, bodies, sums);
    let err = fixture.source.check("stable", "").unwrap_err().to_string();
    assert!(err.contains("exceeded ten redirects"), "{err}");
}

#[test]
fn fetch_rejects_a_checksum_mismatch() {
    let (mut bodies, sums) = standard();
    bodies.insert("protectorate-linux-amd64".to_string(), b"tampered".to_vec());
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
    assert!(
        err.contains("checksum mismatch for protectorate-linux-amd64"),
        "{err}"
    );
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_rejects_a_missing_asset() {
    let (mut bodies, sums) = standard();
    bodies.remove("protectorate-android-arm64");
    let mut release = release_with(&bodies);
    release
        .names
        .retain(|name| name != "protectorate-android-arm64");
    let fixture = make_fixture(release, bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
    assert!(err.contains("has no protectorate-android-arm64"), "{err}");
}

#[test]
fn fetch_rejects_an_unsafe_web_archive() {
    let cases = [
        (
            "escape",
            common::tar_gz(&[
                ("index.html", b'0', b"<html>index</html>"),
                ("../evil", b'0', b"escape"),
            ]),
        ),
        (
            "absolute",
            common::tar_gz(&[
                ("index.html", b'0', b"<html>index</html>"),
                ("/etc/passwd", b'0', b"absolute"),
            ]),
        ),
        (
            "backslash",
            common::tar_gz(&[
                ("index.html", b'0', b"<html>index</html>"),
                ("web\\evil", b'0', b"backslash"),
            ]),
        ),
        (
            "symlink",
            common::tar_gz(&[
                ("index.html", b'0', b"<html>index</html>"),
                ("link", b'2', b""),
            ]),
        ),
        (
            "no index",
            common::tar_gz(&[("main.dart.js", b'0', b"// dart output")]),
        ),
        (
            "not a tar",
            common::gzip(b"this is not a tar archive at all"),
        ),
    ];
    for (label, archive) in cases {
        let (bodies, sums) = with_web(archive);
        let fixture = make_fixture(release_with(&bodies), bodies, sums);
        let info = fixture.source.check("stable", "").unwrap();
        let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
        match label {
            "escape" | "absolute" | "backslash" => {
                assert!(err.contains("unsafe web path"), "{label}: {err}")
            }
            "symlink" => assert!(err.contains("unsupported entry"), "{label}: {err}"),
            "no index" => assert!(err.contains("no index.html"), "{label}: {err}"),
            _ => assert!(err.contains("web"), "{label}: {err}"),
        }
        assert_no_staging_left(fixture.dir.path());
    }
}

#[test]
fn fetch_accepts_a_gnu_long_name() {
    let long = format!("assets/{}app.css", "deep-".repeat(20));
    let (bodies, sums) = with_web(gnu_long_name(&long, b"body{}"));
    let fixture = make_fixture(release_with(&bodies), bodies, sums);
    let info = fixture.source.check("stable", "").unwrap();
    let dest = fixture.source.fetch(&info, noop).unwrap();
    assert_eq!(
        fs::read_to_string(dest.join("web").join(&long)).unwrap(),
        "body{}"
    );
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_repeats_by_verifying_the_cached_release() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    let first = fixture.source.fetch(&info, noop).unwrap();
    let before = fs::metadata(first.join(BINARIES[0])).unwrap();
    let second = fixture.source.fetch(&info, noop).unwrap();
    assert_eq!(first, second);
    let after = fs::metadata(first.join(BINARIES[0])).unwrap();
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_refuses_to_overwrite_a_tampered_release() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    let dest = fixture.source.fetch(&info, noop).unwrap();
    fs::write(dest.join(BINARIES[0]), b"swapped out").unwrap();
    let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
    assert!(
        err.contains("checksum mismatch for protectorate-linux-amd64"),
        "{err}"
    );
    assert_eq!(fs::read(dest.join(BINARIES[0])).unwrap(), b"swapped out");

    fs::write(dest.join("SHA256SUMS"), b"deadbeef  VERSION\n").unwrap();
    let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
    assert!(
        err.contains("already exists with different checksums"),
        "{err}"
    );
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_refuses_to_write_through_a_release_dir_symlink() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    fs::create_dir(fixture.dir.path().join("elsewhere")).unwrap();
    let target = fixture.dir.path().join("elsewhere");
    let link = fixture.source.release_dir(VERSION);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(not(unix))]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();
    let err = fixture.source.fetch(&info, noop).unwrap_err().to_string();
    assert!(err.contains("is a symlink"), "{err}");
    assert!(target.is_dir());
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_cleans_the_stage_when_progress_cancels() {
    let fixture = standard_fixture();
    let info = fixture.source.check("stable", "").unwrap();
    let mut seen = 0usize;
    let err = fixture
        .source
        .fetch(&info, |name, _, _| {
            seen += 1;
            if seen == 3 {
                return Err(Error::Invalid(format!("cancelled at {name}")));
            }
            Ok(())
        })
        .unwrap_err();
    assert!(err.to_string().contains("cancelled at"), "{err}");
    assert!(!fixture.source.release_dir(VERSION).exists());
    assert_no_staging_left(fixture.dir.path());
}

#[test]
fn fetch_rejects_an_invalid_version() {
    let fixture = standard_fixture();
    let info = Info {
        tag: "v9".to_string(),
        version: "current".to_string(),
        ..Default::default()
    };
    assert!(!valid_version(&info.version));
    assert!(fixture.source.fetch(&info, noop).is_err());
}

#[test]
fn release_json_helper_matches_the_client() {
    let urls = BTreeMap::from([(
        "VERSION".to_string(),
        "/redirect/assets/VERSION".to_string(),
    )]);
    let text = release_json("http://host", "v1", false, false, &["VERSION"], &urls, 0);
    assert!(text.contains("\"browser_download_url\":\"http://host/redirect/assets/VERSION\""));
}

/// A GNU long name entry applies to the header that follows it.
fn gnu_long_name(long_name: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let payload = [long_name.as_bytes(), b"\0"].concat();
    out.extend_from_slice(&tar_header("PaxHeaders.0", b'L', payload.len() as u64));
    out.extend_from_slice(&payload);
    out.extend(std::iter::repeat_n(0u8, pad(payload.len())));
    out.extend_from_slice(&tar_header("./short.css", b'0', body.len() as u64));
    out.extend_from_slice(body);
    out.extend(std::iter::repeat_n(0u8, pad(body.len())));
    out.extend_from_slice(&tar_header("./index.html", b'0', 18));
    out.extend_from_slice(b"<html>index</html>");
    out.extend(std::iter::repeat_n(0u8, pad(18)));
    out.extend(std::iter::repeat_n(0u8, 1024));
    common::gzip(&out)
}

fn pad(len: usize) -> usize {
    (512 - len % 512) % 512
}

fn assert_no_staging_left(dir: &Path) {
    let leftovers: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with(".fetch-")
                || name.starts_with(".store-")
                || name.starts_with(".pointer-")
        })
        .collect();
    assert!(leftovers.is_empty(), "staging left behind: {leftovers:?}");
}
