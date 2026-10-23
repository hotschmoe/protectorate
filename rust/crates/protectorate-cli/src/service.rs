//! Public local service definition generator. It prints by default and never
//! enables or starts a service.
use protectorate_svcgen::{self as svcgen, Spec, ROLE_HUB, ROLE_NODE, TRIGGER_LOGON};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Systemd,
    WindowsTask,
    InstallWindows,
    Termux,
}

#[derive(Debug)]
struct Request {
    kind: Kind,
    spec: Spec,
    write: Option<PathBuf>,
    install: bool,
    trigger: String,
}

pub(super) fn run_if_requested(args: &[String]) -> Option<Result<(), String>> {
    let at = super::local_root_at(args, "service")?;
    let leading = super::local_alias_leading(args, at, "service", &[("--home", true)])?;
    Some(leading.and_then(|leading| {
        let action = args.get(at + 1).ok_or_else(usage)?;
        let mut options = leading;
        options.extend_from_slice(&args[at + 2..]);
        let request = parse(action, &options)?;
        run(request)
    }))
}

fn usage() -> String {
    "service requires systemd <hub|node>, windows-task, install-windows, or termux".into()
}

fn parse(action: &str, args: &[String]) -> Result<Request, String> {
    let kind = match action {
        "systemd" => Kind::Systemd,
        "windows-task" => Kind::WindowsTask,
        "install-windows" => Kind::InstallWindows,
        "termux" => Kind::Termux,
        _ => return Err(usage()),
    };
    let mut spec = Spec::default();
    let mut write = None;
    let mut install = false;
    let mut trigger = TRIGGER_LOGON.to_owned();
    let mut role = None;
    let mut index = 0;
    while index < args.len() {
        let token = &args[index];
        if token == "--" {
            spec.args.extend_from_slice(&args[index + 1..]);
            break;
        }
        let (flag, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(name, value)| (name, Some(value)));
        let mut value = |flag: &str| -> Result<String, String> {
            if let Some(inline) = inline {
                return Ok(inline.to_owned());
            }
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--binary" => spec.binary = value(flag)?,
            "--home" => spec.home = value(flag)?,
            "--write" => write = Some(PathBuf::from(value(flag)?)),
            "--install" if inline.is_none() => install = true,
            "--user-name" if kind == Kind::Systemd => spec.user = value(flag)?,
            "--path" if kind == Kind::Systemd => spec.path = value(flag)?,
            "--tmux-socket" if kind == Kind::Systemd => spec.tmux_socket = value(flag)?,
            "--trigger" if kind == Kind::WindowsTask => trigger = value(flag)?,
            "--elevated" if kind == Kind::WindowsTask && inline.is_none() => spec.elevated = true,
            "--win-user" if kind == Kind::WindowsTask => spec.win_user = value(flag)?,
            _ if kind == Kind::Systemd && role.is_none() && !token.starts_with('-') => {
                role = Some(token.clone());
            }
            _ => {
                return Err(format!(
                    "unknown service {action} argument {token:?}; put serve flags after --"
                ))
            }
        }
        index += 1;
    }
    spec.role = match kind {
        Kind::Systemd => {
            let role = role.ok_or("service systemd requires hub or node")?;
            if !matches!(role.as_str(), ROLE_HUB | ROLE_NODE) {
                return Err(format!(
                    "service systemd role must be hub or node, got {role:?}"
                ));
            }
            role
        }
        _ => ROLE_NODE.to_owned(),
    };
    if write.is_some() && install {
        return Err("--write and --install are mutually exclusive".into());
    }
    if install && !matches!(kind, Kind::Systemd | Kind::Termux) {
        return Err("--install is supported only for systemd and termux; use --write here".into());
    }
    if spec.home.is_empty() {
        spec.home = default_home()?;
    }
    if spec.binary.is_empty() {
        spec.binary = default_companion_binary()?;
    }
    if spec.tmux_socket.is_empty() && kind == Kind::Systemd {
        spec.tmux_socket = env::var("PROTECTORATE_TMUX_SOCKET").unwrap_or_default();
    }
    Ok(Request {
        kind,
        spec,
        write,
        install,
        trigger,
    })
}

fn default_home() -> Result<String, String> {
    let path = env::var_os("PROTECTORATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                env::var_os("LOCALAPPDATA")
                    .map(|local| PathBuf::from(local).join("Protectorate-Node"))
                    .or_else(|| {
                        env::var_os("USERPROFILE")
                            .map(|home| PathBuf::from(home).join(".protectorate"))
                    })
            } else {
                env::var_os("HOME").map(|home| PathBuf::from(home).join(".protectorate"))
            }
        })
        .ok_or("state home is unset; pass --home")?;
    path.into_os_string()
        .into_string()
        .map_err(|_| "state directory path is not UTF-8; pass --home".into())
}

fn default_companion_binary() -> Result<String, String> {
    let executable =
        env::current_exe().map_err(|error| format!("locate CLI executable: {error}"))?;
    let binary = executable
        .parent()
        .ok_or("CLI has no parent directory")?
        .join(super::companion_name());
    if !binary.is_file() {
        return Err(format!(
            "{} is missing; install the matching Rust companion or pass --binary",
            binary.display()
        ));
    }
    binary
        .into_os_string()
        .into_string()
        .map_err(|_| "companion path is not UTF-8; pass --binary".into())
}

fn run(request: Request) -> Result<(), String> {
    let Request {
        kind,
        spec,
        write,
        install,
        trigger,
    } = request;
    match kind {
        Kind::Systemd => {
            let content = svcgen::systemd(&spec).map_err(|error| error.to_string())?;
            let destination = if install {
                Some(if spec.user.is_empty() {
                    env::var_os("HOME")
                        .map(PathBuf::from)
                        .ok_or("HOME is unset; use --write")?
                        .join(".config/systemd/user")
                } else {
                    PathBuf::from("/etc/systemd/system")
                })
            } else {
                write
            };
            let name = format!("{}.service", spec.name());
            let notes_path = if let Some(dir) = destination {
                let path = write_new(&dir, &name, content.as_bytes())?;
                crate::write_stdout(&format!("wrote {}\n", path.display()))?;
                if install {
                    path.to_string_lossy().into_owned()
                } else {
                    name
                }
            } else {
                crate::write_stdout(&content)?;
                name
            };
            eprintln!("\n{}", svcgen::systemd_notes(&spec, &notes_path));
        }
        Kind::WindowsTask => {
            let content = svcgen::task_xml(&spec, &trigger).map_err(|error| error.to_string())?;
            let name = format!("{}.xml", spec.name());
            let notes_path = if let Some(dir) = write {
                let path = write_new(&dir, &name, &svcgen::encode_utf16le(&content))?;
                crate::write_stdout(&format!("wrote {}\n", path.display()))?;
                path.to_string_lossy().into_owned()
            } else {
                crate::write_stdout(&content)?;
                name
            };
            eprintln!("\n{}", svcgen::task_notes(&spec, &trigger, &notes_path));
        }
        Kind::InstallWindows => {
            let content =
                svcgen::windows_service(&spec, &spec.home).map_err(|error| error.to_string())?;
            if let Some(dir) = write {
                let path = write_new(&dir, "protectorate-node-service.cmd", content.as_bytes())?;
                crate::write_stdout(&format!("wrote {}\n", path.display()))?;
            } else {
                crate::write_stdout(&content)?;
            }
        }
        Kind::Termux => run_termux(&spec, write, install)?,
    }
    Ok(())
}

/// The termux-services (runit) run script plus the Termux:Boot script. Only
/// --install writes into a validated Termux directory layout.
fn run_termux(spec: &Spec, write: Option<PathBuf>, install: bool) -> Result<(), String> {
    let run = svcgen::termux_run(spec).map_err(|error| error.to_string())?;
    let boot = svcgen::termux_boot();
    let name = spec.name();
    if install {
        if !cfg!(unix) {
            return Err(
                "service termux --install writes Termux scripts and needs a Unix host".into(),
            );
        }
        let roots = termux_roots()?;
        let run_dir = roots.prefix.join("var").join("service").join(&name);
        let boot_dir = roots.home.join(".termux").join("boot");
        check_new_script(&roots.prefix, &run_dir, "run")?;
        check_new_script(&roots.home, &boot_dir, &name)?;
        write_exec(&roots.prefix, &run_dir, "run", run.as_bytes())?;
        write_exec(&roots.home, &boot_dir, &name, boot.as_bytes())?;
    } else if let Some(dir) = write {
        check_new_script(&dir, &dir.join(&name), "run")?;
        check_new_script(&dir, &dir.join("boot"), &name)?;
        write_exec(&dir, &dir.join(&name), "run", run.as_bytes())?;
        write_exec(&dir, &dir.join("boot"), &name, boot.as_bytes())?;
    } else {
        crate::write_stdout(&format!(
            "# $PREFIX/var/service/{name}/run\n{run}\n# ~/.termux/boot/{name}\n{boot}"
        ))?;
    }
    eprintln!("\n{}", svcgen::termux_notes(spec));
    Ok(())
}

/// The live Termux layout the generated scripts belong to.
#[derive(Debug)]
struct TermuxRoots {
    prefix: PathBuf,
    home: PathBuf,
}

fn termux_roots() -> Result<TermuxRoots, String> {
    termux_roots_from(
        env::var_os("PREFIX").as_deref(),
        env::var_os("HOME").as_deref(),
    )
}

/// Genuine Termux keeps its prefix at .../com.termux/files/usr with the home
/// beside it, and every generated script starts on $PREFIX/bin/sh. Anything
/// else is a plain --write target, never a live install.
fn termux_roots_from(prefix: Option<&OsStr>, home: Option<&OsStr>) -> Result<TermuxRoots, String> {
    let prefix = termux_path(prefix.map(PathBuf::from), "PREFIX")?;
    let home = termux_path(home.map(PathBuf::from), "HOME")?;
    let parts = prefix
        .components()
        .filter_map(|part| match part {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.len() < 3
        || parts[parts.len() - 3..]
            != [
                OsStr::new("com.termux"),
                OsStr::new("files"),
                OsStr::new("usr"),
            ]
    {
        return Err(format!(
            "service termux --install needs PREFIX to end in com.termux/files/usr, got {}",
            prefix.display()
        ));
    }
    let expected_home = prefix
        .parent()
        .map(|files| files.join("home"))
        .ok_or("PREFIX has no Termux home; use --write")?;
    if home != expected_home {
        return Err(format!(
            "service termux --install needs HOME to be {}, the home beside PREFIX {}",
            expected_home.display(),
            prefix.display()
        ));
    }
    let shell = prefix.join("bin").join("sh");
    if !fs::metadata(&shell).is_ok_and(|meta| meta.is_file()) {
        return Err(format!(
            "{} is missing, so this is not a Termux environment; use --write",
            shell.display()
        ));
    }
    Ok(TermuxRoots { prefix, home })
}

fn termux_path(path: Option<PathBuf>, label: &str) -> Result<PathBuf, String> {
    let path =
        path.ok_or_else(|| format!("service termux --install needs {label}; use --write"))?;
    if !path.is_absolute() {
        return Err(format!(
            "service termux --install needs an absolute {label}; use --write"
        ));
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::RootDir => clean.push("/"),
            Component::Normal(part) => clean.push(part),
            _ => {
                return Err(format!(
                    "service termux --install rejects . or .. in {label}; use --write"
                ))
            }
        }
    }
    Ok(clean)
}

/// Write one Termux script below `base` and make it executable. Every
/// directory from `base` down to the target must be a real directory, never a
/// symlink, and an existing file is never overwritten.
fn write_exec(base: &Path, dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    check_new_script(base, dir, name)?;
    let path = write_new(dir, name, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("chmod {}: {error}", path.display()))?;
    }
    crate::write_stdout(&format!("wrote {}\n", path.display()))?;
    Ok(path)
}

fn check_new_script(base: &Path, dir: &Path, name: &str) -> Result<(), String> {
    guard_below(base, dir)?;
    let target = dir.join(name);
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "{} is a symlink; refusing to write through it",
            target.display()
        )),
        Ok(_) => Err(format!(
            "{} exists; existing definitions are never overwritten",
            target.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {}: {error}", target.display())),
    }
}

/// Check `dir` and every step below `base`. A missing step is fine: it gets
/// created for the script. A symlink or a file in its place is not.
fn guard_below(base: &Path, dir: &Path) -> Result<(), String> {
    let rest = dir
        .strip_prefix(base)
        .map_err(|_| format!("{} does not sit inside {}", dir.display(), base.display()))?;
    let mut current = base.to_path_buf();
    guard_dir(&current)?;
    for part in rest.components() {
        current.push(part);
        guard_dir(&current)?;
    }
    Ok(())
}

fn guard_dir(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "{} is a symlink; refusing to write through it",
            path.display()
        )),
        Ok(meta) if !meta.is_dir() => {
            Err(format!("{} exists and is not a directory", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {}: {error}", path.display())),
    }
}

fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    let path = dir.join(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            format!(
                "{}: {error}; existing definitions are never overwritten",
                path.display()
            )
        })?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn termux_options_keep_role_node_and_reject_mixing_targets() {
        let request = parse(
            "termux",
            &argv(&[
                "--binary",
                "/data/data/com.termux/files/usr/bin/protectorate-rs",
                "--home",
                "/data/data/com.termux/files/home/.protectorate",
                "--",
                "--listen",
                "127.0.0.1:7475",
            ]),
        )
        .unwrap();
        assert_eq!(request.kind, Kind::Termux);
        assert_eq!(request.spec.role, ROLE_NODE);
        assert_eq!(request.spec.args, argv(&["--listen", "127.0.0.1:7475"]));

        for (label, action, args, expected) in [
            (
                "write plus install",
                "termux",
                &["--write", "/tmp/out", "--install"][..],
                "mutually exclusive",
            ),
            (
                "install on windows-task",
                "windows-task",
                &["--install"][..],
                "--install is supported only for systemd and termux",
            ),
            (
                "install on install-windows",
                "install-windows",
                &["--install"][..],
                "--install is supported only for systemd and termux",
            ),
            (
                "positional role",
                "termux",
                &["hub"][..],
                "unknown service termux argument",
            ),
        ] {
            let error = parse(action, &argv(args)).unwrap_err();
            assert!(error.contains(expected), "{label}: {error}");
        }
    }

    #[test]
    fn termux_install_roots_are_guarded() {
        let root = tempfile::tempdir().unwrap();
        let prefix = root.path().join("com.termux/files/usr");
        let home = root.path().join("com.termux/files/home");
        let foreign_prefix = root.path().join("other/com.termux/files/usr");
        fs::create_dir_all(prefix.join("bin")).unwrap();
        fs::write(prefix.join("bin/sh"), "#!/bin/sh\n").unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(foreign_prefix.join("bin")).unwrap();

        let cases = [
            ("missing PREFIX", None, Some(home.clone()), "needs PREFIX"),
            ("missing HOME", Some(prefix.clone()), None, "needs HOME"),
            (
                "relative PREFIX",
                Some(PathBuf::from("com.termux/files/usr")),
                Some(home.clone()),
                "absolute PREFIX",
            ),
            (
                "traversal in HOME",
                Some(prefix.clone()),
                Some(root.path().join("com.termux/files/home/../home")),
                "rejects . or .. in HOME",
            ),
            (
                "foreign PREFIX",
                Some(root.path().join("usr")),
                Some(home.clone()),
                "com.termux/files/usr",
            ),
            (
                "foreign HOME",
                Some(prefix.clone()),
                Some(root.path().join("home")),
                "the home beside PREFIX",
            ),
            (
                "no Termux shell",
                Some(foreign_prefix.clone()),
                Some(root.path().join("other/com.termux/files/home")),
                "is missing, so this is not a Termux environment",
            ),
        ];
        for (label, prefix, home, expected) in cases {
            let error = termux_roots_from(
                prefix.as_deref().map(Path::as_os_str),
                home.as_deref().map(Path::as_os_str),
            )
            .unwrap_err();
            assert!(error.contains(expected), "{label}: {error}");
        }

        let roots = termux_roots_from(Some(prefix.as_os_str()), Some(home.as_os_str())).unwrap();
        assert_eq!(roots.prefix, prefix);
        assert_eq!(roots.home, home);
    }

    #[cfg(unix)]
    #[test]
    fn termux_scripts_never_follow_a_symlink_or_replace_a_file() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().to_path_buf();
        let real = base.join("real");
        fs::create_dir_all(real.join("protectorate-node")).unwrap();
        std::os::unix::fs::symlink(&real, real.join("link")).unwrap();
        std::os::unix::fs::symlink(
            base.join("missing"),
            real.join("protectorate-node/dangling"),
        )
        .unwrap();

        for (label, dir, name) in [
            (
                "symlinked directory",
                real.join("link/protectorate-node"),
                "run",
            ),
            (
                "symlinked target",
                real.join("protectorate-node"),
                "dangling",
            ),
        ] {
            let error = write_exec(&real, &dir, name, b"#!/bin/sh\nexec true\n").unwrap_err();
            assert!(
                error.contains("is a symlink; refusing to write through it"),
                "{label}: {error}"
            );
        }

        let path = write_exec(
            &real,
            &real.join("protectorate-node"),
            "run",
            b"#!/bin/sh\nexec true\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );

        let error = write_exec(
            &real,
            &real.join("protectorate-node"),
            "run",
            b"#!/bin/sh\nexec changed\n",
        )
        .unwrap_err();
        assert!(error.contains("never overwritten"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), b"#!/bin/sh\nexec true\n");
    }
}
