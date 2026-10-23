//! Initial Rust hub service wiring for the existing Connect API.
mod audit;
mod chat;
mod checkpoints;
mod device;
mod diagnostics;
mod directory;
mod distribution;
mod download;
mod envoy;
mod grants;
mod jobs;
mod lease;
mod local_model;
mod mail;
mod node_files;
mod node_update;
mod nodes;
mod quota;
mod release;
mod session_monitor;
mod sleeves;
mod workspaces;
use axum::{
    http::{header, HeaderMap},
    Router,
};
pub use chat::{ChatLauncher, ChatRuntime};
use chrono::{DateTime, SecondsFormat, Utc};
use protectorate_audit::Audit;
use protectorate_auth::{self as auth, Method as ScopeMethod};
use protectorate_backlog::{self as backlog, Filter, NewNote, Note, Patch, Store as BacklogStore};
use protectorate_directory::Directory;
use protectorate_identity::{self as identity, Store as IdentityStore, Token};
use protectorate_inventory::Inventory;
use protectorate_lease::{Chooser as LeaseChooser, LeaseTable};
use protectorate_mailbox::Store as MailStore;
use protectorate_node_monitor::Monitor as NodeMonitor;
use protectorate_quota::{
    self as quotas, QuotaCache, Reader as QuotaReader, Transport as QuotaTransport,
};
use protectorate_release::Source as ReleaseSource;
use protectorate_session::{
    self as session, CreateOptions as SessionOptions, Manager as SessionManager,
    State as SessionState,
};
use protectorate_sleeve_registry::{Record as SleeveRecord, Registry as SleeveRegistry};
use protectorate_wire::{
    self as wire, AuthContext, Catalog, Code, Method, RpcError, RpcRequest, RpcResponse,
    UnaryServer,
};
use serde_json::{json, Value};
pub use sleeves::TerminalLauncher;
mod terminal;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub type NoteEventSink = Arc<dyn Fn(&str, &Note) + Send + Sync>;

struct EventBus {
    sequence: AtomicU64,
    sender: tokio::sync::broadcast::Sender<Value>,
}

impl EventBus {
    fn new() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(64);
        Self {
            sequence: AtomicU64::new(0),
            sender,
        }
    }

    fn publish(&self, kind: &str, subject: &str, payload: Value) {
        let event = json!({
            "id": self.sequence.fetch_add(1, Ordering::Relaxed).wrapping_add(1).to_string(),
            "time": timestamp(Utc::now()),
            "kind": kind,
            "subject": subject,
            "payload": payload,
        });
        let _ = self.sender.send(event);
    }
}

/// The first service slice: hub status and persistent backlog notes. The
/// remaining services can register on the same descriptor-backed server.
pub struct Hub {
    pub version: String,
    pub hostname: String,
    owner_token: Arc<Zeroizing<String>>,
    pub identity: Arc<IdentityStore>,
    pub audit: Arc<Audit>,
    pub backlog: Arc<BacklogStore>,
    pub sessions: Arc<SessionManager>,
    session_monitor: Arc<session_monitor::Monitor>,
    pub directory: Arc<Directory>,
    directory_kick: Arc<tokio::sync::Notify>,
    pub leases: Arc<LeaseTable>,
    pub inventory: Arc<Inventory>,
    pub jobs: Arc<jobs::Jobs>,
    node_admission: Arc<std::sync::RwLock<()>>,
    node_draining: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
    renewing: std::sync::Mutex<std::collections::BTreeSet<String>>,
    pub node_files: Arc<node_files::FileRelay>,
    node_binary_dir: Option<std::path::PathBuf>,
    node_update_lock: Arc<tokio::sync::Mutex<()>>,
    node_update_timeout: std::time::Duration,
    node_update_poll: std::time::Duration,
    pub node_monitor: Arc<NodeMonitor>,
    ca: Option<Arc<protectorate_tls::Ca>>,
    pub mail: Arc<MailStore>,
    msg_ack_timeout: std::time::Duration,
    pub sleeves: Arc<SleeveRegistry>,
    workspaces: workspaces::Config,
    pub terminal_launcher: Option<Arc<sleeves::TerminalLauncher>>,
    pub chat_runtime: Option<Arc<chat::ChatRuntime>>,
    chat_recovery: Vec<SleeveRecord>,
    sleeve_effort: String,
    local_model_hosts: Vec<String>,
    egress_allow: Vec<String>,
    sleeve_launch_lock: std::sync::Mutex<()>,
    resleeving: std::sync::Mutex<std::collections::BTreeSet<String>>,
    envoy_spawn_lock: std::sync::Mutex<()>,
    lease_path: std::path::PathBuf,
    pub quota: Arc<QuotaCache>,
    pub quota_transport: Arc<dyn QuotaTransport>,
    pub releases: Option<Arc<ReleaseSource>>,
    release_restart: Option<Arc<dyn Fn() + Send + Sync>>,
    release_update_lock: Arc<tokio::sync::Mutex<()>>,
    release_last: std::sync::Mutex<Option<(String, String, String)>>,
    pub note_events: Option<NoteEventSink>,
    events: Arc<EventBus>,
    public_device_auth: bool,
    require_device_peer: bool,
    trusted_proxies: Vec<std::net::IpAddr>,
    public_url: Option<String>,
    orchestrator_admin_grants: bool,
    device_rates: Arc<device::Rates>,
    started_at: DateTime<Utc>,
    started: Instant,
}

impl Hub {
    pub fn open(
        home: impl Into<std::path::PathBuf>,
        owner_token: impl Into<String>,
        version: impl Into<String>,
    ) -> protectorate_identity::Result<Self> {
        let home = home.into();
        let workspace_config = workspaces::Config::new(&home);
        let hostname = std::env::var(if cfg!(windows) {
            "COMPUTERNAME"
        } else {
            "HOSTNAME"
        })
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|name| name.trim().to_owned())
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
        let directory =
            Directory::open(home.join("directory.json")).map_err(|error| match error {
                protectorate_directory::Error::Io(error) => identity::Error::Io(error),
                protectorate_directory::Error::Json(error) => identity::Error::Json(error),
                error => identity::Error::Invalid(error.to_string()),
            })?;
        let events = Arc::new(EventBus::new());
        let inventory = Arc::new(
            Inventory::open_with_local(
                home.join("nodes.json"),
                protectorate_inventory::Node {
                    name: hostname.clone(),
                    address: "local".into(),
                    os: platform_os().into(),
                    arch: platform_arch().into(),
                    tier: protectorate_inventory::SHARED.into(),
                    ..protectorate_inventory::Node::default()
                },
            )
            .map_err(|error| identity::Error::Invalid(error.to_string()))?,
        );
        inventory
            .write_hosts()
            .map_err(|error| identity::Error::Invalid(format!("write hosts.md: {error}")))?;
        let node_monitor = Arc::new(NodeMonitor::new(
            Arc::clone(&inventory),
            hostname.clone(),
            home.clone(),
        ));
        let node_admission = Arc::new(std::sync::RwLock::new(()));
        let node_draining = Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
        let jobs = Arc::new(
            jobs::Jobs::open(home.join("jobs"), Arc::clone(&inventory), hostname.clone())
                .map_err(identity::Error::Io)?
                .with_admission(Arc::clone(&node_admission), Arc::clone(&node_draining)),
        );
        let node_files = Arc::new(node_files::FileRelay::new(
            Arc::clone(&inventory),
            hostname.clone(),
        ));
        let mail = Arc::new(
            MailStore::open(home.join("mailbox.jsonl"))
                .map_err(|error| identity::Error::Invalid(error.to_string()))?,
        );
        let sleeves = Arc::new(
            SleeveRegistry::open(home.join("sleeves.json"))
                .map_err(|error| identity::Error::Invalid(error.to_string()))?,
        );
        let chat_recovery = sleeves
            .list()
            .into_iter()
            .filter(|row| {
                row.mode == protectorate_sleeve_registry::Mode::Chat
                    && row.state == protectorate_sleeve_registry::State::Running
            })
            .collect();
        sleeves
            .mark_unmanaged_chat_exited(&timestamp(Utc::now()))
            .map_err(|error| identity::Error::Invalid(error.to_string()))?;
        let identity = Arc::new(IdentityStore::open(home.join("identity.json"))?);
        // A restarted hub cannot reattach its chat driver processes. The
        // registry has marked them exited, so their scoped CLI credentials
        // must stop working before the listener accepts another request.
        let now = Utc::now().timestamp();
        let tokens = identity.list();
        for row in sleeves.list() {
            if row.mode != protectorate_sleeve_registry::Mode::Chat
                || row.state != protectorate_sleeve_registry::State::Exited
            {
                continue;
            }
            let token_name = format!("sleeve:{}", row.name);
            for token in &tokens {
                if token.name == token_name && token.is_active(now) {
                    identity.revoke(&token.id, now)?;
                }
            }
            let token_file = home
                .join("sleeves")
                .join(&row.name)
                .join("home/.protectorate-token");
            match std::fs::remove_file(token_file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(identity::Error::Io(error)),
            }
        }
        let lease_path = home.join("leases.json");
        let leases = Arc::new(
            lease::open_table(
                &lease_path,
                Arc::clone(&events),
                Arc::clone(&mail),
                lease::default_chooser(
                    Arc::clone(&inventory),
                    Arc::clone(&node_monitor),
                    hostname.clone(),
                ),
                Arc::clone(&node_admission),
                Arc::clone(&node_draining),
            )
            .map_err(|error| identity::Error::Invalid(error.to_string()))?,
        );
        let quota = Arc::new(QuotaCache::new(
            quota::default_readers(),
            quotas::QuotaOptions::default(),
        ));
        quota::publish_updates(&quota, Arc::clone(&events));
        let run_sleeves = Arc::clone(&sleeves);
        let audit = Audit::new(home.join("audit"))
            .with_on_error(|error| eprintln!("hub audit write failed: {error}"))
            .with_run_ids(move |entry| {
                let concerns_sleeve = ["SleeveService", "ChatService"].iter().any(|service| {
                    entry
                        .procedure
                        .starts_with(&format!("/protectorate.v1.{service}/"))
                });
                actor_sleeve(&entry.actor)
                    .or_else(|| concerns_sleeve.then_some(entry.subject.as_str()))
                    .map(|name| run_of(&run_sleeves, name))
                    .unwrap_or_default()
            });
        Ok(Self {
            version: version.into(),
            hostname,
            owner_token: Arc::new(Zeroizing::new(owner_token.into())),
            identity,
            audit: Arc::new(audit),
            backlog: Arc::new(BacklogStore::new(home.clone())),
            sessions: Arc::new(SessionManager::new("")),
            session_monitor: Arc::new(session_monitor::Monitor::new(
                std::time::Duration::from_secs(24 * 3600),
            )),
            directory: Arc::new(directory),
            directory_kick: Arc::new(tokio::sync::Notify::new()),
            leases,
            inventory,
            jobs,
            node_admission,
            node_draining,
            renewing: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            node_files,
            node_binary_dir: None,
            node_update_lock: Arc::new(tokio::sync::Mutex::new(())),
            node_update_timeout: std::time::Duration::from_secs(5 * 60),
            node_update_poll: std::time::Duration::from_secs(2),
            node_monitor,
            ca: None,
            mail,
            msg_ack_timeout: std::time::Duration::from_secs(30 * 60),
            sleeves,
            workspaces: workspace_config,
            terminal_launcher: None,
            chat_runtime: None,
            chat_recovery,
            sleeve_effort: "high".into(),
            local_model_hosts: Vec::new(),
            egress_allow: Vec::new(),
            sleeve_launch_lock: std::sync::Mutex::new(()),
            resleeving: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            envoy_spawn_lock: std::sync::Mutex::new(()),
            lease_path,
            quota,
            quota_transport: Arc::new(quotas::UreqTransport::new()),
            releases: None,
            release_restart: None,
            release_update_lock: Arc::new(tokio::sync::Mutex::new(())),
            release_last: std::sync::Mutex::new(None),
            note_events: None,
            events,
            public_device_auth: false,
            require_device_peer: false,
            trusted_proxies: Vec::new(),
            public_url: None,
            orchestrator_admin_grants: false,
            device_rates: Arc::new(device::Rates::default()),
            started_at: Utc::now(),
            started: Instant::now(),
        })
    }

    pub fn with_release_source(mut self, source: Arc<ReleaseSource>) -> Self {
        self.releases = Some(source);
        self
    }

    pub fn with_release_restart(mut self, restart: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.release_restart = Some(restart);
        self
    }

    pub fn with_node_binary_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.node_binary_dir = Some(dir.into());
        self
    }

    pub fn with_node_update_timing(
        mut self,
        timeout: std::time::Duration,
        poll: std::time::Duration,
    ) -> Self {
        self.node_update_timeout = timeout;
        self.node_update_poll = poll;
        self
    }

    pub fn with_identity(mut self, store: Arc<IdentityStore>) -> Self {
        self.identity = store;
        self
    }
    pub fn with_backlog(mut self, store: Arc<BacklogStore>) -> Self {
        self.backlog = store;
        self
    }
    pub fn with_note_events(mut self, events: NoteEventSink) -> Self {
        self.note_events = Some(events);
        self
    }
    pub fn with_sessions(mut self, sessions: Arc<SessionManager>) -> Self {
        self.sessions = sessions;
        self
    }
    pub fn with_dead_retention(mut self, retention: std::time::Duration) -> Self {
        self.session_monitor = Arc::new(session_monitor::Monitor::new(retention));
        self
    }
    pub fn with_terminal_launcher(mut self, launcher: TerminalLauncher) -> Self {
        self.terminal_launcher = Some(Arc::new(launcher));
        self
    }
    /// Set the directories offered by BrowseWorkspaces. Paths are resolved
    /// once, and a browse request is checked against them again when used.
    pub fn with_workspace_roots(mut self, roots: Vec<std::path::PathBuf>) -> std::io::Result<Self> {
        self.workspaces.set_roots(roots)?;
        Ok(self)
    }
    pub fn with_reference_dirs(mut self, refs: Vec<std::path::PathBuf>) -> std::io::Result<Self> {
        self.workspaces.set_references(refs)?;
        Ok(self)
    }
    pub fn with_msg_ack_timeout(mut self, timeout: std::time::Duration) -> Self {
        if !timeout.is_zero() {
            self.msg_ack_timeout = timeout;
        }
        self
    }
    pub fn with_chat_runtime(mut self, runtime: ChatRuntime) -> Self {
        self.chat_runtime = Some(Arc::new(runtime));
        self
    }
    pub fn with_sleeve_effort(mut self, effort: String) -> Self {
        self.sleeve_effort = effort;
        self
    }
    pub fn recover_chat_sleeves(&self) -> Vec<(String, String)> {
        chat::recover_chat_sleeves(self)
    }
    pub fn with_local_model_hosts(mut self, hosts: Vec<String>) -> Self {
        self.local_model_hosts = hosts;
        self
    }
    /// Operator-approved destinations, such as package registries and Git
    /// hosts, that every sleeve may reach besides its provider and the hub.
    pub fn with_egress_allow(mut self, hosts: Vec<String>) -> Self {
        self.egress_allow = hosts;
        self
    }
    pub fn with_directory(mut self, directory: Arc<Directory>) -> Self {
        self.directory = directory;
        self
    }
    /// The CA that issues node certificates when nodes join and renew.
    pub fn with_ca(mut self, ca: Arc<protectorate_tls::Ca>) -> Self {
        self.ca = Some(ca);
        self
    }

    /// Reach nodes over mutual TLS with the hub's certificate. Call after
    /// `with_node_monitor`.
    pub fn with_node_tls(self, tls: protectorate_connect_client::NodeTls) -> Self {
        self.node_monitor.dialer.set(tls.clone());
        self.node_files.dialer.set(tls.clone());
        self.jobs.set_node_tls(tls);
        self
    }
    pub fn with_node_monitor(
        mut self,
        monitor: Arc<NodeMonitor>,
    ) -> protectorate_lease::Result<Self> {
        self.leases = Arc::new(lease::open_table(
            &self.lease_path,
            Arc::clone(&self.events),
            Arc::clone(&self.mail),
            lease::default_chooser(
                Arc::clone(&self.inventory),
                Arc::clone(&monitor),
                self.hostname.clone(),
            ),
            Arc::clone(&self.node_admission),
            Arc::clone(&self.node_draining),
        )?);
        self.node_monitor = monitor;
        Ok(self)
    }
    /// Supply node admission and selector placement when a node monitor is available.
    /// The request is an explicit resource string or a JSON selector object.
    pub fn with_lease_chooser(mut self, chooser: LeaseChooser) -> protectorate_lease::Result<Self> {
        self.leases = Arc::new(lease::open_table(
            &self.lease_path,
            Arc::clone(&self.events),
            Arc::clone(&self.mail),
            chooser,
            Arc::clone(&self.node_admission),
            Arc::clone(&self.node_draining),
        )?);
        Ok(self)
    }
    pub fn with_quota(
        mut self,
        cache: Arc<QuotaCache>,
        transport: Arc<dyn QuotaTransport>,
    ) -> Self {
        quota::publish_updates(&cache, Arc::clone(&self.events));
        self.quota = cache;
        self.quota_transport = transport;
        self
    }
    pub fn with_quota_sources(self, interval: std::time::Duration, local_urls: &[String]) -> Self {
        let mut readers = quota::default_readers();
        readers.extend(
            local_urls
                .iter()
                .map(|url| Arc::new(quotas::Local::new(url)) as Arc<dyn QuotaReader>),
        );
        let cache = Arc::new(QuotaCache::new(
            readers,
            quotas::QuotaOptions {
                interval,
                stale_after: interval.saturating_mul(2),
                ..Default::default()
            },
        ));
        let transport = Arc::clone(&self.quota_transport);
        self.with_quota(cache, transport)
    }
    /// Enables unauthenticated pairing on a listener restricted to loopback.
    pub fn with_loopback_device_auth(mut self) -> Self {
        self.public_device_auth = true;
        self
    }
    /// Enables pairing on a TLS listener that supplies the actual TCP peer.
    pub fn with_tls_device_auth(mut self) -> Self {
        self.public_device_auth = true;
        self.require_device_peer = true;
        self
    }
    pub fn with_trusted_proxies(mut self, proxies: Vec<std::net::IpAddr>) -> Self {
        self.trusted_proxies = proxies.into_iter().map(device::normalize_ip).collect();
        self
    }
    /// Public origin used when a caller does not provide an install URL.
    pub fn with_public_url(mut self, url: String) -> protectorate_wire::Result<Self> {
        self.public_url = Some(nodes::validate_install_base(&url)?);
        Ok(self)
    }
    /// Allows the orchestrator token to mint task grants for one shared node.
    pub fn with_orchestrator_admin_grants(mut self) -> Self {
        self.orchestrator_admin_grants = true;
        self
    }

    pub fn emit(&self, kind: &str, subject: &str, payload: Value) {
        self.events.publish(kind, subject, payload);
    }

    pub fn router(self) -> wire::Result<Router> {
        let catalog = Catalog::embedded()?;
        let identity = Arc::clone(&self.identity);
        let owner = Arc::clone(&self.owner_token);
        let public_device_auth = self.public_device_auth;
        let require_device_peer = self.require_device_peer;
        let trusted_proxies = self.trusted_proxies.clone();
        let denied_trusted_proxies = trusted_proxies.clone();
        let device_rates = Arc::clone(&self.device_rates);
        let denied_audit = Arc::clone(&self.audit);
        let denied_owner = Arc::clone(&owner);
        let denied_identity_store = Arc::clone(&identity);
        let authorize = Arc::new(
            move |method: &Method, headers: &HeaderMap, peer: Option<std::net::SocketAddr>| {
                let pairing_ip = if device::public_procedure(&method.procedure) {
                    peer.map(|address| device::source_ip(address.ip(), headers, &trusted_proxies))
                        .or_else(|| {
                            (!require_device_peer)
                                .then_some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
                        })
                } else {
                    None
                };
                if device::public_procedure(&method.procedure) {
                    if !public_device_auth {
                        Err(RpcError::new(
                            Code::PermissionDenied,
                            "device pairing is unavailable on this listener",
                        ))
                    } else if require_device_peer && peer.is_none() {
                        Err(RpcError::new(
                            Code::PermissionDenied,
                            "device pairing requires a transport peer",
                        ))
                    } else {
                        // A loopback-only listener may omit peer metadata in embedded
                        // routers. TLS listeners must always provide it.
                        let ip = pairing_ip
                            .expect("public pairing has a transport peer or loopback fallback");
                        device_rates
                            .admit(&method.procedure, ip)
                            .map(|_| AuthContext {
                                actor: "unauthenticated".into(),
                                requesting_ip: Some(ip),
                                ..AuthContext::default()
                            })
                    }
                } else {
                    authorize(&owner, &identity, method, headers)
                }
            },
        );
        let completion_audit = Arc::clone(&self.audit);
        let rejection_audit = Arc::clone(&self.audit);
        let observer = Arc::new(
            move |method: &Method,
                  auth: &AuthContext,
                  message: &wire::DynamicMessage,
                  error: Option<&RpcError>| {
                audit::record_completion(&completion_audit, method, auth, message, error);
            },
        );
        // A 4 MiB file chunk expands to about 5.6 MiB in Connect JSON's
        // base64 field. Keep the hub relay's request bound above that while
        // preserving a fixed ceiling for every request.
        let mut server = UnaryServer::new_with_peer(catalog, authorize)
            .with_max_request_bytes(8 << 20)
            .with_max_stream_message_bytes(8 << 20)
            .with_admission_denial_observer(Arc::new(
                move |method, headers, peer, message, error| {
                    let actor = if device::public_procedure(&method.procedure) {
                        AuthContext {
                            actor: "unauthenticated".into(),
                            requesting_ip: peer
                                .map(|address| {
                                    device::source_ip(
                                        address.ip(),
                                        headers,
                                        &denied_trusted_proxies,
                                    )
                                })
                                .or_else(|| {
                                    (!require_device_peer).then_some(std::net::IpAddr::V4(
                                        std::net::Ipv4Addr::LOCALHOST,
                                    ))
                                }),
                            ..AuthContext::default()
                        }
                    } else {
                        denied_identity(&denied_owner, &denied_identity_store, headers)
                    };
                    audit::record_denied(&denied_audit, method, &actor, message, error);
                },
            ))
            .with_completion_observer(observer)
            .with_rejection_observer(Arc::new(move |method, auth, error| {
                audit::record_rejection(&rejection_audit, method, auth, error);
            }));
        let all_methods = Arc::new(server.catalog().methods().cloned().collect::<Vec<_>>());
        let hub = Arc::new(self);
        let weak = Arc::downgrade(&hub);
        hub.node_monitor.set_event_sink(Arc::new(move |event| {
            if let Some(hub) = weak.upgrade() {
                nodes::publish_poll_event(&hub, event);
            }
        }));
        hub.session_monitor.start(Arc::clone(&hub));
        device::start_expiry_sweep(&hub);
        hub.jobs.resume();
        let downloads = Arc::clone(&hub);
        audit::register(&mut server, Arc::clone(&hub))?;
        device::register(&mut server, Arc::clone(&hub))?;
        grants::register(&mut server, Arc::clone(&hub), Arc::clone(&all_methods))?;
        directory::register(&mut server, Arc::clone(&hub))?;
        quota::register(&mut server, Arc::clone(&hub))?;
        release::register(&mut server, Arc::clone(&hub))?;
        lease::register(&mut server, Arc::clone(&hub))?;
        mail::register(&mut server, Arc::clone(&hub))?;
        nodes::register(&mut server, Arc::clone(&hub))?;
        node_files::register(&mut server, Arc::clone(&hub))?;
        node_update::register(&mut server, Arc::clone(&hub))?;
        jobs::register(&mut server, Arc::clone(&hub))?;
        sleeves::register(&mut server, Arc::clone(&hub))?;
        checkpoints::register(&mut server, Arc::clone(&hub))?;
        workspaces::register(&mut server, Arc::clone(&hub))?;
        chat::register(&mut server, Arc::clone(&hub))?;
        let subscribe = Arc::clone(&hub);
        server.register_streaming("/protectorate.v1.EventService/Subscribe", move |request, sender| {
            let hub = Arc::clone(&subscribe);
            async move {
                let mut events = hub.events.sender.subscribe();
                let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
                let watch = (!request.auth.token_id.is_empty())
                    .then(|| hub.identity.watch_token(&request.auth.token_id, now_unix()));
                let mut check_revocation = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    let event = tokio::select! {
                        _ = sender.cancelled() => break,
                        _ = check_revocation.tick() => {
                            if watch.as_ref().is_some_and(|watch| watch.is_cancelled()) {
                                return Err(RpcError::new(Code::Unauthenticated, "token revoked"));
                            }
                            continue;
                        },
                        _ = heartbeat.tick() => json!({
                            "id": hub.events.sequence.fetch_add(1, Ordering::Relaxed).wrapping_add(1).to_string(),
                            "time": timestamp(Utc::now()),
                            "kind": "heartbeat", "subject": "hub",
                            "payload": {"uptime_seconds": hub.started.elapsed().as_secs_f64()},
                        }),
                        next = events.recv() => match next {
                            Ok(event) => event,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        },
                    };
                    let bytes = serde_json::to_vec(&event)
                        .map_err(|_| RpcError::new(Code::Internal, "failed to encode event"))?;
                    let message = wire::decode_json(request.method.output(), &bytes)?;
                    let send = sender.send(message);
                    tokio::pin!(send);
                    loop {
                        tokio::select! {
                            result = &mut send => {
                                result?;
                                break;
                            },
                            _ = check_revocation.tick() => {
                                if watch.as_ref().is_some_and(|watch| watch.is_cancelled()) {
                                    return Err(RpcError::new(Code::Unauthenticated, "token revoked"));
                                }
                            },
                        }
                    }
                }
                Ok(Default::default())
            }
        })?;
        let status = Arc::clone(&hub);
        server.register("/protectorate.v1.HubService/GetStatus", move |request| {
            let hub = Arc::clone(&status);
            async move {
                let available = hub
                    .releases
                    .as_ref()
                    .map(|source| source.available(source.channel()))
                    .unwrap_or_default();
                let channel = hub
                    .releases
                    .as_ref()
                    .map(|source| source.channel())
                    .unwrap_or("");
                respond(
                    &request,
                    json!({
                        "version": hub.version,
                        "releaseConfirmed": hub.releases.as_ref().is_none_or(|source| {
                            !source.dir().join(".hub-trial.json").exists()
                                && source.current().version == hub.version
                        }),
                        "hostname": hub.hostname,
                        "os": platform_os(),
                        "arch": platform_arch(),
                        "uptimeSeconds": hub.started.elapsed().as_secs().to_string(),
                        "startedAt": timestamp(hub.started_at),
                        "availableVersion": available.version,
                        "updateChannel": channel,
                        "releaseNotesUrl": available.notes_url,
                        "auditHead": audit::head_json(hub.audit.head().ok().flatten().unwrap_or_default()),
                    }),
                )
            }
        })?;
        let runtime_status = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.HubService/GetSleeveRuntime",
            move |request| {
                let hub = Arc::clone(&runtime_status);
                async move {
                    let configured = hub.terminal_launcher.is_some() || hub.chat_runtime.is_some();
                    let helper = hub
                        .terminal_launcher
                        .as_ref()
                        .map(|launcher| launcher.helper.as_path())
                        .or_else(|| {
                            hub.chat_runtime
                                .as_ref()
                                .map(|runtime| runtime.helper_path())
                        })
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let overlays = configured
                        && cfg!(target_os = "linux")
                        && std::fs::read_to_string("/proc/filesystems").is_ok_and(|filesystems| {
                            filesystems
                                .lines()
                                .any(|line| line.split_whitespace().last() == Some("overlay"))
                        });
                    respond(
                        &request,
                        json!({
                            "runtime": if configured { "rust-namespaces" } else { "" },
                            "helperPath": helper,
                            "overlays": overlays,
                        }),
                    )
                }
            },
        )?;
        let create_token = Arc::clone(&hub);
        let create_methods = Arc::clone(&all_methods);
        server.register("/protectorate.v1.TokenService/CreateToken", move |request| {
            let hub = Arc::clone(&create_token);
            let methods = Arc::clone(&create_methods);
            async move {
                let input = input(&request)?;
                let scopes = strings(&input, "scopes");
                let ttl = parse_duration(input.get("ttl"))?;
                let methods = scope_methods(&methods);
                let issued = hub.identity.create(
                    &string(&input, "name"),
                    &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
                    ttl,
                    now_unix(),
                    &methods,
                ).map_err(token_create_error)?;
                respond(&request, json!({"token": token_json(&issued.token), "secret": issued.secret.expose()}))
            }
        })?;
        let list_tokens = Arc::clone(&hub);
        server.register("/protectorate.v1.TokenService/ListTokens", move |request| {
            let hub = Arc::clone(&list_tokens);
            async move {
                respond(&request, json!({"tokens": hub.identity.list().iter().map(token_json).collect::<Vec<_>>() }))
            }
        })?;
        let revoke_token = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.TokenService/RevokeToken",
            move |request| {
                let hub = Arc::clone(&revoke_token);
                async move {
                    let token = hub
                        .identity
                        .revoke(&string(&input(&request)?, "id"), now_unix())
                        .map_err(token_error)?;
                    respond(&request, json!({"token": token_json(&token)}))
                }
            },
        )?;
        let update_token = Arc::clone(&hub);
        let update_methods = Arc::clone(&all_methods);
        server.register(
            "/protectorate.v1.TokenService/UpdateTokenScopes",
            move |request| {
                let hub = Arc::clone(&update_token);
                let all_methods = Arc::clone(&update_methods);
                async move {
                    let input = input(&request)?;
                    let add = strings(&input, "add");
                    let remove = strings(&input, "remove");
                    let methods = scope_methods(&all_methods);
                    let (token, before) = hub
                        .identity
                        .update_scopes(
                            &string(&input, "id"),
                            &add.iter().map(String::as_str).collect::<Vec<_>>(),
                            &remove.iter().map(String::as_str).collect::<Vec<_>>(),
                            now_unix(),
                            &methods,
                        )
                        .map_err(token_error)?;
                    let response = respond(&request, json!({"token": token_json(&token)}))?;
                    hub.audit.write(protectorate_audit::Entry {
                        actor: request.auth.actor.clone(),
                        token_id: request.auth.token_id.clone(),
                        task: request.auth.task.clone(),
                        procedure: request.method.procedure.clone(),
                        subject: token.id.clone(),
                        outcome: "ok".into(),
                        detail: format!("scopes {before:?} -> {:?}", token.scopes),
                        ..protectorate_audit::Entry::default()
                    });
                    Ok(response)
                }
            },
        )?;
        let list = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.BacklogService/ListNotes",
            move |request| {
                let hub = Arc::clone(&list);
                async move {
                    let input = input(&request)?;
                    let notes = hub
                        .backlog
                        .list(Filter {
                            project: string(&input, "project"),
                            query: string(&input, "query"),
                            state: string(&input, "state"),
                        })
                        .map_err(backlog_error)?;
                    respond(
                        &request,
                        json!({"notes": notes.iter().map(note_json).collect::<Vec<_>>() }),
                    )
                }
            },
        )?;
        let get = Arc::clone(&hub);
        server.register("/protectorate.v1.BacklogService/GetNote", move |request| {
            let hub = Arc::clone(&get);
            async move {
                let input = input(&request)?;
                let note = hub
                    .backlog
                    .get(&string(&input, "id"))
                    .map_err(backlog_error)?;
                respond(&request, json!({"note": note_json(&note)}))
            }
        })?;
        let create = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.BacklogService/CreateNote",
            move |request| {
                let hub = Arc::clone(&create);
                async move {
                    let input = input(&request)?;
                    let note = hub
                        .backlog
                        .create(NewNote {
                            title: string(&input, "title"),
                            body: string(&input, "body"),
                            pinned: boolean(&input, "pinned"),
                            project: string(&input, "project"),
                            source: sender(&request.auth.actor),
                        })
                        .map_err(backlog_error)?;
                    if let Some(events) = &hub.note_events {
                        events("note.created", &note);
                    }
                    hub.emit("note.created", &note.id, note_event_payload(&note));
                    respond(&request, json!({"note": note_json(&note)}))
                }
            },
        )?;
        let update = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.BacklogService/UpdateNote",
            move |request| {
                let hub = Arc::clone(&update);
                async move {
                    let input = input(&request)?;
                    let (note, changed) = hub
                        .backlog
                        .update(
                            &string(&input, "id"),
                            Patch {
                                title: optional_string(&input, "title"),
                                body: optional_string(&input, "body"),
                                pinned: input.get("pinned").and_then(Value::as_bool),
                                project: optional_string(&input, "project"),
                                state: optional_string(&input, "state"),
                                resolution: optional_string(&input, "resolution"),
                                done_by: request.auth.actor.clone(),
                            },
                        )
                        .map_err(backlog_error)?;
                    if changed {
                        if let Some(events) = &hub.note_events {
                            events("note.updated", &note);
                        }
                        hub.emit("note.updated", &note.id, note_event_payload(&note));
                    }
                    respond(&request, json!({"note": note_json(&note)}))
                }
            },
        )?;
        let delete = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.BacklogService/DeleteNote",
            move |request| {
                let hub = Arc::clone(&delete);
                async move {
                    let input = input(&request)?;
                    let note = hub
                        .backlog
                        .delete(&string(&input, "id"))
                        .map_err(backlog_error)?;
                    if let Some(events) = &hub.note_events {
                        events("note.deleted", &note);
                    }
                    hub.emit("note.deleted", &note.id, note_event_payload(&note));
                    respond(&request, json!({}))
                }
            },
        )?;
        let list_sessions = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.SessionService/ListSessions",
            move |request| {
                let manager = Arc::clone(&list_sessions.sessions);
                async move {
                    let sessions = tokio::task::spawn_blocking(move || manager.list())
                        .await
                        .map_err(join_error)?
                        .map_err(session_error)?;
                    let own_session = sleeve_session(&request.auth.actor);
                    respond(
                        &request,
                        json!({"sessions": sessions.iter().filter(|s| own_session.as_ref().is_none_or(|name| s.name == *name)).map(session_json).collect::<Vec<_>>() }),
                    )
                }
            },
        )?;
        let create_session = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.SessionService/CreateSession",
            move |request| {
                let hub = Arc::clone(&create_session);
                async move {
                    let input = input(&request)?;
                    let name = string(&input, "name");
                    if name.starts_with("sleeve-") {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "sleeve names are reserved",
                        ));
                    }
                    let options = SessionOptions {
                        name,
                        cwd: optional_string(&input, "cwd")
                            .filter(|s| !s.is_empty())
                            .map(Into::into),
                        command: input
                            .get("command")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        env: input
                            .get("env")
                            .and_then(Value::as_object)
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k, v)| {
                                        v.as_str().map(|v| (k.clone(), v.to_owned()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    };
                    let session = tokio::task::spawn_blocking(move || {
                        hub.session_monitor.scan(&hub);
                        let result = hub.sessions.create(&options);
                        if result.is_ok() {
                            hub.session_monitor.scan(&hub);
                        }
                        result
                    })
                    .await
                    .map_err(join_error)?
                    .map_err(session_error)?;
                    respond(&request, json!({"session": session_json(&session)}))
                }
            },
        )?;
        let kill_session = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.SessionService/KillSession",
            move |request| {
                let hub = Arc::clone(&kill_session);
                async move {
                    let name = string(&input(&request)?, "name");
                    tokio::task::spawn_blocking(move || {
                        hub.session_monitor.scan(&hub);
                        let result = hub.sessions.kill(&name);
                        if result.is_ok() {
                            hub.session_monitor.scan(&hub);
                        }
                        result
                    })
                    .await
                    .map_err(join_error)?
                    .map_err(session_error)?;
                    respond(&request, json!({}))
                }
            },
        )?;
        let respawn_session = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.SessionService/RespawnSession",
            move |request| {
                let hub = Arc::clone(&respawn_session);
                async move {
                    let name = string(&input(&request)?, "name");
                    if name.starts_with("sleeve-") {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "sleeve sessions cannot be respawned here",
                        ));
                    }
                    let session = tokio::task::spawn_blocking(move || {
                        hub.session_monitor.scan(&hub);
                        let result = hub.sessions.respawn(&name);
                        if result.is_ok() {
                            hub.session_monitor.scan(&hub);
                        }
                        result
                    })
                    .await
                    .map_err(join_error)?
                    .map_err(session_error)?;
                    respond(&request, json!({"session": session_json(&session)}))
                }
            },
        )?;
        let terminal_hub = Arc::clone(&hub);
        let installer_ca = Arc::clone(&hub);
        let installer_sh = Arc::clone(&hub);
        let installer_termux = Arc::clone(&hub);
        let installer_ps1 = Arc::clone(&hub);
        let debug_metrics = Arc::clone(&hub);
        let debug_profile = Arc::clone(&hub);
        let debug_snapshot = Arc::clone(&hub);
        let debug_unknown = Arc::clone(&hub);
        let debug_index = Arc::clone(&hub);
        let debug_node = Arc::clone(&hub);
        server.register(
            "/protectorate.v1.SessionService/CapturePane",
            move |request| {
                let manager = Arc::clone(&hub.sessions);
                async move {
                    let input = input(&request)?;
                    let name = string(&input, "name");
                    if sleeve_session(&request.auth.actor).is_some_and(|own| own != name) {
                        return Err(RpcError::new(
                            Code::PermissionDenied,
                            "sleeve may only capture its own session",
                        ));
                    }
                    let lines = input
                        .get("lines")
                        .and_then(Value::as_i64)
                        .unwrap_or_default()
                        .max(0) as usize;
                    let text = tokio::task::spawn_blocking(move || manager.capture(&name, lines))
                        .await
                        .map_err(join_error)?
                        .map_err(session_error)?;
                    respond(&request, json!({"text": text}))
                }
            },
        )?;
        Ok(server
            .router()
            .route(
                "/dl/{file}",
                axum::routing::get(
                    move |axum::extract::Path(file): axum::extract::Path<String>| {
                        let hub = Arc::clone(&downloads);
                        async move { download::serve(hub, file).await }
                    },
                ),
            )
            .route(
                "/ca.pem",
                axum::routing::get(move || distribution::ca(Arc::clone(&installer_ca))),
            )
            .route(
                "/install.sh",
                axum::routing::get(move || distribution::linux(Arc::clone(&installer_sh))),
            )
            .route(
                "/install-termux.sh",
                axum::routing::get(move || distribution::termux(Arc::clone(&installer_termux))),
            )
            .route(
                "/install.ps1",
                axum::routing::get(move || distribution::windows(Arc::clone(&installer_ps1))),
            )
            .route(
                "/debug/metrics",
                axum::routing::get(move |headers| {
                    diagnostics::metrics(Arc::clone(&debug_metrics), headers)
                }),
            )
            .route(
                "/debug/pprof/",
                axum::routing::get(move |headers| {
                    diagnostics::index(Arc::clone(&debug_index), headers)
                }),
            )
            .route(
                "/debug/nodes/{name}/{kind}",
                axum::routing::get(move |headers, path| {
                    diagnostics::node(Arc::clone(&debug_node), headers, path)
                }),
            )
            .route(
                "/debug/pprof/profile",
                axum::routing::get(move |headers, query| {
                    diagnostics::profile(Arc::clone(&debug_profile), headers, query)
                }),
            )
            .route(
                "/debug/pprof/{kind}",
                axum::routing::get(move |headers, path| {
                    diagnostics::snapshot(Arc::clone(&debug_snapshot), headers, path)
                }),
            )
            .route(
                "/debug/{*path}",
                axum::routing::get(move |headers| {
                    diagnostics::unknown(Arc::clone(&debug_unknown), headers)
                }),
            )
            .route(
                "/ws/terminal/{name}",
                axum::routing::get(move |path, headers, uri, upgrade| {
                    terminal::attach(Arc::clone(&terminal_hub), path, headers, uri, upgrade)
                }),
            ))
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let text = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let prefix = text.get(..7)?;
    if prefix.eq_ignore_ascii_case("Bearer ") && text.len() > 7 {
        Some(&text[7..])
    } else {
        None
    }
}
fn denied_identity(
    owner: &Zeroizing<String>,
    store: &IdentityStore,
    headers: &HeaderMap,
) -> AuthContext {
    let Some(secret) = bearer(headers) else {
        return AuthContext {
            actor: "unauthenticated".into(),
            ..AuthContext::default()
        };
    };
    if !owner.is_empty() && owner.as_bytes().ct_eq(secret.as_bytes()).into() {
        return AuthContext {
            actor: "owner".into(),
            owner: true,
            ..AuthContext::default()
        };
    }
    match store.lookup(secret, now_unix()) {
        Ok(token) => AuthContext {
            actor: format!("token:{}", token.name),
            token_id: token.id,
            task: token.task,
            ..AuthContext::default()
        },
        Err(_) => AuthContext {
            actor: "unauthenticated".into(),
            ..AuthContext::default()
        },
    }
}
fn authorize(
    owner: &Zeroizing<String>,
    store: &IdentityStore,
    method: &Method,
    headers: &HeaderMap,
) -> wire::Result<AuthContext> {
    let secret = bearer(headers)
        .ok_or_else(|| RpcError::new(Code::Unauthenticated, "missing or invalid bearer token"))?;
    if !owner.is_empty() && owner.as_bytes().ct_eq(secret.as_bytes()).into() {
        return Ok(AuthContext {
            actor: "owner".into(),
            owner: true,
            ..AuthContext::default()
        });
    }
    let token = store
        .lookup(secret, now_unix())
        .map_err(|_| RpcError::new(Code::Unauthenticated, "missing or invalid bearer token"))?;
    let allowed = if method.owner_only {
        method.device_allowed && token.device
    } else if method.service == "DeviceService" {
        token.device
    } else if token.device
        || (method.tool == "sleeve_kill_sleeve" && can_kill_owned_sleeve(&token.scopes))
        || (method.tool == "chat_report" && token.name.starts_with("sleeve:"))
    {
        true
    } else {
        let scope_method = ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        };
        auth::scopes_allow(
            &token.scopes.iter().map(String::as_str).collect::<Vec<_>>(),
            &scope_method,
        )
    };
    if !allowed {
        return Err(RpcError::new(
            Code::PermissionDenied,
            format!(
                "token {:?} lacks the required scope: {}",
                token.name, method.tool
            ),
        ));
    }
    Ok(AuthContext {
        actor: format!("token:{}", token.name),
        token_id: token.id,
        task: token.task,
        scopes: token.scopes,
        device: token.device,
        ..AuthContext::default()
    })
}

fn can_kill_owned_sleeve(scopes: &[String]) -> bool {
    auth::scopes_allow(
        &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
        &ScopeMethod {
            tool: "sleeve_spawn_sleeve",
            service: "SleeveService",
            read_only: false,
            owner_only: false,
        },
    )
}

fn input(request: &RpcRequest) -> wire::Result<Value> {
    serde_json::to_value(&request.message)
        .map_err(|_| RpcError::new(Code::Internal, "failed to read request"))
}
fn respond(request: &RpcRequest, value: Value) -> wire::Result<RpcResponse> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| RpcError::new(Code::Internal, "failed to encode response"))?;
    Ok(wire::decode_json(request.method.output(), &bytes)?.into())
}
fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn parse_duration(value: Option<&Value>) -> wire::Result<std::time::Duration> {
    let Some(value) = value else {
        return Ok(std::time::Duration::ZERO);
    };
    let text = value
        .as_str()
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "invalid ttl"))?;
    let number = text
        .strip_suffix('s')
        .ok_or_else(|| RpcError::new(Code::InvalidArgument, "invalid ttl"))?;
    let (seconds, fraction) = number.split_once('.').unwrap_or((number, ""));
    if seconds.is_empty()
        || !seconds.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || number.matches('.').count() > 1
    {
        return Err(RpcError::new(Code::InvalidArgument, "invalid ttl"));
    }
    let seconds = seconds
        .parse()
        .map_err(|_| RpcError::new(Code::InvalidArgument, "invalid ttl"))?;
    let nanos = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<9}")
            .parse()
            .map_err(|_| RpcError::new(Code::InvalidArgument, "invalid ttl"))?
    };
    Ok(std::time::Duration::new(seconds, nanos))
}
fn scope_methods(methods: &[Method]) -> Vec<ScopeMethod<'_>> {
    methods
        .iter()
        .map(|method| ScopeMethod {
            tool: &method.tool,
            service: &method.service,
            read_only: method.read_only,
            owner_only: method.owner_only,
        })
        .collect()
}
fn token_json(token: &Token) -> Value {
    let mut out = json!({
        "id": token.id, "name": token.name, "scopes": token.scopes,
        "createdAt": unix_timestamp(token.created),
        "task": token.task, "mintedBy": token.minted_by,
        "platform": token.platform, "approvedBy": token.approved_by,
    });
    for (key, value) in [
        ("expiresAt", token.expires),
        ("lastUsedAt", token.last_used),
        ("revokedAt", token.revoked_at),
        ("approvedAt", token.approved_at),
    ] {
        if let Some(value) = value {
            out[key] = json!(unix_timestamp(value));
        }
    }
    out
}
fn unix_timestamp(seconds: i64) -> String {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(timestamp)
        .unwrap_or_default()
}
fn token_error(error: identity::Error) -> RpcError {
    let code = match error {
        identity::Error::UnknownToken => Code::NotFound,
        identity::Error::Invalid(_)
        | identity::Error::Scopes(_)
        | identity::Error::Ambiguous(_)
        | identity::Error::Expired
        | identity::Error::Revoked => Code::InvalidArgument,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}
fn token_create_error(error: identity::Error) -> RpcError {
    token_error(error)
}
fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn boolean(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn sender(actor: &str) -> String {
    actor_sleeve(actor).unwrap_or(actor).into()
}
/// The sleeve an actor authenticates as: a sleeve token or Envoy's.
fn actor_sleeve(actor: &str) -> Option<&str> {
    if actor == "token:orchestrator-envoy" {
        Some("envoy")
    } else {
        actor.strip_prefix("token:sleeve:")
    }
}
fn run_of(sleeves: &SleeveRegistry, name: &str) -> String {
    sleeves.get(name).map(|row| row.run_id).unwrap_or_default()
}
/// The run of the sleeve an actor authenticates as, empty for anyone else.
fn actor_run(hub: &Hub, actor: &str) -> String {
    actor_sleeve(actor)
        .map(|name| run_of(&hub.sleeves, name))
        .unwrap_or_default()
}
fn sleeve_session(actor: &str) -> Option<String> {
    actor
        .strip_prefix("token:sleeve:")
        .map(|name| format!("sleeve-{name}"))
}
fn platform_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}
fn platform_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    }
}
fn timestamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Nanos, true)
}
fn note_json(note: &Note) -> Value {
    let mut out = json!({
        "id": note.id, "title": note.title, "body": note.body,
        "createdAt": timestamp(note.created_at), "updatedAt": timestamp(note.updated_at),
        "pinned": note.pinned, "project": note.project, "source": note.source, "state": note.state,
    });
    if let Some(done_at) = note.done_at {
        out["completion"] = json!({"doneAt": timestamp(done_at), "doneBy": note.done_by, "resolution": note.resolution});
    }
    out
}
fn note_event_payload(note: &Note) -> Value {
    json!({
        "id": note.id,
        "title": note.title,
        "project": note.project,
        "source": note.source,
        "pinned": note.pinned,
        "state": note.state,
    })
}
fn backlog_error(error: backlog::Error) -> RpcError {
    let code = match error {
        backlog::Error::Invalid(_) => Code::InvalidArgument,
        backlog::Error::NotFound(_) => Code::NotFound,
        _ => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn session_json(session: &session::Session) -> Value {
    let mut out = json!({
        "name": session.name,
        "cwd": session.cwd.to_string_lossy(),
        "command": session.command,
        "createdAt": timestamp(DateTime::<Utc>::from(session.created)),
        "attachedClients": session.attached_clients,
        "lastActivity": timestamp(DateTime::<Utc>::from(session.last_activity)),
        "paneCols": session.cols,
        "paneRows": session.rows,
        "state": match session.state {
            SessionState::Running => "SESSION_STATE_RUNNING",
            SessionState::Exited => "SESSION_STATE_EXITED",
        },
        "exitCode": session.exit_code,
        "exitSignal": session.exit_signal,
    });
    if let Some(exited) = session.exited {
        out["exitedAt"] = json!(timestamp(DateTime::<Utc>::from(exited)));
    }
    out
}

fn session_error(error: session::Error) -> RpcError {
    let code = match error {
        session::Error::InvalidName | session::Error::InvalidResponse(_) => Code::InvalidArgument,
        session::Error::NotFound => Code::NotFound,
        session::Error::Exists => Code::AlreadyExists,
        session::Error::NotExited => Code::FailedPrecondition,
        session::Error::NoServer => Code::Unavailable,
        session::Error::OutputTooLarge => Code::ResourceExhausted,
        session::Error::Tmux(_) | session::Error::Io(_) => Code::Internal,
    };
    RpcError::new(code, error.to_string())
}

fn no_local_agent() -> RpcError {
    RpcError::new(
        Code::FailedPrecondition,
        "the hub's own machine runs no node agent; invite and join it as a node to run work there",
    )
}

fn join_error(error: tokio::task::JoinError) -> RpcError {
    RpcError::new(Code::Internal, error.to_string())
}
