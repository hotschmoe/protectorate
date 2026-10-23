//! WireGuard over authenticated direct UDP endpoints or one verified DERP region.

use protectorate_derp_client::{Client, Config, Event};
use protectorate_tailnet_map::{DerpNode, Netmap, Node};
use protectorate_tailnet_route::{InboundPacket, RouteTable};
use protectorate_tailnet_wg::{Identity, Initiator, Responder, Session};
use socket2::SockRef;
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};
use zeroize::Zeroize;

fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn configure_direct_receive_buffer(socket: &UdpSocket) {
    // The default Linux receive buffer can hold fewer than one virtual TCP
    // window of small WireGuard datagrams once per-packet overhead is counted.
    // The kernel may cap this request; even at the usual 212 KiB cap, Linux
    // doubles the effective SO_RCVBUF value and avoids burst loss seen on nodes.
    if let Err(error) = SockRef::from(socket).set_recv_buffer_size(DIRECT_RECV_BUFFER) {
        eprintln!("tailnet direct UDP receive buffer: {error}");
    }
}
fn wg_error(error: protectorate_tailnet_wg::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn node_key(raw: &str) -> io::Result<[u8; 32]> {
    let raw = raw
        .strip_prefix("nodekey:")
        .ok_or_else(|| invalid("node key prefix"))?;
    if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("node key hex"));
    }
    let mut key = [0; 32];
    for (index, pair) in raw.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
        key[index] = (digit(pair[0]).ok_or_else(|| invalid("node key hex"))? << 4)
            | digit(pair[1]).ok_or_else(|| invalid("node key hex"))?;
    }
    Ok(key)
}

fn key_text(key: &[u8; 32]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut result = String::from("nodekey:");
    for byte in key {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayTrust {
    PrivatePin(String),
    TailscalePublic,
}

pub fn relay_trust(node: &DerpNode) -> Option<RelayTrust> {
    if let Some(pin) = node
        .cert_name
        .strip_prefix("sha256-raw:")
        .filter(|pin| pin.len() == 64 && pin.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Some(RelayTrust::PrivatePin(format!(
            "sha256-raw:{}",
            pin.to_ascii_lowercase()
        )));
    }
    if !node.cert_name.is_empty() {
        return None;
    }
    let label = node.host_name.strip_suffix(".tailscale.com")?;
    if label.len() <= 4
        || !label.starts_with("derp")
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return None;
    }
    Some(RelayTrust::TailscalePublic)
}

pub fn relay_port(node: &DerpNode) -> Option<u16> {
    match node.derp_port {
        0 if relay_trust(node) == Some(RelayTrust::TailscalePublic) => Some(443),
        0 => None,
        port => Some(port),
    }
}

fn relay_config(
    map: &Netmap,
    addr: SocketAddr,
    ca_der: Option<Vec<u8>>,
) -> io::Result<(i64, Config)> {
    let local = map
        .self_node
        .as_ref()
        .ok_or_else(|| invalid("map has no local node"))?;
    if local.home_derp <= 0 {
        return Err(invalid("local node has no DERP region"));
    }
    let regions = &map
        .derp_map
        .as_ref()
        .ok_or_else(|| invalid("map lacks DERP servers"))?
        .regions;
    let region = regions
        .get(&local.home_derp.to_string())
        .filter(|region| region.region_id == local.home_derp)
        .ok_or_else(|| invalid("DERP region absent"))?;
    let relay = region
        .nodes
        .iter()
        .find(|node| relay_port(node) == Some(addr.port()) && relay_address_matches(node, addr))
        .ok_or_else(|| denied("DERP address is not advertised by current map"))?;
    let trust = relay_trust(relay).ok_or_else(|| {
        invalid("DERP requires a private certificate pin or Tailscale public hostname")
    })?;
    let hostname = if relay.host_name.is_empty() {
        return Err(invalid("DERP hostname missing"));
    } else {
        relay.host_name.clone()
    };
    let mut config = Config::new(addr, hostname);
    config.ca_der = ca_der;
    config.certificate_pin = match trust {
        RelayTrust::PrivatePin(pin) => Some(pin),
        RelayTrust::TailscalePublic => None,
    };
    Ok((local.home_derp, config))
}

fn relay_address_matches(node: &protectorate_tailnet_map::DerpNode, addr: SocketAddr) -> bool {
    let literal_ips: Vec<_> = [node.ipv4.as_str(), node.ipv6.as_str()]
        .into_iter()
        .filter(|raw| !raw.is_empty())
        .collect();
    if !literal_ips.is_empty() {
        return literal_ips
            .iter()
            .any(|raw| raw.parse::<IpAddr>().ok() == Some(addr.ip()));
    }
    // Private DERP maps may publish only HostName. DNS selects the dial IP;
    // TLS still verifies that hostname and the map's exact leaf pin.
    !node.host_name.is_empty()
        && (
            node.host_name.as_str(),
            relay_port(node).unwrap_or_default(),
        )
            .to_socket_addrs()
            .is_ok_and(|mut addresses| addresses.any(|resolved| resolved == addr))
}

fn unique_peer_keys(map: &Netmap) -> io::Result<()> {
    let mut keys = std::collections::BTreeSet::new();
    let self_key = node_key(
        &map.self_node
            .as_ref()
            .ok_or_else(|| invalid("map has no local node"))?
            .key,
    )?;
    for peer in map.peers.values() {
        let key = node_key(&peer.key)?;
        if key == self_key || !keys.insert(key) {
            return Err(denied("two peers claim one node key"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendStatus {
    Sent,
    HandshakeStarted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveStatus {
    /// A checked IP packet delivered from an authenticated peer.
    Packet { peer_id: u64, bytes: Vec<u8> },
    /// DERP acknowledged an exact liveness challenge.
    Pong([u8; 8]),
    /// Handshake, keepalive, or relay status frame; no IP packet.
    Control,
}

/// Lifetime counters for successfully sent or authenticated WireGuard frames.
/// An IP receive is counted only after decrypting and passing the current ACL.
#[derive(Default)]
pub struct TransportCounters {
    direct_ip_sent: AtomicU64,
    direct_ip_received: AtomicU64,
    derp_ip_sent: AtomicU64,
    derp_ip_received: AtomicU64,
    direct_handshakes_sent: AtomicU64,
    derp_handshakes_sent: AtomicU64,
    direct_keepalives_sent: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransportStats {
    pub direct_ip_sent: u64,
    pub direct_ip_received: u64,
    pub derp_ip_sent: u64,
    pub derp_ip_received: u64,
    pub direct_handshakes_sent: u64,
    pub derp_handshakes_sent: u64,
    pub direct_keepalives_sent: u64,
}

#[derive(Clone, Debug, Default)]
pub struct PeerTraffic {
    pub key: String,
    pub last_handshake: Option<SystemTime>,
    pub direct_addr: Option<SocketAddr>,
    pub relay: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    path_updated: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct PeerTelemetry(Arc<std::sync::RwLock<BTreeMap<u64, PeerTraffic>>>);

impl PeerTelemetry {
    pub fn snapshot(&self) -> BTreeMap<u64, PeerTraffic> {
        let mut peers = self
            .0
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        for peer in peers.values_mut() {
            if peer.direct_addr.is_some()
                && peer
                    .path_updated
                    .is_some_and(|at| at.elapsed() >= DIRECT_LIVE_FOR)
            {
                peer.direct_addr = None;
            }
        }
        peers
    }

    fn retain_map(&self, map: &Netmap) {
        let mut peers = self.0.write().unwrap_or_else(|poison| poison.into_inner());
        peers.retain(|id, traffic| {
            map.peers
                .get(id)
                .is_some_and(|node| node.key == traffic.key)
        });
    }

    fn record(&self, id: u64, key: &str, update: impl FnOnce(&mut PeerTraffic)) {
        let mut peers = self.0.write().unwrap_or_else(|poison| poison.into_inner());
        let peer = peers.entry(id).or_default();
        if peer.key != key {
            *peer = PeerTraffic {
                key: key.to_owned(),
                ..PeerTraffic::default()
            };
        }
        update(peer);
    }
}

impl TransportCounters {
    pub fn snapshot(&self) -> TransportStats {
        TransportStats {
            direct_ip_sent: self.direct_ip_sent.load(Ordering::Relaxed),
            direct_ip_received: self.direct_ip_received.load(Ordering::Relaxed),
            derp_ip_sent: self.derp_ip_sent.load(Ordering::Relaxed),
            derp_ip_received: self.derp_ip_received.load(Ordering::Relaxed),
            direct_handshakes_sent: self.direct_handshakes_sent.load(Ordering::Relaxed),
            derp_handshakes_sent: self.derp_handshakes_sent.load(Ordering::Relaxed),
            direct_keepalives_sent: self.direct_keepalives_sent.load(Ordering::Relaxed),
        }
    }

    fn sent(&self, direct: bool, bytes: &[u8]) {
        match (direct, wg_kind(bytes)) {
            (true, Some(1)) => &self.direct_handshakes_sent,
            (false, Some(1)) => &self.derp_handshakes_sent,
            (true, Some(4)) if bytes.len() == 32 => &self.direct_keepalives_sent,
            (true, Some(4)) if bytes.len() > 32 => &self.direct_ip_sent,
            (false, Some(4)) if bytes.len() > 32 => &self.derp_ip_sent,
            _ => return,
        }
        .fetch_add(1, Ordering::Relaxed);
    }
}

fn wg_kind(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}

struct PeerState {
    responder: Responder,
    initiator: Option<Initiator>,
    handshake_started: Option<Instant>,
    session: Option<Session>,
    queued: Option<Vec<u8>>,
}

const MAX_RETURN_FLOWS: usize = 4096;
const PENDING_FLOW_LIFETIME: Duration = Duration::from_secs(15);
const ACTIVE_FLOW_LIFETIME: Duration = Duration::from_secs(120);
const DIRECT_HANDSHAKE_RETRY: Duration = Duration::from_millis(250);
const DIRECT_HANDSHAKE_FALLBACK: Duration = Duration::from_secs(1);
const HANDSHAKE_REFRESH: Duration = Duration::from_secs(3);
const DIRECT_LIVE_FOR: Duration = Duration::from_secs(5);
const DIRECT_REPROBE_INTERVAL: Duration = Duration::from_secs(10);
const DIRECT_PROBE_RESPONSE_WINDOW: Duration = Duration::from_secs(3);
const MAX_DIRECT_PROBES_PER_TICK: usize = 8;
const MAX_DIRECT_PACKET: usize = 4096;
const DIRECT_RECV_BUFFER: usize = 1 << 20;

struct PendingDirect {
    peer_key: [u8; 32],
    handshake: Vec<u8>,
    sent: Instant,
    retried: bool,
}

#[derive(Clone, Copy)]
enum Transport {
    Derp,
    Direct(SocketAddr),
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct FlowKey {
    peer_id: u64,
    local: SocketAddr,
    remote: SocketAddr,
}
#[derive(Clone, Copy)]
enum FlowPhase {
    SynSent { client_seq: u32 },
    SynAckSeen { client_seq: u32, server_seq: u32 },
    Established,
}
struct ReturnFlow {
    peer_key: [u8; 32],
    phase: FlowPhase,
    expires: Instant,
}

pub struct PacketEngine {
    private: [u8; 32],
    public: [u8; 32],
    relay_addr: SocketAddr,
    relay_pin: Option<String>,
    relay_hostname: String,
    region: i64,
    client: Client,
    map: Option<Netmap>,
    routes: Option<RouteTable>,
    peers: BTreeMap<u64, PeerState>,
    return_flows: BTreeMap<FlowKey, ReturnFlow>,
    direct: Option<UdpSocket>,
    direct_recent: BTreeMap<u64, Instant>,
    pending_direct: BTreeMap<u64, PendingDirect>,
    direct_probes: BTreeMap<u64, Instant>,
    counters: Arc<TransportCounters>,
    telemetry: PeerTelemetry,
}

impl Drop for PacketEngine {
    fn drop(&mut self) {
        self.private.zeroize();
    }
}

impl PacketEngine {
    /// `addr` must match the authenticated map's literal IP and port, or
    /// resolve from its HostName when no literal IP is advertised. The TLS
    /// hostname and certificate trust policy are taken from that map.
    pub fn connect(
        private: [u8; 32],
        map: &Netmap,
        addr: SocketAddr,
        ca_der: Option<Vec<u8>>,
    ) -> io::Result<Self> {
        let identity = Identity::from_private(private).map_err(wg_error)?;
        let public = identity.public_key();
        let routes = RouteTable::compile(map).map_err(io::Error::other)?;
        let local = map
            .self_node
            .as_ref()
            .ok_or_else(|| invalid("map has no local node"))?;
        if node_key(&local.key)? != public {
            return Err(denied("map local key differs from node identity"));
        }
        unique_peer_keys(map)?;
        let (region, config) = relay_config(map, addr, ca_der)?;
        let client = Client::connect(&config, private)?;
        if client.public_key() != public {
            return Err(denied("DERP identity differs from WireGuard identity"));
        }
        Ok(Self {
            private,
            public,
            relay_addr: addr,
            relay_pin: config.certificate_pin,
            relay_hostname: config.hostname,
            region,
            client,
            map: Some(map.clone()),
            routes: Some(routes),
            peers: BTreeMap::new(),
            return_flows: BTreeMap::new(),
            direct: None,
            direct_recent: BTreeMap::new(),
            pending_direct: BTreeMap::new(),
            direct_probes: BTreeMap::new(),
            counters: Arc::new(TransportCounters::default()),
            telemetry: PeerTelemetry::default(),
        })
    }

    /// Share counters with the owning runtime across relay reconnects.
    pub fn set_counters(&mut self, counters: Arc<TransportCounters>) {
        self.counters = counters;
    }

    pub fn set_peer_telemetry(&mut self, telemetry: PeerTelemetry) {
        telemetry.retain_map(self.map.as_ref().expect("connected map"));
        self.telemetry = telemetry;
    }

    pub fn peer_traffic(&self) -> BTreeMap<u64, PeerTraffic> {
        self.telemetry.snapshot()
    }

    pub fn transport_stats(&self) -> TransportStats {
        self.counters.snapshot()
    }

    /// Bind the UDP socket before its address is reported to control. The
    /// caller must advertise only a routable address on this bound interface.
    pub fn bind_direct(&mut self, address: SocketAddr) -> io::Result<SocketAddr> {
        if self.direct.is_some() {
            return Err(invalid("direct UDP socket already bound"));
        }
        let socket = UdpSocket::bind(address)?;
        configure_direct_receive_buffer(&socket);
        socket.set_nonblocking(true)?;
        let bound = socket.local_addr()?;
        self.direct = Some(socket);
        Ok(bound)
    }

    /// Move the live UDP socket after an interface change without resetting
    /// authenticated WireGuard sessions. The new address must be published by
    /// the caller after this returns.
    pub fn rebind_direct(&mut self, address: SocketAddr) -> io::Result<SocketAddr> {
        let socket = UdpSocket::bind(address)?;
        configure_direct_receive_buffer(&socket);
        socket.set_nonblocking(true)?;
        let bound = socket.local_addr()?;
        self.direct = Some(socket);
        self.direct_recent.clear();
        self.direct_probes.clear();
        Ok(bound)
    }

    fn direct_endpoint(&self, peer_id: u64) -> Option<SocketAddr> {
        let socket = self.direct.as_ref()?;
        let local = socket.local_addr().ok()?;
        self.peer(peer_id).ok()?.endpoints.iter().find_map(|raw| {
            let endpoint = raw.parse::<SocketAddr>().ok()?;
            (endpoint.is_ipv4() == local.is_ipv4()
                && endpoint.port() != 0
                && !endpoint.ip().is_unspecified()
                && !endpoint.ip().is_multicast())
            .then_some(endpoint)
        })
    }

    fn direct_peer(&self, endpoint: SocketAddr) -> Option<(u64, [u8; 32])> {
        let map = self.map.as_ref()?;
        let mut matches = map.peers.values().filter(|peer| {
            self.peer(peer.id).is_ok()
                && peer
                    .endpoints
                    .iter()
                    .any(|raw| raw.parse::<SocketAddr>().ok() == Some(endpoint))
        });
        let peer = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some((peer.id, node_key(&peer.key).ok()?))
    }

    fn send_direct(&self, peer_id: u64, bytes: &[u8]) -> bool {
        let sent = self
            .direct
            .as_ref()
            .zip(self.direct_endpoint(peer_id))
            .is_some_and(|(socket, endpoint)| {
                socket.send_to(bytes, endpoint).ok() == Some(bytes.len())
            });
        if sent {
            self.counters.sent(true, bytes);
        }
        sent
    }

    fn send_reply(&mut self, key: [u8; 32], bytes: &[u8], via: Transport) -> io::Result<Transport> {
        if let Transport::Direct(endpoint) = via {
            if self
                .direct
                .as_ref()
                .is_some_and(|socket| socket.send_to(bytes, endpoint).ok() == Some(bytes.len()))
            {
                self.counters.sent(true, bytes);
                return Ok(Transport::Direct(endpoint));
            }
        }
        self.send_derp(key, bytes)?;
        Ok(Transport::Derp)
    }

    /// Retry direct handshakes through DERP after a bounded wait and refresh
    /// unanswered initiations on either path. A direct response arriving
    /// after fallback can still finish the same WireGuard exchange.
    pub fn tick_direct(&mut self) -> io::Result<()> {
        let now = Instant::now();
        // A DERP send can succeed while the other member is disconnected. If
        // its response never arrives, TCP retries otherwise encounter the
        // pending initiator forever, even after both relays reconnect.
        let stale: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(id, state)| {
                state
                    .handshake_started
                    .filter(|at| now.duration_since(*at) >= HANDSHAKE_REFRESH)
                    .map(|_| *id)
            })
            .collect();
        for id in stale {
            if let Some(state) = self.peers.get_mut(&id) {
                let queued = state.queued.take();
                state.initiator = None;
                state.handshake_started = None;
                self.pending_direct.remove(&id);
                if let Some(packet) = queued {
                    self.send_ip(&packet)?;
                } else {
                    let destination = node_key(&self.peer(id)?.key)?;
                    self.start_recovery_handshake(id, destination)?;
                }
            }
        }
        let retry: Vec<_> = self
            .pending_direct
            .iter()
            .filter(|(_, pending)| {
                !pending.retried && now.duration_since(pending.sent) >= DIRECT_HANDSHAKE_RETRY
            })
            .map(|(id, _)| *id)
            .collect();
        for id in retry {
            if let Some(handshake) = self
                .pending_direct
                .get(&id)
                .map(|pending| pending.handshake.clone())
            {
                let _ = self.send_direct(id, &handshake);
                if let Some(pending) = self.pending_direct.get_mut(&id) {
                    pending.retried = true;
                }
            }
        }
        let ready: Vec<_> = self
            .pending_direct
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.sent) >= DIRECT_HANDSHAKE_FALLBACK)
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            if let Some(pending) = self.pending_direct.remove(&id) {
                self.direct_recent.remove(&id);
                self.send_derp(pending.peer_key, &pending.handshake)?;
            }
        }
        let candidates: Vec<_> = self
            .peers
            .keys()
            .copied()
            .filter(|id| {
                self.peers
                    .get(id)
                    .and_then(|state| state.session.as_ref())
                    .is_some_and(|session| !session.needs_rekey())
                    && self.direct_endpoint(*id).is_some()
                    && self
                        .direct_recent
                        .get(id)
                        .is_none_or(|seen| now.duration_since(*seen) >= DIRECT_LIVE_FOR)
                    && self
                        .direct_probes
                        .get(id)
                        .is_none_or(|sent| now.duration_since(*sent) >= DIRECT_REPROBE_INTERVAL)
            })
            .take(MAX_DIRECT_PROBES_PER_TICK)
            .collect();
        for id in candidates {
            let probe = self
                .peers
                .get_mut(&id)
                .and_then(|state| state.session.as_mut())
                .filter(|session| !session.needs_rekey())
                .and_then(|session| session.encrypt(&[]).ok());
            if let Some(probe) = probe {
                self.direct_probes.insert(id, now);
                let _ = self.send_direct(id, &probe);
            }
        }
        Ok(())
    }

    /// Process one datagram only when its exact source endpoint belongs to one
    /// current usable peer. WireGuard still authenticates the claimed key.
    /// Invalid datagrams are ignored without affecting the DERP connection.
    pub fn receive_direct(&mut self) -> io::Result<Option<ReceiveStatus>> {
        let Some(socket) = self.direct.as_ref() else {
            return Ok(None);
        };
        let mut data = [0u8; MAX_DIRECT_PACKET];
        let (length, endpoint) = match socket.recv_from(&mut data) {
            Ok(packet) => packet,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(_) => {
                self.direct = None;
                self.direct_recent.clear();
                self.direct_probes.clear();
                return Ok(None);
            }
        };
        let Some((peer_id, key)) = self.direct_peer(endpoint) else {
            return Ok(None);
        };
        match self.process_packet(key, &data[..length], Transport::Direct(endpoint)) {
            Ok((status, authenticated)) => {
                if matches!(status, ReceiveStatus::Packet { .. }) {
                    self.counters
                        .direct_ip_received
                        .fetch_add(1, Ordering::Relaxed);
                }
                if authenticated {
                    self.direct_recent.insert(peer_id, Instant::now());
                    self.pending_direct.remove(&peer_id);
                }
                Ok(Some(status))
            }
            Err(_) => Ok(None),
        }
    }

    /// Replace the current authenticated map. A peer's WireGuard state survives
    /// routine map deltas only while its key, region and exact route remain
    /// valid. Every packet is still checked against the newly compiled filter.
    /// Any validation error clears the prior map and all sessions.
    pub fn replace_map(&mut self, map: &Netmap) -> io::Result<()> {
        let validated = (|| -> io::Result<RouteTable> {
            let local = map
                .self_node
                .as_ref()
                .ok_or_else(|| invalid("map has no local node"))?;
            if node_key(&local.key)? != self.public {
                return Err(denied("map local key changed"));
            }
            unique_peer_keys(map)?;
            let routes = RouteTable::compile(map).map_err(io::Error::other)?;
            let (region, config) = relay_config(map, self.relay_addr, None)?;
            if region != self.region
                || config.certificate_pin != self.relay_pin
                || config.hostname != self.relay_hostname
            {
                return Err(denied("DERP identity changed; reconnect required"));
            }
            if self.map.as_ref().is_some_and(|old| {
                old.session_handle == map.session_handle && map.session_seq < old.session_seq
            }) {
                return Err(denied("map sequence regressed"));
            }
            Ok(routes)
        })();
        let routes = match validated {
            Ok(routes) => routes,
            Err(error) => {
                self.clear_map();
                return Err(error);
            }
        };
        let old = self.map.as_ref();
        let old_routes = self.routes.as_ref();
        let same_poll = old.is_some_and(|old| old.session_handle == map.session_handle);
        let now = SystemTime::now();
        self.peers.retain(|id, state| {
            let old_peer = old.and_then(|old| old.peers.get(id));
            let new_peer = map.peers.get(id);
            let same_identity = old_peer
                .zip(new_peer)
                .is_some_and(|(old, new)| old.key == new.key && old.home_derp == new.home_derp);
            let was_usable = old_peer
                .zip(old_routes)
                .is_some_and(|(peer, routes)| peer_usable(peer, routes, self.region, now));
            let usable = new_peer.is_some_and(|peer| peer_usable(peer, &routes, self.region, now));
            let queued_valid = state.queued.as_deref().is_none_or(|packet| {
                parse_ip(packet).ok().is_some_and(|ip| {
                    map.self_node
                        .as_ref()
                        .is_some_and(|local| node_has_ip(local, ip.source))
                        && routes
                            .route_outbound(ip.destination, now)
                            .is_some_and(|route| route.id == *id)
                })
            });
            same_poll && same_identity && was_usable && usable && queued_valid
        });
        self.return_flows.retain(|key, flow| {
            same_poll
                && flow.expires > Instant::now()
                && map.peers.get(&key.peer_id).is_some_and(|peer| {
                    node_key(&peer.key).ok() == Some(flow.peer_key)
                        && peer_usable(peer, &routes, self.region, now)
                        && node_has_ip(peer, key.remote.ip())
                })
                && map
                    .self_node
                    .as_ref()
                    .is_some_and(|local| node_has_ip(local, key.local.ip()))
                && routes
                    .route_outbound(key.remote.ip(), now)
                    .is_some_and(|peer| peer.id == key.peer_id)
        });
        self.direct_recent.retain(|id, _| {
            map.peers.get(id).is_some_and(|peer| {
                old.and_then(|previous| previous.peers.get(id))
                    .is_some_and(|previous| {
                        previous.key == peer.key && previous.endpoints == peer.endpoints
                    })
                    && peer_usable(peer, &routes, self.region, now)
            })
        });
        self.pending_direct.retain(|id, pending| {
            map.peers.get(id).is_some_and(|peer| {
                node_key(&peer.key).ok() == Some(pending.peer_key)
                    && peer_usable(peer, &routes, self.region, now)
            })
        });
        self.direct_probes.retain(|id, _| {
            map.peers.get(id).is_some_and(|peer| {
                old.and_then(|previous| previous.peers.get(id))
                    .is_some_and(|previous| {
                        previous.key == peer.key && previous.endpoints == peer.endpoints
                    })
                    && peer_usable(peer, &routes, self.region, now)
            })
        });
        self.routes = Some(routes);
        self.map = Some(map.clone());
        self.telemetry.retain_map(map);
        Ok(())
    }

    /// Call immediately when authenticated map polling disconnects or expires.
    pub fn clear_map(&mut self) {
        self.peers.clear();
        self.return_flows.clear();
        self.direct_recent.clear();
        self.pending_direct.clear();
        self.direct_probes.clear();
        self.routes = None;
        self.map = None;
    }

    /// Probe the authenticated relay. A failed write invalidates all sessions.
    pub fn ping_relay(&mut self, nonce: [u8; 8]) -> io::Result<()> {
        if self.map.is_none() {
            return Err(denied("no current map"));
        }
        if let Err(error) = self.client.ping(nonce) {
            self.clear_map();
            return Err(error);
        }
        Ok(())
    }

    fn send_derp(&mut self, destination: [u8; 32], bytes: &[u8]) -> io::Result<()> {
        if let Err(error) = self.client.send(destination, bytes) {
            self.clear_map();
            return Err(error);
        }
        self.counters.sent(false, bytes);
        Ok(())
    }

    fn peer(&self, id: u64) -> io::Result<&Node> {
        let peer = self
            .map
            .as_ref()
            .ok_or_else(|| denied("no current map"))?
            .peers
            .get(&id)
            .ok_or_else(|| denied("unknown peer"))?;
        let route = self
            .routes
            .as_ref()
            .ok_or_else(|| denied("no current route table"))?;
        if !peer_usable(peer, route, self.region, SystemTime::now()) {
            return Err(denied("peer has no usable current route"));
        }
        Ok(peer)
    }

    fn record_handshake(&self, id: u64) {
        if let Some(peer) = self.map.as_ref().and_then(|map| map.peers.get(&id)) {
            self.telemetry.record(id, &peer.key, |traffic| {
                traffic.last_handshake = Some(SystemTime::now());
            });
        }
    }

    fn record_packet(&self, id: u64, via: Transport, bytes: usize, received: bool) {
        if let Some(peer) = self.map.as_ref().and_then(|map| map.peers.get(&id)) {
            self.telemetry.record(id, &peer.key, |traffic| {
                if received {
                    traffic.rx_bytes = traffic.rx_bytes.saturating_add(bytes as u64);
                } else {
                    traffic.tx_bytes = traffic.tx_bytes.saturating_add(bytes as u64);
                }
                match via {
                    Transport::Direct(addr) => {
                        traffic.direct_addr = Some(addr);
                        traffic.relay = false;
                    }
                    Transport::Derp => {
                        traffic.direct_addr = None;
                        traffic.relay = true;
                    }
                }
                traffic.path_updated = Some(Instant::now());
            });
        }
    }

    fn ensure_state(&mut self, peer_id: u64) -> io::Result<&mut PeerState> {
        let key = node_key(&self.peer(peer_id)?.key)?;
        if !self.peers.contains_key(&peer_id) {
            let identity = Identity::from_private(self.private).map_err(wg_error)?;
            let responder = Responder::new(identity, key, [0; 32]).map_err(wg_error)?;
            self.peers.insert(
                peer_id,
                PeerState {
                    responder,
                    initiator: None,
                    handshake_started: None,
                    session: None,
                    queued: None,
                },
            );
        }
        Ok(self.peers.get_mut(&peer_id).expect("inserted peer"))
    }

    fn start_recovery_handshake(&mut self, peer_id: u64, destination: [u8; 32]) -> io::Result<()> {
        if self
            .peers
            .get(&peer_id)
            .is_some_and(|state| state.initiator.is_some())
        {
            return Ok(());
        }
        let identity = Identity::from_private(self.private).map_err(wg_error)?;
        let (initiator, handshake) = Initiator::new(
            &identity,
            destination,
            [0; 32],
            protectorate_tailnet_wg::tai64n_now().map_err(wg_error)?,
        )
        .map_err(wg_error)?;
        let state = self.ensure_state(peer_id)?;
        state.initiator = Some(initiator);
        state.handshake_started = Some(Instant::now());
        self.send_derp(destination, &handshake)
    }

    /// Send an unfragmented IP packet. The first packet starts a handshake and
    /// is held until its response. At most one packet may be queued per peer.
    pub fn send_ip(&mut self, packet: &[u8]) -> io::Result<SendStatus> {
        let parsed = parse_ip(packet)?;
        let local = self
            .map
            .as_ref()
            .and_then(|map| map.self_node.as_ref())
            .ok_or_else(|| denied("no current map"))?;
        if !node_has_ip(local, parsed.source) {
            return Err(denied("outbound IP source is not local"));
        }
        let route = self
            .routes
            .as_ref()
            .ok_or_else(|| denied("no current route table"))?;
        let peer = route
            .route_outbound(parsed.destination, SystemTime::now())
            .ok_or_else(|| denied("outbound IP has no permitted exact peer"))?;
        let peer_id = peer.id;
        let destination = node_key(&peer.key)?;
        self.peer(peer_id)?;
        let private = self.private;
        let state = self.ensure_state(peer_id)?;
        if let Some(session) = state
            .session
            .as_mut()
            .filter(|session| !session.needs_rekey())
        {
            let encrypted = session.encrypt(packet).map_err(wg_error)?;
            let direct_live = self
                .direct_recent
                .get(&peer_id)
                .is_some_and(|seen| seen.elapsed() < DIRECT_LIVE_FOR);
            let via = if direct_live && self.send_direct(peer_id, &encrypted) {
                Transport::Direct(self.direct_endpoint(peer_id).expect("direct send endpoint"))
            } else {
                self.send_derp(destination, &encrypted)?;
                Transport::Derp
            };
            self.record_packet(peer_id, via, packet.len(), false);
            self.observe_outbound_tcp(peer_id, destination, &parsed);
            return Ok(SendStatus::Sent);
        }
        if state.initiator.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "peer handshake already pending",
            ));
        }
        let identity = Identity::from_private(private).map_err(wg_error)?;
        let (initiator, handshake) = Initiator::new(
            &identity,
            destination,
            [0; 32],
            protectorate_tailnet_wg::tai64n_now().map_err(wg_error)?,
        )
        .map_err(wg_error)?;
        state.initiator = Some(initiator);
        state.handshake_started = Some(Instant::now());
        state.queued = Some(packet.to_vec());
        if self.send_direct(peer_id, &handshake) {
            self.pending_direct.insert(
                peer_id,
                PendingDirect {
                    peer_key: destination,
                    handshake: handshake.to_vec(),
                    sent: Instant::now(),
                    retried: false,
                },
            );
        } else {
            self.send_derp(destination, &handshake)?;
        }
        self.observe_outbound_tcp(peer_id, destination, &parsed);
        Ok(SendStatus::HandshakeStarted)
    }

    fn observe_outbound_tcp(&mut self, peer_id: u64, peer_key: [u8; 32], ip: &ParsedIp) {
        let Some(tcp) = ip.tcp else {
            return;
        };
        let key = FlowKey {
            peer_id,
            local: SocketAddr::new(ip.source, tcp.source_port),
            remote: SocketAddr::new(ip.destination, tcp.destination_port),
        };
        let now = Instant::now();
        self.return_flows.retain(|_, flow| flow.expires > now);
        if tcp.flags & 0x02 != 0 && tcp.flags & 0x10 == 0 {
            if self.return_flows.contains_key(&key) || self.return_flows.len() < MAX_RETURN_FLOWS {
                self.return_flows.insert(
                    key,
                    ReturnFlow {
                        peer_key,
                        phase: FlowPhase::SynSent {
                            client_seq: tcp.seq,
                        },
                        expires: now + PENDING_FLOW_LIFETIME,
                    },
                );
            }
            return;
        }
        if let Some(flow) = self.return_flows.get_mut(&key) {
            if flow.peer_key != peer_key {
                return;
            }
            if let FlowPhase::SynAckSeen {
                client_seq,
                server_seq,
            } = flow.phase
            {
                if tcp.flags & 0x10 != 0
                    && tcp.seq == client_seq.wrapping_add(1)
                    && tcp.ack == server_seq.wrapping_add(1)
                {
                    flow.phase = FlowPhase::Established;
                }
            }
            if matches!(flow.phase, FlowPhase::Established) {
                flow.expires = now + ACTIVE_FLOW_LIFETIME;
            }
        }
    }

    fn authorize_return_tcp(&mut self, peer_id: u64, peer_key: [u8; 32], ip: &ParsedIp) -> bool {
        let Some(tcp) = ip.tcp else {
            return false;
        };
        let key = FlowKey {
            peer_id,
            local: SocketAddr::new(ip.destination, tcp.destination_port),
            remote: SocketAddr::new(ip.source, tcp.source_port),
        };
        let Some(map) = self.map.as_ref() else {
            return false;
        };
        let Some(routes) = self.routes.as_ref() else {
            return false;
        };
        if !map
            .self_node
            .as_ref()
            .is_some_and(|local| node_has_ip(local, ip.destination))
            || !map.peers.get(&peer_id).is_some_and(|peer| {
                node_key(&peer.key).ok() == Some(peer_key) && node_has_ip(peer, ip.source)
            })
            || routes
                .route_outbound(ip.source, SystemTime::now())
                .is_none_or(|peer| peer.id != peer_id)
        {
            return false;
        }
        let Some(flow) = self.return_flows.get_mut(&key) else {
            return false;
        };
        let now = Instant::now();
        if flow.peer_key != peer_key || flow.expires <= now {
            self.return_flows.remove(&key);
            return false;
        }
        match flow.phase {
            FlowPhase::SynSent { client_seq } => {
                if tcp.flags & (0x02 | 0x10) != (0x02 | 0x10)
                    || tcp.flags & (0x01 | 0x04) != 0
                    || tcp.ack != client_seq.wrapping_add(1)
                {
                    return false;
                }
                flow.phase = FlowPhase::SynAckSeen {
                    client_seq,
                    server_seq: tcp.seq,
                };
                flow.expires = now + PENDING_FLOW_LIFETIME;
                true
            }
            FlowPhase::SynAckSeen {
                client_seq,
                server_seq,
            } => {
                tcp.flags & (0x02 | 0x10) == (0x02 | 0x10)
                    && tcp.seq == server_seq
                    && tcp.ack == client_seq.wrapping_add(1)
            }
            FlowPhase::Established => {
                flow.expires = now + ACTIVE_FLOW_LIFETIME;
                true
            }
        }
    }

    /// Read exactly one DERP event, authenticate its sender and process WG.
    pub fn receive(&mut self) -> io::Result<ReceiveStatus> {
        let event = match self.client.recv(None) {
            Ok(event) => event,
            Err(error) => {
                self.clear_map();
                return Err(error);
            }
        };
        self.process_event(event)
    }

    /// Read one event with a bounded wait. Idle timeouts return `None` and
    /// preserve the live map and WireGuard sessions; transport errors clear them.
    pub fn receive_timeout(&mut self, timeout: Duration) -> io::Result<Option<ReceiveStatus>> {
        let event = match self.client.recv_timeout(timeout) {
            Ok(event) => event,
            Err(error) => {
                self.clear_map();
                return Err(error);
            }
        };
        event.map(|event| self.process_event(event)).transpose()
    }

    fn process_event(&mut self, event: Event) -> io::Result<ReceiveStatus> {
        let (source, data) = match event {
            Event::Packet { source, data } => (source, data),
            Event::Pong(nonce) => return Ok(ReceiveStatus::Pong(nonce)),
            // A failed delivery can arrive after a later handshake was sent.
            // Only the control map may revoke the peer's authenticated state.
            Event::PeerGone { .. } => return Ok(ReceiveStatus::Control),
            Event::Restarting { .. } => {
                self.clear_map();
                return Err(denied("DERP relay restarting"));
            }
            _ => return Ok(ReceiveStatus::Control),
        };
        let (status, _) = match self.process_packet(source, &data, Transport::Derp) {
            Ok(packet) => packet,
            // DERP send errors clear the map before returning. Errors with a
            // current map are invalid peer frames and cannot drop the relay.
            Err(_) if self.map.is_some() => return Ok(ReceiveStatus::Control),
            Err(error) => return Err(error),
        };
        if matches!(status, ReceiveStatus::Packet { .. }) {
            self.counters
                .derp_ip_received
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(status)
    }

    fn process_packet(
        &mut self,
        source: [u8; 32],
        data: &[u8],
        via: Transport,
    ) -> io::Result<(ReceiveStatus, bool)> {
        if data.len() < 4 {
            return Err(invalid("short WG packet"));
        }
        let map = self.map.as_ref().ok_or_else(|| denied("no current map"))?;
        let peer = map
            .peers
            .values()
            .find(|peer| node_key(&peer.key).ok() == Some(source))
            .ok_or_else(|| denied("WG sender is not a current peer"))?;
        let peer_id = peer.id;
        self.peer(peer_id)?;
        let kind = u32::from_le_bytes(data[..4].try_into().expect("exact slice"));
        match kind {
            1 => {
                let (reply, queued_encrypted) = {
                    let local_key = self.public;
                    let state = self.ensure_state(peer_id)?;
                    if state.initiator.is_some() && local_key < source {
                        return Ok((ReceiveStatus::Control, false));
                    }
                    let pending = state.responder.accept(data).map_err(wg_error)?;
                    let (reply, mut session) = pending.respond().map_err(wg_error)?;
                    let encrypted = state
                        .queued
                        .take()
                        .map(|packet| {
                            let length = packet.len();
                            session
                                .encrypt(&packet)
                                .map(|encrypted| (encrypted, length))
                        })
                        .transpose()
                        .map_err(wg_error)?;
                    state.initiator = None;
                    state.handshake_started = None;
                    state.session = Some(session);
                    (reply, encrypted)
                };
                self.pending_direct.remove(&peer_id);
                self.send_reply(source, &reply, via)?;
                if let Some((encrypted, length)) = queued_encrypted {
                    let sent_via = self.send_reply(source, &encrypted, via)?;
                    self.record_packet(peer_id, sent_via, length, false);
                }
                self.record_handshake(peer_id);
                Ok((ReceiveStatus::Control, true))
            }
            2 => {
                let queued_encrypted = {
                    let state = self.ensure_state(peer_id)?;
                    let Some(initiator) = state.initiator.take() else {
                        return if state.session.is_some() {
                            Ok((ReceiveStatus::Control, false))
                        } else {
                            Err(denied("unsolicited WG response"))
                        };
                    };
                    let mut session = initiator.consume_response(data).map_err(wg_error)?;
                    let encrypted = state
                        .queued
                        .take()
                        .map(|packet| {
                            let length = packet.len();
                            session
                                .encrypt(&packet)
                                .map(|encrypted| (encrypted, length))
                        })
                        .transpose()
                        .map_err(wg_error)?;
                    state.session = Some(session);
                    state.handshake_started = None;
                    encrypted
                };
                self.pending_direct.remove(&peer_id);
                if let Some((encrypted, length)) = queued_encrypted {
                    let sent_via = self.send_reply(source, &encrypted, via)?;
                    self.record_packet(peer_id, sent_via, length, false);
                }
                self.record_handshake(peer_id);
                Ok((ReceiveStatus::Control, true))
            }
            4 => {
                // A mapped peer can deliver packets from its old session
                // after this member reconnects to DERP. Drop that ciphertext
                // and ask it to authenticate a fresh session through DERP.
                if self.ensure_state(peer_id)?.session.is_none() {
                    if matches!(via, Transport::Derp) {
                        self.start_recovery_handshake(peer_id, source)?;
                    }
                    return Ok((ReceiveStatus::Control, false));
                }
                let session = self
                    .ensure_state(peer_id)?
                    .session
                    .as_mut()
                    .expect("checked session");
                let clear = session.decrypt(data).map_err(wg_error)?;
                if clear.is_empty() {
                    if let Transport::Direct(_) = via {
                        let awaiting_echo = self
                            .direct_probes
                            .remove(&peer_id)
                            .is_some_and(|sent| sent.elapsed() <= DIRECT_PROBE_RESPONSE_WINDOW);
                        if !awaiting_echo {
                            let reply = self
                                .ensure_state(peer_id)?
                                .session
                                .as_mut()
                                .expect("authenticated session")
                                .encrypt(&[])
                                .map_err(wg_error)?;
                            self.send_reply(source, &reply, via)?;
                        }
                    }
                    return Ok((ReceiveStatus::Control, true));
                }
                let parsed = parse_ip(&clear)?;
                let allowed = self
                    .routes
                    .as_ref()
                    .ok_or_else(|| denied("no current route table"))?
                    .authorize_inbound(
                        InboundPacket {
                            peer_id,
                            authenticated_node_key: &key_text(&source),
                            source: parsed.source,
                            destination: parsed.destination,
                            protocol: parsed.protocol,
                            destination_port: parsed.destination_port,
                        },
                        SystemTime::now(),
                    );
                if !allowed && !self.authorize_return_tcp(peer_id, source, &parsed) {
                    return Err(denied("inbound IP packet denied by current map"));
                }
                self.record_packet(peer_id, via, clear.len(), true);
                Ok((
                    ReceiveStatus::Packet {
                        peer_id,
                        bytes: clear,
                    },
                    true,
                ))
            }
            _ => Err(invalid("unsupported WG message")),
        }
    }
}

fn node_has_ip(node: &Node, ip: IpAddr) -> bool {
    node.addresses.iter().any(|address| {
        address.split_once('/').is_some_and(|(raw, bits)| {
            raw.parse::<IpAddr>().ok() == Some(ip)
                && bits == if ip.is_ipv4() { "32" } else { "128" }
        })
    })
}

fn peer_usable(peer: &Node, routes: &RouteTable, region: i64, now: SystemTime) -> bool {
    if peer.home_derp != region
        || peer.machine_authorized == Some(false)
        || peer.unsigned_peer_api_only
    {
        return false;
    }
    peer.addresses.iter().any(|address| {
        address
            .split_once('/')
            .and_then(|(ip, _)| ip.parse::<IpAddr>().ok())
            .and_then(|ip| routes.route_outbound(ip, now))
            .is_some_and(|routed| routed.id == peer.id)
    })
}

struct ParsedIp {
    source: IpAddr,
    destination: IpAddr,
    protocol: u8,
    destination_port: u16,
    tcp: Option<TcpMeta>,
}
#[derive(Clone, Copy)]
struct TcpMeta {
    source_port: u16,
    destination_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
}

fn parse_ip(packet: &[u8]) -> io::Result<ParsedIp> {
    if packet.is_empty() {
        return Err(invalid("empty IP packet"));
    }
    let (source, destination, protocol, body) = match packet[0] >> 4 {
        4 => {
            if packet.len() < 20 {
                return Err(invalid("short IPv4 packet"));
            }
            let header_len = ((packet[0] & 15) as usize) * 4;
            let total = u16::from_be_bytes([packet[2], packet[3]]) as usize;
            if header_len < 20
                || total != packet.len()
                || total < header_len
                || packet[6] & 0xbf != 0
                || packet[7] != 0
            {
                return Err(invalid("invalid or fragmented IPv4 packet"));
            }
            let sum = packet[..header_len]
                .chunks_exact(2)
                .fold(0u32, |sum, bytes| {
                    sum + u16::from_be_bytes([bytes[0], bytes[1]]) as u32
                });
            let folded = (sum & 0xffff) + (sum >> 16);
            if (folded & 0xffff) + (folded >> 16) != 0xffff {
                return Err(invalid("IPv4 header checksum"));
            }
            (
                IpAddr::from([packet[12], packet[13], packet[14], packet[15]]),
                IpAddr::from([packet[16], packet[17], packet[18], packet[19]]),
                packet[9],
                &packet[header_len..],
            )
        }
        6 => {
            if packet.len() < 40
                || packet.len() != 40 + u16::from_be_bytes([packet[4], packet[5]]) as usize
            {
                return Err(invalid("invalid IPv6 packet length"));
            }
            let source: [u8; 16] = packet[8..24].try_into().expect("exact slice");
            let destination: [u8; 16] = packet[24..40].try_into().expect("exact slice");
            (
                IpAddr::from(source),
                IpAddr::from(destination),
                packet[6],
                &packet[40..],
            )
        }
        _ => return Err(invalid("unsupported IP version")),
    };
    let mut tcp = None;
    let destination_port = match protocol {
        6 if body.len() >= 20
            && (body[12] >> 4) >= 5
            && ((body[12] >> 4) as usize) * 4 <= body.len() =>
        {
            tcp = Some(TcpMeta {
                source_port: u16::from_be_bytes([body[0], body[1]]),
                destination_port: u16::from_be_bytes([body[2], body[3]]),
                seq: u32::from_be_bytes(body[4..8].try_into().expect("exact slice")),
                ack: u32::from_be_bytes(body[8..12].try_into().expect("exact slice")),
                flags: body[13],
            });
            u16::from_be_bytes([body[2], body[3]])
        }
        17 if body.len() >= 8 && u16::from_be_bytes([body[4], body[5]]) as usize == body.len() => {
            u16::from_be_bytes([body[2], body[3]])
        }
        1 if source.is_ipv4() && body.len() >= 4 => 0,
        58 if source.is_ipv6() && body.len() >= 4 => 0,
        _ => return Err(invalid("unsupported or malformed IP protocol")),
    };
    Ok(ParsedIp {
        source,
        destination,
        protocol,
        destination_port,
        tcp,
    })
}
