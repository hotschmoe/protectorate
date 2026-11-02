//! `$PROTECTORATE_HOME`: the one root the hub owns (SPEC 6b), its config and its tokens.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use pt_core::SleeveId;

#[derive(Clone, Debug)]
pub struct Home {
    pub root: PathBuf,
}

impl Home {
    pub fn journal(&self) -> PathBuf {
        self.root.join("journal")
    }
    pub fn sleeves(&self) -> PathBuf {
        self.root.join("sleeves")
    }
    pub fn sleeve_dir(&self, id: SleeveId) -> PathBuf {
        self.sleeves().join(id.0.to_string())
    }
    pub fn sleeve_home(&self, id: SleeveId) -> PathBuf {
        self.sleeve_dir(id).join("home")
    }
    pub fn transcript(&self, id: SleeveId) -> PathBuf {
        self.sleeve_dir(id).join("transcript.jsonl")
    }
    pub fn envoy(&self) -> PathBuf {
        self.root.join("envoy")
    }
    pub fn worktrees(&self) -> PathBuf {
        self.root.join("worktrees")
    }
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }
    /// The owner's socket: never visible inside a sandbox.
    pub fn socket(&self) -> PathBuf {
        self.run().join("api.sock")
    }
    /// Bound read-only into every sandbox: the CLI link.
    pub fn bin(&self) -> PathBuf {
        self.run().join("bin")
    }
    /// Bound read-only into every sandbox (a directory, so a socket re-created by a restarted
    /// hub stays reachable from sleeves that outlived it).
    pub fn sleeve_run(&self) -> PathBuf {
        self.run().join("sleeves")
    }
    /// The sleeves' socket: a sleeve capability is required on it.
    pub fn sleeve_socket(&self) -> PathBuf {
        self.sleeve_run().join("api.sock")
    }
    /// The event stream's kept messages (`stream.rs`).
    pub fn stream(&self) -> PathBuf {
        self.root.join("stream.log")
    }
    pub fn tokens(&self) -> PathBuf {
        self.root.join("tokens")
    }
    pub fn config(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn devices(&self) -> PathBuf {
        self.root.join("devices.json")
    }
    pub fn doctor(&self) -> PathBuf {
        self.root.join("doctor.json")
    }
    /// The default tailnet dir: headscale's state, the hub member's keys, enrollment records.
    pub fn tailnet(&self) -> PathBuf {
        self.root.join("tailnet")
    }
    /// The embedded headscale and member, unpacked under their SHA-256.
    pub fn net(&self) -> PathBuf {
        self.root.join("net")
    }
    /// Each node job's log and artifacts as the hub fetched them.
    pub fn jobs(&self) -> PathBuf {
        self.root.join("jobs")
    }
    /// The default history index directory.
    pub fn history(&self) -> PathBuf {
        self.root.join("history")
    }
    /// The hub's own harness logins, shared by every sleeve (`logins.rs`).
    pub fn logins(&self) -> PathBuf {
        self.root.join("logins")
    }
    /// The last run of each harness's own update command.
    pub fn harness_updates(&self) -> PathBuf {
        self.root.join("harness-updates.json")
    }

    /// Creates the layout (mode 0700 at the root) and canonicalizes the root.
    pub fn prepare(root: &Path) -> io::Result<Home> {
        let ctx = |what: &Path, e: io::Error| {
            io::Error::new(e.kind(), format!("prepare {}: {e}", what.display()))
        };
        fs::create_dir_all(root).map_err(|e| ctx(root, e))?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| ctx(root, e))?;
        let root = fs::canonicalize(root).map_err(|e| ctx(root, e))?;
        let home = Home { root };
        for d in [
            home.journal(),
            home.sleeves(),
            home.envoy(),
            home.worktrees(),
            home.run(),
            home.bin(),
            home.sleeve_run(),
            home.jobs(),
        ] {
            fs::create_dir_all(&d).map_err(|e| ctx(&d, e))?;
        }
        Ok(home)
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut b = [0u8; N];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| io::Error::new(e.kind(), format!("read /dev/urandom: {e}")))?;
    Ok(b)
}

pub fn random_hex(n: usize) -> io::Result<String> {
    let b = random_bytes::<32>()?;
    Ok(pt_hash::to_hex(&b)[..n.min(64)].to_string())
}

/// Writes `bytes` to a new file at `path` with mode 0600 through a temporary sibling and rename.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let ctx = |e: io::Error| io::Error::new(e.kind(), format!("write {}: {e}", path.display()));
    let tmp = path.with_extension("tmp");
    let mut f = fs::OpenOptions::new()
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

pt_json::record! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct TargetConf {
        pub harness: String = "pi_local".to_string(),
        pub provider: String = String::new(),
        pub model: String = String::new(),
    }

    /// The `fleet` section: the tailnet this hub runs (SPEC 3.2). `serve --fleet-*` flags
    /// override each key.
    #[derive(Debug, Clone, PartialEq)]
    pub struct FleetConf {
        /// headscale's listener, the hub's LAN address; empty turns the fleet off.
        pub listen: String = String::new(),
        /// The tailnet dir; empty means `<home>/tailnet`. Point it at an existing one to adopt it.
        pub dir: String = String::new(),
        /// The hub member's WireGuard UDP port; 0 picks one.
        pub udp_port: u16 = 0,
        /// The control URL nodes use; empty means `http://<listen>` (LAN-direct, plain DERP).
        pub server_url: String = String::new(),
        /// With an `https` server URL: the IPv4 the DERP map pins for its host (the proxy's).
        pub derp_ipv4: String = String::new(),
        /// Proxies whose X-Forwarded-For headscale trusts.
        pub trusted_proxies: Vec<String> = Vec::new(),
        /// The hub member's tailnet hostname.
        pub hub_name: String = "hub".to_string(),
        /// How often the poller re-reads the node list and each node's presence.
        pub poll_secs: u64 = 15,
    }

    /// `config.json`. Every key is optional; a missing one takes the default below.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Config {
        pub claude_program: String = "claude".to_string(),
        pub codex_program: String = "codex".to_string(),
        pub grok_program: String = "grok".to_string(),
        pub pi_program: String = "pi".to_string(),
        /// Empty means the harness's own default.
        pub claude_model: String = String::new(),
        pub codex_model: String = String::new(),
        pub grok_model: String = String::new(),
        pub pi_vendor_provider: String = String::new(),
        pub pi_vendor_model: String = String::new(),
        /// The local OpenAI-compatible endpoint, as a provider in Pi's own models.json, and the
        /// model to ask it for. While either is empty, doctor finds `pi_local` ineligible, so
        /// nothing spawns or resleeves onto it.
        pub pi_local_provider: String = String::new(),
        pub pi_local_model: String = String::new(),
        /// The same endpoint's base URL (with or without `/v1`), read for its health as the
        /// local quota with that provider's API key. Empty: the provider's own `baseUrl`.
        pub local_url: String = String::new(),
        /// How often the quota readers run (each reading is kept 5 min); 0 never reads.
        pub quota_poll_secs: u64 = 60,
        /// The history index (SPEC 3.7). It is derived and holds log text, so keep it private
        /// and out of backups. Empty means `<home>/history`.
        pub history_dir: String = String::new(),
        /// Absolute directory new project roots may be created under
        /// (`sleeve spawn --new-path`). Empty turns that off.
        pub projects_dir: String = String::new(),
        pub envoy: TargetConf = TargetConf {
            harness: "claude".to_string(),
            provider: String::new(),
            model: String::new(),
        },
        pub default_harness: String = "claude".to_string(),
        pub resleeve_to: Vec<String> = vec!["pi_local".to_string()],
        /// Harnesses (`claude`, `codex`, `grok`) whose headless sleeves run supervised: each
        /// tool approval goes to the owner. Empty: every sleeve runs unsupervised.
        pub supervised: Vec<String> = Vec::new(),
        /// Absolute dirs added to a sleeve's PATH when they exist and the hub's own PATH lacks
        /// them, for toolchains the hub service does not see (Go, Flutter). `~/.cargo/bin` is
        /// always considered.
        pub sleeve_path: Vec<String> = Vec::new(),
        /// Run each incarnation in a systemd user scope (limits and cgroup.kill).
        pub scopes: bool = true,
        /// D9: sleeves Envoy may spawn in one turn an agent message started before each further
        /// spawn needs the owner's approval.
        pub d9_spawn_limit: u64 = 3,
        /// D9: how long a guarded call waits for the owner before it fails.
        pub d9_approval_timeout_secs: u64 = 900,
        /// How often each installed harness's own update command runs (D33); 0 never runs it
        /// on a schedule (`protectorate harness update` still does).
        pub harness_update_hours: u64 = 24,
        /// Where hub and node updates come from (SPEC 6a): the GitHub API (or a loopback
        /// `http://` stand-in) and the repo.
        pub release_api: String = "https://api.github.com".to_string(),
        pub release_repo: String = "hotschmoe/protectorate".to_string(),
        /// `stable` or `dev`: what `hub update` and `node update` take without `--version`.
        pub release_channel: String = "stable".to_string(),
        pub fleet: FleetConf = FleetConf {
            listen: String::new(),
            dir: String::new(),
            udp_port: 0,
            server_url: String::new(),
            derp_ipv4: String::new(),
            trusted_proxies: Vec::new(),
            hub_name: "hub".to_string(),
            poll_secs: 15,
        },
    }
}

impl Config {
    /// Reads `config.json`, writing the defaults first when it does not exist.
    pub fn load(home: &Home) -> io::Result<Config> {
        let path = home.config();
        match fs::read(&path) {
            Ok(bytes) => pt_json::from_bytes(&bytes).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let c: Config = pt_json::from_str("{}").expect("defaults parse");
                let text = pt_json::ToJson::to_json(&c).to_string();
                write_private(&path, text.as_bytes())?;
                Ok(c)
            }
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("read {}: {e}", path.display()),
            )),
        }
    }
}

pt_json::record! {
    #[derive(Debug, Clone)]
    struct TokensFile {
        secret: String,
        owner_sha256: String,
        hub_tag: String,
    }
}

/// The hub's secrets. Capabilities are derived from `secret`, so only the owner token's hash is
/// stored and nothing else needs a table.
pub struct Tokens {
    secret: String,
    owner_sha256: String,
    /// A short random tag in every scope name, so two hubs on one machine never collide.
    pub hub_tag: String,
}

pub const OWNER_PREFIX: &str = "po1.";
pub const CAP_PREFIX: &str = "pc1.";

pub fn sha_hex(s: &str) -> String {
    pt_hash::to_hex(&pt_hash::sha256(s.as_bytes()))
}

impl Tokens {
    /// Loads `tokens`, or creates it and returns the new owner token, which is shown once.
    pub fn load_or_create(home: &Home) -> io::Result<(Tokens, Option<String>)> {
        let path = home.tokens();
        match fs::read(&path) {
            Ok(bytes) => {
                let f: TokensFile = pt_json::from_bytes(&bytes).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}: {e}", path.display()),
                    )
                })?;
                Ok((
                    Tokens {
                        secret: f.secret,
                        owner_sha256: f.owner_sha256,
                        hub_tag: f.hub_tag,
                    },
                    None,
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let owner = format!("{OWNER_PREFIX}{}", random_hex(48)?);
                let t = Tokens {
                    secret: random_hex(64)?,
                    owner_sha256: sha_hex(&owner),
                    hub_tag: format!("h{}", random_hex(6)?),
                };
                let f = TokensFile {
                    secret: t.secret.clone(),
                    owner_sha256: t.owner_sha256.clone(),
                    hub_tag: t.hub_tag.clone(),
                };
                write_private(&path, pt_json::ToJson::to_json(&f).to_string().as_bytes())?;
                Ok((t, Some(owner)))
            }
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("read {}: {e}", path.display()),
            )),
        }
    }

    fn derive(&self, what: &str) -> String {
        format!(
            "{CAP_PREFIX}{}",
            &sha_hex(&format!("{}:{what}", self.secret))[..48]
        )
    }

    /// The `pc1.` capability of one sleeve, handed to it in `PROTECTORATE_TOKEN`.
    pub fn sleeve(&self, id: SleeveId) -> String {
        self.derive(&format!("sleeve:{}", id.0))
    }

    /// The capability vendor general agents use: Envoy methods only (D9).
    pub fn envoy_only(&self) -> String {
        self.derive("envoy-only")
    }

    pub fn is_owner(&self, token: &str) -> bool {
        token.starts_with(OWNER_PREFIX) && sha_hex(token) == self.owner_sha256
    }
}
