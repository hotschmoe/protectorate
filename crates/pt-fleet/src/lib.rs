//! The hub's side of the fleet (SPEC 3.2, 3.5): headscale as a sandboxed, supervised child,
//! administered through its own CLI; the hub's `member` and its DIAL socket; enrollment; a client
//! for the node agent's RPCs; and a poller that turns them into `pt_api::Node` values. See
//! README.md for the API.

#![forbid(unsafe_code)]

mod child;
mod client;
mod enroll;
mod headscale;
mod member;
mod poller;
mod view;

use std::fmt;
use std::fs::{self, OpenOptions};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

pub use child::ChildState;
pub use client::NodeClient;
pub use enroll::{Enrolled, PENDING_TTL_MS, Pending, Registration, valid_name};
pub use headscale::{Admin, HsNode, TAG_HUB, TAG_NODE, USER};
pub use member::{Peer, Status};
pub use poller::{FleetEvent, NodeView, PollConfig, Poller};
pub use pt_embed::Embedded;

/// The tailnet port of the node agent (go/policy.hujson).
pub const NODE_PORT: u16 = 7475;

/// The one DERP node in the map, pinned by IP (no DNS, no public DERP).
#[derive(Clone, Debug)]
pub struct Derp {
    /// Hostname, also the TLS name when `tls`.
    pub host: String,
    pub ipv4: Ipv4Addr,
    pub port: u16,
    /// false: plain HTTP straight to headscale (LAN-direct tests); every member is told so.
    pub tls: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The tailnet directory: headscale's state and keys, the hub member's keys and the
    /// enrollment records. A hub adopts an existing one by pointing here.
    pub dir: PathBuf,
    pub headscale: PathBuf,
    pub member: PathBuf,
    pub bwrap: PathBuf,
    /// What nodes use as control URL: `https://protectorate.hotschmoe.com` behind Traefik, or
    /// `http://<listen>` LAN-direct.
    pub server_url: String,
    /// headscale's listener: the hub's LAN address.
    pub listen: SocketAddr,
    /// headscale binds a STUN socket while DERP is on, though the map turns STUN off.
    pub stun_listen: SocketAddr,
    pub derp: Derp,
    /// Proxies whose X-Forwarded-For headscale trusts (stronghold).
    pub trusted_proxies: Vec<String>,
    /// The hub member's tailnet hostname.
    pub hub_name: String,
    /// The hub member's WireGuard UDP port; 0 picks one.
    pub hub_udp_port: u16,
}

impl Config {
    /// Nodes reach headscale's control and DERP directly at `listen` over plain HTTP.
    pub fn lan_direct(
        dir: PathBuf,
        headscale: PathBuf,
        member: PathBuf,
        listen: SocketAddr,
    ) -> Config {
        let ipv4 = match listen.ip() {
            std::net::IpAddr::V4(a) => a,
            std::net::IpAddr::V6(_) => Ipv4Addr::LOCALHOST,
        };
        Config {
            dir,
            headscale,
            member,
            bwrap: PathBuf::from("bwrap"),
            server_url: format!("http://{listen}"),
            listen,
            stun_listen: SocketAddr::from((Ipv4Addr::LOCALHOST, listen.port().wrapping_add(1))),
            derp: Derp {
                host: ipv4.to_string(),
                ipv4,
                port: listen.port(),
                tls: false,
            },
            trusted_proxies: Vec::new(),
            hub_name: "hub".into(),
            hub_udp_port: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Error {
    /// The node answered with an error frame.
    Node(pt_frame::msg::ErrorReply),
    Failed(String),
}

impl Error {
    pub(crate) fn new(msg: impl Into<String>) -> Error {
        Error::Failed(msg.into())
    }

    pub(crate) fn context(self, ctx: impl fmt::Display) -> Error {
        match self {
            Error::Failed(m) => Error::Failed(format!("{ctx}: {m}")),
            Error::Node(mut r) => {
                r.message = format!("{ctx}: {}", r.message);
                Error::Node(r)
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Node(r) => write!(f, "node refused ({}): {}", r.code, r.message),
            Error::Failed(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

struct Inner {
    cfg: Config,
    admin: Admin,
    paths: headscale::Paths,
    socket: Arc<Mutex<PathBuf>>,
    fingerprint: String,
    headscale: child::Supervisor,
    member: child::Supervisor,
    enroll: Mutex<()>,
}

/// The running fleet layer. Cheap to clone; the children stop when the last clone is dropped.
#[derive(Clone)]
pub struct Fleet {
    inner: Arc<Inner>,
}

const READY: Duration = Duration::from_secs(60);

impl Fleet {
    /// Writes headscale's config, starts it in its sandbox and waits for `/health`, makes the
    /// fleet user, enrolls the hub's own member on first start, then starts that member.
    pub fn start(cfg: Config) -> Result<Fleet> {
        let ctx = |e: Error| e.context("starting the fleet");
        private_dir(&cfg.dir).map_err(ctx)?;
        let paths = headscale::Paths::new(&cfg);
        headscale::write_config(&cfg, &paths).map_err(ctx)?;

        let (c1, p1, c2) = (cfg.clone(), paths.clone(), cfg.clone());
        let hs = child::Supervisor::start(child::Spec {
            name: "headscale",
            command: Box::new(move || headscale::command(&c1, &p1, &["serve"])),
            log: paths.dir.join("headscale.log"),
            health: Box::new(move || headscale::healthy(&c2)),
            ready_within: READY,
            signal_grandchild: true,
        })
        .map_err(ctx)?;
        hs.wait_healthy(READY).map_err(ctx)?;
        let admin = Admin::new(&cfg);
        admin.ensure_user().map_err(ctx)?;
        let fingerprint = headscale::control_fingerprint(&cfg).map_err(ctx)?;
        if !member::enrolled(&cfg) {
            member::enroll(&cfg, &admin, &fingerprint).map_err(ctx)?;
        }

        let socket = Arc::new(Mutex::new(PathBuf::new()));
        let (c3, s3, s4) = (cfg.clone(), socket.clone(), socket.clone());
        let m = child::Supervisor::start(child::Spec {
            name: "member",
            command: Box::new(move || {
                let path = member::fresh_socket(&c3)?;
                *lock(&s3) = path.clone();
                Ok(member::run_command(&c3, &path))
            }),
            log: cfg.dir.join("member.log"),
            health: Box::new(move || {
                let path = lock(&s4).clone();
                member::status(&path).is_ok_and(|st| st.backend_state == "Running")
            }),
            ready_within: READY,
            signal_grandchild: false,
        })
        .map_err(ctx)?;
        m.wait_healthy(READY).map_err(ctx)?;

        let fleet = Fleet {
            inner: Arc::new(Inner {
                cfg,
                admin,
                paths,
                socket,
                fingerprint,
                headscale: hs,
                member: m,
                enroll: Mutex::new(()),
            }),
        };
        fleet.ensure_hub_tag().map_err(ctx)?;
        Ok(fleet)
    }

    /// An enrollment cut short after `auth register` leaves the hub untagged; tag it now.
    fn ensure_hub_tag(&self) -> Result<()> {
        let st = self.status()?;
        let id: u64 = st
            .this
            .as_ref()
            .and_then(|p| p.stable_id.parse().ok())
            .ok_or_else(|| Error::new("hub member STATUS has no self"))?;
        let node = self.admin().node(id)?;
        if !node.has_tag(TAG_HUB) {
            self.admin().tag(id, TAG_HUB)?;
        }
        Ok(())
    }

    pub fn config(&self) -> &Config {
        &self.inner.cfg
    }

    pub fn admin(&self) -> &Admin {
        &self.inner.admin
    }

    /// `sha256:<hex>` of headscale's Noise key, as nodes pin it.
    pub fn control_fingerprint(&self) -> Result<String> {
        let now = headscale::control_fingerprint(&self.inner.cfg)?;
        if now != self.inner.fingerprint {
            return Err(Error::new(format!(
                "headscale's control key changed since start: {} then, {now} now",
                self.inner.fingerprint
            )));
        }
        Ok(now)
    }

    /// The hub member's STATUS: its own state and every peer.
    pub fn status(&self) -> Result<Status> {
        member::status(&self.member_socket())
    }

    /// The member socket of the running member (its name changes when a crash left one behind).
    pub fn member_socket(&self) -> PathBuf {
        lock(&self.inner.socket).clone()
    }

    /// A connection to the agent on `node` (tailnet IP or name) at port 7475, through member.
    pub fn connect(&self, node: &str) -> Result<NodeClient> {
        let target = format!("{node}:{NODE_PORT}");
        let stream = member::dial(&self.member_socket(), &target)?;
        NodeClient::new(&target, stream)
    }

    /// Starts polling every `tag:node` machine; `publish` gets each update.
    pub fn poll(
        &self,
        cfg: PollConfig,
        publish: impl Fn(FleetEvent) + Send + Sync + 'static,
    ) -> Result<Poller> {
        Poller::start(self.clone(), cfg, Arc::new(publish))
            .map_err(|e| Error::new(format!("starting the poller: {e}")))
    }

    /// Stops headscale, copies its database and keys into `<dir>/snapshots/<ms>-<label>/`
    /// and starts it again. Called before a headscale upgrade, since its migrations are one-way.
    pub fn snapshot_headscale(&self, label: &str) -> Result<PathBuf> {
        let i = &self.inner;
        i.headscale.pause();
        let copied = headscale::copy_state(&i.paths, &i.cfg.dir.join("snapshots"), label);
        i.headscale.resume();
        let healthy = i.headscale.wait_healthy(READY);
        let dir = copied.map_err(|e| e.context("snapshotting headscale"))?;
        healthy
            .map_err(|e| e.context(format!("headscale after the snapshot to {}", dir.display())))?;
        Ok(dir)
    }

    /// Stops member, then headscale, and returns once both have exited. They stay stopped even
    /// while other clones of this `Fleet` are alive; dropping the last clone also stops them.
    pub fn shutdown(&self) {
        self.inner.member.stop();
        self.inner.headscale.stop();
    }

    pub fn headscale_state(&self) -> ChildState {
        self.inner.headscale.state()
    }

    pub fn member_state(&self) -> ChildState {
        self.inner.member.state()
    }

    /// Kills headscale or member (`which`) so its supervisor restarts it; for tests and doctor.
    pub fn kill_child(&self, which: &str) -> Result<()> {
        match which {
            "headscale" => self.inner.headscale.kill_child(),
            "member" => self.inner.member.kill_child(),
            other => Err(Error::new(format!("no child named {other}"))),
        }
    }

    pub(crate) fn enroll_lock(&self) -> MutexGuard<'_, ()> {
        lock(&self.inner.enroll)
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| Error::new(format!("creating {}: {e}", dir.display())))?;
    let mode = fs::metadata(dir)
        .map_err(|e| Error::new(format!("{}: {e}", dir.display())))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| Error::new(format!("restricting {}: {e}", dir.display())))?;
    }
    Ok(())
}

/// Writes through a temporary file and a rename, so a reader never sees half a file.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let ctx = |e: std::io::Error| Error::new(format!("writing {}: {e}", path.display()));
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(ctx)?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(ctx)?;
    fs::rename(&tmp, path).map_err(ctx)
}

pub(crate) fn append_log(path: &Path) -> Result<fs::File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| Error::new(format!("opening {}: {e}", path.display())))
}

pub(crate) fn path_str(p: &Path) -> Result<&str> {
    match p.to_str() {
        Some(s) if p.is_absolute() => Ok(s),
        _ => Err(Error::new(format!(
            "{} is not an absolute UTF-8 path",
            p.display()
        ))),
    }
}
