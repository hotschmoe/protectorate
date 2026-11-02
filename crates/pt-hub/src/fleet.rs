//! The hub's fleet (SPEC 3.2, 3.5): headscale and member through pt-fleet, started in the
//! background so the API is up meanwhile, the poller whose views the API and the job executors
//! read, and node changes on the event stream. Stopped with the hub.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use pt_api::{self as api, EventKind as Kind, HubStreamMessage, Level};
use pt_fleet::{Embedded, Fleet, FleetEvent, PollConfig, Poller};

use crate::hub::Hub;
use crate::log::{info, warning};
use crate::transcript::lock;

/// headscale and member as this binary carries them; `missing` says why one is absent.
#[derive(Clone, Copy, Debug)]
pub struct Net {
    pub headscale: Option<Embedded>,
    pub member: Option<Embedded>,
    pub missing: &'static str,
}

/// `serve --fleet-*`, each overriding its `config.json` key.
#[derive(Clone, Debug, Default)]
pub struct FleetFlags {
    pub dir: Option<PathBuf>,
    /// `off` turns a configured fleet off.
    pub listen: Option<String>,
    pub udp_port: Option<u16>,
    pub server_url: Option<String>,
}

#[derive(Clone)]
pub struct Up {
    pub fleet: Fleet,
    pub poller: Arc<Poller>,
}

enum Slot {
    /// No fleet configured.
    Off,
    Starting,
    Up(Up),
    /// Configured but not running, with the reason.
    Down(String),
}

pub struct FleetSlot {
    slot: Mutex<Slot>,
    cv: Condvar,
    /// The node as last streamed, without its timestamps: a poll round that changed nothing
    /// else is not streamed again.
    streamed: Mutex<BTreeMap<String, api::Node>>,
}

impl Default for FleetSlot {
    fn default() -> FleetSlot {
        FleetSlot {
            slot: Mutex::new(Slot::Off),
            cv: Condvar::new(),
            streamed: Mutex::new(BTreeMap::new()),
        }
    }
}

/// How the fleet stands, for a caller that needs it now.
pub enum Status {
    Off,
    Up(Up),
    Down(String),
}

impl FleetSlot {
    fn set(&self, s: Slot) {
        *lock(&self.slot) = s;
        self.cv.notify_all();
    }

    pub fn status(&self) -> Status {
        match &*lock(&self.slot) {
            Slot::Off => Status::Off,
            Slot::Starting => Status::Down("the fleet is starting".into()),
            Slot::Up(u) => Status::Up(u.clone()),
            Slot::Down(why) => Status::Down(why.clone()),
        }
    }

    /// The running fleet, waiting up to `timeout` while it starts.
    pub fn wait_up(&self, timeout: Duration) -> Result<Up, String> {
        let deadline = Instant::now() + timeout;
        let mut s = lock(&self.slot);
        loop {
            match &*s {
                Slot::Up(u) => return Ok(u.clone()),
                Slot::Off => return Err("no fleet is configured".into()),
                Slot::Down(why) => return Err(why.clone()),
                Slot::Starting => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("the fleet is still starting".into());
            }
            s = self
                .cv
                .wait_timeout(s, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Stops the poller, then member and headscale; returns once they have exited.
    pub fn shutdown(&self) {
        let prev = std::mem::replace(
            &mut *lock(&self.slot),
            Slot::Down("the hub is stopping".into()),
        );
        self.cv.notify_all();
        if let Slot::Up(up) = prev {
            let Up { fleet, poller } = up;
            drop(poller);
            fleet.shutdown();
            info!("fleet: headscale and member stopped");
        }
    }
}

/// A trusted proxy as the prefix headscale requires: a bare address becomes its /32 or /128.
fn proxy_prefix(p: &str) -> Result<String, String> {
    let bad = || format!("fleet.trusted_proxies: {p:?} is not an IP address or prefix");
    let (ip, len) = match p.split_once('/') {
        Some((ip, len)) => (ip, Some(len)),
        None => (p, None),
    };
    let ip: IpAddr = ip.parse().map_err(|_| bad())?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let len = match len {
        None => max,
        Some(l) => l.parse::<u8>().ok().filter(|&l| l <= max).ok_or_else(bad)?,
    };
    Ok(format!("{ip}/{len}"))
}

/// The fleet config from `config.json` and the flags, or `None` when no fleet is configured.
fn config(hub: &Hub, flags: &FleetFlags) -> Result<Option<pt_fleet::Config>, String> {
    let c = &hub.config.fleet;
    let listen = flags.listen.clone().unwrap_or_else(|| c.listen.clone());
    if listen.is_empty() || listen == "off" {
        return Ok(None);
    }
    let listen: SocketAddr = listen
        .parse()
        .map_err(|_| format!("fleet listen {listen:?} is not an address and port"))?;
    if listen.port() == 0 {
        return Err("fleet listen needs a fixed port: nodes pin the control URL".into());
    }
    let dir = match (&flags.dir, c.dir.as_str()) {
        (Some(d), _) => d.clone(),
        (None, "") => hub.home.tailnet(),
        (None, d) => PathBuf::from(d),
    };
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()
            .map_err(|e| format!("fleet dir: {e}"))?
            .join(dir)
    };
    let mut cfg = pt_fleet::Config::lan_direct(dir, PathBuf::new(), PathBuf::new(), listen);
    cfg.hub_udp_port = flags.udp_port.unwrap_or(c.udp_port);
    cfg.hub_name = c.hub_name.clone();
    cfg.trusted_proxies = c
        .trusted_proxies
        .iter()
        .map(|p| proxy_prefix(p))
        .collect::<Result<_, _>>()?;
    let url = flags
        .server_url
        .clone()
        .unwrap_or_else(|| c.server_url.clone());
    if !url.is_empty() {
        let url = url.trim_end_matches('/').to_string();
        if let Some(rest) = url.strip_prefix("https://") {
            let (host, port) = match rest.rsplit_once(':') {
                Some((h, p)) => (
                    h.to_string(),
                    p.parse()
                        .map_err(|_| format!("fleet server URL {url:?}: bad port"))?,
                ),
                None => (rest.to_string(), 443),
            };
            let ipv4: Ipv4Addr = c.derp_ipv4.parse().map_err(|_| {
                format!(
                    "an https fleet server URL needs fleet.derp_ipv4 (got {:?})",
                    c.derp_ipv4
                )
            })?;
            cfg.derp = pt_fleet::Derp {
                host,
                ipv4,
                port,
                tls: true,
            };
        } else if !url.starts_with("http://") {
            return Err(format!(
                "fleet server URL {url:?} is not http:// or https://"
            ));
        }
        cfg.server_url = url;
    }
    if let IpAddr::V4(a) = listen.ip()
        && a.is_unspecified()
        && !cfg.derp.tls
    {
        return Err(
            "a LAN-direct fleet needs a concrete listen address: nodes reach DERP at it".into(),
        );
    }
    Ok(Some(cfg))
}

fn unpack(hub: &Hub, net: &Net) -> Result<(PathBuf, PathBuf), String> {
    let (Some(hs), Some(m)) = (net.headscale, net.member) else {
        return Err(format!(
            "the fleet is disabled: this protectorate binary does not embed {} ({})",
            match (net.headscale.is_none(), net.member.is_none()) {
                (true, true) => "headscale and member",
                (true, false) => "headscale",
                _ => "member",
            },
            net.missing
        ));
    };
    let dir = hub.home.net();
    let hs_path = hs.unpack(&dir).map_err(|e| e.to_string())?;
    let m_path = m.unpack(&dir).map_err(|e| e.to_string())?;
    Ok((hs_path, m_path))
}

/// Starts the fleet in the background when one is configured. A configured fleet that cannot
/// start (no embedded binaries, a bad config, headscale failing) leaves the hub running with
/// node and job methods answering `unavailable` and the reason.
pub fn start(hub: &Arc<Hub>, flags: FleetFlags, net: Net) {
    let cfg = match config(hub, &flags) {
        Ok(None) => return info!("fleet: not configured"),
        Ok(Some(c)) => c,
        Err(e) => {
            warning!("fleet: {e}");
            return hub.fleet.set(Slot::Down(e));
        }
    };
    hub.fleet.set(Slot::Starting);
    let weak = Arc::downgrade(hub);
    let started = thread::Builder::new()
        .name("pt-fleet-start".into())
        .spawn(move || {
            let Some(hub) = weak.upgrade() else { return };
            match run(&hub, cfg, &net) {
                Ok(up) => {
                    let mut slot = lock(&hub.fleet.slot);
                    if matches!(*slot, Slot::Starting) {
                        *slot = Slot::Up(up);
                        hub.fleet.cv.notify_all();
                    } else {
                        drop(slot);
                        up.fleet.shutdown();
                    }
                }
                Err(e) => {
                    warning!("fleet: {e}");
                    hub.fleet.set(Slot::Down(e));
                }
            }
        });
    if let Err(e) = started {
        hub.fleet
            .set(Slot::Down(format!("starting the fleet thread: {e}")));
    }
}

fn run(hub: &Arc<Hub>, mut cfg: pt_fleet::Config, net: &Net) -> Result<Up, String> {
    let (headscale, member) = unpack(hub, net)?;
    info!(
        "fleet: headscale {} and member {}",
        headscale.display(),
        member.display()
    );
    cfg.headscale = headscale;
    cfg.member = member;
    let (dir, url) = (cfg.dir.clone(), cfg.server_url.clone());
    let fleet = Fleet::start(cfg).map_err(|e| e.to_string())?;
    let fp = fleet.control_fingerprint().map_err(|e| e.to_string())?;
    info!("fleet up: {} control {url} key {fp}", dir.display());
    let every = Duration::from_secs(hub.config.fleet.poll_secs.max(1));
    let poll = PollConfig {
        presence_every: every,
        list_every: every,
        ..PollConfig::default()
    };
    let weak: Weak<Hub> = Arc::downgrade(hub);
    let poller = fleet
        .poll(poll, move |e| {
            if let Some(hub) = weak.upgrade() {
                published(&hub, e);
            }
        })
        .map_err(|e| {
            fleet.shutdown();
            e.to_string()
        })?;
    Ok(Up {
        fleet,
        poller: Arc::new(poller),
    })
}

fn quiet(n: &api::Node) -> api::Node {
    let mut n = n.clone();
    n.last_seen_ms = api::EpochMs(0);
    n.presence.sampled_at_ms = None;
    n
}

/// The owner removed node `id` (`Poller::remove` announces nothing): off the stream now.
pub fn removed(hub: &Hub, id: u64) {
    let id = id.to_string();
    lock(&hub.fleet.streamed).remove(&id);
    hub.bus
        .push(|seq| HubStreamMessage::NodeRemoved { seq, id });
}

/// The owner renamed a node (`Poller::rename` announces nothing): streamed under its new name.
pub fn renamed(hub: &Hub, node: api::Node) {
    lock(&hub.fleet.streamed).insert(node.id.clone(), quiet(&node));
    hub.bus.push(|seq| HubStreamMessage::Node { seq, node });
}

/// One poller round: streamed when something besides the timestamps changed, and an event
/// line when the node came up, went down or changed presence.
fn published(hub: &Hub, e: FleetEvent) {
    match e {
        FleetEvent::Node(n) => {
            let key = quiet(&n);
            let before = lock(&hub.fleet.streamed).insert(n.id.clone(), key.clone());
            if before.as_ref() == Some(&key) {
                return;
            }
            let node = (*n).clone();
            hub.bus.push(|seq| HubStreamMessage::Node { seq, node });
            let was = before.as_ref();
            if was.map(|b| b.online) != Some(n.online) {
                let (level, text) = if n.online {
                    (Level::Info, format!("node {} is online", n.name))
                } else if was.is_some() {
                    (Level::Warn, format!("node {} went offline", n.name))
                } else {
                    (Level::Warn, format!("node {} is offline", n.name))
                };
                hub.event(Kind::Node, level, &n.name, "owner", &text);
            } else if was.is_some_and(|b| b.presence.state != n.presence.state) {
                hub.event(
                    Kind::Node,
                    Level::Info,
                    &n.name,
                    "owner",
                    &format!("node {} is {}", n.name, n.presence.state),
                );
            }
        }
        FleetEvent::Removed(id) => {
            let name = lock(&hub.fleet.streamed)
                .remove(&id)
                .map_or_else(|| id.clone(), |n| n.name);
            hub.bus
                .push(|seq| HubStreamMessage::NodeRemoved { seq, id });
            hub.event(
                Kind::Node,
                Level::Warn,
                &name,
                "owner",
                &format!("node {name} left the tailnet"),
            );
        }
    }
}
