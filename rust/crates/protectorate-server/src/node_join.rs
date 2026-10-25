//! Join a hub: create this node's key, redeem the invite code with it and keep
//! the certificate the hub CA issues. The private key never leaves the node.
use protectorate_wire::v1;
use std::{
    env,
    error::Error,
    fs, io,
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{net::TcpListener, sync::mpsc};

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
/// address the node listens on. The node rewrites the address when its
/// machine's IP changes.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
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

/// The agent's mutual TLS listener configuration and the hook that serves a
/// certificate the hub renews.
pub(crate) fn serve_tls(
    home: &Path,
) -> io::Result<(
    Arc<tokio_rustls::rustls::ServerConfig>,
    protectorate_node::CertRenewal,
)> {
    let cert = Arc::new(Joined::certificate(home)?);
    let config = cert.server_config()?;
    let path = home.join("tls/node.pem");
    let renew: protectorate_node::CertRenewal = Arc::new(move |pem: &str| {
        cert.replace(pem.as_bytes())?;
        protectorate_fsutil::write_file(&path, pem.as_bytes(), 0o644)
    });
    Ok((config, renew))
}

const ADDRESS_INTERVAL_ENV: &str = "PROTECTORATE_NODE_ADDRESS_INTERVAL";
const DEFAULT_ADDRESS_INTERVAL: Duration = Duration::from_secs(30);
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// A successful report is repeated this often, so a hub restored from an old
/// backup learns the address again. The hub treats an unchanged one as a no-op.
const REPORT_REFRESH: i64 = 600;
const REPORT_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether this machine has `ip` assigned: only then can a socket bind to it.
pub(crate) fn assigned(ip: IpAddr) -> bool {
    UdpSocket::bind((ip, 0)).is_ok()
}

/// The result of the last attempt to tell the hub the node's address, kept
/// beside `node.json` for `node doctor`.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Report {
    pub(crate) address: SocketAddr,
    pub(crate) time: i64,
    pub(crate) ok: bool,
    pub(crate) message: String,
}

impl Report {
    const FILE: &'static str = "address-report.json";

    pub(crate) fn load(home: &Path) -> Option<Self> {
        serde_json::from_slice(&fs::read(home.join(Self::FILE)).ok()?).ok()
    }

    fn save(&self, home: &Path) -> io::Result<()> {
        protectorate_fsutil::write_file(
            &home.join(Self::FILE),
            &serde_json::to_vec_pretty(self)?,
            0o600,
        )
    }
}

/// The address this node should serve on, given the one it serves on now.
/// The current address stays while this machine still has it; otherwise the
/// node takes the address its route to the hub leaves from, on the same port.
fn follow_route(hub: &str, current: SocketAddr) -> io::Result<SocketAddr> {
    if assigned(current.ip()) {
        return Ok(current);
    }
    routed(hub, current.port())
}

fn routed(hub: &str, port: u16) -> io::Result<SocketAddr> {
    let address = SocketAddr::new(
        route_ip(hub).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("no route to the hub {hub} to pick an address from: {error}"),
            )
        })?,
        port,
    );
    protectorate_tls::check_listen(address)?;
    Ok(address)
}

fn persist_address(home: &Path, joined: &Joined, address: SocketAddr) -> io::Result<()> {
    if joined.address == address {
        return Ok(());
    }
    let joined = Joined {
        hub: joined.hub.clone(),
        name: joined.name.clone(),
        address,
    };
    protectorate_fsutil::write_file(
        &home.join("node.json"),
        &serde_json::to_vec_pretty(&joined).map_err(io::Error::other)?,
        0o600,
    )
}

/// Binds the node's listener. An explicit `--listen` is served exactly as
/// given: it is never replaced, and a failure to bind it is an error. Without
/// one the node serves the address it joined with while this machine still
/// has it, and otherwise the address its route to the hub uses, keeping the
/// port; the new address is stored in `node.json`.
pub(crate) async fn bind(
    home: &Path,
    explicit: Option<SocketAddr>,
) -> io::Result<(TcpListener, Follow)> {
    let home = home.to_owned();
    let joined = Joined::load(&home)?;
    let mut candidates = match explicit {
        Some(address) => vec![address],
        None => {
            let hub = joined.hub.clone();
            let recorded = joined.address;
            let candidates = tokio::task::spawn_blocking(move || {
                let mut candidates = Vec::new();
                if assigned(recorded.ip()) {
                    candidates.push(Ok(recorded));
                }
                candidates.push(routed(&hub, recorded.port()));
                candidates
            })
            .await
            .map_err(io::Error::other)?;
            let mut found = Vec::new();
            let mut last = None;
            for candidate in candidates {
                match candidate {
                    Ok(address) if !found.contains(&address) => found.push(address),
                    Ok(_) => {}
                    Err(error) => last = Some(error),
                }
            }
            if found.is_empty() {
                let error = last.expect("the route candidate is always present");
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "the joined address {} is not assigned to this machine and the node cannot pick another: {error}",
                        recorded
                    ),
                ));
            }
            found
        }
    };
    let mut failure = None;
    for address in candidates.drain(..) {
        protectorate_tls::check_listen(address)?;
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let persisted = home.clone();
                let stored = joined.clone();
                if address != joined.address {
                    eprintln!(
                        "protectorate-rs node: serving on {address}, not the joined {}",
                        joined.address
                    );
                }
                tokio::task::spawn_blocking(move || persist_address(&persisted, &stored, address))
                    .await
                    .map_err(io::Error::other)?
                    .map_err(|error| {
                        io::Error::new(error.kind(), format!("store the node address: {error}"))
                    })?;
                return Ok((
                    listener,
                    Follow {
                        home,
                        hub: joined.hub,
                        current: address,
                        strict: explicit.is_some(),
                    },
                ));
            }
            Err(error) => {
                failure = Some(io::Error::new(
                    error.kind(),
                    format!("cannot listen on {address}: {error}"),
                ))
            }
        }
    }
    let error = failure.expect("a candidate was tried");
    Err(match explicit {
        Some(_) => io::Error::new(
            error.kind(),
            format!("{error}; --listen is served exactly as given, drop it to let the node follow its machine's address"),
        ),
        None => error,
    })
}

/// Keeps the node's address current while it runs: rebinds the listener when
/// the machine loses the address it serves, stores the new one and tells the
/// hub.
pub(crate) struct Follow {
    home: PathBuf,
    hub: String,
    current: SocketAddr,
    strict: bool,
}

impl Follow {
    /// Runs until the listener it feeds is gone. `swap` hands a freshly bound
    /// listener to the accept loop.
    pub(crate) fn spawn(self, swap: mpsc::Sender<TcpListener>) {
        tokio::spawn(self.run(swap));
    }

    async fn run(mut self, swap: mpsc::Sender<TcpListener>) {
        let interval = env::var(ADDRESS_INTERVAL_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .map_or(DEFAULT_ADDRESS_INTERVAL, Duration::from_secs);
        let mut warned = String::new();
        let mut warn = |message: String| {
            if warned != message {
                eprintln!("protectorate-rs node: {message}");
                warned = message;
            }
        };
        loop {
            if !assigned(self.current.ip()) {
                if self.strict {
                    warn(format!(
                        "--listen address {} is no longer assigned to this machine and is not replaced because --listen was given: restart the node without --listen to let it follow the machine's address",
                        self.current
                    ));
                } else {
                    match self.rebind().await {
                        Ok(listener) => {
                            if swap.send(listener).await.is_err() {
                                return;
                            }
                        }
                        Err(error) => warn(format!(
                            "address {} is no longer assigned to this machine and no new one could be taken: {error}",
                            self.current
                        )),
                    }
                }
            }
            let serving = assigned(self.current.ip());
            if serving {
                self.report().await;
            }
            let wait = if serving {
                interval
            } else {
                RETRY_INTERVAL.min(interval)
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = swap.closed() => return,
            }
        }
    }

    async fn rebind(&mut self) -> io::Result<TcpListener> {
        let hub = self.hub.clone();
        let current = self.current;
        let next = tokio::task::spawn_blocking(move || follow_route(&hub, current))
            .await
            .map_err(io::Error::other)??;
        let listener = TcpListener::bind(next).await.map_err(|error| {
            io::Error::new(error.kind(), format!("cannot listen on {next}: {error}"))
        })?;
        let home = self.home.clone();
        let joined = Joined::load(&home)?;
        tokio::task::spawn_blocking(move || persist_address(&home, &joined, next))
            .await
            .map_err(io::Error::other)?
            .map_err(|error| {
                io::Error::new(error.kind(), format!("store the node address: {error}"))
            })?;
        eprintln!(
            "protectorate-rs node: address {} is gone; now serving on {next}",
            self.current
        );
        self.current = next;
        Ok(listener)
    }

    /// Tells the hub the current address unless the hub already acknowledged
    /// it recently.
    async fn report(&self) {
        let home = self.home.clone();
        let address = self.current;
        let previous = Report::load(&home);
        let due = previous.as_ref().is_none_or(|last| {
            !last.ok
                || last.address != address
                || protectorate_fsutil::unix_seconds() - last.time >= REPORT_REFRESH
        });
        if !due {
            return;
        }
        let result = tokio::task::spawn_blocking(move || {
            let outcome = report_address(&home, address);
            let report = Report {
                address,
                time: protectorate_fsutil::unix_seconds(),
                ok: outcome.is_ok(),
                message: match &outcome {
                    Ok(true) => "the hub updated its record".into(),
                    Ok(false) => "the hub already had this address".into(),
                    Err(error) => error.clone(),
                },
            };
            let _ = report.save(&home);
            (outcome, previous)
        })
        .await;
        if let Ok((outcome, previous)) = result {
            match outcome {
                Ok(true) => eprintln!("protectorate-rs node: told the hub it serves on {address}"),
                Ok(false) => {}
                Err(error) if previous.is_none_or(|last| last.ok || last.message != error) => {
                    eprintln!(
                        "protectorate-rs node: could not report {address} to the hub: {error}"
                    );
                }
                Err(_) => {}
            }
        }
    }
}

/// Calls the hub's ReportNodeAddress over mutual TLS with the node's own
/// certificate. Returns whether the hub changed its record.
fn report_address(home: &Path, address: SocketAddr) -> Result<bool, String> {
    let joined = Joined::load(home).map_err(|error| error.to_string())?;
    let tls = home.join("tls");
    let read = |name: &str| {
        protectorate_tls::read_pem(&tls.join(name)).map_err(|e| format!("{name}: {e}"))
    };
    let key = protectorate_tls::read_private(&tls.join("node-key.pem"))
        .map_err(|error| format!("node-key.pem: {error}"))?;
    let identity = protectorate_connect_client::NodeTls::from_pem(
        &read("ca.pem")?,
        &read("node.pem")?,
        key.as_bytes(),
    )
    .map_err(|error| error.message)?;
    let client =
        protectorate_connect_client::Client::new_for_node(&joined.hub, &identity, REPORT_TIMEOUT)
            .map_err(|error| error.message)?;
    client
        .unary(
            v1::node_service::REPORT_NODE_ADDRESS,
            &v1::ReportNodeAddressRequest {
                address: address.to_string(),
            },
        )
        .map(|reply| reply.changed)
        .map_err(|error| format!("{} reported {}: {}", joined.hub, address, error.message))
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

/// The address of the interface this machine uses to reach the hub. Connecting
/// a UDP socket sends nothing; it only makes the OS choose a route.
pub(crate) fn route_ip(hub: &str) -> io::Result<IpAddr> {
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
    Ok(socket.local_addr()?.ip())
}

fn route_address(hub: &str) -> io::Result<SocketAddr> {
    Ok(SocketAddr::new(route_ip(hub)?, NODE_PORT))
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
