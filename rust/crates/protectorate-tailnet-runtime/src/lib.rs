//! A joined node's map, pinned DERP/WireGuard transport, and virtual TCP listener.
//! Connectivity is advertised only after both the control map and DERP handshake
//! succeed. Losing the map destroys packet and TCP sessions for that generation.

use protectorate_tailnet_map::Netmap;
use protectorate_tailnet_member::MemberIdentity;
use protectorate_tailnet_packets::{
    relay_port, relay_trust, PacketEngine, PeerTelemetry, PeerTraffic, ReceiveStatus,
    TransportCounters, TransportStats,
};
use protectorate_tailnet_poll::{PollUpdate, Poller};
use protectorate_tailnet_route::RouteTable;
use protectorate_tailnet_stream::{Accepted, Dialer, Driver, Ingress, IngressError};
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    sync::{mpsc, watch, Mutex},
    task::JoinHandle,
    time,
};

const QUEUE: usize = 64;
const RECEIVE_SLICE: Duration = Duration::from_millis(50);
const RELAY_PROBE_INTERVAL: Duration = Duration::from_secs(15);
const RELAY_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

struct RelayWatchdog {
    interval: Duration,
    timeout: Duration,
    next_probe: Instant,
    pending: Option<([u8; 8], Instant)>,
    sequence: u64,
}

impl RelayWatchdog {
    fn new(now: Instant, interval: Duration, timeout: Duration) -> Self {
        Self {
            interval,
            timeout,
            next_probe: now + interval,
            pending: None,
            sequence: 0,
        }
    }

    fn tick(&mut self, now: Instant) -> io::Result<Option<[u8; 8]>> {
        if let Some((_, sent)) = self.pending {
            if now.duration_since(sent) >= self.timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "DERP relay did not acknowledge liveness probe",
                ));
            }
            return Ok(None);
        }
        if now < self.next_probe {
            return Ok(None);
        }
        self.sequence = self.sequence.wrapping_add(1);
        let nonce = self.sequence.to_be_bytes();
        self.pending = Some((nonce, now));
        Ok(Some(nonce))
    }

    fn pong(&mut self, nonce: [u8; 8], now: Instant) {
        if self.pending.is_some_and(|(expected, _)| expected == nonce) {
            self.pending = None;
            self.next_probe = now + self.interval;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum State {
    #[default]
    Disconnected,
    Connected {
        generation: u64,
        listener: SocketAddr,
    },
}

#[derive(Clone)]
pub struct MapSnapshot {
    pub generation: u64,
    pub map: Arc<Netmap>,
}

#[derive(Clone)]
pub struct StatusSnapshot {
    pub map: MapSnapshot,
    pub traffic: BTreeMap<u64, PeerTraffic>,
}

/// Read-only view of the map successfully applied to the live packet engine.
#[derive(Clone)]
pub struct Authorization {
    state: watch::Receiver<State>,
    map: watch::Receiver<Option<MapSnapshot>>,
}

#[derive(PartialEq, Eq)]
struct PeerEndpoint {
    generation: u64,
    id: u64,
    key: String,
    address: SocketAddr,
}

impl Authorization {
    pub fn current_map(&self) -> Option<MapSnapshot> {
        let State::Connected { generation, .. } = *self.state.borrow() else {
            return None;
        };
        self.map
            .borrow()
            .as_ref()
            .filter(|snapshot| snapshot.generation == generation)
            .cloned()
    }

    pub fn peer_has_tag(&self, generation: u64, peer_id: u64, tag: &str) -> bool {
        let Some(snapshot) = self.current_map() else {
            return false;
        };
        if snapshot.generation != generation {
            return false;
        }
        let Some(peer) = snapshot.map.peers.get(&peer_id) else {
            return false;
        };
        if !peer.tags.iter().any(|candidate| candidate == tag) {
            return false;
        }
        let Ok(routes) = RouteTable::compile(&snapshot.map) else {
            return false;
        };
        peer.addresses
            .iter()
            .filter_map(|address| exact_ip(address))
            .any(|ip| {
                routes
                    .route_outbound(ip, SystemTime::now())
                    .is_some_and(|routed| routed.id == peer_id)
            })
    }

    pub fn peer_addr(&self, name: &str, port: u16) -> io::Result<SocketAddr> {
        Ok(self.resolve_peer(name, port)?.address)
    }

    fn resolve_peer(&self, name: &str, port: u16) -> io::Result<PeerEndpoint> {
        if name.is_empty() || port == 0 {
            return Err(invalid("tailnet peer name or port missing"));
        }
        let snapshot = self.current_map().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "no connected tailnet map")
        })?;
        let routes = RouteTable::compile(&snapshot.map).map_err(io::Error::other)?;
        let mut matches = snapshot.map.peers.values().filter(|peer| {
            peer.name == name
                || peer
                    .name
                    .strip_prefix(name)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        });
        let peer = matches
            .next()
            .ok_or_else(|| invalid("tailnet peer name absent"))?;
        if matches.next().is_some() {
            return Err(invalid("tailnet peer name ambiguous"));
        }
        let ip = peer
            .addresses
            .iter()
            .filter_map(|address| exact_ip(address))
            .find(|ip| {
                routes
                    .route_outbound(*ip, SystemTime::now())
                    .is_some_and(|routed| routed.id == peer.id)
            })
            .ok_or_else(|| invalid("tailnet peer has no usable exact address"))?;
        Ok(PeerEndpoint {
            generation: snapshot.generation,
            id: peer.id,
            key: peer.key.clone(),
            address: SocketAddr::new(ip, port),
        })
    }
}

fn exact_ip(prefix: &str) -> Option<IpAddr> {
    let (address, mask) = prefix.split_once('/')?;
    let ip = address.parse::<IpAddr>().ok()?;
    match (ip, mask) {
        (IpAddr::V4(_), "32") | (IpAddr::V6(_), "128") => Some(ip),
        _ => None,
    }
}

/// Own this handle for the lifetime of the virtual listener. The accepted
/// receiver yields Tokio duplex streams usable by an HTTP/1.1 server.
pub struct Runtime {
    accepted: Option<mpsc::Receiver<Accepted>>,
    state: watch::Receiver<State>,
    dialer: watch::Receiver<Option<Dialer>>,
    authorization: Authorization,
    counters: Arc<TransportCounters>,
    telemetry: PeerTelemetry,
    stop: Arc<AtomicBool>,
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

impl Runtime {
    pub fn start(directory: impl AsRef<Path>, listen_port: u16) -> io::Result<Self> {
        let directory = directory.as_ref();
        let direct_directory = directory.to_path_buf();
        let poller = Poller::open(directory).map_err(io::Error::other)?;
        let private = MemberIdentity::open(directory)
            .map_err(io::Error::other)?
            .node_private_key_bytes();
        let updates = poller.subscribe();
        let stop = Arc::new(AtomicBool::new(false));
        let (accepted_tx, accepted) = mpsc::channel(16);
        let (state_tx, state) = watch::channel(State::Disconnected);
        let (dialer_tx, dialer) = watch::channel(None);
        let (map_tx, map) = watch::channel(None);
        let authorization = Authorization {
            state: state.clone(),
            map,
        };
        let counters = Arc::new(TransportCounters::default());
        let run_counters = counters.clone();
        let telemetry = PeerTelemetry::default();
        let run_telemetry = telemetry.clone();
        let poll_stop = stop.clone();
        let poll_task = tokio::task::spawn_blocking(move || poller.run(&poll_stop));
        let run_stop = stop.clone();
        let supervisor = tokio::spawn(async move {
            supervise(
                updates,
                direct_directory,
                run_counters,
                run_telemetry,
                private,
                listen_port,
                accepted_tx,
                state_tx,
                dialer_tx,
                map_tx,
                run_stop,
            )
            .await;
            let _ = poll_task.await;
        });
        Ok(Self {
            accepted: Some(accepted),
            state,
            dialer,
            authorization,
            counters,
            telemetry,
            stop,
            supervisor: Mutex::new(Some(supervisor)),
        })
    }

    pub fn state(&self) -> State {
        *self.state.borrow()
    }

    /// Lifetime packet path counters, including prior relay reconnects.
    pub fn transport_stats(&self) -> TransportStats {
        self.counters.snapshot()
    }
    pub fn status_snapshot(&self) -> Option<StatusSnapshot> {
        Some(StatusSnapshot {
            map: self.authorization.current_map()?,
            traffic: self.telemetry.snapshot(),
        })
    }
    pub fn subscribe_state(&self) -> watch::Receiver<State> {
        self.state.clone()
    }

    pub fn authorization(&self) -> Authorization {
        self.authorization.clone()
    }
    pub fn peer_addr(&self, name: &str, port: u16) -> io::Result<SocketAddr> {
        self.authorization.peer_addr(name, port)
    }
    /// Dial a named peer and reject any map or identity change during the TCP
    /// handshake, including same-generation address reassignment.
    pub async fn dial_peer(&self, name: &str, port: u16) -> io::Result<tokio::io::DuplexStream> {
        let before = self.authorization.resolve_peer(name, port)?;
        let stream = self.dial(before.address).await?;
        let after = self.authorization.resolve_peer(name, port)?;
        if before != after {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "tailnet peer identity changed during dial",
            ));
        }
        Ok(stream)
    }
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn take_accepted(&mut self) -> Option<mpsc::Receiver<Accepted>> {
        self.accepted.take()
    }

    /// Opens a stream to another member through the current authenticated
    /// map and pinned private DERP. Returns only after the TCP handshake.
    pub async fn dial(&self, remote: SocketAddr) -> io::Result<tokio::io::DuplexStream> {
        let expected = self.state();
        if !matches!(expected, State::Connected { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "tailnet is not connected",
            ));
        }
        let dialer = self.dialer.borrow().clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "tailnet dialer is unavailable")
        })?;
        let stream = dialer.connect(remote).await?;
        if self.state() != expected {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "tailnet map changed during dial",
            ));
        }
        Ok(stream)
    }

    pub async fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(supervisor) = self.supervisor.lock().await.take() {
            let _ = supervisor.await;
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

enum Command {
    Replace(Box<Netmap>, u64),
    Packet(Vec<u8>),
}

#[derive(Clone, Copy)]
enum Event {
    Waiting,
    Ready(u64),
    Down,
}

struct Connection {
    generation: u64,
    listener: SocketAddr,
    peer_security: BTreeMap<u64, PeerSecurity>,
    packet_filters: BTreeMap<String, Vec<serde_json::Value>>,
    revision: u64,
    commands: mpsc::Sender<Command>,
    events: watch::Receiver<Event>,
    stop: Arc<AtomicBool>,
    forward_stop: watch::Sender<bool>,
    dialer: Dialer,
    driver: JoinHandle<()>,
    forward: JoinHandle<()>,
    worker: JoinHandle<()>,
}

#[derive(PartialEq, Eq)]
struct PeerSecurity {
    key: String,
    addresses: Vec<String>,
    allowed_ips: Vec<String>,
    tags: Vec<String>,
    home_derp: i64,
    machine_authorized: Option<bool>,
    unsigned_peer_api_only: bool,
    key_expiry: Option<String>,
}

fn peer_security(map: &Netmap) -> BTreeMap<u64, PeerSecurity> {
    map.peers
        .iter()
        .map(|(id, peer)| {
            (
                *id,
                PeerSecurity {
                    key: peer.key.clone(),
                    addresses: peer.addresses.clone(),
                    allowed_ips: peer.allowed_ips().to_vec(),
                    tags: peer.tags.clone(),
                    home_derp: peer.home_derp,
                    machine_authorized: peer.machine_authorized,
                    unsigned_peer_api_only: peer.unsigned_peer_api_only,
                    key_expiry: peer.key_expiry.clone(),
                },
            )
        })
        .collect()
}

impl Connection {
    #[allow(clippy::too_many_arguments)]
    fn start(
        map: Netmap,
        generation: u64,
        directory: PathBuf,
        counters: Arc<TransportCounters>,
        telemetry: PeerTelemetry,
        private: [u8; 32],
        listen_port: u16,
        accepted: mpsc::Sender<Accepted>,
    ) -> io::Result<Self> {
        let relay = select_relay(&map)?;
        let listener = select_listener(&map, listen_port)?;
        let peer_security = peer_security(&map);
        let packet_filters = map.packet_filters.clone();
        let (driver, mut ports) = Driver::new_with_generation(listener, generation);
        let ingress = ports.ingress.clone();
        let dialer = ports.dialer.clone();
        let (commands, receiver) = mpsc::channel(QUEUE);
        let (events_tx, events) = watch::channel(Event::Waiting);
        let forward_events = events_tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = tokio::task::spawn_blocking(move || {
            packet_worker(PacketWorker {
                private,
                map,
                relay,
                directory,
                counters,
                telemetry,
                commands: receiver,
                ingress,
                events: events_tx,
                stop: worker_stop,
            })
        });
        let driver = tokio::spawn(driver.run());
        let (forward_stop, mut stopped) = watch::channel(false);
        let forward_tx = commands.clone();
        let forward = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    packet = ports.egress.recv() => match packet {
                        Some(packet) => {
                            if !forward_packet(&forward_tx, &mut stopped, packet).await { break; }
                        }
                        None => break,
                    },
                    stream = ports.accepted.recv() => match stream {
                        Some(stream) => {
                            tokio::select! {
                                _ = stopped.changed() => break,
                                result = accepted.send(stream) => if result.is_err() { break; },
                            }
                        }
                        None => break,
                    },
                }
            }
            ports.shutdown();
            forward_events.send_replace(Event::Down);
        });
        Ok(Self {
            generation,
            listener,
            peer_security,
            packet_filters,
            revision: 0,
            commands,
            events,
            stop,
            forward_stop,
            dialer,
            driver,
            forward,
            worker,
        })
    }

    fn replace(&mut self, map: Netmap) -> io::Result<()> {
        if select_listener(&map, self.listener.port())? != self.listener {
            return Err(invalid("tailnet listener address changed"));
        }
        if peer_security(&map) != self.peer_security || map.packet_filters != self.packet_filters {
            return Err(invalid("tailnet peer identity or authority changed"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("map revision overflow"))?;
        self.commands
            .try_send(Command::Replace(Box::new(map), revision))
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "tailnet command queue full"))?;
        self.revision = revision;
        Ok(())
    }

    async fn shutdown(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.forward_stop.send(true);
        let _ = self.forward.await;
        let _ = self.worker.await;
        let _ = self.driver.await;
    }
}

async fn forward_packet(
    commands: &mpsc::Sender<Command>,
    stopped: &mut watch::Receiver<bool>,
    packet: Vec<u8>,
) -> bool {
    tokio::select! {
        _ = stopped.changed() => false,
        result = commands.send(Command::Packet(packet)) => result.is_ok(),
    }
}

// The three watch senders publish distinct externally observed state changes.
#[allow(clippy::too_many_arguments)]
async fn supervise(
    mut updates: watch::Receiver<PollUpdate>,
    directory: PathBuf,
    counters: Arc<TransportCounters>,
    telemetry: PeerTelemetry,
    private: [u8; 32],
    listen_port: u16,
    accepted: mpsc::Sender<Accepted>,
    state: watch::Sender<State>,
    dialer: watch::Sender<Option<Dialer>>,
    authorization_map: watch::Sender<Option<MapSnapshot>>,
    stop: Arc<AtomicBool>,
) {
    let mut connection: Option<Connection> = None;
    let mut retry = time::interval(Duration::from_millis(500));
    retry.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut retry_after = time::Instant::now();
    let mut retry_delay = Duration::from_millis(500);
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        if connection.as_ref().is_some_and(|current| {
            current.forward.is_finished()
                || current.driver.is_finished()
                || current.worker.is_finished()
        }) {
            state.send_replace(State::Disconnected);
            authorization_map.send_replace(None);
            dialer.send_replace(None);
            if let Some(old) = connection.take() {
                old.shutdown().await;
            }
            retry_after = time::Instant::now() + retry_delay;
            retry_delay = (retry_delay * 2).min(Duration::from_secs(10));
        }
        let update = updates.borrow().clone();
        match (&mut connection, &update.map) {
            (Some(current), Some(map)) if current.generation == update.generation => {
                // Replacements are sent only on changed() below.
                let _ = map;
            }
            (Some(_), _) => {
                let old = connection.take().expect("existing connection");
                state.send_replace(State::Disconnected);
                authorization_map.send_replace(None);
                dialer.send_replace(None);
                old.shutdown().await;
                retry_after = time::Instant::now();
            }
            (None, Some(map)) => {
                if time::Instant::now() >= retry_after {
                    match Connection::start(
                        map.clone(),
                        update.generation,
                        directory.clone(),
                        counters.clone(),
                        telemetry.clone(),
                        private,
                        listen_port,
                        accepted.clone(),
                    ) {
                        Ok(next) => connection = Some(next),
                        Err(error) => {
                            eprintln!("tailnet connection setup: {error}");
                            retry_after = time::Instant::now() + retry_delay;
                            retry_delay = (retry_delay * 2).min(Duration::from_secs(10));
                        }
                    }
                }
            }
            (None, None) => {}
        }
        tokio::select! {
            biased;
            changed = updates.changed() => {
                if changed.is_err() { break; }
                let new = updates.borrow_and_update().clone();
                if let Some(current) = connection.as_mut() {
                    if current.generation == new.generation {
                        match new.map {
                            Some(map) => {
                                state.send_replace(State::Disconnected);
                                authorization_map.send_replace(None);
                                dialer.send_replace(None);
                                if current.replace(map).is_err() {
                                    let old = connection.take().expect("existing connection");
                                    state.send_replace(State::Disconnected);
                                    authorization_map.send_replace(None);
                                    dialer.send_replace(None);
                                    old.shutdown().await;
                                    retry_after = time::Instant::now() + retry_delay;
                                    retry_delay = (retry_delay * 2).min(Duration::from_secs(10));
                                }
                            }
                            None => {
                                let old = connection.take().expect("existing connection");
                                state.send_replace(State::Disconnected);
                                authorization_map.send_replace(None);
                                dialer.send_replace(None);
                                old.shutdown().await;
                            }
                        }
                    }
                }
            }
            event = async {
                let events = &mut connection.as_mut().expect("select guard").events;
                if events.changed().await.is_ok() { Some(*events.borrow_and_update()) } else { None }
            }, if connection.is_some() => {
                match event {
                    Some(Event::Ready(revision)) if connection.as_ref().is_some_and(|current|
                        updates.borrow().map.is_some() && current.generation == updates.borrow().generation
                            && current.revision == revision && !current.forward.is_finished()
                            && !current.driver.is_finished()) => {
                        retry_delay = Duration::from_millis(500);
                        let listener = connection.as_ref().expect("ready connection").listener;
                        if let Some(map) = updates.borrow().map.as_ref() {
                            authorization_map.send_replace(Some(MapSnapshot {
                                generation: update.generation,
                                map: Arc::new(map.clone()),
                            }));
                        }
                        dialer.send_replace(Some(connection.as_ref().expect("ready connection").dialer.clone()));
                        state.send_replace(State::Connected { generation: update.generation, listener });
                    }
                    Some(Event::Ready(_) | Event::Waiting) => {}
                    _ => {
                        state.send_replace(State::Disconnected);
                        authorization_map.send_replace(None);
                        dialer.send_replace(None);
                        if let Some(old) = connection.take() { old.shutdown().await; }
                        retry_after = time::Instant::now() + retry_delay;
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(10));
                    }
                }
            }
            _ = retry.tick() => {}
        }
    }
    state.send_replace(State::Disconnected);
    authorization_map.send_replace(None);
    dialer.send_replace(None);
    if let Some(old) = connection {
        old.shutdown().await;
    }
}

struct PacketWorker {
    private: [u8; 32],
    map: Netmap,
    relay: SocketAddr,
    directory: PathBuf,
    counters: Arc<TransportCounters>,
    telemetry: PeerTelemetry,
    commands: mpsc::Receiver<Command>,
    ingress: Ingress,
    events: watch::Sender<Event>,
    stop: Arc<AtomicBool>,
}

fn packet_worker(
    PacketWorker {
        private,
        map,
        relay,
        directory,
        counters,
        telemetry,
        mut commands,
        ingress,
        events,
        stop,
    }: PacketWorker,
) {
    let mut engine = match PacketEngine::connect(private, &map, relay, None) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("tailnet DERP connect: {error}");
            events.send_replace(Event::Down);
            return;
        }
    };
    engine.set_counters(counters);
    engine.set_peer_telemetry(telemetry);
    if let Ok(endpoint) = local_udp_endpoint(relay) {
        if let Ok(bound) = engine.bind_direct(SocketAddr::new(endpoint.ip(), 0)) {
            if let Ok(poller) = Poller::open(&directory) {
                if let Err(error) = poller.report_endpoints(&[bound]) {
                    eprintln!("tailnet direct endpoint report: {error}");
                }
            }
        }
    }
    events.send_replace(Event::Ready(0));
    let mut watchdog =
        RelayWatchdog::new(Instant::now(), RELAY_PROBE_INTERVAL, RELAY_PROBE_TIMEOUT);
    let mut pending_ingress: Option<(u64, Vec<u8>)> = None;
    while !stop.load(Ordering::Acquire) {
        match watchdog.tick(Instant::now()) {
            Ok(Some(nonce)) => {
                if let Err(error) = engine.ping_relay(nonce) {
                    eprintln!("tailnet DERP liveness probe: {error}");
                    events.send_replace(Event::Down);
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("tailnet DERP liveness: {error}");
                events.send_replace(Event::Down);
                return;
            }
        }
        for _ in 0..QUEUE {
            match commands.try_recv() {
                Ok(Command::Replace(map, revision)) => {
                    if let Err(error) = engine.replace_map(&map) {
                        eprintln!("tailnet map replace: {error}");
                        events.send_replace(Event::Down);
                        return;
                    }
                    events.send_replace(Event::Ready(revision));
                }
                Ok(Command::Packet(packet)) => {
                    if let Err(error) = engine.send_ip(&packet) {
                        if error.kind() != io::ErrorKind::WouldBlock {
                            eprintln!("tailnet packet send: {error}");
                            events.send_replace(Event::Down);
                            return;
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => return,
            }
        }
        if let Err(error) = engine.tick_direct() {
            eprintln!("tailnet direct handshake fallback: {error}");
            events.send_replace(Event::Down);
            return;
        }
        if let Some((peer_id, bytes)) = pending_ingress.take() {
            if let Err((error, bytes)) = ingress.try_send_retain(peer_id, bytes) {
                match error {
                    IngressError::Full => {
                        pending_ingress = Some((peer_id, bytes));
                    }
                    IngressError::Closed => return,
                    IngressError::Oversized => {}
                }
            }
        }
        let mut direct_read = 0;
        for _ in 0..QUEUE {
            if pending_ingress.is_some() {
                break;
            }
            match engine.receive_direct() {
                Ok(Some(ReceiveStatus::Packet { peer_id, bytes })) => {
                    direct_read += 1;
                    if let Err((error, bytes)) = ingress.try_send_retain(peer_id, bytes) {
                        match error {
                            IngressError::Full => {
                                pending_ingress = Some((peer_id, bytes));
                            }
                            IngressError::Closed => return,
                            IngressError::Oversized => {}
                        }
                    }
                }
                Ok(Some(ReceiveStatus::Control | ReceiveStatus::Pong(_))) => {
                    direct_read += 1;
                }
                Ok(None) | Err(_) => break,
            }
        }
        if pending_ingress.is_some() {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        // A direct UDP burst can overrun the kernel receive buffer if every
        // batch waits the full DERP idle timeout before draining the socket.
        // Poll DERP briefly while direct traffic is active so its control
        // frames still progress without imposing a 50 ms gap per UDP batch.
        let relay_wait = if direct_read > 0 {
            Duration::from_millis(1)
        } else {
            RECEIVE_SLICE
        };
        match engine.receive_timeout(relay_wait) {
            Ok(Some(ReceiveStatus::Packet { peer_id, bytes })) => {
                if let Err((error, bytes)) = ingress.try_send_retain(peer_id, bytes) {
                    match error {
                        IngressError::Full => {
                            pending_ingress = Some((peer_id, bytes));
                        }
                        IngressError::Closed => return,
                        IngressError::Oversized => {}
                    }
                }
            }
            Ok(Some(ReceiveStatus::Pong(nonce))) => watchdog.pong(nonce, Instant::now()),
            Ok(Some(ReceiveStatus::Control) | None) => {}
            Err(error) => {
                eprintln!("tailnet packet receive: {error}");
                events.send_replace(Event::Down);
                return;
            }
        }
    }
}

fn local_udp_endpoint(relay: SocketAddr) -> io::Result<SocketAddr> {
    let bind = if relay.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let probe = UdpSocket::bind(bind)?;
    probe.connect(relay)?;
    let local = probe.local_addr()?;
    if local.ip().is_unspecified() || local.ip().is_loopback() && !relay.ip().is_loopback() {
        return Err(invalid("no routable local UDP address"));
    }
    Ok(local)
}

/// Only the local node's advertised region may be used. The packet engine
/// independently checks the exact address and certificate trust policy.
pub fn select_relay(map: &Netmap) -> io::Result<SocketAddr> {
    let local = map
        .self_node
        .as_ref()
        .ok_or_else(|| invalid("map has no local node"))?;
    let region = map
        .derp_map
        .as_ref()
        .and_then(|derp| derp.regions.get(&local.home_derp.to_string()))
        .filter(|region| region.region_id == local.home_derp && local.home_derp > 0)
        .ok_or_else(|| invalid("local DERP region absent"))?;
    for node in &region.nodes {
        if node.stun_only || relay_trust(node).is_none() {
            continue;
        }
        let Some(port) = relay_port(node) else {
            continue;
        };
        for address in [&node.ipv4, &node.ipv6] {
            if let Ok(ip) = address.parse::<IpAddr>() {
                return Ok(SocketAddr::new(ip, port));
            }
        }
        if !node.host_name.is_empty() {
            if let Ok(mut addresses) = (node.host_name.as_str(), port).to_socket_addrs() {
                if let Some(address) = addresses.next() {
                    return Ok(address);
                }
            }
        }
    }
    Err(invalid("DERP address or certificate trust missing"))
}

/// Select an exact /32 or /128 address assigned to the local node by the
/// authenticated control map. IPv4 is preferred when both families exist.
pub fn select_listener(map: &Netmap, port: u16) -> io::Result<SocketAddr> {
    if port == 0 {
        return Err(invalid("tailnet listener port is zero"));
    }
    let local = map
        .self_node
        .as_ref()
        .ok_or_else(|| invalid("map has no local node"))?;
    let mut ipv6 = None;
    for prefix in &local.addresses {
        let Some((ip, mask)) = prefix.split_once('/') else {
            continue;
        };
        let Ok(ip) = ip.parse::<IpAddr>() else {
            continue;
        };
        match (ip, mask) {
            (IpAddr::V4(_), "32") => return Ok(SocketAddr::new(ip, port)),
            (IpAddr::V6(_), "128") => ipv6 = Some(ip),
            _ => {}
        }
    }
    ipv6.map(|ip| SocketAddr::new(ip, port))
        .ok_or_else(|| invalid("local node has no exact tailnet address"))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn outbound_packet_waits_for_bounded_worker_queue_instead_of_dropping() {
        let (commands, mut receiver) = mpsc::channel(1);
        let (_stop_tx, mut stopped) = watch::channel(false);
        commands.send(Command::Packet(vec![1])).await.unwrap();
        let waiting = forward_packet(&commands, &mut stopped, vec![2]);
        tokio::pin!(waiting);
        assert!(time::timeout(Duration::from_millis(20), &mut waiting)
            .await
            .is_err());
        let Some(Command::Packet(first)) = receiver.recv().await else {
            panic!("first queued packet missing");
        };
        assert_eq!(first, vec![1]);
        assert!(time::timeout(Duration::from_secs(1), &mut waiting)
            .await
            .unwrap());
        let Some(Command::Packet(second)) = receiver.recv().await else {
            panic!("backpressured packet missing");
        };
        assert_eq!(second, vec![2]);
    }

    #[test]
    fn silent_relay_probe_times_out_but_matching_pong_keeps_connection_alive() {
        let start = Instant::now();
        let interval = Duration::from_secs(15);
        let timeout = Duration::from_secs(5);
        let mut watchdog = RelayWatchdog::new(start, interval, timeout);
        assert_eq!(
            watchdog
                .tick(start + interval - Duration::from_millis(1))
                .unwrap(),
            None
        );
        let nonce = watchdog.tick(start + interval).unwrap().unwrap();
        watchdog.pong([9; 8], start + interval + Duration::from_secs(1));
        assert_eq!(
            watchdog
                .tick(start + interval + timeout - Duration::from_millis(1))
                .unwrap(),
            None
        );
        assert_eq!(
            watchdog
                .tick(start + interval + timeout)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );

        let mut watchdog = RelayWatchdog::new(start, interval, timeout);
        assert_eq!(watchdog.tick(start + interval).unwrap(), Some(nonce));
        watchdog.pong(nonce, start + interval + Duration::from_secs(1));
        assert_eq!(watchdog.tick(start + interval + timeout).unwrap(), None);
        assert!(watchdog
            .tick(start + interval * 2 + Duration::from_secs(1))
            .unwrap()
            .is_some());
    }

    fn map(pin: &str, node_region: i64) -> Netmap {
        let mut map = Netmap::default();
        let frame = json!({
            "Node": {
                "ID": 1,
                "Key": format!("nodekey:{}", "11".repeat(32)),
                "Addresses": ["100.64.0.1/32"],
                "HomeDERP": node_region
            },
            "Peers": [],
            "DERPMap": {"Regions": {"900": {"RegionID": 900, "Nodes": [{
                "Name": "private", "HostName": "localhost", "IPv4": "127.0.0.1",
                "DERPPort": 8443, "CertName": pin
            }]}}, "omitDefaultRegions": true},
            "PacketFilter": [],
            "MapSessionHandle": "test-session", "Seq": 1
        });
        map.apply_json(&serde_json::to_vec(&frame).unwrap())
            .unwrap();
        map
    }

    #[test]
    fn private_relay_requires_exact_local_region_and_leaf_pin() {
        let pin = format!("sha256-raw:{}", "ab".repeat(32));
        assert_eq!(
            select_relay(&map(&pin, 900)).unwrap(),
            "127.0.0.1:8443".parse().unwrap()
        );
        assert!(select_relay(&map("localhost", 900)).is_err());
        assert!(select_relay(&map(&pin, 901)).is_err());
        assert_eq!(
            select_listener(&map(&pin, 900), 7475).unwrap(),
            "100.64.0.1:7475".parse().unwrap()
        );
        assert!(select_listener(&map(&pin, 900), 0).is_err());
    }

    #[test]
    fn public_relay_uses_tailscale_hostname_without_private_pin() {
        let mut public = map("", 900);
        let set_relay = |map: &mut Netmap, hostname: &str, cert_name: &str| {
            let node = &mut map
                .derp_map
                .as_mut()
                .unwrap()
                .regions
                .get_mut("900")
                .unwrap()
                .nodes[0];
            node.host_name = hostname.into();
            node.cert_name = cert_name.into();
        };
        set_relay(&mut public, "derp1f.tailscale.com", "");
        assert_eq!(
            select_relay(&public).unwrap(),
            "127.0.0.1:8443".parse().unwrap()
        );
        public
            .derp_map
            .as_mut()
            .unwrap()
            .regions
            .get_mut("900")
            .unwrap()
            .nodes[0]
            .derp_port = 0;
        assert_eq!(
            select_relay(&public).unwrap(),
            "127.0.0.1:443".parse().unwrap()
        );
        set_relay(&mut public, "derp1f.attacker.example", "");
        assert!(select_relay(&public).is_err());
        set_relay(&mut public, "derp1f.tailscale.com", "sha256-raw:bad");
        assert!(select_relay(&public).is_err());
    }

    #[test]
    fn authorization_tracks_peer_id_tag_generation_and_ip_reassignment() {
        let mut current = map(&format!("sha256-raw:{}", "ab".repeat(32)), 900);
        let peer = |id: u64, name: &str, tag: &str| {
            serde_json::from_value(json!({
                "ID": id, "Name": name,
                "Key": format!("nodekey:{}", format!("{id:02x}").repeat(32)),
                "Addresses": ["100.64.0.2/32"], "AllowedIPs": ["100.64.0.2/32"],
                "HomeDERP": 900, "MachineAuthorized": true, "Tags": [tag]
            }))
            .unwrap()
        };
        current.peers.insert(2, peer(2, "hub", "tag:hub"));
        let (state_tx, state) = watch::channel(State::Connected {
            generation: 7,
            listener: "100.64.0.1:7475".parse().unwrap(),
        });
        let (map_tx, map) = watch::channel(Some(MapSnapshot {
            generation: 7,
            map: Arc::new(current.clone()),
        }));
        let auth = Authorization { state, map };
        assert!(auth.peer_has_tag(7, 2, "tag:hub"));
        assert!(!auth.peer_has_tag(6, 2, "tag:hub"));
        assert!(!auth.peer_has_tag(7, 3, "tag:hub"));
        assert_eq!(
            auth.peer_addr("hub", 7470).unwrap(),
            "100.64.0.2:7470".parse().unwrap()
        );

        current.peers.get_mut(&2).unwrap().tags.clear();
        map_tx.send_replace(Some(MapSnapshot {
            generation: 7,
            map: Arc::new(current.clone()),
        }));
        assert!(!auth.peer_has_tag(7, 2, "tag:hub"));

        current.peers.remove(&2);
        current.peers.insert(3, peer(3, "hub", "tag:hub"));
        map_tx.send_replace(Some(MapSnapshot {
            generation: 7,
            map: Arc::new(current),
        }));
        assert!(!auth.peer_has_tag(7, 2, "tag:hub"));
        assert!(auth.peer_has_tag(7, 3, "tag:hub"));

        state_tx.send_replace(State::Disconnected);
        assert!(!auth.peer_has_tag(7, 3, "tag:hub"));
        assert_eq!(
            auth.peer_addr("hub", 7470).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        state_tx.send_replace(State::Connected {
            generation: 8,
            listener: "100.64.0.1:7475".parse().unwrap(),
        });
        assert!(!auth.peer_has_tag(7, 3, "tag:hub"));
    }
}
