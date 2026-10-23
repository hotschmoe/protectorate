//! Install a Rust node into a private per-user release directory.
#[cfg(target_os = "linux")]
use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
use std::{
    env,
    error::Error,
    fs,
    io::{self, Read},
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
    control: String,
    name: Option<String>,
    key: Option<String>,
    key_file: Option<PathBuf>,
    ca_cert: Option<PathBuf>,
    listen: String,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    allow_plain_jobs: bool,
    dry_run: bool,
    no_start: bool,
    serve_args: Vec<String>,
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut control = None;
    let mut name = None;
    let mut key = None;
    let mut key_file = None;
    let mut ca_cert = None;
    let mut listen = "127.0.0.1:7475".to_owned();
    let mut tls_cert = None;
    let mut tls_key = None;
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
            "--control" => control = Some(args.next().ok_or("--control requires URL")?),
            "--name" => name = Some(args.next().ok_or("--name requires NAME")?),
            "--key" => key = Some(args.next().ok_or("--key requires KEY")?),
            "--key-file" => {
                key_file = Some(PathBuf::from(
                    args.next().ok_or("--key-file requires FILE")?,
                ))
            }
            "--ca-cert" => {
                ca_cert = Some(PathBuf::from(args.next().ok_or("--ca-cert requires FILE")?))
            }
            "--listen" => listen = args.next().ok_or("--listen requires IP:PORT")?,
            "--tls-cert" => {
                tls_cert = Some(PathBuf::from(
                    args.next().ok_or("--tls-cert requires FILE")?,
                ))
            }
            "--tls-key" => {
                tls_key = Some(PathBuf::from(args.next().ok_or("--tls-key requires FILE")?))
            }
            "--allow-plain-jobs" => allow_plain_jobs = true,
            "--dry-run" => dry_run = true,
            "--no-start" => no_start = true,
            _ => return Err(format!("unknown node install argument {flag:?}")),
        }
    }
    if key.is_some() && key_file.is_some() {
        return Err("--key and --key-file are exclusive".into());
    }
    for forbidden in [
        "--home",
        "--agent-token-file",
        "--sandbox-helper",
        "--listen",
        "--tls-cert",
        "--tls-key",
    ] {
        if serve_args
            .iter()
            .any(|arg| arg == forbidden || arg.starts_with(&format!("{forbidden}=")))
        {
            return Err(format!(
                "{forbidden} is managed by node install; use the install flag where available"
            ));
        }
    }
    let control = control.ok_or("node install requires --control URL")?;
    if !control.starts_with("http://") && !control.starts_with("https://") {
        return Err("--control requires an HTTP or HTTPS URL".into());
    }
    if ca_cert.is_some() && !control.starts_with("https://") {
        return Err("--ca-cert requires HTTPS control URL".into());
    }
    if tls_cert.is_some() != tls_key.is_some() {
        return Err("--tls-cert and --tls-key must be given together".into());
    }
    if serve_args.iter().any(|arg| arg.contains(['\n', '\r'])) {
        return Err("node serve arguments cannot contain newlines".into());
    }
    let addr: std::net::SocketAddr = listen.parse().map_err(|_| "--listen requires IP:PORT")?;
    if !addr.ip().is_loopback() && tls_cert.is_none() {
        return Err("non-loopback --listen requires --tls-cert and --tls-key".into());
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        control,
        name,
        key,
        key_file,
        ca_cert,
        listen,
        tls_cert,
        tls_key,
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

fn existing_join(home: &Path, control: &str, name: Option<&str>) -> io::Result<bool> {
    let dir = home.join("tsnet");
    let join = dir.join("join.json");
    if !optional_file(&join)? {
        return Ok(false);
    }
    let metadata = fs::metadata(&join)?;
    if metadata.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "node enrollment state is too large",
        ));
    }
    for key in ["machine.key", "node.key"] {
        regular(&dir.join(key))?;
    }
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&join)?)?;
    let field = |name| {
        state
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
    };
    if field("controlUrl") != Some(control)
        || field("name").is_none()
        || field("nodeKey").is_none()
        || field("discoKey").is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "existing enrollment is incomplete or uses another control URL",
        ));
    }
    if name.is_some() && field("name") != name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "existing enrollment uses another node name",
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

#[cfg(target_os = "linux")]
fn token(path: &Path) -> io::Result<bool> {
    if optional_file(path)? {
        let meta = fs::metadata(path)?;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "agent token is not private",
            ));
        }
        if meta.len() == 0 || meta.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "agent token has invalid size",
            ));
        }
        return Ok(false);
    }
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut text = String::with_capacity(65);
    for byte in bytes {
        use std::fmt::Write;
        write!(text, "{byte:02x}").unwrap();
    }
    text.push('\n');
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    use std::io::Write;
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(true)
}

fn join_args(command: &Command, home: &Path) -> Vec<String> {
    let mut args = vec![
        "--home".into(),
        home.display().to_string(),
        "--control".into(),
        command.control.clone(),
    ];
    if let Some(name) = &command.name {
        args.extend(["--name".into(), name.clone()]);
    }
    if let Some(path) = &command.key_file {
        args.extend(["--key-file".into(), path.display().to_string()]);
    }
    if let Some(key) = &command.key {
        args.extend(["--key".into(), key.clone()]);
    }
    if let Some(path) = &command.ca_cert {
        args.extend(["--ca-cert".into(), path.display().to_string()]);
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
    let release_root = user_home.join(".local/share/protectorate-rust/node-install");
    let unit = user_home.join(".config/systemd/user").join(UNIT);
    for path in [&home, &user_home, source_dir].into_iter().chain(
        [command.tls_cert.as_deref(), command.tls_key.as_deref()]
            .into_iter()
            .flatten(),
    ) {
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
        command.key_file.as_deref(),
        command.ca_cert.as_deref(),
        command.tls_cert.as_deref(),
        command.tls_key.as_deref(),
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
    let already_joined = existing_join(&home, &command.control, command.name.as_deref())?;
    let mut args = vec![
        "--home".into(),
        home.display().to_string(),
        "--agent-token-file".into(),
        home.join("agent-token").display().to_string(),
        "--listen".into(),
        command.listen.clone(),
    ];
    if command.allow_plain_jobs {
        args.push("--allow-plain-jobs".into());
    }
    if let (Some(cert), Some(key)) = (&command.tls_cert, &command.tls_key) {
        args.extend([
            "--tls-cert".into(),
            cert.display().to_string(),
            "--tls-key".into(),
            key.display().to_string(),
        ]);
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
            "register with control"
        }
    );
    println!("service unit: {}", unit.display());
    if command.dry_run {
        println!("dry run: no files or services changed");
        return Ok(());
    }
    if !already_joined
        && command.key.is_none()
        && command.key_file.is_none()
        && env::var_os("PROTECTORATE_ENROLL_KEY").is_none()
    {
        return Err("enrollment requires --key, --key-file or PROTECTORATE_ENROLL_KEY".into());
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
    let created_token = token(&home.join("agent-token"))?;
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
        if created_token {
            let _ = fs::remove_file(home.join("agent-token"));
        }
        return Err(error);
    }
    println!(
        "installed Rust node at {}; agent token: {}",
        current.display(),
        home.join("agent-token").display()
    );
    if command.no_start {
        println!("service staged; start with systemctl --user enable --now {UNIT}");
    }
    Ok(())
}
