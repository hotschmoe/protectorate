use protectorate_release::{
    cache_key, default_dir, parse_checksums, prune_plan, release_path, resolved_version,
    valid_version, validate_api, validate_channel, validate_repo, verify_checksum, Asset, Error,
    Info, Source,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::time::{Duration, SystemTime};

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

fn sums_for(entries: &[(&str, &[u8])]) -> String {
    entries
        .iter()
        .map(|(name, bytes)| format!("{}  {name}", hex(&Sha256::digest(bytes))))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn versions_are_validated() {
    let cases = [
        ("1.2.3", true),
        ("v1", true),
        ("1.2.3-rc.1+build.7", true),
        ("_x", false),
        (".hidden", false),
        ("", false),
        ("1.2 3", false),
        ("1.2/3", false),
        ("current", false),
        ("previous", false),
        (&"1".repeat(160), true),
        (&"1".repeat(161), false),
    ];
    for (v, want) in cases {
        assert_eq!(valid_version(v), want, "version {v:?}");
    }
}

#[test]
fn channels_are_validated() {
    assert!(validate_channel("stable").is_ok());
    assert!(validate_channel("dev").is_ok());
    for bad in ["", "Stable", "beta", "current"] {
        let err = validate_channel(bad).unwrap_err().to_string();
        assert!(err.contains("must be stable or dev"), "{err}");
    }
}

#[test]
fn repos_are_validated() {
    assert!(validate_repo("owner/repo").is_ok());
    assert!(validate_repo("a.b_c-/x.y_z-").is_ok());
    for bad in [
        "",
        "owner",
        "owner/",
        "/repo",
        "a/b/c",
        "a/b ",
        "o wner/repo",
        "//",
    ] {
        let err = validate_repo(bad).unwrap_err().to_string();
        assert!(err.contains("must be OWNER/REPO"), "{bad:?} {err}");
    }
}

#[test]
fn api_urls_are_validated() {
    assert_eq!(
        validate_api("https://api.github.com").unwrap(),
        "https://api.github.com"
    );
    assert_eq!(
        validate_api("https://ghe.example.com/api/v3/").unwrap(),
        "https://ghe.example.com/api/v3"
    );
    assert_eq!(
        validate_api("http://localhost:8080").unwrap(),
        "http://localhost:8080"
    );
    for bad in [
        "ftp://api.example.com",
        "https://",
        "https://user@host",
        "https://host?x=1",
        "https://host#f",
        "/relative",
        "https://host:port",
        "https://host:",
        "https://bad host",
    ] {
        assert!(validate_api(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn default_dir_honors_xdg() {
    std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-data");
    assert_eq!(
        default_dir(),
        std::path::Path::new("/tmp/xdg-data").join("protectorate/releases")
    );
    std::env::remove_var("XDG_DATA_HOME");
}

#[test]
fn source_construction_defaults_and_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    assert_eq!(s.api(), "https://api.github.com");
    assert_eq!(s.dir(), tmp.path().canonicalize().unwrap());
    assert!(Source::new("bad repo", "stable", "", "").is_err());
    assert!(Source::new("owner/repo", "nightly", "", "").is_err());
    assert!(Source::new("owner/repo", "stable", "ftp://x", "").is_err());
}

#[test]
fn asset_lookup_detects_duplicates() {
    let info = Info {
        tag: "v1".into(),
        assets: vec![
            Asset {
                name: "VERSION".into(),
                url: "https://host/v".into(),
            },
            Asset {
                name: "VERSION".into(),
                url: "https://host/v2".into(),
            },
        ],
        ..Default::default()
    };
    let err = info.asset_url("VERSION").unwrap_err().to_string();
    assert!(err.contains("duplicate release asset VERSION"), "{err}");
    let err = info.asset_url("SHA256SUMS").unwrap_err().to_string();
    assert!(err.contains("release v1 has no SHA256SUMS"), "{err}");
}

#[test]
fn channel_matching() {
    let cases: Vec<(Info, &str, &str, bool)> = vec![
        (
            Info {
                tag: "v1".into(),
                ..Default::default()
            },
            "stable",
            "",
            true,
        ),
        (
            Info {
                tag: "v1".into(),
                prerelease: true,
                ..Default::default()
            },
            "stable",
            "",
            false,
        ),
        (
            Info {
                tag: "v1".into(),
                draft: true,
                ..Default::default()
            },
            "stable",
            "",
            false,
        ),
        (
            Info {
                tag: "dev".into(),
                prerelease: true,
                ..Default::default()
            },
            "dev",
            "",
            true,
        ),
        (
            Info {
                tag: "dev".into(),
                ..Default::default()
            },
            "dev",
            "",
            false,
        ),
        (
            Info {
                tag: "v1".into(),
                prerelease: true,
                ..Default::default()
            },
            "dev",
            "",
            false,
        ),
        (
            Info {
                tag: "dev".into(),
                draft: true,
                ..Default::default()
            },
            "dev",
            "",
            false,
        ),
        (
            Info {
                tag: "v1".into(),
                prerelease: true,
                ..Default::default()
            },
            "stable",
            "v1",
            true,
        ),
        (
            Info {
                tag: "v1".into(),
                draft: true,
                ..Default::default()
            },
            "dev",
            "v1",
            false,
        ),
    ];
    for (info, channel, version, ok) in cases {
        assert_eq!(
            info.matches_channel(channel, version).is_ok(),
            ok,
            "{channel} {version}"
        );
    }
}

#[test]
fn cache_keys_and_release_paths() {
    assert_eq!(cache_key("stable", ""), "stable");
    assert_eq!(cache_key("dev", "1.2.3"), "dev:1.2.3");
    assert_eq!(release_path("stable", ""), "latest");
    assert_eq!(release_path("dev", ""), "tags/dev");
    assert_eq!(release_path("stable", "1.2.3"), "tags/1.2.3");
    assert_eq!(release_path("stable", "1.2+3"), "tags/1.2+3");
    assert_eq!(release_path("stable", "a b"), "tags/a%20b");
}

#[test]
fn checksums_verify_and_reject() {
    let payload = b"hello protectorate";
    let sums = sums_for(&[("VERSION", payload), ("protectorate-linux-amd64", b"bin")]);
    assert!(verify_checksum("VERSION", payload, sums.as_bytes()).is_ok());
    assert!(verify_checksum("protectorate-linux-amd64", b"bin", sums.as_bytes()).is_ok());

    let err = verify_checksum("VERSION", b"tampered", sums.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(err.contains("checksum mismatch for VERSION"), "{err}");

    let err = verify_checksum("VERSION", payload, b"deadbeef  VERSION\n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("missing or invalid checksum for VERSION"),
        "{err}"
    );

    let dup = format!(
        "{}  VERSION\n{}  VERSION\n",
        hex(&Sha256::digest(payload)),
        "a".repeat(64)
    );
    let err = verify_checksum("VERSION", payload, dup.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(err.contains("duplicate checksum for VERSION"), "{err}");

    let err = verify_checksum("VERSION", payload, b"not-a-line\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("missing or invalid checksum"), "{err}");

    let star = sums_for(&[("VERSION", payload)]).replace("  VERSION", " *VERSION");
    assert!(verify_checksum("VERSION", payload, star.as_bytes()).is_ok());

    let upper = sums_for(&[("VERSION", payload)]).to_uppercase();
    assert!(verify_checksum("VERSION", payload, upper.as_bytes()).is_ok());

    let map = parse_checksums(b"aa  one\nbb  two\n\nshort\n").unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map["one"], "aa");
}

#[test]
fn resolved_version_prefers_asset() {
    let payload = b"9.9.9\n";
    let sums = sums_for(&[("VERSION", payload)]);
    let info = Info {
        tag: "v1".into(),
        version: "v1".into(),
        ..Default::default()
    };
    assert_eq!(resolved_version(&info, None, None).unwrap(), "v1");
    assert_eq!(
        resolved_version(&info, Some(sums.as_bytes()), Some(payload)).unwrap(),
        "9.9.9"
    );
    let err = resolved_version(&info, None, Some(payload))
        .unwrap_err()
        .to_string();
    assert!(err.contains("VERSION requires SHA256SUMS"), "{err}");
    let dev = Info {
        tag: "dev".into(),
        ..Default::default()
    };
    assert!(resolved_version(&dev, None, None).is_err());
    let bad = Info {
        tag: "bad tag".into(),
        ..Default::default()
    };
    assert!(resolved_version(&bad, None, None).is_err());
}

#[test]
fn store_writes_release_json_and_sums() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let info = Info {
        tag: "v1.2.3".into(),
        version: "1.2.3".into(),
        channel: "stable".into(),
        ..Default::default()
    };
    let sums = sums_for(&[("VERSION", b"1.2.3\n")]);
    let dest = s.store(&info, sums.as_bytes()).unwrap();
    assert_eq!(dest, s.release_dir("1.2.3"));
    let stored: Info =
        serde_json::from_slice(&fs::read(dest.join("release.json")).unwrap()).unwrap();
    assert_eq!(stored, info);
    assert_eq!(fs::read(dest.join("SHA256SUMS")).unwrap(), sums.as_bytes());
    let no_version = Info {
        tag: "v2".into(),
        ..Default::default()
    };
    assert!(s
        .store(&no_version, sums.as_bytes())
        .unwrap()
        .ends_with("v2"));
    let bad = Info {
        tag: "current".into(),
        version: "current".into(),
        ..Default::default()
    };
    assert!(s.store(&bad, sums.as_bytes()).is_err());
}

#[test]
fn store_refuses_to_enter_an_existing_release_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    let info = Info {
        tag: "v1".into(),
        version: "v1".into(),
        ..Default::default()
    };
    s.store(&info, sums.as_bytes()).unwrap();

    // The same version with different checksums is refused, and the stored
    // metadata stays exactly as it was.
    let other = sums_for(&[("VERSION", b"2")]);
    let err = s.store(&info, other.as_bytes()).unwrap_err().to_string();
    assert!(
        err.contains("already exists with different checksums"),
        "{err}"
    );
    assert_eq!(
        fs::read(s.release_dir("v1").join("SHA256SUMS")).unwrap(),
        sums.as_bytes()
    );

    // Matching checksums are accepted without rewriting anything.
    fs::write(s.release_dir("v1").join("release.json"), b"kept").unwrap();
    let dest = s.store(&info, sums.as_bytes()).unwrap();
    assert_eq!(dest, s.release_dir("v1"));
    assert_eq!(fs::read(dest.join("release.json")).unwrap(), b"kept");
}

#[test]
fn store_refuses_to_write_through_a_release_dir_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    let info = Info {
        tag: "v1".into(),
        version: "v1".into(),
        ..Default::default()
    };
    fs::create_dir_all(s.dir().join("v1")).unwrap();
    fs::write(
        s.dir().join("v1").join("protectorate-linux-amd64"),
        b"active",
    )
    .unwrap();
    fs::remove_dir_all(s.dir().join("v1")).unwrap();
    fs::create_dir_all(tmp.path().join("active")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("active"), s.release_dir("v1")).unwrap();
    #[cfg(not(unix))]
    std::os::windows::fs::symlink_file(tmp.path().join("active"), s.release_dir("v1")).unwrap();

    let err = s.store(&info, sums.as_bytes()).unwrap_err().to_string();
    assert!(err.contains("is a symlink"), "{err}");
    assert!(!tmp.path().join("active").join("release.json").exists());
    assert!(tmp.path().join("active").is_dir());
    let leftovers: Vec<_> = fs::read_dir(s.dir())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".store-")
        })
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn activate_restore_and_current_pointer() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    for tag in ["v1", "v2"] {
        let info = Info {
            tag: tag.into(),
            version: tag.into(),
            ..Default::default()
        };
        s.store(&info, sums.as_bytes()).unwrap();
    }
    assert_eq!(s.current(), Info::default());
    assert!(s.activate("v9").is_err());
    s.activate("v1").unwrap();
    assert_eq!(s.current().version, "v1");
    s.activate("v2").unwrap();
    assert_eq!(s.current().version, "v2");
    assert_eq!(pointer_target(&s, "current"), std::path::Path::new("v2"));
    s.restore("").unwrap();
    assert!(!s.current_dir().exists());
    s.restore("").unwrap();
    s.activate("v1").unwrap();
    assert_eq!(pointer_target(&s, "current"), std::path::Path::new("v1"));
    assert!(s.activate("bad version").is_err());
    assert!(s.activate("current").is_err());
}

/// The version a pointer names, read from disk in whatever representation this
/// platform wrote: a symlink target on Unix, a pointer file where symlinks are
/// unavailable.
fn pointer_target(s: &Source, name: &str) -> std::path::PathBuf {
    let path = s.dir().join(name);
    let fi = fs::symlink_metadata(&path).unwrap();
    if fi.file_type().is_symlink() {
        fs::read_link(&path).unwrap()
    } else {
        let doc: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        std::path::PathBuf::from(doc["version"].as_str().unwrap())
    }
}

#[test]
fn pointer_files_are_read_on_every_platform() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    let info = Info {
        tag: "v1".into(),
        version: "v1".into(),
        ..Default::default()
    };
    s.store(&info, sums.as_bytes()).unwrap();

    // A pointer written as a file is understood exactly like a symlink.
    fs::write(s.dir().join("current"), br#"{"version":"v1"}"#).unwrap();
    assert_eq!(s.current().version, "v1");
    s.activate("v1").unwrap();
    assert_eq!(pointer_target(&s, "current"), std::path::Path::new("v1"));
    s.restore("").unwrap();
    assert!(!s.dir().join("current").exists());

    // A pointer file naming a version that is not on disk is no current release.
    fs::write(s.dir().join("current"), br#"{"version":"gone"}"#).unwrap();
    assert_eq!(s.current(), Info::default());
    for body in [
        r#"{"version":"not a version"}"#,
        r#"{"version":"current"}"#,
        r#"{"version":7}"#,
        "not json",
        "",
    ] {
        fs::write(s.dir().join("current"), body).unwrap();
        assert_eq!(s.current(), Info::default());
        let err = s.restore("").unwrap_err().to_string();
        assert!(err.contains("release pointer"), "{err}");
        assert!(s.dir().join("current").is_file());
    }

    fs::write(s.dir().join("current"), "not json").unwrap();
    let err = s.restore("").unwrap_err().to_string();
    assert!(err.contains("not a symlink or a pointer file"), "{err}");
}

#[test]
fn set_pointer_refuses_unsafe_pointer_types() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    fs::write(s.dir().join("current"), b"x").unwrap();
    let err = s.restore("").unwrap_err().to_string();
    assert!(err.contains("not a symlink"), "{err}");
    assert!(s.dir().join("current").is_file());

    fs::remove_file(s.dir().join("current")).unwrap();
    fs::create_dir(s.dir().join("current")).unwrap();
    fs::write(s.dir().join("current").join("release.json"), b"{}").unwrap();
    let err = s.restore("").unwrap_err().to_string();
    assert!(err.contains("is a directory"), "{err}");
    assert!(s.dir().join("current").is_dir());
    assert_eq!(s.current(), Info::default());

    let sums = sums_for(&[("VERSION", b"1")]);
    let info = Info {
        tag: "v1".into(),
        version: "v1".into(),
        ..Default::default()
    };
    s.store(&info, sums.as_bytes()).unwrap();
    let err = s.activate("v1").unwrap_err().to_string();
    assert!(err.contains("is a directory"), "{err}");
    assert!(s.dir().join("current").is_dir());
    assert_eq!(staging_leftovers(&s), Vec::<String>::new());
}

#[test]
fn prune_keeps_current_previous_and_newest() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    let base = SystemTime::now() - Duration::from_secs(10_000);
    for (i, tag) in ["v1", "v2", "v3", "v4"].iter().enumerate() {
        let info = Info {
            tag: (*tag).into(),
            version: (*tag).into(),
            ..Default::default()
        };
        s.store(&info, sums.as_bytes()).unwrap();
        let when = base + Duration::from_secs(i as u64 * 100);
        filetime_touch(&s.release_dir(tag), when);
    }
    fs::create_dir_all(s.dir().join("not-a-release")).unwrap();
    fs::write(s.dir().join("stray"), b"x").unwrap();
    s.activate("v2").unwrap();
    s.prune("v1").unwrap();
    assert!(s.release_dir("v2").is_dir());
    assert!(s.release_dir("v1").is_dir());
    assert!(!s.release_dir("v3").exists());
    assert!(!s.release_dir("v4").exists());
    assert!(s.dir().join("not-a-release").is_dir());
    assert!(s.dir().join("stray").is_file());
    assert_eq!(pointer_target(&s, "previous"), std::path::Path::new("v1"));

    s.prune("").unwrap();
    assert!(s.release_dir("v2").is_dir());
    assert!(s.release_dir("v1").is_dir());
    assert_eq!(pointer_target(&s, "previous"), std::path::Path::new("v1"));

    s.restore("").unwrap();
    s.prune("v1").unwrap();
    assert!(s.release_dir("v2").is_dir());
}

fn filetime_touch(path: &std::path::Path, when: SystemTime) {
    let file = fs::File::open(path).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(when))
        .unwrap();
}

fn staging_leftovers(s: &Source) -> Vec<String> {
    fs::read_dir(s.dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".pointer-"))
        .collect()
}

#[test]
fn reactivating_a_pointer_leaves_no_staging_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    for tag in ["v1", "v2"] {
        let info = Info {
            tag: tag.into(),
            version: tag.into(),
            ..Default::default()
        };
        s.store(&info, sums.as_bytes()).unwrap();
    }
    for tag in ["v1", "v2", "v1"] {
        s.activate(tag).unwrap();
        assert_eq!(s.current().version, tag);
        assert_eq!(staging_leftovers(&s), Vec::<String>::new());
    }
    s.prune("").unwrap();
    assert_eq!(staging_leftovers(&s), Vec::<String>::new());
    assert_eq!(pointer_target(&s, "previous"), std::path::Path::new("v2"));
}

/// Windows without symlink privileges (no Developer Mode, no admin) cannot
/// create the pointer symlink, so the pointer lands as a JSON file naming the
/// version. Everything above has to work the same either way.
#[cfg(windows)]
#[test]
fn windows_pointers_work_without_symlink_privileges() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    let sums = sums_for(&[("VERSION", b"1")]);
    for tag in ["v1", "v2"] {
        let info = Info {
            tag: tag.into(),
            version: tag.into(),
            ..Default::default()
        };
        s.store(&info, sums.as_bytes()).unwrap();
    }
    s.activate("v1").unwrap();
    s.activate("v2").unwrap();
    let fi = fs::symlink_metadata(s.dir().join("current")).unwrap();
    assert!(fi.file_type().is_symlink() || fi.is_file());
    assert_eq!(pointer_target(&s, "current"), std::path::Path::new("v2"));
    s.prune("v1").unwrap();
    assert_eq!(pointer_target(&s, "previous"), std::path::Path::new("v1"));
    assert!(s.release_dir("v1").is_dir());
    assert_eq!(staging_leftovers(&s), Vec::<String>::new());
    s.restore("").unwrap();
    assert!(!s.dir().join("current").exists());
}

#[test]
fn prune_plan_is_pure_and_deterministic() {
    let now = SystemTime::now();
    let releases: Vec<(String, SystemTime)> = [("v1", 3), ("v2", 2), ("v3", 1)]
        .iter()
        .map(|(n, age)| (n.to_string(), now - Duration::from_secs(age * 100)))
        .collect();
    let (keep, prev) = prune_plan("v1", "", &releases);
    assert_eq!(
        keep,
        ["v1".to_string(), "v3".to_string()].into_iter().collect()
    );
    assert_eq!(prev, "v3");
    let (keep, prev) = prune_plan("v1", "v2", &releases);
    assert_eq!(
        keep,
        ["v1".to_string(), "v2".to_string()].into_iter().collect()
    );
    assert_eq!(prev, "v2");
    let (keep, prev) = prune_plan("v1", "gone", &releases);
    assert_eq!(
        keep,
        ["v1".to_string(), "v3".to_string()].into_iter().collect()
    );
    assert_eq!(prev, "v3");
    let (keep, prev) = prune_plan("v1", "v1", &releases);
    assert_eq!(
        keep,
        ["v1".to_string(), "v3".to_string()].into_iter().collect()
    );
    assert_eq!(prev, "v3");
    let (keep, prev) = prune_plan("solo", "", &[]);
    assert_eq!(keep, ["solo".to_string()].into_iter().collect());
    assert_eq!(prev, "");
}

#[test]
fn available_cache_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let s = Source::new("owner/repo", "stable", "", tmp.path().to_str().unwrap()).unwrap();
    assert_eq!(s.available("stable"), Info::default());
    let info = Info {
        tag: "v1".into(),
        version: "1.2.3".into(),
        channel: "stable".into(),
        ..Default::default()
    };
    s.record("stable", "W/\"etag\"", info.clone());
    s.record("stable:1.2.3", "W/\"etag2\"", info.clone());
    assert_eq!(s.available("stable").version, "1.2.3");
    assert_eq!(s.available("dev"), Info::default());
    let cached = s.cached(&cache_key("stable", "1.2.3")).unwrap();
    assert_eq!(cached.etag, "W/\"etag2\"");
    assert_eq!(cached.info, info);
    assert!(s.cached("missing").is_none());
}

#[test]
fn info_json_matches_github_shape() {
    let value = json!({
        "tag_name": "v1.2.3",
        "name": "Release 1.2.3",
        "html_url": "https://github.com/o/r/releases/tag/v1.2.3",
        "prerelease": false,
        "draft": false,
        "assets": [{ "name": "VERSION", "browser_download_url": "https://host/v" }],
        "unexpected_field": 1
    });
    let info: Info = serde_json::from_str(&value.to_string()).unwrap();
    assert_eq!(info.tag, "v1.2.3");
    assert_eq!(info.notes_url, "https://github.com/o/r/releases/tag/v1.2.3");
    assert_eq!(info.asset_url("VERSION").unwrap(), "https://host/v");
}

#[test]
fn error_context_is_prefixed() {
    let err = Source::new("bad", "stable", "", "").unwrap_err();
    assert!(matches!(err, Error::Invalid(_)));
    assert!(err.to_string().starts_with("release repo"));
}
