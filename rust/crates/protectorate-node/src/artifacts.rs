//! Bounded artifact selection and durable copies for job replay.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const MAX_FILES: usize = 20_000;
const MAX_BYTES: u64 = 4 << 30;

#[derive(Clone)]
pub(crate) struct Artifact {
    pub path: String,
    pub file: PathBuf,
    pub mode: u32,
}

pub(crate) fn validate(patterns: &[String]) -> Result<(), String> {
    if patterns.len() > 128 {
        return Err("too many artifact globs".into());
    }
    for pattern in patterns {
        let path = Path::new(pattern);
        if pattern.is_empty()
            || pattern.contains('\\')
            || !path.components().all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(format!("invalid artifact glob {pattern:?}"));
        }
        glob::Pattern::new(pattern)
            .map_err(|e| format!("invalid artifact glob {pattern:?}: {e}"))?;
    }
    Ok(())
}

pub(crate) fn collect(
    workdir: &Path,
    patterns: &[String],
    dest: &Path,
    remaining_bytes: u64,
) -> Result<Vec<Artifact>, String> {
    if patterns.is_empty() {
        return Ok(vec![]);
    }
    validate(patterns)?;
    let canonical = fs::canonicalize(workdir).map_err(|e| e.to_string())?;
    let mut selected = BTreeSet::new();
    for pattern in patterns {
        let absolute = workdir.join(pattern);
        for hit in glob::glob(&absolute.to_string_lossy()).map_err(|e| e.to_string())? {
            let hit = hit.map_err(|e| e.to_string())?;
            gather(&hit, &canonical, &mut selected)?;
            if selected.len() > MAX_FILES {
                return Err("too many artifact files".into());
            }
        }
    }
    // Reserve the aggregate allowance before writing any artifact bytes.
    let mut total = 0u64;
    let mut sizes = BTreeMap::new();
    for path in &selected {
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        sizes.insert(path.clone(), metadata.len());
        total = total
            .checked_add(metadata.len())
            .ok_or("artifacts exceed disk budget")?;
        if total > MAX_BYTES || total > remaining_bytes {
            return Err("artifacts exceed disk budget".into());
        }
    }
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let result = (|| -> Result<Vec<Artifact>, String> {
        let mut result = Vec::with_capacity(selected.len());
        for path in selected {
            let rel = path.strip_prefix(&canonical).map_err(|e| e.to_string())?;
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if sizes.get(&path) != Some(&metadata.len()) {
                return Err("artifact changed while collecting".into());
            }
            let to = dest.join(rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let from = open_selected(&canonical, rel, &metadata).map_err(|e| e.to_string())?;
            let mut output = File::create(&to).map_err(|e| e.to_string())?;
            let copied = io::copy(&mut from.take(metadata.len() + 1), &mut output)
                .map_err(|e| e.to_string())?;
            if copied != metadata.len() {
                return Err(format!(
                    "artifact changed while copying: {}",
                    path.display()
                ));
            }
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = 0u32;
            result.push(Artifact {
                path: rel.to_string_lossy().replace('\\', "/"),
                file: to,
                mode,
            });
        }
        Ok(result)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(dest);
    }
    result
}
pub(crate) fn restore(dir: &Path) -> Result<Vec<Artifact>, String> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let root = fs::canonicalize(dir).map_err(|e| e.to_string())?;
    let mut selected = BTreeSet::new();
    gather(dir, &root, &mut selected)?;
    if selected.len() > MAX_FILES {
        return Err("too many retained artifacts".into());
    }
    let mut total = 0u64;
    selected
        .into_iter()
        .map(|file| {
            let metadata = fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
            total = total
                .checked_add(metadata.len())
                .ok_or("retained artifacts exceed 4 GiB")?;
            if total > MAX_BYTES {
                return Err("retained artifacts exceed 4 GiB".into());
            }
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = 0;
            Ok(Artifact {
                path: file
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                file,
                mode,
            })
        })
        .collect()
}
fn open_selected(root: &Path, rel: &Path, expected: &fs::Metadata) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let file = open_beneath(root, rel)?;
        let actual = file.metadata()?;
        if !actual.is_file()
            || actual.dev() != expected.dev()
            || actual.ino() != expected.ino()
            || actual.len() != expected.len()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact changed while opening",
            ));
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = options.open(root.join(rel))?;
        #[cfg(windows)]
        if !handle_path_beneath(&file, root)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "artifact escapes job workspace",
            ));
        }
        let actual = file.metadata()?;
        if !actual.is_file() || actual.len() != expected.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "artifact changed while opening",
            ));
        }
        Ok(file)
    }
}

#[cfg(unix)]
fn open_beneath(root: &Path, rel: &Path) -> io::Result<File> {
    use std::{
        ffi::CString,
        os::fd::FromRawFd,
        os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    };

    let mut dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root)?;
    let mut components = rel.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid artifact path",
            ));
        };
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid artifact path"))?;
        let flags = if components.peek().is_some() {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW
        } else {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOFOLLOW
        };
        // SAFETY: dir owns a valid directory descriptor, and name is NUL terminated.
        let fd =
            unsafe { libc::openat(std::os::fd::AsRawFd::as_raw_fd(&dir), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned descriptor.
        let opened = unsafe { File::from_raw_fd(fd) };
        if components.peek().is_some() {
            dir = opened;
        } else {
            return Ok(opened);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "empty artifact path",
    ))
}

#[cfg(windows)]
fn handle_path_beneath(file: &File, root: &Path) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;

    let handle = file.as_raw_handle();
    let mut path = vec![0u16; 512];
    loop {
        // SAFETY: handle is open, and path is a writable UTF-16 buffer.
        let n =
            unsafe { GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0) };
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if (n as usize) < path.len() {
            let resolved = PathBuf::from(String::from_utf16_lossy(&path[..n as usize]));
            let resolved = resolved.to_string_lossy().to_lowercase();
            let root = root.to_string_lossy().to_lowercase();
            let root = root.trim_end_matches(['\\', '/']);
            return Ok(resolved == root || resolved.starts_with(&format!("{root}\\")));
        }
        path.resize(n as usize + 1, 0);
    }
}
fn gather(path: &Path, root: &Path, found: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let canonical = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !canonical.starts_with(root) {
        return Ok(());
    }
    if metadata.is_file() {
        found.insert(canonical);
        return Ok(());
    }
    if metadata.is_dir() {
        for item in fs::read_dir(path).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            gather(&item.path(), root, found)?;
            if found.len() > MAX_FILES {
                return Err("too many artifact files".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collects_directory_and_rejects_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        fs::create_dir_all(work.join("out")).unwrap();
        fs::write(work.join("out/a"), b"hello").unwrap();
        assert!(validate(&["../other".into()]).is_err());
        let files = collect(&work, &["out".into()], &tmp.path().join("saved"), 100).unwrap();
        assert_eq!(files[0].path, "out/a");
        assert_eq!(fs::read(&files[0].file).unwrap(), b"hello");
        let too_small = tmp.path().join("too-small");
        assert!(collect(&work, &["out".into()], &too_small, 4).is_err());
        assert!(!too_small.exists());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_replaced_file_before_copying() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let selected = tmp.path().join("selected");
        let outside = tmp.path().join("outside");
        fs::write(&selected, b"allowed").unwrap();
        fs::write(&outside, b"private").unwrap();
        let expected = fs::symlink_metadata(&selected).unwrap();
        fs::remove_file(&selected).unwrap();
        symlink(&outside, &selected).unwrap();
        assert!(open_selected(tmp.path(), Path::new("selected"), &expected).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn skips_escaping_glob_match_and_keeps_safe_artifacts() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(work.join("output")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(work.join("output/result"), b"safe").unwrap();
        fs::write(outside.join("secret"), b"private").unwrap();
        symlink(&outside, work.join("escape")).unwrap();
        let files = collect(&work, &["*".into()], &tmp.path().join("saved"), 100).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "output/result");
        assert_eq!(fs::read(&files[0].file).unwrap(), b"safe");
    }
    #[cfg(unix)]
    #[test]
    fn rejects_replaced_parent_before_copying() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(work.join("out")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(work.join("out/result"), b"safe").unwrap();
        fs::write(outside.join("result"), b"secret").unwrap();
        let expected = fs::symlink_metadata(work.join("out/result")).unwrap();
        fs::rename(work.join("out"), work.join("old-out")).unwrap();
        symlink(&outside, work.join("out")).unwrap();
        assert!(open_selected(&work, Path::new("out/result"), &expected).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn rejects_replaced_file_with_reparse_point() {
        use std::os::windows::fs::symlink_file;
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        fs::create_dir(&work).unwrap();
        let selected = work.join("result");
        let outside = tmp.path().join("outside");
        fs::write(&selected, b"allowed").unwrap();
        fs::write(&outside, b"private").unwrap();
        let expected = fs::symlink_metadata(&selected).unwrap();
        fs::rename(&selected, work.join("original")).unwrap();
        if let Err(error) = symlink_file(&outside, &selected) {
            if error.kind() == io::ErrorKind::PermissionDenied {
                eprintln!("skipping Windows reparse-point test: symlink creation denied");
                return;
            }
            panic!("create test symlink: {error}");
        }
        let canonical = fs::canonicalize(&work).unwrap();
        assert!(open_selected(&canonical, Path::new("original"), &expected).is_ok());
        assert!(open_selected(&canonical, Path::new("result"), &expected).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn rejects_replaced_parent_with_reparse_point() {
        use std::os::windows::fs::symlink_dir;
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(work.join("out")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(work.join("out/result"), b"safe").unwrap();
        fs::write(outside.join("result"), b"evil").unwrap();
        let expected = fs::symlink_metadata(work.join("out/result")).unwrap();
        fs::rename(work.join("out"), work.join("original")).unwrap();
        if let Err(error) = symlink_dir(&outside, work.join("out")) {
            if error.kind() == io::ErrorKind::PermissionDenied {
                eprintln!("skipping Windows reparse-point test: symlink creation denied");
                return;
            }
            panic!("create test symlink: {error}");
        }
        let canonical = fs::canonicalize(&work).unwrap();
        assert!(open_selected(&canonical, Path::new("original/result"), &expected).is_ok());
        assert!(open_selected(&canonical, Path::new("out/result"), &expected).is_err());
    }
}
