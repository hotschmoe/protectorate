//! Register this node with the configured Headscale control server.
//! Registration persists identity; serving and hub-to-node authorization are separate.

use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_member::MemberIdentity;
use std::{env, error::Error, fs, io, io::Read, path::PathBuf, time::Duration};

pub(crate) struct Command {
    home: PathBuf,
    control: String,
    name: String,
    key: Option<String>,
    key_file: Option<PathBuf>,
    ca_cert: Option<PathBuf>,
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut control = None;
    let mut name = None;
    let mut key = None;
    let mut key_file = None;
    let mut ca_cert = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
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
            _ => return Err(format!("unknown node join argument {flag:?}")),
        }
    }
    if key.is_some() && key_file.is_some() {
        return Err("--key and --key-file are exclusive".into());
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        control: control.ok_or("node join requires --control URL")?,
        name: name.unwrap_or_else(default_name),
        key,
        key_file,
        ca_cert,
    })
}

fn default_name() -> String {
    let hostname = env::var(if cfg!(windows) {
        "COMPUTERNAME"
    } else {
        "HOSTNAME"
    })
    .ok()
    .filter(|s| !s.is_empty());
    #[cfg(not(windows))]
    let hostname = hostname.or_else(|| fs::read_to_string("/etc/hostname").ok());
    let hostname = hostname.unwrap_or_else(|| "node".to_owned());
    let mut label = hostname
        .trim()
        .split('.')
        .next()
        .unwrap_or("node")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(63)
        .collect::<String>();
    label = label.trim_matches('-').to_owned();
    if label.is_empty() {
        "node".into()
    } else {
        label
    }
}

fn resolve_key(command: &Command) -> Result<String, Box<dyn Error>> {
    let key = if let Some(key) = &command.key {
        key.clone()
    } else if let Some(path) = &command.key_file {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || metadata.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "enrollment key file must be a bounded regular file",
            )
            .into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "enrollment key file must be private (mode 0600)",
                )
                .into());
            }
        }
        fs::read_to_string(path)?
    } else {
        env::var("PROTECTORATE_ENROLL_KEY").unwrap_or_default()
    };
    let key = key.trim().to_owned();
    if key.is_empty() || key.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "enrollment key is required and must be at most 4096 bytes",
        )
        .into());
    }
    Ok(key)
}

fn read_ca_cert(path: &std::path::Path) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > 60 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "control CA must be a nonempty regular file of at most 60 KiB",
        )
        .into());
    }
    let mut bytes = Vec::new();
    file.take(60 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > 60 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "control CA changed size").into());
    }
    Ok(bytes)
}

pub(crate) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    let key = resolve_key(&command)?;
    let name = command.name.clone();
    let control = command.control.clone();
    let ca_cert = command.ca_cert.clone();
    let directory = command.home.join("tsnet");
    let registered = tokio::task::spawn_blocking(move || {
        if ca_cert.is_some() && !control.starts_with("https://") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--ca-cert requires an HTTPS control URL",
            )
            .into());
        }
        let ca_pem = ca_cert.as_deref().map(read_ca_cert).transpose()?;
        let ca_text = ca_pem.as_deref().map(std::str::from_utf8).transpose()?;
        let member = MemberIdentity::open(&directory)?;
        let disco = DiscoIdentity::open(&directory)?;
        let state = serde_json::json!({
            "controlUrl": &control,
            "controlCaPem": ca_text,
            "name": &name,
            "nodeKey": member.node_public_key(),
            "discoKey": disco.public_key().to_string(),
        });
        let mut bytes = serde_json::to_vec_pretty(&state)?;
        bytes.push(b'\n');
        if bytes.len() > 64 << 10 {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "joined state exceeds 64 KiB").into(),
            );
        }
        member.register_with_ca_pem(
            &control,
            &key,
            &name,
            Duration::from_secs(30),
            ca_pem.as_deref(),
        )?;
        protectorate_fsutil::write_file(&directory.join("join.json"), &bytes, 0o600)?;
        Ok::<_, Box<dyn Error + Send + Sync>>(state)
    })
    .await?
    .map_err(|error| -> Box<dyn Error> { error })?;
    println!(
        "registered as {}; start node serve for hub-authorized tailnet service calls",
        registered["name"].as_str().unwrap_or("node")
    );
    Ok(())
}
