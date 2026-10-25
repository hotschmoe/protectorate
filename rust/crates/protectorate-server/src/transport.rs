//! Bounded TLS acceptor and trusted transport peer propagation.
use axum::{
    extract::{ConnectInfo, Request},
    middleware::{self, Next},
    response::Response,
    serve::{IncomingStream, Listener},
    Router,
};
use std::{
    future::pending,
    io,
    net::SocketAddr,
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

pub(crate) struct TlsListener {
    local: SocketAddr,
    ready: mpsc::Receiver<(TlsIo, SocketAddr)>,
}

async fn next_listener(swap: &mut Option<mpsc::Receiver<TcpListener>>) -> Option<TcpListener> {
    match swap {
        Some(swap) => swap.recv().await,
        None => pending().await,
    }
}

impl TlsListener {
    pub(crate) fn new(
        listener: TcpListener,
        config: Arc<rustls::ServerConfig>,
    ) -> io::Result<Self> {
        Self::new_rebindable(listener, config, None)
    }

    /// Like `new`, and accepts on each listener sent through `swap` from then
    /// on, so a node can follow its machine's address without a restart.
    /// Connections already accepted keep running.
    pub(crate) fn new_rebindable(
        mut listener: TcpListener,
        config: Arc<rustls::ServerConfig>,
        mut swap: Option<mpsc::Receiver<TcpListener>>,
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
                    next = next_listener(&mut swap) => {
                        match next {
                            Some(next) => listener = next,
                            None => swap = None,
                        }
                        continue;
                    }
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
                        let client_cert = stream
                            .get_ref()
                            .1
                            .peer_certificates()
                            .and_then(|chain| chain.first())
                            .map(|cert| Arc::<[u8]>::from(cert.as_ref()));
                        let _ = tx
                            .send((
                                TlsIo {
                                    stream,
                                    client_cert,
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
    client_cert: Option<Arc<[u8]>>,
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
pub(crate) struct Peer(pub SocketAddr, pub Option<Arc<[u8]>>);

impl<'a> axum::extract::connect_info::Connected<IncomingStream<'a, TlsListener>> for Peer {
    fn connect_info(stream: IncomingStream<'a, TlsListener>) -> Self {
        Self(*stream.remote_addr(), stream.io().client_cert.clone())
    }
}

async fn insert_peer(mut request: Request, next: Next) -> Response {
    let tls = request
        .extensions()
        .get::<ConnectInfo<Peer>>()
        .map(|info| (info.0 .0, info.0 .1.clone()));
    let plain = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let peer = match (tls, plain) {
        (Some((addr, client_cert)), _) => Some(protectorate_wire::Peer {
            addr,
            tls: true,
            client_cert,
        }),
        (None, Some(addr)) => Some(protectorate_wire::Peer {
            addr,
            tls: false,
            client_cert: None,
        }),
        _ => None,
    };
    if let Some(peer) = peer {
        request.extensions_mut().insert(peer);
    }
    next.run(request).await
}

pub(crate) fn peer_router(router: Router) -> Router {
    router.layer(middleware::from_fn(insert_peer))
}
