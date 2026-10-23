//! Bounded Tokio stream adapter for the userspace tailnet TCP listener.
//!
//! The ingress sender accepts complete, **already authenticated** plaintext IP
//! packets. The caller MUST verify that the packet source IP belongs to its
//! authenticated WireGuard peer before sending it here. This crate cannot infer
//! peer identity from plaintext IP packets.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;
use std::{
    io,
    net::{IpAddr, SocketAddr},
};

use protectorate_tailnet_tcp::{ConnectionId, Endpoint, Error as TcpError, Event};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};

const PACKET_CAPACITY: usize = 64;
const ACCEPT_CAPACITY: usize = 16;
const COMMAND_CAPACITY: usize = 32;
const STREAM_CAPACITY: usize = 64 * 1024;
const BRIDGE_CAPACITY: usize = 128;
const MAX_BRIDGE_PENDING_BYTES: usize = 2 * MAX_IP_PACKET;
const READ_CHUNK: usize = 4096;
const MAX_DEFERRED_BYTES: usize = 4 * 1024 * 1024;
const BRIDGE_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_IP_PACKET: usize = 65_575;

/// One accepted connection. `stream` implements Tokio `AsyncRead` and
/// `AsyncWrite` and can be handed to an HTTP/1.1 connection handler.
pub struct Accepted {
    pub id: ConnectionId,
    pub peer_id: u64,
    pub generation: u64,
    pub stream: DuplexStream,
}

struct AuthenticatedPacket {
    peer_id: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressError {
    Oversized,
    Full,
    Closed,
}

/// Ingress for complete packets after WireGuard authentication and source-IP
/// validation by the caller. `send` waits when the bounded queue is full.
#[derive(Clone)]
pub struct Ingress {
    sender: mpsc::Sender<AuthenticatedPacket>,
}
impl Ingress {
    pub async fn send(&self, peer_id: u64, packet: Vec<u8>) -> Result<(), IngressError> {
        if packet.len() > MAX_IP_PACKET {
            return Err(IngressError::Oversized);
        }
        self.sender
            .send(AuthenticatedPacket {
                peer_id,
                bytes: packet,
            })
            .await
            .map_err(|_| IngressError::Closed)
    }

    pub fn try_send(&self, peer_id: u64, packet: Vec<u8>) -> Result<(), IngressError> {
        self.try_send_retain(peer_id, packet)
            .map_err(|(error, _)| error)
    }

    /// Like `try_send`, but returns the packet on a full queue so a tunnel
    /// reader can pause and retry without silently losing TCP data.
    pub fn try_send_retain(
        &self,
        peer_id: u64,
        packet: Vec<u8>,
    ) -> Result<(), (IngressError, Vec<u8>)> {
        if packet.len() > MAX_IP_PACKET {
            return Err((IngressError::Oversized, packet));
        }
        self.sender
            .try_send(AuthenticatedPacket {
                peer_id,
                bytes: packet,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(packet) => (IngressError::Full, packet.bytes),
                mpsc::error::TrySendError::Closed(packet) => (IngressError::Closed, packet.bytes),
            })
    }
}

/// The network-facing ports of a running driver. All channels have finite
/// capacity. A sender blocked on `ingress` backpressures the tunnel reader.
pub struct Ports {
    pub ingress: Ingress,
    pub egress: mpsc::Receiver<Vec<u8>>,
    pub accepted: mpsc::Receiver<Accepted>,
    pub dialer: Dialer,
    stop: watch::Sender<bool>,
}

/// Bounded outbound TCP dial handle. `connect` completes only after a valid
/// SYN-ACK and the final ACK has entered the packet engine's egress queue.
#[derive(Clone)]
pub struct Dialer {
    commands: mpsc::Sender<Command>,
}
impl Dialer {
    pub async fn connect(&self, remote: SocketAddr) -> io::Result<DuplexStream> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::Dial(remote, reply))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tailnet TCP driver stopped"))?;
        tokio::time::timeout(Duration::from_secs(15), result)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tailnet TCP dial timed out"))?
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tailnet TCP driver stopped"))?
    }
}
impl Ports {
    pub fn shutdown(&self) {
        let _ = self.stop.send(true);
    }
}

enum Command {
    Dial(SocketAddr, oneshot::Sender<io::Result<DuplexStream>>),
    Write(ConnectionId, Vec<u8>),
    Close(ConnectionId),
}
struct Bridge {
    writer_tx: mpsc::Sender<BridgeEvent>,
    pending: VecDeque<BridgeEvent>,
    pending_bytes: usize,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

enum BridgeEvent {
    Data(Vec<u8>),
    PeerClosed,
}

impl Bridge {
    fn new(id: ConnectionId, bridge: DuplexStream, commands: mpsc::Sender<Command>) -> Self {
        let (reader, incoming) = tokio::io::split(bridge);
        let reader = tokio::spawn(pump_outbound(id, reader, commands));
        let (writer_tx, mut writes) = mpsc::channel(BRIDGE_CAPACITY);
        let writer = tokio::spawn(async move {
            let mut incoming = incoming;
            while let Some(event) = writes.recv().await {
                match event {
                    BridgeEvent::Data(bytes) => {
                        if incoming.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    BridgeEvent::PeerClosed => {
                        let _ = incoming.shutdown().await;
                        break;
                    }
                }
            }
        });
        Self {
            writer_tx,
            pending: VecDeque::new(),
            pending_bytes: 0,
            reader,
            writer,
        }
    }

    fn stalled(&self) -> bool {
        !self.pending.is_empty() || self.writer_tx.capacity() == 0
    }

    fn enqueue(&mut self, event: BridgeEvent) -> bool {
        self.flush_pending();
        if self.pending.is_empty() {
            match self.writer_tx.try_send(event) {
                Ok(()) => return true,
                Err(mpsc::error::TrySendError::Closed(_)) => return false,
                Err(mpsc::error::TrySendError::Full(returned)) => {
                    return self.defer(returned);
                }
            }
        }
        self.defer(event)
    }

    fn defer(&mut self, event: BridgeEvent) -> bool {
        let len = match &event {
            BridgeEvent::Data(bytes) => bytes.len(),
            BridgeEvent::PeerClosed => 0,
        };
        if self.pending_bytes.saturating_add(len) > MAX_BRIDGE_PENDING_BYTES {
            return false;
        }
        self.pending_bytes += len;
        self.pending.push_back(event);
        true
    }

    fn flush_pending(&mut self) {
        while let Some(event) = self.pending.pop_front() {
            let len = match &event {
                BridgeEvent::Data(bytes) => bytes.len(),
                BridgeEvent::PeerClosed => 0,
            };
            match self.writer_tx.try_send(event) {
                Ok(()) => self.pending_bytes -= len,
                Err(mpsc::error::TrySendError::Full(event)) => {
                    self.pending.push_front(event);
                    break;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.pending.clear();
                    self.pending_bytes = 0;
                    break;
                }
            }
        }
    }

    fn abort(self) {
        self.reader.abort();
        self.writer.abort();
    }

    fn finish(self) {
        self.reader.abort();
        tokio::spawn(async move {
            let mut writer = self.writer;
            let drain = async {
                for event in self.pending {
                    if self.writer_tx.send(event).await.is_err() {
                        return;
                    }
                }
                drop(self.writer_tx);
                let _ = (&mut writer).await;
            };
            if tokio::time::timeout(BRIDGE_DRAIN_TIMEOUT, drain)
                .await
                .is_err()
            {
                writer.abort();
            }
        });
    }
}

/// Drives one TCP listener from authenticated plaintext IP packets. Run with
/// `tokio::spawn(driver.run())`, then use the returned `Ports`.
pub struct Driver {
    endpoint: Endpoint,
    generation: u64,
    connection_peers: BTreeMap<ConnectionId, u64>,
    local_ip: IpAddr,
    next_port: u16,
    pending_dials: BTreeMap<ConnectionId, oneshot::Sender<io::Result<DuplexStream>>>,
    ingress: mpsc::Receiver<AuthenticatedPacket>,
    egress: mpsc::Sender<Vec<u8>>,
    accepted: mpsc::Sender<Accepted>,
    commands: mpsc::Receiver<Command>,
    command_tx: mpsc::Sender<Command>,
    stop: watch::Receiver<bool>,
    bridges: BTreeMap<ConnectionId, Bridge>,
    blocked_writes: BTreeMap<ConnectionId, VecDeque<Vec<u8>>>,
    deferred_bytes: usize,
    pending_closes: BTreeSet<ConnectionId>,
    #[cfg(test)]
    backpressure_seen: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Driver {
    pub fn new(listener: SocketAddr) -> (Self, Ports) {
        Self::new_with_generation(listener, 0)
    }

    pub fn new_with_generation(listener: SocketAddr, generation: u64) -> (Self, Ports) {
        let (ingress_tx, ingress) = mpsc::channel(PACKET_CAPACITY);
        let (egress, egress_rx) = mpsc::channel(PACKET_CAPACITY);
        let (accepted, accepted_rx) = mpsc::channel(ACCEPT_CAPACITY);
        let (command_tx, commands) = mpsc::channel(COMMAND_CAPACITY);
        let (stop_tx, stop) = watch::channel(false);
        (
            Self {
                endpoint: Endpoint::new(listener),
                generation,
                connection_peers: BTreeMap::new(),
                local_ip: listener.ip(),
                next_port: 49_152,
                pending_dials: BTreeMap::new(),
                ingress,
                egress,
                accepted,
                commands,
                command_tx: command_tx.clone(),
                stop,
                bridges: BTreeMap::new(),
                blocked_writes: BTreeMap::new(),
                deferred_bytes: 0,
                pending_closes: BTreeSet::new(),
                #[cfg(test)]
                backpressure_seen: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            Ports {
                ingress: Ingress { sender: ingress_tx },
                egress: egress_rx,
                accepted: accepted_rx,
                dialer: Dialer {
                    commands: command_tx.clone(),
                },
                stop: stop_tx,
            },
        )
    }

    /// Runs until shutdown, channel closure, or task cancellation. Malformed
    /// packets are dropped; all other operations remain bounded by channels.
    pub async fn run(mut self) {
        let mut ticker = interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                result = self.stop.changed() => {
                    if result.is_err() || *self.stop.borrow() { break; }
                }
                packet = self.ingress.recv() => {
                    let Some(packet) = packet else { break; };
                    if packet.bytes.len() > MAX_IP_PACKET || packet.peer_id == 0 { continue; }
                    let Ok(id) = Endpoint::packet_id(&packet.bytes) else { continue; };
                    if self.connection_peers.get(&id).is_some_and(|bound| *bound != packet.peer_id) { continue; }
                    // Leave this flow's next segment unacknowledged until its
                    // application reader makes room. TCP will retransmit it;
                    // other flows can still use the same driver.
                    if self.bridges.get(&id).is_some_and(Bridge::stalled)
                        && Endpoint::packet_delivers_to_app(&packet.bytes).unwrap_or(false)
                    { continue; }
                    let existed = self.endpoint.has_connection(id);
                    if let Ok(events) = self.endpoint.receive(&packet.bytes, Instant::now()) {
                        if !existed && self.endpoint.has_connection(id) {
                            self.connection_peers.insert(id, packet.peer_id);
                        }
                        if !self.deliver(events).await { break; }
                    }
                    if !self.retry_blocked().await { break; }
                }
                command = self.commands.recv() => {
                    let Some(command) = command else { break; };
                    if !self.command(command).await { break; }
                }
                _ = ticker.tick() => {
                    let finished: Vec<_> = self.bridges.iter()
                        .filter_map(|(&id, bridge)| bridge.writer.is_finished().then_some(id))
                        .collect();
                    for id in finished {
                        self.abort_connection(id);
                    }
                    for bridge in self.bridges.values_mut() {
                        bridge.flush_pending();
                    }
                    self.cancel_abandoned_dials();
                    let events = self.endpoint.tick(Instant::now());
                    if !self.deliver(events).await || !self.retry_blocked().await { break; }
                }
            }
        }
        for (_, bridge) in self.bridges {
            bridge.abort();
        }
    }

    async fn command(&mut self, command: Command) -> bool {
        match command {
            Command::Dial(remote, reply) => {
                let mut reply = Some(reply);
                for _ in 0..(u16::MAX - 49_152) {
                    let local = SocketAddr::new(self.local_ip, self.next_port);
                    self.next_port = if self.next_port == u16::MAX {
                        49_152
                    } else {
                        self.next_port + 1
                    };
                    let id = ConnectionId { local, remote };
                    match self.endpoint.dial(local, remote, Instant::now()) {
                        Ok(events) => {
                            self.pending_dials
                                .insert(id, reply.take().expect("dial reply"));
                            return self.deliver(events).await;
                        }
                        Err(TcpError::AlreadyConnected) => continue,
                        Err(error) => {
                            let _ = reply
                                .take()
                                .expect("dial reply")
                                .send(Err(io::Error::other(error)));
                            return true;
                        }
                    }
                }
                let _ = reply.take().expect("dial reply").send(Err(io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "tailnet ephemeral TCP ports exhausted",
                )));
                true
            }
            Command::Write(id, bytes) => {
                if self.blocked_writes.contains_key(&id) {
                    self.defer_write(id, bytes);
                    return true;
                }
                match self.endpoint.send(id, &bytes, Instant::now()) {
                    Ok(events) => self.deliver(events).await,
                    Err(TcpError::Backpressure) => {
                        self.defer_write(id, bytes);
                        true
                    }
                    Err(_) => true,
                }
            }
            Command::Close(id) => {
                if self.blocked_writes.contains_key(&id) {
                    self.pending_closes.insert(id);
                    return true;
                }
                match self.endpoint.close(id, Instant::now()) {
                    Ok(events) => self.deliver(events).await,
                    Err(_) => true,
                }
            }
        }
    }

    fn defer_write(&mut self, id: ConnectionId, bytes: Vec<u8>) {
        #[cfg(test)]
        self.backpressure_seen
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if self.deferred_bytes.saturating_add(bytes.len()) > MAX_DEFERRED_BYTES {
            self.clear_deferred(id);
            self.endpoint.abort(id);
            self.connection_peers.remove(&id);
            if let Some(bridge) = self.bridges.remove(&id) {
                bridge.abort();
            }
            if let Some(reply) = self.pending_dials.remove(&id) {
                let _ = reply.send(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "tailnet stream backpressure limit exceeded",
                )));
            }
            return;
        }
        self.deferred_bytes += bytes.len();
        self.blocked_writes.entry(id).or_default().push_back(bytes);
    }

    fn clear_deferred(&mut self, id: ConnectionId) {
        if let Some(queue) = self.blocked_writes.remove(&id) {
            self.deferred_bytes -= queue.iter().map(Vec::len).sum::<usize>();
        }
        self.pending_closes.remove(&id);
    }

    fn abort_connection(&mut self, id: ConnectionId) {
        self.endpoint.abort(id);
        self.connection_peers.remove(&id);
        self.clear_deferred(id);
        if let Some(bridge) = self.bridges.remove(&id) {
            bridge.abort();
        }
        if let Some(reply) = self.pending_dials.remove(&id) {
            let _ = reply.send(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tailnet application stream closed",
            )));
        }
    }

    fn cancel_abandoned_dials(&mut self) {
        self.pending_dials.retain(|id, reply| {
            if reply.is_closed() {
                self.endpoint.abort(*id);
                false
            } else {
                true
            }
        });
    }

    async fn retry_blocked(&mut self) -> bool {
        let ids: Vec<_> = self.blocked_writes.keys().copied().collect();
        for id in ids {
            let Some(mut queue) = self.blocked_writes.remove(&id) else {
                continue;
            };
            let Some(bytes) = queue.pop_front() else {
                continue;
            };
            self.deferred_bytes -= bytes.len();
            match self.endpoint.send(id, &bytes, Instant::now()) {
                Ok(events) => {
                    if !queue.is_empty() {
                        self.blocked_writes.insert(id, queue);
                    }
                    if !self.deliver(events).await {
                        return false;
                    }
                    if !self.blocked_writes.contains_key(&id) && self.pending_closes.remove(&id) {
                        if let Ok(events) = self.endpoint.close(id, Instant::now()) {
                            if !self.deliver(events).await {
                                return false;
                            }
                        }
                    }
                }
                Err(TcpError::Backpressure) => {
                    self.deferred_bytes += bytes.len();
                    queue.push_front(bytes);
                    self.blocked_writes.insert(id, queue);
                }
                Err(_) => {
                    self.deferred_bytes -= queue.iter().map(Vec::len).sum::<usize>();
                    self.pending_closes.remove(&id);
                }
            }
        }
        true
    }

    async fn deliver(&mut self, events: Vec<Event>) -> bool {
        let mut queue: VecDeque<Event> = events.into();
        // A single application stream may have closed while its peer still had
        // data in flight. Its receive sequence has already advanced, so drop
        // the ACK emitted after that data when abandoning this TCP connection.
        // Other connections must keep using the same packet driver.
        let mut abandoned = BTreeSet::new();
        while let Some(event) = queue.pop_front() {
            match event {
                Event::Packet(packet) => {
                    if Endpoint::packet_id(&packet).is_ok_and(|id| abandoned.contains(&id)) {
                        continue;
                    }
                    tokio::select! {
                        biased;
                        _ = self.stop.changed() => return false,
                        result = self.egress.send(packet) => if result.is_err() { return false; },
                    }
                }
                Event::Accepted(id) => {
                    let Some(peer_id) = self.connection_peers.get(&id).copied() else {
                        continue;
                    };
                    let (app, bridge) = tokio::io::duplex(STREAM_CAPACITY);
                    self.bridges
                        .insert(id, Bridge::new(id, bridge, self.command_tx.clone()));
                    if self
                        .accepted
                        .try_send(Accepted {
                            id,
                            peer_id,
                            generation: self.generation,
                            stream: app,
                        })
                        .is_err()
                    {
                        self.abort_connection(id);
                        abandoned.insert(id);
                    }
                }
                Event::Connected(id) => {
                    let Some(reply) = self.pending_dials.remove(&id) else {
                        self.endpoint.abort(id);
                        continue;
                    };
                    let (app, bridge) = tokio::io::duplex(STREAM_CAPACITY);
                    self.bridges
                        .insert(id, Bridge::new(id, bridge, self.command_tx.clone()));
                    if reply.send(Ok(app)).is_err() {
                        if let Some(bridge) = self.bridges.remove(&id) {
                            bridge.abort();
                        }
                        self.endpoint.abort(id);
                    }
                }
                Event::Data(id, bytes) => {
                    let delivered = self
                        .bridges
                        .get_mut(&id)
                        .is_some_and(|bridge| bridge.enqueue(BridgeEvent::Data(bytes)));
                    if !delivered {
                        self.abort_connection(id);
                        abandoned.insert(id);
                    }
                }
                Event::PeerClosed(id) => {
                    if self
                        .bridges
                        .get_mut(&id)
                        .is_some_and(|bridge| !bridge.enqueue(BridgeEvent::PeerClosed))
                    {
                        self.abort_connection(id);
                        abandoned.insert(id);
                    }
                }
                Event::Closed(id) => {
                    self.clear_deferred(id);
                    self.connection_peers.remove(&id);
                    if let Some(reply) = self.pending_dials.remove(&id) {
                        let _ = reply.send(Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "tailnet TCP handshake closed",
                        )));
                    }
                    if let Some(bridge) = self.bridges.remove(&id) {
                        bridge.finish();
                    }
                }
            }
        }
        true
    }
}

async fn pump_outbound(
    id: ConnectionId,
    mut reader: ReadHalf<DuplexStream>,
    commands: mpsc::Sender<Command>,
) {
    let mut buffer = [0u8; READ_CHUNK];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                let _ = commands.send(Command::Close(id)).await;
                break;
            }
            Ok(len) => {
                if commands
                    .send(Command::Write(id, buffer[..len].to_vec()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn full_ingress_returns_packet_for_lossless_retry() {
        let listener: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let (mut driver, ports) = Driver::new(listener);
        for _ in 0..PACKET_CAPACITY {
            ports.ingress.try_send(2, vec![1]).unwrap();
        }
        let (error, packet) = ports.ingress.try_send_retain(2, vec![7, 8]).unwrap_err();
        assert_eq!(error, IngressError::Full);
        assert_eq!(packet, vec![7, 8]);
        let _ = driver.ingress.recv().await.unwrap();
        ports.ingress.try_send_retain(2, packet).unwrap();
        let mut last = None;
        for _ in 0..PACKET_CAPACITY {
            last = driver.ingress.recv().await;
        }
        assert_eq!(last.unwrap().bytes, vec![7, 8]);
    }

    #[tokio::test]
    async fn closed_application_stream_does_not_stop_other_connections() {
        let listener: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let (mut driver, _ports) = Driver::new(listener);
        let closed = ConnectionId {
            local: listener,
            remote: "100.64.0.2:49152".parse().unwrap(),
        };
        let live = ConnectionId {
            local: listener,
            remote: "100.64.0.3:49153".parse().unwrap(),
        };
        let (closed_app, closed_bridge) = tokio::io::duplex(STREAM_CAPACITY);
        drop(closed_app);
        driver.bridges.insert(
            closed,
            Bridge::new(closed, closed_bridge, driver.command_tx.clone()),
        );
        driver.connection_peers.insert(closed, 2);

        let (mut live_app, live_bridge) = tokio::io::duplex(STREAM_CAPACITY);
        driver.bridges.insert(
            live,
            Bridge::new(live, live_bridge, driver.command_tx.clone()),
        );
        driver.connection_peers.insert(live, 3);

        let mut events: Vec<_> = (0..=BRIDGE_CAPACITY)
            .map(|_| Event::Data(closed, b"already closed".to_vec()))
            .collect();
        events.push(Event::Data(live, b"still connected".to_vec()));
        assert!(driver.deliver(events).await);
        let mut received = [0; 15];
        live_app.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"still connected");
        // The writer notices the closed application on its next task turn;
        // the driver's tick then reaps only that flow.
        tokio::time::timeout(Duration::from_secs(1), async {
            while !driver
                .bridges
                .get(&closed)
                .is_some_and(|bridge| bridge.writer.is_finished())
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("closed application writer did not stop");
        driver.abort_connection(closed);
        assert!(!driver.bridges.contains_key(&closed));
        assert!(!driver.connection_peers.contains_key(&closed));
        assert!(driver.bridges.contains_key(&live));
        driver.bridges.remove(&live).unwrap().abort();
    }

    #[tokio::test]
    async fn unread_application_stream_does_not_hold_up_other_connections() {
        let listener: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let (mut driver, _ports) = Driver::new(listener);
        let slow = ConnectionId {
            local: listener,
            remote: "100.64.0.2:49152".parse().unwrap(),
        };
        let live = ConnectionId {
            local: listener,
            remote: "100.64.0.3:49153".parse().unwrap(),
        };
        let (_unread_app, slow_bridge) = tokio::io::duplex(STREAM_CAPACITY);
        driver.bridges.insert(
            slow,
            Bridge::new(slow, slow_bridge, driver.command_tx.clone()),
        );
        driver.connection_peers.insert(slow, 2);
        let (mut live_app, live_bridge) = tokio::io::duplex(STREAM_CAPACITY);
        driver.bridges.insert(
            live,
            Bridge::new(live, live_bridge, driver.command_tx.clone()),
        );
        driver.connection_peers.insert(live, 3);
        let mut events: Vec<_> = (0..BRIDGE_CAPACITY + STREAM_CAPACITY / READ_CHUNK + 8)
            .map(|_| Event::Data(slow, vec![0x5a; READ_CHUNK]))
            .collect();
        events.push(Event::Data(live, b"other stream".to_vec()));
        assert!(
            tokio::time::timeout(Duration::from_secs(2), driver.deliver(events))
                .await
                .expect("one unread stream blocked the driver")
        );
        let mut received = [0u8; 12];
        tokio::time::timeout(Duration::from_secs(1), live_app.read_exact(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&received, b"other stream");
        assert!(driver.bridges.contains_key(&slow));
        assert!(driver.bridges.contains_key(&live));
        driver.bridges.remove(&live).unwrap().abort();
    }

    #[tokio::test]
    async fn full_accept_queue_does_not_hold_up_other_connections() {
        let listener: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let (mut driver, _ports) = Driver::new(listener);
        let new = ConnectionId {
            local: listener,
            remote: "100.64.0.2:49152".parse().unwrap(),
        };
        let live = ConnectionId {
            local: listener,
            remote: "100.64.0.3:49153".parse().unwrap(),
        };
        driver.connection_peers.insert(new, 2);
        for _ in 0..ACCEPT_CAPACITY {
            let (app, _) = tokio::io::duplex(1);
            driver
                .accepted
                .try_send(Accepted {
                    id: new,
                    peer_id: 2,
                    generation: 0,
                    stream: app,
                })
                .unwrap();
        }
        let (mut live_app, bridge) = tokio::io::duplex(STREAM_CAPACITY);
        driver
            .bridges
            .insert(live, Bridge::new(live, bridge, driver.command_tx.clone()));
        driver.connection_peers.insert(live, 3);
        assert!(tokio::time::timeout(
            Duration::from_secs(1),
            driver.deliver(vec![
                Event::Accepted(new),
                Event::Data(live, b"live".to_vec()),
            ])
        )
        .await
        .unwrap());
        let mut received = [0; 4];
        live_app.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"live");
        assert!(!driver.bridges.contains_key(&new));
        driver.bridges.remove(&live).unwrap().abort();
    }

    #[tokio::test]
    async fn terminal_close_delivers_queued_data_before_eof() {
        let listener: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let (mut driver, _ports) = Driver::new(listener);
        let id = ConnectionId {
            local: listener,
            remote: "100.64.0.2:49152".parse().unwrap(),
        };
        let (mut app, bridge) = tokio::io::duplex(STREAM_CAPACITY);
        driver
            .bridges
            .insert(id, Bridge::new(id, bridge, driver.command_tx.clone()));
        driver.connection_peers.insert(id, 2);
        assert!(
            driver
                .deliver(vec![
                    Event::Data(id, b"final data".to_vec()),
                    Event::PeerClosed(id),
                    Event::Closed(id),
                ])
                .await
        );
        let mut received = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), app.read_to_end(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, b"final data");
    }

    #[tokio::test]
    async fn dialer_and_listener_exchange_http_after_lost_handshake_packets() {
        let server_addr: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let client_addr: SocketAddr = "100.64.0.2:9999".parse().unwrap();
        let (server, mut server_ports) = Driver::new(server_addr);
        let (client, mut client_ports) = Driver::new(client_addr);
        let dialer = client_ports.dialer.clone();
        let client_ingress = client_ports.ingress.clone();
        let server_ingress = server_ports.ingress.clone();
        let server_stop = server_ports.stop.clone();
        let client_stop = client_ports.stop.clone();
        let server_task = tokio::spawn(server.run());
        let client_task = tokio::spawn(client.run());
        let client_to_server = tokio::spawn(async move {
            let mut first = true;
            while let Some(packet) = client_ports.egress.recv().await {
                if first {
                    first = false;
                    continue;
                } // Lost SYN.
                if server_ingress.send(2, packet).await.is_err() {
                    break;
                }
            }
        });
        let server_to_client = tokio::spawn(async move {
            let mut first = true;
            while let Some(packet) = server_ports.egress.recv().await {
                if first {
                    first = false;
                    continue;
                } // Lost SYN-ACK.
                if client_ingress.send(1, packet).await.is_err() {
                    break;
                }
            }
        });
        let mut outbound =
            tokio::time::timeout(Duration::from_secs(6), dialer.connect(server_addr))
                .await
                .unwrap()
                .unwrap();
        let mut inbound =
            tokio::time::timeout(Duration::from_secs(2), server_ports.accepted.recv())
                .await
                .unwrap()
                .unwrap()
                .stream;
        let request = b"GET / HTTP/1.1\r\nHost: node\r\n\r\n";
        outbound.write_all(request).await.unwrap();
        let mut received = vec![0; request.len()];
        tokio::time::timeout(Duration::from_secs(2), inbound.read_exact(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, request);
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        inbound.write_all(response).await.unwrap();
        let mut received = vec![0; response.len()];
        tokio::time::timeout(Duration::from_secs(2), outbound.read_exact(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, response);
        let _ = server_stop.send(true);
        let _ = client_stop.send(true);
        server_task.await.unwrap();
        client_task.await.unwrap();
        client_to_server.abort();
        server_to_client.abort();
    }

    #[tokio::test]
    async fn megabyte_request_crosses_bounded_stream_without_truncation() {
        let server_addr: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let client_addr: SocketAddr = "100.64.0.2:9999".parse().unwrap();
        let (server, mut server_ports) = Driver::new(server_addr);
        let (client, mut client_ports) = Driver::new(client_addr);
        let dialer = client_ports.dialer.clone();
        let client_ingress = client_ports.ingress.clone();
        let server_ingress = server_ports.ingress.clone();
        let server_stop = server_ports.stop.clone();
        let client_stop = client_ports.stop.clone();
        let server_task = tokio::spawn(server.run());
        let client_task = tokio::spawn(client.run());
        let client_to_server = tokio::spawn(async move {
            while let Some(packet) = client_ports.egress.recv().await {
                if server_ingress.send(2, packet).await.is_err() {
                    break;
                }
            }
        });
        let server_to_client = tokio::spawn(async move {
            while let Some(packet) = server_ports.egress.recv().await {
                if client_ingress.send(1, packet).await.is_err() {
                    break;
                }
            }
        });
        let mut outbound =
            tokio::time::timeout(Duration::from_secs(5), dialer.connect(server_addr))
                .await
                .unwrap()
                .unwrap();
        let mut inbound =
            tokio::time::timeout(Duration::from_secs(5), server_ports.accepted.recv())
                .await
                .unwrap()
                .unwrap()
                .stream;
        let payload: Vec<u8> = (0..1_500_000).map(|index| (index % 251) as u8).collect();
        let mut received = vec![0; payload.len()];
        let (written, read) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(
                outbound.write_all(&payload),
                inbound.read_exact(&mut received)
            )
        })
        .await
        .unwrap();
        written.unwrap();
        read.unwrap();
        assert_eq!(received, payload);
        let _ = server_stop.send(true);
        let _ = client_stop.send(true);
        server_task.await.unwrap();
        client_task.await.unwrap();
        client_to_server.abort();
        server_to_client.abort();
    }

    #[tokio::test]
    async fn delayed_reader_receives_complete_large_response() {
        let server_addr: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let client_addr: SocketAddr = "100.64.0.2:9999".parse().unwrap();
        let (server, mut server_ports) = Driver::new(server_addr);
        let (client, mut client_ports) = Driver::new(client_addr);
        let dialer = client_ports.dialer.clone();
        let client_ingress = client_ports.ingress.clone();
        let server_ingress = server_ports.ingress.clone();
        let server_stop = server_ports.stop.clone();
        let client_stop = client_ports.stop.clone();
        let server_task = tokio::spawn(server.run());
        let client_task = tokio::spawn(client.run());
        let client_to_server = tokio::spawn(async move {
            while let Some(packet) = client_ports.egress.recv().await {
                if server_ingress.send(2, packet).await.is_err() {
                    break;
                }
            }
        });
        let server_to_client = tokio::spawn(async move {
            while let Some(packet) = server_ports.egress.recv().await {
                if client_ingress.send(1, packet).await.is_err() {
                    break;
                }
            }
        });
        let mut client_stream = dialer.connect(server_addr).await.unwrap();
        let mut server_stream = server_ports.accepted.recv().await.unwrap().stream;
        let payload: Vec<u8> = (0..262_144).map(|index| (index % 251) as u8).collect();
        let response = [
            b"HTTP/1.1 200 OK\r\nContent-Length: 262144\r\n\r\n".as_slice(),
            payload.as_slice(),
        ]
        .concat();
        let writer = tokio::spawn(async move { server_stream.write_all(&response).await });
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut received = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(30),
            client_stream.read_to_end(&mut received),
        )
        .await
        .unwrap()
        .unwrap();
        writer.await.unwrap().unwrap();
        assert_eq!(
            received.len(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 262144\r\n\r\n".len() + payload.len()
        );
        assert!(received.starts_with(b"HTTP/1.1 200 OK\r\nContent-Length: 262144\r\n\r\n"));
        assert_eq!(&received[received.len() - payload.len()..], payload);
        let _ = server_stop.send(true);
        let _ = client_stop.send(true);
        server_task.await.unwrap();
        client_task.await.unwrap();
        client_to_server.abort();
        server_to_client.abort();
    }

    #[tokio::test]
    async fn stalled_upload_does_not_block_another_dial() {
        let server_addr: SocketAddr = "100.64.0.1:7470".parse().unwrap();
        let client_addr: SocketAddr = "100.64.0.2:9999".parse().unwrap();
        let (server, mut server_ports) = Driver::new(server_addr);
        let (client, mut client_ports) = Driver::new(client_addr);
        let backpressure_seen = client.backpressure_seen.clone();
        let dialer = client_ports.dialer.clone();
        let client_ingress = client_ports.ingress.clone();
        let server_ingress = server_ports.ingress.clone();
        let server_stop = server_ports.stop.clone();
        let client_stop = client_ports.stop.clone();
        let drop_first_ack = Arc::new(AtomicBool::new(false));
        let drop_for_link = drop_first_ack.clone();
        let server_task = tokio::spawn(server.run());
        let client_task = tokio::spawn(client.run());
        let client_to_server = tokio::spawn(async move {
            while let Some(packet) = client_ports.egress.recv().await {
                if server_ingress.send(2, packet).await.is_err() {
                    break;
                }
            }
        });
        let server_to_client = tokio::spawn(async move {
            while let Some(packet) = server_ports.egress.recv().await {
                if drop_for_link.load(Ordering::Relaxed)
                    && Endpoint::packet_id(&packet).is_ok_and(|id| id.local.port() == 49_152)
                {
                    continue;
                }
                if client_ingress.send(1, packet).await.is_err() {
                    break;
                }
            }
        });
        let mut first = dialer.connect(server_addr).await.unwrap();
        let mut first_inbound = server_ports.accepted.recv().await.unwrap().stream;
        let drain = tokio::spawn(async move {
            let mut buf = [0u8; READ_CHUNK];
            while first_inbound.read(&mut buf).await.unwrap_or(0) != 0 {}
        });
        drop_first_ack.store(true, Ordering::Relaxed);
        let upload = tokio::spawn(async move { first.write_all(&vec![0x5a; 1_500_000]).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !backpressure_seen.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("first stream never reached TCP backpressure");
        let mut second = tokio::time::timeout(Duration::from_secs(4), dialer.connect(server_addr))
            .await
            .unwrap()
            .unwrap();
        let mut second_inbound =
            tokio::time::timeout(Duration::from_secs(2), server_ports.accepted.recv())
                .await
                .unwrap()
                .unwrap()
                .stream;
        second.write_all(b"second flow").await.unwrap();
        let mut received = [0u8; 11];
        tokio::time::timeout(
            Duration::from_secs(2),
            second_inbound.read_exact(&mut received),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&received, b"second flow");
        let _ = server_stop.send(true);
        let _ = client_stop.send(true);
        server_task.await.unwrap();
        client_task.await.unwrap();
        upload.abort();
        drain.abort();
        client_to_server.abort();
        server_to_client.abort();
    }
}
