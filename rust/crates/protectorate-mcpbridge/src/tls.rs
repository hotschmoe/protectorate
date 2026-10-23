//! Bounded TLS listener for remote MCP access.
use crate::{http::router, Bridge, Code, HttpConfig, Result, RpcError};
use axum::serve::Listener;
use std::{
    fs::File,
    future::{pending, Future},
    io::{self, BufReader},
    net::SocketAddr,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
};
use tokio_rustls::{rustls, server::TlsStream, TlsAcceptor};

const MAX_HANDSHAKES: usize = 32;
const MAX_CONNECTIONS: usize = 256;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PEM_BYTES: u64 = 1024 * 1024;

#[cfg(windows)]
#[path = "tls_windows.rs"]
mod windows;

pub fn tls_config(cert: &Path, key: &Path) -> Result<Arc<rustls::ServerConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = private_file(cert, false).map_err(|error| tls_file_error("certificate", error))?;
    let key = private_file(key, true).map_err(|error| tls_file_error("key", error))?;
    let certs = rustls_pemfile::certs(&mut BufReader::new(cert))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| tls_file_error("certificate", error))?;
    if certs.is_empty() {
        return Err(RpcError::invalid("TLS certificate file is empty"));
    }
    let key = rustls_pemfile::private_key(&mut BufReader::new(key))
        .map_err(|error| tls_file_error("key", error))?
        .ok_or_else(|| RpcError::invalid("TLS private key is missing"))?;
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| RpcError::invalid(format!("TLS certificate and key: {error}")))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn tls_file_error(kind: &str, error: io::Error) -> RpcError {
    RpcError::invalid(format!("TLS {kind} file: {error}"))
}

fn private_file(path: &Path, key: bool) -> io::Result<File> {
    #[cfg(windows)]
    let file = windows::open_checked(path, key)?;
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?
    };
    #[cfg(not(any(unix, windows)))]
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
    Ok(file)
}

pub async fn bind_tls(listen: &str, config: Arc<rustls::ServerConfig>) -> Result<TlsListener> {
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|error| RpcError::new(Code::Unavailable, format!("bind {listen}: {error}")))?;
    TlsListener::new(listener, config)
        .map_err(|error| RpcError::new(Code::Internal, format!("start MCP TLS: {error}")))
}

pub async fn serve_tls(
    listener: TlsListener,
    bridge: Arc<Bridge>,
    config: HttpConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    axum::serve(listener, router(bridge, config))
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| RpcError::new(Code::Internal, format!("serve MCP HTTPS: {error}")))
}

pub struct TlsListener {
    local: SocketAddr,
    ready: mpsc::Receiver<(TlsIo, SocketAddr)>,
}

impl TlsListener {
    fn new(listener: TcpListener, config: Arc<rustls::ServerConfig>) -> io::Result<Self> {
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
                            eprintln!("MCP TLS accept: {error}");
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

    pub fn local_addr(&self) -> SocketAddr {
        self.local
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

pub struct TlsIo {
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
