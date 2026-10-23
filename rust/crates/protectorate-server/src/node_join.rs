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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_requires_control_and_exclusive_key_sources() {
        assert!(parse(["--key".into(), "abc".into()].into_iter()).is_err());
        assert!(parse(
            [
                "--control".into(),
                "http://control".into(),
                "--key".into(),
                "a".into(),
                "--key-file".into(),
                "secret".into()
            ]
            .into_iter()
        )
        .is_err());
        let cmd = parse(
            [
                "--control".into(),
                "http://control".into(),
                "--name".into(),
                "gpu1".into(),
                "--key".into(),
                "abc".into(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(cmd.name, "gpu1");
        assert_eq!(resolve_key(&cmd).unwrap(), "abc");
        assert!(parse(
            [
                "--control".into(),
                "https://control".into(),
                "--ca-cert".into()
            ]
            .into_iter()
        )
        .is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
    async fn real_headscale_node_join_persists_registered_identity() {
        real_headscale_join(false).await;
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
    async fn real_headscale_node_join_with_private_https_ca() {
        real_headscale_join(true).await;
    }

    #[cfg(target_os = "linux")]
    async fn real_headscale_join(https: bool) {
        use protectorate_tailnet::Config;
        use std::{
            net::TcpListener,
            os::unix::fs::PermissionsExt,
            process::{Child, Command as ProcessCommand, Stdio},
            time::Instant,
        };

        struct ChildGuard(Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let free_addr = || {
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .to_string()
        };
        let binary = env::var("PROT_HEADSCALE_BINARY").expect("set PROT_HEADSCALE_BINARY");
        let home = tempfile::tempdir().unwrap();
        let listen = free_addr();
        let origin = if https {
            format!("https://localhost:{}", listen.rsplit_once(':').unwrap().1)
        } else {
            format!("http://{listen}")
        };
        let mut config = Config::for_server(&origin);
        config.listen = listen;
        config.metrics_listen = free_addr();
        config.grpc_listen = free_addr();
        config.embedded_derp = false;
        config.public_derp = true;
        let ca_pem = if https {
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let cert_path = home.path().join("control-cert.pem");
            let key_path = home.path().join("control-key.pem");
            let pem = cert.cert.pem();
            fs::write(&cert_path, &pem).unwrap();
            fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
            config.tls_cert = Some(cert_path.clone());
            config.tls_key = Some(key_path);
            Some((cert_path, pem))
        } else {
            None
        };
        config.write_files(home.path(), None).unwrap();
        let cfg = home.path().join("tailnet/headscale.yaml");
        let log_path = home.path().join("headscale.log");
        let log = fs::File::create(&log_path).unwrap();
        let child = ProcessCommand::new(&binary)
            .arg("serve")
            .arg("-c")
            .arg(&cfg)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let mut child = ChildGuard(child);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                panic!(
                    "Headscale exited {status}: {}",
                    fs::read_to_string(&log_path).unwrap()
                );
            }
            if std::net::TcpStream::connect(&config.listen).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Headscale startup timed out: {}",
                fs::read_to_string(&log_path).unwrap()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let cli = |args: &[&str]| -> serde_json::Value {
            let output = ProcessCommand::new(&binary)
                .arg("-c")
                .arg(&cfg)
                .arg("-o")
                .arg("json")
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "headscale {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap()
        };
        let user = cli(&["users", "create", "protectorate"]);
        let id = user["id"]
            .as_u64()
            .or_else(|| user["id"].as_str().and_then(|s| s.parse().ok()))
            .unwrap();
        let key = cli(&["preauthkeys", "create", "-u", &id.to_string()]);
        let secret = key["key"].as_str().unwrap();
        let mut args = vec![
            "--home".into(),
            home.path().to_string_lossy().into_owned(),
            "--control".into(),
            origin,
            "--name".into(),
            "rust-node".into(),
            "--key".into(),
            secret.into(),
        ];
        if let Some((path, _)) = &ca_pem {
            args.push("--ca-cert".into());
            args.push(path.to_string_lossy().into_owned());
        }
        let command = parse(args.into_iter()).unwrap();
        run(command).await.unwrap();
        let state_path = home.path().join("tsnet/join.json");
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(state["name"], "rust-node");
        assert!(state["nodeKey"].as_str().unwrap().starts_with("nodekey:"));
        assert!(state["discoKey"].as_str().unwrap().starts_with("discokey:"));
        assert_eq!(
            state["controlCaPem"].as_str(),
            ca_pem.as_ref().map(|(_, pem)| pem.as_str())
        );
        assert_eq!(
            DiscoIdentity::open(home.path().join("tsnet"))
                .unwrap()
                .public_key()
                .to_string(),
            state["discoKey"]
        );
        assert_eq!(
            fs::metadata(state_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        if https {
            use std::sync::atomic::{AtomicBool, Ordering};
            let poller = protectorate_tailnet_poll::Poller::open(home.path().join("tsnet"))
                .unwrap()
                .with_timeout(Duration::from_secs(3))
                .unwrap();
            let stop = AtomicBool::new(false);
            std::thread::scope(|scope| {
                let task = scope.spawn(|| poller.poll_once(&stop));
                let deadline = Instant::now() + Duration::from_secs(8);
                loop {
                    if let Some(map) = poller.snapshot() {
                        assert_eq!(
                            map.self_node.unwrap().key,
                            state["nodeKey"].as_str().unwrap()
                        );
                        break;
                    }
                    assert!(Instant::now() < deadline, "private-CA map poll timed out");
                    std::thread::sleep(Duration::from_millis(20));
                }
                stop.store(true, Ordering::Release);
                task.join().unwrap().unwrap();
            });
            assert!(poller.snapshot().is_none());
        }
        let nodes = cli(&["nodes", "list"]);
        assert!(
            nodes
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["given_name"] == "rust-node" || node["name"] == "rust-node"),
            "registered node absent from Headscale list"
        );
    }
}
