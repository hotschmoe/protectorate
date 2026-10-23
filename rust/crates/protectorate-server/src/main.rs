//! Experimental Rust hub and node entry points.
mod client;
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
mod node_tailnet;
mod tailnet_runtime;
mod transport;
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
    NodeTailnet(node_tailnet::Command),
    NodeJoin(node_join::Command),
    NodeInstall(node_install::Command),
    HubBackup(BackupCommand),
    HubTailnetInstall(PathBuf),
    HubTailnetInit(PathBuf, protectorate_tailnet::Config),
    Client(client::Command),
    HubRestore(RestoreCommand),
    HubPublishFlutterCache(PublishFlutterCacheCommand),
    HubPublishFlutterTools(PublishFlutterCacheCommand),
    HubPublishPubCache(PublishPubCacheCommand),
    HubPublishGoModCache(PublishPubCacheCommand),
    ApiCall(ApiCall),
    DirectTransfer(DirectTransfer),
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

#[derive(Clone, Copy)]
enum TailnetApi {
    Hub,
    Node,
}

/// Tailnet ACLs use fixed service ports even when the host listener uses a
/// custom or ephemeral port.
fn tailnet_api_port(service: TailnetApi, _host_listener: SocketAddr) -> u16 {
    match service {
        TailnetApi::Hub => protectorate_tailnet::HUB_API_PORT,
        TailnetApi::Node => protectorate_tailnet::NODE_PORT,
    }
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
    token_file: PathBuf,
    agent_token_file: Option<PathBuf>,
    local_node_url: Option<String>,
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
    quota_interval: Duration,
    local_llm_urls: Vec<String>,
    local_model_hosts: Vec<String>,
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
    listen: SocketAddr,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    node_ca_cert: Option<PathBuf>,
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
struct DirectTransfer {
    pull: bool,
    node_url: String,
    token_file: PathBuf,
    local: PathBuf,
    remote: String,
    force: bool,
    compress: bool,
    compact: bool,
}
struct NodeUninstallCommand {
    home: PathBuf,
    dry_run: bool,
}

fn usage() -> &'static str {
    "Usage:\n  protectorate-rs version --json\n  protectorate-rs serve --home DIR --owner-token-file FILE [--web DIR] [--agent-token-file FILE] [--node-ca-cert FILE] [--local-node-url URL] [--workspace-root DIR] [--reference-dir DIR] [--sleeve-effort high] [--msg-ack-timeout 30m] [--dead-retention 24h|0] [--node-interval 30s] [--quota-interval 5m] [--local-llm-url URL] [--local-model-host HOST] [--trusted-proxy IP] [--public-url URL] [--watch DIR] [--tmux-socket NAME] [--orchestrator-admin-grants] [--release-repo OWNER/REPO] [--update-channel stable|dev] [--release-api-url URL] [--release-dir DIR] [--node-binary-dir DIR] [--pi-executable FILE] [--pi-tool-root DIR] [--claude-executable FILE] [--claude-tool-root DIR] [--codex-executable FILE] [--codex-tool-root DIR] [--grok-executable FILE] [--grok-tool-root DIR] [--terminal-tool-root DIR] [--flutter-sdk-root DIR --flutter-cache-lower DIR] [--flutter-tools-lower DIR] [--pub-cache-lower DIR] [--go-mod-cache-lower DIR] [--backup-dir DIR] [--backup-every 6h] [--listen 127.0.0.1:7470] [--tls-cert FILE --tls-key FILE]\n  protectorate-rs hub tailnet install [--home DIR]\n  protectorate-rs hub tailnet init --server-url URL [--home DIR] [--no-embedded-derp --public-derp] [--pool NAME]\n  protectorate-rs client invite --name NAME [--expires 1h] [--home DIR]\n  protectorate-rs client register REGISTRATION_KEY [--home DIR]\n  protectorate-rs hub backup [--home DIR] [--to DIR]\n  protectorate-rs hub restore FILE --home DIR\n  protectorate-rs hub publish-flutter-cache --home DIR --flutter-sdk-root DIR [--max-bytes 3221225472]\n  protectorate-rs hub publish-flutter-tools --home DIR --flutter-sdk-root DIR [--max-bytes 4194304]\n  protectorate-rs hub publish-pub-cache --home DIR --pub-cache-root DIR [--max-bytes 3221225472]\n  protectorate-rs hub publish-go-mod-cache --home DIR --go-mod-cache-root DIR [--max-bytes 3221225472]\n  protectorate-rs node join --control URL [--name NAME] [--key KEY | --key-file FILE] [--ca-cert FILE] [--home DIR]\n  protectorate-rs node install --control URL [--name NAME] [--key KEY | --key-file FILE | PROTECTORATE_ENROLL_KEY] [--ca-cert FILE] [--home DIR] [--listen IP:PORT] [--tls-cert FILE --tls-key FILE] [--allow-plain-jobs] [--dry-run] [--no-start] [-- serve-flags]\n  protectorate-rs node renew NAME [--home DIR]\n  protectorate-rs node revoke NAME [--home DIR]\n  protectorate-rs node uninstall [--home DIR] [--dry-run]\n  protectorate-rs node doctor [--home DIR] [--hub URL] [--ca-cert FILE] [--json]\n  protectorate-rs node serve --agent-token-file FILE [--home DIR] [--file-root DIR] [--file-unconfined] [--job-concurrency 2] [--sandbox-helper FILE] [--allow-plain-jobs] [--watch-dir DIR] [--docker-host URL] [--listen 127.0.0.1:7475] [--tls-cert FILE --tls-key FILE]\n  protectorate-rs node direct-put LOCAL REMOTE --agent-token-file FILE [--node-url URL] [--force] [--no-compress] [--json]\n  protectorate-rs node direct-get REMOTE LOCAL --agent-token-file FILE [--node-url URL] [--force] [--no-compress] [--json]\n  protectorate-rs api call Service/Method [JSON] [--hub URL] [--token-file FILE] [--json]\n\nNon-loopback listeners require TLS certificate and private key files; loopback HTTP remains the default. Hub TLS certificates must cover the listen IP, or 127.0.0.1/::1 for wildcard listeners, for internal clients and update health checks.\n"
}

fn default_home() -> Result<PathBuf, String> {
    if let Some(home) = env::var_os("PROTECTORATE_HOME") {
        return Ok(PathBuf::from(home));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".protectorate-rust"))
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
        Some("tailnet") => {
            let action = args.next().ok_or("hub tailnet requires install or init")?;
            let mut home = None;
            let mut config = protectorate_tailnet::Config::default();
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--home" => {
                        home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
                    }
                    "--server-url" if action == "init" => {
                        config.server_url = args.next().ok_or("--server-url requires URL")?
                    }
                    "--listen" if action == "init" => {
                        config.listen = args.next().ok_or("--listen requires IP:PORT")?
                    }
                    "--grpc-listen" if action == "init" => {
                        config.grpc_listen = args.next().ok_or("--grpc-listen requires IP:PORT")?
                    }
                    "--metrics-listen" if action == "init" => {
                        config.metrics_listen =
                            args.next().ok_or("--metrics-listen requires IP:PORT")?
                    }
                    "--base-domain" if action == "init" => {
                        config.base_domain = args.next().ok_or("--base-domain requires NAME")?
                    }
                    "--hub-name" if action == "init" => {
                        config.hub_name = args.next().ok_or("--hub-name requires NAME")?
                    }
                    "--no-embedded-derp" if action == "init" => config.embedded_derp = false,
                    "--derp-listen" if action == "init" => {
                        config.derp_listen = args.next().ok_or("--derp-listen requires IP:PORT")?
                    }
                    "--stun-listen" if action == "init" => {
                        config.stun_listen = args.next().ok_or("--stun-listen requires IP:PORT")?
                    }
                    "--derp-ipv4" if action == "init" => {
                        config.derp_ipv4 = Some(args.next().ok_or("--derp-ipv4 requires IPv4")?)
                    }
                    "--public-derp" if action == "init" => config.public_derp = true,
                    "--tls-cert" if action == "init" => {
                        config.tls_cert = Some(PathBuf::from(
                            args.next().ok_or("--tls-cert requires FILE")?,
                        ))
                    }
                    "--tls-key" if action == "init" => {
                        config.tls_key =
                            Some(PathBuf::from(args.next().ok_or("--tls-key requires FILE")?))
                    }
                    "--key-expiry-days" if action == "init" => {
                        config.key_expiry_days = args
                            .next()
                            .ok_or("--key-expiry-days requires N")?
                            .parse()
                            .map_err(|_| "--key-expiry-days requires N")?
                    }
                    "--pool" if action == "init" => config
                        .pools
                        .push(args.next().ok_or("--pool requires NAME")?),
                    _ => return Err(format!("unknown tailnet {action} argument {flag:?}")),
                }
            }
            let home = home.map_or_else(default_home, Ok)?;
            match action.as_str() {
                "install" => Ok(Command::HubTailnetInstall(home)),
                "init" => {
                    config.validate().map_err(|error| error.to_string())?;
                    Ok(Command::HubTailnetInit(home, config))
                }
                _ => Err("hub tailnet requires install or init".into()),
            }
        }
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
        Some("client") => return client::parse(args).map(|command| Some(Command::Client(command))),
        Some("api") => return parse_api(args).map(|a| Some(Command::ApiCall(a))),
        Some("node") => {
            match args.next().as_deref() {
                Some("serve") => {}
                Some(action @ ("renew" | "revoke")) => {
                    return node_tailnet::parse(action, args)
                        .map(|command| Some(Command::NodeTailnet(command)));
                }
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
                Some("direct-put") => {
                    return parse_direct_transfer(args, false)
                        .map(|a| Some(Command::DirectTransfer(a)));
                }
                Some("direct-get") => {
                    return parse_direct_transfer(args, true)
                        .map(|a| Some(Command::DirectTransfer(a)));
                }
                _ => return Err(usage().into()),
            }
            true
        }
        _ => return Err(usage().into()),
    };
    let mut home = None;
    let mut token_file = None;
    let mut agent_token_file = None;
    let mut local_node_url = None;
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
    let mut quota_interval = Duration::from_secs(5 * 60);
    let mut local_llm_urls = Vec::new();
    let mut local_model_hosts = Vec::new();
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
    let mut listen = if node {
        "127.0.0.1:7475"
    } else {
        "127.0.0.1:7470"
    }
    .parse::<SocketAddr>()
    .unwrap();
    let mut tls_cert = None;
    let mut tls_key = None;
    let mut node_ca_cert = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?)),
            "--owner-token-file" if !node => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or("--owner-token-file requires FILE")?,
                ))
            }
            "--agent-token-file" if node => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or("--agent-token-file requires FILE")?,
                ))
            }
            "--agent-token-file" => {
                agent_token_file = Some(PathBuf::from(
                    args.next().ok_or("--agent-token-file requires FILE")?,
                ))
            }
            "--local-node-url" if !node => {
                local_node_url = Some(args.next().ok_or("--local-node-url requires URL")?);
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
                listen = args
                    .next()
                    .ok_or("--listen requires ADDRESS")?
                    .parse()
                    .map_err(|_| "--listen requires IP:PORT")?
            }
            "--tls-cert" => {
                tls_cert = Some(PathBuf::from(
                    args.next().ok_or("--tls-cert requires FILE")?,
                ))
            }
            "--tls-key" => {
                tls_key = Some(PathBuf::from(args.next().ok_or("--tls-key requires FILE")?))
            }
            "--node-ca-cert" if !node => {
                node_ca_cert = Some(PathBuf::from(
                    args.next().ok_or("--node-ca-cert requires FILE")?,
                ))
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
    if tls_cert.is_some() != tls_key.is_some() {
        return Err("--tls-cert and --tls-key must be supplied together".into());
    }
    if !listen.ip().is_loopback() && tls_cert.is_none() {
        return Err("non-loopback --listen requires --tls-cert and --tls-key".into());
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
    let token_file = token_file.ok_or(if node {
        "--agent-token-file is required"
    } else {
        "--owner-token-file is required"
    })?;
    let options = Options {
        home,
        token_file,
        agent_token_file,
        local_node_url,
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
        quota_interval,
        local_llm_urls,
        local_model_hosts,
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
        tls_cert,
        tls_key,
        node_ca_cert,
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

fn parse_direct_transfer(
    mut args: impl Iterator<Item = String>,
    pull: bool,
) -> Result<DirectTransfer, String> {
    let first = args
        .next()
        .ok_or("file transfer requires REMOTE and LOCAL paths")?;
    let second = args
        .next()
        .ok_or("file transfer requires REMOTE and LOCAL paths")?;
    let (remote, local) = if pull {
        (first, PathBuf::from(second))
    } else {
        (second, PathBuf::from(first))
    };
    if remote.is_empty() || local.as_os_str().is_empty() {
        return Err("file paths cannot be empty".into());
    }
    let mut node_url = "http://127.0.0.1:7475".to_owned();
    let mut token_file = None;
    let mut force = false;
    let mut compress = true;
    let mut compact = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--node-url" => node_url = args.next().ok_or("--node-url requires URL")?,
            "--agent-token-file" => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or("--agent-token-file requires FILE")?,
                ))
            }
            "--force" | "-f" => force = true,
            "--no-compress" => compress = false,
            "--json" => compact = true,
            _ => return Err(format!("unknown transfer argument {arg:?}")),
        }
    }
    Ok(DirectTransfer {
        pull,
        node_url,
        token_file: token_file.ok_or("--agent-token-file is required")?,
        local,
        remote,
        force,
        compress,
        compact,
    })
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

fn direct_transfer(options: DirectTransfer) -> Result<(), Box<dyn Error>> {
    let token = read_token(&options.token_file)?;
    let client = protectorate_connect_client::Client::new(
        &options.node_url,
        token,
        Duration::from_secs(600),
    )?;
    let result = if options.pull {
        protectorate_node_client::pull(
            &client,
            "",
            &options.remote,
            &options.local,
            options.force,
            options.compress,
        )?
    } else {
        protectorate_node_client::push(
            &client,
            "",
            &options.local,
            &options.remote,
            options.force,
            options.compress,
        )?
    };
    if options.compact {
        println!(
            "{}",
            serde_json::json!({"path": result.path, "size": result.size.to_string(), "sha256": result.sha256, "skipped": result.skipped})
        );
    } else {
        println!(
            "{}\t{} bytes\t{} skipped",
            result.path.display(),
            result.size,
            result.skipped
        );
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

/// Adapts the existing blocking Connect clients to the authenticated virtual
/// TCP dialer. A listener is created only for a name present in the current
/// map; every accepted connection resolves that name again before dialing.
struct NodeProxy {
    runtime: Arc<protectorate_tailnet_runtime::Runtime>,
    handle: tokio::runtime::Handle,
    endpoints: std::sync::Mutex<std::collections::HashMap<String, String>>,
    secret: String,
}

async fn proxy_request(
    inbound: &mut tokio::net::TcpStream,
    secret: &str,
) -> std::io::Result<(Vec<u8>, u64)> {
    use tokio::io::AsyncReadExt;
    let mut buffered = Vec::with_capacity(4096);
    let end = loop {
        if let Some(end) = buffered.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break end + 4;
        }
        if buffered.len() >= 16 * 1024 {
            return Err(std::io::Error::other(
                "node proxy request headers too large",
            ));
        }
        let mut chunk = [0_u8; 4096];
        let count = inbound.read(&mut chunk).await?;
        if count == 0 {
            return Err(std::io::Error::other(
                "node proxy request ended before headers",
            ));
        }
        buffered.extend_from_slice(&chunk[..count]);
    };
    let header = std::str::from_utf8(&buffered[..end])
        .map_err(|_| std::io::Error::other("invalid node proxy request headers"))?;
    let mut lines = header.split("\r\n");
    let first = lines.next().unwrap_or_default();
    if !first.starts_with("POST /protectorate.v1.NodeAgentService/")
        || !first.ends_with(" HTTP/1.1")
    {
        return Err(std::io::Error::other(
            "node proxy accepts only node RPC requests",
        ));
    }
    let mut forwarded = Vec::with_capacity(buffered.len());
    forwarded.extend_from_slice(first.as_bytes());
    forwarded.extend_from_slice(b"\r\n");
    let mut authorized = false;
    let mut content_length = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(std::io::Error::other("invalid node proxy header"));
        };
        if name.eq_ignore_ascii_case("authorization") {
            if authorized || value.trim() != format!("Bearer {secret}") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "node proxy authorization failed",
                ));
            }
            authorized = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(std::io::Error::other("duplicate node proxy content length"));
            }
            content_length = value.trim().parse::<u64>().ok();
            if content_length.is_none() {
                return Err(std::io::Error::other("invalid node proxy content length"));
            }
            forwarded.extend_from_slice(line.as_bytes());
            forwarded.extend_from_slice(b"\r\n");
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(std::io::Error::other(
                "chunked node proxy requests are unsupported",
            ));
        } else if name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("proxy-connection")
        {
            // One request per proxy connection; a second request cannot carry
            // the local-only Authorization header into the tailnet.
        } else {
            forwarded.extend_from_slice(line.as_bytes());
            forwarded.extend_from_slice(b"\r\n");
        }
    }
    if !authorized {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "node proxy authorization missing",
        ));
    }
    let length =
        content_length.ok_or_else(|| std::io::Error::other("node proxy content length missing"))?;
    let initial_body = &buffered[end..];
    if initial_body.len() as u64 > length {
        return Err(std::io::Error::other(
            "pipelined node proxy requests are refused",
        ));
    }
    forwarded.extend_from_slice(b"Connection: close\r\n\r\n");
    forwarded.extend_from_slice(initial_body);
    Ok((forwarded, length - initial_body.len() as u64))
}

impl NodeProxy {
    fn endpoint(&self, name: &str, address: &str) -> Result<String, String> {
        // Explicit direct addresses retain the existing HTTPS or loopback
        // path and its bearer token. Invited nodes use their DNS label here.
        if address != name {
            return direct_node_url(address);
        }
        self.runtime
            .peer_addr(name, 7475)
            .map_err(|e| e.to_string())?;
        let mut endpoints = self.endpoints.lock().unwrap();
        if let Some(url) = endpoints.get(name) {
            return Ok(url.clone());
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("bind node proxy: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let url = format!(
            "tailnet+http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let runtime = Arc::clone(&self.runtime);
        let name = name.to_owned();
        let secret = self.secret.clone();
        let _guard = self.handle.enter();
        let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
        endpoints.insert(name.clone(), url.clone());
        self.handle.spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                let runtime = Arc::clone(&runtime);
                let name = name.clone();
                let secret = secret.clone();
                tokio::spawn(async move {
                    let Ok((request, remaining)) = proxy_request(&mut inbound, &secret).await
                    else {
                        return;
                    };
                    let Ok(Ok(mut outbound)) = tokio::time::timeout(
                        Duration::from_secs(15),
                        runtime.dial_peer(&name, 7475),
                    )
                    .await
                    else {
                        return;
                    };
                    use tokio::io::AsyncWriteExt;
                    if outbound.write_all(&request).await.is_err() {
                        return;
                    }
                    use tokio::io::AsyncReadExt;
                    let Ok(sent) =
                        tokio::io::copy(&mut (&mut inbound).take(remaining), &mut outbound).await
                    else {
                        return;
                    };
                    if sent != remaining {
                        return;
                    }
                    let _ = tokio::io::copy(&mut outbound, &mut inbound).await;
                });
            }
        });
        Ok(url)
    }
}

fn direct_node_url(address: &str) -> Result<String, String> {
    let (scheme, authority) = if let Some(authority) = address.strip_prefix("https://") {
        ("https", authority)
    } else if let Some(authority) = address.strip_prefix("http://") {
        ("http", authority)
    } else if address.starts_with("127.0.0.1") || address.starts_with("[::1]") {
        ("http", address)
    } else {
        ("https", address)
    };
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ', '\\']) {
        return Err("invalid direct node address".into());
    }
    let has_port = if authority.starts_with('[') {
        authority.contains("]:")
    } else {
        authority.contains(':')
    };
    let authority = if has_port {
        authority.to_owned()
    } else {
        format!("{authority}:7475")
    };
    if scheme == "http" && !authority.starts_with("127.0.0.1:") && !authority.starts_with("[::1]:")
    {
        return Err("direct node HTTP requires loopback".into());
    }
    Ok(format!("{scheme}://{authority}"))
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
    let cli = companion("protectorate-cli");
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
    let token = read_token(&options.token_file)?;
    let node_trust = options
        .node_ca_cert
        .as_ref()
        .map(|path| {
            let pem = transport::load_ca(path)?;
            protectorate_connect_client::TrustRoots::from_pem(&pem)
                .map_err(|error| std::io::Error::other(error.to_string()))
        })
        .transpose()?;
    let tls = match (&options.tls_cert, &options.tls_key) {
        (Some(cert), Some(key)) => Some(transport::tls_config(cert, key)?),
        _ => None,
    };
    let tls_ca_pem = options
        .tls_cert
        .as_ref()
        .map(fs::read_to_string)
        .transpose()?;
    if tls_ca_pem.as_ref().is_some_and(|pem| pem.len() > 16 * 1024) {
        return Err("hub TLS certificate chain exceeds terminal environment limit (16 KiB)".into());
    }
    let listener = tokio::net::TcpListener::bind(options.listen).await?;
    let hub_url = format!(
        "{}://{}",
        if tls.is_some() { "https" } else { "http" },
        transport::reachable_addr(listener.local_addr()?)
    );
    let tailnet = tailnet_runtime::Runtime::start_if_configured(&home).await?;
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
    hub = hub.with_sleeve_effort(options.sleeve_effort.clone());
    hub = hub.with_dead_retention(options.dead_retention);
    hub = hub.with_trusted_proxies(options.trusted_proxies.clone());
    if let Some(url) = &options.public_url {
        hub = hub.with_public_url(url.clone())?;
    }
    if tailnet.is_some() {
        hub.jobs.defer_resume();
    }
    if let Some(runtime) = &tailnet {
        hub = hub
            .with_tailnet_admin(runtime.admin())
            .with_tailnet_pools(runtime.pools().to_vec())
            .with_tailnet_hub_name(runtime.hub_name().to_owned());
    }
    if let Some(trust) = node_trust.clone() {
        hub = hub.with_node_trust(trust);
    }
    hub = if tls.is_some() {
        hub.with_tls_device_auth()
    } else {
        hub.with_loopback_device_auth()
    };
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
            tls_ca_pem,
            cli_executable: cli,
            tasks_max: 4096,
            tool_roots: terminal_tool_roots,
            flutter_sdk_root: options.flutter_sdk_root.clone(),
            flutter_cache_lower: options.flutter_cache_lower.clone(),
            flutter_tools_lower: options.flutter_tools_lower.clone(),
            pub_cache_lower: options.pub_cache_lower.clone(),
            go_mod_cache_lower: options.go_mod_cache_lower.clone(),
        })
        .with_msg_ack_timeout(options.msg_ack_timeout);
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
    if let Some(endpoint) = options.local_node_url {
        protectorate_connect_client::Client::new_with_trust(
            &endpoint,
            "validation-only",
            Duration::from_secs(1),
            node_trust.as_ref(),
        )?;
        hub = hub.with_local_node_endpoint(endpoint);
    }
    if let Some(path) = options.agent_token_file {
        hub = hub
            .with_node_token(read_token(&path)?)
            .map_err(std::io::Error::other)?;
    }
    for (name, error) in hub.recover_chat_sleeves() {
        eprintln!("chat sleeve {name} could not resume: {error}");
    }
    let node_monitor = Arc::clone(&hub.node_monitor);
    let routed_monitor = Arc::clone(&hub.node_monitor);
    let routed_jobs = Arc::clone(&hub.jobs);
    let routed_files = Arc::clone(&hub.node_files);
    let quota = Arc::clone(&hub.quota);
    let transport = Arc::clone(&hub.quota_transport);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let node_stop = Arc::clone(&stop);
    let node_interval = options.node_interval;
    eprintln!("protectorate-rs serving on {}", listener.local_addr()?);
    let tailnet_status = hub.tailnet_status_slot();
    let chat_runtime = hub.chat_runtime.clone();
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
    let expiry_worker = tailnet.as_ref().map(|runtime| {
        let admin = runtime.admin();
        let days = runtime.key_expiry_days();
        let stop = Arc::clone(&stop);
        tokio::task::spawn_blocking(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Err(error) = admin.enforce_expiry(days) {
                    eprintln!("tailnet key expiry: {error}");
                }
                for _ in 0..600 {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
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
    let tailnet_http = tailnet
        .as_ref()
        .map(|_| {
            transport::TailnetHttp::start(
                &home.join("tailnet/tsnet-hub"),
                tailnet_api_port(TailnetApi::Hub, listener.local_addr()?),
                router.clone(),
            )
        })
        .transpose()?;
    if let Some(http) = &tailnet_http {
        let status_runtime = http.runtime();
        *tailnet_status
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) =
            Some(Arc::new(move || status_runtime.status_snapshot()));
        let proxy = Arc::new(NodeProxy {
            runtime: http.runtime(),
            handle: tokio::runtime::Handle::current(),
            endpoints: std::sync::Mutex::new(std::collections::HashMap::new()),
            secret: protectorate_connect_client::virtual_proxy_secret()?,
        });
        let route = Arc::new(move |name: &str, address: &str| proxy.endpoint(name, address));
        routed_monitor.set_route(route.clone());
        routed_jobs.set_route(route.clone());
        routed_files.set_route(route);
        let jobs = Arc::clone(&routed_jobs);
        let runtime = http.runtime();
        tokio::spawn(async move {
            let mut state = runtime.subscribe_state();
            loop {
                if matches!(
                    *state.borrow(),
                    protectorate_tailnet_runtime::State::Connected { .. }
                ) {
                    jobs.resume_deferred();
                    break;
                }
                if state.changed().await.is_err() {
                    break;
                }
            }
        });
    }
    let result = if let Some(tls) = tls {
        let listener = transport::TlsListener::new(listener, tls)?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<transport::Peer>(),
        )
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_signal() => {},
                _ = async {
                    while !stopping.load(Ordering::SeqCst) {
                        hub_restart_notify.notified().await;
                    }
                } => {},
            }
        })
        .await
    } else {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_signal() => {},
                _ = async {
                    while !stopping.load(Ordering::SeqCst) {
                        hub_restart_notify.notified().await;
                    }
                } => {},
            }
        })
        .await
    };
    stop.store(true, Ordering::Relaxed);
    backup_cancel.cancel();
    if let Some(runtime) = chat_runtime {
        tokio::task::spawn_blocking(move || runtime.shutdown()).await?;
    }
    worker.await?;
    node_worker.await?;
    if let Some(expiry_worker) = expiry_worker {
        expiry_worker.await?;
    }
    if let Some(backup_worker) = backup_worker {
        backup_worker.await??;
    }
    if let Some(http) = tailnet_http {
        match tokio::time::timeout(Duration::from_secs(8), http.stop()).await {
            Ok(result) => result?,
            Err(_) => eprintln!("hub tailnet shutdown exceeded 8 seconds; exiting"),
        }
    }
    if let Some(runtime) = tailnet {
        runtime.stop().await?;
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
    let token = read_token(&options.token_file)?;
    let tls = match (&options.tls_cert, &options.tls_key) {
        (Some(cert), Some(key)) => Some(transport::tls_config(cert, key)?),
        _ => None,
    };
    let home = match options.home {
        Some(home) => home,
        None => match env::var_os("PROTECTORATE_HOME") {
            Some(home) => PathBuf::from(home),
            None => PathBuf::from(env::var_os("HOME").ok_or("HOME is unset; pass --home")?)
                .join(".protectorate-rust"),
        },
    };
    fs::create_dir_all(&home)?;
    let home = fs::canonicalize(home)?;
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
            options: protectorate_sandbox::runtime::LaunchOptions::new(
                helper,
                runtime_dir,
                user_home,
                home.clone(),
            ),
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
    let agent = protectorate_node::Agent::new(collector, token)
        .with_desk_home(&home)?
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
    let listener = tokio::net::TcpListener::bind(options.listen).await?;
    eprintln!("protectorate-rs node serving on {}", listener.local_addr()?);
    let shutdown_requested = Arc::clone(&restart_requested);
    let uninstall_shutdown = Arc::clone(&uninstall_requested);
    let trusted_router = if home.join("tsnet/join.json").try_exists()? {
        Some(transport::peer_router(
            agent.clone().router_trusted_tailnet()?,
        ))
    } else {
        None
    };
    let router = transport::peer_router(agent.router()?);
    let tailnet_http = if home.join("tsnet/join.json").try_exists()? {
        Some(transport::TailnetHttp::start_node(
            &home.join("tsnet"),
            tailnet_api_port(TailnetApi::Node, listener.local_addr()?),
            trusted_router.expect("joined node has trusted router"),
        )?)
    } else {
        None
    };
    if let Some(tls) = tls {
        let listener = transport::TlsListener::new(listener, tls)?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<transport::Peer>(),
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
    } else {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
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
    }
    if let Some(http) = tailnet_http {
        match tokio::time::timeout(Duration::from_secs(8), http.stop()).await {
            Ok(result) => result?,
            Err(_) => eprintln!("node tailnet shutdown exceeded 8 seconds; exiting"),
        }
    }
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

async fn hub_health(options: &Options, expected: &str) -> bool {
    let token = match read_token(&options.token_file) {
        Ok(token) => token,
        Err(_) => return false,
    };
    if let Some(cert) = &options.tls_cert {
        return transport::hub_health(options.listen, cert, &token, expected).await;
    }
    let endpoint = format!("http://{}", transport::reachable_addr(options.listen));
    let expected = expected.to_owned();
    tokio::task::spawn_blocking(move || {
        let client =
            protectorate_connect_client::Client::new(&endpoint, token, Duration::from_secs(2))
                .ok()?;
        let status = client
            .call(
                "/protectorate.v1.HubService/GetStatus",
                &serde_json::json!({}),
            )
            .ok()?;
        Some(status["version"] == expected)
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
                // A stopped blocking map poll may still be inside its read
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
                // The map poll uses a blocking control stream. After the
                // HTTP and tailnet stop window, leaving Tokio's runtime by
                // return would wait for that blocking read until its socket
                // timeout. The process is already stopping, so close its
                // sockets promptly instead of delaying the user service.
                NodeServeExit::Uninstalled | NodeServeExit::Stopped => std::process::exit(0),
            }
        }
        Some(Command::NodeUninstall(options)) => {
            node_uninstall_with(options, std::path::Path::new("/usr/bin/systemctl"))?;
        }
        Some(Command::NodeTailnet(command)) => node_tailnet::run(command).await?,
        Some(Command::NodeJoin(command)) => node_join::run(command).await?,
        Some(Command::NodeInstall(command)) => node_install::run(command).await?,
        Some(Command::NodeDoctor(command)) => node_doctor::run(command).await?,
        Some(Command::HubBackup(options)) => hub_backup(options)?,
        Some(Command::HubTailnetInstall(home)) => {
            let path = protectorate_tailnet_install::install(&home)?;
            println!("installed {} (SHA-256 verified)", path.display());
        }
        Some(Command::HubTailnetInit(home, config)) => {
            config.save(&home)?;
            println!(
                "saved tailnet configuration in {}",
                home.join("tailnet").display()
            );
        }
        Some(Command::Client(command)) => client::run(command).await?,
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
        Some(Command::DirectTransfer(options)) => direct_transfer(options)?,
    }
    Ok(())
}

/// How long a node's last good poll stays usable. The hub polls every
/// `node_interval`, so the window has to scale with it: a node polled every two
/// minutes would otherwise be reported UNREACHABLE, alerted, and made the
/// subject of node.unreachable between two successful polls. Go's NodeOptions
/// does the same (StaleAfter defaults to 2 * Interval).
fn node_timing(interval: Duration) -> protectorate_node_monitor::Timing {
    protectorate_node_monitor::Timing {
        stale_after: interval.saturating_mul(2),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_default_tools_include_installed_go_and_add_explicit_roots() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("owner");
        let go = home.join(".local/go");
        fs::create_dir_all(go.join("bin")).unwrap();
        fs::write(go.join("bin/go"), b"go").unwrap();
        let defaults = select_terminal_tool_roots(&home, &[], None);
        assert!(defaults.contains(&go));
        let custom = root.path().join("custom");
        let sdk = root.path().join("flutter");
        assert_eq!(
            select_terminal_tool_roots(&home, std::slice::from_ref(&custom), Some(&sdk)),
            vec![custom, sdk, go]
        );
    }

    #[test]
    fn published_pub_overlay_uses_its_own_tools_instead_of_live_owner_binds() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("owner");
        let pub_bin = home.join(".pub-cache/bin");
        let go_bin = home.join("go/bin");
        fs::create_dir_all(&pub_bin).unwrap();
        fs::create_dir_all(&go_bin).unwrap();
        let mut roots = select_terminal_tool_roots(&home, &[], None);
        assert!(roots.contains(&pub_bin));
        omit_pub_roots_covered_by_overlay(&mut roots, &home, Some(root.path()));
        assert!(!roots.contains(&pub_bin));
        assert!(roots.contains(&go_bin));
    }
    #[test]
    fn node_stale_window_scales_with_the_poll_interval() {
        assert_eq!(
            node_timing(Duration::from_secs(30)).stale_after,
            Duration::from_secs(60)
        );
        assert_eq!(
            node_timing(Duration::from_secs(45)).stale_after,
            Duration::from_secs(90)
        );
        assert_eq!(
            node_timing(Duration::from_secs(120)).stale_after,
            Duration::from_secs(240)
        );
        assert_eq!(
            node_timing(Duration::from_secs(120)).fetch_timeout,
            Duration::from_secs(15)
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn hub_guardian_hands_off_listener_and_rolls_back_failed_trial() {
        use sha2::{Digest, Sha256};
        use std::{net::TcpListener as StdListener, os::unix::fs::PermissionsExt};

        async fn scenario(fail: bool) {
            let temp = tempfile::tempdir().unwrap();
            let release_dir = temp.path().join("releases");
            let home = temp.path().join("home");
            let token = temp.path().join("token");
            fs::create_dir_all(&home).unwrap();
            fs::write(&token, "owner-secret").unwrap();
            fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
            let reserved = StdListener::bind("127.0.0.1:0").unwrap();
            let port = reserved.local_addr().unwrap().port();
            drop(reserved);
            let source = protectorate_release::Source::new(
                "owner/repo",
                "stable",
                "http://127.0.0.1:1",
                release_dir.to_str().unwrap(),
            )
            .unwrap();
            let v1 = protectorate_release::Info {
                tag: "v1".into(),
                version: "v1".into(),
                ..Default::default()
            };
            source.store(&v1, b"").unwrap();
            source.activate("v1").unwrap();
            let trigger = temp.path().join("trigger");
            let old = temp.path().join("old-hub");
            let old_program = format!(
                "#!/usr/bin/env python3\nimport os,socket,time\ns=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',{port})); s.listen(); s.settimeout(.1)\nwhile True:\n if os.path.exists({trigger:?}):\n  os.unlink({trigger:?}); s.close(); raise SystemExit(75)\n try: c,_=s.accept()\n except socket.timeout: continue\n c.recv(4096); body=b'{{\"version\":\"v1\"}}'; c.sendall(b'HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: '+str(len(body)).encode()+b'\\r\\nConnection: close\\r\\n\\r\\n'+body); c.close()\n",
                trigger = trigger.to_string_lossy().to_string()
            );
            fs::write(&old, old_program).unwrap();
            fs::set_permissions(&old, fs::Permissions::from_mode(0o755)).unwrap();
            let candidate_program = if fail {
                "#!/bin/sh\nexit 19\n".to_owned()
            } else {
                format!(
                    "#!/usr/bin/env python3\nimport socket\ns=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',{port})); s.listen()\nwhile True:\n c,_=s.accept(); c.recv(4096); body=b'{{\"version\":\"v2\"}}'; c.sendall(b'HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: '+str(len(body)).encode()+b'\\r\\nConnection: close\\r\\n\\r\\n'+body); c.close()\n"
                )
            };
            let mut sums = String::new();
            for component in protectorate_release::RUST_BUNDLE {
                let bytes = if component == "protectorate-rs" {
                    candidate_program.as_bytes()
                } else {
                    b"companion".as_slice()
                };
                let name = format!("{component}-{}-{}", std::env::consts::OS, host_arch());
                sums.push_str(&format!("{:x}  {name}\n", Sha256::digest(bytes)));
            }
            let v2 = protectorate_release::Info {
                tag: "v2".into(),
                version: "v2".into(),
                ..Default::default()
            };
            let stage = source.store(&v2, sums.as_bytes()).unwrap();
            for component in protectorate_release::RUST_BUNDLE {
                let bytes = if component == "protectorate-rs" {
                    candidate_program.as_bytes()
                } else {
                    b"companion".as_slice()
                };
                let path = stage.join(format!(
                    "{component}-{}-{}",
                    std::env::consts::OS,
                    host_arch()
                ));
                fs::write(&path, bytes).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                fs::hard_link(&path, stage.join(component)).unwrap();
            }
            let Some(Command::Hub(options)) = parse(
                vec![
                    "serve".into(),
                    "--home".into(),
                    home.to_string_lossy().into_owned(),
                    "--owner-token-file".into(),
                    token.to_string_lossy().into_owned(),
                    "--listen".into(),
                    format!("127.0.0.1:{port}"),
                    "--release-dir".into(),
                    release_dir.to_string_lossy().into_owned(),
                ]
                .into_iter(),
            )
            .unwrap() else {
                panic!("hub options");
            };
            let stop = Arc::new(tokio::sync::Notify::new());
            let stop_child = Arc::clone(&stop);
            let task = tokio::spawn(async move {
                supervise_hub_restart_with(options, old, vec![], async move {
                    stop_child.notified().await
                })
                .await
                .map_err(|e| e.to_string())
            });
            let endpoint = format!("http://127.0.0.1:{port}");
            let healthy = |version: &'static str| {
                let endpoint = endpoint.clone();
                async move {
                    tokio::task::spawn_blocking(move || {
                        let client = protectorate_connect_client::Client::new(
                            &endpoint,
                            "owner-secret",
                            Duration::from_secs(1),
                        )
                        .ok()?;
                        let value = client
                            .call(
                                "/protectorate.v1.HubService/GetStatus",
                                &serde_json::json!({}),
                            )
                            .ok()?;
                        Some(value["version"] == version)
                    })
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false)
                }
            };
            tokio::time::timeout(Duration::from_secs(5), async {
                while !healthy("v1").await {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
            fs::write(
                release_dir.join(".hub-trial.json"),
                r#"{"previous":"v1","version":"v2"}"#,
            )
            .unwrap();
            assert_eq!(source.current().version, "v1");
            fs::write(&trigger, b"go").unwrap();
            let expected = if fail { "v1" } else { "v2" };
            let trial_result = tokio::time::timeout(Duration::from_secs(8), async {
                while release_dir.join(".hub-trial.json").exists() || !healthy(expected).await {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await;
            if trial_result.is_err() {
                panic!(
                    "hub trial timed out: fail={fail} marker={} current={} guardian_finished={} trigger={} v1_healthy={} v2_healthy={}",
                    release_dir.join(".hub-trial.json").exists(),
                    source.current().version,
                    task.is_finished(),
                    trigger.exists(),
                    healthy("v1").await,
                    healthy("v2").await
                );
            }
            assert_eq!(source.current().version, expected);
            if !fail {
                assert_eq!(
                    fs::read_link(release_dir.join("previous")).unwrap(),
                    PathBuf::from("v1")
                );
            }
            assert!(
                !task.is_finished(),
                "guardian must remain the service main process"
            );
            stop.notify_one();
            tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
        scenario(false).await;
        scenario(true).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn marker_free_clean_node_exit_does_not_restart_guardian() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("node-binary");
        fs::write(&exe, b"#!/bin/sh\nsleep 1\nexit 0\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        tokio::time::timeout(Duration::from_secs(3), supervise_node_restart(&exe))
            .await
            .unwrap()
            .unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn uninstall_exit_beats_a_pending_update_trial() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("node-binary");
        fs::write(&exe, b"#!/bin/sh\nexit 76\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            dir.path().join("node-binary.update"),
            r#"{"phase":"trial","deadline_ms":0}"#,
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), supervise_node_restart(&exe))
            .await
            .unwrap()
            .unwrap();
        assert!(dir.path().join("node-binary.update").exists());
    }
    #[test]
    fn tailnet_installer_uses_a_pinned_release_and_requires_no_service_token() {
        let Some(Command::HubTailnetInstall(home)) = parse(args(&[
            "hub",
            "tailnet",
            "install",
            "--home",
            "/tmp/tailnet-home",
        ]))
        .unwrap() else {
            panic!("expected Headscale install command");
        };
        assert_eq!(
            protectorate_tailnet_install::binary_path(&home),
            PathBuf::from("/tmp/tailnet-home/bin/headscale-0.29.5")
        );
        assert!(parse(args(&["hub", "tailnet", "install", "--version", "latest"])).is_err());
    }
    #[test]
    fn tailnet_init_saves_validated_public_relay_configuration() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("state");
        let Some(Command::HubTailnetInit(parsed_home, config)) = parse(
            vec![
                "hub".into(),
                "tailnet".into(),
                "init".into(),
                "--home".into(),
                home.display().to_string(),
                "--server-url".into(),
                "https://control.example:7480".into(),
                "--no-embedded-derp".into(),
                "--public-derp".into(),
                "--pool".into(),
                "build".into(),
            ]
            .into_iter(),
        )
        .unwrap() else {
            panic!("expected tailnet init")
        };
        assert_eq!(parsed_home, home);
        config.save(&home).unwrap();
        assert_eq!(protectorate_tailnet::Config::load(&home).unwrap(), config);
        assert!(parse(args(&[
            "hub",
            "tailnet",
            "init",
            "--server-url",
            "http://127.0.0.1:7480",
            "--no-embedded-derp"
        ]))
        .is_err());
    }
    #[test]
    fn node_uninstall_parses_dry_run_without_service_token() {
        let Some(Command::NodeUninstall(command)) = parse(args(&[
            "node",
            "uninstall",
            "--home",
            "/tmp/node-state",
            "--dry-run",
        ]))
        .unwrap() else {
            panic!("expected node uninstall command")
        };
        assert!(command.dry_run);
        assert_eq!(command.home, PathBuf::from("/tmp/node-state"));
        assert!(parse(args(&["node", "uninstall", "--delete-files"])).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn local_node_uninstall_dry_run_and_fake_systemctl() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("systemctl");
        let marker = dir.path().join("called");
        fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        node_uninstall_with(
            NodeUninstallCommand {
                home: dir.path().join("state"),
                dry_run: true,
            },
            &fake,
        )
        .unwrap();
        assert!(!marker.exists());
        node_uninstall_with(
            NodeUninstallCommand {
                home: dir.path().join("state"),
                dry_run: false,
            },
            &fake,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(marker).unwrap(),
            "--user disable --now protectorate-node.service\n"
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_candidate_is_restored_by_previous_process() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("node-binary");
        let previous = temp.path().join("node-binary.old");
        fs::write(&exe, b"#!/bin/sh\nexit 17\n").unwrap();
        fs::write(&previous, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&previous, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            temp.path().join("node-binary.update"),
            serde_json::json!({
                "phase":"trial","from":"v1","to":"v2","attempts":0,
                "deadline_ms":0,"reason":""
            })
            .to_string(),
        )
        .unwrap();
        supervise_node_restart(&exe).await.unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(fs::read(&exe).unwrap(), b"#!/bin/sh\nexit 0\n");
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(temp.path().join("node-binary.update")).unwrap())
                .unwrap();
        assert_eq!(marker["phase"], "rolledback");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn guardian_stays_alive_after_confirmation_until_child_exits() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("node-binary");
        let marker = temp.path().join("node-binary.update");
        fs::write(&exe, format!("#!/bin/sh\nprintf '%s\\n' '{{\"phase\":\"confirmed\",\"from\":\"v1\",\"to\":\"v2\",\"attempts\":1,\"deadline_ms\":0,\"reason\":\"\"}}' > '{}'\nsleep 1\nexit 0\n", marker.display())).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &marker,
            serde_json::json!({"phase":"trial","from":"v1","to":"v2","attempts":0,"deadline_ms":u64::MAX,"reason":""}).to_string(),
        )
        .unwrap();
        let handle = tokio::spawn(async move {
            supervise_node_restart(&exe)
                .await
                .map_err(|e| e.to_string())
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        if handle.is_finished() {
            panic!(
                "guardian exited while the confirmed child was alive: {:?}",
                handle.await.unwrap()
            );
        }
        tokio::time::timeout(Duration::from_secs(3), handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    fn args<'a>(items: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        items.iter().map(|item| (*item).to_owned())
    }
    #[test]
    fn backup_commands_and_schedule_require_explicit_destinations() {
        let Some(Command::HubBackup(backup)) = parse(args(&[
            "hub",
            "backup",
            "--home",
            "/tmp/source",
            "--to",
            "/tmp/snapshots",
        ]))
        .unwrap() else {
            panic!("expected hub backup");
        };
        assert_eq!(backup.home, PathBuf::from("/tmp/source"));
        assert_eq!(backup.to, Some(PathBuf::from("/tmp/snapshots")));
        let Some(Command::HubRestore(restore)) = parse(args(&[
            "hub",
            "restore",
            "/tmp/a.pbackup",
            "--home",
            "/tmp/empty",
        ]))
        .unwrap() else {
            panic!("expected hub restore");
        };
        assert_eq!(restore.archive, PathBuf::from("/tmp/a.pbackup"));
        assert_eq!(restore.home, PathBuf::from("/tmp/empty"));
        let Some(Command::Hub(scheduled)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/source",
            "--owner-token-file",
            "/tmp/token",
            "--backup-dir",
            "/tmp/snapshots",
            "--backup-every",
            "12h",
        ]))
        .unwrap() else {
            panic!("expected hub serve");
        };
        assert_eq!(scheduled.backup_every, Duration::from_secs(12 * 3600));
        assert!(parse(args(&[
            "serve",
            "--home",
            "/tmp/source",
            "--owner-token-file",
            "/tmp/token",
            "--backup-every",
            "12h",
        ]))
        .is_err());
        assert!(parse_interval("0h").is_err());
        assert!(parse_interval("18446744073709551615d").is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn hub_watch_directory_reaches_local_node_metrics() {
        let temp = tempfile::tempdir().unwrap();
        let watched = temp.path().join("watched");
        fs::create_dir(&watched).unwrap();
        fs::write(watched.join("sample"), b"four").unwrap();
        let hub =
            protectorate_hub::Hub::open(temp.path().join("hub"), "owner-secret", "v-test").unwrap();
        let monitor = protectorate_node_monitor::Monitor::new_with_watch_paths(
            Arc::clone(&hub.inventory),
            hub.hostname.clone(),
            temp.path().join("hub"),
            vec![watched.clone()],
        );
        monitor.poll_node(&hub.hostname);
        let status = monitor.status(&hub.hostname, "local", false);
        assert_eq!(
            status["metrics"]["watchDirs"][0]["path"],
            watched.to_string_lossy().as_ref()
        );
        assert!(
            status["metrics"]["watchDirs"][0]["sizeBytes"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                >= 4
        );
    }

    #[test]
    fn parses_safe_server_arguments() {
        assert!(matches!(
            parse(args(&["version", "--json"])).unwrap(),
            Some(Command::Version)
        ));
        assert!(parse(args(&["version"])).is_err());
        let Some(Command::Hub(options)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
        ]))
        .unwrap() else {
            panic!("expected hub command");
        };
        assert_eq!(options.listen.to_string(), "127.0.0.1:7470");
        assert!(options.agent_token_file.is_none());
        assert!(options.local_node_url.is_none());
        assert!(options.workspace_roots.is_empty());
        assert_eq!(options.msg_ack_timeout, Duration::from_secs(30 * 60));
        assert_eq!(options.dead_retention, Duration::from_secs(24 * 3600));
        assert_eq!(options.release_repo, "hotschmoe/protectorate");
        assert_eq!(options.release_channel, "stable");
        assert!(options.release_api_url.is_empty());
        assert!(options.release_dir.is_none());
        assert!(options.node_binary_dir.is_none());
        assert_eq!(options.node_interval, Duration::from_secs(30));
        assert_eq!(options.quota_interval, Duration::from_secs(300));
        assert!(options.local_model_hosts.is_empty());
        assert!(options.trusted_proxies.is_empty());
        let Some(Command::Hub(tuned)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--dead-retention",
            "0",
            "--node-interval",
            "45s",
            "--quota-interval",
            "10m",
            "--local-llm-url",
            "http://127.0.0.1:1234/v1",
            "--local-llm-url",
            "https://model.lan/api",
            "--local-model-host",
            "llm.example.com",
            "--local-model-host",
            "GPU.LOCAL",
            "--trusted-proxy",
            "127.0.0.1",
            "--trusted-proxy",
            "::1",
            "--public-url",
            "https://hub.example.test:7480",
            "--watch",
            "/tmp/models",
            "--watch",
            "/srv/cache",
            "--tmux-socket",
            "test-hub",
        ]))
        .unwrap() else {
            panic!("expected tuned hub command");
        };
        assert_eq!(tuned.node_interval, Duration::from_secs(45));
        assert_eq!(tuned.dead_retention, Duration::ZERO);
        assert_eq!(tuned.quota_interval, Duration::from_secs(600));
        assert_eq!(
            tuned.local_llm_urls,
            ["http://127.0.0.1:1234/v1", "https://model.lan/api"]
        );
        assert_eq!(tuned.local_model_hosts, ["llm.example.com", "GPU.LOCAL"]);
        assert_eq!(
            tuned.public_url.as_deref(),
            Some("https://hub.example.test:7480")
        );
        assert_eq!(
            tuned.trusted_proxies,
            [
                "127.0.0.1".parse::<std::net::IpAddr>().unwrap(),
                "::1".parse().unwrap(),
            ]
        );
        assert_eq!(
            tuned.hub_watch_paths,
            [PathBuf::from("/tmp/models"), PathBuf::from("/srv/cache")]
        );
        assert_eq!(tuned.tmux_socket, "test-hub");
        for url in [
            "file:///tmp/models",
            "http://user:secret@localhost/v1",
            "http://localhost/v1?key=secret",
        ] {
            assert!(parse(args(&[
                "serve",
                "--home",
                "/tmp/h",
                "--owner-token-file",
                "/tmp/token",
                "--local-llm-url",
                url
            ]))
            .is_err());
        }
        assert!(parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--trusted-proxy",
            "proxy.example.com",
        ]))
        .is_err());
        let Some(Command::Hub(routed)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--agent-token-file",
            "/tmp/agent-token",
            "--local-node-url",
            "http://127.0.0.1:7475",
            "--workspace-root",
            "/srv/projects",
            "--reference-dir",
            "/srv/references",
            "--msg-ack-timeout",
            "45m",
        ]))
        .unwrap() else {
            panic!("expected routed hub command");
        };
        assert_eq!(
            routed.local_node_url.as_deref(),
            Some("http://127.0.0.1:7475")
        );
        assert_eq!(routed.workspace_roots, [PathBuf::from("/srv/projects")]);
        assert_eq!(routed.reference_dirs, [PathBuf::from("/srv/references")]);
        assert_eq!(routed.msg_ack_timeout, Duration::from_secs(45 * 60));
        let Some(Command::Hub(release)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--release-repo",
            "owner/repo",
            "--update-channel",
            "dev",
            "--release-api-url",
            "http://127.0.0.1:9000",
            "--release-dir",
            "/tmp/releases",
            "--node-binary-dir",
            "/tmp/bins",
        ]))
        .unwrap() else {
            panic!("expected release hub command")
        };
        assert_eq!(release.release_repo, "owner/repo");
        assert_eq!(release.release_channel, "dev");
        assert_eq!(release.release_api_url, "http://127.0.0.1:9000");
        assert_eq!(release.release_dir, Some(PathBuf::from("/tmp/releases")));
        assert_eq!(release.node_binary_dir, Some(PathBuf::from("/tmp/bins")));
        assert!(parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--update-channel",
            "nightly",
        ]))
        .is_err());
        let Some(Command::Hub(pi)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--pi-executable",
            "/opt/pi/bin/pi",
            "--pi-tool-root",
            "/opt/pi/lib",
            "--pi-tool-root",
            "/opt/node",
        ]))
        .unwrap() else {
            panic!("expected Pi hub command");
        };
        assert_eq!(pi.pi_executable, Some(PathBuf::from("/opt/pi/bin/pi")));
        assert_eq!(
            pi.pi_tool_roots,
            [PathBuf::from("/opt/pi/lib"), PathBuf::from("/opt/node")]
        );
        let Some(Command::Hub(terminal)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--terminal-tool-root",
            "/opt/cargo/bin",
            "--terminal-tool-root",
            "/opt/rust/toolchains",
        ]))
        .unwrap() else {
            panic!("expected terminal hub command");
        };
        assert_eq!(
            terminal.terminal_tool_roots,
            [
                PathBuf::from("/opt/cargo/bin"),
                PathBuf::from("/opt/rust/toolchains")
            ]
        );
        let Some(Command::HubPublishFlutterCache(publish)) = parse(args(&[
            "hub",
            "publish-flutter-cache",
            "--home",
            "/tmp/state",
            "--flutter-sdk-root",
            "/opt/flutter",
            "--max-bytes",
            "2048",
        ]))
        .unwrap() else {
            panic!("expected Flutter cache publication command");
        };
        assert_eq!(publish.home, PathBuf::from("/tmp/state"));
        assert_eq!(publish.sdk, PathBuf::from("/opt/flutter"));
        assert_eq!(publish.max_bytes, 2048);
        let Some(Command::HubPublishFlutterTools(tools)) = parse(args(&[
            "hub",
            "publish-flutter-tools",
            "--home",
            "/tmp/state",
            "--flutter-sdk-root",
            "/opt/flutter",
        ]))
        .unwrap() else {
            panic!("expected Flutter tools publication command");
        };
        assert_eq!(tools.max_bytes, 4 * 1024 * 1024);
        let Some(Command::HubPublishPubCache(pub_cache)) = parse(args(&[
            "hub",
            "publish-pub-cache",
            "--home",
            "/tmp/state",
            "--pub-cache-root",
            "/home/owner/.pub-cache",
        ]))
        .unwrap() else {
            panic!("expected Pub cache publication command")
        };
        assert_eq!(pub_cache.source, PathBuf::from("/home/owner/.pub-cache"));
        assert_eq!(pub_cache.max_bytes, 3 * 1024 * 1024 * 1024);
        let Some(Command::HubPublishGoModCache(go_mod)) = parse(args(&[
            "hub",
            "publish-go-mod-cache",
            "--home",
            "/tmp/state",
            "--go-mod-cache-root",
            "/home/owner/go/pkg/mod",
        ]))
        .unwrap() else {
            panic!("expected Go module cache publication command")
        };
        assert_eq!(go_mod.source, PathBuf::from("/home/owner/go/pkg/mod"));
        assert_eq!(go_mod.max_bytes, 3 * 1024 * 1024 * 1024);
        let Some(Command::Hub(claude)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--claude-executable",
            "/opt/claude/bin/claude",
            "--claude-tool-root",
            "/opt/claude/sdk",
        ]))
        .unwrap() else {
            panic!("expected Claude hub command");
        };
        assert_eq!(
            claude.claude_executable,
            Some(PathBuf::from("/opt/claude/bin/claude"))
        );
        assert_eq!(claude.claude_tool_roots, [PathBuf::from("/opt/claude/sdk")]);
        let Some(Command::Hub(codex)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--codex-executable",
            "/opt/codex/bin/codex",
            "--codex-tool-root",
            "/opt/codex/package",
        ]))
        .unwrap() else {
            panic!("expected Codex hub command");
        };
        assert_eq!(
            codex.codex_executable,
            Some(PathBuf::from("/opt/codex/bin/codex"))
        );
        assert_eq!(
            codex.codex_tool_roots,
            [PathBuf::from("/opt/codex/package")]
        );
        assert_eq!(codex.sleeve_effort, "high");
        let Some(Command::Hub(native)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--sleeve-effort",
            "",
        ]))
        .unwrap() else {
            panic!("expected native effort hub command");
        };
        assert!(native.sleeve_effort.is_empty());
        let Some(Command::Hub(grok)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--grok-executable",
            "/opt/grok/bin/grok",
            "--grok-tool-root",
            "/opt/grok/runtime",
        ]))
        .unwrap() else {
            panic!("expected Grok hub command");
        };
        assert_eq!(
            grok.grok_executable,
            Some(PathBuf::from("/opt/grok/bin/grok"))
        );
        assert_eq!(grok.grok_tool_roots, [PathBuf::from("/opt/grok/runtime")]);
        let Some(Command::Node(node)) = parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/agent-token",
            "--watch-dir",
            "/srv/work",
            "--watch-dir",
            "/srv/cache",
            "--docker-host",
            "unix:///run/docker.sock",
        ]))
        .unwrap() else {
            panic!("expected node command");
        };
        assert_eq!(
            node.watch_paths,
            [PathBuf::from("/srv/work"), PathBuf::from("/srv/cache")]
        );
        assert_eq!(node.docker_host.as_deref(), Some("unix:///run/docker.sock"));
        let Some(Command::Hub(options)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--agent-token-file",
            "/tmp/node-token",
        ]))
        .unwrap() else {
            panic!("expected hub command");
        };
        assert_eq!(
            options.agent_token_file,
            Some(PathBuf::from("/tmp/node-token"))
        );
        let Some(Command::Node(options)) = parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/node-token",
        ]))
        .unwrap() else {
            panic!("expected node command");
        };
        assert_eq!(options.listen.to_string(), "127.0.0.1:7475");
        assert!(!options.file_unconfined);
        assert_eq!(options.job_concurrency, 2);
        assert!(!options.allow_plain_jobs);
        let Some(Command::Node(files)) = parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--file-root",
            "/tmp/files",
            "--file-unconfined",
        ]))
        .unwrap() else {
            panic!("expected node command")
        };
        assert_eq!(files.file_root, Some(PathBuf::from("/tmp/files")));
        assert!(files.file_unconfined);
        let Some(Command::Node(jobs)) = parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--job-concurrency",
            "4",
            "--sandbox-helper",
            "/tmp/helper",
            "--allow-plain-jobs",
        ]))
        .unwrap() else {
            panic!("expected node command")
        };
        assert_eq!(jobs.job_concurrency, 4);
        assert_eq!(jobs.sandbox_helper, Some(PathBuf::from("/tmp/helper")));
        assert!(jobs.allow_plain_jobs);
        assert!(parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--job-concurrency",
            "0",
        ]))
        .is_err());
        assert!(parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--listen",
            "0.0.0.0:7470"
        ]))
        .is_err());
        assert!(parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--listen",
            "0.0.0.0:7475"
        ]))
        .is_err());
        assert!(parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--listen",
            "0.0.0.0:7470",
            "--tls-cert",
            "/tmp/cert.pem"
        ]))
        .is_err());
        assert!(parse(args(&[
            "node",
            "serve",
            "--listen",
            "127.0.0.1:7475",
            "--tls-key",
            "/tmp/key.pem"
        ]))
        .is_err());
        let Some(Command::Hub(tls)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--listen",
            "0.0.0.0:7470",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
        ]))
        .unwrap() else {
            panic!("expected TLS hub command")
        };
        assert_eq!(tls.tls_cert, Some(PathBuf::from("/tmp/cert.pem")));
        let Some(Command::Node(tls)) = parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--listen",
            "0.0.0.0:7475",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
        ]))
        .unwrap() else {
            panic!("expected TLS node command")
        };
        assert_eq!(tls.tls_key, Some(PathBuf::from("/tmp/key.pem")));
        let Some(Command::Hub(remote_ca)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--node-ca-cert",
            "/tmp/node-ca.pem",
        ]))
        .unwrap() else {
            panic!("expected hub with remote node CA")
        };
        assert_eq!(
            remote_ca.node_ca_cert,
            Some(PathBuf::from("/tmp/node-ca.pem"))
        );
        assert!(parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--node-ca-cert",
            "/tmp/node-ca.pem",
        ]))
        .is_err());
        assert!(parse(args(&["serve", "--home", "/tmp/h"])).is_err());
        let Some(Command::ApiCall(call)) = parse(args(&[
            "api",
            "call",
            "HubService/GetStatus",
            "{}",
            "--hub",
            "http://127.0.0.1:7470",
            "--json",
        ]))
        .unwrap() else {
            panic!("expected API call")
        };
        assert_eq!(call.procedure, "/protectorate.v1.HubService/GetStatus");
        assert_eq!(call.input, serde_json::json!({}));
        assert!(call.compact);
        assert!(parse(args(&["api", "call", "HubService/GetStatus", "{bad}"])).is_err());
        let Some(Command::DirectTransfer(put)) = parse(args(&[
            "node",
            "direct-put",
            "/tmp/source",
            "target",
            "--agent-token-file",
            "/tmp/token",
            "--force",
        ]))
        .unwrap() else {
            panic!("expected direct put")
        };
        assert!(!put.pull);
        assert_eq!(put.local, PathBuf::from("/tmp/source"));
        assert_eq!(put.remote, "target");
        assert!(put.force);
        assert!(parse(args(&["node", "direct-get", "target", "/tmp/dest"])).is_err());
    }

    #[test]
    fn orchestrator_admin_grants_are_opt_in_for_the_hub() {
        let Some(Command::Hub(defaults)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
        ]))
        .unwrap() else {
            panic!("expected hub options");
        };
        assert!(!defaults.orchestrator_admin_grants);
        let Some(Command::Hub(enabled)) = parse(args(&[
            "serve",
            "--home",
            "/tmp/h",
            "--owner-token-file",
            "/tmp/token",
            "--orchestrator-admin-grants",
        ]))
        .unwrap() else {
            panic!("expected hub options");
        };
        assert!(enabled.orchestrator_admin_grants);
        assert!(parse(args(&[
            "node",
            "serve",
            "--agent-token-file",
            "/tmp/token",
            "--orchestrator-admin-grants",
        ]))
        .is_err());
    }

    #[test]
    fn custom_host_port_does_not_change_acl_tailnet_ports() {
        let ephemeral: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let custom: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        for host in [ephemeral, custom] {
            assert_eq!(tailnet_api_port(TailnetApi::Hub, host), 7470);
            assert_eq!(tailnet_api_port(TailnetApi::Node, host), 7475);
        }
    }

    #[tokio::test]
    async fn node_proxy_rejects_local_intruders_and_strips_its_secret() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = listener.local_addr().unwrap();
        assert!(endpoint.ip().is_loopback());
        let request = |token: &str| {
            format!(
                "POST /protectorate.v1.NodeAgentService/GetMetrics HTTP/1.1\r\nHost: node\r\nAuthorization: Bearer {token}\r\nContent-Length: 2\r\n\r\n{{}}"
            )
        };
        let mut intruder = tokio::net::TcpStream::connect(endpoint).await.unwrap();
        intruder
            .write_all(request("wrong").as_bytes())
            .await
            .unwrap();
        let (mut accepted, _) = listener.accept().await.unwrap();
        let denied = proxy_request(&mut accepted, "private").await.unwrap_err();
        assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied);
        let mut owner = tokio::net::TcpStream::connect(endpoint).await.unwrap();
        owner
            .write_all(request("private").as_bytes())
            .await
            .unwrap();
        let (mut accepted, _) = listener.accept().await.unwrap();
        let (forwarded, remaining) = proxy_request(&mut accepted, "private").await.unwrap();
        assert_eq!(remaining, 0);
        assert!(
            forwarded.starts_with(b"POST /protectorate.v1.NodeAgentService/GetMetrics HTTP/1.1")
        );
        assert!(!String::from_utf8_lossy(&forwarded)
            .to_ascii_lowercase()
            .contains("authorization:"));
        assert!(forwarded.ends_with(b"Connection: close\r\n\r\n{}"));
        let mut second = tokio::net::TcpStream::connect(endpoint).await.unwrap();
        second
            .write_all(request("private").as_bytes())
            .await
            .unwrap();
        let (mut accepted, _) = listener.accept().await.unwrap();
        let (forwarded, remaining) = proxy_request(&mut accepted, "private").await.unwrap();
        assert_eq!(remaining, 0);
        assert!(!String::from_utf8_lossy(&forwarded)
            .to_ascii_lowercase()
            .contains("authorization:"));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires PROT_HEADSCALE_BINARY pointing to Headscale 0.29.5"]
    async fn real_headscale_node_job_and_metrics_through_loopback_proxy_and_virtual_tcp() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use protectorate_inventory::{Node, CARE};
        use protectorate_tailnet::{Config, TAG_HUB, TAG_NODE};
        use protectorate_tailnet_admin::Admin;
        use protectorate_tailnet_discovery::DiscoIdentity;
        use protectorate_tailnet_member::MemberIdentity;
        use protectorate_tailnet_runtime::State;
        use sha2::{Digest, Sha256};
        use std::{
            net::TcpListener,
            os::unix::fs::PermissionsExt,
            process::{Child, Command, Stdio},
            time::Instant,
        };

        struct ChildGuard(Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        struct Metrics;
        impl protectorate_node::MetricsCollector for Metrics {
            fn collect(&self) -> std::io::Result<protectorate_node_metrics::NodeMetrics> {
                Ok(protectorate_node_metrics::NodeMetrics::default())
            }
        }
        fn free_addr() -> SocketAddr {
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
        }
        fn join(root: &std::path::Path, name: &str, control: &str, admin: &Admin, tag: &str) {
            let dir = root.join(name);
            let member = MemberIdentity::open(&dir).unwrap();
            let disco = DiscoIdentity::open(&dir).unwrap();
            let key = admin
                .create_key(&[tag.to_owned()], Duration::from_secs(3600))
                .unwrap();
            member
                .register_http(control, &key.key, name, Duration::from_secs(10))
                .unwrap();
            let state = serde_json::to_vec(&serde_json::json!({
                "controlUrl": control, "name": name,
                "nodeKey": member.node_public_key(),
                "discoKey": disco.public_key().to_string(),
            }))
            .unwrap();
            fs::write(dir.join("join.json"), state).unwrap();
            fs::set_permissions(dir.join("join.json"), fs::Permissions::from_mode(0o600)).unwrap();
        }
        async fn connected(runtime: &protectorate_tailnet_runtime::Runtime) {
            let mut state = runtime.subscribe_state();
            tokio::time::timeout(Duration::from_secs(30), async move {
                loop {
                    if matches!(*state.borrow(), State::Connected { .. }) {
                        return;
                    }
                    state.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
        }
        async fn disconnected(runtime: &protectorate_tailnet_runtime::Runtime) {
            let mut state = runtime.subscribe_state();
            tokio::time::timeout(Duration::from_secs(15), async move {
                loop {
                    if *state.borrow() == State::Disconnected {
                        return;
                    }
                    state.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
        }

        let binary = env::var("PROT_HEADSCALE_BINARY").expect("set PROT_HEADSCALE_BINARY");
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let control_addr = free_addr();
        let derp_addr = free_addr();
        let stun_addr = free_addr();
        let control = format!("http://localhost:{}", control_addr.port());
        let mut config = Config::for_server(&control);
        config.listen = control_addr.to_string();
        config.grpc_listen = free_addr().to_string();
        config.metrics_listen = free_addr().to_string();
        config.derp_listen = derp_addr.to_string();
        config.stun_listen = stun_addr.to_string();
        config.derp_ipv4 = Some("127.0.0.1".into());
        config.save(home).unwrap();
        let relay = protectorate_derp::Service::start(protectorate_derp::Config {
            listen: derp_addr,
            stun_listen: stun_addr,
            hostname: "localhost".into(),
            verify_url: format!("http://127.0.0.1:{}/verify", control_addr.port()),
            state_dir: home.join("tailnet"),
        })
        .unwrap();
        let relay_pin = relay.certificate_pin.clone();
        config
            .write_files(home, Some(&relay.certificate_pin))
            .unwrap();
        let headscale = ChildGuard(
            Command::new(&binary)
                .args(["serve", "-c"])
                .arg(home.join("tailnet/headscale.yaml"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if Admin::new(&control, "readiness", Duration::from_secs(1))
                .unwrap()
                .health()
                .is_ok()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Headscale did not become healthy"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let output = Command::new(&binary)
            .arg("-c")
            .arg(home.join("tailnet/headscale.yaml"))
            .args(["apikeys", "create", "--expiration", "1d"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let key = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap()
            .to_owned();
        let mut admin = Admin::new(&control, &key, Duration::from_secs(10)).unwrap();
        admin.ensure_policy_user().unwrap();
        join(home, "hub", &control, &admin, TAG_HUB);
        join(home, "node", &control, &admin, TAG_NODE);

        let hub_runtime =
            Arc::new(protectorate_tailnet_runtime::Runtime::start(home.join("hub"), 7470).unwrap());
        let node_jobs = protectorate_node::Jobs::new(home.join("node-jobs"), 1)
            .unwrap()
            .with_plain_mode();
        let node_files = home.join("node-files");
        let agent = protectorate_node::Agent::new(Arc::new(Metrics), "local-secret")
            .with_jobs(node_jobs)
            .with_files(&node_files, false)
            .unwrap();
        let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let run_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attach_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_requests = Arc::clone(&observed);
        let run_requests = Arc::clone(&run_seen);
        let attach_requests = Arc::clone(&attach_seen);
        let router = agent
            .router_trusted_tailnet()
            .unwrap()
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let observed = Arc::clone(&observed_requests);
                    let run_seen = Arc::clone(&run_requests);
                    let attach_seen = Arc::clone(&attach_requests);
                    async move {
                        assert!(
                            request
                                .headers()
                                .get(axum::http::header::AUTHORIZATION)
                                .is_none(),
                            "local proxy bearer reached the node"
                        );
                        observed.fetch_add(1, Ordering::SeqCst);
                        if request.uri().path().ends_with("/RunJob") {
                            run_seen.fetch_add(1, Ordering::SeqCst);
                        }
                        if request.uri().path().ends_with("/AttachJob") {
                            attach_seen.fetch_add(1, Ordering::SeqCst);
                        }
                        next.run(request).await
                    }
                },
            ));
        let node_http = transport::TailnetHttp::start_node(
            &home.join("node"),
            7475,
            transport::peer_router(router),
        )
        .unwrap();
        connected(&hub_runtime).await;
        connected(&node_http.runtime()).await;
        let proxy = Arc::new(NodeProxy {
            runtime: Arc::clone(&hub_runtime),
            handle: tokio::runtime::Handle::current(),
            endpoints: std::sync::Mutex::new(std::collections::HashMap::new()),
            secret: protectorate_connect_client::virtual_proxy_secret().unwrap(),
        });
        let url = proxy.endpoint("node", "node").unwrap();
        assert!(url.starts_with("tailnet+http://127.0.0.1:"));
        let endpoint = url.strip_prefix("tailnet+").unwrap().to_owned();
        let metrics = tokio::task::spawn_blocking(move || {
            let client = protectorate_connect_client::Client::new_virtual(
                &endpoint,
                Duration::from_secs(20),
            )
            .unwrap();
            let first = client
                .call(
                    "/protectorate.v1.NodeAgentService/GetMetrics",
                    &serde_json::json!({}),
                )
                .unwrap();
            let second = client
                .call(
                    "/protectorate.v1.NodeAgentService/GetMetrics",
                    &serde_json::json!({}),
                )
                .unwrap();
            (first, second)
        })
        .await
        .unwrap();
        assert!(metrics.0.is_object() && metrics.1.is_object());
        assert_eq!(observed.load(Ordering::SeqCst), 2);

        async fn hub_call(
            addr: SocketAddr,
            method: &str,
            body: serde_json::Value,
        ) -> serde_json::Value {
            let method = format!("/protectorate.v1.JobService/{method}");
            tokio::task::spawn_blocking(move || {
                let client = protectorate_connect_client::Client::new(
                    &format!("http://{addr}"),
                    "owner-secret",
                    Duration::from_secs(10),
                )
                .unwrap();
                client.call(&method, &body).unwrap()
            })
            .await
            .unwrap()
        }
        async fn file_call(
            addr: SocketAddr,
            method: &str,
            body: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            let method = format!("/protectorate.v1.NodeService/{method}");
            tokio::task::spawn_blocking(move || {
                let client = protectorate_connect_client::Client::new(
                    &format!("http://{addr}"),
                    "owner-secret",
                    Duration::from_secs(20),
                )
                .map_err(|error| error.to_string())?;
                client
                    .call(&method, &body)
                    .map_err(|error| error.to_string())
            })
            .await
            .unwrap()
        }
        let hub =
            protectorate_hub::Hub::open(home.join("hub-state"), "owner-secret", "test").unwrap();
        hub.inventory
            .add(
                Node {
                    name: "node".into(),
                    address: "node".into(),
                    tier: CARE.into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        let route_proxy = Arc::clone(&proxy);
        let hub = hub
            .with_node_token("local-secret".into())
            .unwrap()
            .with_node_route(Arc::new(move |name, address| {
                route_proxy.endpoint(name, address)
            }));
        let hub_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hub_addr = hub_listener.local_addr().unwrap();
        let hub_server = tokio::spawn(async move {
            axum::serve(hub_listener, hub.router().unwrap())
                .await
                .unwrap()
        });
        let release_job = home.join("release-running-job");
        let command = format!(
            "printf start; i=0; while [ ! -e {} ] && [ \"$i\" -lt 1200 ]; do sleep 0.1; i=$((i+1)); done; [ -e {} ] || exit 1; printf finish",
            release_job.display(),
            release_job.display()
        );
        let submission = serde_json::json!({
            "requestId": "real-derp-outage-job-001",
            "target": {"node": "node"},
            "command": ["/bin/sh", "-c", command],
            "timeout": "150s"
        });
        let submitted = hub_call(hub_addr, "SubmitJob", submission.clone()).await;
        let job_id = submitted["id"].as_str().unwrap().to_owned();
        let log = home
            .join("hub-state/jobs-rust")
            .join(format!("{job_id}.log"));
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if fs::read(&log).is_ok_and(|bytes| bytes == b"start") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("running job did not persist its first output");
        assert_eq!(run_seen.load(Ordering::SeqCst), 1);
        assert_eq!(attach_seen.load(Ordering::SeqCst), 0);

        let chunk_size = 4 << 20;
        let payload: Vec<u8> = (0..chunk_size + 32768)
            .map(|index| ((index * 37 + index / 251) % 256) as u8)
            .collect();
        let expected_hash = format!("{:x}", Sha256::digest(&payload));
        let transfer_id = "real-derp-outage-file-001";
        let file_name = "through-relay.bin";
        let first_chunk = &payload[..chunk_size];
        let second_chunk = &payload[chunk_size..];
        let first_file = file_call(
            hub_addr,
            "PutFile",
            serde_json::json!({
                "name":"node", "transferId":transfer_id, "path":file_name,
                "kind":"FILE_KIND_FILE", "mode":384, "offset":"0",
                "chunk":{"data":STANDARD.encode(first_chunk), "rawSize":first_chunk.len()}
            }),
        )
        .await
        .expect("first file chunk should reach the node through the hub");
        assert_eq!(first_file["received"], chunk_size.to_string());
        assert!(!node_files.join(file_name).exists());

        drop(relay);
        disconnected(&hub_runtime).await;
        disconnected(&node_http.runtime()).await;
        assert!(proxy.endpoint("node", "node").is_err());
        let second_request = serde_json::json!({
            "name":"node", "transferId":transfer_id, "offset":chunk_size.to_string(),
            "chunk":{"data":STANDARD.encode(second_chunk), "rawSize":second_chunk.len()}
        });
        let failed_chunk = tokio::time::timeout(
            Duration::from_secs(30),
            file_call(hub_addr, "PutFile", second_request.clone()),
        )
        .await
        .expect("file request did not fail during relay outage");
        assert!(failed_chunk.is_err(), "relay outage accepted a file chunk");
        assert!(!node_files.join(file_name).exists());
        let during_outage = hub_call(hub_addr, "GetJob", serde_json::json!({"id": job_id})).await;
        assert_eq!(
            during_outage["state"], "JOB_STATE_RUNNING",
            "{during_outage}"
        );
        let relay = protectorate_derp::Service::start(protectorate_derp::Config {
            listen: derp_addr,
            stun_listen: stun_addr,
            hostname: "localhost".into(),
            verify_url: format!("http://127.0.0.1:{}/verify", control_addr.port()),
            state_dir: home.join("tailnet"),
        })
        .unwrap();
        assert_eq!(relay.certificate_pin, relay_pin);
        connected(&hub_runtime).await;
        connected(&node_http.runtime()).await;
        let recovered = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Ok(url) = proxy.endpoint("node", "node") {
                    return url;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(recovered, url);
        let retried_chunk = file_call(hub_addr, "PutFile", second_request)
            .await
            .expect("file chunk should resume after relay recovery");
        assert_eq!(retried_chunk["received"], payload.len().to_string());
        assert!(!node_files.join(file_name).exists());
        let finished_file = file_call(
            hub_addr,
            "PutFile",
            serde_json::json!({
                "name":"node", "transferId":transfer_id,
                "offset":payload.len().to_string(), "last":true,
                "trailer":{"size":payload.len().to_string(),"sha256":expected_hash}
            }),
        )
        .await
        .expect("verified trailer should publish the file");
        assert_eq!(finished_file["done"], true);
        assert_eq!(finished_file["received"], payload.len().to_string());
        assert_eq!(fs::read(node_files.join(file_name)).unwrap(), payload);
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(node_files.join(file_name)).unwrap())
            ),
            expected_hash
        );
        let duplicate = file_call(
            hub_addr,
            "PutFile",
            serde_json::json!({
                "name":"node", "transferId":transfer_id,
                "offset":payload.len().to_string(), "last":true,
                "trailer":{"size":payload.len().to_string(),"sha256":expected_hash}
            }),
        )
        .await;
        assert!(duplicate.is_err(), "duplicate final frame published again");
        let pulled = tokio::task::spawn_blocking(move || {
            let client = protectorate_connect_client::Client::new(
                &format!("http://{hub_addr}"),
                "owner-secret",
                Duration::from_secs(30),
            )
            .unwrap();
            let mut bytes = Vec::new();
            let mut trailer = None;
            client
                .stream(
                    "/protectorate.v1.NodeService/GetFile",
                    &serde_json::json!({"name":"node","path":file_name,"noCompress":true}),
                    |part| {
                        if let Some(chunk) = part.get("chunk") {
                            bytes.extend(STANDARD.decode(chunk["data"].as_str().unwrap()).unwrap());
                        }
                        if let Some(end) = part.get("trailer") {
                            trailer = Some(end.clone());
                        }
                        Ok(())
                    },
                )
                .unwrap();
            (bytes, trailer.unwrap())
        })
        .await
        .unwrap();
        assert_eq!(pulled.0, payload);
        assert_eq!(pulled.1["sha256"], expected_hash);
        fs::write(&release_job, b"go").unwrap();
        let endpoint = recovered.strip_prefix("tailnet+").unwrap().to_owned();
        let resumed = tokio::task::spawn_blocking(move || {
            protectorate_connect_client::Client::new_virtual(&endpoint, Duration::from_secs(20))
                .unwrap()
                .call(
                    "/protectorate.v1.NodeAgentService/GetMetrics",
                    &serde_json::json!({}),
                )
                .unwrap()
        })
        .await
        .unwrap();
        assert!(resumed.is_object());
        let finished = tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let job = hub_call(hub_addr, "GetJob", serde_json::json!({"id": job_id})).await;
                if matches!(
                    job["state"].as_str(),
                    Some("JOB_STATE_SUCCEEDED" | "JOB_STATE_FAILED" | "JOB_STATE_TIMED_OUT")
                ) {
                    return job;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("running job did not finish after DERP recovery");
        assert_eq!(finished["state"], "JOB_STATE_SUCCEEDED", "{finished}");
        assert_eq!(finished["logBytes"], "11");
        assert_eq!(fs::read(&log).unwrap(), b"startfinish");
        assert_eq!(run_seen.load(Ordering::SeqCst), 1);
        assert!(attach_seen.load(Ordering::SeqCst) >= 1);
        let replay = hub_call(hub_addr, "SubmitJob", submission).await;
        assert_eq!(replay["id"], job_id);
        assert!(observed.load(Ordering::SeqCst) >= 3);
        hub_server.abort();
        let (node_stop, ()) = tokio::join!(node_http.stop(), hub_runtime.shutdown());
        node_stop.unwrap();
        drop(relay);
        drop(headscale);
    }
}
