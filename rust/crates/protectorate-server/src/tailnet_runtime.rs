//! Own the hub's Headscale control process and private relay when configured.

use protectorate_tailnet::{Config, StopToken, Supervisor, TAG_HUB};
use protectorate_tailnet_admin::Admin;
use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_member::MemberIdentity;
use std::{
    error::Error,
    fs, io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub struct Runtime {
    admin: Arc<Admin>,
    pools: Vec<String>,
    hub_name: String,
    key_expiry_days: i64,
    stop: StopToken,
    supervisor: Option<tokio::task::JoinHandle<protectorate_tailnet::Result<()>>>,
    _derp: Option<protectorate_derp::Service>,
}

impl Runtime {
    pub async fn start_if_configured(home: &Path) -> Result<Option<Self>> {
        if !home.join("tailnet/config.json").try_exists()? {
            return Ok(None);
        }
        let config = Config::load(home)?;
        // A pre-existing Headscale can answer /health while our child is
        // blocked on the database lock or fails to bind. Refuse startup rather
        // than silently attaching this Rust hub to another control process.
        let probe = std::net::TcpListener::bind(&config.listen).map_err(|error| {
            io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("Headscale listen {} is unavailable: {error}", config.listen),
            )
        })?;
        drop(probe);
        let local = loopback(&config.listen)?;
        let control = local_control_url(&config)?;
        let derp = if config.embedded_derp {
            let hostname = config
                .server_url
                .parse::<axum::http::Uri>()?
                .host()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "tailnet control URL has no host",
                    )
                })?
                .to_owned();
            Some(protectorate_derp::Service::start(
                protectorate_derp::Config {
                    listen: config.derp_listen.parse()?,
                    stun_listen: config.stun_listen.parse()?,
                    hostname,
                    verify_url: format!("http://{local}/verify"),
                    state_dir: home.join("tailnet"),
                },
            )?)
        } else {
            None
        };
        let pools = config.pools.clone();
        let hub_name = config.hub_name.clone();
        let key_expiry_days = i64::from(config.key_expiry_days);
        let mut supervisor = Supervisor::new(home, config);
        if let Some(derp) = &derp {
            supervisor = supervisor.with_derp_cert_pin(&derp.certificate_pin);
        }
        let binary = supervisor.binary.clone();
        let stop = StopToken::new();
        let run_stop = stop.clone();
        let handle = tokio::task::spawn_blocking(move || supervisor.run(&run_stop));
        let outcome = async {
            wait_ready(&control, local, &handle).await?;
            let key = load_or_create_key(home, &binary, &control, local).await?;
            let mut admin =
                Admin::new_with_local_addr(&control, &key, Duration::from_secs(30), Some(local))?;
            let join_home = home.to_owned();
            let join_control = control.clone();
            let join_name = hub_name.clone();
            tokio::task::spawn_blocking(move || {
                admin.ensure_policy_user()?;
                ensure_hub_member(&join_home, &join_control, local, &join_name, &admin)?;
                Ok::<_, protectorate_tailnet_admin::Error>(admin)
            })
            .await?
            .map(Arc::new)
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })
        }
        .await;
        match outcome {
            Ok(admin) => Ok(Some(Self {
                admin,
                pools,
                hub_name,
                key_expiry_days,
                stop,
                supervisor: Some(handle),
                _derp: derp,
            })),
            Err(error) => {
                stop.stop();
                let _ = handle.await;
                Err(error)
            }
        }
    }

    pub fn admin(&self) -> Arc<Admin> {
        Arc::clone(&self.admin)
    }

    pub fn pools(&self) -> &[String] {
        &self.pools
    }

    pub fn hub_name(&self) -> &str {
        &self.hub_name
    }

    pub fn key_expiry_days(&self) -> i64 {
        self.key_expiry_days
    }

    pub async fn stop(mut self) -> Result<()> {
        self.stop.stop();
        if let Some(handle) = self.supervisor.take() {
            handle.await??;
        }
        Ok(())
    }
}

fn ensure_hub_member(
    home: &Path,
    control: &str,
    local: SocketAddr,
    name: &str,
    admin: &Admin,
) -> protectorate_tailnet_admin::Result<()> {
    use protectorate_tailnet_admin::Error as AdminError;
    let state_dir = home.join("tailnet/tsnet-hub");
    let member = MemberIdentity::open(&state_dir)
        .map_err(|e| AdminError::Invalid(format!("hub member identity: {e}")))?;
    let disco = DiscoIdentity::open(&state_dir)
        .map_err(|e| AdminError::Invalid(format!("hub disco identity: {e}")))?;
    let node_key = member.node_public_key();
    let disco_key = disco.public_key().to_string();
    let state_path = state_dir.join("join.json");
    let saved = read_hub_join(&state_path).map_err(|e| AdminError::Invalid(e.to_string()))?;
    if let Some(saved) = &saved {
        if saved["controlUrl"] != control
            || saved["name"] != name
            || saved["nodeKey"] != node_key
            || saved["discoKey"] != disco_key
        {
            return Err(AdminError::Invalid(
                "hub joined state differs from durable identity or control configuration".into(),
            ));
        }
    }

    // Headscale is the authority after a crash between registration and the
    // local state write. Reconcile before creating another single-use key.
    let nodes = admin.nodes()?;
    if nodes
        .iter()
        .any(|node| node.node_key != node_key && (node.given_name == name || node.name == name))
    {
        return Err(AdminError::Invalid(format!(
            "hub name {name:?} belongs to a different node key"
        )));
    }
    let own = nodes.iter().find(|node| node.node_key == node_key);
    if let Some(node) = own {
        if node.given_name != name && node.name != name {
            return Err(AdminError::Invalid(format!(
                "hub node key is registered under a different name: {}",
                node.given_name
            )));
        }
        if !node.tags.iter().any(|tag| tag == TAG_HUB) {
            return Err(AdminError::Invalid("hub identity lacks tag:hub".into()));
        }
    } else {
        let invite = admin.create_key(&[TAG_HUB.to_owned()], Duration::from_secs(3600))?;
        member
            .register_with_local_addr(
                control,
                &invite.key,
                name,
                Duration::from_secs(30),
                None,
                Some(local),
            )
            .map_err(|e| AdminError::Transport(format!("register hub identity: {e}")))?;
        let registered = admin
            .nodes()?
            .into_iter()
            .find(|node| node.node_key == node_key);
        match registered {
            Some(node) if node.tags.iter().any(|tag| tag == TAG_HUB) => {}
            Some(_) => return Err(AdminError::Invalid("registered hub lacks tag:hub".into())),
            None => {
                return Err(AdminError::Invalid(
                    "registered hub is absent from Headscale".into(),
                ));
            }
        }
    }
    if saved.is_none() {
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "controlUrl": control,
            "name": name,
            "nodeKey": node_key,
            "discoKey": disco_key,
        }))
        .map_err(|e| AdminError::Decode(format!("encode hub joined state: {e}")))?;
        protectorate_fsutil::write_file(&state_path, &bytes, 0o600)
            .map_err(|e| AdminError::Invalid(format!("persist hub joined state: {e}")))?;
    }
    Ok(())
}

fn read_hub_join(path: &Path) -> io::Result<Option<serde_json::Value>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file() || metadata.len() > 64 << 10 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "hub joined state must be a bounded regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "hub joined state must be private",
            ));
        }
    }
    let bytes = fs::read(path)?;
    let state = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(state))
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.stop();
    }
}

pub(crate) fn loopback(address: &str) -> Result<SocketAddr> {
    let mut address: SocketAddr = address.parse()?;
    if address.ip().is_unspecified() {
        address.set_ip(match address.ip() {
            IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        });
    }
    Ok(address)
}

pub(crate) fn local_control_url(config: &Config) -> Result<String> {
    let url: axum::http::Uri = config.server_url.parse()?;
    let authority = url.authority().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "tailnet control URL has no authority",
        )
    })?;
    let scheme = if config.tls_cert.is_some() {
        "https"
    } else {
        "http"
    };
    Ok(format!("{scheme}://{authority}"))
}

async fn wait_ready(
    base: &str,
    local: SocketAddr,
    supervisor: &tokio::task::JoinHandle<protectorate_tailnet::Result<()>>,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if supervisor.is_finished() {
            return Err(io::Error::other("Headscale supervisor stopped before readiness").into());
        }
        let base = base.to_owned();
        let healthy = tokio::task::spawn_blocking(move || {
            Admin::new_with_local_addr(&base, "readiness", Duration::from_secs(2), Some(local))
                .and_then(|admin| admin.health())
                .is_ok()
        })
        .await?;
        if healthy {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Headscale did not become ready within 60 seconds",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub(crate) async fn load_or_create_key(
    home: &Path,
    binary: &Path,
    base: &str,
    local: SocketAddr,
) -> Result<String> {
    let path = home.join("tailnet/apikey");
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.len() > 4096 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Headscale API key must be a regular file of at most 4096 bytes",
                )
                .into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Headscale API key must be private (mode 0600)",
                    )
                    .into());
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let existing = match fs::read_to_string(&path) {
        Ok(key) => key.trim().to_owned(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    if !existing.is_empty() {
        let base = base.to_owned();
        let key = existing.clone();
        let works = tokio::task::spawn_blocking(move || {
            Admin::new_with_local_addr(&base, &key, Duration::from_secs(3), Some(local))
                .and_then(|admin| admin.nodes())
                .is_ok()
        })
        .await?;
        if works {
            return Ok(existing);
        }
    }
    create_key(home, binary).await
}

async fn create_key(home: &Path, binary: &Path) -> Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(binary)
            .arg("-c")
            .arg(home.join("tailnet/headscale.yaml"))
            .args(["apikeys", "create", "--expiration", "365d"])
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Headscale API key creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    if output.stdout.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Headscale API key output is too long",
        )
        .into());
    }
    let text = std::str::from_utf8(&output.stdout)?;
    let key = text
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_matches('"');
    if key.is_empty() || key.len() > 4096 || key.contains(['\r', '\n']) {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "Headscale returned no API key").into(),
        );
    }
    let path: PathBuf = home.join("tailnet/apikey");
    protectorate_fsutil::write_file(&path, format!("{key}\n").as_bytes(), 0o600)?;
    Ok(key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_dial_keeps_the_public_name_but_uses_headscale_listen_scheme() {
        let mut config = Config::for_server("https://control.example:7480");
        assert_eq!(
            local_control_url(&config).unwrap(),
            "http://control.example:7480"
        );
        config.tls_cert = Some("/tmp/cert".into());
        config.tls_key = Some("/tmp/key".into());
        assert_eq!(
            local_control_url(&config).unwrap(),
            "https://control.example:7480"
        );
        assert_eq!(
            loopback("0.0.0.0:7480").unwrap(),
            "127.0.0.1:7480".parse().unwrap()
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn configured_hub_bootstraps_key_and_supervises_a_disposable_headscale() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let mut config = Config::for_server("http://control.example:7480");
        config.listen = address.to_string();
        config.embedded_derp = false;
        config.public_derp = true;
        config.save(home).unwrap();
        let hub_dir = home.join("tailnet/tsnet-hub");
        let member = MemberIdentity::open(&hub_dir).unwrap();
        let disco = DiscoIdentity::open(&hub_dir).unwrap();
        protectorate_fsutil::write_file(
            &hub_dir.join("join.json"),
            &serde_json::to_vec(&serde_json::json!({
                "controlUrl": "http://control.example:7480",
                "name": config.hub_name,
                "nodeKey": member.node_public_key(),
                "discoKey": disco.public_key().to_string(),
            }))
            .unwrap(),
            0o600,
        )
        .unwrap();
        let bin = home.join(format!(
            "bin/headscale-{}",
            protectorate_tailnet::HEADSCALE_VERSION
        ));
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, format!("#!/bin/sh\nif [ \"$3\" = apikeys ]; then echo test-api-key; exit 0; fi\nexec python3 \"$0.py\" {} \"$0.node\"\n", address.port())).unwrap();
        fs::write(format!("{}.node", bin.display()), serde_json::to_vec(&serde_json::json!({"nodes":[{"id":"1","givenName":config.hub_name,"nodeKey":member.node_public_key(),"tags":[TAG_HUB]}]})).unwrap()).unwrap();
        fs::write(
            format!("{}.py", bin.display()),
            r#"import http.server, sys
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/health':
            body = b'ok'
        elif self.path.startswith('/api/v1/user'):
            body = b'{"users":[{"id":"1","name":"protectorate"}]}'
        elif self.path.startswith('/api/v1/node'):
            body = open(sys.argv[2], 'rb').read()
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), Handler).serve_forever()
"#,
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        let runtime =
            tokio::time::timeout(Duration::from_secs(5), Runtime::start_if_configured(home))
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        assert_eq!(
            fs::read_to_string(home.join("tailnet/apikey")).unwrap(),
            "test-api-key\n"
        );
        assert!(home.join("tailnet/headscale.yaml").is_file());
        runtime.stop().await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
    async fn real_headscale_registers_durable_tagged_hub_once() {
        use std::{env, os::unix::fs::symlink};
        let source = env::var("PROT_HEADSCALE_BINARY").expect("set PROT_HEADSCALE_BINARY");
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let free_addr = || {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            addr.to_string()
        };
        let listen = free_addr();
        let mut config = Config::for_server(format!(
            "http://control.example:{}",
            listen.rsplit_once(':').unwrap().1
        ));
        config.listen = listen;
        config.metrics_listen = free_addr();
        config.grpc_listen = free_addr();
        config.embedded_derp = false;
        config.public_derp = true;
        config.save(home).unwrap();
        let binary = home.join(format!(
            "bin/headscale-{}",
            protectorate_tailnet::HEADSCALE_VERSION
        ));
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        symlink(source, &binary).unwrap();
        let first = Runtime::start_if_configured(home).await.unwrap().unwrap();
        let state_path = home.join("tailnet/tsnet-hub/join.json");
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        let node_key = state["nodeKey"].as_str().unwrap();
        let disco_key = state["discoKey"].as_str().unwrap();
        assert!(node_key.starts_with("nodekey:"));
        assert!(disco_key.starts_with("discokey:"));
        assert_eq!(
            DiscoIdentity::open(home.join("tailnet/tsnet-hub"))
                .unwrap()
                .public_key()
                .to_string(),
            disco_key
        );
        let nodes = first.admin().nodes().unwrap();
        let hub = nodes.iter().find(|n| n.node_key == node_key).unwrap();
        assert!(hub.tags.iter().any(|tag| tag == TAG_HUB));
        let id = hub.id;
        first.stop().await.unwrap();
        let second = Runtime::start_if_configured(home).await.unwrap().unwrap();
        let nodes = second.admin().nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, id);
        assert_eq!(nodes[0].node_key, node_key);
        second.stop().await.unwrap();
        fs::remove_file(&state_path).unwrap();
        let recovered = Runtime::start_if_configured(home).await.unwrap().unwrap();
        let nodes = recovered.admin().nodes().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, id);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&state_path).unwrap()).unwrap()
                ["nodeKey"],
            node_key
        );
        recovered.stop().await.unwrap();
    }
}
