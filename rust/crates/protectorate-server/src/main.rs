//! Rust hub and node entry points.
#[cfg(target_os = "linux")]
mod driver;
#[cfg(unix)]
mod flutter_cache;
#[cfg(not(unix))]
mod flutter_cache {
    use std::{
        io,
        path::{Path, PathBuf},
    };

    pub fn publish(_state: &Path, _sdk: &Path, _max_bytes: u64) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Flutter cache publication requires Unix",
        ))
    }

    pub fn publish_tools(_state: &Path, _sdk: &Path, _max_bytes: u64) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Flutter tools publication requires Unix",
        ))
    }
    pub fn publish_pub(_state: &Path, _cache: &Path, _max_bytes: u64) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Pub cache publication requires Unix",
        ))
    }
    pub fn publish_go_mod(_state: &Path, _cache: &Path, _max_bytes: u64) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Go module cache publication requires Unix",
        ))
    }
}
mod node_doctor;
mod node_install;
mod node_join;
mod transport;
mod watchdog;
mod web;
use std::{
    env,
    error::Error,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

enum Command {
    Version,
    #[cfg(target_os = "linux")]
    Driver(driver::Command),
    Hub(Options),
    Node(Options),
    NodeUninstall(NodeUninstallCommand),
    NodeDoctor(node_doctor::Command),
    NodeJoin(node_join::Command),
    NodeInstall(node_install::Command),
    HubBackup(BackupCommand),
    HubRestore(RestoreCommand),
    HubPublishFlutterCache(PublishFlutterCacheCommand),
    HubPublishFlutterTools(PublishFlutterCacheCommand),
    HubPublishPubCache(PublishPubCacheCommand),
    HubPublishGoModCache(PublishPubCacheCommand),
    ApiCall(ApiCall),
}

fn build_version() -> &'static str {
    option_env!("PROTECTORATE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn default_codex_executable(user_home: &Path) -> PathBuf {
    let local = user_home.join(".local/bin/codex");
    if local.is_file() {
        return local;
    }
    let nvm_root = user_home.join(".nvm/versions/node");
    let nvm = fs::read_dir(nvm_root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let package = entry.path().join("lib/node_modules/@openai/codex");
            let executable = package.join("bin/codex.js");
            if !executable.is_file() {
                return None;
            }
            let manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(package.join("package.json")).ok()?).ok()?;
            let mut parts = manifest["version"].as_str()?.split('.');
            let version = [
                parts.next()?.parse::<u64>().ok()?,
                parts.next()?.parse::<u64>().ok()?,
                parts.next()?.parse::<u64>().ok()?,
            ];
            Some((version, executable))
        })
        .max_by_key(|(version, _)| *version);
    if let Some((_, executable)) = nvm {
        return executable;
    }
    if let Some(path) = env::var_os("PATH") {
        if let Some(executable) = env::split_paths(&path)
            .map(|directory| directory.join("codex"))
            .find(|candidate| candidate.is_file())
        {
            return executable;
        }
    }
    PathBuf::from("/usr/bin/codex")
}

fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}
struct Options {
    home: Option<PathBuf>,
    token_file: Option<PathBuf>,
    pi_executable: Option<PathBuf>,
    pi_tool_roots: Vec<PathBuf>,
    claude_executable: Option<PathBuf>,
    claude_tool_roots: Vec<PathBuf>,
    codex_executable: Option<PathBuf>,
    codex_tool_roots: Vec<PathBuf>,
    grok_executable: Option<PathBuf>,
    grok_tool_roots: Vec<PathBuf>,
    sleeve_effort: String,
    terminal_tool_roots: Vec<PathBuf>,
    flutter_sdk_root: Option<PathBuf>,
    flutter_cache_lower: Option<PathBuf>,
    flutter_tools_lower: Option<PathBuf>,
    pub_cache_lower: Option<PathBuf>,
    go_mod_cache_lower: Option<PathBuf>,
    workspace_roots: Vec<PathBuf>,
    reference_dirs: Vec<PathBuf>,
    msg_ack_timeout: Duration,
    dead_retention: Duration,
    node_interval: Duration,
    capability_ttl: Duration,
    device_interval: Duration,
    disk_interval: Duration,
    quota_interval: Duration,
    local_llm_urls: Vec<String>,
    local_model_hosts: Vec<String>,
    egress_allow: Vec<String>,
    trusted_proxies: Vec<std::net::IpAddr>,
    public_url: Option<String>,
    hub_watch_paths: Vec<PathBuf>,
    tmux_socket: String,
    orchestrator_admin_grants: bool,
    release_repo: String,
    release_channel: String,
    release_api_url: String,
    release_dir: Option<PathBuf>,
    web_dir: Option<PathBuf>,
    node_binary_dir: Option<PathBuf>,
    listen: Option<SocketAddr>,
    tls_listen: Vec<SocketAddr>,
    tls_names: Vec<String>,
    file_root: Option<PathBuf>,
    file_unconfined: bool,
    job_concurrency: usize,
    allow_plain_jobs: bool,
    sandbox_helper: Option<PathBuf>,
    watch_paths: Vec<PathBuf>,
    docker_host: Option<String>,
    backup_dir: Option<PathBuf>,
    backup_every: Duration,
}
struct BackupCommand {
    home: PathBuf,
    to: Option<PathBuf>,
}
struct RestoreCommand {
    archive: PathBuf,
    home: PathBuf,
}
struct PublishFlutterCacheCommand {
    home: PathBuf,
    sdk: PathBuf,
    max_bytes: u64,
}
struct PublishPubCacheCommand {
    home: PathBuf,
    source: PathBuf,
    max_bytes: u64,
}
struct ApiCall {
    hub: String,
    token_file: Option<PathBuf>,
    procedure: String,
    input: serde_json::Value,
    compact: bool,
}
struct NodeUninstallCommand {
    home: PathBuf,
    dry_run: bool,
}

fn usage() -> &'static str {
    "Usage:\n  protectorate-rs version --json\n  protectorate-rs serve --home DIR --owner-token-file FILE [--listen 127.0.0.1:7470] [--tls-listen IP:PORT]... [--tls-name HOST]... [--public-url URL] [--web DIR] [--workspace-root DIR] [--reference-dir DIR] [--sleeve-effort high] [--msg-ack-timeout 30m] [--dead-retention 24h|0] [--node-interval 30s] [--capability-ttl 1h] [--device-interval 5s] [--disk-interval 10s] [--quota-interval 5m] [--local-llm-url URL] [--local-model-host HOST] [--egress-allow HOST[:PORT]] [--trusted-proxy IP] [--watch DIR] [--tmux-socket NAME] [--orchestrator-admin-grants] [--release-repo OWNER/REPO] [--update-channel stable|dev] [--release-api-url URL] [--release-dir DIR] [--node-binary-dir DIR] [--pi-executable FILE] [--pi-tool-root DIR] [--claude-executable FILE] [--claude-tool-root DIR] [--codex-executable FILE] [--codex-tool-root DIR] [--grok-executable FILE] [--grok-tool-root DIR] [--terminal-tool-root DIR] [--flutter-sdk-root DIR --flutter-cache-lower DIR] [--flutter-tools-lower DIR] [--pub-cache-lower DIR] [--go-mod-cache-lower DIR] [--backup-dir DIR] [--backup-every 6h]\n  protectorate-rs hub backup [--home DIR] [--to DIR]\n  protectorate-rs hub restore FILE --home DIR\n  protectorate-rs hub publish-flutter-cache --home DIR --flutter-sdk-root DIR [--max-bytes 3221225472]\n  protectorate-rs hub publish-flutter-tools --home DIR --flutter-sdk-root DIR [--max-bytes 4194304]\n  protectorate-rs hub publish-pub-cache --home DIR --pub-cache-root DIR [--max-bytes 3221225472]\n  protectorate-rs hub publish-go-mod-cache --home DIR --go-mod-cache-root DIR [--max-bytes 3221225472]\n  protectorate-rs node join --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR]\n  protectorate-rs node install --hub URL --ca-cert FILE [--code CODE | --code-file FILE] [--name NAME] [--listen IP:PORT] [--home DIR] [--allow-plain-jobs] [--dry-run] [--no-start] [-- serve-flags]\n  protectorate-rs node uninstall [--home DIR] [--dry-run]\n  protectorate-rs node doctor [--home DIR] [--json]\n  protectorate-rs node serve [--home DIR] [--listen IP:PORT] [--file-root DIR] [--file-unconfined] [--job-concurrency 2] [--sandbox-helper FILE] [--allow-plain-jobs] [--egress-allow HOST[:PORT]] [--watch-dir DIR] [--docker-host URL]\n  protectorate-rs api call Service/Method [JSON] [--hub URL] [--token-file FILE] [--json]\n\nListeners bind explicit private addresses only: loopback, RFC 1918, 100.64.0.0/10, fc00::/7 or fe80::/10. The hub serves plain HTTP on its loopback --listen and HTTPS with its own CA (HOME/ca/ca.pem) on each --tls-listen; --tls-name adds a DNS name to its certificate. A node joins with an invite code (PROTECTORATE_JOIN_CODE) and serves only mutual TLS, to the hub, on the address it joined with.\n"
}

fn default_home() -> Result<PathBuf, String> {
    if let Some(home) = env::var_os("PROTECTORATE_HOME") {
        return Ok(PathBuf::from(home));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".protectorate"))
        .ok_or_else(|| "HOME is unset; pass --home".into())
}

fn parse_interval(value: &str) -> Result<Duration, String> {
    let (number, multiplier) = match value.chars().last() {
        Some('s') => (&value[..value.len() - 1], 1),
        Some('m') => (&value[..value.len() - 1], 60),
        Some('h') => (&value[..value.len() - 1], 3600),
        Some('d') => (&value[..value.len() - 1], 86400),
        _ => {
            return Err("duration must be positive and use s, m, h, or d (for example 30s)".into())
        }
    };
    let amount = number
        .parse::<u64>()
        .map_err(|_| "duration must be positive and use s, m, h, or d (for example 30s)")?;
    let seconds = amount
        .checked_mul(multiplier)
        .filter(|seconds| *seconds > 0)
        .ok_or("duration is out of range")?;
    Ok(Duration::from_secs(seconds))
}

fn parse_dead_retention(value: &str) -> Result<Duration, String> {
    if matches!(value, "0" | "0s" | "0m" | "0h" | "0d") {
        Ok(Duration::ZERO)
    } else {
        parse_interval(value)
    }
}

fn parse_hub_command(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    match args.next().as_deref() {
        Some("backup") => {
            let mut home = None;
            let mut to = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--home" => {
                        home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                    }
                    "--to" => to = Some(PathBuf::from(args.next().ok_or("--to requires DIR")?)),
                    _ => return Err(format!("unknown backup argument {flag:?}")),
                }
            }
            Ok(Command::HubBackup(BackupCommand {
                home: home.map_or_else(default_home, Ok)?,
                to,
            }))
        }
        Some("restore") => {
            let archive = PathBuf::from(args.next().ok_or("restore requires FILE")?);
            let mut home = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--home" => {
                        home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                    }
                    _ => return Err(format!("unknown restore argument {flag:?}")),
                }
            }
            Ok(Command::HubRestore(RestoreCommand {
                archive,
                home: home.ok_or("restore requires --home DIR")?,
            }))
        }
        Some(action @ ("publish-flutter-cache" | "publish-flutter-tools")) => {
            let mut home = None;
            let mut sdk = None;
            let mut max_bytes = if action == "publish-flutter-tools" {
                4 * 1024 * 1024u64
            } else {
                3 * 1024 * 1024 * 1024u64
            };
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--home" => {
                        home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                    }
                    "--flutter-sdk-root" => {
                        sdk = Some(PathBuf::from(
                            args.next().ok_or("--flutter-sdk-root requires DIR")?,
                        ))
                    }
                    "--max-bytes" => {
                        max_bytes = args
                            .next()
                            .ok_or("--max-bytes requires integer")?
                            .parse()
                            .map_err(|_| "--max-bytes requires integer")?
                    }
                    _ => return Err(format!("unknown Flutter cache argument {flag:?}")),
                }
            }
            let command = PublishFlutterCacheCommand {
                home: home.map_or_else(default_home, Ok)?,
                sdk: sdk.ok_or("Flutter cache publication requires --flutter-sdk-root DIR")?,
                max_bytes,
            };
            Ok(if action == "publish-flutter-tools" {
                Command::HubPublishFlutterTools(command)
            } else {
                Command::HubPublishFlutterCache(command)
            })
        }
        Some(action @ ("publish-pub-cache" | "publish-go-mod-cache")) => {
            let mut home = None;
            let mut source = None;
            let mut max_bytes = 3 * 1024 * 1024 * 1024u64;
            let source_flag = if action == "publish-pub-cache" {
                "--pub-cache-root"
            } else {
                "--go-mod-cache-root"
            };
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--home" => {
                        home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                    }
                    value if value == source_flag => {
                        source = Some(PathBuf::from(args.next().ok_or("cache root requires DIR")?))
                    }
                    "--max-bytes" => {
                        max_bytes = args
                            .next()
                            .ok_or("--max-bytes requires integer")?
                            .parse()
                            .map_err(|_| "--max-bytes requires integer")?
                    }
                    _ => return Err(format!("unknown Pub cache argument {flag:?}")),
                }
            }
            let command = PublishPubCacheCommand {
                home: home.map_or_else(default_home, Ok)?,
                source: source.ok_or("cache publication requires a source root")?,
                max_bytes,
            };
            Ok(if action == "publish-pub-cache" {
                Command::HubPublishPubCache(command)
            } else {
                Command::HubPublishGoModCache(command)
            })
        }
        _ => Err(usage().into()),
    }
}

fn parse(args: impl Iterator<Item = String>) -> Result<Option<Command>, String> {
    let mut args = args.peekable();
    let node = match args.next().as_deref() {
        Some("--help" | "-h") | None => return Ok(None),
        Some("serve") => false,
        Some("version") => {
            if args.next().as_deref() != Some("--json") || args.next().is_some() {
                return Err("version requires --json".into());
            }
            return Ok(Some(Command::Version));
        }
        Some("hub") => return parse_hub_command(args).map(Some),
        #[cfg(target_os = "linux")]
        Some("driver") => return driver::parse(args).map(|command| Some(Command::Driver(command))),
        Some("api") => return parse_api(args).map(|a| Some(Command::ApiCall(a))),
        Some("node") => {
            match args.next().as_deref() {
                Some("serve") => {}
                Some("join") => {
                    return node_join::parse(args).map(|command| Some(Command::NodeJoin(command)));
                }
                Some("install") => {
                    return node_install::parse(args)
                        .map(|command| Some(Command::NodeInstall(command)));
                }
                Some("doctor") => {
                    return node_doctor::parse(args)
                        .map(|command| Some(Command::NodeDoctor(command)));
                }
                Some("uninstall") => {
                    let mut home = None;
                    let mut dry_run = false;
                    while let Some(flag) = args.next() {
                        match flag.as_str() {
                            "--home" => {
                                home =
                                    Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                            }
                            "--dry-run" => dry_run = true,
                            _ => return Err(format!("unknown node uninstall argument {flag:?}")),
                        }
                    }
                    return Ok(Some(Command::NodeUninstall(NodeUninstallCommand {
                        home: home.map_or_else(default_home, Ok)?,
                        dry_run,
                    })));
                }
                _ => return Err(usage().into()),
            }
            true
        }
        _ => return Err(usage().into()),
    };
    let mut home = None;
    let mut token_file = None;
    let mut pi_executable = None;
    let mut pi_tool_roots = Vec::new();
    let mut claude_executable = None;
    let mut claude_tool_roots = Vec::new();
    let mut codex_executable = None;
    let mut codex_tool_roots = Vec::new();
    let mut grok_executable = None;
    let mut grok_tool_roots = Vec::new();
    let mut terminal_tool_roots = Vec::new();
    let mut sleeve_effort = "high".to_owned();
    let mut flutter_sdk_root = None;
    let mut flutter_cache_lower = None;
    let mut flutter_tools_lower = None;
    let mut pub_cache_lower = None;
    let mut go_mod_cache_lower = None;
    let mut workspace_roots = Vec::new();
    let mut reference_dirs = Vec::new();
    let mut msg_ack_timeout = Duration::from_secs(30 * 60);
    let mut dead_retention = Duration::from_secs(24 * 3600);
    let mut node_interval = Duration::from_secs(30);
    let mut capability_ttl = Duration::from_secs(60 * 60);
    let mut device_interval = Duration::from_secs(5);
    let mut disk_interval = Duration::from_secs(10);
    let mut quota_interval = Duration::from_secs(5 * 60);
    let mut local_llm_urls = Vec::new();
    let mut local_model_hosts = Vec::new();
    let mut egress_allow = Vec::new();
    let mut trusted_proxies = Vec::new();
    let mut public_url = None;
    let mut hub_watch_paths = Vec::new();
    let mut tmux_socket =
        env::var("PROTECTORATE_TMUX_SOCKET").unwrap_or_else(|_| "protectorate-rs".into());
    let mut orchestrator_admin_grants = false;
    let mut release_repo = "hotschmoe/protectorate".to_owned();
    let mut release_channel = "stable".to_owned();
    let mut release_api_url = String::new();
    let mut release_dir = None;
    let mut web_dir = None;
    let mut node_binary_dir = None;
    let mut file_root = None;
    let mut file_unconfined = false;
    let mut job_concurrency = 2;
    let mut allow_plain_jobs = false;
    let mut sandbox_helper = None;
    let mut watch_paths = Vec::new();
    let mut docker_host = None;
    let mut backup_dir = None;
    let mut backup_every = Duration::from_secs(6 * 3600);
    let mut backup_every_set = false;
    let mut listen = None;
    let mut tls_listen = Vec::new();
    let mut tls_names = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--owner-token-file" if !node => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or("--owner-token-file requires FILE")?,
                ))
            }
            "--workspace-root" if !node => {
                workspace_roots.push(PathBuf::from(
                    args.next().ok_or("--workspace-root requires DIR")?,
                ));
            }
            "--reference-dir" if !node => {
                reference_dirs.push(PathBuf::from(
                    args.next().ok_or("--reference-dir requires DIR")?,
                ));
            }
            "--msg-ack-timeout" if !node => {
                msg_ack_timeout =
                    parse_interval(&args.next().ok_or("--msg-ack-timeout requires DURATION")?)
                        .map_err(|_| {
                            "--msg-ack-timeout requires a positive duration such as 30m"
                        })?;
            }
            "--dead-retention" if !node => {
                dead_retention = parse_dead_retention(
                    &args.next().ok_or("--dead-retention requires DURATION")?,
                )?;
            }
            "--sleeve-effort" if !node => {
                sleeve_effort = args.next().ok_or("--sleeve-effort requires VALUE")?;
            }
            "--node-interval" if !node => {
                node_interval =
                    parse_interval(&args.next().ok_or("--node-interval requires DURATION")?)?;
            }
            "--capability-ttl" if !node => {
                capability_ttl =
                    parse_interval(&args.next().ok_or("--capability-ttl requires DURATION")?)?;
            }
            "--device-interval" if !node => {
                device_interval =
                    parse_interval(&args.next().ok_or("--device-interval requires DURATION")?)?;
            }
            "--disk-interval" if !node => {
                disk_interval =
                    parse_interval(&args.next().ok_or("--disk-interval requires DURATION")?)?;
            }
            "--quota-interval" if !node => {
                quota_interval =
                    parse_interval(&args.next().ok_or("--quota-interval requires DURATION")?)?;
            }
            "--local-llm-url" if !node => {
                let url = args.next().ok_or("--local-llm-url requires URL")?;
                let uri: axum::http::Uri = url
                    .parse()
                    .map_err(|_| "--local-llm-url requires an HTTP(S) base URL")?;
                if !matches!(uri.scheme_str(), Some("http" | "https"))
                    || uri.host().is_none()
                    || uri.query().is_some()
                    || uri.authority().is_some_and(|a| a.as_str().contains('@'))
                    || url.len() > 2048
                {
                    return Err(
                        "--local-llm-url requires an HTTP(S) base URL without credentials or query"
                            .into(),
                    );
                }
                local_llm_urls.push(url);
            }
            "--local-model-host" if !node => {
                let host = args.next().ok_or("--local-model-host requires HOST")?;
                if host.is_empty() || host.len() > 253 || host.contains('/') || host.contains(' ') {
                    return Err("--local-model-host requires a hostname or IP address".into());
                }
                local_model_hosts.push(host);
            }
            "--egress-allow" => {
                egress_allow.push(args.next().ok_or("--egress-allow requires HOST[:PORT]")?);
            }
            "--trusted-proxy" if !node => {
                let address = args.next().ok_or("--trusted-proxy requires IP address")?;
                trusted_proxies.push(
                    address
                        .parse::<std::net::IpAddr>()
                        .map_err(|_| "--trusted-proxy requires an IP address")?,
                );
            }
            "--public-url" if !node => {
                public_url = Some(args.next().ok_or("--public-url requires URL")?);
            }
            "--watch" if !node => {
                hub_watch_paths.push(PathBuf::from(args.next().ok_or("--watch requires DIR")?));
            }
            "--tmux-socket" if !node => {
                tmux_socket = args.next().ok_or("--tmux-socket requires NAME")?;
                if tmux_socket.is_empty() || tmux_socket.contains('/') || tmux_socket.contains('\\')
                {
                    return Err("--tmux-socket requires a nonempty tmux socket name".into());
                }
            }
            "--orchestrator-admin-grants" if !node => orchestrator_admin_grants = true,
            "--release-repo" if !node => {
                release_repo = args.next().ok_or("--release-repo requires OWNER/REPO")?;
            }
            "--update-channel" if !node => {
                release_channel = args
                    .next()
                    .ok_or("--update-channel requires stable or dev")?;
            }
            "--release-api-url" if !node => {
                release_api_url = args.next().ok_or("--release-api-url requires URL")?;
            }
            "--release-dir" if !node => {
                release_dir = Some(PathBuf::from(
                    args.next().ok_or("--release-dir requires DIR")?,
                ));
            }
            "--web" if !node => {
                web_dir = Some(PathBuf::from(args.next().ok_or("--web requires DIR")?));
            }
            "--node-binary-dir" if !node => {
                node_binary_dir = Some(PathBuf::from(
                    args.next().ok_or("--node-binary-dir requires DIR")?,
                ));
            }
            "--pi-executable" if !node => {
                pi_executable = Some(PathBuf::from(
                    args.next().ok_or("--pi-executable requires FILE")?,
                ));
            }
            "--pi-tool-root" if !node => {
                pi_tool_roots.push(PathBuf::from(
                    args.next().ok_or("--pi-tool-root requires DIR")?,
                ));
            }
            "--claude-executable" if !node => {
                claude_executable = Some(PathBuf::from(
                    args.next().ok_or("--claude-executable requires FILE")?,
                ));
            }
            "--claude-tool-root" if !node => {
                claude_tool_roots.push(PathBuf::from(
                    args.next().ok_or("--claude-tool-root requires DIR")?,
                ));
            }
            "--codex-executable" if !node => {
                codex_executable = Some(PathBuf::from(
                    args.next().ok_or("--codex-executable requires FILE")?,
                ));
            }
            "--codex-tool-root" if !node => {
                codex_tool_roots.push(PathBuf::from(
                    args.next().ok_or("--codex-tool-root requires DIR")?,
                ));
            }
            "--grok-executable" if !node => {
                grok_executable = Some(PathBuf::from(
                    args.next().ok_or("--grok-executable requires FILE")?,
                ));
            }
            "--grok-tool-root" if !node => {
                grok_tool_roots.push(PathBuf::from(
                    args.next().ok_or("--grok-tool-root requires DIR")?,
                ));
            }
            "--terminal-tool-root" if !node => {
                terminal_tool_roots.push(PathBuf::from(
                    args.next().ok_or("--terminal-tool-root requires DIR")?,
                ));
            }
            "--flutter-sdk-root" if !node => {
                flutter_sdk_root = Some(PathBuf::from(
                    args.next().ok_or("--flutter-sdk-root requires DIR")?,
                ));
            }
            "--flutter-cache-lower" if !node => {
                flutter_cache_lower = Some(PathBuf::from(
                    args.next().ok_or("--flutter-cache-lower requires DIR")?,
                ));
            }
            "--flutter-tools-lower" if !node => {
                flutter_tools_lower = Some(PathBuf::from(
                    args.next().ok_or("--flutter-tools-lower requires DIR")?,
                ));
            }
            "--pub-cache-lower" if !node => {
                pub_cache_lower = Some(PathBuf::from(
                    args.next().ok_or("--pub-cache-lower requires DIR")?,
                ));
            }
            "--go-mod-cache-lower" if !node => {
                go_mod_cache_lower = Some(PathBuf::from(
                    args.next().ok_or("--go-mod-cache-lower requires DIR")?,
                ));
            }
            "--listen" => {
                listen = Some(
                    args.next()
                        .ok_or("--listen requires IP:PORT")?
                        .parse()
                        .map_err(|_| "--listen requires IP:PORT")?,
                )
            }
            "--tls-listen" if !node => tls_listen.push(
                args.next()
                    .ok_or("--tls-listen requires IP:PORT")?
                    .parse()
                    .map_err(|_| "--tls-listen requires IP:PORT")?,
            ),
            "--tls-name" if !node => {
                let name = args.next().ok_or("--tls-name requires HOST")?;
                if name.is_empty()
                    || name.len() > 253
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
                {
                    return Err("--tls-name requires a DNS name".into());
                }
                tls_names.push(name);
            }
            "--file-root" if node => {
                file_root = Some(PathBuf::from(
                    args.next().ok_or("--file-root requires DIR")?,
                ))
            }
            "--file-unconfined" if node => file_unconfined = true,
            "--job-concurrency" if node => {
                job_concurrency = args
                    .next()
                    .ok_or("--job-concurrency requires a number")?
                    .parse::<usize>()
                    .map_err(|_| "--job-concurrency requires a number")?;
                if !(1..=32).contains(&job_concurrency) {
                    return Err("--job-concurrency must be between 1 and 32".into());
                }
            }
            "--allow-plain-jobs" if node => allow_plain_jobs = true,
            "--sandbox-helper" if node => {
                sandbox_helper = Some(PathBuf::from(
                    args.next().ok_or("--sandbox-helper requires FILE")?,
                ))
            }
            "--watch-dir" if node => {
                watch_paths.push(PathBuf::from(
                    args.next().ok_or("--watch-dir requires DIR")?,
                ));
            }
            "--docker-host" if node => {
                docker_host = Some(args.next().ok_or("--docker-host requires URL")?);
            }
            "--backup-dir" if !node => {
                backup_dir = Some(PathBuf::from(
                    args.next().ok_or("--backup-dir requires DIR")?,
                ))
            }
            "--backup-every" if !node => {
                backup_every =
                    parse_interval(&args.next().ok_or("--backup-every requires DURATION")?)?;
                backup_every_set = true;
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err(format!("unknown argument {flag:?}\n{}", usage())),
        }
    }
    for address in listen.iter().chain(&tls_listen) {
        protectorate_tls::check_listen(*address).map_err(|error| error.to_string())?;
    }
    if !node && listen.is_some_and(|address| !address.ip().is_loopback()) {
        return Err("--listen is the hub's plain HTTP listener and must be loopback; use --tls-listen for other private addresses".into());
    }
    if !node && home.is_none() {
        return Err("--home is required".into());
    }
    if !node {
        protectorate_release::validate_repo(&release_repo).map_err(|e| e.to_string())?;
        protectorate_release::validate_channel(&release_channel).map_err(|e| e.to_string())?;
        if !release_api_url.is_empty() {
            protectorate_release::validate_api(&release_api_url).map_err(|e| e.to_string())?;
        }
    }
    if backup_every_set && backup_dir.is_none() {
        return Err("--backup-every requires --backup-dir".into());
    }
    if !node && token_file.is_none() {
        return Err("--owner-token-file is required".into());
    }
    let options = Options {
        home,
        token_file,
        pi_executable,
        pi_tool_roots,
        claude_executable,
        claude_tool_roots,
        codex_executable,
        codex_tool_roots,
        grok_executable,
        grok_tool_roots,
        sleeve_effort,
        terminal_tool_roots,
        flutter_sdk_root,
        flutter_cache_lower,
        flutter_tools_lower,
        pub_cache_lower,
        go_mod_cache_lower,
        workspace_roots,
        reference_dirs,
        msg_ack_timeout,
        dead_retention,
        node_interval,
        capability_ttl,
        device_interval,
        disk_interval,
        quota_interval,
        local_llm_urls,
        local_model_hosts,
        egress_allow,
        trusted_proxies,
        public_url,
        hub_watch_paths,
        tmux_socket,
        orchestrator_admin_grants,
        release_repo,
        release_channel,
        release_api_url,
        release_dir,
        web_dir,
        node_binary_dir,
        listen,
        tls_listen,
        tls_names,
        file_root,
        file_unconfined,
        job_concurrency,
        allow_plain_jobs,
        sandbox_helper,
        watch_paths,
        docker_host,
        backup_dir,
        backup_every,
    };
    Ok(Some(if node {
        Command::Node(options)
    } else {
        Command::Hub(options)
    }))
}

fn parse_api(args: impl Iterator<Item = String>) -> Result<ApiCall, String> {
    let mut args = args.peekable();
    if args.next().as_deref() != Some("call") {
        return Err(usage().into());
    }
    let method = args.next().ok_or("api call requires Service/Method")?;
    let (service, method_name) = method
        .split_once('/')
        .ok_or("method must look like Service/Method")?;
    if service.is_empty() || method_name.is_empty() || method_name.contains('/') {
        return Err("method must look like Service/Method".into());
    }
    let service = if service.contains('.') {
        service.to_owned()
    } else {
        format!("protectorate.v1.{service}")
    };
    let procedure = format!("/{service}/{method_name}");
    let mut body = None;
    let mut hub = env::var("PROTECTORATE_HUB").unwrap_or_else(|_| "http://127.0.0.1:7470".into());
    let mut token_file = env::var_os("PROTECTORATE_TOKEN_FILE").map(PathBuf::from);
    let mut compact = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hub" => hub = args.next().ok_or("--hub requires URL")?,
            "--token-file" => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or("--token-file requires FILE")?,
                ))
            }
            "--json" => compact = true,
            _ if arg.starts_with('-') => return Err(format!("unknown argument {arg:?}")),
            _ if body.is_none() => body = Some(arg),
            _ => return Err("api call accepts at most one JSON argument".into()),
        }
    }
    let input = serde_json::from_str(body.as_deref().unwrap_or("{}"))
        .map_err(|e| format!("invalid request JSON: {e}"))?;
    Ok(ApiCall {
        hub,
        token_file,
        procedure,
        input,
        compact,
    })
}

fn read_token(path: &PathBuf) -> Result<String, Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err("token file must be readable only by its owner".into());
        }
    }
    let token = fs::read_to_string(path)?;
    let token = token.trim_end_matches(['\r', '\n']).to_owned();
    if token.is_empty() {
        return Err("token file is empty".into());
    }
    Ok(token)
}

fn api_call(options: ApiCall) -> Result<(), Box<dyn Error>> {
    let token = match options.token_file {
        Some(path) => read_token(&path)?,
        None => env::var("PROTECTORATE_TOKEN").map_err(|_| {
            "api call requires --token-file, PROTECTORATE_TOKEN_FILE, or PROTECTORATE_TOKEN"
        })?,
    };
    let client =
        protectorate_connect_client::Client::new(&options.hub, token, Duration::from_secs(30))?;
    let output = client.call(&options.procedure, &options.input)?;
    if options.compact {
        println!("{}", serde_json::to_string(&output)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&output)?);
    }
    Ok(())
}

fn node_uninstall_with(
    options: NodeUninstallCommand,
    systemctl: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    let home = std::path::absolute(options.home)?;
    let user_home = std::path::absolute(PathBuf::from(
        env::var_os("HOME").ok_or("HOME is unset; cannot plan node uninstall")?,
    ))?;
    let plan =
        protectorate_node::UninstallPlan::new(home, user_home, std::env::current_exe()?, false)?;
    if options.dry_run {
        println!("dry run: nothing will be changed");
    }
    for action in plan.actions() {
        println!("- {action}");
    }
    if !options.dry_run {
        if let Err(error) = plan.run(systemctl) {
            eprintln!("warning: {error}");
        }
    }
    println!("Nothing was deleted. You may remove:");
    for path in plan.left() {
        println!("- {path}");
    }
    Ok(())
}

fn hub_backup(options: BackupCommand) -> Result<(), Box<dyn Error>> {
    let dir = options.to.unwrap_or_else(|| options.home.join("backups"));
    let archive = protectorate_backup::create(
        &protectorate_backup::Options {
            home: options.home,
            dir,
            version: build_version().into(),
            created_at: None,
        },
        &protectorate_backup::Cancellation::default(),
    )?;
    println!("{}", archive.display());
    let sidecar = PathBuf::from(format!("{}.sha256", archive.display()));
    let checksum = fs::read_to_string(sidecar)?;
    let checksum = checksum
        .split_whitespace()
        .next()
        .ok_or("backup checksum sidecar is empty")?;
    println!("sha256 {checksum}");
    Ok(())
}

fn hub_restore(options: RestoreCommand) -> Result<(), Box<dyn Error>> {
    let manifest = protectorate_backup::restore(
        &options.archive,
        &options.home,
        &protectorate_backup::Cancellation::default(),
    )?;
    println!(
        "restored {} files into {} (backup of {} made {} by hub {})",
        manifest.files.len(),
        options.home.display(),
        manifest.home.display(),
        manifest.created,
        manifest.hub_version
    );
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

async fn serve_hub(options: Options) -> Result<bool, Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(options.home.as_ref().expect("hub home checked by parser"))?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(options.home.as_ref().expect("hub home checked by parser"))?;
    let home = fs::canonicalize(options.home.as_ref().expect("hub home checked by parser"))?;
    let user_home = fs::canonicalize(PathBuf::from(
        env::var_os("HOME").ok_or("HOME is unset; cannot plan terminal sleeves")?,
    ))?;
    if home == user_home {
        return Err("hub state home must differ from the user's home for terminal sleeves".into());
    }
    let executable_dir = env::current_exe()?
        .parent()
        .ok_or("server executable has no directory")?
        .to_path_buf();
    let companion = |name: &str| {
        let local = executable_dir.join(name);
        if local.is_file() {
            local
        } else {
            executable_dir.join(format!("{name}-{}-{}", std::env::consts::OS, host_arch()))
        }
    };
    let runner = companion("protectorate-sleeve-runner");
    let helper = companion("protectorate-sleeve-sandbox-helper");
    let cli = companion("protectorate");
    let cli = cli.is_file().then(|| fs::canonicalize(cli)).transpose()?;
    if !runner.is_file() || !helper.is_file() {
        return Err(format!("terminal sleeve companions are missing; install protectorate-sleeve-runner and protectorate-sleeve-sandbox-helper beside protectorate-rs in {}", executable_dir.display()).into());
    }
    let runtime_dir = home.join("runtime");
    fs::create_dir_all(&runtime_dir)?;
    let backup_options = options.backup_dir.map(|dir| protectorate_backup::Options {
        home: home.clone(),
        dir,
        version: build_version().into(),
        created_at: None,
    });
    let backup_every = options.backup_every;
    let token = read_token(
        options
            .token_file
            .as_ref()
            .expect("hub token checked by parser"),
    )?;
    let ca = protectorate_tls::Ca::open_or_create(&home.join("ca"))?;
    let listener = tokio::net::TcpListener::bind(hub_listen(options.listen)).await?;
    let mut tls_listeners = Vec::new();
    for address in &options.tls_listen {
        tls_listeners.push(tokio::net::TcpListener::bind(address).await?);
    }
    let mut names = Vec::new();
    for listener in &tls_listeners {
        names.push(listener.local_addr()?.ip().to_string());
    }
    names.extend(options.tls_names.iter().cloned());
    let identity = ca.hub_identity(&names)?;
    let tls = protectorate_tls::hub_server_config(&identity, ca.pem().as_bytes())?;
    let node_tls = protectorate_connect_client::NodeTls::from_pem(
        ca.pem().as_bytes(),
        identity.cert_pem.as_bytes(),
        identity.key_pem.as_bytes(),
    )?;
    let hub_url = format!("http://{}", listener.local_addr()?);
    let public_url = match (&options.public_url, tls_listeners.first()) {
        (Some(url), _) => Some(url.clone()),
        (None, Some(listener)) => Some(format!("https://{}", listener.local_addr()?)),
        (None, None) => None,
    };
    if options.flutter_sdk_root.is_some() != options.flutter_cache_lower.is_some() {
        return Err("--flutter-sdk-root and --flutter-cache-lower must be given together".into());
    }
    if options.flutter_tools_lower.is_some() && options.flutter_sdk_root.is_none() {
        return Err("--flutter-tools-lower requires --flutter-sdk-root".into());
    }
    let mut terminal_tool_roots = select_terminal_tool_roots(
        &user_home,
        &options.terminal_tool_roots,
        options.flutter_sdk_root.as_deref(),
    );
    omit_pub_roots_covered_by_overlay(
        &mut terminal_tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    let mut hub = protectorate_hub::Hub::open(home.clone(), token, build_version())?;
    let monitor = protectorate_node_monitor::Monitor::new_with_watch_paths(
        Arc::clone(&hub.inventory),
        hub.hostname.clone(),
        home.clone(),
        options.hub_watch_paths.clone(),
    )
    .with_timing(node_timing(options.node_interval));
    hub = hub.with_node_monitor(Arc::new(monitor))?;
    hub = hub.with_quota_sources(options.quota_interval, &options.local_llm_urls);
    hub = hub.with_local_model_hosts(options.local_model_hosts.clone());
    hub = hub.with_egress_allow(options.egress_allow.clone());
    hub = hub.with_sleeve_effort(options.sleeve_effort.clone());
    hub = hub.with_dead_retention(options.dead_retention);
    hub = hub.with_trusted_proxies(options.trusted_proxies.clone());
    if let Some(url) = public_url {
        hub = hub.with_public_url(url)?;
    }
    hub = hub
        .with_ca(Arc::new(ca))
        .with_node_tls(node_tls)
        .with_tls_device_auth();
    hub = hub
        .with_sessions(Arc::new(protectorate_session::Manager::new(
            &options.tmux_socket,
        )))
        .with_terminal_launcher(protectorate_hub::TerminalLauncher {
            state_root: home.clone(),
            user_home: user_home.clone(),
            runtime_dir: runtime_dir.clone(),
            runner,
            helper: helper.clone(),
            hub_url: hub_url.clone(),
            cli_executable: cli,
            tasks_max: 4096,
            tool_roots: terminal_tool_roots,
            flutter_sdk_root: options.flutter_sdk_root.clone(),
            flutter_cache_lower: options.flutter_cache_lower.clone(),
            flutter_tools_lower: options.flutter_tools_lower.clone(),
            pub_cache_lower: options.pub_cache_lower.clone(),
            go_mod_cache_lower: options.go_mod_cache_lower.clone(),
        })
        .with_msg_ack_timeout(options.msg_ack_timeout)
        .with_capability_ttl(options.capability_ttl)
        .with_device_interval(options.device_interval)
        .with_disk_interval(options.disk_interval);
    if options.orchestrator_admin_grants {
        hub = hub.with_orchestrator_admin_grants();
    }
    let release_dir = options
        .release_dir
        .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    let release_source = protectorate_release::Source::new(
        &options.release_repo,
        &options.release_channel,
        &options.release_api_url,
        &release_dir,
    )?;
    let web = web::Web::new(
        options.web_dir.as_deref(),
        release_source.current_dir().join("web"),
    )?;
    let hub_restart = Arc::new(AtomicBool::new(false));
    let hub_restart_notify = Arc::new(tokio::sync::Notify::new());
    let restart_flag = Arc::clone(&hub_restart);
    let restart_wake = Arc::clone(&hub_restart_notify);
    hub = hub
        .with_release_source(Arc::new(release_source))
        .with_release_restart(Arc::new(move || {
            restart_flag.store(true, Ordering::SeqCst);
            restart_wake.notify_one();
        }));
    if let Some(binary_dir) = options.node_binary_dir {
        hub = hub.with_node_binary_dir(binary_dir);
    }
    let browse_root = user_home.join("github");
    let mut workspace_roots = options.workspace_roots;
    if browse_root.is_dir() {
        workspace_roots.push(browse_root);
    }
    hub = hub.with_workspace_roots(workspace_roots)?;
    hub = hub.with_reference_dirs(options.reference_dirs)?;
    let explicit_pi = options.pi_executable.is_some();
    let pi_executable = options
        .pi_executable
        .unwrap_or_else(|| user_home.join(".local/bin/pi"));
    let pi_executable = if pi_executable.is_file() {
        Some(fs::canonicalize(pi_executable)?)
    } else if explicit_pi {
        return Err(format!("Pi executable {} does not exist", pi_executable.display()).into());
    } else {
        None
    };
    let mut default_chat_tool_roots = protectorate_sleeve_runner::default_tool_roots(&user_home);
    omit_pub_roots_covered_by_overlay(
        &mut default_chat_tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    let mut tool_roots = options.pi_tool_roots;
    tool_roots.extend(default_chat_tool_roots.iter().cloned());
    if let Some(parent) = pi_executable.as_ref().and_then(|path| path.parent()) {
        tool_roots.push(parent.to_path_buf());
    }
    let local_package = user_home.join(".local/share/pi-omp-setup");
    if local_package.is_dir() {
        tool_roots.push(local_package);
    }
    omit_pub_roots_covered_by_overlay(
        &mut tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    tool_roots.sort();
    tool_roots.dedup();
    let explicit_claude = options.claude_executable.is_some();
    let claude_executable = options
        .claude_executable
        .unwrap_or_else(|| user_home.join(".local/bin/claude"));
    let claude_wrapper_parent = claude_executable.parent().map(PathBuf::from);
    let claude_executable = if claude_executable.is_file() {
        Some(fs::canonicalize(claude_executable)?)
    } else if explicit_claude {
        return Err(format!(
            "Claude executable {} does not exist",
            claude_executable.display()
        )
        .into());
    } else {
        None
    };
    let mut claude_tool_roots = options.claude_tool_roots;
    claude_tool_roots.extend(default_chat_tool_roots.iter().cloned());
    if let Some(parent) = claude_executable.as_ref().and_then(|path| path.parent()) {
        claude_tool_roots.push(parent.to_path_buf());
    }
    if claude_executable.is_some() {
        if let Some(parent) = claude_wrapper_parent {
            claude_tool_roots.push(parent);
        }
    }
    omit_pub_roots_covered_by_overlay(
        &mut claude_tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    claude_tool_roots.sort();
    claude_tool_roots.dedup();
    let explicit_codex = options.codex_executable.is_some();
    let codex_candidate = options
        .codex_executable
        .unwrap_or_else(|| default_codex_executable(&user_home));
    let codex_executable = if codex_candidate.is_file() {
        Some(fs::canonicalize(&codex_candidate)?)
    } else if explicit_codex {
        return Err(format!(
            "Codex executable {} does not exist",
            codex_candidate.display()
        )
        .into());
    } else {
        None
    };
    let mut codex_tool_roots = options.codex_tool_roots;
    codex_tool_roots.extend(default_chat_tool_roots.iter().cloned());
    if let Some(executable) = &codex_executable {
        // The npm shim loads its platform binary from its package directory.
        if executable
            .file_name()
            .is_some_and(|name| name == "codex.js")
        {
            if let Some(package) = executable.parent().and_then(|p| p.parent()) {
                codex_tool_roots.push(package.to_path_buf());
            }
        } else if let Some(parent) = executable.parent() {
            codex_tool_roots.push(parent.to_path_buf());
        }
    }
    // System tools already have /usr inside the sandbox; an extra bind is rejected.
    codex_tool_roots.retain(|root| !root.starts_with("/usr"));
    omit_pub_roots_covered_by_overlay(
        &mut codex_tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    codex_tool_roots.sort();
    codex_tool_roots.dedup();
    let explicit_grok = options.grok_executable.is_some();
    let grok_candidate = options
        .grok_executable
        .unwrap_or_else(|| user_home.join(".local/bin/grok"));
    let grok_executable = if grok_candidate.is_file() {
        Some(fs::canonicalize(&grok_candidate)?)
    } else if explicit_grok {
        return Err(format!(
            "Grok executable {} does not exist",
            grok_candidate.display()
        )
        .into());
    } else {
        None
    };
    let mut grok_tool_roots = options.grok_tool_roots;
    grok_tool_roots.extend(default_chat_tool_roots);
    if let Some(parent) = grok_executable.as_ref().and_then(|path| path.parent()) {
        grok_tool_roots.push(parent.to_path_buf());
    }
    omit_pub_roots_covered_by_overlay(
        &mut grok_tool_roots,
        &user_home,
        options.pub_cache_lower.as_deref(),
    );
    grok_tool_roots.sort();
    grok_tool_roots.dedup();
    if pi_executable.is_some()
        || claude_executable.is_some()
        || codex_executable.is_some()
        || grok_executable.is_some()
    {
        let mut chat_environment = std::collections::BTreeMap::new();
        chat_environment.insert("PROTECTORATE_HUB".into(), hub_url);
        hub = hub.with_chat_runtime(protectorate_hub::ChatRuntime::new(
            protectorate_hub::ChatLauncher {
                state_root: home.clone(),
                user_home: user_home.clone(),
                runtime_dir: runtime_dir.clone(),
                helper: helper.clone(),
                pi_executable,
                claude_executable,
                claude_tool_roots,
                codex_executable,
                codex_tool_roots,
                grok_executable,
                grok_tool_roots,
                tool_roots,
                flutter_sdk_root: options.flutter_sdk_root.clone(),
                flutter_cache_lower: options.flutter_cache_lower.clone(),
                flutter_tools_lower: options.flutter_tools_lower.clone(),
                pub_cache_lower: options.pub_cache_lower.clone(),
                go_mod_cache_lower: options.go_mod_cache_lower.clone(),
                environment: chat_environment,
                tasks_max: 4096,
            },
        ));
    }
    for (name, error) in hub.recover_chat_sleeves() {
        eprintln!("chat sleeve {name} could not resume: {error}");
    }
    let node_monitor = Arc::clone(&hub.node_monitor);
    let quota = Arc::clone(&hub.quota);
    let transport = Arc::clone(&hub.quota_transport);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let node_stop = Arc::clone(&stop);
    let node_interval = options.node_interval;
    eprintln!("protectorate-rs serving on {}", listener.local_addr()?);
    for listener in &tls_listeners {
        eprintln!("protectorate-rs serving TLS on {}", listener.local_addr()?);
    }
    let chat_runtime = hub.chat_runtime.clone();
    let mut dog = watchdog::Watchdog::new("hub", parse_interval)?;
    let identity = Arc::clone(&hub.identity);
    dog.sync_probe("identity store lock", move || drop(identity.devices()));
    let sleeves = Arc::clone(&hub.sleeves);
    dog.sync_probe("sleeve registry lock", move || drop(sleeves.list()));
    let mail = Arc::clone(&hub.mail);
    dog.sync_probe("mailbox lock", move || drop(mail.unresolved()));
    let inventory = Arc::clone(&hub.inventory);
    dog.sync_probe("node inventory lock", move || drop(inventory.list()));
    let directory = Arc::clone(&hub.directory);
    dog.sync_probe("directory lock", move || drop(directory.list()));
    let _watchdog = dog.arm()?;
    let router = web.attach(hub.router()?);
    let worker = tokio::task::spawn_blocking(move || quota.run(transport.as_ref(), &worker_stop));
    let node_worker = tokio::task::spawn_blocking(move || {
        while !node_stop.load(Ordering::Relaxed) {
            node_monitor.poll_all();
            let mut remaining = node_interval;
            while !remaining.is_zero() {
                if node_stop.load(Ordering::Relaxed) {
                    return;
                }
                let step = remaining.min(Duration::from_millis(100));
                std::thread::sleep(step);
                remaining -= step;
            }
        }
    });
    let backup_cancel = protectorate_backup::Cancellation::default();
    let backup_worker = backup_options.map(|backup_options| {
        let cancel = backup_cancel.clone();
        tokio::task::spawn_blocking(move || {
            protectorate_backup::run(
                &backup_options,
                backup_every,
                &cancel,
                |event| match event {
                    protectorate_backup::RunEvent::Created(path) => {
                        eprintln!("hub backup created: {}", path.display())
                    }
                    protectorate_backup::RunEvent::Failed(error) => {
                        eprintln!("hub backup failed: {error}")
                    }
                },
            )
        })
    });
    let stopping = Arc::clone(&hub_restart);
    let router = transport::peer_router(router);
    let (stop_listeners, listening) = tokio::sync::watch::channel(false);
    let until_stopped = |mut listening: tokio::sync::watch::Receiver<bool>| async move {
        let _ = listening.wait_for(|stopped| *stopped).await;
    };
    let mut servers = tokio::task::JoinSet::new();
    for listener in tls_listeners {
        let listener = transport::TlsListener::new(listener, Arc::clone(&tls))?;
        let service = router
            .clone()
            .into_make_service_with_connect_info::<transport::Peer>();
        let stopped = until_stopped(listening.clone());
        servers.spawn(async move {
            axum::serve(listener, service)
                .with_graceful_shutdown(stopped)
                .await
        });
    }
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    let stopped = until_stopped(listening);
    servers.spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(stopped)
            .await
    });
    let mut result = Ok(());
    tokio::select! {
        _ = shutdown_signal() => {},
        _ = async {
            while !stopping.load(Ordering::SeqCst) {
                hub_restart_notify.notified().await;
            }
        } => {},
        Some(ended) = servers.join_next() => result = ended?,
    }
    let _ = stop_listeners.send(true);
    while let Some(ended) = servers.join_next().await {
        result = result.and(ended?);
    }
    stop.store(true, Ordering::Relaxed);
    backup_cancel.cancel();
    if let Some(runtime) = chat_runtime {
        tokio::task::spawn_blocking(move || runtime.shutdown()).await?;
    }
    worker.await?;
    node_worker.await?;
    if let Some(backup_worker) = backup_worker {
        backup_worker.await??;
    }
    result?;
    Ok(hub_restart.load(Ordering::SeqCst))
}

fn omit_pub_roots_covered_by_overlay(
    roots: &mut Vec<PathBuf>,
    user_home: &Path,
    pub_cache_lower: Option<&Path>,
) {
    if pub_cache_lower.is_some() {
        let pub_cache = user_home.join(".pub-cache");
        roots.retain(|root| !root.starts_with(&pub_cache));
    }
}

fn select_terminal_tool_roots(
    user_home: &Path,
    configured: &[PathBuf],
    flutter_sdk: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = protectorate_sleeve_runner::default_tool_roots(user_home);
    roots.extend_from_slice(configured);
    if let Some(sdk) = flutter_sdk {
        roots.push(sdk.to_path_buf());
    }
    roots.sort();
    roots.dedup();
    roots
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeServeExit {
    Stopped,
    Restart,
    Uninstalled,
}

async fn serve_node(options: Options) -> Result<NodeServeExit, Box<dyn Error>> {
    let home = match options.home {
        Some(home) => home,
        None => default_home()?,
    };
    let home = fs::canonicalize(home)?;
    let (tls, renew) = node_join::serve_tls(&home)?;
    let metrics_options = protectorate_node_metrics::Options {
        version: build_version().to_owned(),
        home: home.clone(),
        watch_paths: options.watch_paths,
        docker_host: options.docker_host,
        ..Default::default()
    };
    #[cfg(target_os = "windows")]
    let collector: Arc<dyn protectorate_node::MetricsCollector> = Arc::new(
        protectorate_node_metrics::Collector::windows(metrics_options),
    );
    #[cfg(not(target_os = "windows"))]
    let collector: Arc<dyn protectorate_node::MetricsCollector> =
        Arc::new(protectorate_node_metrics::Collector::linux(metrics_options));
    let file_root = options.file_root.unwrap_or_else(|| home.join("node-files"));
    #[cfg(windows)]
    let mut jobs = protectorate_node::Jobs::open_windows(&home, options.job_concurrency)?;
    #[cfg(not(windows))]
    let mut jobs = protectorate_node::Jobs::new(&home, options.job_concurrency)?;
    if options.allow_plain_jobs {
        jobs = jobs.with_plain_mode();
    }
    #[cfg(target_os = "linux")]
    {
        let helper = match options.sandbox_helper {
            Some(path) => path,
            None => std::env::current_exe()?.with_file_name("protectorate-node-sandbox-helper"),
        };
        if !helper.is_file() {
            return Err(format!(
                "sandbox helper {} is missing; build and install protectorate-node-sandbox-helper beside protectorate-rs",
                helper.display()
            )
            .into());
        }
        let user_home = fs::canonicalize(PathBuf::from(
            env::var_os("HOME").ok_or("HOME is unset; cannot plan sandboxed jobs")?,
        ))?;
        if home == user_home {
            return Err(
                "node state home must differ from the user's home for sandboxed jobs".into(),
            );
        }
        let runtime_dir = home.join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        let mut sandbox_options = protectorate_sandbox::runtime::LaunchOptions::new(
            helper,
            runtime_dir,
            user_home.clone(),
            home.clone(),
        );
        sandbox_options.egress_log = Some(Arc::new(
            |decision: &protectorate_sandbox::egress::Decision| {
                eprintln!(
                    "job egress {:?} {}:{}: {}",
                    decision.verdict, decision.host, decision.port, decision.reason
                )
            },
        ));
        jobs = jobs.with_sandbox(protectorate_node::SandboxConfig {
            policy: protectorate_sandbox::Policy {
                state_root: home.clone(),
                user_home: user_home.clone(),
                protected: Vec::new(),
                provider_credentials: Default::default(),
                trusted_tool_roots: Vec::new(),
                immutable_cache_roots: Vec::new(),
                nested_cgroups_available: false,
            },
            options: sandbox_options,
            egress: Some(protectorate_sandbox::Egress {
                hub: None,
                allow: options.egress_allow.clone(),
                private_hosts: Vec::new(),
            }),
        });
    }
    let restart_requested = Arc::new(AtomicBool::new(false));
    let restart_notify = Arc::new(tokio::sync::Notify::new());
    let uninstall_requested = Arc::new(AtomicBool::new(false));
    let uninstall_notify = Arc::new(tokio::sync::Notify::new());
    let restart_flag = Arc::clone(&restart_requested);
    let restart_wake = Arc::clone(&restart_notify);
    let restart: protectorate_node::UpdateRestart = Arc::new(move || {
        restart_flag.store(true, Ordering::SeqCst);
        restart_wake.notify_one();
    });
    let agent = protectorate_node::Agent::new(collector)
        .with_certificate_renewal(renew)
        .with_files_durable(file_root, options.file_unconfined, &home)?
        .with_jobs(jobs)
        .with_update(std::env::current_exe()?, build_version(), restart)?;
    #[cfg(target_os = "linux")]
    let agent = {
        let user_home = fs::canonicalize(PathBuf::from(
            env::var_os("HOME").ok_or("HOME is unset; cannot plan node uninstall")?,
        ))?;
        let plan = protectorate_node::UninstallPlan::new(
            &home,
            user_home,
            std::env::current_exe()?,
            true,
        )?;
        let requested = Arc::clone(&uninstall_requested);
        let notify = Arc::clone(&uninstall_notify);
        agent.with_uninstall(
            plan,
            "/usr/bin/systemctl",
            Arc::new(move || {
                requested.store(true, Ordering::SeqCst);
                notify.notify_one();
            }),
        )
    };
    if agent.start_update_trial()? {
        return Ok(NodeServeExit::Restart);
    }
    let (listener, follow) = node_join::bind(&home, options.listen).await?;
    let (swap, rebind) = tokio::sync::mpsc::channel(1);
    let mut dog = watchdog::Watchdog::new("node", parse_interval)?;
    if let Some(jobs) = agent.jobs() {
        dog.async_probe("job table locks", move || {
            let jobs = Arc::clone(&jobs);
            async move {
                jobs.list().await;
            }
        });
    }
    let _watchdog = dog.arm()?;
    eprintln!("protectorate-rs node serving on {}", listener.local_addr()?);
    follow.spawn(swap);
    let shutdown_requested = Arc::clone(&restart_requested);
    let uninstall_shutdown = Arc::clone(&uninstall_requested);
    axum::serve(
        transport::TlsListener::new_rebindable(listener, tls, Some(rebind))?,
        transport::peer_router(agent.router()?)
            .into_make_service_with_connect_info::<transport::Peer>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = shutdown_signal() => {},
            _ = async {
                while !shutdown_requested.load(Ordering::SeqCst) {
                    restart_notify.notified().await;
                }
            } => {},
            _ = async {
                while !uninstall_shutdown.load(Ordering::SeqCst) {
                    uninstall_notify.notified().await;
                }
            } => {},
        }
    })
    .await?;
    Ok(if uninstall_requested.load(Ordering::SeqCst) {
        NodeServeExit::Uninstalled
    } else if restart_requested.load(Ordering::SeqCst) {
        NodeServeExit::Restart
    } else {
        NodeServeExit::Stopped
    })
}

const GUARD_CHILD: &str = "PROTECTORATE_NODE_GUARD_CHILD";
const HUB_GUARD_CHILD: &str = "PROTECTORATE_HUB_GUARD_CHILD";

fn hub_source(options: &Options) -> Result<protectorate_release::Source, Box<dyn Error>> {
    let dir = options
        .release_dir
        .as_ref()
        .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    Ok(protectorate_release::Source::new(
        &options.release_repo,
        &options.release_channel,
        &options.release_api_url,
        &dir,
    )?)
}

fn hub_trial(
    source: &protectorate_release::Source,
) -> Result<Option<(String, String)>, Box<dyn Error>> {
    let path = source.dir().join(".hub-trial.json");
    let body = match fs::read(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let marker: serde_json::Value = serde_json::from_slice(&body)?;
    let previous = marker["previous"]
        .as_str()
        .ok_or("hub trial has no previous version")?;
    let version = marker["version"]
        .as_str()
        .ok_or("hub trial has no candidate version")?;
    if !previous.is_empty() && !protectorate_release::valid_version(previous)
        || !protectorate_release::valid_version(version)
    {
        return Err("hub trial has invalid release version".into());
    }
    Ok(Some((previous.to_owned(), version.to_owned())))
}

fn rollback_hub_trial(
    source: &protectorate_release::Source,
    previous: &str,
) -> Result<(), Box<dyn Error>> {
    source.restore(previous)?;
    fs::remove_file(source.dir().join(".hub-trial.json"))?;
    Ok(())
}

fn confirm_hub_trial(
    source: &protectorate_release::Source,
    previous: &str,
    version: &str,
) -> Result<(), Box<dyn Error>> {
    source.verify_rust_bundle(version, std::env::consts::OS, host_arch())?;
    source.activate(version)?;
    source.retain_previous(previous)?;
    fs::remove_file(source.dir().join(".hub-trial.json"))?;
    if let Err(error) = source.prune(previous) {
        eprintln!("prune confirmed hub releases: {error}");
    }
    Ok(())
}

fn hub_candidate(source: &protectorate_release::Source) -> Option<PathBuf> {
    let version = source.current().version;
    if version.is_empty() {
        return None;
    }
    source
        .verify_rust_bundle(&version, std::env::consts::OS, host_arch())
        .ok()
}

fn spawn_hub_child(
    exe: &std::path::Path,
    args: &[std::ffi::OsString],
) -> std::io::Result<tokio::process::Child> {
    tokio::process::Command::new(exe)
        .args(args)
        .env(HUB_GUARD_CHILD, "1")
        .spawn()
}

fn hub_listen(listen: Option<SocketAddr>) -> SocketAddr {
    listen.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7470)))
}

async fn hub_health(options: &Options, expected: &str) -> bool {
    let Some(Ok(token)) = options.token_file.as_ref().map(read_token) else {
        return false;
    };
    let endpoint = format!("http://{}", hub_listen(options.listen));
    let expected = expected.to_owned();
    tokio::task::spawn_blocking(move || {
        let client =
            protectorate_connect_client::Client::new(&endpoint, token, Duration::from_secs(2))
                .ok()?;
        let status = client
            .unary(
                protectorate_wire::v1::hub_service::GET_STATUS,
                &protectorate_wire::v1::GetStatusRequest {},
            )
            .ok()?;
        Some(status.version == expected)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

async fn supervise_hub_restart(options: Options) -> Result<(), Box<dyn Error>> {
    let original = fs::canonicalize(std::env::current_exe()?)?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    supervise_hub_restart_with(options, original, args, shutdown_signal()).await
}

async fn supervise_hub_restart_with<F>(
    options: Options,
    original: PathBuf,
    args: Vec<std::ffi::OsString>,
    shutdown: F,
) -> Result<(), Box<dyn Error>>
where
    F: std::future::Future<Output = ()>,
{
    let source = hub_source(&options)?;
    if let Some((previous, _)) = hub_trial(&source)? {
        rollback_hub_trial(&source, &previous)?;
    }
    let mut current = hub_candidate(&source).unwrap_or_else(|| original.clone());
    let mut child = spawn_hub_child(&current, &args)?;
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => { let _ = child.kill().await; return Ok(()); }
            status = child.wait() => {
                let status = status?;
                if status.code() == Some(75) {
                    let Some((previous, version)) = hub_trial(&source)? else { return Err("hub requested restart without trial marker".into()); };
                    let candidate = match source.verify_rust_bundle(&version, std::env::consts::OS, host_arch()) {
                        Ok(candidate) => candidate,
                        Err(error) => {
                            eprintln!("hub candidate verification failed: {error}");
                            rollback_hub_trial(&source, &previous)?;
                            child = spawn_hub_child(&current, &args)?;
                            continue;
                        }
                    };
                    let mut next = match spawn_hub_child(&candidate, &args) {
                        Ok(next) => next,
                        Err(error) => {
                            eprintln!("hub candidate could not start: {error}");
                            rollback_hub_trial(&source, &previous)?;
                            child = spawn_hub_child(&current, &args)?;
                            continue;
                        }
                    };
                    let mut healthy = false;
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
                    while tokio::time::Instant::now() < deadline {
                        tokio::select! {
                            _ = &mut shutdown => { let _ = next.kill().await; return Ok(()); }
                            result = next.wait() => { eprintln!("hub candidate exited before health confirmation: {}", result?); break; }
                            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                                if hub_health(&options, &version).await { healthy = true; break; }
                            }
                        }
                    }
                    if healthy {
                        match confirm_hub_trial(&source, &previous, &version).map_err(|error| error.to_string()) {
                            Ok(()) => {
                                current = candidate;
                                child = next;
                            }
                            Err(error) => {
                                eprintln!("hub trial confirmation failed: {error}");
                                let _ = next.kill().await;
                                rollback_hub_trial(&source, &previous)?;
                                child = spawn_hub_child(&current, &args)?;
                            }
                        }
                    } else {
                        let _ = next.kill().await;
                        rollback_hub_trial(&source, &previous)?;
                        child = spawn_hub_child(&current, &args)?;
                    }
                    continue;
                }
                if status.success() { return Ok(()); }
                return Err(format!("hub child exited {status}").into());
            }
        }
    }
}

fn update_marker(exe: &std::path::Path) -> std::io::Result<(String, u64)> {
    protectorate_node::guardian_update_state(exe)
}

fn spawn_guard_child(
    exe: &std::path::Path,
    args: &[std::ffi::OsString],
) -> std::io::Result<tokio::process::Child> {
    // A recently replaced executable can briefly report ETXTBSY on some
    // filesystems. Retry only that transient error; all other launch errors
    // should be reported immediately to the guardian.
    for attempt in 0..5 {
        match tokio::process::Command::new(exe)
            .args(args)
            .env(GUARD_CHILD, "1")
            .spawn()
        {
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 4 => {
                std::thread::sleep(Duration::from_millis(25));
            }
            result => return result,
        }
    }
    unreachable!("the final spawn attempt always returns")
}

async fn supervise_node_restart(exe: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let first = protectorate_node::guardian_restart_target(exe)?;
    let mut candidate = spawn_guard_child(&first, &args)?;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                let _ = candidate.kill().await;
                return Ok(());
            }
            status = candidate.wait() => {
                let status = status?;
                if status.code() == Some(76) {
                    return Ok(());
                }
                let phase = match update_marker(exe) {
                    Ok((phase, _)) => phase,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound && status.success() => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                if status.code() == Some(75) {
                    let target = protectorate_node::guardian_restart_target(exe)?;
                    candidate = spawn_guard_child(&target, &args)?;
                    continue;
                }
                if phase == "trial" || phase == "ready" {
                    protectorate_node::recover_failed_trial(exe, &format!("new node binary exited {status} before confirmation"))?;
                    let target = protectorate_node::guardian_restart_target(exe)?;
                    candidate = spawn_guard_child(&target, &args)?;
                    continue;
                }
                if status.success() {
                    return Ok(());
                }
                if phase == "confirmed" || phase == "rolledback" {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let target = protectorate_node::guardian_restart_target(exe)?;
                    candidate = spawn_guard_child(&target, &args)?;
                    continue;
                }
                return Err(format!("node child exited {status} with unknown update state").into());
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                let (phase, deadline_ms) = match update_marker(exe) {
                    Ok(marker) => marker,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
                if phase == "trial" && now_ms >= deadline_ms.saturating_add(15_000) {
                    let _ = candidate.kill().await;
                    protectorate_node::recover_failed_trial(exe, "new node binary did not confirm before deadline")?;
                    let target = protectorate_node::guardian_restart_target(exe)?;
                    candidate = spawn_guard_child(&target, &args)?;
                } else if phase == "ready" {
                    if let Err(error) = protectorate_node::guardian_confirm_ready(exe) {
                        eprintln!("node release confirmation failed: {error}");
                        let _ = candidate.kill().await;
                        protectorate_node::recover_failed_trial(exe, &format!("release confirmation failed: {error}"))?;
                        let target = protectorate_node::guardian_restart_target(exe)?;
                        candidate = spawn_guard_child(&target, &args)?;
                    }
                }
            }
        }
    }
}

fn report_version() {
    println!(
        "{}",
        serde_json::json!({
            "version": build_version(),
            "os": std::env::consts::OS,
            "arch": host_arch(),
        })
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match parse(env::args().skip(1)).map_err(std::io::Error::other)? {
        None => print!("{}", usage()),
        Some(Command::Version) => report_version(),
        #[cfg(target_os = "linux")]
        Some(Command::Driver(command)) => driver::run(command).await?,
        Some(Command::Hub(options)) => {
            if std::env::var_os(HUB_GUARD_CHILD).is_some() {
                if serve_hub(options).await? {
                    std::process::exit(75);
                }
                // A blocking node call may still be inside its socket
                // timeout. The hub child has finished service cleanup; exit
                // so runtime teardown cannot outlast the service stop window.
                std::process::exit(0);
            } else {
                supervise_hub_restart(options).await?;
            }
        }
        Some(Command::Node(options)) => {
            let executable = fs::canonicalize(std::env::current_exe()?)?;
            match serve_node(options).await? {
                NodeServeExit::Restart => {
                    if std::env::var_os(GUARD_CHILD).is_some() {
                        std::process::exit(75);
                    }
                    supervise_node_restart(&executable).await?;
                }
                NodeServeExit::Uninstalled if std::env::var_os(GUARD_CHILD).is_some() => {
                    std::process::exit(76);
                }
                NodeServeExit::Uninstalled | NodeServeExit::Stopped => {}
            }
        }
        Some(Command::NodeUninstall(options)) => {
            node_uninstall_with(options, std::path::Path::new("/usr/bin/systemctl"))?;
        }
        Some(Command::NodeJoin(command)) => node_join::run(command).await?,
        Some(Command::NodeInstall(command)) => node_install::run(command).await?,
        Some(Command::NodeDoctor(command)) => node_doctor::run(command).await?,
        Some(Command::HubBackup(options)) => hub_backup(options)?,
        Some(Command::HubRestore(options)) => hub_restore(options)?,
        Some(Command::HubPublishFlutterCache(options)) => {
            let lower = flutter_cache::publish(&options.home, &options.sdk, options.max_bytes)?;
            println!("{}", lower.display());
        }
        Some(Command::HubPublishFlutterTools(options)) => {
            let lower =
                flutter_cache::publish_tools(&options.home, &options.sdk, options.max_bytes)?;
            println!("{}", lower.display());
        }
        Some(Command::HubPublishPubCache(options)) => {
            let lower =
                flutter_cache::publish_pub(&options.home, &options.source, options.max_bytes)?;
            println!("{}", lower.display());
        }
        Some(Command::HubPublishGoModCache(options)) => {
            let lower =
                flutter_cache::publish_go_mod(&options.home, &options.source, options.max_bytes)?;
            println!("{}", lower.display());
        }
        Some(Command::ApiCall(options)) => api_call(options)?,
    }
    Ok(())
}

/// How long a node's last good poll stays usable. The hub polls every
/// `node_interval`, so the window has to scale with it: a node polled every two
/// minutes would otherwise be reported UNREACHABLE, alerted, and made the
/// subject of node.unreachable between two successful polls.
fn node_timing(interval: Duration) -> protectorate_node_monitor::Timing {
    protectorate_node_monitor::Timing {
        stale_after: interval.saturating_mul(2),
        ..Default::default()
    }
}
