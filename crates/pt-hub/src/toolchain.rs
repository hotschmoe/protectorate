//! What a sleeve needs to build real projects with the owner's toolchains (SPEC 3.4). Sleeves run
//! with a private home, a cleared environment and a login shell that resets PATH, so each piece
//! is restored here:
//!
//! - PATH keeps the hub's CLI dir and the hub's own PATH, then the owner's tool dirs.
//! - The owner's rustup toolchains are read-only; cargo's registry and git caches, the Go module
//!   and build caches and the pub cache are the owner's own, bound read-write, so builds start
//!   warm and every sleeve fills the same cache (their tools lock these for concurrent use).
//! - The Flutter SDK is read-only except `bin/cache`, which the tool writes on every run.
//! - git commits as the owner.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct Toolchain {
    pub path: String,
    pub env: Vec<(String, String)>,
    pub rw: Vec<PathBuf>,
}

/// Dirs under the owner's home that are on a sleeve's PATH when they exist and the hub's own PATH
/// lacks them. `extra` is the config's `sleeve_path`.
const OWNER_BINS: [&str; 2] = [".cargo/bin", "go/bin"];

pub fn prepare(
    owner_home: &Path,
    home: &Path,
    cli_bin: &Path,
    hub_path: &str,
    extra: &[String],
) -> io::Result<Toolchain> {
    let mut parts: Vec<String> = vec![cli_bin.to_string_lossy().into_owned()];
    parts.extend(
        hub_path
            .split(':')
            .filter(|p| !p.is_empty())
            .map(String::from),
    );
    let tail = OWNER_BINS
        .iter()
        .map(|r| owner_home.join(r).to_string_lossy().into_owned())
        .chain(["/usr/local/go/bin".to_string()])
        .chain(extra.iter().cloned());
    for dir in tail {
        if !parts.contains(&dir) && Path::new(&dir).is_dir() {
            parts.push(dir);
        }
    }
    let path = parts.join(":");

    let mut t = Toolchain {
        path: path.clone(),
        ..Toolchain::default()
    };
    let owned = |key: &str, default: PathBuf| -> PathBuf {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or(default)
    };

    let rustup = owned("RUSTUP_HOME", owner_home.join(".rustup"));
    let cargo = owned("CARGO_HOME", owner_home.join(".cargo"));
    if rustup.is_dir() {
        t.env
            .push(("RUSTUP_HOME".into(), rustup.to_string_lossy().into_owned()));
    }
    if cargo.is_dir() {
        t.env
            .push(("CARGO_HOME".into(), cargo.to_string_lossy().into_owned()));
        t.rw.extend(existing_dirs([cargo.join("registry"), cargo.join("git")]));
    }

    let gopath = owned("GOPATH", owner_home.join("go"));
    let gopath = gopath.to_string_lossy();
    let gopath = gopath.split(':').next().unwrap_or_default();
    let gomod = owned("GOMODCACHE", Path::new(gopath).join("pkg/mod"));
    let cache_home = owned("XDG_CACHE_HOME", owner_home.join(".cache"));
    let gobuild = owned("GOCACHE", cache_home.join("go-build"));
    let pub_cache = owned("PUB_CACHE", owner_home.join(".pub-cache"));
    for (key, dir) in [
        ("GOMODCACHE", gomod),
        ("GOCACHE", gobuild),
        ("PUB_CACHE", pub_cache),
    ] {
        if dir.is_dir() {
            t.env.push((key.into(), dir.to_string_lossy().into_owned()));
            t.rw.push(dir);
        }
    }

    if let Some(sdk_cache) = flutter_cache(&path) {
        t.rw.push(sdk_cache);
    }

    seed_profile(home)?;
    seed_gitconfig(home)?;
    t.env.push(("PROTECTORATE_SLEEVE_PATH".into(), path));
    Ok(t)
}

fn existing_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// The `bin/cache` of the Flutter SDK that `flutter` on `path` belongs to.
fn flutter_cache(path: &str) -> Option<PathBuf> {
    let exe = path
        .split(':')
        .map(|d| Path::new(d).join("flutter"))
        .find(|p| p.is_file())?;
    let cache = fs::canonicalize(exe).ok()?.parent()?.join("cache");
    cache.is_dir().then_some(cache)
}

const PROFILE: &str = "# Protectorate: /etc/profile resets PATH in a login shell.\n\
    if [ -n \"$PROTECTORATE_SLEEVE_PATH\" ]; then PATH=\"$PROTECTORATE_SLEEVE_PATH\"; export PATH; fi\n";

fn seed_profile(home: &Path) -> io::Result<()> {
    seed(&home.join(".profile"), PROFILE)
}

/// The owner's git identity, so a sleeve's commits do not need flags. A sleeve may change it:
/// the file is only written when the home has none.
fn seed_gitconfig(home: &Path) -> io::Result<()> {
    let path = home.join(".gitconfig");
    if path.symlink_metadata().is_ok() {
        return Ok(());
    }
    let get = |key: &str| {
        Command::new("git")
            .args(["config", "--global", "--get", key])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|v| !v.is_empty() && !v.contains(['\n', '"', '\\']))
    };
    let (name, email) = (get("user.name"), get("user.email"));
    if name.is_none() && email.is_none() {
        return Ok(());
    }
    let mut text = String::from("[user]\n");
    if let Some(n) = name {
        text.push_str(&format!("\tname = \"{n}\"\n"));
    }
    if let Some(e) = email {
        text.push_str(&format!("\temail = \"{e}\"\n"));
    }
    seed(&path, &text)
}

/// Writes `text` when `path` does not exist; never follows or replaces what a sleeve left.
fn seed(path: &Path, text: &str) -> io::Result<()> {
    let opened = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);
    match opened {
        Ok(mut f) => f
            .write_all(text.as_bytes())
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(io::Error::new(e.kind(), format!("{}: {e}", path.display()))),
    }
}
