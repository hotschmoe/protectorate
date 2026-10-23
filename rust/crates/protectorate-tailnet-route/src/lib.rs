//! Exact peer routing and inbound packet authorization for one live netmap.
use chrono::{DateTime, Utc};
use protectorate_tailnet_map::{Netmap, Node};
use serde_json::Value;
use std::{collections::BTreeMap, fmt, net::IpAddr, time::SystemTime};

const MAX_RULES: usize = 4096;
const MAX_ENTRIES: usize = 16384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
fn invalid(message: impl Into<String>) -> Error {
    Error(message.into())
}

#[derive(Debug, Clone)]
enum IpSet {
    Any,
    Prefix(IpAddr, u8),
    Range(IpAddr, IpAddr),
}
impl IpSet {
    fn parse(raw: &str) -> Result<Self, Error> {
        if raw == "*" {
            return Ok(Self::Any);
        }
        // Node capabilities are not packet grants until the packet engine can
        // verify a source's NodeCapMap. An entire cap-only rule is inert.
        if raw.starts_with("cap:") {
            return Err(invalid("capability source unsupported"));
        }
        if let Some((address, bits)) = raw.split_once('/') {
            let address: IpAddr = address
                .parse()
                .map_err(|_| invalid("invalid filter CIDR"))?;
            let bits: u8 = bits
                .parse()
                .map_err(|_| invalid("invalid filter CIDR bits"))?;
            if bits > if address.is_ipv4() { 32 } else { 128 } || !is_network(address, bits) {
                return Err(invalid("noncanonical filter CIDR"));
            }
            return Ok(Self::Prefix(address, bits));
        }
        if let Some((start, end)) = raw.split_once('-') {
            let start: IpAddr = start.parse().map_err(|_| invalid("invalid range start"))?;
            let end: IpAddr = end.parse().map_err(|_| invalid("invalid range end"))?;
            if start.is_ipv4() != end.is_ipv4() || ip_number(start) > ip_number(end) {
                return Err(invalid("invalid filter range"));
            }
            return Ok(Self::Range(start, end));
        }
        let ip: IpAddr = raw.parse().map_err(|_| invalid("invalid filter IP"))?;
        Ok(Self::Prefix(ip, if ip.is_ipv4() { 32 } else { 128 }))
    }
    fn contains(&self, ip: IpAddr) -> bool {
        match *self {
            Self::Any => true,
            Self::Prefix(address, bits) => {
                same_family(address, ip)
                    && (bits == 0 || {
                        let host = (if address.is_ipv4() { 32 } else { 128 }) - bits;
                        (ip_number(address) >> host) == (ip_number(ip) >> host)
                    })
            }
            Self::Range(start, end) => {
                same_family(start, ip)
                    && ip_number(start) <= ip_number(ip)
                    && ip_number(ip) <= ip_number(end)
            }
        }
    }
}
fn same_family(a: IpAddr, b: IpAddr) -> bool {
    a.is_ipv4() == b.is_ipv4()
}
fn ip_number(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(v) => u32::from(v) as u128,
        IpAddr::V6(v) => u128::from(v),
    }
}
fn is_network(ip: IpAddr, bits: u8) -> bool {
    let width = if ip.is_ipv4() { 32 } else { 128 };
    let remainder = width - bits;
    if remainder == 128 {
        ip_number(ip) == 0
    } else {
        ip_number(ip) & ((1u128 << remainder) - 1) == 0
    }
}
fn exact_address(raw: &str) -> Result<IpAddr, Error> {
    let (ip, bits) = raw
        .split_once('/')
        .ok_or_else(|| invalid("node address lacks CIDR"))?;
    let ip: IpAddr = ip.parse().map_err(|_| invalid("invalid node address"))?;
    let bits: u8 = bits
        .parse()
        .map_err(|_| invalid("invalid node address bits"))?;
    if bits != if ip.is_ipv4() { 32 } else { 128 } {
        return Err(invalid("nonhost node address"));
    }
    Ok(ip)
}

#[derive(Debug, Clone)]
struct Destination {
    ips: IpSet,
    first: u16,
    last: u16,
}
#[derive(Debug, Clone)]
struct Rule {
    sources: Vec<IpSet>,
    destinations: Vec<Destination>,
    protocols: Vec<u8>,
}
impl Rule {
    fn parse(value: &Value) -> Result<Self, Error> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("filter rule is not object"))?;
        if object.contains_key("SrcBits") {
            return Err(invalid("legacy SrcBits unsupported"));
        }
        let sources = object
            .get("SrcIPs")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("filter SrcIPs missing"))?;
        let destinations = object
            .get("DstPorts")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("filter DstPorts missing"))?;
        if sources.len() > MAX_ENTRIES || destinations.len() > MAX_ENTRIES {
            return Err(invalid("filter rule too large"));
        }
        let mut parsed_sources = Vec::new();
        for source in sources {
            let raw = source
                .as_str()
                .ok_or_else(|| invalid("filter source is not string"))?;
            if raw.starts_with("cap:") {
                continue;
            }
            parsed_sources.push(IpSet::parse(raw)?);
        }
        let mut parsed_destinations = Vec::new();
        for destination in destinations {
            let dest = destination
                .as_object()
                .ok_or_else(|| invalid("filter destination is not object"))?;
            if dest.contains_key("Bits") {
                return Err(invalid("legacy destination Bits unsupported"));
            }
            let ip = dest
                .get("IP")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("filter destination IP missing"))?;
            let ports = dest
                .get("Ports")
                .and_then(Value::as_object)
                .ok_or_else(|| invalid("filter ports missing"))?;
            let first = ports
                .get("First")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("filter first port missing"))?;
            let last = ports
                .get("Last")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("filter last port missing"))?;
            if first > last || last > u16::MAX as u64 {
                return Err(invalid("invalid filter port range"));
            }
            parsed_destinations.push(Destination {
                ips: IpSet::parse(ip)?,
                first: first as u16,
                last: last as u16,
            });
        }
        let protocols = match object.get("IPProto") {
            None | Some(Value::Null) => vec![6, 17, 1, 58],
            Some(Value::Array(values)) if values.is_empty() => vec![6, 17, 1, 58],
            Some(Value::Array(values)) => {
                let mut parsed = Vec::new();
                for value in values {
                    let n = value
                        .as_i64()
                        .ok_or_else(|| invalid("invalid filter protocol"))?;
                    if let Ok(n) = u8::try_from(n) {
                        parsed.push(n);
                    }
                }
                parsed
            }
            _ => return Err(invalid("invalid filter protocol list")),
        };
        Ok(Self {
            sources: parsed_sources,
            destinations: parsed_destinations,
            protocols,
        })
    }
    fn allows(&self, source: IpAddr, destination: IpAddr, protocol: u8, port: u16) -> bool {
        self.protocols.contains(&protocol)
            && self.sources.iter().any(|set| set.contains(source))
            && self
                .destinations
                .iter()
                .any(|dst| dst.ips.contains(destination) && dst.first <= port && port <= dst.last)
    }
}

#[derive(Debug, Clone)]
pub struct RouteTable {
    local: Node,
    peers: BTreeMap<u64, Node>,
    /// `None` means two peers claimed this exact IP and the address is unusable.
    by_address: BTreeMap<IpAddr, Option<u64>>,
    local_addresses: Vec<IpAddr>,
    rules: Vec<Rule>,
}

/// Metadata extracted from an authenticated tunnel packet. The caller must
/// parse and validate the IP packet before constructing this value.
#[derive(Debug, Clone, Copy)]
pub struct InboundPacket<'a> {
    pub peer_id: u64,
    pub authenticated_node_key: &'a str,
    pub source: IpAddr,
    pub destination: IpAddr,
    pub protocol: u8,
    pub destination_port: u16,
}
impl RouteTable {
    /// The caller must supply a netmap from the current authenticated poll and
    /// discard this snapshot immediately when that poll becomes stale.
    pub fn compile(map: &Netmap) -> Result<Self, Error> {
        let local = map
            .self_node
            .clone()
            .ok_or_else(|| invalid("netmap has no local node"))?;
        let local_addresses = local
            .addresses
            .iter()
            .map(|s| exact_address(s))
            .collect::<Result<Vec<_>, _>>()?;
        if local_addresses.is_empty() {
            return Err(invalid("local node has no address"));
        }
        let mut by_address = BTreeMap::new();
        for peer in map.peers.values() {
            for address in &peer.addresses {
                let ip = exact_address(address)?;
                use std::collections::btree_map::Entry;
                match by_address.entry(ip) {
                    Entry::Vacant(entry) => {
                        entry.insert(Some(peer.id));
                    }
                    Entry::Occupied(mut entry) => {
                        entry.insert(None);
                    }
                }
            }
        }
        for address in &local_addresses {
            if by_address.contains_key(address) {
                by_address.insert(*address, None);
            }
        }
        let mut rules = Vec::new();
        let mut entries = 0usize;
        for group in map.packet_filters.values() {
            for value in group {
                if rules.len() >= MAX_RULES {
                    return Err(invalid("too many filter rules"));
                }
                let rule = Rule::parse(value)?;
                entries += rule.sources.len() + rule.destinations.len() + rule.protocols.len();
                if entries > MAX_ENTRIES {
                    return Err(invalid("filter entries too large"));
                }
                rules.push(rule);
            }
        }
        Ok(Self {
            local,
            peers: map.peers.clone(),
            by_address,
            local_addresses,
            rules,
        })
    }

    pub fn route_outbound(&self, destination: IpAddr, now: SystemTime) -> Option<&Node> {
        if !usable(&self.local, now) {
            return None;
        }
        let peer = self
            .peers
            .get(self.by_address.get(&destination)?.as_ref()?)?;
        usable(peer, now).then_some(peer)
    }

    /// The tunnel must supply the node key authenticated for this packet.
    /// `port` is the destination TCP/UDP port, or zero for ICMP.
    pub fn authorize_inbound(&self, packet: InboundPacket<'_>, now: SystemTime) -> bool {
        if !self.local_addresses.contains(&packet.destination) || !usable(&self.local, now) {
            return false;
        }
        if self.by_address.get(&packet.source) != Some(&Some(packet.peer_id)) {
            return false;
        }
        if !self
            .peers
            .get(&packet.peer_id)
            .is_some_and(|peer| peer.key == packet.authenticated_node_key && usable(peer, now))
        {
            return false;
        }
        self.rules.iter().any(|rule| {
            rule.allows(
                packet.source,
                packet.destination,
                packet.protocol,
                packet.destination_port,
            )
        })
    }
}

fn usable(node: &Node, now: SystemTime) -> bool {
    // Online only describes the coordination connection; direct packets can
    // remain valid when it is false.
    if node.machine_authorized == Some(false)
        || node.unsigned_peer_api_only
        || node.key.len() != "nodekey:".len() + 64
        || !node.key.starts_with("nodekey:")
        || !node.key["nodekey:".len()..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return false;
    }
    if node
        .key_expiry
        .as_deref()
        .is_some_and(|raw| !raw.is_empty())
    {
        let Some(expiry) = node
            .key_expiry
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        else {
            return false;
        };
        let now: DateTime<Utc> = now.into();
        if expiry <= now {
            return false;
        }
    }
    true
}
