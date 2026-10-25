//! A fail-closed plan and Linux launcher for Protectorate workloads.
//! [`ValidatedPlan`] describes authority; [`runtime`] isolates the process.
//! Missing namespaces, cgroups, or requested overlay support are launch
//! errors. A sandboxed workload never falls back to running on the host.

#[cfg(target_os = "linux")]
pub mod egress;
#[cfg(windows)]
#[path = "job_windows.rs"]
pub mod job;
#[cfg(target_os = "linux")]
pub mod runtime;
#[cfg(windows)]
#[path = "runtime_windows.rs"]
pub mod runtime;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workload {
    ChatSleeve,
    TerminalSleeve,
    NestedRuntimeTestSleeve,
    SharedNodeJob,
    ReadOnlyHelper(Helper),
    /// Snapshots and restores a sleeve workspace into a private store kept in
    /// its home, so the workspace is writable and nothing else is.
    CheckpointHelper,
    /// An administrator intentionally requested an unsandboxed job. A plan
    /// carrying this variant never contains sandbox grants or credentials.
    PlainNodeJob,
    /// The local developer command uses a harness's official headless
    /// interface with the user's own configuration, outside any sleeve.
    DirectHarnessDriver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Helper {
    Git,
    Handoff,
    Catalog,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    Claude,
    Codex,
    Grok,
    Pi,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Pi => "pi",
        }
    }

    /// The hosts each harness needs for inference and token refresh, taken
    /// from the installed CLIs. Pi covers its subscription sign-ins plus
    /// Gemini and OpenRouter; other Pi providers need operator entries.
    pub fn api_hosts(self) -> &'static [&'static str] {
        const CLAUDE: &[&str] = &["api.anthropic.com", "claude.ai", "platform.claude.com"];
        const CODEX: &[&str] = &["api.openai.com", "chatgpt.com", "auth.openai.com"];
        const GROK: &[&str] = &[
            "api.x.ai",
            "auth.x.ai",
            "accounts.x.ai",
            "cli-chat-proxy.grok.com",
        ];
        const PI: &[&str] = &[
            "api.anthropic.com",
            "platform.claude.com",
            "api.openai.com",
            "chatgpt.com",
            "auth.openai.com",
            "api.x.ai",
            "auth.x.ai",
            "generativelanguage.googleapis.com",
            "openrouter.ai",
        ];
        match self {
            Self::Claude => CLAUDE,
            Self::Codex => CODEX,
            Self::Grok => GROK,
            Self::Pi => PI,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindGrant {
    pub source: PathBuf,
    pub target: PathBuf,
    pub access: Access,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheOverlay {
    pub lower: PathBuf,
    /// Per-workload private state below `<namespace>/<name>/overlays`, outside
    /// the home mount and therefore invisible to the workload.
    pub upper: PathBuf,
    pub work: PathBuf,
    pub target: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMask {
    /// A regular empty file owned by the runtime, never `/dev/null`.
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// 100 means one core; zero is unlimited except for nested workloads.
    pub cpu_percent: u32,
    /// Zero is unlimited except for nested workloads.
    pub memory_bytes: u64,
    /// Must be positive for every sandboxed workload.
    pub tasks_max: u32,
}

/// Sizes toolchain worker pools to a CPU quota of `cpu_percent` (100 per
/// core, rounded down, at least one). A quota does not change the core count
/// tools detect, so without this each would start a worker per host core.
/// Values already in `environment` win; zero (unlimited) sets nothing.
pub fn limit_build_jobs(environment: &mut BTreeMap<String, String>, cpu_percent: u32) {
    if cpu_percent == 0 {
        return;
    }
    let jobs = (cpu_percent / 100).max(1);
    for (key, value) in [
        ("GOMAXPROCS", jobs.to_string()),
        ("GOFLAGS", format!("-p={jobs}")),
        ("CARGO_BUILD_JOBS", jobs.to_string()),
        ("FLUTTER_TEST_CONCURRENCY", jobs.to_string()),
    ] {
        environment.entry(key.into()).or_insert(value);
    }
}

/// Outbound authority requested for a sandbox. The provider API hosts of the
/// request's credential are added by the policy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Egress {
    /// `host:port` of the hub. Unlike other destinations it may resolve to a
    /// private or loopback address.
    pub hub: Option<String>,
    /// Operator-approved `host`, `host:port` or `*.domain[:port]` patterns;
    /// the port defaults to 443. Only public addresses are reachable.
    pub allow: Vec<String>,
    /// Hosts reachable on any port at private addresses, such as local model
    /// servers. These are the only IP literals a workload may name.
    pub private_hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Network {
    /// Only for intentionally unsandboxed host processes.
    Host,
    /// A private network namespace with nothing but loopback.
    Loopback,
    /// A private network namespace whose only way out is the host CONNECT
    /// proxy enforcing this policy.
    Proxied(EgressPolicy),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Rule {
    /// Lowercase host, or `*.` followed by a domain for its subdomains.
    host: String,
    /// `None` admits any port.
    port: Option<u16>,
    private: bool,
    source: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EgressPolicy {
    hub: Option<(String, u16)>,
    rules: Vec<Rule>,
}

/// Why a CONNECT target is admitted before its name is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Admission {
    /// Private and loopback addresses are acceptable for this destination.
    pub private: bool,
    pub rule: &'static str,
}

impl EgressPolicy {
    pub fn hub(&self) -> Option<(&str, u16)> {
        self.hub.as_ref().map(|(host, port)| (host.as_str(), *port))
    }

    pub fn admit(&self, host: &str, port: u16) -> std::result::Result<Admission, String> {
        let host = normalize_host(host);
        if self.hub.as_ref() == Some(&(host.clone(), port)) {
            return Ok(Admission {
                private: true,
                rule: "hub",
            });
        }
        let literal = host.parse::<std::net::IpAddr>().is_ok();
        if !literal && !valid_hostname(&host) {
            return Err(format!("invalid host name {host:?}"));
        }
        self.rules
            .iter()
            .find(|rule| {
                rule.port.is_none_or(|p| p == port)
                    && match rule.host.strip_prefix("*.") {
                        Some(domain) => !literal && host.ends_with(&format!(".{domain}")),
                        None => rule.host == host,
                    }
            })
            .map(|rule| Admission {
                private: rule.private,
                rule: rule.source,
            })
            .ok_or_else(|| {
                if literal {
                    "IP literals are refused".to_owned()
                } else {
                    "host is not allowlisted".to_owned()
                }
            })
    }
}

/// Whether an address is globally routable. Private, loopback, link-local
/// (including cloud metadata), shared, documentation, multicast and reserved
/// ranges are not.
pub fn public_address(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0 && v4.octets()[2] == 0)
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_address(IpAddr::V4(v4));
            }
            let s = v6.segments();
            if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0; 4] {
                let [a, b] = s[6].to_be_bytes();
                let [c, d] = s[7].to_be_bytes();
                return public_address(IpAddr::V4([a, b, c, d].into()));
            }
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] == 0))
                || s[0] == 0x2002
                || (s[0] == 0x100 && s[1..4] == [0; 3]))
        }
    }
}

fn normalize_host(host: &str) -> String {
    let host = host.trim_end_matches('.');
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
        .to_ascii_lowercase()
}

fn valid_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Split `host[:port]`; a bracketed IPv6 literal must carry its port.
fn split_host_port(spec: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = spec.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        return Some((host.to_ascii_lowercase(), Some(port.parse().ok()?)));
    }
    match spec.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => Some((
            normalize_host(host),
            Some(port.parse().ok().filter(|p| *p > 0)?),
        )),
        Some(_) => None,
        None => Some((normalize_host(spec), None)),
    }
}

fn egress_policy(egress: Egress, credential: Option<Provider>) -> Result<EgressPolicy> {
    let hub = egress
        .hub
        .map(|spec| match split_host_port(&spec) {
            Some((host, Some(port)))
                if valid_hostname(&host) || host.parse::<std::net::IpAddr>().is_ok() =>
            {
                Ok((host, port))
            }
            _ => Err(invalid(format!(
                "egress hub must be host:port, not {spec:?}"
            ))),
        })
        .transpose()?;
    let mut rules = Vec::new();
    for host in credential.map_or(&[][..], Provider::api_hosts) {
        rules.push(Rule {
            host: (*host).into(),
            port: Some(443),
            private: false,
            source: credential.unwrap().as_str(),
        });
    }
    for spec in egress.allow {
        let (host, port) = split_host_port(&spec)
            .filter(|(host, _)| {
                let name = host.strip_prefix("*.").unwrap_or(host);
                valid_hostname(name) && name.parse::<std::net::IpAddr>().is_err()
            })
            .ok_or_else(|| {
                invalid(format!(
                    "egress allow entry must be a host name pattern, not {spec:?}"
                ))
            })?;
        rules.push(Rule {
            host,
            port: Some(port.unwrap_or(443)),
            private: false,
            source: "operator",
        });
    }
    for spec in egress.private_hosts {
        let host = normalize_host(&spec);
        if !valid_hostname(&host) && host.parse::<std::net::IpAddr>().is_err() {
            return Err(invalid(format!("egress private host is invalid: {spec:?}")));
        }
        rules.push(Rule {
            host,
            port: None,
            private: true,
            source: "local-model",
        });
    }
    Ok(EgressPolicy { hub, rules })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cgroup {
    Bounded,
    DelegatedNested,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub workload: Workload,
    pub command: Vec<String>,
    /// Required for chat and terminal sleeves, optional for a job or helper.
    pub workspace: Option<PathBuf>,
    /// Authorizes only `<state_root>/workspaces/<name>` (or a child) as the
    /// workspace. Required when the workspace is managed by the hub.
    pub managed_workspace_name: Option<String>,
    /// Existing directory below `Policy::state_root`, bound at user_home.
    pub private_home: Option<PathBuf>,
    /// Explicit additional read-only or writable paths. Sibling modules must
    /// be separate narrow grants rather than broad parent mounts.
    pub grants: Vec<BindGrant>,
    /// Read-only roots from `Policy::trusted_tool_roots`. These are a distinct
    /// authority from ordinary grants and must be mounted before overlays.
    pub tool_roots: Vec<PathBuf>,
    /// Optional executable inside those roots; direct symlink hops must stay
    /// within selected roots.
    pub selected_executable: Option<PathBuf>,
    pub credential: Option<Provider>,
    pub overlays: Vec<CacheOverlay>,
    pub masks: Vec<FileMask>,
    pub limits: Limits,
    /// `None` leaves the sandbox with loopback only.
    pub egress: Option<Egress>,
}

#[derive(Clone, Debug)]
pub struct Policy {
    pub state_root: PathBuf,
    pub user_home: PathBuf,
    pub protected: Vec<PathBuf>,
    pub provider_credentials: BTreeMap<Provider, PathBuf>,
    /// Explicitly approved narrow tool directories, such as `.cargo/bin` and
    /// `.rustup/toolchains`. A caller still has to select each one per plan.
    pub trusted_tool_roots: Vec<PathBuf>,
    /// Published immutable cache generations. Live owner caches are not safe
    /// overlay lowers and must not be placed here.
    pub immutable_cache_roots: Vec<PathBuf>,
    pub nested_cgroups_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanKind {
    Sandboxed,
    IntentionallyUnsandboxedJob,
    IntentionallyUnsandboxedHarness,
}

/// Data a launcher can consume after repeating the path checks. Fields are
/// private so callers cannot create an unchecked plan.
#[derive(Clone, Debug)]
pub struct ValidatedPlan {
    kind: PlanKind,
    workload: Workload,
    command: Vec<String>,
    private_home: Option<PathBuf>,
    grants: Vec<BindGrant>,
    tool_grants: Vec<BindGrant>,
    selected_executable: Option<PathBuf>,
    credential: Option<BindGrant>,
    overlays: Vec<CacheOverlay>,
    masks: Vec<FileMask>,
    limits: Limits,
    network: Network,
    cgroup: Option<Cgroup>,
}

impl ValidatedPlan {
    pub fn kind(&self) -> PlanKind {
        self.kind.clone()
    }
    pub fn workload(&self) -> Workload {
        self.workload
    }
    pub fn command(&self) -> &[String] {
        &self.command
    }
    pub fn private_home(&self) -> Option<&Path> {
        self.private_home.as_deref()
    }
    pub fn grants(&self) -> &[BindGrant] {
        &self.grants
    }
    pub fn tool_grants(&self) -> &[BindGrant] {
        &self.tool_grants
    }
    pub fn selected_executable(&self) -> Option<&Path> {
        self.selected_executable.as_deref()
    }
    pub fn credential(&self) -> Option<&BindGrant> {
        self.credential.as_ref()
    }
    pub fn overlays(&self) -> &[CacheOverlay] {
        &self.overlays
    }
    pub fn masks(&self) -> &[FileMask] {
        &self.masks
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn network(&self) -> &Network {
        &self.network
    }
    pub fn cgroup(&self) -> Option<Cgroup> {
        self.cgroup
    }
}

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

impl Policy {
    pub fn plan(&self, request: Request) -> Result<ValidatedPlan> {
        validate_command(&request.command)?;
        let state = existing_dir(&self.state_root)?;
        let user_home = existing_dir(&self.user_home)?;
        if state == user_home {
            return Err(invalid("state root cannot be the entire user home"));
        }
        if matches!(
            request.workload,
            Workload::PlainNodeJob | Workload::DirectHarnessDriver
        ) {
            if request.workspace.is_some()
                || request.managed_workspace_name.is_some()
                || request.private_home.is_some()
                || !request.grants.is_empty()
                || !request.tool_roots.is_empty()
                || request.selected_executable.is_some()
                || request.credential.is_some()
                || !request.overlays.is_empty()
                || !request.masks.is_empty()
                || request.egress.is_some()
            {
                return Err(invalid(
                    "direct host processes cannot carry sandbox mounts or credentials",
                ));
            }
            return Ok(ValidatedPlan {
                kind: if request.workload == Workload::PlainNodeJob {
                    PlanKind::IntentionallyUnsandboxedJob
                } else {
                    PlanKind::IntentionallyUnsandboxedHarness
                },
                workload: request.workload,
                command: request.command,
                private_home: None,
                grants: Vec::new(),
                tool_grants: Vec::new(),
                selected_executable: None,
                credential: None,
                overlays: Vec::new(),
                masks: Vec::new(),
                limits: request.limits,
                network: Network::Host,
                cgroup: None,
            });
        }
        if request.limits.tasks_max == 0 {
            return Err(invalid("sandboxed workloads require a positive task limit"));
        }
        if request.limits.memory_bytes != 0 && request.limits.memory_bytes < 64 * 1024 * 1024 {
            return Err(invalid("memory limit must be at least 64 MiB"));
        }
        let nested = request.workload == Workload::NestedRuntimeTestSleeve;
        if nested
            && (request.limits.cpu_percent == 0
                || request.limits.memory_bytes == 0
                || !self.nested_cgroups_available)
        {
            return Err(invalid("nested runtime tests require CPU and memory limits and available cgroup delegation"));
        }
        let home = request
            .private_home
            .ok_or_else(|| invalid("sandboxed workloads require a private home"))?;
        let home = validate_private_home(&home, &state, request.workload)?;
        if matches!(request.workload, Workload::ReadOnlyHelper(_))
            && (request.credential.is_some() || !request.overlays.is_empty())
        {
            return Err(invalid(
                "read-only helpers cannot mount credentials or writable cache overlays",
            ));
        }
        let protected = self.protected_paths(&state, &user_home)?;
        let mut grants = Vec::new();
        let workspace_access = if matches!(request.workload, Workload::ReadOnlyHelper(_)) {
            Access::ReadOnly
        } else {
            Access::ReadWrite
        };
        let managed_name = request.managed_workspace_name.clone();
        let managed = request
            .managed_workspace_name
            .as_deref()
            .map(|name| {
                validate_name(name)?;
                let path = state.join("workspaces").join(name);
                let real = existing_dir(&path)?;
                if real != path {
                    return Err(invalid("managed workspace path contains a symlink"));
                }
                Ok(real)
            })
            .transpose()?;
        let workspace = request
            .workspace
            .map(|p| self.validate_source(&p, workspace_access, &protected, managed.as_deref()))
            .transpose()?;
        if let Some(managed) = &managed {
            if workspace.as_ref().is_none_or(|ws| !ws.starts_with(managed)) {
                return Err(invalid(
                    "named workspace must stay within its own managed directory",
                ));
            }
        }
        if matches!(
            request.workload,
            Workload::ChatSleeve | Workload::TerminalSleeve | Workload::NestedRuntimeTestSleeve
        ) && workspace.is_none()
        {
            return Err(invalid("sleeves require an explicit workspace"));
        }
        if let Some(ws) = &workspace {
            grants.push(BindGrant {
                source: ws.clone(),
                target: ws.clone(),
                access: workspace_access,
            });
            if request.workload == Workload::ReadOnlyHelper(Helper::Git) {
                for extra in git_helper_dirs(ws) {
                    if grants.iter().any(|grant| {
                        extra.starts_with(&grant.source) || extra.starts_with(&grant.target)
                    }) {
                        continue;
                    }
                    if grants.iter().any(|grant| grant.source.starts_with(&extra)) {
                        return Err(invalid(format!(
                            "linked git directory {} would overlap the workspace mount",
                            extra.display()
                        )));
                    }
                    let source = self
                        .validate_source(&extra, Access::ReadOnly, &protected, managed.as_deref())
                        .map_err(|_| {
                            invalid(format!(
                                "the repository of {} is at {}, which is not readable",
                                ws.display(),
                                extra.display()
                            ))
                        })?;
                    grants.push(BindGrant {
                        source: source.clone(),
                        target: source,
                        access: Access::ReadOnly,
                    });
                }
            }
        }
        for grant in request.grants {
            if matches!(request.workload, Workload::ReadOnlyHelper(_))
                && grant.access == Access::ReadWrite
            {
                return Err(invalid("read-only helpers cannot have writable grants"));
            }
            // Managed clones sit below the hub state directory. A named clone
            // root may be shared read-only, either at its real path for relative
            // dependencies or at a separately validated target.
            let managed_sibling = grant.access == Access::ReadOnly
                && named_managed_workspace(&grant.source, &state).is_some();
            let own_base_view = grant.access == Access::ReadOnly
                && grant.source == grant.target
                && managed_name
                    .as_deref()
                    .is_some_and(|name| grant.source == state.join("base-views").join(name));
            let (source, target) = if managed_sibling || own_base_view {
                let source = existing_dir(&grant.source)?;
                if source != grant.source {
                    return Err(invalid("managed sibling path contains a symlink"));
                }
                let target = if managed_sibling && grant.target != grant.source {
                    validate_target(&grant.target, &user_home, &state)?
                } else {
                    grant.target
                };
                (source, target)
            } else {
                (
                    self.validate_source(&grant.source, grant.access, &protected, None)?,
                    validate_target(&grant.target, &user_home, &state)?,
                )
            };
            grants.push(BindGrant {
                source,
                target,
                access: grant.access,
            });
        }
        let tool_grants = request
            .tool_roots
            .into_iter()
            .map(|root| self.validate_tool_root(&root, &user_home, &state))
            .collect::<Result<Vec<_>>>()?;
        let all_grants = grants
            .iter()
            .chain(&tool_grants)
            .cloned()
            .collect::<Vec<_>>();
        reject_overlapping_targets(&all_grants)?;
        reject_mixed_source_overlap(&all_grants)?;
        let selected_executable = request
            .selected_executable
            .map(|path| validate_selected_executable(&path, &tool_grants))
            .transpose()?;
        if let Some(executable) = &selected_executable {
            if Path::new(&request.command[0]) != executable {
                return Err(invalid(
                    "selected executable must be the command being launched",
                ));
            }
        }
        // The home mount is a deliberate parent of workspaces under $HOME,
        // but no grant may replace the entire home or expose private state.
        if grants
            .iter()
            .any(|grant| grant.target == user_home || grant.target.starts_with(&home))
        {
            return Err(invalid("grant shadows private home or its state"));
        }
        let credential = request
            .credential
            .map(|provider| {
                if matches!(
                    request.workload,
                    Workload::SharedNodeJob
                        | Workload::ReadOnlyHelper(_)
                        | Workload::CheckpointHelper
                ) {
                    return Err(invalid(
                        "jobs and helpers cannot receive provider credentials",
                    ));
                }
                let source = self.provider_credentials.get(&provider).ok_or_else(|| {
                    invalid("selected provider credential source is not configured")
                })?;
                let source = existing_dir(source)?;
                if source != state.join("auth").join(provider.as_str()) {
                    return Err(invalid(
                        "provider credential source must be its dedicated auth directory",
                    ));
                }
                let target = PathBuf::from(format!("/run/protectorate/auth/{}", provider.as_str()));
                Ok(BindGrant {
                    source,
                    target,
                    access: Access::ReadWrite,
                })
            })
            .transpose()?;
        let overlays = request
            .overlays
            .into_iter()
            .map(|overlay| self.validate_overlay(overlay, &home, &user_home, &all_grants))
            .collect::<Result<Vec<_>>>()?;
        reject_overlapping_overlay_targets(&overlays, &all_grants)?;
        let masks = request
            .masks
            .into_iter()
            .map(|mask| validate_mask(mask, &state, &grants))
            .collect::<Result<Vec<_>>>()?;
        let network = match request.egress {
            Some(egress) => Network::Proxied(egress_policy(egress, request.credential)?),
            None => Network::Loopback,
        };
        Ok(ValidatedPlan {
            kind: PlanKind::Sandboxed,
            workload: request.workload,
            command: request.command,
            private_home: Some(home),
            grants,
            tool_grants,
            selected_executable,
            credential,
            overlays,
            masks,
            limits: request.limits,
            network,
            cgroup: Some(if nested {
                Cgroup::DelegatedNested
            } else {
                Cgroup::Bounded
            }),
        })
    }

    fn protected_paths(&self, state: &Path, user_home: &Path) -> Result<Vec<PathBuf>> {
        let mut paths = vec![state.to_path_buf(), user_home.to_path_buf()];
        for name in [
            ".claude",
            ".codex",
            ".grok",
            ".pi",
            ".ssh",
            ".gnupg",
            ".aws",
            ".cargo",
            ".rustup",
            ".local/share",
            ".config",
            ".docker",
            ".kube",
            ".netrc",
            ".git-credentials",
            ".npmrc",
            ".pypirc",
            ".password-store",
            ".pub-cache",
            "go",
        ] {
            let path = user_home.join(name);
            paths.push(path.clone());
            if path.exists() {
                paths.push(fs::canonicalize(path)?);
            }
        }
        for path in &self.protected {
            clean_absolute(path)?;
            paths.push(path.clone());
            if path.exists() {
                paths.push(fs::canonicalize(path)?);
            }
        }
        Ok(paths)
    }

    fn validate_tool_root(&self, root: &Path, user_home: &Path, state: &Path) -> Result<BindGrant> {
        clean_absolute(root)?;
        if !self
            .trusted_tool_roots
            .iter()
            .any(|approved| approved == root)
        {
            return Err(invalid("tool root is not configured by the policy"));
        }
        let real = existing_dir(root)?;
        if real != root
            || overlaps(&real, state)
            || real == user_home
            || user_home.starts_with(&real)
            || sensitive_system(&real)
        {
            return Err(invalid(
                "tool root exposes state, the user home, or a system mount",
            ));
        }
        // The hub explicitly configures SDKs and tools for the current host.
        // Paths such as ~/development/flutter or /opt/flutter must work too.
        // Only these known narrow exceptions may live inside a protected
        // package-manager home; provider credentials never become tools.
        const NARROW: &[&str] = &[
            ".cargo/bin",
            ".rustup/toolchains",
            ".local/bin",
            ".local/lib",
            ".local/opt",
            ".local/share/claude/versions",
            ".local/share/pi-omp-setup",
            ".grok/downloads",
            "go/bin",
            ".pub-cache/bin",
            ".pub-cache/global_packages",
        ];
        let exception = root
            .strip_prefix(user_home)
            .ok()
            .is_some_and(|relative| NARROW.iter().any(|suffix| relative == Path::new(suffix)));
        if self
            .protected
            .iter()
            .any(|protected| overlaps(&real, protected))
        {
            return Err(invalid("tool root exposes an explicitly protected path"));
        }
        if !exception
            && self
                .protected_paths(state, user_home)?
                .iter()
                .filter(|protected| *protected != state && *protected != user_home)
                .any(|protected| overlaps(&real, protected))
        {
            return Err(invalid("tool root exposes protected host data"));
        }
        Ok(BindGrant {
            source: real,
            target: root.to_path_buf(),
            access: Access::ReadOnly,
        })
    }

    fn validate_source(
        &self,
        source: &Path,
        access: Access,
        protected: &[PathBuf],
        managed: Option<&Path>,
    ) -> Result<PathBuf> {
        clean_absolute(source)?;
        let real = if access == Access::ReadOnly {
            existing_file_or_dir(source)?
        } else {
            existing_dir(source)?
        };
        if let Some(managed) = managed {
            if real.starts_with(managed) && source.starts_with(managed) {
                return Ok(real);
            }
        }
        if sensitive_system(&real) || sensitive_system(source) {
            return Err(invalid("bind source touches a system mount"));
        }
        if protected
            .iter()
            .any(|path| overlaps(&real, path) || overlaps(source, path))
        {
            // Narrow paths under the user home are allowed, but the home itself
            // and all known secret subtrees remain forbidden.
            let home = existing_dir(&self.user_home)?;
            let only_home_overlap = real.starts_with(&home)
                && source.starts_with(&home)
                && protected
                    .iter()
                    .filter(|p| *p != &home)
                    .all(|p| !overlaps(&real, p) && !overlaps(source, p));
            if !only_home_overlap {
                return Err(invalid("bind source exposes protected host data"));
            }
        }
        if access == Access::ReadWrite
            && self
                .immutable_cache_roots
                .iter()
                .any(|root| overlaps(&real, root))
        {
            return Err(invalid("published cache cannot be writable"));
        }
        Ok(real)
    }

    fn validate_overlay(
        &self,
        overlay: CacheOverlay,
        home: &Path,
        user_home: &Path,
        grants: &[BindGrant],
    ) -> Result<CacheOverlay> {
        if self.immutable_cache_roots.is_empty() {
            return Err(invalid(
                "cache overlays require a published immutable lower",
            ));
        }
        let lower = existing_dir(&overlay.lower)?;
        let state = existing_dir(&self.state_root)?;
        if !lower.starts_with(state.join("cache")) {
            return Err(invalid("overlay lower must be a published state cache"));
        }
        if !self
            .immutable_cache_roots
            .iter()
            .any(|root| fs::canonicalize(root).is_ok_and(|r| lower.starts_with(r)))
        {
            return Err(invalid("overlay lower is not a published immutable cache"));
        }
        let upper = existing_dir(&overlay.upper)?;
        let work = existing_dir(&overlay.work)?;
        let overlay_state = home
            .parent()
            .ok_or_else(|| invalid("private home has no workload state directory"))?
            .join("overlays");
        if upper == work
            || !upper.starts_with(&overlay_state)
            || !work.starts_with(&overlay_state)
            || upper == overlay_state
            || work == overlay_state
            || upper.parent() != work.parent()
            || overlaps(&upper, &work)
        {
            return Err(invalid(
                "overlay upper and work must be distinct directories in private workload overlay state",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if fs::metadata(&upper)?.dev() != fs::metadata(&work)?.dev() {
                return Err(invalid("overlay upper and work must share a filesystem"));
            }
        }
        let target = validate_target(&overlay.target, user_home, &state)?;
        if grants
            .iter()
            .any(|grant| !overlay_layer_allowed(&target, grant))
        {
            return Err(invalid("overlay target overlaps a bind grant"));
        }
        Ok(CacheOverlay {
            lower,
            upper,
            work,
            target,
        })
    }
}

fn validate_command(command: &[String]) -> Result<()> {
    if command.is_empty() || command[0].is_empty() || command.iter().any(|arg| arg.contains('\0')) {
        return Err(invalid(
            "command must contain an executable and no NUL bytes",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid("managed workspace name is invalid"));
    }
    Ok(())
}

fn named_managed_workspace<'a>(path: &'a Path, state: &Path) -> Option<&'a str> {
    let relative = path.strip_prefix(state.join("workspaces")).ok()?;
    let mut parts = relative.components();
    let Component::Normal(name) = parts.next()? else {
        return None;
    };
    if parts.next().is_some() {
        return None;
    }
    let name = name.to_str()?;
    validate_name(name).ok()?;
    Some(name)
}

/// Identify the named managed checkout containing a workload's workspace.
/// The policy still checks that the directory exists and resolves within it.
pub fn managed_workspace_name(state: &Path, workspace: &Path) -> Option<String> {
    let relative = workspace.strip_prefix(state.join("workspaces")).ok()?;
    let Component::Normal(name) = relative.components().next()? else {
        return None;
    };
    let name = name.to_str()?;
    validate_name(name).ok()?;
    Some(name.to_owned())
}

fn validate_selected_executable(path: &Path, tools: &[BindGrant]) -> Result<PathBuf> {
    clean_absolute(path)?;
    let mut hop = path.to_path_buf();
    for _ in 0..=16 {
        let root = tools
            .iter()
            .find(|grant| hop.starts_with(&grant.target))
            .ok_or_else(|| invalid("executable symlink hop is outside selected tool roots"))?;
        let mut ancestor = hop
            .parent()
            .ok_or_else(|| invalid("invalid executable path"))?;
        while ancestor != root.target {
            if !ancestor.starts_with(&root.target) {
                return Err(invalid("executable has an unmounted parent"));
            }
            if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
                return Err(invalid(
                    "executable has a symlinked directory; select its resolved tool root",
                ));
            }
            ancestor = ancestor
                .parent()
                .ok_or_else(|| invalid("invalid executable path"))?;
        }
        let meta = fs::symlink_metadata(&hop)?;
        if meta.file_type().is_symlink() {
            let link = fs::read_link(&hop)?;
            let next = if link.is_absolute() {
                link
            } else {
                hop.parent().unwrap().join(link)
            };
            hop = normalize_link_target(&next)?;
            continue;
        }
        if !meta.is_file() {
            return Err(invalid("selected tool executable must be a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                return Err(invalid("selected tool executable is not executable"));
            }
        }
        return Ok(path.to_path_buf());
    }
    Err(invalid("tool executable has too many symlink hops"))
}

fn normalize_link_target(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("symlink target must resolve absolutely"));
    }
    let mut out = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => out.push(name),
            Component::ParentDir => {
                if !out.pop() {
                    return Err(invalid("symlink escapes filesystem root"));
                }
            }
            Component::Prefix(_) => return Err(invalid("unsupported executable path prefix")),
        }
    }
    Ok(out)
}

fn clean_absolute(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(invalid(format!(
            "path must be clean and absolute: {}",
            path.display()
        )));
    }
    if path.to_string_lossy().contains("//") {
        return Err(invalid("path must be clean and absolute"));
    }
    Ok(())
}

fn existing_dir(path: &Path) -> Result<PathBuf> {
    clean_absolute(path)?;
    let real = fs::canonicalize(path)?;
    if !fs::metadata(&real)?.is_dir() {
        return Err(invalid(format!("not a directory: {}", path.display())));
    }
    Ok(real)
}

fn existing_file_or_dir(path: &Path) -> Result<PathBuf> {
    clean_absolute(path)?;
    let real = fs::canonicalize(path)?;
    let kind = fs::metadata(&real)?;
    if !kind.is_file() && !kind.is_dir() {
        return Err(invalid(format!(
            "not a regular file or directory: {}",
            path.display()
        )));
    }
    Ok(real)
}

fn validate_private_home(path: &Path, state: &Path, workload: Workload) -> Result<PathBuf> {
    let real = existing_dir(path)?;
    if real != path {
        return Err(invalid("private home path contains a symlink"));
    }
    let namespace = match workload {
        Workload::ChatSleeve | Workload::TerminalSleeve | Workload::NestedRuntimeTestSleeve => {
            "sleeves"
        }
        Workload::SharedNodeJob => "jobs",
        Workload::ReadOnlyHelper(_) | Workload::CheckpointHelper => "helpers",
        Workload::PlainNodeJob | Workload::DirectHarnessDriver => {
            return Err(invalid("direct host processes have no private home"))
        }
    };
    let relative = real
        .strip_prefix(state.join(namespace))
        .map_err(|_| invalid("private home is outside its workload namespace"))?;
    let parts = relative.components().collect::<Vec<_>>();
    if parts.len() != 2 || parts[1] != Component::Normal(std::ffi::OsStr::new("home")) {
        return Err(invalid("private home must be <namespace>/<name>/home"));
    }
    let name = parts[0]
        .as_os_str()
        .to_str()
        .ok_or_else(|| invalid("invalid private-home name"))?;
    validate_name(name)?;
    Ok(real)
}

fn validate_target(target: &Path, user_home: &Path, state: &Path) -> Result<PathBuf> {
    clean_absolute(target)?;
    if target == Path::new("/")
        || target == user_home
        || target.starts_with(state)
        || sensitive_system(target)
    {
        return Err(invalid("bind target shadows sandbox infrastructure"));
    }
    // Existing host-side symlink ancestors would redirect a mount after
    // system/home binds. Reject them instead of guessing launcher ordering.
    let mut ancestor = target;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| invalid("invalid bind target"))?;
    }
    if fs::canonicalize(ancestor)? != ancestor {
        return Err(invalid("bind target has a symlink ancestor"));
    }
    Ok(target.to_path_buf())
}

fn sensitive_system(path: &Path) -> bool {
    [
        "/usr", "/etc", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/proc", "/dev", "/sys",
        "/run", "/tmp", "/var/tmp",
    ]
    .iter()
    .any(|prefix| {
        let root = Path::new(prefix);
        path == root
            || root.starts_with(path)
            || (path.starts_with(root) && *prefix != "/tmp" && *prefix != "/var/tmp")
    })
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Extra Git directories a linked worktree or submodule needs besides the
/// workspace checkout: the `.git` file's gitdir and its commondir, resolved, with nested paths coalesced to the outer dir.
fn linked_git_dirs(workspace: &Path) -> Vec<PathBuf> {
    let raw = match fs::read_to_string(workspace.join(".git")) {
        Ok(raw) => raw,
        Err(_) => return Vec::new(),
    };
    let Some(name) = raw.trim().strip_prefix("gitdir:") else {
        return Vec::new();
    };
    let gitdir = {
        let name = name.trim();
        let path = if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            workspace.join(name)
        };
        match existing_dir_if_present(&path) {
            Some(path) => path,
            None => return Vec::new(),
        }
    };
    let mut dirs = vec![gitdir.clone()];
    if let Ok(common) = fs::read_to_string(gitdir.join("commondir")) {
        let common = common.trim();
        let path = if Path::new(common).is_absolute() {
            PathBuf::from(common)
        } else {
            gitdir.join(common)
        };
        if let Some(path) = existing_dir_if_present(&path) {
            if path != gitdir {
                dirs.push(path);
            }
        }
    }
    dirs
}

fn git_helper_dirs(workspace: &Path) -> Vec<PathBuf> {
    let mut dirs = linked_git_dirs(workspace);
    dirs.sort_by_key(|path| path.as_os_str().len());
    let mut out = Vec::new();
    for dir in dirs {
        if dir == *workspace || dir.starts_with(workspace) {
            continue;
        }
        if out.iter().any(|have: &PathBuf| dir.starts_with(have)) {
            continue;
        }
        out.retain(|have| !have.starts_with(&dir));
        out.push(dir);
    }
    out
}

fn existing_dir_if_present(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path)
        .ok()
        .filter(|real| fs::metadata(real).is_ok_and(|meta| meta.is_dir()))
}

fn reject_overlapping_targets(grants: &[BindGrant]) -> Result<()> {
    let mut seen = HashSet::new();
    for (index, grant) in grants.iter().enumerate() {
        if !seen.insert(&grant.target)
            || grants[..index]
                .iter()
                .any(|other| overlaps(&grant.target, &other.target))
        {
            return Err(invalid("bind targets overlap or shadow each other"));
        }
    }
    Ok(())
}

fn reject_mixed_source_overlap(grants: &[BindGrant]) -> Result<()> {
    for (index, grant) in grants.iter().enumerate() {
        if grants[..index].iter().any(|other| {
            grant.access != other.access
                && overlaps(&grant.source, &other.source)
                && !narrow_cli_alias_within_workspace(grant, other)
        }) {
            return Err(invalid("read-only and writable grant sources overlap"));
        }
    }
    Ok(())
}

fn narrow_cli_alias_within_workspace(a: &BindGrant, b: &BindGrant) -> bool {
    let (read_only, writable) = if a.access == Access::ReadOnly {
        (a, b)
    } else {
        (b, a)
    };
    // A self-hosting development sleeve can build the CLI in its writable
    // workspace. The second bind exposes just that file on PATH; it does not
    // promise immutability while the same file is writable through the workspace.
    read_only.target == Path::new("/opt/protectorate/bin/protectorate")
        && read_only.source.is_file()
        && read_only.source.starts_with(&writable.source)
        && read_only.source != writable.source
}

fn reject_overlapping_overlay_targets(
    overlays: &[CacheOverlay],
    grants: &[BindGrant],
) -> Result<()> {
    for (index, overlay) in overlays.iter().enumerate() {
        if grants
            .iter()
            .any(|grant| !overlay_layer_allowed(&overlay.target, grant))
            || overlays[..index]
                .iter()
                .any(|other| overlaps(&overlay.target, &other.target))
        {
            return Err(invalid("overlay targets overlap other mounts"));
        }
    }
    Ok(())
}

// A later cache overlay may carve a writable cache from an earlier read-only
// tool bind. The launcher must mount binds before overlays in this order.
fn overlay_layer_allowed(target: &Path, grant: &BindGrant) -> bool {
    !overlaps(target, &grant.target)
        || (grant.access == Access::ReadOnly
            && target != grant.target
            && target.starts_with(&grant.target))
}

fn validate_mask(mask: FileMask, state: &Path, grants: &[BindGrant]) -> Result<FileMask> {
    clean_absolute(&mask.source)?;
    clean_absolute(&mask.target)?;
    let source = fs::canonicalize(&mask.source)?;
    let meta = fs::metadata(&source)?;
    if !source.starts_with(state.join("masks")) || !meta.is_file() || meta.len() != 0 {
        return Err(invalid(
            "mask source must be an empty regular file inside runtime mask state",
        ));
    }
    if grants
        .iter()
        .any(|grant| grant.access == Access::ReadWrite && mask.target.starts_with(&grant.target))
    {
        return Err(invalid("mask target overlaps a writable grant"));
    }
    if !grants
        .iter()
        .any(|grant| grant.access == Access::ReadOnly && mask.target.starts_with(&grant.target))
    {
        return Err(invalid("mask target must be inside a read-only grant"));
    }
    let mut ancestor = mask.target.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| invalid("invalid mask target"))?;
    }
    if fs::canonicalize(ancestor)? != ancestor {
        return Err(invalid("mask target has a symlink ancestor"));
    }
    if sensitive_system(&mask.target) {
        return Err(invalid("mask target touches sandbox infrastructure"));
    }
    Ok(FileMask {
        source,
        target: mask.target,
    })
}

/// Writes newline-terminated JSON lines to a child's stdin from a dedicated
/// thread, so no caller blocks on a full pipe while holding locks the child's
/// output reader needs. Unread input is capped; on a write error `failed` runs
/// once and the thread stops.
pub struct LineInput {
    lines: std::sync::mpsc::Sender<Vec<u8>>,
    unread: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    name: &'static str,
}

impl LineInput {
    pub const MAX_UNREAD: usize = 64 << 20;

    pub fn start(
        mut stdin: impl std::io::Write + Send + 'static,
        name: &'static str,
        failed: impl FnOnce(std::io::Error) + Send + 'static,
    ) -> Self {
        use std::sync::atomic::Ordering;
        let (lines, queue) = std::sync::mpsc::channel::<Vec<u8>>();
        let unread = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let written = std::sync::Arc::clone(&unread);
        std::thread::spawn(move || {
            for line in queue {
                if let Err(error) = stdin.write_all(&line) {
                    failed(error);
                    return;
                }
                written.fetch_sub(line.len(), Ordering::Relaxed);
            }
        });
        Self {
            lines,
            unread,
            name,
        }
    }

    pub fn write(&self, value: &serde_json::Value) -> std::result::Result<(), String> {
        use std::sync::atomic::Ordering;
        let mut line = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        line.push(b'\n');
        let len = line.len();
        if self.unread.fetch_add(len, Ordering::Relaxed) + len > Self::MAX_UNREAD {
            self.unread.fetch_sub(len, Ordering::Relaxed);
            return Err(format!("{} is not reading its input", self.name));
        }
        self.lines
            .send(line)
            .map_err(|_| format!("{} stdin is closed", self.name))
    }
}
