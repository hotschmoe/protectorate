//! Explicit, bounded publication of a Flutter SDK cache generation.
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, OsStr},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 100_000;

struct Publication {
    relative: &'static [&'static str],
    label: &'static str,
    manifest_name: &'static str,
    hard_limit: u64,
    selected: Option<&'static [&'static str]>,
    go_mod_download: bool,
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = make_writable(&self.0);
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn make_writable(dir: &Path) -> io::Result<()> {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if fs::symlink_metadata(&path)?.is_dir() {
            make_writable(&path)?;
        }
    }
    Ok(())
}

pub fn publish(state: &Path, sdk: &Path, max_bytes: u64) -> io::Result<PathBuf> {
    publish_named(
        state,
        sdk,
        max_bytes,
        Publication {
            relative: &["bin", "cache"],
            label: "flutter",
            manifest_name: ".protectorate-flutter-cache.json",
            hard_limit: 8 * 1024 * 1024 * 1024,
            selected: None,
            go_mod_download: false,
        },
    )
}

pub fn publish_tools(state: &Path, sdk: &Path, max_bytes: u64) -> io::Result<PathBuf> {
    publish_named(
        state,
        sdk,
        max_bytes,
        Publication {
            relative: &["packages", "flutter_tools", ".dart_tool"],
            label: "flutter-tools",
            manifest_name: ".protectorate-flutter-tools.json",
            hard_limit: 64 * 1024 * 1024,
            selected: None,
            go_mod_download: false,
        },
    )
}

/// Freeze only reusable Pub packages and global tools. Transient lock, root,
/// and download state from the owner's cache is never exposed to sleeves.
pub fn publish_pub(state: &Path, pub_cache: &Path, max_bytes: u64) -> io::Result<PathBuf> {
    publish_named(
        state,
        pub_cache,
        max_bytes,
        Publication {
            relative: &[],
            label: "pub",
            manifest_name: ".protectorate-pub-cache.json",
            hard_limit: 8 * 1024 * 1024 * 1024,
            selected: Some(&["hosted", "hosted-hashes", "git", "global_packages", "bin"]),
            go_mod_download: false,
        },
    )
}

/// Freeze the owner's Go module cache once for read-only reuse. Build output
/// stays in each sleeve's private cache; no live owner cache is mounted.
pub fn publish_go_mod(state: &Path, mod_cache: &Path, max_bytes: u64) -> io::Result<PathBuf> {
    publish_named(
        state,
        mod_cache,
        max_bytes,
        Publication {
            relative: &["cache", "download"],
            label: "go-mod",
            manifest_name: ".protectorate-go-mod-cache.json",
            hard_limit: 8 * 1024 * 1024 * 1024,
            selected: None,
            go_mod_download: true,
        },
    )
}

fn publish_named(
    state: &Path,
    sdk: &Path,
    max_bytes: u64,
    kind: Publication,
) -> io::Result<PathBuf> {
    let Publication {
        relative,
        label,
        manifest_name,
        hard_limit,
        selected,
        go_mod_download,
    } = kind;
    if max_bytes == 0 || max_bytes > hard_limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache publication budget is outside its hard limit",
        ));
    }
    let sdk_real = fs::canonicalize(sdk)?;
    if sdk_real != sdk
        || !relative
            .iter()
            .fold(sdk.to_path_buf(), |path, part| path.join(part))
            .is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache source must be a real absolute directory with the selected cache",
        ));
    }
    let state = fs::canonicalize(state)?;
    let cache_root = state.join("cache");
    fs::create_dir_all(&cache_root)?;
    if fs::canonicalize(&cache_root)? != cache_root {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache root contains a symlink",
        ));
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = cache_root.join(format!(".{label}-publish-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging)?;
    let mut cleanup = Staging(staging.clone());
    let (content_hash, bytes, entries) = publish_into(
        sdk,
        &staging,
        max_bytes,
        relative,
        selected,
        go_mod_download,
    )?;
    let mut identity = Sha256::new();
    identity.update(sdk.as_os_str().as_encoded_bytes());
    identity.update(content_hash.as_bytes());
    let hash = format!("{:x}", identity.finalize());
    let mut manifest = serde_json::json!({"sdk": sdk.display().to_string(), "sha256": hash, "contentSha256": content_hash, "bytes": bytes, "entries": entries});
    if matches!(label, "pub" | "go-mod") {
        manifest["source"] = serde_json::json!(sdk.display().to_string());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging.join(manifest_name))?;
    serde_json::to_writer(&mut file, &manifest)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::set_permissions(
        staging.join(manifest_name),
        fs::Permissions::from_mode(0o400),
    )?;
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o500))?;
    let published = cache_root.join(format!("{label}-{hash}"));
    if published.exists() {
        make_writable(&staging)?;
        fs::remove_dir_all(&staging)?;
        cleanup.0 = PathBuf::new();
        if fs::symlink_metadata(&published)?.file_type().is_symlink()
            || fs::canonicalize(&published)? != published
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "published Flutter cache path is a symlink",
            ));
        }
        let old_manifest = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(published.join(manifest_name))?;
        let mut old_bytes = Vec::new();
        old_manifest.take(4096).read_to_end(&mut old_bytes)?;
        let existing: serde_json::Value = serde_json::from_slice(&old_bytes)?;
        if existing != manifest {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "published Flutter cache path has a different manifest",
            ));
        }
        return Ok(published);
    }
    fs::rename(&staging, &published)?;
    cleanup.0 = PathBuf::new();
    File::open(&cache_root)?.sync_all()?;
    Ok(published)
}

fn publish_into(
    source: &Path,
    target: &Path,
    limit: u64,
    relative: &[&str],
    selected: Option<&[&str]>,
    go_mod_download: bool,
) -> io::Result<(String, u64, usize)> {
    let sdk = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)?;
    let mut cache = sdk;
    for part in relative {
        cache = open_child(&cache, OsStr::new(part))?;
        if !cache.metadata()?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Flutter cache path is not a directory",
            ));
        }
    }
    let mut state = CopyState {
        hash: Sha256::new(),
        bytes: 0,
        entries: 0,
    };
    copy_tree(
        &cache,
        target,
        Path::new(""),
        &mut state,
        limit,
        0,
        CopySelection {
            names: selected,
            go_mod_download,
        },
    )?;
    Ok((
        format!("{:x}", state.hash.finalize()),
        state.bytes,
        state.entries,
    ))
}

fn open_child(parent: &File, name: &OsStr) -> io::Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in Flutter cache path"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

struct CopyState {
    hash: Sha256,
    bytes: u64,
    entries: usize,
}

#[derive(Clone, Copy)]
struct CopySelection<'a> {
    names: Option<&'a [&'a str]>,
    go_mod_download: bool,
}

/// Go's download cache has module archives and metadata under `module/@v`.
/// Skip transient lock files and refuse unrelated files, which may carry
/// credentials or private tool state.
fn go_download_file(relative: &Path) -> io::Result<bool> {
    if relative.parent().and_then(Path::file_name) != Some(OsStr::new("@v")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Go download cache contains an unexpected file",
        ));
    }
    let name = relative
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Go download cache has a non-UTF-8 filename",
            )
        })?;
    if name == "list" {
        return Ok(true);
    }
    if name.ends_with(".lock") {
        return Ok(false);
    }
    let Some((version, ext)) = name.rsplit_once('.') else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Go download cache contains an unexpected file",
        ));
    };
    if !version.starts_with('v') || !matches!(ext, "info" | "mod" | "zip" | "ziphash") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Go download cache contains an unexpected file",
        ));
    }
    Ok(true)
}

fn copy_tree(
    source: &File,
    target_root: &Path,
    relative_root: &Path,
    state: &mut CopyState,
    limit: u64,
    depth: usize,
    selection: CopySelection<'_>,
) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flutter cache exceeds directory depth limit",
        ));
    }
    let before = source.metadata()?.modified()?;
    let mut items = fs::read_dir(format!("/proc/self/fd/{}", source.as_raw_fd()))?
        .collect::<io::Result<Vec<_>>>()?;
    items.sort_by_key(fs::DirEntry::file_name);
    for item in items {
        if selection.go_mod_download && depth == 0 && item.file_name() == OsStr::new("sumdb") {
            continue;
        }
        if depth == 0
            && selection.names.is_some_and(|names| {
                !names
                    .iter()
                    .any(|name| item.file_name() == OsStr::new(name))
            })
        {
            continue;
        }
        let relative = relative_root.join(item.file_name());
        if selection.go_mod_download && item.file_type()?.is_file() && !go_download_file(&relative)?
        {
            continue;
        }
        state.entries += 1;
        if state.entries > MAX_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Flutter cache exceeds entry limit",
            ));
        }
        let destination = target_root.join(&relative);
        let mut input = open_child(source, &item.file_name())?;
        let metadata = input.metadata()?;
        if metadata.is_dir() {
            state.hash.update(b"D");
            state.hash.update(relative.as_os_str().as_encoded_bytes());
            fs::create_dir(&destination)?;
            copy_tree(
                &input,
                target_root,
                &relative,
                state,
                limit,
                depth + 1,
                CopySelection {
                    names: None,
                    ..selection
                },
            )?;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o500))?;
        } else if metadata.is_file() {
            state.bytes = state.bytes.checked_add(metadata.len()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Flutter cache size overflow")
            })?;
            if state.bytes > limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Flutter cache exceeds byte limit",
                ));
            }
            state.hash.update(b"F");
            state.hash.update(relative.as_os_str().as_encoded_bytes());
            state.hash.update(metadata.len().to_le_bytes());
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            let mut copied = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                if copied.saturating_add(read as u64) > metadata.len()
                    || state
                        .bytes
                        .saturating_sub(metadata.len())
                        .saturating_add(copied)
                        .saturating_add(read as u64)
                        > limit
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Flutter cache grew while publishing",
                    ));
                }
                output.write_all(&buffer[..read])?;
                state.hash.update(&buffer[..read]);
                copied += read as u64;
            }
            let after = input.metadata()?;
            if copied != metadata.len()
                || after.len() != metadata.len()
                || after.modified()? != metadata.modified()?
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Flutter cache changed during publication",
                ));
            }
            drop(output);
            let mode = if metadata.permissions().mode() & 0o111 != 0 {
                0o500
            } else {
                0o400
            };
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode))?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Flutter cache contains a special file",
            ));
        }
    }
    if source.metadata()?.modified()? != before {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Flutter cache directory changed during publication",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_is_bounded_hashed_and_read_only() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let sdk = root.path().join("flutter");
        fs::create_dir(&state).unwrap();
        fs::create_dir_all(sdk.join("bin/cache/dart-sdk")).unwrap();
        fs::create_dir_all(sdk.join("packages/flutter_tools/.dart_tool")).unwrap();
        fs::write(sdk.join("bin/cache/dart-sdk/version"), b"test").unwrap();
        fs::write(
            sdk.join("packages/flutter_tools/.dart_tool/package_config.json"),
            b"{}",
        )
        .unwrap();
        assert!(publish(&state, &sdk, 3).is_err());
        let lower = publish(&state, &sdk, 8).unwrap();
        assert_eq!(publish(&state, &sdk, 8).unwrap(), lower);
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(lower.join(".protectorate-flutter-cache.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest.get("source").is_none());
        assert_eq!(fs::read(lower.join("dart-sdk/version")).unwrap(), b"test");
        assert_eq!(
            fs::metadata(&lower).unwrap().permissions().mode() & 0o222,
            0
        );
        assert_eq!(
            fs::metadata(lower.join("dart-sdk/version"))
                .unwrap()
                .permissions()
                .mode()
                & 0o222,
            0
        );
        let tools = publish_tools(&state, &sdk, 1024).unwrap();
        assert_eq!(fs::read(tools.join("package_config.json")).unwrap(), b"{}");
    }

    #[test]
    fn pub_publication_selects_only_reusable_content_and_rejects_links() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let source = root.path().join("owner/.pub-cache");
        fs::create_dir(&state).unwrap();
        fs::create_dir_all(source.join("hosted/pub.dev/pkg-1.0/lib")).unwrap();
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::create_dir_all(source.join("_temp")).unwrap();
        fs::write(
            source.join("hosted/pub.dev/pkg-1.0/lib/pkg.dart"),
            b"library pkg;",
        )
        .unwrap();
        fs::write(source.join("bin/protoc-gen-dart"), b"#!/bin/sh\n").unwrap();
        fs::write(source.join("_temp/secret"), b"private").unwrap();
        assert!(publish_pub(&state, &source, 5).is_err());
        let lower = publish_pub(&state, &source, 1024).unwrap();
        assert!(lower.join("hosted/pub.dev/pkg-1.0/lib/pkg.dart").is_file());
        assert!(lower.join("bin/protoc-gen-dart").is_file());
        assert!(!lower.join("_temp").exists());
        assert_eq!(publish_pub(&state, &source, 1024).unwrap(), lower);
        symlink(
            root.path(),
            source.join("hosted/pub.dev/pkg-1.0/lib/escape"),
        )
        .unwrap();
        assert!(publish_pub(&state, &source, 1024).is_err());
    }

    #[test]
    fn go_module_publication_is_bounded_and_rejects_unexpected_files() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let source = root.path().join("owner/go/pkg/mod");
        fs::create_dir(&state).unwrap();
        fs::create_dir_all(source.join("cache/download/example.org/module/@v")).unwrap();
        fs::write(
            source.join("cache/download/example.org/module/@v/v1.0.0.mod"),
            b"module example.org/module\n",
        )
        .unwrap();
        fs::write(
            source.join("cache/download/example.org/module/@v/v1.0.0.lock"),
            b"",
        )
        .unwrap();
        fs::create_dir_all(source.join("cache/vcs/private")).unwrap();
        fs::write(
            source.join("cache/vcs/private/config"),
            b"token-in-remote-url",
        )
        .unwrap();
        assert!(publish_go_mod(&state, &source, 4).is_err());
        let lower = publish_go_mod(&state, &source, 1024).unwrap();
        assert_eq!(publish_go_mod(&state, &source, 1024).unwrap(), lower);
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(lower.join(".protectorate-go-mod-cache.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["source"], source.to_string_lossy().as_ref());
        assert_eq!(
            fs::read(lower.join("example.org/module/@v/v1.0.0.mod")).unwrap(),
            b"module example.org/module\n"
        );
        assert!(!lower.join("example.org/module/@v/v1.0.0.lock").exists());
        assert!(!lower.join("vcs").exists());
        assert_eq!(
            fs::metadata(&lower).unwrap().permissions().mode() & 0o222,
            0
        );
        fs::write(
            source.join("cache/download/example.org/module/@v/credentials.json"),
            b"secret",
        )
        .unwrap();
        assert!(publish_go_mod(&state, &source, 1024).is_err());
    }
}
