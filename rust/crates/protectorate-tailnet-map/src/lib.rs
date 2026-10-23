//! Bounded, uncompressed Tailscale map-poll decoder and netmap state.
//!
//! This crate consumes the **HTTP response body** of `/machine/map`. The caller
//! owns registration, the TS2021/HTTP transport, and reconnects. Receiving a
//! netmap alone does not establish a working tailnet connection.
//!
//! Wire format and delta semantics follow Tailscale v1.104.1 `tailcfg.MapRequest`,
//! `tailcfg.MapResponse`, and `control/controlclient/direct.go`.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Read},
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

/// The complete JSON frame cap. Protectorate tailnets are small; a larger map
/// is rejected before allocating or parsing it.
pub const MAX_FRAME: usize = 32 << 20;
const MAX_PEERS: usize = 16_384;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Protocol(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "map stream: {e}"),
            Self::Json(e) => write!(f, "map JSON: {e}"),
            Self::Protocol(e) => write!(f, "map protocol: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// Request JSON sent as the body of an authenticated `/machine/map` POST.
/// Compression is deliberately omitted; the response parser accepts JSON only.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MapRequest {
    pub version: u16,
    pub node_key: String,
    pub disco_key: String,
    pub stream: bool,
    pub keep_alive: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    endpoints: Vec<SocketAddr>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    endpoint_types: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostinfo: Option<MapHostinfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    omit_peers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_session_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_session_seq: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MapHostinfo {
    net_info: PreferredNetInfo,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PreferredNetInfo {
    #[serde(rename = "PreferredDERP")]
    preferred_derp: i64,
}
impl MapRequest {
    pub fn new(
        node_key: impl Into<String>,
        disco_key: impl Into<String>,
        version: u16,
    ) -> Result<Self> {
        let request = Self {
            version,
            node_key: node_key.into(),
            disco_key: disco_key.into(),
            stream: true,
            keep_alive: true,
            endpoints: Vec::new(),
            endpoint_types: Vec::new(),
            hostinfo: None,
            omit_peers: None,
            map_session_handle: None,
            map_session_seq: None,
        };
        check_key(&request.node_key, "nodekey:")?;
        check_key(&request.disco_key, "discokey:")?;
        if version == 0 {
            return Err(Error::Protocol("capability version is zero".into()));
        }
        Ok(request)
    }

    pub fn resume(mut self, handle: String, sequence: i64) -> Result<Self> {
        if handle.is_empty() || handle.len() > 256 || sequence < 0 {
            return Err(Error::Protocol("invalid map session resume token".into()));
        }
        self.map_session_handle = Some(handle);
        self.map_session_seq = Some(sequence);
        Ok(self)
    }

    /// Send a separate one-shot endpoint update. Streaming MapRequest is
    /// read-only at capability 68+, so PreferredDERP must be reported this way.
    pub fn preferred_derp_update(mut self, region: i64) -> Result<Self> {
        if region <= 0 || region > u16::MAX as i64 {
            return Err(Error::Protocol("invalid preferred DERP region".into()));
        }
        self.stream = false;
        self.keep_alive = false;
        self.hostinfo = Some(MapHostinfo {
            net_info: PreferredNetInfo {
                preferred_derp: region,
            },
        });
        self.omit_peers = Some(true);
        self.map_session_handle = None;
        self.map_session_seq = None;
        Ok(self)
    }

    /// Publish currently listening UDP endpoints over an authenticated,
    /// one-shot map request. Tailscale capability 68+ ignores endpoint changes
    /// on streaming requests.
    pub fn endpoint_update(mut self, endpoints: Vec<SocketAddr>) -> Result<Self> {
        if endpoints.is_empty()
            || endpoints.len() > 256
            || endpoints.iter().any(|endpoint| {
                endpoint.port() == 0
                    || endpoint.ip().is_unspecified()
                    || endpoint.ip().is_multicast()
            })
        {
            return Err(Error::Protocol("invalid local UDP endpoints".into()));
        }
        self.stream = false;
        self.keep_alive = false;
        self.omit_peers = Some(true);
        self.map_session_handle = None;
        self.map_session_seq = None;
        self.endpoint_types = vec![1; endpoints.len()]; // tailcfg.EndpointLocal
        self.endpoints = endpoints;
        Ok(self)
    }

    pub fn json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Response {
    #[serde(default)]
    map_session_handle: String,
    #[serde(default)]
    seq: i64,
    #[serde(default)]
    keep_alive: bool,
    node: Option<Node>,
    #[serde(rename = "DERPMap")]
    derp_map: Option<DerpMapUpdate>,
    peers: Option<Vec<Node>>,
    peers_changed: Option<Vec<Node>>,
    peers_removed: Option<Vec<u64>>,
    peers_changed_patch: Option<Vec<PeerChange>>,
    online_change: Option<BTreeMap<String, bool>>,
    user_profiles: Option<Vec<UserProfile>>,
    packet_filter: Option<Vec<serde_json::Value>>,
    packet_filters: Option<BTreeMap<String, Option<Vec<serde_json::Value>>>>,
    #[serde(default)]
    domain: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Node {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(default, rename = "StableID")]
    pub stable_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub user: u64,
    #[serde(default)]
    pub sharer: u64,
    pub key: String,
    #[serde(default)]
    pub machine: String,
    #[serde(default)]
    pub disco_key: String,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default, rename = "AllowedIPs")]
    pub allowed_i_ps: Option<Vec<String>>,
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default, rename = "HomeDERP")]
    pub home_derp: i64,
    #[serde(default, rename = "DERP")]
    pub legacy_derp: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub online: Option<bool>,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub machine_authorized: Option<bool>,
    #[serde(default, rename = "UnsignedPeerAPIOnly")]
    pub unsigned_peer_api_only: bool,
    #[serde(default)]
    pub key_expiry: Option<String>,
    #[serde(default)]
    pub cap: u16,
}
impl Node {
    pub fn allowed_ips(&self) -> &[String] {
        self.allowed_i_ps.as_deref().unwrap_or(&self.addresses)
    }
    fn validate(&self) -> Result<()> {
        if self.id == 0 {
            return Err(Error::Protocol("node ID is zero".into()));
        }
        check_key(&self.key, "nodekey:")?;
        if !self.machine.is_empty() {
            check_key(&self.machine, "mkey:")?;
        }
        if !self.disco_key.is_empty() {
            check_key(&self.disco_key, "discokey:")?;
        }
        if self.addresses.len() > 256
            || self.allowed_ips().len() > 1024
            || self.endpoints.len() > 256
        {
            return Err(Error::Protocol("node address list too large".into()));
        }
        for prefix in self.addresses.iter().chain(self.allowed_ips()) {
            validate_prefix(prefix)?;
        }
        for endpoint in &self.endpoints {
            SocketAddr::from_str(endpoint)
                .map_err(|_| Error::Protocol(format!("invalid endpoint {endpoint}")))?;
        }
        if self.home_derp < 0 {
            return Err(Error::Protocol("negative DERP region".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PeerChange {
    #[serde(rename = "NodeID")]
    node_id: u64,
    #[serde(default, rename = "DERPRegion")]
    derp_region: i64,
    #[serde(default)]
    cap: u16,
    endpoints: Option<Vec<String>>,
    key: Option<String>,
    disco_key: Option<String>,
    online: Option<bool>,
    last_seen: Option<String>,
    key_expiry: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserProfile {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(default)]
    pub login_name: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DerpMap {
    pub regions: BTreeMap<String, DerpRegion>,
    pub omit_default_regions: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DerpMapUpdate {
    regions: Option<BTreeMap<String, DerpRegion>>,
    #[serde(default, rename = "omitDefaultRegions")]
    omit_default_regions: bool,
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DerpRegion {
    #[serde(rename = "RegionID")]
    pub region_id: i64,
    #[serde(default)]
    pub region_code: String,
    #[serde(default)]
    pub nodes: Vec<DerpNode>,
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DerpNode {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host_name: String,
    #[serde(default)]
    pub cert_name: String,
    #[serde(default, rename = "IPv4")]
    pub ipv4: String,
    #[serde(default, rename = "IPv6")]
    pub ipv6: String,
    #[serde(default, rename = "DERPPort")]
    pub derp_port: u16,
    #[serde(default, rename = "STUNPort")]
    pub stun_port: u16,
    #[serde(default, rename = "STUNOnly")]
    pub stun_only: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Netmap {
    pub self_node: Option<Node>,
    pub peers: BTreeMap<u64, Node>,
    pub user_profiles: BTreeMap<u64, UserProfile>,
    pub derp_map: Option<DerpMap>,
    pub domain: String,
    /// Filter rules, keyed by control-assigned ID. These must be enforced by
    /// the future packet engine before any peer traffic is accepted.
    pub packet_filters: BTreeMap<String, Vec<serde_json::Value>>,
    pub session_handle: Option<String>,
    pub session_seq: i64,
    initialized: bool,
}
impl Netmap {
    /// Apply one frame transactionally. Invalid frames leave the prior map intact.
    pub fn apply_json(&mut self, json: &[u8]) -> Result<bool> {
        if json.len() > MAX_FRAME {
            return Err(Error::Protocol("map frame too large".into()));
        }
        let response: Response = serde_json::from_slice(json)?;
        if response.keep_alive {
            return Ok(false);
        }
        let mut next = self.clone();
        next.apply(response)?;
        *self = next;
        Ok(true)
    }

    /// Begin a new HTTP map poll. The first non-keepalive frame of each poll
    /// must provide self and peers anew, so no stale identity survives a
    /// reconnect. Resume tokens can be copied before calling this method.
    pub fn reset_poll(&mut self) {
        *self = Self::default();
    }

    fn apply(&mut self, response: Response) -> Result<()> {
        if !self.initialized && response.node.is_none() {
            return Err(Error::Protocol("initial map lacks self node".into()));
        }
        if let Some(node) = response.node {
            node.validate()?;
            self.self_node = Some(node);
        }
        if let Some(derp) = response.derp_map {
            let had_regions = derp.regions.is_some();
            let regions = derp.regions.unwrap_or_else(|| {
                self.derp_map
                    .as_ref()
                    .map(|map| map.regions.clone())
                    .unwrap_or_default()
            });
            if regions.len() > 128 {
                return Err(Error::Protocol("too many DERP regions".into()));
            }
            for (key, region) in &regions {
                let id: i64 = key
                    .parse()
                    .map_err(|_| Error::Protocol("invalid DERP region key".into()))?;
                if id <= 0 || id != region.region_id || region.nodes.len() > 64 {
                    return Err(Error::Protocol("invalid DERP region".into()));
                }
            }
            self.derp_map = Some(DerpMap {
                regions,
                omit_default_regions: if had_regions {
                    derp.omit_default_regions
                } else {
                    self.derp_map
                        .as_ref()
                        .is_some_and(|map| map.omit_default_regions)
                },
            });
        }
        if let Some(peers) = response.peers.filter(|peers| !peers.is_empty()) {
            if peers.len() > MAX_PEERS {
                return Err(Error::Protocol("too many peers".into()));
            }
            let mut replacement = BTreeMap::new();
            for peer in peers {
                peer.validate()?;
                if replacement.insert(peer.id, peer).is_some() {
                    return Err(Error::Protocol("duplicate peer ID".into()));
                }
            }
            self.peers = replacement;
        } else {
            if let Some(changed) = response.peers_changed {
                for peer in changed {
                    peer.validate()?;
                    self.peers.insert(peer.id, peer);
                }
            }
            if let Some(removed) = response.peers_removed {
                for id in removed {
                    self.peers.remove(&id);
                }
            }
        }
        if let Some(patches) = response.peers_changed_patch {
            for patch in patches {
                self.apply_patch(patch)?;
            }
        }
        if let Some(online) = response.online_change {
            for (raw_id, value) in online {
                let id: u64 = raw_id
                    .parse()
                    .map_err(|_| Error::Protocol("invalid online-change node ID".into()))?;
                if let Some(peer) = self.peers.get_mut(&id) {
                    peer.online = Some(value);
                    if value {
                        peer.last_seen = None;
                    }
                }
            }
        }
        if self.peers.len() > MAX_PEERS {
            return Err(Error::Protocol("too many peers".into()));
        }
        if self
            .self_node
            .as_ref()
            .is_some_and(|node| self.peers.contains_key(&node.id))
        {
            return Err(Error::Protocol("self node also appears as peer".into()));
        }
        if let Some(profiles) = response.user_profiles {
            for profile in profiles {
                if profile.id == 0 {
                    return Err(Error::Protocol("zero user ID".into()));
                }
                self.user_profiles.insert(profile.id, profile);
            }
            if self.user_profiles.len() > MAX_PEERS {
                return Err(Error::Protocol("too many users".into()));
            }
        }
        if let Some(filter) = response.packet_filter {
            self.packet_filters.insert("base".into(), filter);
        }
        if let Some(filters) = response.packet_filters {
            if filters.get("*").is_some_and(Option::is_none) {
                self.packet_filters.clear();
            }
            for (key, value) in filters {
                if key == "*" {
                    continue;
                }
                match value {
                    Some(rules) => {
                        self.packet_filters.insert(key, rules);
                    }
                    None => {
                        self.packet_filters.remove(&key);
                    }
                }
            }
        }
        if !response.domain.is_empty() {
            self.domain = response.domain;
        }
        if !response.map_session_handle.is_empty() {
            if response.map_session_handle.len() > 256 {
                return Err(Error::Protocol("map session handle too long".into()));
            }
            match self.session_handle.as_deref() {
                None => {
                    self.session_handle = Some(response.map_session_handle);
                    self.session_seq = 0;
                }
                Some(handle) if handle == response.map_session_handle => {}
                Some(_) => {
                    return Err(Error::Protocol(
                        "map session changed without a fresh poll".into(),
                    ));
                }
            }
        }
        if response.seq != 0 {
            if response.seq <= self.session_seq {
                return Err(Error::Protocol("map session sequence regressed".into()));
            }
            self.session_seq = response.seq;
        }
        self.initialized = true;
        Ok(())
    }

    fn apply_patch(&mut self, patch: PeerChange) -> Result<()> {
        let Some(peer) = self.peers.get_mut(&patch.node_id) else {
            return Ok(());
        };
        if patch.derp_region != 0 {
            peer.home_derp = patch.derp_region;
        }
        if patch.cap != 0 {
            peer.cap = patch.cap;
        }
        if let Some(endpoints) = patch.endpoints {
            peer.endpoints = endpoints;
        }
        if let Some(key) = patch.key {
            peer.key = key;
        }
        if let Some(disco) = patch.disco_key {
            peer.disco_key = disco;
        }
        if let Some(online) = patch.online {
            peer.online = Some(online);
            if online {
                peer.last_seen = None;
            }
        }
        if let Some(seen) = patch.last_seen {
            peer.last_seen = Some(seen);
        }
        if let Some(expiry) = patch.key_expiry {
            peer.key_expiry = Some(expiry);
        }
        peer.validate()
    }

    /// Address ownership lookup for a future WhoIs path. Only exact node
    /// addresses count; routed subnets cannot assert peer identity.
    pub fn peer_for_address(&self, address: IpAddr) -> Option<&Node> {
        let mut matches = self.peers.values().filter(|node| {
            node.addresses.iter().any(|prefix| {
                prefix
                    .split_once('/')
                    .and_then(|(ip, bits)| {
                        let ip = ip.parse::<IpAddr>().ok()?;
                        let bits = bits.parse::<u8>().ok()?;
                        Some((ip, bits))
                    })
                    .is_some_and(|(ip, bits)| {
                        ip == address && bits == if ip.is_ipv4() { 32 } else { 128 }
                    })
            })
        });
        let peer = matches.next()?;
        // Conflicting control addresses cannot establish peer identity.
        if matches.next().is_some() {
            None
        } else {
            Some(peer)
        }
    }

    pub fn peer_tags(&self, id: u64) -> Option<BTreeSet<&str>> {
        self.peers
            .get(&id)
            .map(|peer| peer.tags.iter().map(String::as_str).collect())
    }
}

/// Reads size-prefixed map JSON frames from an HTTP response body.
pub struct MapStream<R> {
    reader: R,
}
impl<R: Read> MapStream<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let mut header = [0u8; 4];
        match self.reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(e) => return Err(Error::Io(e)),
        }
        self.reader.read_exact(&mut header[1..])?;
        let len = u32::from_le_bytes(header) as usize;
        if len == 0 || len > MAX_FRAME {
            return Err(Error::Protocol(format!("invalid map frame length {len}")));
        }
        let mut data = vec![0u8; len];
        self.reader.read_exact(&mut data)?;
        Ok(Some(data))
    }
    pub fn next_into(&mut self, map: &mut Netmap) -> Result<Option<bool>> {
        self.next_frame()?
            .map(|frame| map.apply_json(&frame))
            .transpose()
    }
}

fn validate_prefix(value: &str) -> Result<()> {
    let (ip, bits) = value
        .split_once('/')
        .ok_or_else(|| Error::Protocol(format!("invalid prefix {value}")))?;
    let address: IpAddr = ip
        .parse()
        .map_err(|_| Error::Protocol(format!("invalid prefix {value}")))?;
    let bits: u8 = bits
        .parse()
        .map_err(|_| Error::Protocol(format!("invalid prefix {value}")))?;
    if bits > if address.is_ipv4() { 32 } else { 128 } {
        return Err(Error::Protocol(format!("invalid prefix {value}")));
    }
    Ok(())
}
fn check_key(key: &str, prefix: &str) -> Result<()> {
    let hex = key
        .strip_prefix(prefix)
        .ok_or_else(|| Error::Protocol(format!("invalid {prefix} key")))?;
    if hex.len() != 64
        || !hex.bytes().all(|b| b.is_ascii_hexdigit())
        || hex.bytes().all(|b| b == b'0')
    {
        return Err(Error::Protocol(format!("invalid {prefix} key")));
    }
    Ok(())
}
