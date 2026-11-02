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
use crate::enroll::Removal;
use crate::fleet::Fleet;
use crate::headscale::{HsNode, TAG_NODE};
use crate::view;

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
    pub fn protocol_error(&self) -> Option<crate::error::Error> {
        let node = self.hello.as_ref()?.protocol;
        (node != pt_frame::msg::PROTOCOL).then_some(crate::error::Error::Protocol {
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
    /// Each polled node's link, for a rename or removal from outside the poller.
    links: Mutex<BTreeMap<u64, Arc<Link>>>,
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
            links: Mutex::new(BTreeMap::new()),
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

    /// `Fleet::rename_node`, then the node's view and link carry the new name at once. Nothing
    /// is published: the caller announces it.
    pub fn rename(&self, id: u64, to: &str) -> crate::error::Result<HsNode> {
        let s = &self.shared;
        let _g = s.fleet.nodes_lock();
        let node = s.fleet.rename_node(id, to)?;
        if let Some(link) = lock(&s.links).get(&id) {
            lock(&link.info).name = node.name.clone();
        }
        if let Some(v) = lock(&s.views).get_mut(&id) {
            v.node.name = node.name.clone();
        }
        Ok(node)
    }

    /// `Fleet::remove_node`, then the node's polling stops and its view is gone at once. Once
    /// headscale has deleted it, the view goes even when dropping the record failed. Nothing is
    /// published: the caller announces it when the second value says the poller dropped the
    /// node (it may have published the node during the removal).
    pub fn remove(&self, id: u64) -> (crate::error::Result<Removal>, bool) {
        let s = &self.shared;
        let _g = s.fleet.nodes_lock();
        let removed = s.fleet.remove_node(id);
        let listed = s
            .fleet
            .admin()
            .nodes()
            .map(|l| l.iter().any(|n| n.id == id))
            .unwrap_or(removed.is_err());
        if !listed {
            let link = lock(&s.links).remove(&id);
            let mut views = lock(&s.views);
            if let Some(link) = link {
                link.stop.store(true, Ordering::SeqCst);
            }
            views.remove(&id);
        }
        (removed, !listed)
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
        let g = s.fleet.nodes_lock();
        match s.fleet.admin().nodes() {
            Ok(list) => {
                let nodes: Vec<HsNode> = list.into_iter().filter(|n| n.has_tag(TAG_NODE)).collect();
                refresh(s, &mut links, &nodes);
            }
            Err(e) => eprintln!("pt-fleet: poller: listing nodes: {e}"),
        }
        drop(g);
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
            lock(&s.links).remove(&n.id);
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
                lock(&s.links).insert(id, link.clone());
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
        lock(&s.links).remove(&id);
        let Some((link, t)) = links.remove(&id) else {
            continue;
        };
        // A link `Poller::remove` stopped was announced by its caller.
        let announced = link.stop.swap(true, Ordering::SeqCst);
        let _ = t.join();
        lock(&s.views).remove(&id);
        if !announced {
            (s.publish)(FleetEvent::Removed(id.to_string()));
        }
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
        // Under the views lock, so a rename or removal made during the round is not undone.
        let mut views = lock(&s.views);
        if link.stop.load(Ordering::SeqCst) {
            return;
        }
        view.node.name = lock(&link.info).name.clone();
        views.insert(id, view.clone());
        (s.publish)(FleetEvent::Node(Box::new(view.node.clone())));
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

impl Fleet {
    /// Starts polling every `tag:node` machine; `publish` gets each update.
    pub fn poll(
        &self,
        cfg: PollConfig,
        publish: impl Fn(FleetEvent) + Send + Sync + 'static,
    ) -> crate::error::Result<Poller> {
        Poller::start(self.clone(), cfg, Arc::new(publish))
            .map_err(|e| crate::error::Error::new(format!("starting the poller: {e}")))
    }
}
