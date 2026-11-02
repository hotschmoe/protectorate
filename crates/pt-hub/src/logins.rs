//! The hub's own harness logins, `<home>/logins/`, laid out like a home so the quota readers and
//! doctor find each file where the harness keeps it. `protectorate harness login` fills it with
//! the vendor's own login command. Every sleeve shares every harness's one login, so a sleeve on
//! any harness can run the others' CLIs: each directory is bound read-write into the sandbox and
//! the sleeve's home points at it, so a token refresh by one sleeve reaches all of them. A copy
//! per sleeve would fork the refresh-token chain, and the first refresh would log every other
//! copy out. The owner's own logins are never used.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use pt_core::Harness;

use crate::home::{Home, random_hex};

/// One harness's share of the store.
struct Layout {
    /// The directory bound read-write into its sandboxes, home-relative.
    dir: &'static str,
    /// Home-relative files a sleeve's home links to the store.
    links: &'static [&'static str],
    /// Home-relative files copied from the store into a sleeve's home when missing: settings the
    /// harness writes per sleeve.
    seeds: &'static [&'static str],
}

fn layout(h: Harness) -> Option<Layout> {
    Some(match h {
        // Claude reads and refreshes `.credentials.json` in CLAUDE_SECURESTORAGE_CONFIG_DIR.
        Harness::Claude => Layout {
            dir: ".claude",
            links: &[],
            seeds: &[],
        },
        Harness::Codex => Layout {
            dir: ".codex",
            links: &[".codex/auth.json"],
            seeds: &[".codex/config.toml"],
        },
        Harness::Grok => Layout {
            dir: ".grok",
            links: &[".grok/auth.json", ".grok/auth.json.lock"],
            seeds: &[],
        },
        Harness::PiVendor | Harness::PiLocal => Layout {
            dir: ".pi/agent",
            links: &[".pi/agent/auth.json", ".pi/agent/models.json"],
            seeds: &[".pi/agent/settings.json"],
        },
        Harness::Antigravity => return None,
    })
}

fn private_dir(p: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)?;
    fs::set_permissions(p, fs::Permissions::from_mode(0o700))
}

/// What a launch adds to its sandbox.
#[derive(Default)]
pub struct Shared {
    /// The store directories, bound read-write.
    pub dirs: Vec<PathBuf>,
    pub env: Vec<(String, String)>,
}

/// One harness per distinct store layout; the Pi kinds share one.
const LOGIN_HARNESSES: [Harness; 4] = [
    Harness::Claude,
    Harness::Codex,
    Harness::Grok,
    Harness::PiVendor,
];

/// Prepares sleeve home `home` with every harness's login, so a sleeve on any harness can run
/// the others' CLIs.
pub fn prepare_all(store: &Path, home: &Path) -> io::Result<Shared> {
    let mut all = Shared::default();
    for h in LOGIN_HARNESSES {
        if let Some(one) = prepare(store, home, h)? {
            all.dirs.push(one.dir);
            all.env.extend(one.env);
        }
    }
    Ok(all)
}

struct One {
    dir: PathBuf,
    env: Vec<(String, String)>,
}

/// Links `h`'s login files in `home` to the store and seeds the settings it lacks. None for a
/// harness with no login here.
fn prepare(store: &Path, home: &Path, h: Harness) -> io::Result<Option<One>> {
    let Some(l) = layout(h) else {
        return Ok(None);
    };
    let ctx = |p: &Path, e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", p.display()));
    let dir = store.join(l.dir);
    private_dir(&dir).map_err(|e| ctx(&dir, e))?;
    for rel in l.links {
        let (target, link) = (store.join(rel), home.join(rel));
        if fs::read_link(&link).is_ok_and(|t| t == target) {
            continue;
        }
        let parent = link.parent().unwrap_or(home);
        private_dir(parent).map_err(|e| ctx(parent, e))?;
        let tmp = parent.join(format!(".pt-link.{}", random_hex(8)?));
        std::os::unix::fs::symlink(&target, &tmp).map_err(|e| ctx(&tmp, e))?;
        fs::rename(&tmp, &link).map_err(|e| ctx(&link, e))?;
    }
    for rel in l.seeds {
        let (src, dst) = (store.join(rel), home.join(rel));
        if dst.symlink_metadata().is_ok() || !src.is_file() {
            continue;
        }
        let parent = dst.parent().unwrap_or(home);
        private_dir(parent).map_err(|e| ctx(parent, e))?;
        fs::copy(&src, &dst).map_err(|e| ctx(&dst, e))?;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o600)).map_err(|e| ctx(&dst, e))?;
    }
    let env = match h {
        Harness::Claude => vec![(
            "CLAUDE_SECURESTORAGE_CONFIG_DIR".to_string(),
            dir.to_string_lossy().into_owned(),
        )],
        _ => Vec::new(),
    };
    Ok(Some(One { dir, env }))
}

/// `protectorate harness login <harness> [ARGS...]`: runs the vendor's own login into the store,
/// on this terminal. Pi's login is an API key in its own config, so it is copied from the owner's
/// `~/.pi/agent` instead; an OAuth entry there is refused, since a copy would fork it.
pub fn login(root: &Path, harness: &str, args: &[String]) -> Result<String, String> {
    let home = Home::prepare(root).map_err(|e| e.to_string())?;
    let store = home.logins();
    private_dir(&store).map_err(|e| format!("{}: {e}", store.display()))?;
    let (program, base, env): (&str, &[&str], Vec<(&str, PathBuf)>) = match harness {
        "claude" => {
            // The scratch config keeps the login's account record out of the owner's ~/.claude.json.
            let scratch = store.join(".claude-login");
            private_dir(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
            (
                "claude",
                &["auth", "login"],
                vec![
                    ("CLAUDE_CONFIG_DIR", scratch),
                    ("CLAUDE_SECURESTORAGE_CONFIG_DIR", store.join(".claude")),
                ],
            )
        }
        "codex" => (
            "codex",
            &["login"],
            vec![("CODEX_HOME", store.join(".codex"))],
        ),
        "grok" => ("grok", &["login"], vec![("GROK_HOME", store.join(".grok"))]),
        "pi" => {
            if !args.is_empty() {
                return Err("harness login pi takes no arguments".into());
            }
            return import_pi(&store);
        }
        other => {
            return Err(format!(
                "unknown harness {other:?}: claude, codex, grok or pi"
            ));
        }
    };
    for (_, dir) in &env {
        private_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let status = Command::new(program)
        .args(base)
        .args(args)
        .envs(env.iter().map(|(k, v)| (*k, v.as_os_str())))
        .status()
        .map_err(|e| format!("run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} {} exited with {status}", base.join(" ")));
    }
    Ok(format!(
        "{harness}: logged in for the hub's sleeves in {}; `protectorate doctor --refresh` rechecks",
        store.display()
    ))
}

fn import_pi(store: &Path) -> Result<String, String> {
    let owner = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(".pi/agent");
    let auth = owner.join("auth.json");
    if let Ok(text) = fs::read_to_string(&auth) {
        let v = pt_json::parse(&text).map_err(|e| format!("{}: {e}", auth.display()))?;
        for (name, entry) in v.as_object().map(|m| m.iter()).into_iter().flatten() {
            let kind = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if kind != "api_key" {
                return Err(format!(
                    "{}: {name} is a {kind:?} login, and a copy would fork it; log in for the hub \
                     with `PI_CODING_AGENT_DIR={} pi` and /login",
                    auth.display(),
                    store.join(".pi/agent").display()
                ));
            }
        }
    }
    let dir = store.join(".pi/agent");
    private_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut copied = Vec::new();
    for f in ["auth.json", "models.json", "settings.json"] {
        let src = owner.join(f);
        if !src.is_file() {
            continue;
        }
        let dst = dir.join(f);
        fs::copy(&src, &dst)
            .map_err(|e| format!("copy {} to {}: {e}", src.display(), dst.display()))?;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", dst.display()))?;
        copied.push(f);
    }
    if copied.is_empty() {
        return Err(format!(
            "{} has no auth.json, models.json or settings.json",
            owner.display()
        ));
    }
    Ok(format!(
        "pi: copied {} from {} into {}; `protectorate doctor --refresh` rechecks",
        copied.join(", "),
        owner.display(),
        dir.display()
    ))
}
