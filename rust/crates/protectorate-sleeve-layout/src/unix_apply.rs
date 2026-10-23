//! All mutations use directory descriptors. A sleeve process can rename or
//! replace its home while the hub prepares it, so path validation alone is
//! insufficient to keep seed and link writes inside that home.

use crate::{context, invalid, Layout, Link};
use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

fn cstr(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| invalid("path contains NUL"))
}

fn root_fd() -> io::Result<OwnedFd> {
    let path = c"/";
    // SAFETY: path is NUL terminated; the returned fd is uniquely owned.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_child(parent: RawFd, name: &OsStr) -> io::Result<OwnedFd> {
    let name = cstr(name)?;
    // SAFETY: name is NUL terminated and parent is a live directory fd.
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let err = io::Error::last_os_error();
        if matches!(err.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
            return Err(invalid(
                "sleeve directory must be a directory without symlinks",
            ));
        }
        return Err(err);
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn mkdir_child(parent: RawFd, name: &OsStr, chmod_existing: bool) -> io::Result<OwnedFd> {
    let name_c = cstr(name)?;
    // SAFETY: name is NUL terminated and parent is a live directory fd.
    let ret = unsafe { libc::mkdirat(parent, name_c.as_ptr(), 0o700) };
    let created = ret == 0;
    if ret < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
        return Err(io::Error::last_os_error());
    }
    let child = open_child(parent, name)?;
    // Match the former layout behavior for a preexisting directory, without
    // resolving its name a second time.
    if (created || chmod_existing) && unsafe { libc::fchmod(child.as_raw_fd(), 0o700) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(child)
}

fn walk(start: &OwnedFd, path: &Path, create: bool) -> io::Result<OwnedFd> {
    let mut current = start.try_clone()?;
    let components: Vec<_> = path.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(invalid("expected a normalized relative sleeve path"));
        };
        current = if create {
            mkdir_child(current.as_raw_fd(), name, index + 1 == components.len())?
        } else {
            open_child(current.as_raw_fd(), name)?
        };
    }
    Ok(current)
}

fn absolute_dir(path: &Path) -> io::Result<OwnedFd> {
    let rel = path
        .strip_prefix("/")
        .map_err(|_| invalid("expected absolute path"))?;
    walk(&root_fd()?, rel, true)
}

fn parent_and_name<'a>(home: &Path, path: &'a Path) -> io::Result<(&'a Path, &'a OsStr)> {
    let rel = path
        .strip_prefix(home)
        .map_err(|_| invalid("layout path outside private home"))?;
    let name = rel
        .file_name()
        .ok_or_else(|| invalid("layout path has no file name"))?;
    let parent = rel
        .parent()
        .ok_or_else(|| invalid("layout path has no parent"))?;
    Ok((parent, name))
}

pub(super) fn apply(layout: &Layout) -> io::Result<()> {
    let home = absolute_dir(&layout.home)
        .map_err(|e| context(format!("create sleeve home {}", layout.home.display()), e))?;
    for dir in &layout.dirs {
        if let Ok(rel) = dir.strip_prefix(&layout.home) {
            walk(&home, rel, true)
        } else {
            absolute_dir(dir)
        }
        .map_err(|e| context(format!("create {}", dir.display()), e))?;
    }
    for seed in &layout.seeds {
        let (rel, name) = parent_and_name(&layout.home, &seed.path)?;
        let parent = walk(&home, rel, false)?;
        create_seed(parent.as_raw_fd(), name, seed.content.as_bytes())
            .map_err(|e| context(format!("seed {}", seed.path.display()), e))?;
    }
    for link in &layout.links {
        let (rel, name) = parent_and_name(&layout.home, &link.path)?;
        let parent = walk(&home, rel, false)?;
        apply_link(parent.as_raw_fd(), name, link)?;
    }
    Ok(())
}

fn create_seed(parent: RawFd, name: &OsStr, content: &[u8]) -> io::Result<()> {
    let name = cstr(name)?;
    // O_EXCL makes an existing seed (including a symlink) remain untouched.
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::AlreadyExists {
            return Ok(());
        }
        return Err(err);
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(content).and_then(|_| file.sync_all())
}

fn read_link(parent: RawFd, name: &CString) -> io::Result<Vec<u8>> {
    let mut buf = vec![0; 4096];
    let size =
        unsafe { libc::readlinkat(parent, name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(size as usize);
    Ok(buf)
}

fn apply_link(parent: RawFd, name: &OsStr, link: &Link) -> io::Result<()> {
    let name = cstr(name)?;
    let target = cstr(link.target.as_os_str())?;
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    let exists =
        unsafe { libc::fstatat(parent, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) } == 0;
    if exists {
        if stat.st_mode & libc::S_IFMT == libc::S_IFLNK
            && read_link(parent, &name)? == link.target.as_os_str().as_bytes()
        {
            return Ok(());
        }
        move_aside(parent, &name)?;
    } else {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::NotFound {
            return Err(context(format!("inspect {}", link.path.display()), err));
        }
    }
    if unsafe { libc::symlinkat(target.as_ptr(), parent, name.as_ptr()) } < 0 {
        return Err(context(
            format!("link {}", link.path.display()),
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

fn move_aside(parent: RawFd, name: &CString) -> io::Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Both names are resolved against the same pinned directory. A concurrent
    // leaf replacement may be moved instead, but cannot escape that directory.
    for attempt in 0..100 {
        let mut aside = name.as_bytes().to_vec();
        aside.extend_from_slice(format!(".orphaned-{stamp}-{attempt}").as_bytes());
        let aside = CString::new(aside).map_err(|_| invalid("invalid aside name"))?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                parent,
                name.as_ptr(),
                parent,
                aside.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(target_os = "linux"))]
        let result = unsafe { libc::renameat(parent, name.as_ptr(), parent, aside.as_ptr()) };
        if result == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::AlreadyExists {
            return Err(err);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free orphaned link name",
    ))
}
