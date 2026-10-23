//! Local vendor login. The hub reads these credentials through its narrow
//! per-harness auth mounts; no credential bytes pass through the hub API.
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
use protectorate_cli::windows_private as windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Harness {
    Claude,
    Codex,
    Grok,
    Pi,
}

impl Harness {
    const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Grok, Self::Pi];

    fn parse(name: &str) -> Option<Self> {
        match name {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "grok" => Some(Self::Grok),
            "pi" => Some(Self::Pi),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
        }
    }
    fn credential(self) -> &'static str {
        match self {
            Self::Claude => ".credentials.json",
            _ => "auth.json",
        }
    }
}

struct Request {
    home: PathBuf,
    harness: Option<Harness>,
    status: bool,
    extra: Vec<String>,
}

fn parse(args: &[String]) -> Result<Option<Request>, String> {
    let mut login_at = 0;
    let mut global_home = None;
    while let Some(arg) = args.get(login_at) {
        let (flag, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(flag, value)| (flag, Some(value)));
        match flag {
            "--home" | "--hub" | "--token-file" | "--timeout" => {
                let value = if let Some(value) = inline {
                    value.to_owned()
                } else {
                    login_at += 1;
                    match args.get(login_at) {
                        Some(value) => value.clone(),
                        None => return Ok(None),
                    }
                };
                if flag == "--home" {
                    global_home = Some(PathBuf::from(value));
                }
            }
            "--json" if inline.is_none() => {}
            _ => break,
        }
        login_at += 1;
    }
    if args.get(login_at).map(String::as_str) != Some("login") {
        return Ok(None);
    }
    // `login` by itself remains the public device pairing command.
    if args.len() == login_at + 1
        || args
            .iter()
            .skip(login_at + 1)
            .any(|arg| arg == "--name" || arg == "--platform")
    {
        return Ok(None);
    }
    let mut home = global_home;
    let mut harness = None;
    let mut status = false;
    let mut extra = Vec::new();
    let mut index = login_at + 1;
    while index < args.len() {
        match args[index].as_str() {
            "--home" => {
                index += 1;
                home = Some(PathBuf::from(args.get(index).ok_or("--home requires DIR")?));
            }
            "--status" => status = true,
            "--" => {
                extra.extend_from_slice(&args[index + 1..]);
                break;
            }
            name if Harness::parse(name).is_some() && harness.is_none() => {
                harness = Harness::parse(name)
            }
            other if other.starts_with('-') => {
                // Device login owns its flags, including --hub and --token-file.
                if harness.is_none() && !status {
                    return Ok(None);
                }
                return Err(format!("unknown harness login option {other:?}"));
            }
            other => {
                return Err(format!(
                    "unknown harness {other:?}; expected claude, codex, grok or pi"
                ))
            }
        }
        index += 1;
    }
    if harness.is_none() && !status {
        return Ok(None);
    }
    if status && harness.is_some() {
        return Err("--status takes no harness".into());
    }
    if status && !extra.is_empty() {
        return Err("--status takes no login arguments".into());
    }
    if harness == Some(Harness::Pi) && !extra.is_empty() {
        return Err("pi takes no login arguments".into());
    }
    let home = match home {
        Some(path) => path,
        None => env::var_os("PROTECTORATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(".protectorate")
            }),
    };
    if home.as_os_str().is_empty() || !home.is_absolute() {
        return Err("harness login requires an absolute hub state home; pass --home DIR".into());
    }
    Ok(Some(Request {
        home,
        harness,
        status,
        extra,
    }))
}

pub(super) fn run_if_requested(args: &[String]) -> Option<Result<(), String>> {
    let request = match parse(args) {
        Ok(request) => request,
        Err(error) => return Some(Err(error)),
    }?;
    Some(run(request))
}

fn run(request: Request) -> Result<(), String> {
    if request.status {
        for harness in Harness::ALL {
            let state = if logged_in(&request.home, harness)? {
                "logged in"
            } else {
                "missing"
            };
            crate::write_stdout(&format!("{:<7} {state}\n", harness.name()))?;
        }
        return Ok(());
    }
    let harness = request.harness.expect("parser requires harness or status");
    let auth = request.home.join("auth").join(harness.name());
    private_dir(&request.home)?;
    private_dir(&request.home.join("auth"))?;
    private_dir(&auth)?;
    let mut argv: Vec<&str> = match harness {
        Harness::Claude => vec!["claude", "auth", "login"],
        Harness::Codex => vec!["codex", "login"],
        Harness::Grok => vec!["grok", "login"],
        Harness::Pi => vec!["pi"],
    };
    if harness == Harness::Pi {
        eprintln!("pi has no login command: type /login in its prompt, finish, then exit pi");
    }
    let mut cmd = Command::new(argv.remove(0));
    cmd.args(argv).args(&request.extra);
    configure_env(&mut cmd, &request.home, harness, &auth)?;
    let result = cmd.status().map_err(|error| {
        format!(
            "{} is not installed or could not start: {error}",
            harness.name()
        )
    })?;
    if !result.success() {
        return Err(format!("{} login exited with {result}", harness.name()));
    }
    if !logged_in(&request.home, harness)? {
        crate::write_stdout(&format!(
            "{}: not ready, no credential in the shared auth dir\n",
            harness.name()
        ))?;
        return Ok(());
    }
    let status_args: Option<&[&str]> = match harness {
        Harness::Claude => Some(&["auth", "status", "--text"]),
        Harness::Codex => Some(&["login", "status"]),
        _ => None,
    };
    if let Some(args) = status_args {
        let mut status = Command::new(harness.name());
        status
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        configure_env(&mut status, &request.home, harness, &auth)?;
        if !status.status().is_ok_and(|exit| exit.success()) {
            crate::write_stdout(&format!(
                "{}: credential present but its status command failed\n",
                harness.name()
            ))?;
            return Ok(());
        }
    }
    crate::write_stdout(&format!(
        "{}: ready, sleeves will use the shared credential\n",
        harness.name()
    ))?;
    Ok(())
}

fn configure_env(
    cmd: &mut Command,
    home: &Path,
    harness: Harness,
    auth: &Path,
) -> Result<(), String> {
    match harness {
        Harness::Claude => {
            let scratch = home.join("login/claude");
            private_dir(&home.join("login"))?;
            private_dir(&scratch)?;
            cmd.env("CLAUDE_CONFIG_DIR", scratch)
                .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", auth);
        }
        Harness::Codex => {
            cmd.env("CODEX_HOME", auth);
        }
        Harness::Grok => {
            cmd.env("GROK_HOME", auth);
        }
        Harness::Pi => {
            cmd.env("PI_CODING_AGENT_DIR", auth);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn private_dir(path: &Path) -> Result<(), String> {
    windows::ensure_private_path(path).map_err(|error| error.message)
}

#[cfg(not(windows))]
fn private_dir(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err(format!("{} must be a real directory", path.display()))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(path)
                    .map_err(|e| format!("create {}: {e}", path.display()))?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        }
        Err(error) => return Err(format!("inspect {}: {error}", path.display())),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(format!(
                "{} must have private directory permissions",
                path.display()
            ));
        }
    }
    Ok(())
}

fn logged_in(home: &Path, harness: Harness) -> Result<bool, String> {
    let path = home
        .join("auth")
        .join(harness.name())
        .join(harness.credential());
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            if !meta.is_file() || meta.len() == 0 {
                return Ok(false);
            }
            #[cfg(windows)]
            {
                for directory in [
                    home.to_path_buf(),
                    home.join("auth"),
                    home.join("auth").join(harness.name()),
                ] {
                    windows::check_private_path(&directory).map_err(|error| error.message)?;
                }
                windows::check_private_file(&path).map_err(|error| error.message)?;
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("check {} credential: {error}", harness.name())),
    }
}
