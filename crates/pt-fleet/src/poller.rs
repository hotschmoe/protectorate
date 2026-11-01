//! The poller: one thread keeps the node list (headscale's `tag:node` machines and member's
//! STATUS), and each node gets a thread with its own connection that asks for presence, samples
//! and inventory on their schedules. Every round publishes the node's `pt_api::Node`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use pt_api::{EpochMs, Node};
use pt_frame::msg::HelloReply;

use crate::client::NodeClient;
use crate::headscale::{HsNode, TAG_NODE};
use crate::{Fleet, view};

#[derive(Clone, Debug)]
pub struct PollConfig {
    pub presence_every: Duration,
    pub sample_every: Duration,
    pub inventory_every: Duration,
    /// How often the node list is re-read from headscale and member.
    pub list_every: Duration,
    /// Per call; presence is also bounded by its cadence. A dial is bounded by member's 30 s.
    pub call_timeout: Duration,
}

impl Default for PollConfig {
    fn default() -> PollConfig {
        PollConfig {
            presence_every: Duration::from_secs(15),
            sample_every: Duration::from_secs(60),
            inventory_every: Duration::from_secs(600),
            list_every: Duration::from_secs(15),
            call_timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Clone, Debug)]
pub enum FleetEvent {
    Node(Box<Node>),
    /// The node left headscale (deleted or untagged); the id is headscale's.
    Removed(String),
}

/// What the poller last learned about one node.
#[derive(Clone, Debug)]
pub struct NodeView {
    pub node: Node,
    /// The agent's own answer to Hello: its node id, version and job journal epoch.
    pub hello: Option<HelloReply>,
    /// The tailnet address the poller dials.
    pub ip: Option<String>,
    /// The last round's error, while the node is unreachable.
    pub error: Option<String>,
}

impl NodeView {
    pub fn protocol_error(&self) -> Option<crate::Error> {
        let node = self.hello.as_ref()?.protocol;
        (node != pt_frame::msg::PROTOCOL).then_some(crate::Error::Protocol {
            node,
            hub: pt_frame::msg::PROTOCOL,
        })
    }
}

pub(crate) type Publish = Arc<dyn Fn(FleetEvent) + Send + Sync>;

struct Shared {
    fleet: Fleet,
    cfg: PollConfig,
    stop: AtomicBool,
    views: Mutex<BTreeMap<u64, NodeView>>,
    publish: Publish,
}

pub struct Poller {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Poller {
    pub(crate) fn start(
        fleet: Fleet,
        cfg: PollConfig,
        publish: Publish,
    ) -> std::io::Result<Poller> {
        let shared = Arc::new(Shared {
            fleet,
            cfg,
            stop: AtomicBool::new(false),
            views: Mutex::new(BTreeMap::new()),
            publish,
        });
        let s = shared.clone();
        let thread = thread::Builder::new()
            .name("fleet-poller".into())
            .spawn(move || manage(&s))?;
        Ok(Poller {
            shared,
            thread: Some(thread),
        })
    }

    /// The latest view of every node, in headscale id order.
    pub fn nodes(&self) -> Vec<NodeView> {
        lock(&self.shared.views).values().cloned().collect()
    }

    /// By headscale id.
    pub fn node(&self, id: &str) -> Option<NodeView> {
        let id: u64 = id.parse().ok()?;
        lock(&self.shared.views).get(&id).cloned()
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Sleeps up to `d`, waking early once any flag is set.
fn nap(flags: &[&AtomicBool], d: Duration) {
    let deadline = Instant::now() + d;
    loop {
        if flags.iter().any(|f| f.load(Ordering::SeqCst)) {
            return;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        thread::sleep(left.min(Duration::from_millis(100)));
    }
}

#[derive(Clone, Default)]
struct Info {
    name: String,
    ip: Option<String>,
    peer_online: bool,
    last_seen_secs: Option<i64>,
}

struct Link {
    info: Mutex<Info>,
    stop: AtomicBool,
}

fn manage(s: &Arc<Shared>) {
    let mut links: BTreeMap<u64, (Arc<Link>, JoinHandle<()>)> = BTreeMap::new();
    while !s.stop.load(Ordering::SeqCst) {
        match s.fleet.admin().nodes() {
            Ok(list) => {
                let nodes: Vec<HsNode> = list.into_iter().filter(|n| n.has_tag(TAG_NODE)).collect();
                refresh(s, &mut links, &nodes);
            }
            Err(e) => eprintln!("pt-fleet: poller: listing nodes: {e}"),
        }
        nap(&[&s.stop], s.cfg.list_every);
    }
    for (link, _) in links.values() {
        link.stop.store(true, Ordering::SeqCst);
    }
    for (_, (_, t)) in links {
        let _ = t.join();
    }
}

fn refresh(
    s: &Arc<Shared>,
    links: &mut BTreeMap<u64, (Arc<Link>, JoinHandle<()>)>,
    nodes: &[HsNode],
) {
    let status = s.fleet.status();
    if let Err(e) = &status {
        eprintln!("pt-fleet: poller: {e}");
    }
    for n in nodes {
        let info = Info {
            name: n.name.clone(),
            ip: n.ips.first().cloned(),
            peer_online: status
                .as_ref()
                .ok()
                .and_then(|st| st.peer(n.id))
                .is_some_and(|p| p.online),
            last_seen_secs: n.last_seen_secs,
        };
        if links.get(&n.id).is_some_and(|(_, t)| t.is_finished()) {
            if let Some((_, t)) = links.remove(&n.id) {
                let _ = t.join();
            }
            eprintln!("pt-fleet: restarting poller thread for node {}", n.id);
        }
        if let Some((link, _)) = links.get(&n.id) {
            *lock(&link.info) = info;
            continue;
        }
        let link = Arc::new(Link {
            info: Mutex::new(info),
            stop: AtomicBool::new(false),
        });
        let (s2, l2, id) = (s.clone(), link.clone(), n.id);
        match thread::Builder::new()
            .name(format!("fleet-node-{id}"))
            .spawn(move || poll_node(&s2, &l2, id))
        {
            Ok(t) => {
                links.insert(id, (link, t));
            }
            Err(e) => eprintln!("pt-fleet: poller thread for node {id}: {e}"),
        }
    }
    let gone: Vec<u64> = links
        .keys()
        .filter(|id| !nodes.iter().any(|n| n.id == **id))
        .copied()
        .collect();
    for id in gone {
        if let Some((link, t)) = links.remove(&id) {
            link.stop.store(true, Ordering::SeqCst);
            let _ = t.join();
        }
        lock(&s.views).remove(&id);
        (s.publish)(FleetEvent::Removed(id.to_string()));
    }
}

/// When each kind of call is next due.
struct Due {
    presence: Instant,
    sample: Instant,
    inventory: Instant,
}

fn poll_node(s: &Shared, link: &Link, id: u64) {
    let now = Instant::now();
    let mut due = Due {
        presence: now,
        sample: now,
        inventory: now,
    };
    let mut client: Option<NodeClient> = None;
    let mut view = NodeView {
        node: Node {
            id: id.to_string(),
            name: String::new(),
            online: false,
            last_seen_ms: EpochMs(0),
            node_version: String::new(),
            error: None,
            inventory: None,
            sample: None,
            presence: view::unknown_presence(),
        },
        hello: None,
        ip: None,
        error: None,
    };
    let mut last_ok: Option<EpochMs> = None;
    loop {
        if link.stop.load(Ordering::SeqCst) || s.stop.load(Ordering::SeqCst) {
            return;
        }
        let next = due.presence.min(due.sample).min(due.inventory);
        let wait = next.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            nap(&[&link.stop, &s.stop], wait);
            continue;
        }
        let info = lock(&link.info).clone();
        view.node.name = info.name.clone();
        view.ip = info.ip.clone();
        let mut result = round(s, &info, &mut client, &mut view, &mut due);
        if result.is_err() && client.as_ref().is_some_and(NodeClient::disconnected) {
            client = None;
            result = round(s, &info, &mut client, &mut view, &mut due);
        }
        match result {
            Ok(()) => {
                last_ok = Some(EpochMs::now());
                view.error = None;
            }
            Err(e) => {
                client = None;
                view.error = Some(e);
                // Retry at the presence cadence; a failed round leaves the others due.
                let retry = Instant::now() + s.cfg.presence_every;
                due.presence = retry;
                due.sample = due.sample.max(retry);
                due.inventory = due.inventory.max(retry);
            }
        }
        view.node.online = info.peer_online && result_ok(&view);
        view.node.error = view.error.clone();
        view.node.last_seen_ms = last_ok
            .unwrap_or_else(|| EpochMs(info.last_seen_secs.map_or(0, |s| s.max(0) as u64 * 1000)));
        lock(&s.views).insert(id, view.clone());
        if !link.stop.load(Ordering::SeqCst) {
            (s.publish)(FleetEvent::Node(Box::new(view.node.clone())));
        }
    }
}

fn result_ok(v: &NodeView) -> bool {
    v.error.is_none()
}

/// The calls that are due, on the node's connection (dialed first when there is none).
fn round(
    s: &Shared,
    info: &Info,
    client: &mut Option<NodeClient>,
    view: &mut NodeView,
    due: &mut Due,
) -> Result<(), String> {
    if client.is_none() {
        if !info.peer_online {
            return Err("offline in member's netmap".into());
        }
        let ip = info.ip.as_deref().ok_or("headscale lists no address")?;
        let mut c = s.fleet.connect(ip).map_err(|e| e.to_string())?;
        c.set_timeout(s.cfg.call_timeout)
            .map_err(|e| e.to_string())?;
        let hello = c.hello().map_err(|e| e.to_string())?;
        view.node.node_version = hello.node_version.clone();
        view.hello = Some(hello);
        c.require_compatible().map_err(|e| e.to_string())?;
        *client = Some(c);
    }
    let c = client.as_mut().ok_or("no connection")?;
    let now = Instant::now();
    c.set_timeout(s.cfg.call_timeout.min(s.cfg.presence_every))
        .map_err(|e| e.to_string())?;
    view.node.presence = view::presence(&c.presence().map_err(|e| e.to_string())?);
    due.presence = now + s.cfg.presence_every;
    c.set_timeout(s.cfg.call_timeout)
        .map_err(|e| e.to_string())?;
    if now >= due.inventory {
        view.node.inventory = Some(view::inventory(&c.inventory().map_err(|e| e.to_string())?));
        due.inventory = now + s.cfg.inventory_every;
    }
    if now >= due.sample {
        view.node.sample = Some(view::sample(&c.sample().map_err(|e| e.to_string())?));
        due.sample = now + s.cfg.sample_every;
    }
    Ok(())
}
