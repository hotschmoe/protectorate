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
    let _ = rustls::crypto::ring::default_provider().install_default();
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
