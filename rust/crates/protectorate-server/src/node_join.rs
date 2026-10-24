//! Join a hub: create this node's key, redeem the invite code with it and keep
//! the certificate the hub CA issues. The private key never leaves the node.
use protectorate_wire::v1;
use std::{
    env,
    error::Error,
    fs, io,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub(crate) const NODE_PORT: u16 = 7475;

pub(crate) struct Command {
    home: PathBuf,
    hub: String,
    name: String,
    code: Option<String>,
    code_file: Option<PathBuf>,
    ca_cert: PathBuf,
    listen: Option<SocketAddr>,
}

/// What `node join` recorded: the hub, this node's name and the private
/// address the node reported and must listen on.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Joined {
    pub(crate) hub: String,
    pub(crate) name: String,
    pub(crate) address: SocketAddr,
}

impl Joined {
    pub(crate) fn load(home: &Path) -> io::Result<Self> {
        let bytes = match fs::read(home.join("node.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "{} has not joined a hub: run node join with an invite code",
                        home.display()
                    ),
                ))
            }
            Err(error) => return Err(error),
        };
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    /// The node's certificate state, checked against its key and the hub CA.
    pub(crate) fn certificate(home: &Path) -> io::Result<protectorate_tls::NodeCert> {
        let tls = home.join("tls");
        protectorate_tls::NodeCert::new(
            &protectorate_tls::read_pem(&tls.join("ca.pem"))?,
            &protectorate_tls::read_pem(&tls.join("node.pem"))?,
            protectorate_tls::read_private(&tls.join("node-key.pem"))?.as_bytes(),
        )
    }
}

/// The joined (or given) private address, the agent's mutual TLS listener
/// configuration and the hook that serves a certificate the hub renews.
pub(crate) fn serve_tls(
    home: &Path,
    listen: Option<SocketAddr>,
) -> io::Result<(
    SocketAddr,
    Arc<tokio_rustls::rustls::ServerConfig>,
    protectorate_node::CertRenewal,
)> {
    let joined = Joined::load(home)?;
    let address = listen.unwrap_or(joined.address);
    protectorate_tls::check_listen(address)?;
    let cert = Arc::new(Joined::certificate(home)?);
    let config = cert.server_config()?;
    let path = home.join("tls/node.pem");
    let renew: protectorate_node::CertRenewal = Arc::new(move |pem: &str| {
        cert.replace(pem.as_bytes())?;
        protectorate_fsutil::write_file(&path, pem.as_bytes(), 0o644)
    });
    Ok((address, config, renew))
}

pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut home = None;
    let mut hub = None;
    let mut name = None;
    let mut code = None;
    let mut code_file = None;
    let mut ca_cert = None;
    let mut listen = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
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
            "--listen" => {
                listen = Some(
                    args.next()
                        .ok_or("--listen requires IP:PORT")?
                        .parse()
                        .map_err(|_| "--listen requires IP:PORT")?,
                )
            }
            _ => return Err(format!("unknown node join argument {flag:?}")),
        }
    }
    if code.is_some() && code_file.is_some() {
        return Err("--code and --code-file are exclusive".into());
    }
    let hub = hub.ok_or("node join requires --hub URL")?;
    if !hub.starts_with("https://") {
        return Err("--hub requires the hub's HTTPS URL".into());
    }
    Ok(Command {
        home: home.map_or_else(crate::default_home, Ok)?,
        hub: hub.trim_end_matches('/').to_owned(),
        name: name.unwrap_or_else(default_name),
        code,
        code_file,
        ca_cert: ca_cert.ok_or("node join requires --ca-cert FILE (the hub CA)")?,
        listen,
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
    let label = hostname
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
    let label = label.trim_matches('-');
    if label.is_empty() {
        "node".into()
    } else {
        label.to_owned()
    }
}

fn resolve_code(command: &Command) -> Result<String, Box<dyn Error>> {
    let code = if let Some(code) = &command.code {
        code.clone()
    } else if let Some(path) = &command.code_file {
        protectorate_tls::read_private(path)?
    } else {
        env::var("PROTECTORATE_JOIN_CODE").unwrap_or_default()
    };
    let code = code.trim().to_owned();
    if code.is_empty() || code.len() > 256 {
        return Err(
            "a join code is required (--code, --code-file or PROTECTORATE_JOIN_CODE)".into(),
        );
    }
    Ok(code)
}

/// The address of the interface this machine uses to reach the hub.
fn route_address(hub: &str) -> io::Result<SocketAddr> {
    let authority = hub.trim_start_matches("https://");
    let has_port = if authority.starts_with('[') {
        authority.contains("]:")
    } else {
        authority.contains(':')
    };
    let target = if has_port {
        authority.to_owned()
    } else {
        format!("{authority}:443")
    };
    let hub = target
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "hub address does not resolve"))?;
    let socket = UdpSocket::bind(if hub.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?;
    socket.connect(hub)?;
    Ok(SocketAddr::new(socket.local_addr()?.ip(), NODE_PORT))
}

fn write_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn join(command: Command) -> Result<Joined, Box<dyn Error + Send + Sync>> {
    let code = resolve_code(&command).map_err(|error| error.to_string())?;
    let ca = protectorate_tls::read_pem(&command.ca_cert)?;
    let address = match command.listen {
        Some(address) => address,
        None => route_address(&command.hub)?,
    };
    protectorate_tls::check_listen(address)?;
    let (key_pem, public_key, proof) =
        protectorate_tls::node_key(&protectorate_tls::join_message(&command.name, &code))?;
    let client = protectorate_connect_client::Client::new_with_ca_pem(
        &command.hub,
        "",
        Duration::from_secs(30),
        Some(&ca),
    )?;
    let reply = client.unary(
        v1::node_service::JOIN_NODE,
        &v1::JoinNodeRequest {
            name: command.name.clone(),
            code,
            public_key,
            proof,
            address: address.to_string(),
        },
    )?;
    let cert = reply.certificate;
    if cert.is_empty() {
        return Err("hub returned no certificate".into());
    }
    protectorate_tls::NodeCert::new(&ca, cert.as_bytes(), key_pem.as_bytes())?;
    let tls = command.home.join("tls");
    write_private_dir(&tls)?;
    protectorate_fsutil::write_file(&tls.join("ca.pem"), &ca, 0o644)?;
    protectorate_fsutil::write_file(&tls.join("node-key.pem"), key_pem.as_bytes(), 0o600)?;
    protectorate_fsutil::write_file(&tls.join("node.pem"), cert.as_bytes(), 0o644)?;
    let joined = Joined {
        hub: command.hub,
        name: command.name,
        address,
    };
    protectorate_fsutil::write_file(
        &command.home.join("node.json"),
        &serde_json::to_vec_pretty(&joined)?,
        0o600,
    )?;
    Ok(joined)
}

pub(crate) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    write_private_dir(&command.home)?;
    let joined = tokio::task::spawn_blocking(move || join(command))
        .await?
        .map_err(|error| -> Box<dyn Error> { error })?;
    println!(
        "joined {} as {}; node serve listens on {}",
        joined.hub, joined.name, joined.address
    );
    Ok(())
}
