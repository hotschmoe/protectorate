//! Headscale configuration and ACL policy for Protectorate's Linux tailnet.
//! Runtime supervision, enrollment and the member transport build on these
//! validated files; no Go state or Tailscale wire format is exposed here.

use http::Uri;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

mod runtime;
pub use runtime::{StopToken, Supervisor, SupervisorTiming};

pub const HEADSCALE_VERSION: &str = "0.29.5";

pub const HUB_API_PORT: u16 = 7470;
pub const NODE_PORT: u16 = 7475;
pub const TAG_HUB: &str = "tag:hub";
pub const TAG_NODE: &str = "tag:node";
pub const TAG_CLIENT: &str = "tag:client";
const POLICY_OWNER: &str = "protectorate@";
const PUBLIC_DERP_MAP: &str = "https://controlplane.tailscale.com/derpmap/default";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    pub server_url: String,
    pub listen: String,
    pub grpc_listen: String,
    pub metrics_listen: String,
    pub base_domain: String,
    pub hub_name: String,
    pub embedded_derp: bool,
    pub derp_listen: String,
    pub stun_listen: String,
    pub derp_ipv4: Option<String>,
    pub public_derp: bool,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    pub key_expiry_days: u16,
    pub pools: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            listen: "0.0.0.0:7480".into(),
            grpc_listen: "127.0.0.1:7482".into(),
            metrics_listen: "127.0.0.1:7481".into(),
            base_domain: "tn.protectorate.internal".into(),
            hub_name: "protectorate-hub".into(),
            embedded_derp: true,
            derp_listen: "0.0.0.0:7483".into(),
            stun_listen: "0.0.0.0:3478".into(),
            derp_ipv4: None,
            public_derp: false,
            tls_cert: None,
            tls_key: None,
            key_expiry_days: 180,
            pools: Vec::new(),
        }
    }
}

impl Config {
    pub fn for_server(server_url: impl Into<String>) -> Self {
        Self {
            server_url: server_url.into(),
            ..Self::default()
        }
    }

    pub fn load(home: &Path) -> Result<Self> {
        let path = home.join("tailnet/config.json");
        let data =
            fs::read(&path).map_err(|error| Error(format!("read {}: {error}", path.display())))?;
        let config: Self = serde_json::from_slice(&data)
            .map_err(|error| Error(format!("parse {}: {error}", path.display())))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, home: &Path) -> Result<()> {
        self.validate()?;
        let mut data = serde_json::to_vec_pretty(self)
            .map_err(|error| Error(format!("serialize tailnet config: {error}")))?;
        data.push(b'\n');
        write_private(&home.join("tailnet/config.json"), &data)
    }

    /// Regenerate files consumed by Headscale. The private DERP map needs the
    /// certificate pin from the separately running relay.
    pub fn write_files(&self, home: &Path, derp_cert_pin: Option<&str>) -> Result<()> {
        let config = self.headscale_config(home)?;
        let policy = self.policy()?;
        let map = if self.embedded_derp {
            Some(
                self.private_derp_map(
                    derp_cert_pin
                        .ok_or_else(|| Error("private DERP certificate pin is required".into()))?,
                )?,
            )
        } else {
            None
        };
        let mut config_bytes = serde_json::to_vec_pretty(&config)
            .map_err(|error| Error(format!("serialize Headscale config: {error}")))?;
        config_bytes.push(b'\n');
        let mut policy_bytes = serde_json::to_vec_pretty(&policy)
            .map_err(|error| Error(format!("serialize Headscale policy: {error}")))?;
        policy_bytes.push(b'\n');
        let dir = home.join("tailnet");
        write_private(&dir.join("headscale.yaml"), &config_bytes)?;
        write_private(&dir.join("policy.json"), &policy_bytes)?;
        if let Some(map) = map {
            write_private(&dir.join("derpmap.json"), map.as_bytes())?;
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let url = self
            .server_url
            .parse::<Uri>()
            .map_err(|_| Error("server_url must be an HTTP or HTTPS URL".into()))?;
        if !matches!(url.scheme_str(), Some("http" | "https"))
            || url.host().is_none()
            || url
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
            || url
                .path_and_query()
                .is_some_and(|path| path.as_str() != "/")
        {
            return Err(Error(
                "server_url must be a bare HTTP or HTTPS origin".into(),
            ));
        }
        for (name, address) in [
            ("listen", &self.listen),
            ("grpc_listen", &self.grpc_listen),
            ("metrics_listen", &self.metrics_listen),
        ] {
            address
                .parse::<SocketAddr>()
                .map_err(|_| Error(format!("{name} must be an IP:port address")))?;
        }
        if self.embedded_derp {
            for (name, address) in [
                ("derp_listen", &self.derp_listen),
                ("stun_listen", &self.stun_listen),
            ] {
                address
                    .parse::<SocketAddr>()
                    .map_err(|_| Error(format!("{name} must be an IP:port address")))?;
            }
        }
        if !self.embedded_derp && !self.public_derp {
            return Err(Error("at least one DERP source is required".into()));
        }
        if self.tls_cert.is_some() != self.tls_key.is_some() {
            return Err(Error("tls_cert and tls_key must be set together".into()));
        }
        if self.key_expiry_days == 0 {
            return Err(Error("key_expiry_days must be positive".into()));
        }
        if !valid_label(&self.hub_name) {
            return Err(Error("hub_name must be a lowercase DNS label".into()));
        }
        if !self.base_domain.split('.').all(valid_label) || !self.base_domain.contains('.') {
            return Err(Error("base_domain must be a lowercase DNS name".into()));
        }
        if self.pools.iter().any(|pool| !valid_label(pool)) {
            return Err(Error("pool names must be lowercase DNS labels".into()));
        }
        if let Some(ip) = &self.derp_ipv4 {
            if ip.parse::<std::net::Ipv4Addr>().is_err() {
                return Err(Error("derp_ipv4 must be an IPv4 address".into()));
            }
        }
        Ok(())
    }

    /// Render the pinned Headscale server's JSON-compatible YAML config.
    pub fn headscale_config(&self, home: &Path) -> Result<Value> {
        self.validate()?;
        let dir = home.join("tailnet");
        let mut derp_urls = Vec::new();
        if self.public_derp {
            derp_urls.push(PUBLIC_DERP_MAP);
        }
        let mut derp_paths = Vec::new();
        if self.embedded_derp {
            derp_paths.push(dir.join("derpmap.json").to_string_lossy().into_owned());
        }
        let mut config = json!({
            "server_url":self.server_url,
            "listen_addr":self.listen,
            "grpc_listen_addr":self.grpc_listen,
            "grpc_allow_insecure":false,
            "metrics_listen_addr":self.metrics_listen,
            "noise":{"private_key_path":dir.join("noise_private.key")},
            "prefixes":{"v4":"100.64.0.0/10","v6":"fd7a:115c:a1e0::/48","allocation":"sequential"},
            "derp":{"server":{"enabled":false},"urls":derp_urls,"paths":derp_paths,"auto_update_enabled":self.public_derp,"update_frequency":"3h"},
            "disable_check_updates":true,
            "node":{"expiry":"0"},
            "database":{"type":"sqlite","sqlite":{"path":dir.join("db.sqlite"),"write_ahead_log":true}},
            "log":{"level":"info","format":"text"},
            "policy":{"mode":"file","path":dir.join("policy.json")},
            "dns":{"magic_dns":true,"base_domain":self.base_domain,"override_local_dns":false,"nameservers":{"global":[]}},
            "unix_socket":socket_path(home),
            "unix_socket_permission":"0700",
            "logtail":{"enabled":false},
            "taildrop":{"enabled":false}
        });
        if let (Some(cert), Some(key)) = (&self.tls_cert, &self.tls_key) {
            config["tls_cert_path"] = json!(cert);
            config["tls_key_path"] = json!(key);
        }
        Ok(config)
    }

    /// Keep tag ownership and reachability narrow; unknown tags have no ACL.
    pub fn policy(&self) -> Result<Value> {
        self.validate()?;
        let mut owners = BTreeMap::from([
            (TAG_HUB.to_owned(), vec![POLICY_OWNER]),
            (TAG_NODE.to_owned(), vec![POLICY_OWNER]),
            (TAG_CLIENT.to_owned(), vec![POLICY_OWNER]),
            ("tag:tier-shared".into(), vec![POLICY_OWNER]),
            ("tag:tier-care".into(), vec![POLICY_OWNER]),
        ]);
        let mut acls = vec![
            json!({"action":"accept","src":[TAG_HUB],"dst":[format!("{TAG_NODE}:{NODE_PORT}"),format!("{TAG_NODE}:22")]}),
            json!({"action":"accept","src":[TAG_CLIENT],"dst":[format!("{TAG_HUB}:{HUB_API_PORT}")]}),
        ];
        let pools = self.pools.iter().collect::<BTreeSet<_>>();
        for pool in pools {
            let tag = format!("tag:pool-{pool}");
            owners.insert(tag.clone(), vec![POLICY_OWNER]);
            acls.push(json!({"action":"accept","src":[tag],"dst":[format!("{tag}:*")]}));
        }
        Ok(json!({"tagOwners":owners,"acls":acls}))
    }

    /// Headscale reads this YAML flow document for the separate private DERP
    /// listener. The region key must be an integer, so JSON serialization of
    /// the outer map would give Headscale the wrong shape.
    pub fn private_derp_map(&self, cert_pin: &str) -> Result<String> {
        self.validate()?;
        if !self.embedded_derp {
            return Err(Error("private DERP is disabled".into()));
        }
        let hash = cert_pin
            .strip_prefix("sha256-raw:")
            .ok_or_else(|| Error("DERP certificate pin must use sha256-raw".into()))?;
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error(
                "DERP certificate pin must contain a SHA-256 digest".into(),
            ));
        }
        let host = self
            .server_url
            .parse::<Uri>()
            .map_err(|error| Error(error.to_string()))?
            .host()
            .ok_or_else(|| Error("server_url has no host".into()))?
            .to_owned();
        let derp_port = self
            .derp_listen
            .parse::<SocketAddr>()
            .map_err(|error| Error(error.to_string()))?
            .port();
        let stun_port = self
            .stun_listen
            .parse::<SocketAddr>()
            .map_err(|error| Error(error.to_string()))?
            .port();
        let mut node = json!({
            "name":"999a", "regionid":999, "hostname":host,
            "derpport":derp_port, "stunport":stun_port, "certname":cert_pin
        });
        if let Some(ip) = &self.derp_ipv4 {
            node["ipv4"] = json!(ip);
        }
        let region = json!({
            "regionid":999,
            "regioncode":"protectorate",
            "regionname":"Protectorate DERP",
            "nodes":[node]
        });
        Ok(format!("{{\"regions\": {{999: {region}}}}}\n"))
    }
}

fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error("path has no parent".into()))?;
    ensure_private_dir(parent)?;
    protectorate_fsutil::write_file(path, data, 0o600)
        .map_err(|error| Error(format!("write {}: {error}", path.display())))
}

fn ensure_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).map_err(|error| Error(format!("create {}: {error}", dir.display())))?;
    let kind = fs::symlink_metadata(dir)
        .map_err(|error| Error(format!("inspect {}: {error}", dir.display())))?
        .file_type();
    if !kind.is_dir() || kind.is_symlink() {
        return Err(Error(format!("{} is not a directory", dir.display())));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| Error(format!("secure {}: {error}", dir.display())))?;
    }
    Ok(())
}

pub fn valid_label(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn socket_path(home: &Path) -> PathBuf {
    let path = home.join("tailnet/headscale.sock");
    if path.as_os_str().len() < 100 {
        return path;
    }
    let hash = Sha256::digest(home.as_os_str().as_encoded_bytes());
    let short = hash[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir().join(format!("protectorate-headscale-{short}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_allows_only_the_product_paths_and_deduplicates_pools() {
        let mut config = Config::for_server("https://control.example:7480");
        config.pools = vec!["build".into(), "gpu".into(), "build".into()];
        let policy = config.policy().unwrap();
        assert_eq!(policy["acls"].as_array().unwrap().len(), 4);
        assert_eq!(
            policy["acls"][0],
            json!({"action":"accept","src":[TAG_HUB],"dst":["tag:node:7475","tag:node:22"]})
        );
        assert_eq!(
            policy["acls"][1],
            json!({"action":"accept","src":[TAG_CLIENT],"dst":["tag:hub:7470"]})
        );
        assert_eq!(policy["acls"][2]["src"], json!(["tag:pool-build"]));
        assert_eq!(policy["acls"][3]["src"], json!(["tag:pool-gpu"]));
        assert!(policy["tagOwners"]["tag:pool-build"].is_array());
        assert!(policy["tagOwners"]
            .as_object()
            .unwrap()
            .get("tag:untrusted")
            .is_none());
    }

    #[test]
    fn invalid_origins_and_missing_derp_or_tls_pair_fail_closed() {
        let mut config = Config::for_server("https://control.example:7480/path");
        assert!(config.validate().is_err());
        config.server_url = "https://user:pass@control.example".into();
        assert!(config.validate().is_err());
        config.server_url = "https://control.example:7480".into();
        config.embedded_derp = false;
        assert!(config.validate().is_err());
        config.public_derp = true;
        config.tls_cert = Some("/tmp/cert.pem".into());
        assert!(config.validate().is_err());
        config.tls_key = Some("/tmp/key.pem".into());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn headscale_paths_stay_private_and_socket_fits_unix_limit() {
        let config = Config::for_server("https://control.example:7480");
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("a".repeat(120));
        let value = config.headscale_config(&deep).unwrap();
        assert_eq!(value["grpc_allow_insecure"], false);
        assert_eq!(value["unix_socket_permission"], "0700");
        assert_eq!(
            value["database"]["sqlite"]["path"],
            json!(deep.join("tailnet/db.sqlite"))
        );
        assert_eq!(
            value["derp"]["paths"],
            json!([deep.join("tailnet/derpmap.json")])
        );
        assert!(value["unix_socket"].as_str().unwrap().len() < 100);
    }

    #[test]
    fn private_derp_map_uses_numeric_region_and_pinned_certificate() {
        let mut config = Config::for_server("http://192.168.10.44:7480");
        config.derp_ipv4 = Some("198.51.100.4".into());
        let pin = format!("sha256-raw:{}", "ab".repeat(32));
        let map = config.private_derp_map(&pin).unwrap();
        assert!(map.starts_with("{\"regions\": {999: "));
        assert!(map.contains("\"hostname\":\"192.168.10.44\""));
        assert!(map.contains("\"derpport\":7483"));
        assert!(map.contains("\"stunport\":3478"));
        assert!(map.contains("\"ipv4\":\"198.51.100.4\""));
        assert!(map.contains(&pin));
        assert!(config.private_derp_map("sha256-raw:short").is_err());
    }
}
