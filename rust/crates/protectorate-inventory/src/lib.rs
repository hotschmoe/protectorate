//! Persistent hub node inventory. One hub process owns this store.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const SHARED: &str = "shared";
pub const CARE: &str = "care";

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Exists(String),
    NotFound(String),
    Invited(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "invalid node: {s}"),
            Self::Exists(s) => write!(f, "node already exists: {s}"),
            Self::NotFound(s) => write!(f, "node not found: {s}"),
            Self::Invited(s) => write!(f, "node {s} is invited but has not joined"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub name: String,
    pub address: String,
    pub os: String,
    pub arch: String,
    pub tags: Vec<String>,
    pub ssh_alias: String,
    pub tier: String,
    pub notes: String,
    pub invited: bool,
    /// SHA-256 (hex) of the unredeemed join code and its expiry, Unix seconds.
    pub code_sha256: String,
    pub code_expires: i64,
    /// The identity component of the node's certificate name. Only the
    /// certificate issued with the current id is trusted.
    pub cert_id: String,
    pub cert_expires: i64,
    /// The node's uncompressed P-256 public key, base64, for renewal.
    pub public_key: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    nodes: Vec<Node>,
}

pub struct Inventory {
    path: PathBuf,
    local_name: String,
    local: Node,
    nodes: Mutex<Vec<Node>>,
}
impl Inventory {
    pub fn open(path: impl Into<PathBuf>, local_name: &str) -> Result<Self> {
        Self::open_with_local(
            path,
            Node {
                name: local_name.to_owned(),
                address: "local".into(),
                tier: SHARED.into(),
                ..Node::default()
            },
        )
    }

    pub fn open_with_local(path: impl Into<PathBuf>, local: Node) -> Result<Self> {
        let path = path.into();
        let local = normalize(local)?;
        let local_name = local.name.clone();
        let mut nodes = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<File>(&bytes)?.nodes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let mut seen = BTreeSet::new();
        for node in &mut nodes {
            *node = normalize(node.clone())?;
            if node.name == local_name || !seen.insert(node.name.clone()) {
                return Err(Error::Exists(node.name.clone()));
            }
        }
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            path,
            local_name,
            local,
            nodes: Mutex::new(nodes),
        })
    }

    /// Regenerate the user-facing host summary from the current inventory.
    pub fn write_hosts(&self) -> Result<()> {
        let nodes = self.nodes.lock().unwrap();
        self.write_hosts_locked(&nodes)
    }

    fn write_hosts_locked(&self, nodes: &[Node]) -> Result<()> {
        let path = self.path.with_file_name("hosts.md");
        fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        protectorate_fsutil::write_file(&path, &render_hosts(&self.local, nodes), 0o644)?;
        Ok(())
    }
    pub fn list(&self) -> Vec<Node> {
        self.nodes.lock().unwrap().clone()
    }
    pub fn get(&self, name: &str) -> Result<Node> {
        let name = name.trim().to_ascii_lowercase();
        self.nodes
            .lock()
            .unwrap()
            .iter()
            .find(|n| n.name == name)
            .cloned()
            .ok_or(Error::NotFound(name))
    }
    pub fn remove(&self, name: &str) -> Result<Node> {
        let name = name.trim().to_ascii_lowercase();
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let at = next
            .iter()
            .position(|node| node.name == name)
            .ok_or_else(|| Error::NotFound(name.clone()))?;
        let removed = next.remove(at);
        self.commit(&mut nodes, next)?;
        Ok(removed)
    }
    pub fn add(&self, node: Node, update: bool) -> Result<Node> {
        let mut nodes = self.nodes.lock().unwrap();
        let name = node.name.trim().to_ascii_lowercase();
        let at = nodes.iter().position(|n| n.name == name);
        if at.is_some() && !update {
            return Err(Error::Exists(name));
        }
        let mut node = node;
        // An edit keeps the enrollment and every field it leaves empty; a new
        // invite (it carries a code) replaces the record and its certificate.
        if let Some(old) = at
            .map(|i| &nodes[i])
            .filter(|_| node.code_sha256.is_empty())
        {
            node.code_expires = old.code_expires;
            node.code_sha256 = old.code_sha256.clone();
            node.cert_id = old.cert_id.clone();
            node.cert_expires = old.cert_expires;
            node.public_key = old.public_key.clone();
            for (field, saved) in [
                (&mut node.address, &old.address),
                (&mut node.tier, &old.tier),
                (&mut node.os, &old.os),
                (&mut node.arch, &old.arch),
                (&mut node.ssh_alias, &old.ssh_alias),
                (&mut node.notes, &old.notes),
            ] {
                if field.is_empty() {
                    *field = saved.clone();
                }
            }
        }
        let node = normalize(node)?;
        if node.name == self.local_name {
            return Err(Error::Exists(node.name));
        }
        let mut next = nodes.clone();
        if let Some(at) = at {
            next[at] = node.clone();
        } else {
            next.push(node.clone());
        }
        self.commit(&mut nodes, next)?;
        Ok(node)
    }
    pub fn set_tier(&self, name: &str, tier: &str) -> Result<Node> {
        let tier = tier.trim().to_ascii_lowercase();
        if tier != SHARED && tier != CARE {
            return Err(Error::Invalid(format!(
                "tier {tier:?} must be shared or care"
            )));
        }
        let name = name.trim().to_ascii_lowercase();
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let node = next
            .iter_mut()
            .find(|n| n.name == name)
            .ok_or_else(|| Error::NotFound(name.clone()))?;
        if node.invited {
            return Err(Error::Invited(name));
        }
        node.tier = tier;
        let node = node.clone();
        self.commit(&mut nodes, next)?;
        Ok(node)
    }
    /// Records an invited node's first successful authenticated metrics poll.
    pub fn clear_invited(&self, name: &str) -> Result<()> {
        let name = name.trim().to_ascii_lowercase();
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let Some(node) = next.iter_mut().find(|node| node.name == name) else {
            return Ok(());
        };
        if !node.invited {
            return Ok(());
        }
        node.invited = false;
        self.commit(&mut nodes, next)
    }
    /// Redeems an invited node's join code once: records the certificate
    /// issued for it and forgets the code. NotFound covers every refusal.
    pub fn redeem(&self, name: &str, code_sha256: &str, now: i64, joined: Node) -> Result<Node> {
        let name = name.trim().to_ascii_lowercase();
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let node = next
            .iter_mut()
            .find(|node| {
                node.name == name
                    && node.invited
                    && !node.code_sha256.is_empty()
                    && node.code_sha256 == code_sha256
                    && now < node.code_expires
            })
            .ok_or_else(|| Error::NotFound(name.clone()))?;
        node.address = joined.address;
        node.code_sha256.clear();
        node.code_expires = 0;
        node.cert_id = joined.cert_id;
        node.cert_expires = joined.cert_expires;
        node.public_key = joined.public_key;
        *node = normalize(node.clone())?;
        let node = node.clone();
        self.commit(&mut nodes, next)?;
        Ok(node)
    }
    /// Records a renewed certificate for the node's current enrollment.
    pub fn renewed(&self, name: &str, cert_id: &str, cert_expires: i64) -> Result<()> {
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let Some(node) = next
            .iter_mut()
            .find(|node| node.name == name && node.cert_id == cert_id)
        else {
            return Err(Error::NotFound(name.to_owned()));
        };
        node.cert_expires = cert_expires;
        self.commit(&mut nodes, next)
    }
    /// Caches OS and architecture reported by a successful metrics poll.
    pub fn set_platform(&self, name: &str, os: &str, arch: &str) -> Result<()> {
        let name = name.trim().to_ascii_lowercase();
        let mut nodes = self.nodes.lock().unwrap();
        let mut next = nodes.clone();
        let Some(node) = next.iter_mut().find(|node| node.name == name) else {
            return Ok(());
        };
        if node.os == os && node.arch == arch {
            return Ok(());
        }
        node.os = os.to_owned();
        node.arch = arch.to_owned();
        self.commit(&mut nodes, next)
    }
    fn commit(&self, nodes: &mut Vec<Node>, mut next: Vec<Node>) -> Result<()> {
        next.sort_by(|a, b| a.name.cmp(&b.name));
        let bytes = serde_json::to_vec_pretty(&File {
            nodes: next.clone(),
        })?;
        fs::create_dir_all(self.path.parent().unwrap_or(Path::new(".")))?;
        protectorate_fsutil::write_file(&self.path, &bytes, 0o600)?;
        *nodes = next;
        if let Err(error) = self.write_hosts_locked(nodes) {
            eprintln!("write hosts.md: {error}");
        }
        Ok(())
    }
}

fn normalize(mut node: Node) -> Result<Node> {
    node.name = node.name.trim().to_ascii_lowercase();
    node.address = node.address.trim().into();
    node.ssh_alias = node.ssh_alias.trim().into();
    node.tier = node.tier.trim().to_ascii_lowercase();
    node.notes = node.notes.trim().into();
    if node.tier.is_empty() {
        node.tier = SHARED.into();
    }
    let name = node.name.as_bytes();
    if name.is_empty()
        || name.len() > 63
        || !name[0].is_ascii_lowercase() && !name[0].is_ascii_digit()
        || !name
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(c))
    {
        return Err(Error::Invalid(format!("name {:?}", node.name)));
    }
    let pending = node.invited && node.address.is_empty();
    if !pending && !valid_address(&node.address) {
        return Err(Error::Invalid(format!("address {:?}", node.address)));
    }
    let alias = node.ssh_alias.as_bytes();
    if !alias.is_empty()
        && (alias.len() > 128
            || !alias[0].is_ascii_alphanumeric()
            || !alias
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"._@-".contains(c)))
    {
        return Err(Error::Invalid(format!("ssh alias {:?}", node.ssh_alias)));
    }
    if node.tier != SHARED && node.tier != CARE {
        return Err(Error::Invalid(format!("tier {:?}", node.tier)));
    }
    let mut tags = BTreeSet::new();
    for tag in node.tags {
        let tag = tag.trim().to_ascii_lowercase();
        let b = tag.as_bytes();
        if b.is_empty()
            || b.len() > 32
            || !b[0].is_ascii_lowercase() && !b[0].is_ascii_digit()
            || !b
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(c))
        {
            return Err(Error::Invalid(format!("tag {tag:?}")));
        }
        tags.insert(tag);
    }
    node.tags = tags.into_iter().collect();
    Ok(node)
}

fn valid_address(address: &str) -> bool {
    let a = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    if a.is_empty()
        || a.bytes()
            .any(|b| b.is_ascii_whitespace() || b"/?#@\\".contains(&b))
    {
        return false;
    }
    let (host, port) = if let Some(inner) = a.strip_prefix('[') {
        let Some((host, tail)) = inner.split_once(']') else {
            return false;
        };
        if host.parse::<IpAddr>().is_err() || !host.contains(':') {
            return false;
        }
        if tail.is_empty() {
            (host, None)
        } else if let Some(port) = tail.strip_prefix(':') {
            (host, Some(port))
        } else {
            return false;
        }
    } else if let Some((host, port)) = a.rsplit_once(':') {
        if host.contains(':') {
            return false;
        }
        (host, Some(port))
    } else {
        (a, None)
    };
    if port.is_some_and(|p| p.parse::<u16>().ok().is_none_or(|n| n == 0)) {
        return false;
    }
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    !host.is_empty()
        && !host.contains("..")
        && host.as_bytes()[0].is_ascii_alphanumeric()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn render_hosts(local: &Node, nodes: &[Node]) -> Vec<u8> {
    let mut out = String::from(
        "# Hosts\n\nGenerated by protectorate from nodes.json. Do not edit.\n\n\
| Name | OS/Arch | Address | SSH alias | Tier | Tags | Notes |\n\
|------|---------|---------|-----------|------|------|-------|\n",
    );
    let mut row = |name: &str, node: &Node, address: &str, notes: &str| {
        let platform = if node.os.is_empty() {
            "?".to_owned()
        } else {
            format!("{}/{}", node.os, node.arch)
        };
        let tags = node.tags.join(", ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            cell(name),
            cell(&platform),
            cell(address),
            cell(or_dash(&node.ssh_alias)),
            cell(&node.tier),
            cell(or_dash(&tags)),
            cell(or_dash(notes)),
        ));
    };
    row(
        &format!("{} (this machine, hub)", local.name),
        local,
        "local",
        &local.notes,
    );
    for node in nodes {
        let notes = if node.invited {
            format!("invited, not yet joined. {}", node.notes)
                .trim()
                .to_owned()
        } else {
            node.notes.clone()
        };
        row(&node.name, node, &node.address, &notes);
    }
    out.push_str("\n- Reach a node with `ssh <SSH alias>` when one is set.\n");
    out.push_str("- Tiers: `shared` (the compute pool: jobs and leases, sandboxed), `care` (a machine kept for a person: never scheduled or leased).\n");
    out.push_str("- Live CPU, GPU, memory and disk per node: `protectorate node ls`, `protectorate node show <name>`.\n");
    out.into_bytes()
}

fn cell(value: &str) -> String {
    value.replace('|', "/").replace(['\r', '\n'], " ")
}

fn or_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_persistent_inventory_and_failed_write_is_not_visible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.json");
        let store = Inventory::open(&path, "hub").unwrap();
        let node = store
            .add(
                Node {
                    name: " Peer ".into(),
                    address: "peer:7475".into(),
                    tags: vec![" GPU ".into(), "gpu".into()],
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        assert_eq!(node.name, "peer");
        assert_eq!(node.tags, ["gpu"]);
        assert_eq!(node.tier, SHARED);
        assert!(matches!(
            store.add(node.clone(), false),
            Err(Error::Exists(_))
        ));
        assert!(matches!(
            store.add(
                Node {
                    name: "hub".into(),
                    address: "hub".into(),
                    ..Node::default()
                },
                false
            ),
            Err(Error::Exists(_))
        ));
        assert!(matches!(
            store.add(
                Node {
                    name: "bad".into(),
                    address: "http://host/path".into(),
                    ..Node::default()
                },
                false
            ),
            Err(Error::Invalid(_))
        ));
        assert_eq!(Inventory::open(&path, "hub").unwrap().list(), store.list());
        // A valid directory at the target causes rename to fail; keep memory unchanged.
        let target = dir.path().join("blocked");
        fs::create_dir(&target).unwrap();
        let blocked = Inventory {
            path: target,
            local_name: "hub".into(),
            local: Node {
                name: "hub".into(),
                address: "local".into(),
                tier: SHARED.into(),
                ..Node::default()
            },
            nodes: Mutex::new(store.list()),
        };
        assert!(blocked.set_tier("peer", CARE).is_err());
        assert_eq!(blocked.get("peer").unwrap().tier, SHARED);
    }
    #[test]
    fn corrupt_or_duplicated_state_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.json");
        fs::write(&path, b"{broken").unwrap();
        assert!(matches!(Inventory::open(&path, "hub"), Err(Error::Json(_))));
        fs::write(
            &path,
            br#"{"nodes":[{"name":"a","address":"a"},{"name":"A","address":"a"}]}"#,
        )
        .unwrap();
        assert!(matches!(
            Inventory::open(&path, "hub"),
            Err(Error::Exists(_))
        ));
    }
    #[test]
    fn remove_is_persistent_and_reports_missing_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.json");
        let store = Inventory::open(&path, "hub").unwrap();
        store
            .add(
                Node {
                    name: "peer".into(),
                    address: "peer:7475".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        assert_eq!(store.remove(" PEER ").unwrap().name, "peer");
        assert!(matches!(store.get("peer"), Err(Error::NotFound(_))));
        assert!(Inventory::open(&path, "hub").unwrap().list().is_empty());
        assert!(matches!(store.remove("peer"), Err(Error::NotFound(_))));
    }

    #[test]
    fn hosts_render_sorted_local_first_and_recover_after_failed_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.json");
        let hosts = dir.path().join("hosts.md");
        let store = Inventory::open_with_local(
            &path,
            Node {
                name: "hub".into(),
                address: "local".into(),
                os: "linux".into(),
                arch: "amd64".into(),
                tier: SHARED.into(),
                ..Node::default()
            },
        )
        .unwrap();
        store.write_hosts().unwrap();
        store
            .add(
                Node {
                    name: "zulu".into(),
                    address: "zulu:7475".into(),
                    invited: true,
                    tags: vec!["gpu".into()],
                    notes: "one|two\nnext".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        store
            .add(
                Node {
                    name: "alpha".into(),
                    address: "alpha:7475".into(),
                    tier: CARE.into(),
                    ssh_alias: "alpha-ssh".into(),
                    ..Node::default()
                },
                false,
            )
            .unwrap();
        let text = fs::read_to_string(&hosts).unwrap();
        assert!(text
            .starts_with("# Hosts\n\nGenerated by protectorate from nodes.json. Do not edit.\n"));
        let local = text
            .find("| hub (this machine, hub) | linux/amd64 | local | - | shared | - | - |")
            .unwrap();
        let alpha = text
            .find("| alpha | ? | alpha:7475 | alpha-ssh | care | - | - |")
            .unwrap();
        let zulu = text.find("| zulu | ? | zulu:7475 | - | shared | gpu | invited, not yet joined. one/two next |").unwrap();
        assert!(local < alpha && alpha < zulu);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&hosts).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }

        fs::remove_file(&hosts).unwrap();
        fs::create_dir(&hosts).unwrap();
        store.clear_invited("zulu").unwrap();
        assert!(!store.get("zulu").unwrap().invited);
        assert!(
            !Inventory::open(&path, "hub")
                .unwrap()
                .get("zulu")
                .unwrap()
                .invited
        );
        assert!(hosts.is_dir());
        assert!(store.write_hosts().is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        fs::remove_dir(&hosts).unwrap();
        store.write_hosts().unwrap();
        assert!(!fs::read_to_string(&hosts)
            .unwrap()
            .contains("invited, not yet joined"));
    }
}
