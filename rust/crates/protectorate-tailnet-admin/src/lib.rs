//! Bounded administrative calls to a Headscale 0.29 REST API.
//! The caller supplies an API key and the URL of the local Headscale listener.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fmt, io::Read, net::SocketAddr, time::Duration};
use ureq::unversioned::{
    resolver::{ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};

const MAX_RESPONSE: u64 = 8 << 20;
const MAX_ERROR: u64 = 64 << 10;
const POLICY_USER: &str = "protectorate";
pub const TAG_NODE: &str = "tag:node";
pub const TAG_CLIENT: &str = "tag:client";
const TIER_PREFIX: &str = "tag:tier-";

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Transport(String),
    Status { code: u16, body: String },
    Decode(String),
    MissingNode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) | Self::Transport(s) | Self::Decode(s) => f.write_str(s),
            Self::Status { code, body } => write!(f, "headscale API: {code} {body}"),
            Self::MissingNode(name) => write!(f, "no such node: {name}"),
        }
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

fn numeric_id<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => s.parse().map_err(serde::de::Error::custom),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("invalid ID")),
        _ => Err(serde::de::Error::custom("invalid ID")),
    }
}

fn null_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PreAuthKeyRef {
    #[serde(deserialize_with = "numeric_id")]
    pub id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    #[serde(deserialize_with = "numeric_id")]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub given_name: String,
    #[serde(default)]
    pub node_key: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(default, deserialize_with = "null_string")]
    pub expiry: String,
    #[serde(default)]
    pub pre_auth_key: Option<PreAuthKeyRef>,
}
impl Node {
    pub fn enrolled_with(&self) -> Option<u64> {
        self.pre_auth_key.as_ref().map(|k| k.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    pub key: String,
    pub id: u64,
}

pub struct Admin {
    base: String,
    key: String,
    user_id: Option<u64>,
    agent: ureq::Agent,
}

#[derive(Debug)]
struct FixedResolver(SocketAddr);
impl Resolver for FixedResolver {
    fn resolve(
        &self,
        _uri: &http::Uri,
        _config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addresses = self.empty();
        addresses.push(self.0);
        Ok(addresses)
    }
}

impl Admin {
    /// Public control-server origin given to machines during enrollment.
    pub fn control_url(&self) -> &str {
        &self.base
    }

    pub fn new(base: &str, key: &str, timeout: Duration) -> Result<Self> {
        Self::new_with_local_addr(base, key, timeout, None)
    }

    /// Resolve every Headscale request to this local socket address while
    /// retaining the public URL's Host header and TLS server name.
    pub fn new_with_local_addr(
        base: &str,
        key: &str,
        timeout: Duration,
        local: Option<SocketAddr>,
    ) -> Result<Self> {
        let base = base.trim_end_matches('/');
        let valid_scheme = base.starts_with("http://") || base.starts_with("https://");
        if !valid_scheme
            || base.contains(['?', '#'])
            || base
                .split_once("://")
                .is_none_or(|(_, h)| h.is_empty() || h.contains('/'))
        {
            return Err(Error::Invalid(
                "Headscale URL must be an HTTP(S) origin".into(),
            ));
        }
        if key.is_empty() || key.contains(['\r', '\n']) || timeout.is_zero() {
            return Err(Error::Invalid("API key and timeout are required".into()));
        }
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout))
            .build();
        let agent = if let Some(address) = local {
            ureq::Agent::with_parts(config, DefaultConnector::default(), FixedResolver(address))
        } else {
            config.into()
        };
        Ok(Self {
            base: base.into(),
            key: key.into(),
            user_id: None,
            agent,
        })
    }

    pub fn set_user_id(&mut self, id: u64) -> Result<()> {
        if id == 0 {
            return Err(Error::Invalid("user ID must be positive".into()));
        }
        self.user_id = Some(id);
        Ok(())
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        if !path.starts_with('/') {
            return Err(Error::Invalid("API path must be absolute".into()));
        }
        let url = format!("{}{path}", self.base);
        let bearer = format!("Bearer {}", self.key);
        let response = match method {
            "GET" if body.is_none() => self.agent.get(&url).header("Authorization", &bearer).call(),
            "DELETE" if body.is_none() => self
                .agent
                .delete(&url)
                .header("Authorization", &bearer)
                .call(),
            "POST" => {
                let request = self.agent.post(&url).header("Authorization", &bearer);
                if let Some(body) = body {
                    request
                        .header("Content-Type", "application/json")
                        .send(serde_json::to_vec(body).map_err(|e| Error::Decode(e.to_string()))?)
                } else {
                    request.send_empty()
                }
            }
            _ => return Err(Error::Invalid("unsupported Headscale request".into())),
        }
        .map_err(|e| Error::Transport(format!("headscale {method} {path}: {e}")))?;
        let code = response.status().as_u16();
        let max = if code == 200 { MAX_RESPONSE } else { MAX_ERROR };
        let mut raw = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(max + 1)
            .read_to_end(&mut raw)
            .map_err(|e| Error::Transport(format!("read Headscale response: {e}")))?;
        if raw.len() as u64 > max {
            return Err(Error::Decode("Headscale response exceeds limit".into()));
        }
        if code != 200 {
            return Err(Error::Status {
                code,
                body: String::from_utf8_lossy(&raw).trim().to_string(),
            });
        }
        if path == "/health" {
            return Ok(Value::Null);
        }
        if raw.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&raw)
            .map_err(|e| Error::Decode(format!("decode Headscale response: {e}")))
    }

    pub fn health(&self) -> Result<()> {
        self.call("GET", "/health", None).map(|_| ())
    }

    pub fn ensure_policy_user(&mut self) -> Result<u64> {
        #[derive(Deserialize)]
        struct User {
            #[serde(deserialize_with = "numeric_id")]
            id: u64,
            name: String,
        }
        #[derive(Deserialize)]
        struct List {
            users: Vec<User>,
        }
        #[derive(Deserialize)]
        struct Created {
            user: User,
        }
        let value = self.call("GET", "/api/v1/user?name=protectorate", None)?;
        let list: List = serde_json::from_value(value).map_err(|e| Error::Decode(e.to_string()))?;
        let id = if let Some(user) = list.users.into_iter().find(|u| u.name == POLICY_USER) {
            user.id
        } else {
            let value = self.call("POST", "/api/v1/user", Some(&json!({"name": POLICY_USER})))?;
            serde_json::from_value::<Created>(value)
                .map_err(|e| Error::Decode(e.to_string()))?
                .user
                .id
        };
        self.set_user_id(id)?;
        Ok(id)
    }

    pub fn nodes(&self) -> Result<Vec<Node>> {
        #[derive(Deserialize)]
        struct List {
            nodes: Vec<Node>,
        }
        serde_json::from_value::<List>(self.call("GET", "/api/v1/node", None)?)
            .map(|list| list.nodes)
            .map_err(|e| Error::Decode(e.to_string()))
    }

    pub fn find(&self, name: &str) -> Result<Node> {
        self.nodes()?
            .into_iter()
            .find(|n| n.given_name == name || n.name == name)
            .ok_or_else(|| Error::MissingNode(name.into()))
    }

    pub fn create_key(&self, tags: &[String], ttl: Duration) -> Result<Invite> {
        #[derive(Deserialize)]
        struct Key {
            #[serde(deserialize_with = "numeric_id")]
            id: u64,
            key: String,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            pre_auth_key: Option<Key>,
        }
        let id = self
            .user_id
            .ok_or_else(|| Error::Invalid("policy user is not initialized".into()))?;
        if ttl.is_zero() {
            return Err(Error::Invalid("expiry must be positive".into()));
        }
        let expiry = Utc::now()
            .checked_add_signed(
                ChronoDuration::from_std(ttl)
                    .map_err(|_| Error::Invalid("expiry is too large".into()))?,
            )
            .ok_or_else(|| Error::Invalid("expiry is too large".into()))?;
        let value = self.call("POST", "/api/v1/preauthkey", Some(&json!({
            "user": id.to_string(), "reusable": false, "ephemeral": false,
            "expiration": expiry.to_rfc3339_opts(chrono::SecondsFormat::Secs, true), "aclTags": tags
        })))?;
        let response: Response =
            serde_json::from_value(value).map_err(|e| Error::Decode(e.to_string()))?;
        match response.pre_auth_key {
            Some(k) if !k.key.is_empty() && k.id != 0 => Ok(Invite {
                key: k.key,
                id: k.id,
            }),
            _ => Err(Error::Decode(
                "create preauth key: missing key or ID".into(),
            )),
        }
    }

    pub fn invite(
        &self,
        name: &str,
        hub_name: &str,
        tags: &[String],
        ttl: Duration,
    ) -> Result<Invite> {
        if !valid_label(name) {
            return Err(Error::Invalid(format!("invalid node name: {name}")));
        }
        if name == hub_name
            || self
                .nodes()?
                .iter()
                .any(|n| n.name == name || n.given_name == name)
        {
            return Err(Error::Invalid(format!("name {name} is already in use")));
        }
        self.create_key(tags, ttl)
    }

    pub fn invite_node(
        &self,
        name: &str,
        hub_name: &str,
        tier: &str,
        pools: &[String],
        configured_pools: &[String],
        ttl: Duration,
    ) -> Result<Invite> {
        if !matches!(tier, "shared" | "care") {
            return Err(Error::Invalid("tier must be shared or care".into()));
        }
        let mut tags = vec![TAG_NODE.to_owned(), format!("{TIER_PREFIX}{tier}")];
        for pool in pools {
            if !configured_pools.contains(pool) {
                return Err(Error::Invalid(format!("pool {pool} is not configured")));
            }
            tags.push(format!("tag:pool-{pool}"));
        }
        self.invite(name, hub_name, &tags, ttl)
    }

    pub fn expire_key(&self, id: u64) -> Result<()> {
        self.call(
            "POST",
            "/api/v1/preauthkey/expire",
            Some(&json!({"id": id.to_string()})),
        )
        .map(|_| ())
    }

    pub fn revoke(&self, name: &str) -> Result<bool> {
        let node = match self.find(name) {
            Ok(n) => n,
            Err(Error::MissingNode(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        self.call(
            "POST",
            &format!("/api/v1/node/{}/expire", node.id),
            Some(&json!({})),
        )?;
        self.call("DELETE", &format!("/api/v1/node/{}", node.id), None)?;
        Ok(true)
    }

    pub fn set_tier(&self, name: &str, tier: &str) -> Result<bool> {
        if !matches!(tier, "shared" | "care") {
            return Err(Error::Invalid("tier must be shared or care".into()));
        }
        let node = match self.find(name) {
            Ok(n) => n,
            Err(Error::MissingNode(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        let mut tags: Vec<String> = node
            .tags
            .into_iter()
            .filter(|t| !t.starts_with(TIER_PREFIX))
            .collect();
        tags.push(format!("{TIER_PREFIX}{tier}"));
        tags.sort();
        tags.dedup();
        self.call(
            "POST",
            &format!("/api/v1/node/{}/tags", node.id),
            Some(&json!({"tags": tags})),
        )?;
        Ok(true)
    }

    pub fn register_client(&self, registration_key: &str) -> Result<Node> {
        if registration_key.is_empty() {
            return Err(Error::Invalid("registration key is empty".into()));
        }
        #[derive(Deserialize)]
        struct Response {
            node: Node,
        }
        let path = format!(
            "/api/v1/node/register?user={POLICY_USER}&key={}",
            percent_encode(registration_key)
        );
        let response: Response = serde_json::from_value(self.call("POST", &path, None)?)
            .map_err(|e| Error::Decode(e.to_string()))?;
        self.call(
            "POST",
            &format!("/api/v1/node/{}/tags", response.node.id),
            Some(&json!({"tags": [TAG_CLIENT]})),
        )?;
        Ok(response.node)
    }

    pub fn set_expiry(&self, id: u64, at: DateTime<Utc>) -> Result<()> {
        let at = at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.call(
            "POST",
            &format!("/api/v1/node/{id}/expire?expiry={}", percent_encode(&at)),
            None,
        )
        .map(|_| ())
    }

    pub fn enforce_expiry(&self, days: i64) -> Result<usize> {
        if days < 1 {
            return Err(Error::Invalid("expiry days must be positive".into()));
        }
        let at = Utc::now()
            .checked_add_signed(ChronoDuration::days(days))
            .ok_or_else(|| Error::Invalid("expiry is too large".into()))?;
        let mut count = 0;
        for n in self.nodes()? {
            if n.expiry.is_empty() && expires(&n) {
                self.set_expiry(n.id, at)?;
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn renew(&self, name: &str, days: i64) -> Result<DateTime<Utc>> {
        if days < 1 {
            return Err(Error::Invalid("expiry days must be positive".into()));
        }
        let node = self.find(name)?;
        if !expires(&node) {
            return Err(Error::Invalid(format!("{name} has no expiry to renew")));
        }
        let at = Utc::now()
            .checked_add_signed(ChronoDuration::days(days))
            .ok_or_else(|| Error::Invalid("expiry is too large".into()))?;
        self.set_expiry(node.id, at)?;
        Ok(at)
    }
}

fn expires(n: &Node) -> bool {
    n.tags.iter().any(|t| t == TAG_NODE || t == TAG_CLIENT)
}

fn valid_label(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=63).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && (b[b.len() - 1].is_ascii_lowercase() || b[b.len() - 1].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
