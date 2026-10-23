//! Bounded, userspace TCP listener for plaintext IP packets from a trusted tunnel.
//!
//! IPv4/IPv6 packets are verified before processing. The caller must enforce
//! source-address policy against the authenticated WireGuard peer. This module
//! does not perform routing, TLS, HTTP, or peer authorization.

use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

const TCP: u8 = 6;
const SYN: u8 = 0x02;
const FIN: u8 = 0x01;
const RST: u8 = 0x04;
const PSH: u8 = 0x08;
const ACK: u8 = 0x10;
const MSS: usize = 536;
const WINDOW: u16 = 65_535;
const MAX_CONNECTIONS: usize = 256;
const MAX_QUEUE: usize = 256 * 1024;
const MAX_RETRANSMITS: u8 = 6;
const RTO: Duration = Duration::from_secs(1);
const IDLE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId {
    pub remote: SocketAddr,
    pub local: SocketAddr,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Packet(Vec<u8>),
    Accepted(ConnectionId),
    /// An outbound SYN completed its three-way handshake.
    Connected(ConnectionId),
    Data(ConnectionId, Vec<u8>),
    /// The peer sent FIN. The application may still send a response.
    PeerClosed(ConnectionId),
    /// The connection was reset, expired, or fully closed.
    Closed(ConnectionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidPacket,
    UnsupportedPacket,
    WrongListener,
    NoConnection,
    NotEstablished,
    Backpressure,
    AlreadyConnected,
    Clock,
    Random,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    SynSent,
    SynReceived,
    Established,
    FinSent,
    FinReceived,
    Closing,
}

struct Outstanding {
    end: u32,
    packet: Vec<u8>,
    sent: Instant,
    retries: u8,
}
struct Connection {
    phase: Phase,
    recv_next: u32,
    send_next: u32,
    send_una: u32,
    peer_window: u16,
    pending: VecDeque<u8>,
    outstanding: VecDeque<Outstanding>,
    /// Out-of-order receive data, bounded by one advertised TCP window.
    reorder: BTreeMap<u32, Vec<u8>>,
    reorder_bytes: usize,
    duplicate_acks: u8,
    last_activity: Instant,
    close_requested: bool,
}

/// A single TCP listener. Keep one instance for the lifetime of the tunnel.
/// The caller must serialize calls and deliver `Packet` events to the tunnel.
pub struct Endpoint {
    listener: SocketAddr,
    connections: BTreeMap<ConnectionId, Connection>,
}

impl Endpoint {
    pub fn new(listener: SocketAddr) -> Self {
        Self {
            listener,
            connections: BTreeMap::new(),
        }
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Begin one outbound connection. The caller chooses an exact local
    /// tailnet IP and nonzero ephemeral port; completion arrives as Connected.
    pub fn dial(
        &mut self,
        local: SocketAddr,
        remote: SocketAddr,
        now: Instant,
    ) -> Result<Vec<Event>, Error> {
        if local.port() == 0 || remote.port() == 0 || local.ip().is_ipv4() != remote.ip().is_ipv4()
        {
            return Err(Error::InvalidPacket);
        }
        let id = ConnectionId { local, remote };
        if self.connections.contains_key(&id) {
            return Err(Error::AlreadyConnected);
        }
        if self.connections.len() >= MAX_CONNECTIONS {
            return Err(Error::Backpressure);
        }
        let mut random = [0u8; 4];
        getrandom::getrandom(&mut random).map_err(|_| Error::Random)?;
        let isn = u32::from_be_bytes(random);
        let syn = emit(id, isn, 0, SYN, WINDOW, &[])?;
        self.connections.insert(
            id,
            Connection {
                phase: Phase::SynSent,
                recv_next: 0,
                send_next: isn.wrapping_add(1),
                send_una: isn,
                peer_window: WINDOW,
                pending: VecDeque::new(),
                outstanding: VecDeque::from([Outstanding {
                    end: isn.wrapping_add(1),
                    packet: syn.clone(),
                    sent: now,
                    retries: 0,
                }]),
                reorder: BTreeMap::new(),
                reorder_bytes: 0,
                duplicate_acks: 0,
                last_activity: now,
                close_requested: false,
            },
        );
        Ok(vec![Event::Packet(syn)])
    }

    pub fn abort(&mut self, id: ConnectionId) -> bool {
        self.connections.remove(&id).is_some()
    }

    /// Extract the checked TCP tuple before admitting a packet to a connection.
    pub fn packet_id(packet: &[u8]) -> Result<ConnectionId, Error> {
        let segment = Segment::parse(packet)?;
        Ok(ConnectionId {
            local: segment.local,
            remote: segment.remote,
        })
    }

    /// Whether accepting this segment may deliver bytes or EOF to the app.
    /// ACK-only segments can continue advancing the reverse direction when a
    /// connection's application reader is backed up.
    pub fn packet_delivers_to_app(packet: &[u8]) -> Result<bool, Error> {
        let segment = Segment::parse(packet)?;
        Ok(!segment.payload.is_empty() || segment.flags & FIN != 0)
    }

    pub fn has_connection(&self, id: ConnectionId) -> bool {
        self.connections.contains_key(&id)
    }

    /// Consumes one complete plaintext IPv4 or IPv6 packet. Malformed packets
    /// return an error; unrelated packets return an empty event list.
    pub fn receive(&mut self, packet: &[u8], now: Instant) -> Result<Vec<Event>, Error> {
        let seg = Segment::parse(packet)?;
        let id = ConnectionId {
            remote: seg.remote,
            local: seg.local,
        };
        if seg.local != self.listener && !self.connections.contains_key(&id) {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        if !self.connections.contains_key(&id) {
            if seg.flags & SYN == 0 || seg.flags & (ACK | RST | FIN) != 0 {
                return Ok(out);
            }
            if self.connections.len() >= MAX_CONNECTIONS {
                return Err(Error::Backpressure);
            }
            let mut random = [0u8; 4];
            getrandom::getrandom(&mut random).map_err(|_| Error::Random)?;
            let isn = u32::from_be_bytes(random);
            let syn_ack = emit(id, isn, seg.seq.wrapping_add(1), SYN | ACK, WINDOW, &[])?;
            self.connections.insert(
                id,
                Connection {
                    phase: Phase::SynReceived,
                    recv_next: seg.seq.wrapping_add(1),
                    send_next: isn.wrapping_add(1),
                    send_una: isn,
                    peer_window: seg.window,
                    pending: VecDeque::new(),
                    outstanding: VecDeque::from([Outstanding {
                        end: isn.wrapping_add(1),
                        packet: syn_ack.clone(),
                        sent: now,
                        retries: 0,
                    }]),
                    reorder: BTreeMap::new(),
                    reorder_bytes: 0,
                    duplicate_acks: 0,
                    last_activity: now,
                    close_requested: false,
                },
            );
            out.push(Event::Packet(syn_ack));
            return Ok(out);
        }
        let mut remove = false;
        {
            let conn = self.connections.get_mut(&id).ok_or(Error::NoConnection)?;
            if now < conn.last_activity {
                return Err(Error::Clock);
            }
            conn.last_activity = now;
            if seg.flags & RST != 0 {
                if seg.seq == conn.recv_next {
                    remove = true;
                    out.push(Event::Closed(id));
                }
            } else if conn.phase == Phase::SynSent {
                if seg.flags & (SYN | ACK) == SYN | ACK
                    && seg.flags & (FIN | RST) == 0
                    && seg.ack == conn.send_next
                    && seg.payload.is_empty()
                {
                    conn.recv_next = seg.seq.wrapping_add(1);
                    conn.send_una = seg.ack;
                    conn.outstanding.clear();
                    conn.peer_window = seg.window;
                    conn.phase = Phase::Established;
                    out.push(Event::Packet(emit(
                        id,
                        conn.send_next,
                        conn.recv_next,
                        ACK,
                        WINDOW,
                        &[],
                    )?));
                    out.push(Event::Connected(id));
                }
            } else if conn.phase == Phase::SynReceived
                && seg.flags & SYN != 0
                && seg.seq.wrapping_add(1) == conn.recv_next
            {
                // A retransmitted SYN does not create a second connection.
                if let Some(first) = conn.outstanding.front() {
                    out.push(Event::Packet(first.packet.clone()));
                }
            } else if seg.flags & ACK != 0
                && (seq_between(seg.ack, conn.send_una, conn.send_next)
                    || (conn.phase != Phase::SynReceived && seq_before(seg.ack, conn.send_una)))
            {
                if seq_between(seg.ack, conn.send_una, conn.send_next) {
                    if seg.ack == conn.send_una && conn.phase != Phase::SynReceived {
                        if conn.duplicate_acks < 3 && !conn.outstanding.is_empty() {
                            conn.duplicate_acks += 1;
                            if conn.duplicate_acks == 3 {
                                if let Some(first) = conn.outstanding.front_mut() {
                                    first.sent = now;
                                    out.push(Event::Packet(first.packet.clone()));
                                }
                            }
                        }
                    } else {
                        conn.duplicate_acks = 0;
                    }
                    conn.send_una = seg.ack;
                    while conn
                        .outstanding
                        .front()
                        .is_some_and(|p| !seq_before(seg.ack, p.end))
                    {
                        conn.outstanding.pop_front();
                    }
                    conn.peer_window = seg.window;
                }
                if conn.phase == Phase::SynReceived
                    && seg.ack == conn.send_next
                    && seg.seq == conn.recv_next
                {
                    conn.phase = Phase::Established;
                    out.push(Event::Accepted(id));
                }
                if conn.phase != Phase::SynReceived {
                    if seg.flags & SYN != 0
                        && seg.seq.wrapping_add(1) == conn.recv_next
                        && seg.ack == conn.send_next
                    {
                        out.push(Event::Packet(emit(
                            id,
                            conn.send_next,
                            conn.recv_next,
                            ACK,
                            WINDOW,
                            &[],
                        )?));
                    }
                    if !seg.payload.is_empty() || seg.flags & FIN != 0 {
                        let may_receive = matches!(conn.phase, Phase::Established | Phase::FinSent);
                        let overlap = conn.recv_next.wrapping_sub(seg.seq);
                        let offset = if seg.seq == conn.recv_next {
                            Some(0)
                        } else if seq_before(seg.seq, conn.recv_next)
                            && (overlap as usize) <= seg.payload.len()
                        {
                            Some(overlap as usize)
                        } else {
                            None
                        };
                        if may_receive {
                            if let Some(offset) = offset {
                                let fresh = &seg.payload[offset..];
                                if !fresh.is_empty() {
                                    conn.recv_next = conn.recv_next.wrapping_add(
                                        u32::try_from(fresh.len())
                                            .map_err(|_| Error::InvalidPacket)?,
                                    );
                                    out.push(Event::Data(id, fresh.to_vec()));
                                }
                                drain_reordered(id, conn, &mut out);
                                if seg.flags & FIN != 0
                                    && seg.seq.wrapping_add(seg.payload.len() as u32)
                                        == conn.recv_next
                                {
                                    conn.recv_next = conn.recv_next.wrapping_add(1);
                                    conn.phase = if conn.phase == Phase::FinSent {
                                        Phase::Closing
                                    } else {
                                        Phase::FinReceived
                                    };
                                    out.push(Event::PeerClosed(id));
                                }
                            } else if !seg.payload.is_empty() {
                                let distance = seg.seq.wrapping_sub(conn.recv_next) as usize;
                                if distance > 0
                                    && distance < usize::from(WINDOW)
                                    && seg.payload.len() <= usize::from(WINDOW) - distance
                                    && conn.reorder_bytes + seg.payload.len() <= usize::from(WINDOW)
                                {
                                    use std::collections::btree_map::Entry;
                                    if let Entry::Vacant(slot) = conn.reorder.entry(seg.seq) {
                                        slot.insert(seg.payload.to_vec());
                                        conn.reorder_bytes += seg.payload.len();
                                    }
                                }
                            }
                        }
                        out.push(Event::Packet(emit(
                            id,
                            conn.send_next,
                            conn.recv_next,
                            ACK,
                            WINDOW,
                            &[],
                        )?));
                    }
                    flush(id, conn, now, &mut out)?;
                }
            }
        }
        if remove {
            self.connections.remove(&id);
        }
        Ok(out)
    }

    /// Enqueues bytes for a connected peer. No more than 256 KiB may be queued.
    /// Unacknowledged data is retransmitted by `tick`.
    pub fn send(
        &mut self,
        id: ConnectionId,
        bytes: &[u8],
        now: Instant,
    ) -> Result<Vec<Event>, Error> {
        let conn = self.connections.get_mut(&id).ok_or(Error::NoConnection)?;
        if !matches!(conn.phase, Phase::Established | Phase::FinReceived) || conn.close_requested {
            return Err(Error::NotEstablished);
        }
        if conn
            .pending
            .len()
            .checked_add(bytes.len())
            .ok_or(Error::Backpressure)?
            > MAX_QUEUE
        {
            return Err(Error::Backpressure);
        }
        conn.pending.extend(bytes);
        let mut out = Vec::new();
        flush(id, conn, now, &mut out)?;
        Ok(out)
    }

    /// Sends FIN after queued bytes have been transmitted.
    pub fn close(&mut self, id: ConnectionId, now: Instant) -> Result<Vec<Event>, Error> {
        let conn = self.connections.get_mut(&id).ok_or(Error::NoConnection)?;
        if !matches!(conn.phase, Phase::Established | Phase::FinReceived) {
            return Err(Error::NotEstablished);
        }
        conn.close_requested = true;
        let mut out = Vec::new();
        flush(id, conn, now, &mut out)?;
        Ok(out)
    }

    /// Retransmits timed out segments and discards expired connections.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let mut out = Vec::new();
        let mut expired = Vec::new();
        for (&id, conn) in &mut self.connections {
            if now < conn.last_activity || now.duration_since(conn.last_activity) >= IDLE {
                expired.push(id);
                out.push(Event::Closed(id));
                continue;
            }
            if let Some(front) = conn.outstanding.front_mut() {
                let delay = RTO.saturating_mul(1_u32 << u32::from(front.retries.min(5)));
                if now >= front.sent && now.duration_since(front.sent) >= delay {
                    if front.retries == MAX_RETRANSMITS {
                        expired.push(id);
                        out.push(Event::Closed(id));
                    } else {
                        front.retries += 1;
                        front.sent = now;
                        out.push(Event::Packet(front.packet.clone()));
                    }
                }
            }
            if conn.phase == Phase::Closing && conn.outstanding.is_empty() {
                expired.push(id);
                out.push(Event::Closed(id));
            }
        }
        for id in expired {
            self.connections.remove(&id);
        }
        out
    }
}

fn drain_reordered(id: ConnectionId, conn: &mut Connection, out: &mut Vec<Event>) {
    while let Some(seq) = conn.reorder.iter().find_map(|(&seq, _)| {
        (seq == conn.recv_next || seq_before(seq, conn.recv_next)).then_some(seq)
    }) {
        let bytes = conn.reorder.remove(&seq).expect("selected segment");
        conn.reorder_bytes -= bytes.len();
        let offset = conn.recv_next.wrapping_sub(seq) as usize;
        let fresh = bytes.get(offset..).unwrap_or_default();
        if !fresh.is_empty() {
            conn.recv_next = conn.recv_next.wrapping_add(fresh.len() as u32);
            out.push(Event::Data(id, fresh.to_vec()));
        }
    }
}

fn flush(
    id: ConnectionId,
    conn: &mut Connection,
    now: Instant,
    out: &mut Vec<Event>,
) -> Result<(), Error> {
    let in_flight = conn.send_next.wrapping_sub(conn.send_una) as usize;
    let mut allowance = usize::from(conn.peer_window).saturating_sub(in_flight);
    while !conn.pending.is_empty() && allowance > 0 {
        let len = MSS.min(conn.pending.len()).min(allowance);
        let data: Vec<u8> = conn.pending.drain(..len).collect();
        let seq = conn.send_next;
        let packet = emit(id, seq, conn.recv_next, PSH | ACK, WINDOW, &data)?;
        conn.send_next = conn
            .send_next
            .wrapping_add(u32::try_from(len).map_err(|_| Error::InvalidPacket)?);
        conn.outstanding.push_back(Outstanding {
            end: conn.send_next,
            packet: packet.clone(),
            sent: now,
            retries: 0,
        });
        out.push(Event::Packet(packet));
        allowance -= len;
    }
    if conn.close_requested
        && conn.pending.is_empty()
        && allowance > 0
        && matches!(conn.phase, Phase::Established | Phase::FinReceived)
    {
        let seq = conn.send_next;
        let packet = emit(id, seq, conn.recv_next, FIN | ACK, WINDOW, &[])?;
        conn.send_next = conn.send_next.wrapping_add(1);
        conn.outstanding.push_back(Outstanding {
            end: conn.send_next,
            packet: packet.clone(),
            sent: now,
            retries: 0,
        });
        conn.phase = if conn.phase == Phase::FinReceived {
            Phase::Closing
        } else {
            Phase::FinSent
        };
        out.push(Event::Packet(packet));
    }
    Ok(())
}
fn seq_before(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}
fn seq_between(x: u32, lo: u32, hi: u32) -> bool {
    !seq_before(x, lo) && !seq_before(hi, x)
}

struct Segment<'a> {
    local: SocketAddr,
    remote: SocketAddr,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &'a [u8],
}
impl<'a> Segment<'a> {
    fn parse(packet: &'a [u8]) -> Result<Self, Error> {
        let (src, dst, tcp) = match packet.first().map(|b| b >> 4) {
            Some(4) => {
                if packet.len() < 20 {
                    return Err(Error::InvalidPacket);
                }
                let header = usize::from(packet[0] & 15) * 4;
                let len = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
                if header < 20
                    || len < header + 20
                    || len != packet.len()
                    || packet[9] != TCP
                    || packet[6] & 0xbf != 0
                    || packet[7] != 0
                    || checksum(&packet[..header]) != 0
                {
                    return Err(Error::InvalidPacket);
                }
                (
                    IpAddr::V4([packet[12], packet[13], packet[14], packet[15]].into()),
                    IpAddr::V4([packet[16], packet[17], packet[18], packet[19]].into()),
                    &packet[header..],
                )
            }
            Some(6) => {
                if packet.len() < 60 || packet[6] != TCP {
                    return Err(Error::UnsupportedPacket);
                }
                let len = usize::from(u16::from_be_bytes([packet[4], packet[5]]));
                if len + 40 != packet.len() {
                    return Err(Error::InvalidPacket);
                }
                let src: [u8; 16] = packet[8..24].try_into().map_err(|_| Error::InvalidPacket)?;
                let dst: [u8; 16] = packet[24..40]
                    .try_into()
                    .map_err(|_| Error::InvalidPacket)?;
                (
                    IpAddr::V6(src.into()),
                    IpAddr::V6(dst.into()),
                    &packet[40..],
                )
            }
            _ => return Err(Error::UnsupportedPacket),
        };
        if tcp.len() < 20 {
            return Err(Error::InvalidPacket);
        }
        let offset = usize::from(tcp[12] >> 4) * 4;
        if offset < 20 || offset > tcp.len() || tcp[12] & 0x0f != 0 || tcp[13] & 0xc0 != 0 {
            return Err(Error::InvalidPacket);
        }
        if tcp_checksum(src, dst, tcp) != 0 {
            return Err(Error::InvalidPacket);
        }
        let remote = SocketAddr::new(src, u16::from_be_bytes([tcp[0], tcp[1]]));
        let local = SocketAddr::new(dst, u16::from_be_bytes([tcp[2], tcp[3]]));
        Ok(Self {
            local,
            remote,
            seq: u32::from_be_bytes(tcp[4..8].try_into().map_err(|_| Error::InvalidPacket)?),
            ack: u32::from_be_bytes(tcp[8..12].try_into().map_err(|_| Error::InvalidPacket)?),
            flags: tcp[13],
            window: u16::from_be_bytes([tcp[14], tcp[15]]),
            payload: &tcp[offset..],
        })
    }
}

fn emit(
    id: ConnectionId,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) -> Result<Vec<u8>, Error> {
    let (src, dst, header) = match (id.local.ip(), id.remote.ip()) {
        (IpAddr::V4(src), IpAddr::V4(dst)) => (IpAddr::V4(src), IpAddr::V4(dst), 20),
        (IpAddr::V6(src), IpAddr::V6(dst)) => (IpAddr::V6(src), IpAddr::V6(dst), 40),
        _ => return Err(Error::InvalidPacket),
    };
    let len = header + 20 + payload.len();
    if len > u16::MAX as usize {
        return Err(Error::InvalidPacket);
    }
    let mut out = vec![0u8; len];
    if let (IpAddr::V4(s), IpAddr::V4(d)) = (src, dst) {
        out[0] = 0x45;
        out[2..4].copy_from_slice(&(len as u16).to_be_bytes());
        out[6] = 0x40; // Don't fragment.
        out[8] = 64;
        out[9] = TCP;
        out[12..16].copy_from_slice(&s.octets());
        out[16..20].copy_from_slice(&d.octets());
        let sum = checksum(&out[..20]);
        out[10..12].copy_from_slice(&sum.to_be_bytes());
    } else if let (IpAddr::V6(s), IpAddr::V6(d)) = (src, dst) {
        out[0] = 0x60;
        out[4..6].copy_from_slice(&((len - 40) as u16).to_be_bytes());
        out[6] = TCP;
        out[7] = 64;
        out[8..24].copy_from_slice(&s.octets());
        out[24..40].copy_from_slice(&d.octets());
    }
    let tcp = &mut out[header..];
    tcp[0..2].copy_from_slice(&id.local.port().to_be_bytes());
    tcp[2..4].copy_from_slice(&id.remote.port().to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&window.to_be_bytes());
    tcp[20..].copy_from_slice(payload);
    let sum = tcp_checksum(src, dst, tcp);
    tcp[16..18].copy_from_slice(&sum.to_be_bytes());
    Ok(out)
}
fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        let word = u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]);
        sum += u32::from(word);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}
fn tcp_checksum(src: IpAddr, dst: IpAddr, tcp: &[u8]) -> u16 {
    let mut pseudo = Vec::with_capacity(40 + tcp.len());
    match (src, dst) {
        (IpAddr::V4(s), IpAddr::V4(d)) => {
            pseudo.extend(s.octets());
            pseudo.extend(d.octets());
            pseudo.extend([0, TCP]);
            pseudo.extend((tcp.len() as u16).to_be_bytes());
        }
        (IpAddr::V6(s), IpAddr::V6(d)) => {
            pseudo.extend(s.octets());
            pseudo.extend(d.octets());
            pseudo.extend((tcp.len() as u32).to_be_bytes());
            pseudo.extend([0, 0, 0, TCP]);
        }
        _ => return 1,
    }
    pseudo.extend(tcp);
    checksum(&pseudo)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> ConnectionId {
        ConnectionId {
            remote: "100.64.0.2:54321".parse().unwrap(),
            local: "100.64.0.1:8080".parse().unwrap(),
        }
    }
    fn peer(id: ConnectionId, seq: u32, ack: u32, flags: u8, data: &[u8]) -> Vec<u8> {
        emit(
            ConnectionId {
                remote: id.local,
                local: id.remote,
            },
            seq,
            ack,
            flags,
            WINDOW,
            data,
        )
        .unwrap()
    }
    fn packet(events: &[Event]) -> &[u8] {
        match &events[0] {
            Event::Packet(p) => p,
            _ => panic!("missing packet"),
        }
    }
    fn handshake(id: ConnectionId, now: Instant) -> (Endpoint, u32) {
        let mut ep = Endpoint::new(id.local);
        let syn = peer(id, 42, 0, SYN, &[]);
        let out = ep.receive(&syn, now).unwrap();
        assert_eq!(Segment::parse(packet(&out)).unwrap().flags, SYN | ACK);
        let synack = Segment::parse(packet(&out)).unwrap();
        assert_eq!(synack.ack, 43);
        assert_eq!(
            ep.receive(&peer(id, 43, synack.seq.wrapping_add(1), ACK, &[]), now)
                .unwrap(),
            vec![Event::Accepted(id)]
        );
        (ep, synack.seq.wrapping_add(1))
    }

    #[test]
    fn outbound_client_retransmits_lost_syn_and_data_then_reads_reply() {
        let now = Instant::now();
        let id = conn();
        let mut client = Endpoint::new("100.64.0.2:9999".parse().unwrap());
        let mut server = Endpoint::new(id.local);
        let syn = client.dial(id.remote, id.local, now).unwrap();
        assert_eq!(Segment::parse(packet(&syn)).unwrap().flags, SYN);
        // First SYN is lost. The timer sends the same sequence again.
        let retry = client.tick(now + RTO);
        assert_eq!(packet(&retry), packet(&syn));
        let synack = server.receive(packet(&retry), now + RTO).unwrap();
        // The first SYN-ACK is also lost.
        let synack_retry = server.tick(now + RTO * 2);
        assert_eq!(packet(&synack_retry), packet(&synack));
        let connected = client
            .receive(packet(&synack_retry), now + RTO * 2)
            .unwrap();
        assert!(connected.contains(&Event::Connected(ConnectionId {
            local: id.remote,
            remote: id.local
        })));
        let accepted = server.receive(packet(&connected), now + RTO * 2).unwrap();
        assert!(accepted.contains(&Event::Accepted(id)));

        let outbound = client
            .send(
                ConnectionId {
                    local: id.remote,
                    remote: id.local,
                },
                b"GET / HTTP/1.1\r\n\r\n",
                now + RTO * 2,
            )
            .unwrap();
        // Drop the first data packet and let the client's RTO resend it.
        let resent = client.tick(now + RTO * 3);
        assert_eq!(packet(&resent), packet(&outbound));
        let incoming = server.receive(packet(&resent), now + RTO * 3).unwrap();
        assert!(incoming.contains(&Event::Data(id, b"GET / HTTP/1.1\r\n\r\n".to_vec())));
        let _ = client
            .receive(packet(&incoming[1..]), now + RTO * 3)
            .unwrap();
        let response = server
            .send(id, b"HTTP/1.1 200 OK\r\n\r\n", now + RTO * 3)
            .unwrap();
        let received = client.receive(packet(&response), now + RTO * 3).unwrap();
        assert!(received.contains(&Event::Data(
            ConnectionId {
                local: id.remote,
                remote: id.local
            },
            b"HTTP/1.1 200 OK\r\n\r\n".to_vec()
        )));
    }

    #[test]
    fn lost_first_segment_recovers_buffered_followers_with_three_duplicate_acks() {
        let now = Instant::now();
        let id = conn();
        let client_id = ConnectionId {
            local: id.remote,
            remote: id.local,
        };
        let mut client = Endpoint::new(id.remote);
        let mut server = Endpoint::new(id.local);
        let syn = client.dial(client_id.local, client_id.remote, now).unwrap();
        let synack = server.receive(packet(&syn), now).unwrap();
        let connected = client.receive(packet(&synack), now).unwrap();
        let accepted = server.receive(packet(&connected), now).unwrap();
        assert!(accepted.contains(&Event::Accepted(id)));

        let payload: Vec<u8> = (0..MSS * 4).map(|index| (index % 251) as u8).collect();
        let emitted = client.send(client_id, &payload, now).unwrap();
        assert_eq!(emitted.len(), 4);
        let lost = packet(&emitted[..1]).to_vec();
        let mut fast_retransmit = None;
        for event in emitted.iter().skip(1) {
            let Event::Packet(segment) = event else {
                panic!("expected TCP packet");
            };
            let receiver_events = server.receive(segment, now).unwrap();
            assert_eq!(receiver_events.len(), 1);
            assert_eq!(
                Segment::parse(packet(&receiver_events)).unwrap().payload,
                []
            );
            let sender_events = client.receive(packet(&receiver_events), now).unwrap();
            if !sender_events.is_empty() {
                fast_retransmit = Some(packet(&sender_events).to_vec());
            }
        }
        assert_eq!(fast_retransmit.as_deref(), Some(lost.as_slice()));
        let recovered = server.receive(&fast_retransmit.unwrap(), now).unwrap();
        let reconstructed: Vec<u8> = recovered
            .iter()
            .filter_map(|event| match event {
                Event::Data(_, bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect();
        assert_eq!(reconstructed, payload);
        assert_eq!(Segment::parse(packet(&recovered[4..])).unwrap().ack, {
            let first_seq = Segment::parse(&lost).unwrap().seq;
            first_seq.wrapping_add(payload.len() as u32)
        });
        assert!(client
            .receive(packet(&recovered[4..]), now)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ipv4_handshake_data_response_and_fin() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, server_seq) = handshake(id, now);
        let events = ep
            .receive(
                &peer(id, 43, server_seq, ACK | PSH, b"GET / HTTP/1.1\r\n"),
                now,
            )
            .unwrap();
        assert_eq!(events[0], Event::Data(id, b"GET / HTTP/1.1\r\n".to_vec()));
        assert_eq!(Segment::parse(packet(&events[1..])).unwrap().ack, 59);
        let events = ep.send(id, b"HTTP/1.1 200 OK\r\n", now).unwrap();
        assert_eq!(
            Segment::parse(packet(&events)).unwrap().payload,
            b"HTTP/1.1 200 OK\r\n"
        );
        let after = server_seq.wrapping_add(17);
        assert!(ep
            .receive(&peer(id, 59, after, ACK, &[]), now)
            .unwrap()
            .is_empty());
        let events = ep
            .receive(&peer(id, 59, after, ACK | FIN, &[]), now)
            .unwrap();
        assert_eq!(events[0], Event::PeerClosed(id));
        assert_eq!(Segment::parse(packet(&events[1..])).unwrap().ack, 60);
        assert_eq!(ep.close(id, now).unwrap().len(), 1);
    }

    #[test]
    fn duplicate_data_is_not_delivered_twice_and_retransmits() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, server_seq) = handshake(id, now);
        let data = peer(id, 43, server_seq, ACK | PSH, b"abc");
        assert!(matches!(
            ep.receive(&data, now).unwrap()[0],
            Event::Data(_, _)
        ));
        let duplicate = ep.receive(&data, now).unwrap();
        assert_eq!(duplicate.len(), 1);
        assert_eq!(Segment::parse(packet(&duplicate)).unwrap().ack, 46);
        let reply = ep.send(id, b"response", now).unwrap();
        let retry = ep.tick(now + RTO);
        assert_eq!(packet(&retry), packet(&reply));
        // Exhaust the retry budget, then remove the connection.
        let mut later = now + RTO;
        for n in 1..=MAX_RETRANSMITS {
            later += RTO.saturating_mul(1_u32 << u32::from(n.min(5)));
            let events = ep.tick(later);
            if n == MAX_RETRANSMITS {
                assert_eq!(events, vec![Event::Closed(id)]);
            }
        }
        assert_eq!(ep.connection_count(), 0);
    }

    #[test]
    fn checksum_fragments_and_wrong_destination_rejected() {
        let now = Instant::now();
        let id = conn();
        let mut ep = Endpoint::new(id.local);
        let mut syn = peer(id, 10, 0, SYN, &[]);
        syn[27] ^= 1;
        assert_eq!(ep.receive(&syn, now).unwrap_err(), Error::InvalidPacket);
        let mut syn = peer(id, 10, 0, SYN, &[]);
        syn[6] |= 0x20;
        // IPv4 checksum must be updated before checking fragment policy.
        syn[10] = 0;
        syn[11] = 0;
        let sum = checksum(&syn[..20]);
        syn[10..12].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(ep.receive(&syn, now).unwrap_err(), Error::InvalidPacket);
        let wrong = ConnectionId {
            local: "100.64.0.99:8080".parse().unwrap(),
            ..id
        };
        assert!(ep
            .receive(&peer(wrong, 10, 0, SYN, &[]), now)
            .unwrap()
            .is_empty());
        assert_eq!(ep.connection_count(), 0);
    }

    #[test]
    fn ipv6_handshake_and_packet_checksums() {
        let now = Instant::now();
        let id = ConnectionId {
            remote: "[fd7a:115c:a1e0::2]:54321".parse().unwrap(),
            local: "[fd7a:115c:a1e0::1]:8080".parse().unwrap(),
        };
        let (mut ep, seq) = handshake(id, now);
        let reply = ep.send(id, b"hello", now).unwrap();
        let seg = Segment::parse(packet(&reply)).unwrap();
        assert_eq!(seg.payload, b"hello");
        assert_eq!(seg.seq, seq);
        let mut corrupt = packet(&reply).to_vec();
        corrupt[64] ^= 1;
        assert_eq!(Segment::parse(&corrupt).err(), Some(Error::InvalidPacket));
    }

    #[test]
    fn overlapping_retransmission_delivers_only_new_suffix() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, server_seq) = handshake(id, now);
        let first = ep
            .receive(&peer(id, 43, server_seq, ACK | PSH, b"abc"), now)
            .unwrap();
        assert_eq!(first[0], Event::Data(id, b"abc".to_vec()));
        let overlap = ep
            .receive(&peer(id, 44, server_seq, ACK | PSH, b"bcde"), now)
            .unwrap();
        assert_eq!(overlap[0], Event::Data(id, b"de".to_vec()));
        assert_eq!(Segment::parse(packet(&overlap[1..])).unwrap().ack, 48);
    }

    #[test]
    fn stale_ack_can_carry_fresh_request_data() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, server_seq) = handshake(id, now);
        let _sent = ep.send(id, b"a", now).unwrap();
        ep.receive(&peer(id, 43, server_seq.wrapping_add(1), ACK, &[]), now)
            .unwrap();
        let events = ep
            .receive(&peer(id, 43, server_seq, ACK | PSH, b"request"), now)
            .unwrap();
        assert_eq!(events[0], Event::Data(id, b"request".to_vec()));
    }

    #[test]
    fn local_fin_waits_for_peer_fin_before_terminal_close() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, server_seq) = handshake(id, now);
        let fin = ep.close(id, now).unwrap();
        assert_eq!(Segment::parse(packet(&fin)).unwrap().flags, FIN | ACK);
        assert_eq!(
            ep.send(id, b"late", now).unwrap_err(),
            Error::NotEstablished
        );
        let ack = ep
            .receive(&peer(id, 43, server_seq.wrapping_add(1), ACK, &[]), now)
            .unwrap();
        assert!(ack.is_empty());
        assert_eq!(ep.connection_count(), 1);
        let peer_fin = ep
            .receive(
                &peer(id, 43, server_seq.wrapping_add(1), FIN | ACK, &[]),
                now,
            )
            .unwrap();
        assert_eq!(peer_fin[0], Event::PeerClosed(id));
        assert_eq!(ep.tick(now), vec![Event::Closed(id)]);
        assert_eq!(ep.connection_count(), 0);
    }

    #[test]
    fn queued_data_obeys_peer_window() {
        let now = Instant::now();
        let id = conn();
        let (mut ep, seq) = handshake(id, now);
        let mut zero = peer(id, 43, seq, ACK, &[]);
        zero[34..36].copy_from_slice(&0u16.to_be_bytes());
        zero[36..38].fill(0);
        let tcp_sum = tcp_checksum(id.remote.ip(), id.local.ip(), &zero[20..]);
        zero[36..38].copy_from_slice(&tcp_sum.to_be_bytes());
        assert!(ep.receive(&zero, now).unwrap().is_empty());
        assert!(ep.send(id, b"queued", now).unwrap().is_empty());
        let out = ep.receive(&peer(id, 43, seq, ACK, &[]), now).unwrap();
        assert_eq!(Segment::parse(packet(&out)).unwrap().payload, b"queued");
    }
}
