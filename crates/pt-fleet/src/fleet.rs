use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::child::{self, ChildState};
use crate::client::NodeClient;
use crate::config::{Config, NODE_PORT};
use crate::error::{Error, Result};
use crate::files::private_dir;
use crate::headscale::{self, Admin, TAG_HUB};
use crate::member::{self, Status};

struct Inner {
    cfg: Config,
    admin: Admin,
    paths: headscale::Paths,
    socket: Arc<Mutex<PathBuf>>,
    fingerprint: String,
    headscale: child::Supervisor,
    member: child::Supervisor,
    enroll: Mutex<()>,
    /// Held from a headscale node list to the poller's update from it, and across a rename or
    /// removal, so a stale list never undoes either.
    nodes: Mutex<()>,
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
                nodes: Mutex::new(()),
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

    /// Stops headscale, copies its database and keys into `<dir>/snapshots/<ms>-<label>/`
    /// and starts it again. Called before a headscale upgrade, since its migrations are one-way.
    pub fn snapshot_headscale(&self, label: &str) -> Result<PathBuf> {
        let i = &self.inner;
        i.headscale.pause();
        let copied = headscale::copy_state(&i.paths, &i.cfg.dir.join("snapshots"), label);
        i.headscale.resume();
        let healthy = i.headscale.wait_healthy(READY);
        let dir = copied.map_err(|e| e.context("snapshotting headscale"))?;
        if let Err(e) = healthy {
            let _ = fs::remove_dir_all(&dir);
            return Err(e.context(format!("headscale after the snapshot to {}", dir.display())));
        }
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

    pub(crate) fn nodes_lock(&self) -> MutexGuard<'_, ()> {
        lock(&self.inner.nodes)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
