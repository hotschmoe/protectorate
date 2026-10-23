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
