//! Install a Rust node into a private per-user release directory.
#[cfg(target_os = "linux")]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::{
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::{SystemTime, UNIX_EPOCH},
};

const UNIT: &str = "protectorate-node.service";
const MARKER: &str = "# Managed by Protectorate Rust node install\n";
const RUST_SCRIPT_MARKER: &str = "# Managed by Protectorate Rust installer\n";
const COMPONENTS: &[&str] = &["protectorate-rs", "protectorate-node-sandbox-helper"];

pub(crate) struct Command {
    home: PathBuf,
    hub: String,
    name: Option<String>,
    code: Option<String>,
    code_file: Option<PathBuf>,
    ca_cert: PathBuf,
    listen: Option<String>,
    allow_plain_jobs: bool,
    dry_run: bool,
    no_start: bool,
    serve_args: Vec<String>,
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut hub = None;
    let mut name = None;
    let mut code = None;
    let mut code_file = None;
    let mut ca_cert = None;
    let mut listen = None;
    let mut allow_plain_jobs = false;
    let mut dry_run = false;
    let mut no_start = false;
    let mut serve_args = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--" => {
                serve_args.extend(args);
                break;
            }
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--hub" => hub = Some(args.next().ok_or("--hub requires URL")?),
            "--name" => name = Some(args.next().ok_or("--name requires NAME")?),
            "--code" => code = Some(args.next().ok_or("--code requires CODE")?),
            "--code-file" => {
                code_file = Some(PathBuf::from(
                    args.next().ok_or("--code-file requires FILE")?,
                ))
            }
            "--ca-cert" => {
                ca_cert = Some(PathBuf::from(args.next().ok_or("--ca-cert requires FILE")?))
            }
            "--listen" => listen = Some(args.next().ok_or("--listen requires IP:PORT")?),
            "--allow-plain-jobs" => allow_plain_jobs = true,
            "--dry-run" => dry_run = true,
            "--no-start" => no_start = true,
            _ => return Err(format!("unknown node install argument {flag:?}")),
        }
    }
    if code.is_some() && code_file.is_some() {
        return Err("--code and --code-file are exclusive".into());
    }
    for forbidden in ["--home", "--sandbox-helper", "--listen"] {
        if serve_args
            .iter()
            .any(|arg| arg == forbidden || arg.starts_with(&format!("{forbidden}=")))
        {
            return Err(format!(
                "{forbidden} is managed by node install; use the install flag where available"
            ));
        }
    }
    let hub = hub.ok_or("node install requires --hub URL")?;
    if !hub.starts_with("https://") {
        return Err("--hub requires the hub's HTTPS URL".into());
    }
    if serve_args.iter().any(|arg| arg.contains(['\n', '\r'])) {
        return Err("node serve arguments cannot contain newlines".into());
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        hub: hub.trim_end_matches('/').to_owned(),
        name,
        code,
        code_file,
        ca_cert: ca_cert.ok_or("node install requires --ca-cert FILE (the hub CA)")?,
        listen,
        allow_plain_jobs,
        dry_run,
        no_start,
        serve_args,
    })
}

fn regular(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not a nonempty regular file: {}", path.display()),
        ));
    }
    Ok(())
}

fn optional_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected path type: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// A node already joined to this hub under this name is not joined again.
fn existing_join(home: &Path, hub: &str, name: Option<&str>) -> io::Result<bool> {
    let joined = match super::node_join::Joined::load(home) {
        Ok(joined) => joined,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if joined.hub != hub || name.is_some_and(|name| name != joined.name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} already joined {} as {}; uninstall first to join another hub or name",
                home.display(),
                joined.hub,
                joined.name
            ),
        ));
    }
    Ok(true)
}

fn run_systemctl(systemctl: &Path, args: &[&str]) -> io::Result<()> {
    let output = ProcessCommand::new(systemctl)
        .arg("--user")
        .args(args)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "systemctl --user {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn service_status(systemctl: &Path, args: &[&str]) -> io::Result<bool> {
    Ok(ProcessCommand::new(systemctl)
        .arg("--user")
        .args(args)
        .status()?
        .success())
}

fn join_args(command: &Command, home: &Path) -> Vec<String> {
    let mut args = vec![
        "--home".into(),
        home.display().to_string(),
        "--hub".into(),
        command.hub.clone(),
        "--ca-cert".into(),
        command.ca_cert.display().to_string(),
    ];
    if let Some(name) = &command.name {
        args.extend(["--name".into(), name.clone()]);
    }
    if let Some(path) = &command.code_file {
        args.extend(["--code-file".into(), path.display().to_string()]);
    }
    if let Some(code) = &command.code {
        args.extend(["--code".into(), code.clone()]);
    }
    if let Some(listen) = &command.listen {
        args.extend(["--listen".into(), listen.clone()]);
    }
    args
}

#[cfg(target_os = "linux")]
pub(crate) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    let executable = fs::canonicalize(env::current_exe()?)?;
    let user_home = PathBuf::from(env::var_os("HOME").ok_or("HOME is unset")?);
    run_with(command, &executable, &user_home, Path::new("systemctl")).await
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run(_command: Command) -> Result<(), Box<dyn Error>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "node install requires Linux").into())
}

#[cfg(target_os = "linux")]
async fn run_with(
    command: Command,
    executable: &Path,
    user_home: &Path,
    systemctl: &Path,
) -> Result<(), Box<dyn Error>> {
    let user_home = fs::canonicalize(user_home)?;
    let home = std::path::absolute(&command.home)?;
    let source_dir = executable.parent().ok_or("executable has no parent")?;
    let release_root = user_home.join(".local/share/protectorate/node-install");
    let unit = user_home.join(".config/systemd/user").join(UNIT);
    for path in [&home, &user_home, source_dir] {
        if path.to_string_lossy().contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "service paths cannot contain newlines",
            )
            .into());
        }
    }
    for name in COMPONENTS {
        regular(&source_dir.join(name))?;
    }
    for path in [
        Some(command.ca_cert.as_path()),
        command.code_file.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        regular(path)?;
    }
    let old_unit = if optional_file(&unit)? {
        let bytes = fs::read(&unit)?;
        if !bytes.starts_with(MARKER.as_bytes())
            && !bytes.starts_with(RUST_SCRIPT_MARKER.as_bytes())
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("unmanaged service unit: {}", unit.display()),
            )
            .into());
        }
        Some(bytes)
    } else {
        None
    };
    let current = release_root.join("current");
    let old_pointer = match fs::read_link(&current) {
        Ok(path)
            if path.components().count() == 1
                && path.to_str().is_some_and(|s| s.starts_with("release-")) =>
        {
            Some(path)
        }
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "node release pointer has an unsafe target",
            )
            .into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let already_joined = existing_join(&home, &command.hub, command.name.as_deref())?;
    let mut args = vec!["--home".into(), home.display().to_string()];
    if command.allow_plain_jobs {
        args.push("--allow-plain-jobs".into());
    }
    args.extend(command.serve_args.iter().cloned());
    let parsed = crate::parse(
        std::iter::once("node".to_owned())
            .chain(std::iter::once("serve".to_owned()))
            .chain(args.iter().cloned()),
    )
    .map_err(io::Error::other)?;
    if !matches!(parsed, Some(crate::Command::Node(_))) {
        return Err("invalid node serve arguments".into());
    }
    let spec = protectorate_svcgen::Spec {
        role: protectorate_svcgen::ROLE_NODE.into(),
        binary: current.join("protectorate-rs").display().to_string(),
        args,
        home: home.display().to_string(),
        path: format!(
            "{}:/usr/local/bin:/usr/bin:/bin",
            user_home.join(".local/bin").display()
        ),
        ..Default::default()
    };
    let unit_text = format!("{MARKER}{}", protectorate_svcgen::systemd(&spec)?);
    println!(
        "install Rust node from {} into {}",
        source_dir.display(),
        release_root.display()
    );
    println!(
        "enrollment: {}",
        if already_joined {
            "keep existing"
        } else {
            "join the hub"
        }
    );
    println!("service unit: {}", unit.display());
    if command.dry_run {
        println!("dry run: no files or services changed");
        return Ok(());
    }
    if !already_joined
        && command.code.is_none()
        && command.code_file.is_none()
        && env::var_os("PROTECTORATE_JOIN_CODE").is_none()
    {
        return Err("joining requires --code, --code-file or PROTECTORATE_JOIN_CODE".into());
    }
    run_systemctl(systemctl, &["show-environment"])?;
    let old_enabled =
        old_unit.is_some() && service_status(systemctl, &["is-enabled", "--quiet", UNIT])?;
    let old_active =
        old_unit.is_some() && service_status(systemctl, &["is-active", "--quiet", UNIT])?;
    fs::create_dir_all(&release_root)?;
    fs::create_dir_all(unit.parent().unwrap())?;
    fs::create_dir_all(&home)?;
    #[cfg(unix)]
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let name = format!("release-{}-{stamp}", std::process::id());
    let staged = release_root.join(format!(".{name}"));
    let published = release_root.join(&name);
    fs::create_dir(&staged)?;
    let result = async {
        for component in COMPONENTS {
            let dest = staged.join(component);
            fs::copy(source_dir.join(component), &dest)?;
            fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
        }
        if !already_joined {
            super::node_join::run(
                super::node_join::parse(join_args(&command, &home).into_iter())
                    .map_err(io::Error::other)?,
            )
            .await?;
        }
        fs::rename(&staged, &published)?;
        let pointer_tmp = release_root.join(format!(".current-{}-{stamp}", std::process::id()));
        symlink(&name, &pointer_tmp)?;
        fs::rename(&pointer_tmp, &current)?;
        protectorate_fsutil::write_file(&unit, unit_text.as_bytes(), 0o600)?;
        run_systemctl(systemctl, &["daemon-reload"])?;
        if !command.no_start {
            if old_active {
                run_systemctl(systemctl, &["enable", UNIT])?;
                run_systemctl(systemctl, &["restart", UNIT])?;
            } else {
                run_systemctl(systemctl, &["enable", "--now", UNIT])?;
            }
        }
        Ok::<(), Box<dyn Error>>(())
    }
    .await;
    if let Err(error) = result {
        if !command.no_start && fs::read_link(&current).ok().as_deref() == Some(Path::new(&name)) {
            let _ = run_systemctl(systemctl, &["stop", UNIT]);
        }
        if !command.no_start && !old_enabled {
            let _ = run_systemctl(systemctl, &["disable", "--now", UNIT]);
        }
        let restored_pointer = if let Some(previous) = old_pointer {
            let rollback = release_root.join(format!(".rollback-{}-{stamp}", std::process::id()));
            symlink(previous, &rollback).and_then(|()| fs::rename(rollback, &current))
        } else {
            match fs::remove_file(&current) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                result => result,
            }
        };
        if let Err(ref rollback_error) = restored_pointer {
            eprintln!("node install: release rollback failed: {rollback_error}");
        }
        if let Some(bytes) = old_unit {
            let _ = protectorate_fsutil::write_file(&unit, &bytes, 0o600);
        } else {
            let _ = fs::remove_file(&unit);
        }
        let _ = run_systemctl(systemctl, &["daemon-reload"]);
        if !command.no_start && old_enabled {
            let _ = run_systemctl(systemctl, &["enable", UNIT]);
        }
        if !command.no_start && old_active {
            let _ = run_systemctl(systemctl, &["restart", UNIT]);
        }
        let _ = fs::remove_dir_all(&staged);
        if restored_pointer.is_ok() {
            let _ = fs::remove_dir_all(&published);
        }
        return Err(error);
    }
    println!("installed Rust node at {}", current.display());
    if command.no_start {
        println!("service staged; start with systemctl --user enable --now {UNIT}");
    }
    Ok(())
}
