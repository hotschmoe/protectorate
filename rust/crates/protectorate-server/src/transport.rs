//! Bounded TLS acceptor and trusted transport peer propagation.
use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    serve::{IncomingStream, Listener},
    Router,
};
use std::{
    fs::File,
    future::pending,
    io::{self, BufReader},
    net::{IpAddr, SocketAddr},
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore},
};
use tokio_rustls::{rustls, server::TlsStream, TlsAcceptor};

const MAX_HANDSHAKES: usize = 32;
const MAX_CONNECTIONS: usize = 256;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PEM_BYTES: u64 = 1024 * 1024;

pub(crate) fn tls_config(cert: &Path, key: &Path) -> io::Result<Arc<rustls::ServerConfig>> {
    ensure_provider();
    let cert = private_file(cert, false)?;
    let key = private_file(key, true)?;
    let mut cert = BufReader::new(cert);
    let certs = rustls_pemfile::certs(&mut cert).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "TLS certificate file is empty",
        ));
    }
    let mut key = BufReader::new(key);
    let key = rustls_pemfile::private_key(&mut key)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "TLS private key is missing"))?;
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn ensure_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn private_file(path: &Path, key: bool) -> io::Result<File> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?
    };
    #[cfg(not(unix))]
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_PEM_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TLS PEM must be a regular file under 1 MiB",
        ));
    }
    #[cfg(unix)]
    if key {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "TLS key must be readable only by its owner",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = key;
    Ok(file)
}

pub(crate) fn load_ca(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let file = private_file(path, false)?;
    let mut pem = Vec::new();
    file.take(MAX_PEM_BYTES + 1).read_to_end(&mut pem)?;
    if pem.len() as u64 > MAX_PEM_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TLS CA file exceeds 1 MiB",
        ));
    }
    Ok(pem)
}

pub(crate) struct TlsListener {
    local: SocketAddr,
    ready: mpsc::Receiver<(TlsIo, SocketAddr)>,
}

impl TlsListener {
    pub(crate) fn new(
        listener: TcpListener,
        config: Arc<rustls::ServerConfig>,
    ) -> io::Result<Self> {
        let local = listener.local_addr()?;
        let (tx, ready) = mpsc::channel(MAX_HANDSHAKES);
        let connections = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let handshakes = Arc::new(Semaphore::new(MAX_HANDSHAKES));
        let acceptor = TlsAcceptor::from(config);
        tokio::spawn(async move {
            loop {
                let connection = tokio::select! {
                    _ = tx.closed() => break,
                    result = Arc::clone(&connections).acquire_owned() => match result { Ok(permit) => permit, Err(_) => break },
                };
                let handshake = tokio::select! {
                    _ = tx.closed() => break,
                    result = Arc::clone(&handshakes).acquire_owned() => match result { Ok(permit) => permit, Err(_) => break },
                };
                let (stream, peer) = tokio::select! {
                    _ = tx.closed() => break,
                    result = listener.accept() => match result {
                        Ok(pair) => pair,
                        Err(error) => {
                            eprintln!("TLS accept: {error}");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            continue;
                        }
                    },
                };
                let acceptor = acceptor.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result =
                        tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await;
                    drop(handshake);
                    if let Ok(Ok(stream)) = result {
                        let _ = tx
                            .send((
                                TlsIo {
                                    stream,
                                    _permit: connection,
                                },
                                peer,
                            ))
                            .await;
                    }
                });
            }
        });
        Ok(Self { local, ready })
    }
}

impl Listener for TlsListener {
    type Io = TlsIo;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(pair) => pair,
            None => pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local)
    }
}

pub(crate) struct TlsIo {
    stream: TlsStream<TcpStream>,
    _permit: OwnedSemaphorePermit,
}

impl AsyncRead for TlsIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[derive(Clone)]
pub(crate) struct Peer(pub SocketAddr);

#[derive(Clone, Copy, Debug)]
pub(crate) struct TailnetPeer {
    pub remote: SocketAddr,
    pub peer_id: u64,
    pub generation: u64,
}

fn hub_tailnet_identity(
    snapshot: &protectorate_tailnet_runtime::MapSnapshot,
    peer: TailnetPeer,
) -> Option<protectorate_wire::TailnetIdentity> {
    if snapshot.generation != peer.generation {
        return None;
    }
    let node = snapshot.map.peers.get(&peer.peer_id)?;
    if !node
        .tags
        .iter()
        .any(|tag| tag == "tag:hub" || tag == "tag:client")
    {
        return None;
    }
    let name = node.name.trim_end_matches('.');
    (!name.is_empty()).then(|| protectorate_wire::TailnetIdentity(name.to_owned()))
}

/// A virtual HTTP/1.1 listener carrying streams that the packet engine has
/// already authenticated and checked against the current tailnet map.
pub(crate) struct TailnetListener {
    local: SocketAddr,
    accepted: mpsc::Receiver<protectorate_tailnet_stream::Accepted>,
}

impl TailnetListener {
    pub(crate) fn new(
        local: SocketAddr,
        accepted: mpsc::Receiver<protectorate_tailnet_stream::Accepted>,
    ) -> Self {
        Self { local, accepted }
    }
}

impl Listener for TailnetListener {
    type Io = DuplexStream;
    type Addr = TailnetPeer;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.accepted.recv().await {
            Some(accepted) => (
                accepted.stream,
                TailnetPeer {
                    remote: accepted.id.remote,
                    peer_id: accepted.peer_id,
                    generation: accepted.generation,
                },
            ),
            None => pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(TailnetPeer {
            remote: self.local,
            peer_id: 0,
            generation: 0,
        })
    }
}

impl<'a> axum::extract::connect_info::Connected<IncomingStream<'a, TailnetListener>> for Peer {
    fn connect_info(stream: IncomingStream<'a, TailnetListener>) -> Self {
        Self(stream.remote_addr().remote)
    }
}

impl<'a> axum::extract::connect_info::Connected<IncomingStream<'a, TailnetListener>>
    for TailnetPeer
{
    fn connect_info(stream: IncomingStream<'a, TailnetListener>) -> Self {
        *stream.remote_addr()
    }
}

impl<'a> axum::extract::connect_info::Connected<IncomingStream<'a, TlsListener>> for Peer {
    fn connect_info(stream: IncomingStream<'a, TlsListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

async fn insert_peer(mut request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<Peer>>()
        .map(|info| info.0 .0)
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<TailnetPeer>>()
                .map(|info| info.0.remote)
        })
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0)
        });
    if let Some(peer) = peer {
        request.extensions_mut().insert(peer);
    }
    next.run(request).await
}

pub(crate) fn peer_router(router: Router) -> Router {
    router.layer(middleware::from_fn(insert_peer))
}

/// Serves the same authenticated router over the joined node's virtual
/// tailnet TCP listener. The runtime owns map freshness and packet identity.
pub(crate) struct TailnetHttp {
    runtime: Arc<protectorate_tailnet_runtime::Runtime>,
    shutdown: watch::Sender<bool>,
    server: tokio::task::JoinHandle<io::Result<()>>,
}

impl TailnetHttp {
    pub(crate) fn start(directory: &Path, port: u16, router: Router) -> io::Result<Self> {
        let mut runtime = protectorate_tailnet_runtime::Runtime::start(directory, port)?;
        let accepted = runtime
            .take_accepted()
            .ok_or_else(|| io::Error::other("tailnet accepted receiver missing"))?;
        let listener = TailnetListener::new(
            SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), port),
            accepted,
        );
        let authorization = runtime.authorization();
        let router = router.layer(middleware::from_fn(
            move |mut request: Request, next: Next| {
                let authorization = authorization.clone();
                async move {
                    let identity = request
                        .extensions()
                        .get::<ConnectInfo<TailnetPeer>>()
                        .and_then(|info| {
                            let snapshot = authorization.current_map()?;
                            hub_tailnet_identity(&snapshot, info.0)
                        });
                    if let Some(identity) = identity {
                        request.extensions_mut().insert(identity);
                        next.run(request).await
                    } else {
                        StatusCode::FORBIDDEN.into_response()
                    }
                }
            },
        ));
        let (shutdown, mut stopping) = watch::channel(false);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<TailnetPeer>(),
            )
            .with_graceful_shutdown(async move {
                while !*stopping.borrow() && stopping.changed().await.is_ok() {}
            })
            .await
        });
        Ok(Self {
            runtime: Arc::new(runtime),
            shutdown,
            server,
        })
    }

    /// The caller may use the same joined runtime for hub-to-node dials.
    pub(crate) fn runtime(&self) -> Arc<protectorate_tailnet_runtime::Runtime> {
        Arc::clone(&self.runtime)
    }

    pub(crate) fn start_node(directory: &Path, port: u16, router: Router) -> io::Result<Self> {
        let mut runtime = protectorate_tailnet_runtime::Runtime::start(directory, port)?;
        let accepted = runtime
            .take_accepted()
            .ok_or_else(|| io::Error::other("tailnet accepted receiver missing"))?;
        let authorization = runtime.authorization();
        let listener = TailnetListener::new(
            SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), port),
            accepted,
        );
        let admission: PeerAdmission = Arc::new(move |generation, peer_id| {
            authorization.peer_has_tag(generation, peer_id, "tag:hub")
        });
        let router = guard_tailnet_router(router, admission);
        let (shutdown, mut stopping) = watch::channel(false);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<TailnetPeer>(),
            )
            .with_graceful_shutdown(async move {
                while !*stopping.borrow() && stopping.changed().await.is_ok() {}
            })
            .await
        });
        Ok(Self {
            runtime: Arc::new(runtime),
            shutdown,
            server,
        })
    }

    pub(crate) async fn stop(self) -> io::Result<()> {
        let _ = self.shutdown.send(true);
        self.runtime.shutdown().await;
        let serve = self.server.await.map_err(io::Error::other)?;
        serve
    }
}

type PeerAdmission = Arc<dyn Fn(u64, u64) -> bool + Send + Sync>;

fn guard_tailnet_router(router: Router, admission: PeerAdmission) -> Router {
    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let admission = admission.clone();
        async move {
            let allowed = request
                .extensions()
                .get::<ConnectInfo<TailnetPeer>>()
                .is_some_and(|peer| admission(peer.0.generation, peer.0.peer_id));
            if allowed {
                next.run(request).await
            } else {
                StatusCode::FORBIDDEN.into_response()
            }
        }
    }))
}

pub(crate) fn reachable_addr(listen: SocketAddr) -> SocketAddr {
    let ip = match listen.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, listen.port())
}

/// Probe a child over the same TLS certificate it serves. A changed or invalid
/// certificate fails health confirmation and triggers guardian rollback.
pub(crate) async fn hub_health(
    listen: SocketAddr,
    cert: &Path,
    token: &str,
    expected: &str,
) -> bool {
    ensure_provider();
    if token.contains(['\r', '\n']) {
        return false;
    }
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut pem = BufReader::new(private_file(cert, false)?);
        let certs = rustls_pemfile::certs(&mut pem).collect::<Result<Vec<_>, _>>()?;
        let mut roots = rustls::RootCertStore::empty();
        for cert in certs {
            roots.add(cert).map_err(io::Error::other)?;
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let peer = reachable_addr(listen);
        let ip = peer.ip();
        let stream = TcpStream::connect(peer).await?;
        let server_name = rustls::pki_types::ServerName::IpAddress(ip.into());
        let mut stream = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(server_name, stream)
            .await?;
        let request = format!(
            "POST /protectorate.v1.HubService/GetStatus HTTP/1.1\r\nHost: {ip}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        );
        stream.write_all(request.as_bytes()).await?;
        let mut response = Vec::new();
        stream.take(64 * 1024).read_to_end(&mut response).await?;
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing HTTP response"))?;
        if !response.starts_with(b"HTTP/1.1 200 ") {
            return Ok::<bool, io::Error>(false);
        }
        let body = &response[header_end + 4..];
        let status: serde_json::Value = serde_json::from_slice(body)?;
        Ok(status["version"] == expected)
    })
    .await;
    matches!(result, Ok(Ok(true)))
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn hub_tailnet_identity_requires_current_tagged_peer() {
        let mut map = protectorate_tailnet_map::Netmap::default();
        let node = |name: &str, tags: &[&str]| {
            serde_json::from_value::<protectorate_tailnet_map::Node>(serde_json::json!({
                "ID": 2,
                "Key": "nodekey:test",
                "Name": name,
                "Tags": tags,
            }))
            .unwrap()
        };
        map.peers
            .insert(2, node("client.example.", &["tag:client"]));
        let peer = TailnetPeer {
            remote: "100.64.0.2:40000".parse().unwrap(),
            peer_id: 2,
            generation: 7,
        };
        let snapshot = |map| protectorate_tailnet_runtime::MapSnapshot {
            generation: 7,
            map: Arc::new(map),
        };
        assert_eq!(
            hub_tailnet_identity(&snapshot(map.clone()), peer)
                .unwrap()
                .0,
            "client.example"
        );
        assert!(hub_tailnet_identity(
            &snapshot(map.clone()),
            TailnetPeer {
                generation: 8,
                ..peer
            }
        )
        .is_none());
        assert!(
            hub_tailnet_identity(&snapshot(map.clone()), TailnetPeer { peer_id: 3, ..peer })
                .is_none()
        );
        map.peers.insert(2, node("node.example.", &["tag:node"]));
        assert!(hub_tailnet_identity(&snapshot(map), peer).is_none());
    }

    #[tokio::test]
    async fn tailnet_guard_rechecks_peer_on_each_keep_alive_request() {
        use axum::routing::get;
        use protectorate_tailnet_stream::Accepted;
        use protectorate_tailnet_tcp::ConnectionId;
        use std::sync::atomic::{AtomicBool, Ordering};
        let allowed = Arc::new(AtomicBool::new(true));
        let check = Arc::clone(&allowed);
        let admission: PeerAdmission = Arc::new(move |generation, peer_id| {
            check.load(Ordering::Acquire) && generation == 7 && peer_id == 2
        });
        let router = guard_tailnet_router(
            Router::new().route("/ok", get(|| async { "OK" })),
            admission,
        );
        let (tx, accepted) = mpsc::channel(4);
        let listener = TailnetListener::new("100.64.0.1:7475".parse().unwrap(), accepted);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<TailnetPeer>(),
            )
            .await
        });
        async fn send_connection(tx: &mpsc::Sender<Accepted>, peer_id: u64) -> DuplexStream {
            let (client, stream) = tokio::io::duplex(4096);
            tx.send(Accepted {
                id: ConnectionId {
                    local: "100.64.0.1:7475".parse().unwrap(),
                    remote: "100.64.0.2:49152".parse().unwrap(),
                },
                peer_id,
                generation: 7,
                stream,
            })
            .await
            .unwrap();
            client
        }
        async fn request(stream: &mut DuplexStream) -> Vec<u8> {
            stream
                .write_all(b"GET /ok HTTP/1.1\r\nHost: node\r\n\r\n")
                .await
                .unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let mut chunk = [0u8; 512];
                    let size = stream.read(&mut chunk).await.unwrap();
                    assert!(size > 0);
                    response.extend_from_slice(&chunk[..size]);
                    if let Some(header_end) =
                        response.windows(4).position(|window| window == b"\r\n\r\n")
                    {
                        let header_end = header_end + 4;
                        let headers = String::from_utf8_lossy(&response[..header_end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if response.len() >= header_end + length {
                            return;
                        }
                    }
                }
            })
            .await
            .unwrap();
            response
        }
        let mut wrong = send_connection(&tx, 3).await;
        assert!(request(&mut wrong).await.starts_with(b"HTTP/1.1 403 "));
        let mut tagged = send_connection(&tx, 2).await;
        assert!(request(&mut tagged).await.starts_with(b"HTTP/1.1 200 "));
        allowed.store(false, Ordering::Release);
        assert!(request(&mut tagged).await.starts_with(b"HTTP/1.1 403 "));
        server.abort();
    }

    #[tokio::test]
    async fn virtual_tailnet_http_keeps_node_bearer_auth() {
        use protectorate_tailnet_stream::Accepted;
        use protectorate_tailnet_tcp::ConnectionId;
        let (tx, accepted) = mpsc::channel(2);
        let local: SocketAddr = "100.64.0.1:7475".parse().unwrap();
        let remote: SocketAddr = "100.64.0.2:41000".parse().unwrap();
        let listener = TailnetListener::new(local, accepted);
        let agent = protectorate_node::Agent::new(Arc::new(Metrics), "secret");
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                peer_router(agent.router().unwrap()).into_make_service_with_connect_info::<Peer>(),
            )
            .await
        });
        async fn request(
            tx: &mpsc::Sender<Accepted>,
            local: SocketAddr,
            remote: SocketAddr,
            auth: &str,
        ) -> Vec<u8> {
            let (mut client, service) = tokio::io::duplex(64 * 1024);
            tx.send(Accepted {
                id: ConnectionId { local, remote },
                peer_id: 2,
                generation: 1,
                stream: service,
            })
            .await
            .unwrap();
            let request = format!(
                "POST /protectorate.v1.NodeAgentService/GetMetrics HTTP/1.1\r\nHost: node\r\nAuthorization: Bearer {auth}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            client.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response))
                .await
                .unwrap()
                .unwrap();
            response
        }
        let denied = request(&tx, local, remote, "wrong").await;
        assert!(
            denied.starts_with(b"HTTP/1.1 401 "),
            "{}",
            String::from_utf8_lossy(&denied)
        );
        let allowed = request(&tx, local, remote, "secret").await;
        assert!(
            allowed.starts_with(b"HTTP/1.1 200 "),
            "{}",
            String::from_utf8_lossy(&allowed)
        );
        drop(tx);
        server.abort();
    }

    #[tokio::test]
    async fn virtual_tailnet_http_keeps_hub_owner_auth() {
        use protectorate_tailnet_stream::Accepted;
        use protectorate_tailnet_tcp::ConnectionId;
        let dir = tempfile::tempdir().unwrap();
        let hub =
            protectorate_hub::Hub::open(dir.path().join("hub"), "owner-secret", "test").unwrap();
        let (tx, accepted) = mpsc::channel(2);
        let local: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let remote: SocketAddr = "100.64.0.2:41000".parse().unwrap();
        let listener = TailnetListener::new(local, accepted);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                peer_router(hub.router().unwrap()).into_make_service_with_connect_info::<Peer>(),
            )
            .await
        });
        for (token, expected) in [
            ("wrong", b"HTTP/1.1 401 ".as_slice()),
            ("owner-secret", b"HTTP/1.1 200 ".as_slice()),
        ] {
            let (mut client, service) = tokio::io::duplex(64 * 1024);
            tx.send(Accepted {
                id: ConnectionId { local, remote },
                peer_id: 2,
                generation: 1,
                stream: service,
            })
            .await
            .unwrap();
            let request = format!(
                "POST /protectorate.v1.HubService/GetStatus HTTP/1.1\r\nHost: hub\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            client.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response))
                .await
                .unwrap()
                .unwrap();
            assert!(
                response.starts_with(expected),
                "{}",
                String::from_utf8_lossy(&response)
            );
        }
        drop(tx);
        server.abort();
    }

    struct Metrics;
    impl protectorate_node::MetricsCollector for Metrics {
        fn collect(&self) -> io::Result<protectorate_node_metrics::NodeMetrics> {
            Ok(protectorate_node_metrics::NodeMetrics::default())
        }
    }

    #[tokio::test]
    async fn private_ca_remote_node_metrics_and_file_relay() {
        let dir = tempfile::tempdir().unwrap();
        let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let cert_path = dir.path().join("node-cert.pem");
        let key_path = dir.path().join("node-key.pem");
        fs::write(&cert_path, generated.cert.pem()).unwrap();
        fs::write(&key_path, generated.signing_key.serialize_pem()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let tls = tls_config(&cert_path, &key_path).unwrap();
        let node_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let node_addr = node_listener.local_addr().unwrap();
        let files = dir.path().join("files");
        let agent = protectorate_node::Agent::new(Arc::new(Metrics), "node-secret")
            .with_files(&files, false)
            .unwrap();
        let node_task = tokio::spawn(async move {
            axum::serve(
                TlsListener::new(node_listener, tls).unwrap(),
                peer_router(agent.router().unwrap()).into_make_service_with_connect_info::<Peer>(),
            )
            .await
            .unwrap();
        });
        let endpoint = format!("https://{node_addr}");
        let wrong = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let wrong_roots =
            protectorate_connect_client::TrustRoots::from_pem(wrong.cert.pem().as_bytes()).unwrap();
        let wrong_endpoint = endpoint.clone();
        let denied = tokio::task::spawn_blocking(move || {
            protectorate_connect_client::Client::new_with_trust(
                &wrong_endpoint,
                "node-secret",
                Duration::from_secs(3),
                Some(&wrong_roots),
            )
            .unwrap()
            .call(
                "/protectorate.v1.NodeAgentService/GetMetrics",
                &serde_json::json!({}),
            )
        })
        .await
        .unwrap();
        assert!(denied.is_err());
        let roots =
            protectorate_connect_client::TrustRoots::from_pem(generated.cert.pem().as_bytes())
                .unwrap();
        let hub = protectorate_hub::Hub::open(dir.path().join("hub"), "owner-secret", "test")
            .unwrap()
            .with_node_trust(roots)
            .with_node_token("node-secret".into())
            .unwrap();
        hub.inventory
            .add(
                protectorate_inventory::Node {
                    name: "peer".into(),
                    address: endpoint.clone(),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        let monitor = Arc::clone(&hub.node_monitor);
        tokio::task::spawn_blocking(move || monitor.poll_node("peer"))
            .await
            .unwrap();
        let status = hub.node_monitor.status("peer", &endpoint, false);
        assert_eq!(status["freshness"], "NODE_FRESHNESS_FRESH", "{status}");
        let hub_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hub_url = format!("http://{}", hub_listener.local_addr().unwrap());
        let hub_task = tokio::spawn(async move {
            axum::serve(hub_listener, hub.router().unwrap())
                .await
                .unwrap();
        });
        let reply = tokio::task::spawn_blocking(move || {
            let client = protectorate_connect_client::Client::new_with_trust(
                &hub_url,
                "owner-secret",
                Duration::from_secs(5),
                None,
            )
            .unwrap();
            client.call(
                "/protectorate.v1.NodeService/PutFile",
                &serde_json::json!({
                    "name":"peer", "transferId":"remote-ca-001", "path":"from-hub",
                    "kind":"FILE_KIND_FILE", "mode":384, "offset":"0",
                    "chunk":{"data":"YWJj","rawSize":3}
                }),
            )?;
            client.call(
                "/protectorate.v1.NodeService/PutFile",
                &serde_json::json!({
                    "name":"peer", "transferId":"remote-ca-001", "offset":"3", "last":true,
                    "trailer":{"size":"3","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}
                }),
            )
        })
        .await
        .unwrap();
        assert_eq!(reply.unwrap()["done"], true);
        assert_eq!(fs::read(files.join("from-hub")).unwrap(), b"abc");
        node_task.abort();
        hub_task.abort();
    }

    #[tokio::test]
    async fn https_hub_enforces_bearer_and_allows_lan_pairing() {
        let dir = tempfile::tempdir().unwrap();
        let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");
        fs::write(&cert_path, generated.cert.pem()).unwrap();
        fs::write(&key_path, generated.signing_key.serialize_pem()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let config = tls_config(&cert_path, &key_path).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hub = protectorate_hub::Hub::open(dir.path(), "secret", "trial")
            .unwrap()
            .with_tls_device_auth()
            .with_trusted_proxies(vec!["127.0.0.1".parse().unwrap()]);
        let router = peer_router(hub.router().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                TlsListener::new(listener, config).unwrap(),
                router.into_make_service_with_connect_info::<Peer>(),
            )
            .await
            .unwrap();
        });
        async fn call(
            addr: SocketAddr,
            cert: &Path,
            path: &str,
            auth: &str,
            body: &str,
        ) -> Vec<u8> {
            let mut roots = rustls::RootCertStore::empty();
            let mut pem = BufReader::new(File::open(cert).unwrap());
            roots
                .add(rustls_pemfile::certs(&mut pem).next().unwrap().unwrap())
                .unwrap();
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let tcp = TcpStream::connect(addr).await.unwrap();
            let mut tls = tokio_rustls::TlsConnector::from(Arc::new(config))
                .connect(
                    rustls::pki_types::ServerName::IpAddress(addr.ip().into()),
                    tcp,
                )
                .await
                .unwrap();
            let request = format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            tls.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(3), tls.read_to_end(&mut response))
                .await
                .unwrap()
                .unwrap();
            response
        }
        let status = "/protectorate.v1.HubService/GetStatus";
        let valid = call(
            addr,
            &cert_path,
            status,
            "Authorization: Bearer secret\r\n",
            "{}",
        )
        .await;
        assert!(
            valid.starts_with(b"HTTP/1.1 200 "),
            "{}",
            String::from_utf8_lossy(&valid)
        );
        let endpoint = format!("https://{addr}");
        let cert_for_client = cert_path.clone();
        let client_status = tokio::task::spawn_blocking(move || {
            protectorate_connect_client::Client::new_with_ca(
                &endpoint,
                "secret",
                Duration::from_secs(3),
                Some(&cert_for_client),
            )
            .unwrap()
            .call(status, &serde_json::json!({}))
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(client_status["version"], "trial");
        let endpoint = format!("https://{addr}");
        let pem = fs::read(&cert_path).unwrap();
        let inline_status = tokio::task::spawn_blocking(move || {
            protectorate_connect_client::Client::new_with_ca_pem(
                &endpoint,
                "secret",
                Duration::from_secs(3),
                Some(&pem),
            )
            .unwrap()
            .call(status, &serde_json::json!({}))
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(inline_status["version"], "trial");
        let missing = call(addr, &cert_path, status, "", "{}").await;
        assert!(!missing.starts_with(b"HTTP/1.1 200 "));
        let wrong = call(
            addr,
            &cert_path,
            status,
            "Authorization: Bearer wrong\r\n",
            "{}",
        )
        .await;
        assert!(!wrong.starts_with(b"HTTP/1.1 200 "));
        let pair = call(
            addr,
            &cert_path,
            "/protectorate.v1.DeviceService/StartDeviceAuth",
            "X-Forwarded-For: 8.8.8.8, 192.168.1.5\r\n",
            r#"{"name":"test phone","platform":"android"}"#,
        )
        .await;
        assert!(
            pair.starts_with(b"HTTP/1.1 200 "),
            "{}",
            String::from_utf8_lossy(&pair)
        );
        let pending = call(
            addr,
            &cert_path,
            "/protectorate.v1.DeviceService/ListDeviceAuth",
            "Authorization: Bearer secret\r\n",
            "{}",
        )
        .await;
        assert!(
            String::from_utf8_lossy(&pending).contains("\"requestingIp\":\"192.168.1.5\""),
            "{}",
            String::from_utf8_lossy(&pending)
        );
        assert!(hub_health(addr, &cert_path, "secret", "trial").await);
        assert!(!hub_health(addr, &cert_path, "wrong", "trial").await);
        task.abort();
    }

    #[test]
    fn tls_key_must_be_private_and_not_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("cert");
        let key = dir.path().join("key");
        fs::write(&cert, "bad").unwrap();
        fs::write(&key, "bad").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(
                tls_config(&cert, &key).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            ));
            fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
            let link = dir.path().join("key-link");
            symlink(&key, &link).unwrap();
            assert!(tls_config(&cert, &link).is_err());
        }
    }
}
