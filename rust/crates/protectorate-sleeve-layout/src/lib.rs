//! Per-harness sleeve home preparation, ported from internal/sleeve/layout.go.
//! A [`Layout`] is a typed, inspectable plan for one harness's private home:
//! directories, first-run seed files, auth links and environment values.
//! [`Layout::apply`] is idempotent and refuses to follow symlinks inside the
//! private home. Tool cache copying and overlays are the sandbox launcher's
//! concern, not this layout's.

use std::{
    fs::{self},
    io::{self},
    path::{Component, Path, PathBuf},
};

#[cfg(not(unix))]
use std::{
    fs::OpenOptions,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
mod unix_apply;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Codex,
    Grok,
    Pi,
}

impl Harness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
        }
    }
}

/// Inputs shared by every harness layout. `home` is the hub state home; the
/// harness auth directory is `home/auth/<harness>`, bound read-write at its
/// host path. `user_home` is the private home's path inside the sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub home: PathBuf,
    pub user_home: PathBuf,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
    pub go_mod_cache: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub path: PathBuf,
    pub target: PathBuf,
}

/// Written only when the file does not exist yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seed {
    pub path: PathBuf,
    pub content: String,
}

const CLAUDE_SEED: &str = "{\"hasCompletedOnboarding\":true}\n";

// The sandbox is the boundary, so Codex runs without its own approval gate
// and sandbox; the user's config.toml, with its trust and model, is not copied.
const CODEX_SEED: &str = "approval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n";

// The profile seed keeps the sleeve's PATH in login shells: /etc/profile
// resets PATH, and harnesses such as Codex run every command through `bash -lc`.
const PROFILE_SEED: &str = concat!(
    "# Written by the protectorate hub: login shells keep the sleeve's PATH.\n",
    "if [ -n \"$PROTECTORATE_SLEEVE_PATH\" ]; then PATH=\"$PROTECTORATE_SLEEVE_PATH\"; export PATH; fi\n",
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    home: PathBuf,
    dirs: Vec<PathBuf>,
    links: Vec<Link>,
    seeds: Vec<Seed>,
    env: Vec<(String, String)>,
}

impl Layout {
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
    pub fn links(&self) -> &[Link] {
        &self.links
    }
    pub fn seeds(&self) -> &[Seed] {
        &self.seeds
    }
    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    /// The layout paths below the private home must never be symlinks: the
    /// sleeve can replace paths inside its mounted home between launches.
    pub fn validate(&self) -> io::Result<()> {
        let mut paths: Vec<&Path> = self.dirs.iter().map(|p| p.as_path()).collect();
        for s in &self.seeds {
            if let Some(dir) = s.path.parent() {
                paths.push(dir);
            }
        }
        for l in &self.links {
            if let Some(dir) = l.path.parent() {
                paths.push(dir);
            }
        }
        for mut p in paths {
            while within(p, &self.home) {
                match fs::symlink_metadata(p) {
                    Ok(fi) if !fi.is_dir() => {
                        return Err(invalid(format!(
                            "sleeve directory {} must be a directory without symlinks",
                            p.display()
                        )));
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(context(
                            format!("inspect sleeve directory {}", p.display()),
                            e,
                        ))
                    }
                    Ok(_) => {}
                }
                if p == self.home {
                    break;
                }
                p = p.parent().unwrap_or(p);
            }
        }
        Ok(())
    }

    /// Creates the directories, symlinks and seed files. Idempotent: existing
    /// seeds are preserved, a correct link is kept, and a file in the way of
    /// a link is renamed aside.
    pub fn apply(&self) -> io::Result<()> {
        self.validate()?;
        #[cfg(unix)]
        return unix_apply::apply(self);
        #[cfg(not(unix))]
        {
            create_dir_all(&self.home, 0o700)
                .map_err(|e| context(format!("create sleeve home {}", self.home.display()), e))?;
            for d in &self.dirs {
                create_dir_all(d, 0o700)
                    .map_err(|e| context(format!("create {}", d.display()), e))?;
            }
            for s in &self.seeds {
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&s.path);
                let mut file = match file {
                    Ok(f) => f,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        return Err(context(format!("seed {}", s.path.display()), e));
                    }
                };
                let result = (|| -> io::Result<()> {
                    file.write_all(s.content.as_bytes())?;
                    file.sync_all()?;
                    Ok(())
                })();
                drop(file);
                result.map_err(|e| context(format!("seed {}", s.path.display()), e))?;
            }
            for l in &self.links {
                apply_link(l)?;
            }
            Ok(())
        }
    }
}

#[cfg(not(unix))]
fn apply_link(link: &Link) -> io::Result<()> {
    match fs::symlink_metadata(&link.path) {
        Ok(fi) if fi.file_type().is_symlink() => {
            if fs::read_link(&link.path).ok().as_deref() == Some(link.target.as_path()) {
                return Ok(());
            }
            move_aside(&link.path)?;
        }
        Ok(_) => move_aside(&link.path)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(context(format!("inspect {}", link.path.display()), e));
        }
    }
    let _ = &link;
    Err(invalid("auth links require a Unix host"))
}

#[cfg(not(unix))]
fn move_aside(path: &Path) -> io::Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let aside = path.with_extension(format!("orphaned-{stamp}"));
    fs::rename(path, &aside).map_err(|e| context(format!("move {} aside", path.display()), e))?;
    Ok(())
}

/// Builds the harness layout for one sleeve. `sleeve_home` is the private
/// home on the host; the same paths relative to `user_home` are what the
/// harness sees inside the sandbox.
pub fn layout(cfg: &Config, harness: Harness, sleeve_home: &Path) -> io::Result<Layout> {
    let home = normalize(sleeve_home)?;
    let user_home = normalize(&cfg.user_home)?;
    let hub_home = normalize(&cfg.home)?;
    let relative = home
        .strip_prefix(hub_home.join("sleeves"))
        .map_err(|_| invalid("sleeve home is outside the sleeve state directory"))?;
    let parts = relative.components().collect::<Vec<_>>();
    if parts.len() != 2 || parts[1] != Component::Normal(std::ffi::OsStr::new("home")) {
        return Err(invalid("sleeve home must be sleeves/<name>/home"));
    }
    let name = parts[0]
        .as_os_str()
        .to_str()
        .ok_or_else(|| invalid("invalid sleeve name"))?;
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid("invalid sleeve name"));
    }
    let auth = hub_home.join("auth").join(harness.as_str());
    let host = |rel: &str| home.join(rel);
    let inside = |rel: &str| user_home.join(rel);
    let mut l = Layout {
        home: home.clone(),
        dirs: vec![home.clone()],
        links: Vec::new(),
        seeds: vec![Seed {
            path: host(".profile"),
            content: PROFILE_SEED.to_string(),
        }],
        env: Vec::new(),
    };
    l.dirs.push(host(".cache/pub"));
    l.dirs.push(host(".cache/go-mod"));
    l.env.push((
        "PUB_CACHE".into(),
        inside(".cache/pub").display().to_string(),
    ));
    l.env.push((
        "GOMODCACHE".into(),
        inside(".cache/go-mod").display().to_string(),
    ));
    l.env.push((
        "GOCACHE".into(),
        inside(".cache/go-build").display().to_string(),
    ));
    if let Some(cache) = &cfg.go_mod_cache {
        let cache = normalize(cache)?;
        let proxy = cache.join("cache/download");
        if fs::metadata(&proxy).map(|fi| fi.is_dir()).unwrap_or(false) {
            l.env.push((
                "GOPROXY".into(),
                format!("file://{},https://proxy.golang.org,direct", proxy.display()),
            ));
        }
    }
    if let (Some(name), Some(email)) = (&cfg.git_name, &cfg.git_email) {
        l.seeds.push(Seed {
            path: host(".gitconfig"),
            content: format!(
                "[user]\n\tname = {}\n\temail = {}\n",
                quote(name),
                quote(email)
            ),
        });
    }
    l.dirs.push(auth.clone());
    match harness {
        Harness::Claude => {
            l.dirs.push(host(".claude"));
            l.seeds.push(Seed {
                path: host(".claude/.claude.json"),
                content: CLAUDE_SEED.to_string(),
            });
            l.env.push((
                "CLAUDE_CONFIG_DIR".into(),
                inside(".claude").display().to_string(),
            ));
            l.env.push((
                "CLAUDE_SECURESTORAGE_CONFIG_DIR".into(),
                auth.display().to_string(),
            ));
            l.env.push(("DISABLE_AUTOUPDATER".into(), "1".into()));
        }
        Harness::Codex => {
            l.dirs.push(host(".codex"));
            l.seeds.push(Seed {
                path: host(".codex/config.toml"),
                content: CODEX_SEED.to_string(),
            });
            l.links.push(Link {
                path: host(".codex/auth.json"),
                target: auth.join("auth.json"),
            });
            l.env
                .push(("CODEX_HOME".into(), inside(".codex").display().to_string()));
        }
        Harness::Grok => {
            l.dirs.push(host(".grok"));
            for name in ["auth.json", "auth.json.lock"] {
                l.links.push(Link {
                    path: host(".grok").join(name),
                    target: auth.join(name),
                });
            }
            l.env
                .push(("GROK_HOME".into(), inside(".grok").display().to_string()));
        }
        Harness::Pi => {
            l.dirs.push(host(".pi/sessions"));
            l.env
                .push(("PI_CODING_AGENT_DIR".into(), auth.display().to_string()));
            l.env.push((
                "PI_CODING_AGENT_SESSION_DIR".into(),
                inside(".pi/sessions").display().to_string(),
            ));
        }
    }
    Ok(l)
}

fn within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn normalize(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid(format!(
            "path must be absolute: {}",
            path.display()
        )));
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::RootDir => out.push("/"),
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Prefix(_) => {
                return Err(invalid(format!(
                    "unsupported path prefix: {}",
                    path.display()
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(invalid(format!(
            "path resolves outside the filesystem root: {}",
            path.display()
        )));
    }
    Ok(out)
}

/// Quotes a string the way the harness config files expect (Go strconv.Quote
/// for the characters that can appear in a name or email).
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(not(unix))]
fn create_dir_all(path: &Path, _mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn context(message: impl Into<String>, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{}: {}", message.into(), err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    fn temp_home() -> (tempfile::TempDir, PathBuf, Config) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let cfg = Config {
            home: root.join("state"),
            user_home: PathBuf::from("/home/user"),
            git_name: None,
            git_email: None,
            go_mod_cache: None,
        };
        (dir, root.join("state/sleeves/demo/home"), cfg)
    }

    fn seed_content(layout: &Layout, rel: &str) -> String {
        let path = layout
            .seeds()
            .iter()
            .find(|s| s.path == layout.home().join(rel))
            .expect("seed")
            .path
            .clone();
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn layout_is_inspectable_per_harness() {
        for (harness, config_dir, expected_env) in [
            (
                Harness::Claude,
                ".claude",
                vec![
                    ("CLAUDE_CONFIG_DIR", "/home/user/.claude"),
                    ("DISABLE_AUTOUPDATER", "1"),
                ],
            ),
            (
                Harness::Codex,
                ".codex",
                vec![("CODEX_HOME", "/home/user/.codex")],
            ),
            (
                Harness::Grok,
                ".grok",
                vec![("GROK_HOME", "/home/user/.grok")],
            ),
            (
                Harness::Pi,
                ".pi/sessions",
                vec![("PI_CODING_AGENT_SESSION_DIR", "/home/user/.pi/sessions")],
            ),
        ] {
            let (_dir, home, cfg) = temp_home();
            let l = layout(&cfg, harness, &home).unwrap();
            assert!(
                l.dirs().contains(&home.join(config_dir)),
                "{harness:?} config dir"
            );
            assert!(l
                .dirs()
                .contains(&cfg.home.join("auth").join(harness.as_str())));
            for (key, value) in expected_env {
                let found = l
                    .env()
                    .iter()
                    .find(|(k, _)| k == key)
                    .unwrap_or_else(|| panic!("{harness:?} {key}"));
                assert_eq!(found.1, value, "{harness:?} {key}");
            }
            let auth_env = match harness {
                Harness::Claude => Some("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
                Harness::Pi => Some("PI_CODING_AGENT_DIR"),
                _ => None,
            };
            if let Some(key) = auth_env {
                let found = l.env().iter().find(|(k, _)| k == key).unwrap();
                assert_eq!(
                    found.1,
                    cfg.home
                        .join("auth")
                        .join(harness.as_str())
                        .display()
                        .to_string()
                );
            }
            let expected_links: Vec<&str> = match harness {
                Harness::Codex => vec![".codex/auth.json"],
                Harness::Grok => vec![".grok/auth.json", ".grok/auth.json.lock"],
                _ => vec![],
            };
            assert_eq!(l.links().len(), expected_links.len(), "{harness:?} links");
            for rel in expected_links {
                let link = l.links().iter().find(|k| k.path == home.join(rel)).unwrap();
                assert_eq!(
                    link.target,
                    cfg.home
                        .join("auth")
                        .join(harness.as_str())
                        .join(Path::new(rel).file_name().unwrap())
                );
            }
        }
    }

    #[test]
    fn seeds_and_git_identity() {
        let (_dir, home, mut cfg) = temp_home();
        cfg.git_name = Some("Ann \"A\" O'Ne\n".into());
        cfg.git_email = Some("a@example.com".into());
        let l = layout(&cfg, Harness::Codex, &home).unwrap();
        l.apply().unwrap();
        assert_eq!(seed_content(&l, ".codex/config.toml"), CODEX_SEED);
        assert_eq!(seed_content(&l, ".profile"), PROFILE_SEED);
        assert_eq!(
            seed_content(&l, ".gitconfig"),
            "[user]\n\tname = \"Ann \\\"A\\\" O'Ne\\n\"\n\temail = \"a@example.com\"\n"
        );
        l.apply().unwrap();
        assert_eq!(seed_content(&l, ".codex/config.toml"), CODEX_SEED);
    }

    #[test]
    fn apply_creates_dirs_links_and_preserves_edits() {
        let (_dir, home, cfg) = temp_home();
        let l = layout(&cfg, Harness::Grok, &home).unwrap();
        l.apply().unwrap();
        for d in l.dirs() {
            assert!(fs::metadata(d).unwrap().is_dir(), "{}", d.display());
        }
        for link in l.links() {
            assert!(fs::symlink_metadata(&link.path)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read_link(&link.path).unwrap(), link.target);
        }
        fs::write(l.home().join(".grok/config.toml"), "model = \"kept\"\n").unwrap();
        l.apply().unwrap();
        assert_eq!(
            fs::read_to_string(l.home().join(".grok/config.toml")).unwrap(),
            "model = \"kept\"\n"
        );
    }

    #[test]
    fn apply_moves_file_aside_from_link() {
        let (_dir, home, cfg) = temp_home();
        let l = layout(&cfg, Harness::Codex, &home).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/auth.json"), "stale\n").unwrap();
        l.apply().unwrap();
        assert!(fs::symlink_metadata(home.join(".codex/auth.json"))
            .unwrap()
            .file_type()
            .is_symlink());
        let asides = fs::read_dir(home.join(".codex"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("orphaned-"))
            .count();
        assert_eq!(asides, 1);
    }

    #[test]
    fn apply_refuses_tampered_private_home() {
        for (harness, tampered, link_dir) in [
            (Harness::Claude, ".claude", false),
            (Harness::Codex, ".codex", true),
            (Harness::Grok, ".grok", true),
            (Harness::Pi, ".pi", true),
        ] {
            let (_dir, home, cfg) = temp_home();
            let l = layout(&cfg, harness, &home).unwrap();
            l.apply().unwrap();
            let outside = tempfile::tempdir().unwrap();
            fs::remove_dir_all(home.join(tampered)).unwrap();
            #[cfg(unix)]
            symlink(outside.path(), home.join(tampered)).unwrap();
            let err = l.apply().unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{harness:?}");
            assert!(err.to_string().contains("without symlinks"), "{harness:?}");
            if link_dir {
                assert!(fs::read_link(home.join(tampered)).is_ok());
            }
        }
    }

    #[test]
    fn apply_refuses_symlinked_seed_parent() {
        let (_dir, home, cfg) = temp_home();
        let l = layout(&cfg, Harness::Claude, &home).unwrap();
        fs::create_dir_all(&home).unwrap();
        let outside = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        symlink(outside.path().join("claude"), home.join(".claude")).unwrap();
        let err = l.apply().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!outside.path().join("claude").exists());
    }

    #[test]
    fn rejects_relative_and_escaping_homes() {
        let (_dir, home, cfg) = temp_home();
        assert_eq!(
            layout(&cfg, Harness::Pi, Path::new("relative/home"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        for escaped in [
            home.join("..").join("..").join("elsewhere"),
            cfg.home.join("sleeves/bad!name/home"),
            cfg.home.join("jobs/demo/home"),
        ] {
            assert_eq!(
                layout(&cfg, Harness::Pi, &escaped).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn go_proxy_env_only_when_cache_present() {
        let (_dir, home, mut cfg) = temp_home();
        cfg.go_mod_cache = Some(cfg.home.join("go-mod"));
        let l = layout(&cfg, Harness::Codex, &home).unwrap();
        assert!(!l.env().iter().any(|(k, _)| k == "GOPROXY"));
        fs::create_dir_all(cfg.home.join("go-mod/cache/download")).unwrap();
        let l = layout(&cfg, Harness::Codex, &home).unwrap();
        let proxy = l
            .env()
            .iter()
            .find(|(k, _)| k == "GOPROXY")
            .unwrap()
            .1
            .clone();
        assert_eq!(
            proxy,
            format!(
                "file://{},https://proxy.golang.org,direct",
                cfg.home.join("go-mod/cache/download").display()
            )
        );
    }
}
