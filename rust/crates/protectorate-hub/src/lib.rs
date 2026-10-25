//! The hub: its Connect services over the state under PROTECTORATE_HOME.
mod access;
mod approvals;
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
use access::scope_methods;
use axum::{http::HeaderMap, Router};
pub use chat::{ChatLauncher, ChatRuntime};
use chrono::{DateTime, SecondsFormat, Utc};
use prost_reflect::prost_types;
use protectorate_audit::Audit;
use protectorate_auth::{self as auth, Method as ScopeMethod};
use protectorate_backlog::{self as backlog, Filter, NewNote, Note, Patch, Store as BacklogStore};
use protectorate_directory::Directory;
use protectorate_identity::{self as identity, Store as IdentityStore};
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
    self as wire,
    v1::{self, backlog_service, event_service, hub_service, session_service},
    AuthContext, Call, Catalog, Code, Method, RpcError, UnaryServer,
};
use serde_json::{json, Value};
pub use sleeves::TerminalLauncher;
mod terminal;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use zeroize::Zeroizing;

/// Locks a std mutex and keeps serving after a panic poisoned it. Every
/// hub mutex guards either `()`, a set or map changed by one call, or a store
/// that builds its next state outside the lock and assigns it in one step, so
/// a panic in a critical section cannot leave a half-applied update behind.
pub(crate) fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct EventBus {
    sequence: AtomicU64,
    sender: tokio::sync::broadcast::Sender<v1::Event>,
}

impl EventBus {
    fn new() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(64);
        Self {
            sequence: AtomicU64::new(0),
            sender,
        }
    }

    fn event(&self, kind: &str, subject: &str, payload: Value) -> v1::Event {
        v1::Event {
            id: self
                .sequence
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1)
                .to_string(),
            time: Some(proto_time(Utc::now())),
            kind: kind.into(),
            subject: subject.into(),
            payload: proto_struct(payload),
        }
    }

    fn publish(&self, kind: &str, subject: &str, payload: Value) {
        if self.sender.receiver_count() == 0 {
            return;
        }
        let _ = self.sender.send(self.event(kind, subject, payload));
    }
}

/// An event payload as google.protobuf.Struct; anything but an object is
/// no payload.
fn proto_struct(payload: Value) -> Option<prost_types::Struct> {
    let Value::Object(fields) = payload else {
        return None;
    };
    Some(prost_types::Struct {
        fields: fields
            .into_iter()
            .map(|(key, value)| (key, proto_value(value)))
            .collect(),
    })
}

fn proto_value(value: Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match value {
        Value::Null => Kind::NullValue(prost_types::NullValue::NullValue as i32),
        Value::Bool(value) => Kind::BoolValue(value),
        Value::Number(value) => Kind::NumberValue(value.as_f64().unwrap_or_default()),
        Value::String(value) => Kind::StringValue(value),
        Value::Array(values) => Kind::ListValue(prost_types::ListValue {
            values: values.into_iter().map(proto_value).collect(),
        }),
        object @ Value::Object(_) => Kind::StructValue(proto_struct(object).unwrap_or_default()),
    };
    prost_types::Value { kind: Some(kind) }
}

pub struct Hub {
    pub version: String,
    pub hostname: String,
    owner_token: Arc<Zeroizing<String>>,
    pub identity: Arc<IdentityStore>,
    methods: Arc<Vec<Method>>,
    capability_ttl: std::time::Duration,
    disk_interval: std::time::Duration,
    pub audit: Arc<Audit>,
    approvals: approvals::Approvals,
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
        let version = version.into();
        // Temp files of writers killed before their rename; best effort.
        for dir in [home.clone(), home.join("jobs"), home.join("ca")] {
            let _ = protectorate_fsutil::remove_stale_temps(&dir);
        }
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
        // hosts.md is derived; a full disk must not keep the hub from serving.
        if let Err(error) = inventory.write_hosts() {
            eprintln!("write hosts.md: {error}");
        }
        let node_monitor = Arc::new(NodeMonitor::new(
            Arc::clone(&inventory),
            hostname.clone(),
            home.clone(),
            version.clone(),
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
            .mark_unmanaged_chat_exited(&now())
            .map_err(|error| identity::Error::Invalid(error.to_string()))?;
        let identity = Arc::new(IdentityStore::open(home.join("identity.json"))?);
        // A restarted hub cannot reattach its chat driver processes. The
        // registry has marked them exited, so their capabilities must stop
        // working before the listener accepts another request.
        let now = Utc::now().timestamp();
        for row in sleeves.list() {
            if row.mode != protectorate_sleeve_registry::Mode::Chat
                || row.state != protectorate_sleeve_registry::State::Exited
            {
                continue;
            }
            identity.revoke_principal(&format!("sleeve:{}", row.name), now)?;
            match std::fs::remove_file(
                home.join("sleeves")
                    .join(&row.name)
                    .join("home/.protectorate-token"),
            ) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(identity::Error::Io(error)),
            }
        }
        let methods = Arc::new(
            Catalog::embedded()
                .map_err(|error| identity::Error::Invalid(error.message))?
                .methods()
                .cloned()
                .collect::<Vec<_>>(),
        );
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
            version,
            hostname,
            owner_token: Arc::new(Zeroizing::new(owner_token.into())),
            identity,
            methods,
            capability_ttl: std::time::Duration::from_secs(60 * 60),
            disk_interval: std::time::Duration::from_secs(10),
            audit: Arc::new(audit),
            approvals: approvals::Approvals::new(approvals::DEFAULT_TTL),
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

    /// The lifetime of the capabilities the hub holds for sleeves and Envoy,
    /// which it refreshes at half that age.
    pub fn with_capability_ttl(mut self, ttl: std::time::Duration) -> Self {
        self.capability_ttl = ttl.clamp(std::time::Duration::from_secs(2), identity::MAX_TTL);
        self
    }
    /// How often a device waiting for approval may poll, which the hub tells
    /// it and raises when it polls faster.
    pub fn with_device_interval(self, interval: std::time::Duration) -> Self {
        self.identity.set_device_interval(interval);
        self
    }

    /// How often running sleeves with a disk limit are measured and held to it.
    pub fn with_disk_interval(mut self, interval: std::time::Duration) -> Self {
        self.disk_interval = interval;
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
    /// How long an approval request waits for the owner.
    pub fn with_approval_ttl(mut self, ttl: std::time::Duration) -> Self {
        self.approvals = approvals::Approvals::new(ttl);
        self
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
        let methods = Arc::clone(&self.methods);
        let public_device_auth = self.public_device_auth;
        let require_device_peer = self.require_device_peer;
        let trusted_proxies = self.trusted_proxies.clone();
        let device_rates = Arc::clone(&self.device_rates);
        let node_inventory = Arc::clone(&self.inventory);
        let authorize = Arc::new(
            move |method: &Method, headers: &HeaderMap, peer: Option<wire::Peer>| {
                if method.node_only {
                    return nodes::authenticate_node(&node_inventory, peer.as_ref());
                }
                if !device::public_procedure(&method.procedure) {
                    let caller = access::authenticate(
                        &owner,
                        &identity,
                        &methods,
                        headers,
                        "POST",
                        &method.procedure,
                        peer.as_ref().is_some_and(|peer| peer.tls),
                    )
                    .map_err(|error| {
                        Box::new(wire::Denied {
                            error,
                            auth: AuthContext {
                                actor: "unauthenticated".into(),
                                ..AuthContext::default()
                            },
                        })
                    })?;
                    return access::authorize(&caller, method);
                }
                // A loopback-only listener may omit peer metadata in embedded
                // routers. TLS listeners must always provide it.
                let ip = peer
                    .as_ref()
                    .map(|peer| device::source_ip(peer.addr.ip(), headers, &trusted_proxies))
                    .or_else(|| {
                        (!require_device_peer)
                            .then_some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
                    });
                let auth = AuthContext {
                    actor: "unauthenticated".into(),
                    requesting_ip: ip,
                    ..AuthContext::default()
                };
                let refuse = |message: &str| {
                    Box::new(wire::Denied {
                        error: RpcError::new(Code::PermissionDenied, message),
                        auth: auth.clone(),
                    })
                };
                if !public_device_auth {
                    return Err(refuse("device pairing is unavailable on this listener"));
                }
                let Some(ip) = ip else {
                    return Err(refuse("device pairing requires a transport peer"));
                };
                device_rates
                    .admit(&method.procedure, ip)
                    .map(|_| auth.clone())
                    .map_err(|error| {
                        Box::new(wire::Denied {
                            error,
                            auth: auth.clone(),
                        })
                    })
            },
        );
        let completion_audit = Arc::clone(&self.audit);
        let rejection_audit = Arc::clone(&self.audit);
        let denied_audit = Arc::clone(&self.audit);
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
        let server = UnaryServer::new_with_peer(catalog, authorize)
            .with_max_request_bytes(8 << 20)
            .with_max_stream_message_bytes(8 << 20)
            .with_admission_denial_observer(Arc::new(move |method, auth, message, error| {
                audit::record_denied(&denied_audit, method, auth, message, error);
            }))
            .with_completion_observer(observer)
            .with_rejection_observer(Arc::new(move |method, auth, error| {
                audit::record_rejection(&rejection_audit, method, auth, error);
            }));
        let hub = Arc::new(self);
        let gate_hub = Arc::clone(&hub);
        let mut server =
            server.with_gate(Arc::new(move |request| approvals::gate(&gate_hub, request)));
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
        approvals::register(&mut server, Arc::clone(&hub))?;
        device::register(&mut server, Arc::clone(&hub))?;
        grants::register(&mut server, Arc::clone(&hub))?;
        access::register(&mut server, Arc::clone(&hub))?;
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
        server.stream(event_service::SUBSCRIBE, move |call: Call<_>, sender| {
            let hub = Arc::clone(&subscribe);
            async move {
                let mut events = hub.events.sender.subscribe();
                let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
                let watch = (!call.auth.token_id.is_empty())
                    .then(|| hub.identity.watch(&call.auth.token_id, protectorate_fsutil::unix_seconds()));
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
                        _ = heartbeat.tick() => hub.events.event(
                            "heartbeat",
                            "hub",
                            json!({"uptime_seconds": hub.started.elapsed().as_secs_f64()}),
                        ),
                        next = events.recv() => match next {
                            Ok(event) => event,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        },
                    };
                    let send = sender.send(&event);
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
        server.unary(hub_service::GET_STATUS, move |_: Call<_>| {
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
                Ok(v1::GetStatusResponse {
                    version: hub.version.clone(),
                    release_confirmed: hub.releases.as_ref().is_none_or(|source| {
                        !source.dir().join(".hub-trial.json").exists()
                            && source.current().version == hub.version
                    }),
                    hostname: hub.hostname.clone(),
                    os: platform_os().into(),
                    arch: platform_arch().into(),
                    uptime_seconds: hub
                        .started
                        .elapsed()
                        .as_secs()
                        .try_into()
                        .unwrap_or(i64::MAX),
                    started_at: Some(proto_time(hub.started_at)),
                    available_version: available.version,
                    update_channel: channel.into(),
                    release_notes_url: available.notes_url,
                    audit_head: Some(audit::head(
                        hub.audit.head().ok().flatten().unwrap_or_default(),
                    )),
                })
            }
        })?;
        let runtime_status = Arc::clone(&hub);
        server.unary(hub_service::GET_SLEEVE_RUNTIME, move |_: Call<_>| {
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
                Ok(v1::GetSleeveRuntimeResponse {
                    runtime: if configured { "rust-namespaces" } else { "" }.into(),
                    helper_path: helper,
                    overlays,
                })
            }
        })?;
        let list = Arc::clone(&hub);
        server.unary(backlog_service::LIST_NOTES, move |call: Call<_>| {
            let hub = Arc::clone(&list);
            async move {
                let request = call.request;
                let notes = hub
                    .backlog
                    .list(Filter {
                        project: request.project,
                        query: request.query,
                        state: request.state,
                    })
                    .map_err(backlog_error)?;
                Ok(v1::ListNotesResponse {
                    notes: notes.iter().map(note).collect(),
                })
            }
        })?;
        let get = Arc::clone(&hub);
        server.unary(backlog_service::GET_NOTE, move |call: Call<_>| {
            let hub = Arc::clone(&get);
            async move {
                let found = hub.backlog.get(&call.request.id).map_err(backlog_error)?;
                Ok(v1::GetNoteResponse {
                    note: Some(note(&found)),
                })
            }
        })?;
        let create = Arc::clone(&hub);
        server.unary(backlog_service::CREATE_NOTE, move |call: Call<_>| {
            let hub = Arc::clone(&create);
            async move {
                let request = call.request;
                let created = hub
                    .backlog
                    .create(NewNote {
                        title: request.title,
                        body: request.body,
                        pinned: request.pinned,
                        project: request.project,
                        source: sender(&call.auth.actor),
                    })
                    .map_err(backlog_error)?;
                hub.emit("note.created", &created.id, note_event_payload(&created));
                Ok(v1::CreateNoteResponse {
                    note: Some(note(&created)),
                })
            }
        })?;
        let update = Arc::clone(&hub);
        server.unary(backlog_service::UPDATE_NOTE, move |call: Call<_>| {
            let hub = Arc::clone(&update);
            async move {
                let request = call.request;
                let (updated, changed) = hub
                    .backlog
                    .update(
                        &request.id,
                        Patch {
                            title: request.title,
                            body: request.body,
                            pinned: request.pinned,
                            project: request.project,
                            state: request.state,
                            resolution: request.resolution,
                            done_by: call.auth.actor,
                        },
                    )
                    .map_err(backlog_error)?;
                if changed {
                    hub.emit("note.updated", &updated.id, note_event_payload(&updated));
                }
                Ok(v1::UpdateNoteResponse {
                    note: Some(note(&updated)),
                })
            }
        })?;
        let delete = Arc::clone(&hub);
        server.unary(backlog_service::DELETE_NOTE, move |call: Call<_>| {
            let hub = Arc::clone(&delete);
            async move {
                let deleted = hub
                    .backlog
                    .delete(&call.request.id)
                    .map_err(backlog_error)?;
                hub.emit("note.deleted", &deleted.id, note_event_payload(&deleted));
                Ok(v1::DeleteNoteResponse {})
            }
        })?;
        let list_sessions = Arc::clone(&hub);
        server.unary(session_service::LIST_SESSIONS, move |call: Call<_>| {
            let manager = Arc::clone(&list_sessions.sessions);
            async move {
                let sessions = tokio::task::spawn_blocking(move || manager.list())
                    .await
                    .map_err(join_error)?
                    .map_err(session_error)?;
                let own_session = sleeve_session(&call.auth.actor);
                Ok(v1::ListSessionsResponse {
                    sessions: sessions
                        .iter()
                        .filter(|s| own_session.as_ref().is_none_or(|name| s.name == *name))
                        .map(session_message)
                        .collect(),
                })
            }
        })?;
        let create_session = Arc::clone(&hub);
        server.unary(
            session_service::CREATE_SESSION,
            move |call: Call<v1::CreateSessionRequest>| {
                let hub = Arc::clone(&create_session);
                async move {
                    let request = call.request;
                    if request.name.starts_with("sleeve-") {
                        return Err(RpcError::new(
                            Code::FailedPrecondition,
                            "sleeve names are reserved",
                        ));
                    }
                    let options = SessionOptions {
                        name: request.name,
                        cwd: Some(request.cwd).filter(|s| !s.is_empty()).map(Into::into),
                        command: request.command,
                        env: request.env.into_iter().collect(),
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
                    Ok(v1::CreateSessionResponse {
                        session: Some(session_message(&session)),
                    })
                }
            },
        )?;
        let kill_session = Arc::clone(&hub);
        server.unary(
            session_service::KILL_SESSION,
            move |call: Call<v1::KillSessionRequest>| {
                let hub = Arc::clone(&kill_session);
                async move {
                    let name = call.request.name;
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
                    Ok(v1::KillSessionResponse {})
                }
            },
        )?;
        let respawn_session = Arc::clone(&hub);
        server.unary(
            session_service::RESPAWN_SESSION,
            move |call: Call<v1::RespawnSessionRequest>| {
                let hub = Arc::clone(&respawn_session);
                async move {
                    let name = call.request.name;
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
                    Ok(v1::RespawnSessionResponse {
                        session: Some(session_message(&session)),
                    })
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
        server.unary(
            session_service::CAPTURE_PANE,
            move |call: Call<v1::CapturePaneRequest>| {
                let manager = Arc::clone(&hub.sessions);
                async move {
                    let name = call.request.name;
                    if sleeve_session(&call.auth.actor).is_some_and(|own| own != name) {
                        return Err(RpcError::new(
                            Code::PermissionDenied,
                            "sleeve may only capture its own session",
                        ));
                    }
                    let lines = call.request.lines.max(0) as usize;
                    let text = tokio::task::spawn_blocking(move || manager.capture(&name, lines))
                        .await
                        .map_err(join_error)?
                        .map_err(session_error)?;
                    Ok(v1::CapturePaneResponse { text })
                }
            },
        )?;
        let debug = Router::new()
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
            .route_layer(axum::middleware::from_fn(access::loopback_only));
        Ok(server
            .router()
            .merge(debug)
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
                "/ws/terminal/{name}",
                axum::routing::get(
                    move |path,
                          peer: Option<axum::Extension<wire::Peer>>,
                          headers,
                          uri,
                          upgrade| {
                        let tls = peer.is_some_and(|peer| peer.0.tls);
                        terminal::attach(
                            Arc::clone(&terminal_hub),
                            path,
                            tls,
                            headers,
                            uri,
                            upgrade,
                        )
                    },
                ),
            ))
    }
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
/// The current time as the RFC 3339 text the registries store.
fn now() -> String {
    timestamp(Utc::now())
}
fn proto_time(time: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: time.timestamp(),
        nanos: time.timestamp_subsec_nanos().min(999_999_999) as i32,
    }
}
fn unix_time(seconds: i64) -> prost_types::Timestamp {
    prost_types::Timestamp { seconds, nanos: 0 }
}
fn note(note: &Note) -> v1::Note {
    v1::Note {
        id: note.id.clone(),
        title: note.title.clone(),
        body: note.body.clone(),
        created_at: Some(proto_time(note.created_at)),
        updated_at: Some(proto_time(note.updated_at)),
        pinned: note.pinned,
        project: note.project.clone(),
        source: note.source.clone(),
        state: note.state.clone(),
        completion: note.done_at.map(|done_at| v1::NoteCompletion {
            done_at: Some(proto_time(done_at)),
            done_by: note.done_by.clone(),
            resolution: note.resolution.clone(),
        }),
    }
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

fn session_message(session: &session::Session) -> v1::Session {
    v1::Session {
        name: session.name.clone(),
        cwd: session.cwd.to_string_lossy().into_owned(),
        command: session.command.clone(),
        created_at: Some(proto_time(session.created.into())),
        attached_clients: session.attached_clients.try_into().unwrap_or(i32::MAX),
        last_activity: Some(proto_time(session.last_activity.into())),
        pane_cols: session.cols.try_into().unwrap_or(i32::MAX),
        pane_rows: session.rows.try_into().unwrap_or(i32::MAX),
        state: match session.state {
            SessionState::Running => v1::SessionState::Running,
            SessionState::Exited => v1::SessionState::Exited,
        } as i32,
        exit_code: session.exit_code,
        exit_signal: session.exit_signal,
        exited_at: session.exited.map(|exited| proto_time(exited.into())),
    }
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
    RpcError::from_join(error)
}
