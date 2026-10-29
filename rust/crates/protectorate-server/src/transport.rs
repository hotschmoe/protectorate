//! The proxy listener, which admits only the trusted reverse proxy, and
//! transport peer propagation.
use axum::{
    extract::{ConnectInfo, Request},
    middleware::{self, Next},
    response::Response,
    serve::{IncomingStream, Listener},
    Router,
};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

/// Plain HTTP on a private address for the reverse proxy that terminates TLS.
/// Any other peer is closed before a byte is read, so a client can never send
/// a token to the hub in the clear.
pub(crate) struct ProxyListener {
    listener: TcpListener,
    proxies: Vec<IpAddr>,
}

impl ProxyListener {
    pub(crate) fn new(listener: TcpListener, proxies: &[IpAddr]) -> Self {
        Self {
            listener,
            proxies: proxies.iter().map(|ip| canonical(*ip)).collect(),
        }
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

impl Listener for ProxyListener {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, peer)) if self.proxies.contains(&canonical(peer.ip())) => {
                    return (stream, peer)
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("proxy listener accept: {error}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

#[derive(Clone)]
pub(crate) struct Proxied(pub SocketAddr);

impl<'a> axum::extract::connect_info::Connected<IncomingStream<'a, ProxyListener>> for Proxied {
    fn connect_info(stream: IncomingStream<'a, ProxyListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

async fn insert_peer(mut request: Request, next: Next) -> Response {
    let proxied = request
        .extensions()
        .get::<ConnectInfo<Proxied>>()
        .map(|info| info.0 .0);
    let plain = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let peer = match (proxied, plain) {
        (Some(addr), _) => Some(protectorate_wire::Peer {
            addr,
            proxied: true,
        }),
        (None, Some(addr)) => Some(protectorate_wire::Peer {
            addr,
            proxied: false,
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
