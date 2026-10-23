//! Reconnecting Headscale map poll for a registered Rust member.
//!
//! A netmap is control data, not packet connectivity. Consumers must enforce
//! packet filters and authenticate transport peers before using its identity.

use protectorate_tailnet_discovery::DiscoIdentity;
use protectorate_tailnet_map::{MapRequest, Netmap};
use protectorate_tailnet_member::{Http2Channel, MemberIdentity, CAPABILITY_VERSION};
use serde::Deserialize;
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::Duration,
};
use tokio::sync::watch;

const MAX_POLL_BYTES: usize = 16 << 20;
const MAX_PENDING_BYTES: usize = 16 << 20;
const MAX_JOIN_BYTES: u64 = 64 << 10;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    State(String),
    Member(protectorate_tailnet_member::Error),
    Map(protectorate_tailnet_map::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "map poll state: {e}"),
            Self::State(e) => write!(f, "map poll state: {e}"),
            Self::Member(e) => write!(f, "map poll channel: {e}"),
            Self::Map(e) => write!(f, "map poll frame: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<protectorate_tailnet_member::Error> for Error {
    fn from(value: protectorate_tailnet_member::Error) -> Self {
        Self::Member(value)
    }
}
impl From<protectorate_tailnet_map::Error> for Error {
    fn from(value: protectorate_tailnet_map::Error) -> Self {
        Self::Map(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// Latest control map and the poll generation that produced it. A consumer
/// must discard packet sessions whenever generation changes, even if it never
/// observed the short disconnected state between two polls.
#[derive(Clone, Default)]
pub struct PollUpdate {
    pub generation: u64,
    pub map: Option<Netmap>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinState {
    control_url: String,
    node_key: String,
    disco_key: String,
    #[serde(default)]
    control_ca_pem: Option<String>,
}

/// Loads only a node that completed `protectorate-rs node join` and still has
/// the same durable private node and disco keys.
pub struct Poller {
    directory: PathBuf,
    origin: String,
    ca_pem: Option<Vec<u8>>,
    node_key: String,
    disco_key: String,
    timeout: Duration,
    snapshot: watch::Sender<PollUpdate>,
    generation: AtomicU64,
    active: AtomicBool,
}

impl Poller {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let path = directory.join("join.json");
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_JOIN_BYTES {
            return Err(Error::State(
                "join.json must be a bounded regular file".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(Error::State("join.json must be owner-only".into()));
            }
        }
        let state: JoinState = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|e| Error::State(format!("invalid join.json: {e}")))?;
        let member = MemberIdentity::open(directory)?;
        if member.node_public_key() != state.node_key {
            return Err(Error::State(
                "join node key differs from durable identity".into(),
            ));
        }
        let disco = DiscoIdentity::open(directory)
            .map_err(|e| Error::State(format!("durable disco identity: {e}")))?;
        let disco_key = disco.public_key().to_string();
        if disco_key != state.disco_key {
            return Err(Error::State(
                "join disco key differs from durable identity".into(),
            ));
        }
        if !state.control_url.starts_with("http://") && !state.control_url.starts_with("https://") {
            return Err(Error::State("join control URL has invalid scheme".into()));
        }
        if state.control_ca_pem.is_some() && !state.control_url.starts_with("https://") {
            return Err(Error::State("private control CA requires HTTPS".into()));
        }
        let ca_pem = state.control_ca_pem.map(String::into_bytes);
        MapRequest::new(&state.node_key, &disco_key, CAPABILITY_VERSION)?;
        let (snapshot, _) = watch::channel(PollUpdate::default());
        Ok(Self {
            directory: directory.to_path_buf(),
            origin: state.control_url,
            ca_pem,
            node_key: state.node_key,
            disco_key,
            // Headscale can leave a healthy streaming map idle beyond twenty
            // seconds before its next keepalive. A shorter socket deadline
            // repeatedly clears the authoritative map and drops live peers.
            timeout: Duration::from_secs(60),
            snapshot,
            generation: AtomicU64::new(0),
            active: AtomicBool::new(false),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(Error::State("poll timeout must be 1..60 seconds".into()));
        }
        self.timeout = timeout;
        Ok(self)
    }

    /// None means the map stream has disconnected or has not initialized.
    pub fn snapshot(&self) -> Option<Netmap> {
        self.snapshot.borrow().map.clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<PollUpdate> {
        self.snapshot.subscribe()
    }

    /// Report UDP sockets that are actually listening for direct peer traffic.
    /// The control channel authenticates this one-shot update with the joined
    /// member identity; callers should withdraw or replace stale endpoints.
    pub fn report_endpoints(&self, endpoints: &[std::net::SocketAddr]) -> Result<()> {
        let member = MemberIdentity::open(&self.directory)?;
        if member.node_public_key() != self.node_key {
            return Err(Error::State("durable node identity changed".into()));
        }
        let request = MapRequest::new(&self.node_key, &self.disco_key, CAPABILITY_VERSION)?
            .endpoint_update(endpoints.to_vec())?
            .json()?;
        let channel = member.connect_control_with_ca_pem(
            &self.origin,
            self.timeout,
            self.ca_pem.as_deref(),
        )?;
        let mut http = Http2Channel::new(channel, &self.origin)?;
        http.post("/machine/map", &request, 1 << 20)?;
        Ok(())
    }

    /// Polls until stopped, backing off after disconnected or malformed
    /// streams. The read timeout bounds cancellation while no DATA arrives.
    pub fn run(&self, stop: &AtomicBool) {
        let mut delay = Duration::from_millis(250);
        while !stop.load(Ordering::Acquire) {
            let result = self.poll_once(stop);
            self.publish(None);
            if stop.load(Ordering::Acquire) {
                break;
            }
            if let Err(error) = &result {
                eprintln!("tailnet map poll: {error}");
            }
            if result.is_ok() {
                delay = Duration::from_millis(250);
            } else {
                delay = (delay * 2).min(Duration::from_secs(10));
            }
            let mut elapsed = Duration::ZERO;
            while elapsed < delay && !stop.load(Ordering::Acquire) {
                let slice = (delay - elapsed).min(Duration::from_millis(50));
                thread::sleep(slice);
                elapsed += slice;
            }
        }
        self.publish(None);
    }

    /// Executes one authenticated stream. Returns when it ends, times out,
    /// exceeds the bounded response size, or is stopped after a DATA frame.
    pub fn poll_once(&self, stop: &AtomicBool) -> Result<()> {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::State("map poll is already running".into()));
        }
        let _inactive_on_exit = InactiveOnExit(self);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.publish(None);
        let _clear_on_exit = ClearOnExit(self);
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let member = MemberIdentity::open(&self.directory)?;
        if member.node_public_key() != self.node_key {
            return Err(Error::State("durable node identity changed".into()));
        }
        let channel = member.connect_control_with_ca_pem(
            &self.origin,
            self.timeout,
            self.ca_pem.as_deref(),
        )?;
        let mut http = Http2Channel::new(channel, &self.origin)?;
        let request =
            MapRequest::new(&self.node_key, &self.disco_key, CAPABILITY_VERSION)?.json()?;
        let mut decoder = Decoder::default();
        let mut fault = None;
        let mut reported_region = None;
        let result = http.post_stream("/machine/map", &request, MAX_POLL_BYTES, |chunk| {
            if stop.load(Ordering::Acquire) {
                return Ok(true);
            }
            let mut preferred_region = None;
            if let Err(error) = decoder.feed(chunk, |map| {
                if map
                    .self_node
                    .as_ref()
                    .is_some_and(|node| node.key == self.node_key)
                {
                    self.publish(Some(map.clone()));
                    if map
                        .self_node
                        .as_ref()
                        .is_some_and(|node| node.home_derp == 0)
                        && reported_region.is_none()
                    {
                        preferred_region = select_preferred_derp(map);
                    }
                }
            }) {
                fault = Some(error);
                return Ok(true);
            }
            if let Some(region) = preferred_region {
                if let Err(error) = self.report_preferred_derp(&member, region) {
                    fault = Some(error);
                    return Ok(true);
                }
                reported_region = Some(region);
            }
            if decoder
                .map
                .self_node
                .as_ref()
                .is_some_and(|node| node.key != self.node_key)
            {
                fault = Some(Error::State(
                    "map self node key differs from joined identity".into(),
                ));
                return Ok(true);
            }
            Ok(false)
        });
        if let Some(error) = fault {
            return Err(error);
        }
        result?;
        decoder.finish()?;
        Ok(())
    }

    fn report_preferred_derp(&self, member: &MemberIdentity, region: i64) -> Result<()> {
        let request = MapRequest::new(&self.node_key, &self.disco_key, CAPABILITY_VERSION)?
            .preferred_derp_update(region)?
            .json()?;
        let channel = member.connect_control_with_ca_pem(
            &self.origin,
            self.timeout,
            self.ca_pem.as_deref(),
        )?;
        let mut http = Http2Channel::new(channel, &self.origin)?;
        http.post("/machine/map", &request, 1 << 20)?;
        Ok(())
    }

    fn publish(&self, map: Option<Netmap>) {
        self.snapshot.send_replace(PollUpdate {
            generation: self.generation.load(Ordering::Acquire),
            map,
        });
    }
}

fn select_preferred_derp(map: &Netmap) -> Option<i64> {
    let regions = &map.derp_map.as_ref()?.regions;
    let mut candidates = regions.values().filter(|region| {
        region.region_id > 0
            && region.nodes.iter().any(|node| {
                !node.stun_only
                    && node.derp_port > 0
                    && node
                        .cert_name
                        .strip_prefix("sha256-raw:")
                        .is_some_and(|pin| {
                            pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit())
                        })
            })
    });
    let region = candidates.next()?.region_id;
    candidates.next().is_none().then_some(region)
}

struct InactiveOnExit<'a>(&'a Poller);
impl Drop for InactiveOnExit<'_> {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::Release);
    }
}

struct ClearOnExit<'a>(&'a Poller);
impl Drop for ClearOnExit<'_> {
    fn drop(&mut self) {
        self.0.publish(None);
    }
}

#[derive(Default)]
struct Decoder {
    pending: Vec<u8>,
    map: Netmap,
}
impl Decoder {
    fn feed(&mut self, chunk: &[u8], mut publish: impl FnMut(&Netmap)) -> Result<()> {
        if self.pending.len().saturating_add(chunk.len()) > MAX_PENDING_BYTES {
            return Err(Error::State("map stream buffer too large".into()));
        }
        self.pending.extend_from_slice(chunk);
        loop {
            if self.pending.len() < 4 {
                return Ok(());
            }
            let size = u32::from_le_bytes(self.pending[..4].try_into().unwrap()) as usize;
            if size == 0 || size > MAX_PENDING_BYTES - 4 {
                return Err(Error::State("invalid map frame length".into()));
            }
            if self.pending.len() < 4 + size {
                return Ok(());
            }
            if self.map.apply_json(&self.pending[4..4 + size])? {
                publish(&self.map);
            }
            self.pending.drain(..4 + size);
        }
    }

    fn finish(&self) -> Result<()> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(Error::State("truncated map frame".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> Vec<u8> {
        let mut out = (json.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(json.as_bytes());
        out
    }

    #[test]
    fn fragmented_map_then_delta() {
        let key = format!("nodekey:{}", "12".repeat(32));
        let first = frame(&format!(
            "{{\"Node\":{{\"ID\":1,\"Key\":\"{key}\"}},\"Peers\":[]}}"
        ));
        let delta = frame(&format!(
            "{{\"PeersChanged\":[{{\"ID\":2,\"Key\":\"{key}\"}}]}}"
        ));
        let mut decoder = Decoder::default();
        let mut snapshots = Vec::new();
        for bytes in first.chunks(3).chain(delta.chunks(7)) {
            decoder
                .feed(bytes, |map| snapshots.push(map.clone()))
                .unwrap();
        }
        decoder.finish().unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].self_node.as_ref().unwrap().id, 1);
        assert!(snapshots[1].peers.contains_key(&2));
    }

    #[test]
    fn rejects_truncated_and_oversized_frames() {
        let mut decoder = Decoder::default();
        decoder.feed(&[3, 0, 0], |_| {}).unwrap();
        assert!(decoder.finish().is_err());
        assert!(Decoder::default()
            .feed(&u32::MAX.to_le_bytes(), |_| {})
            .is_err());
    }

    #[test]
    fn joined_state_must_match_private_node_key() {
        let root = tempfile::tempdir().unwrap();
        let member = MemberIdentity::open(root.path()).unwrap();
        let disco = DiscoIdentity::open(root.path())
            .unwrap()
            .public_key()
            .to_string();
        let path = root.path().join("join.json");
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "controlUrl":"http://127.0.0.1:1234", "nodeKey":member.node_public_key(), "discoKey":disco
        })).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let poller = Poller::open(root.path()).unwrap();
        let updates = poller.subscribe();
        let stop = AtomicBool::new(true);
        poller.poll_once(&stop).unwrap();
        assert_eq!(updates.borrow().generation, 1);
        assert!(updates.borrow().map.is_none());
        poller.poll_once(&stop).unwrap();
        assert_eq!(updates.borrow().generation, 2);
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "controlUrl":"http://127.0.0.1:1234", "nodeKey":member.node_public_key(), "discoKey":format!("discokey:{}", "a".repeat(64))
        })).unwrap()).unwrap();
        assert!(Poller::open(root.path()).is_err());
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "controlUrl":"http://127.0.0.1:1234", "nodeKey":format!("nodekey:{}", "a".repeat(64)), "discoKey":disco
        })).unwrap()).unwrap();
        assert!(Poller::open(root.path()).is_err());
    }

    #[test]
    fn private_ca_trust_is_loaded_only_for_https() {
        let root = tempfile::tempdir().unwrap();
        let member = MemberIdentity::open(root.path()).unwrap();
        let disco = DiscoIdentity::open(root.path())
            .unwrap()
            .public_key()
            .to_string();
        let path = root.path().join("join.json");
        let write = |origin: &str| {
            let state = serde_json::json!({
                "controlUrl": origin,
                "nodeKey": member.node_public_key(),
                "discoKey": disco,
                "controlCaPem": "-----BEGIN CERTIFICATE-----\nexample\n-----END CERTIFICATE-----\n"
            });
            fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
        };
        write("https://control.example");
        let poller = Poller::open(root.path()).unwrap();
        assert!(poller.ca_pem.as_deref().unwrap().starts_with(b"-----BEGIN"));
        write("http://127.0.0.1:1234");
        assert!(Poller::open(root.path()).is_err());
    }
}
