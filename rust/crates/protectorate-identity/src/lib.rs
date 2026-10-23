//! The devices and capabilities a hub has issued. A device record holds the
//! public key that verifies each of its requests; a capability record names
//! one lineage of tokens the hub minted, so it can list, renew, refresh and
//! revoke it. The hub's capability key lives in the same file. Timestamps are
//! Unix seconds. One hub process owns each store file.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use protectorate_auth::{
    capability::{self, Claims},
    proof::{self, Proof},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEVICE_LIFETIME: i64 = 600;
pub const DEVICE_INTERVAL: u32 = 5;
pub const MAX_PENDING_DEVICES: usize = 100;
const NONCE_CAPACITY: usize = 100_000;
/// How long a dead lineage stays listed before it is forgotten.
const RECORD_RETENTION: i64 = 24 * 60 * 60;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    UnknownToken,
    Expired,
    Revoked,
    Proof(&'static str),
    DeviceCapacity,
    DeviceRequest,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::UnknownToken => write!(f, "unknown credential"),
            Self::Expired => write!(f, "capability expired"),
            Self::Revoked => write!(f, "capability revoked"),
            Self::Proof(why) => write!(f, "device proof {why}"),
            Self::DeviceCapacity => write!(f, "device authorization capacity reached"),
            Self::DeviceRequest => write!(f, "no pending device request matches that code"),
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

/// Does not implement Serialize or Clone; reveal only when delivering to its holder.
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    /// Derived from the public key; also the audit `token_id`.
    pub id: String,
    pub name: String,
    pub platform: String,
    /// Uncompressed P-256 point, base64url.
    pub public_key: String,
    pub approved_at: i64,
    pub approved_by: String,
    pub last_used: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub id: String,
    pub principal: String,
    pub scopes: Vec<String>,
    pub task: String,
    pub minted_by: String,
    pub created: i64,
    pub ttl: i64,
    /// The latest expiry of any token in the lineage.
    pub expires: i64,
    pub revoked_at: Option<i64>,
    /// The holder's token file the hub keeps fresh, for sleeves and Envoy.
    pub file: Option<PathBuf>,
}
impl Capability {
    pub fn is_active(&self, now: i64) -> bool {
        self.revoked_at.is_none() && now < self.expires
    }
}

/// What a minted token is for. `minted_by` names the caller.
pub struct Mint<'a> {
    pub principal: &'a str,
    pub scopes: Vec<String>,
    pub task: &'a str,
    pub minted_by: &'a str,
    pub ttl: Duration,
    pub file: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Pending,
    Approved,
    Denied,
    Consumed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceAuth {
    pub user_code: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub ip: String,
    pub expires: i64,
    pub status: DeviceStatus,
    pub interval: u32,
    pub next_poll: Option<i64>,
    pub approved_at: Option<i64>,
    pub approved_by: String,
}
#[derive(Debug)]
pub struct DeviceStarted {
    pub request: DeviceAuth,
    pub device_code: Secret,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollStatus {
    Expired,
    Pending,
    SlowDown,
    Denied,
    Consumed,
    Approved,
}
#[derive(Debug)]
pub struct DevicePoll {
    pub status: PollStatus,
    pub interval: u32,
    pub device: Option<Device>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredRequest {
    request: DeviceAuth,
    code_hash: [u8; 32],
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Disk {
    #[serde(default)]
    key: String,
    #[serde(default)]
    devices: Vec<Device>,
    #[serde(default)]
    capabilities: Vec<Capability>,
    #[serde(default)]
    requests: Vec<StoredRequest>,
}
struct Inner {
    disk: Disk,
    key: Zeroizing<Vec<u8>>,
    last_flush: Option<i64>,
    watches: BTreeMap<String, Vec<Weak<Signal>>>,
    nonces: HashMap<String, i64>,
}
pub struct Store {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}
fn invalid<T>(s: &str) -> Result<T> {
    Err(Error::Invalid(s.into()))
}
fn hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}
fn random_bytes(n: usize) -> Result<Zeroizing<Vec<u8>>> {
    let mut b = Zeroizing::new(vec![0u8; n]);
    getrandom::getrandom(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(b)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn ttl_seconds(ttl: Duration) -> Result<i64> {
    if ttl.is_zero() || ttl > MAX_TTL {
        return invalid("capability lifetime must be positive and at most 24 hours");
    }
    Ok(ttl.as_secs().max(1) as i64)
}
fn key_from_hex(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    (text.len() == 64).then_some(())?;
    let bytes = (0..32)
        .map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    Some(Zeroizing::new(bytes))
}
fn signal(inner: &mut Inner, id: &str) {
    if let Some(watches) = inner.watches.remove(id) {
        for signal in watches.into_iter().filter_map(|s| s.upgrade()) {
            signal.cancel()
        }
    }
}

impl Store {
    pub fn memory() -> Self {
        Self::with_disk(None, Disk::default()).expect("a fresh capability key")
    }
    fn with_disk(path: Option<PathBuf>, mut disk: Disk) -> Result<Self> {
        let key = match key_from_hex(&disk.key) {
            Some(key) => key,
            None => {
                let key = random_bytes(32)?;
                disk.key = hex(&key);
                key
            }
        };
        Ok(Self {
            path,
            inner: Mutex::new(Inner {
                disk,
                key,
                last_flush: None,
                watches: BTreeMap::new(),
                nonces: HashMap::new(),
            }),
        })
    }
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let disk: Disk = match fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Disk::default(),
            Err(e) => return Err(e.into()),
        };
        let generated = key_from_hex(&disk.key).is_none();
        let store = Self::with_disk(Some(path), disk)?;
        if generated {
            store.flush()?;
        }
        Ok(store)
    }
    fn persist(&self, disk: &Disk) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut b = fs::DirBuilder::new();
        b.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            b.mode(0o700);
        }
        b.create(parent)?;
        protectorate_fsutil::write_file(path, &serde_json::to_vec_pretty(disk)?, 0o600)?;
        Ok(())
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn change<T>(&self, change: impl FnOnce(&mut Disk, &[u8]) -> Result<T>) -> Result<T> {
        let mut inner = self.lock();
        let mut next = inner.disk.clone();
        let out = change(&mut next, &inner.key)?;
        if next != inner.disk {
            self.persist(&next)?;
        }
        inner.disk = next;
        Ok(out)
    }
    pub fn flush(&self) -> Result<()> {
        self.persist(&self.lock().disk)
    }

    /// Mints the first token of a new lineage.
    pub fn mint(&self, mint: Mint<'_>, now: i64) -> Result<(Capability, Secret)> {
        let ttl = ttl_seconds(mint.ttl)?;
        if mint.task.len() > 500 || mint.minted_by.is_empty() {
            return invalid("a capability needs its minter and a task of at most 500 bytes");
        }
        let id = hex(&random_bytes(16)?);
        let record = Capability {
            id: id.clone(),
            principal: mint.principal.into(),
            scopes: mint.scopes,
            task: mint.task.trim().into(),
            minted_by: mint.minted_by.into(),
            created: now,
            ttl,
            expires: now.saturating_add(ttl),
            revoked_at: None,
            file: mint.file,
        };
        self.change(|d, key| {
            let token = capability::mint(
                key,
                &record.id,
                &record.principal,
                &record.scopes,
                &record.task,
                now,
                record.expires,
            )
            .map_err(|e| Error::Invalid(format!("{e}: check the name, scopes and task")))?;
            d.capabilities.push(record.clone());
            Ok((record, Secret(Zeroizing::new(token))))
        })
    }

    fn check(&self, inner: &Inner, token: &str, now: i64) -> Result<(Capability, Claims)> {
        let claims = capability::verify(&inner.key, token).map_err(|_| Error::UnknownToken)?;
        let record = inner
            .disk
            .capabilities
            .iter()
            .find(|c| c.id == claims.id)
            .ok_or(Error::UnknownToken)?;
        if record.revoked_at.is_some() {
            return Err(Error::Revoked);
        }
        if now >= claims.expires {
            return Err(Error::Expired);
        }
        Ok((record.clone(), claims))
    }

    pub fn verify(&self, token: &str, now: i64) -> Result<(Capability, Claims)> {
        self.check(&self.lock(), token, now)
    }

    /// A fresh token for the presented one, holder caveats kept: a narrowed
    /// token stays narrowed and never outlives an expiry its holder set.
    pub fn renew(&self, token: &str, now: i64) -> Result<(Capability, Secret)> {
        let (record, claims) = self.verify(token, now)?;
        self.change(|d, key| {
            let expires = now.saturating_add(record.ttl);
            let token = capability::reissue(key, &claims, now, expires)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let stored = d
                .capabilities
                .iter_mut()
                .find(|c| c.id == record.id)
                .ok_or(Error::UnknownToken)?;
            stored.expires = stored.expires.max(expires);
            Ok((stored.clone(), Secret(Zeroizing::new(token))))
        })
    }

    /// A fresh token from the record alone, for the holder file the hub keeps.
    pub fn refresh(&self, id: &str, now: i64) -> Result<Secret> {
        self.change(|d, key| {
            let stored = d
                .capabilities
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or(Error::UnknownToken)?;
            if stored.revoked_at.is_some() {
                return Err(Error::Revoked);
            }
            let expires = now.saturating_add(stored.ttl);
            let token = capability::mint(
                key,
                &stored.id,
                &stored.principal,
                &stored.scopes,
                &stored.task,
                now,
                expires,
            )
            .map_err(|e| Error::Invalid(e.to_string()))?;
            stored.expires = stored.expires.max(expires);
            Ok(Secret(Zeroizing::new(token)))
        })
    }

    pub fn capabilities(&self) -> Vec<Capability> {
        self.lock().disk.capabilities.clone()
    }

    pub fn revoke(&self, id: &str, now: i64) -> Result<Capability> {
        let mut inner = self.lock();
        let mut next = inner.disk.clone();
        let record = next
            .capabilities
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(Error::UnknownToken)?;
        if record.revoked_at.is_none() {
            record.revoked_at = Some(now)
        }
        let out = record.clone();
        if next != inner.disk {
            self.persist(&next)?;
        }
        inner.disk = next;
        signal(&mut inner, id);
        Ok(out)
    }

    /// Revokes every active lineage of one principal, such as a killed sleeve.
    pub fn revoke_principal(&self, principal: &str, now: i64) -> Result<()> {
        for record in self.capabilities() {
            if record.principal == principal && record.revoked_at.is_none() {
                self.revoke(&record.id, now)?;
            }
        }
        Ok(())
    }

    pub fn start_device(
        &self,
        name: &str,
        platform: &str,
        public_key: &[u8],
        ip: &str,
        now: i64,
    ) -> Result<DeviceStarted> {
        if !valid_device_name(name) || !valid_device_label(platform, 40) {
            return invalid("invalid device name or platform");
        }
        if !proof::valid_public_key(public_key) {
            return invalid("the device key must be an uncompressed P-256 public key");
        }
        let secret = Secret(Zeroizing::new(hex(&random_bytes(32)?)));
        self.change(|d, _| {
            d.requests.retain(|r| now < r.request.expires);
            if d.requests.len() >= 1000
                || d.requests
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.request.status,
                            DeviceStatus::Pending | DeviceStatus::Approved
                        )
                    })
                    .count()
                    >= MAX_PENDING_DEVICES
            {
                return Err(Error::DeviceCapacity);
            }
            let code = loop {
                let code = user_code()?;
                if !d.requests.iter().any(|r| r.request.user_code == code) {
                    break code;
                }
            };
            let request = DeviceAuth {
                user_code: code,
                name: name.into(),
                platform: platform.into(),
                public_key: B64.encode(public_key),
                ip: ip.into(),
                expires: now
                    .checked_add(DEVICE_LIFETIME)
                    .ok_or_else(|| Error::Invalid("expiry overflows timestamp".into()))?,
                status: DeviceStatus::Pending,
                interval: DEVICE_INTERVAL,
                next_poll: None,
                approved_at: None,
                approved_by: String::new(),
            };
            d.requests.push(StoredRequest {
                request: request.clone(),
                code_hash: hash(secret.expose()),
            });
            Ok(DeviceStarted {
                request,
                device_code: secret,
            })
        })
    }
    pub fn pending_devices(&self, now: i64) -> Vec<DeviceAuth> {
        self.lock()
            .disk
            .requests
            .iter()
            .filter(|r| r.request.status == DeviceStatus::Pending && now < r.request.expires)
            .map(|r| r.request.clone())
            .collect()
    }
    pub fn decide_device(
        &self,
        code: &str,
        actor: &str,
        approve: bool,
        now: i64,
    ) -> Result<DeviceAuth> {
        if actor.is_empty() {
            return invalid("approval actor is required");
        }
        let got = hash(&normalize_code(code));
        self.change(|d, _| {
            let mut found = None;
            for (i, r) in d.requests.iter().enumerate() {
                if bool::from(got.ct_eq(&hash(&normalize_code(&r.request.user_code)))) {
                    found = Some(i)
                }
            }
            let i = found.ok_or(Error::DeviceRequest)?;
            let r = &mut d.requests[i].request;
            if r.status != DeviceStatus::Pending || now >= r.expires {
                return Err(Error::DeviceRequest);
            }
            if approve {
                r.status = DeviceStatus::Approved;
                r.approved_at = Some(now);
                r.approved_by = actor.into()
            } else {
                r.status = DeviceStatus::Denied
            }
            Ok(r.clone())
        })
    }
    /// Polls a request. The device code only learns the outcome: what the
    /// approved device can do still needs its private key.
    pub fn poll_device(&self, code: &str, now: i64) -> Result<DevicePoll> {
        let got = hash(code);
        self.change(|d, _| {
            let mut found = None;
            for (i, r) in d.requests.iter().enumerate() {
                if bool::from(got.ct_eq(&r.code_hash)) {
                    found = Some(i)
                }
            }
            let poll = |status, interval| DevicePoll {
                status,
                interval,
                device: None,
            };
            let Some(i) = found else {
                return Ok(poll(PollStatus::Expired, DEVICE_INTERVAL));
            };
            let r = &mut d.requests[i].request;
            if now >= r.expires {
                return Ok(poll(PollStatus::Expired, DEVICE_INTERVAL));
            }
            if r.status == DeviceStatus::Consumed {
                return Ok(poll(PollStatus::Consumed, r.interval));
            }
            if r.status == DeviceStatus::Denied {
                return Ok(poll(PollStatus::Denied, r.interval));
            }
            if r.next_poll.is_some_and(|at| now < at) {
                r.interval = r.interval.saturating_add(DEVICE_INTERVAL).min(300);
                r.next_poll = Some(now.saturating_add(r.interval as i64));
                return Ok(poll(PollStatus::SlowDown, r.interval));
            }
            r.next_poll = Some(now.saturating_add(r.interval as i64));
            if r.status != DeviceStatus::Approved {
                return Ok(poll(PollStatus::Pending, r.interval));
            }
            r.status = DeviceStatus::Consumed;
            let key = B64
                .decode(&r.public_key)
                .map_err(|_| Error::Invalid("stored device key".into()))?;
            let device = Device {
                id: proof::device_id(&key),
                name: r.name.clone(),
                platform: r.platform.clone(),
                public_key: r.public_key.clone(),
                approved_at: r.approved_at.unwrap_or(now),
                approved_by: r.approved_by.clone(),
                last_used: None,
            };
            let interval = r.interval;
            d.devices.retain(|known| known.id != device.id);
            d.devices.push(device.clone());
            Ok(DevicePoll {
                status: PollStatus::Approved,
                interval,
                device: Some(device),
            })
        })
    }
    pub fn devices(&self) -> Vec<Device> {
        self.lock().disk.devices.clone()
    }
    /// Admits one signed request: a known device, a clock within
    /// `MAX_SKEW`, a valid signature and a nonce not seen before.
    pub fn verify_proof(
        &self,
        proof: &Proof,
        method: &str,
        path: &str,
        now: i64,
    ) -> Result<Device> {
        if now.abs_diff(proof.time) > proof::MAX_SKEW as u64 {
            return Err(Error::Proof("is outside the allowed clock skew"));
        }
        let mut inner = self.lock();
        let index = inner
            .disk
            .devices
            .iter()
            .position(|d| d.id == proof.device)
            .ok_or(Error::UnknownToken)?;
        let key = B64
            .decode(&inner.disk.devices[index].public_key)
            .map_err(|_| Error::UnknownToken)?;
        if !proof.verify(&key, method, path) {
            return Err(Error::Proof("signature is invalid"));
        }
        let seen = format!("{}.{}", proof.device, proof.nonce);
        if inner.nonces.len() >= NONCE_CAPACITY {
            inner.nonces.retain(|_, until| *until >= now);
            if inner.nonces.len() >= NONCE_CAPACITY {
                return Err(Error::Proof("cache is full; retry shortly"));
            }
        }
        if inner.nonces.contains_key(&seen) {
            return Err(Error::Proof("was replayed"));
        }
        inner.nonces.insert(seen, proof.time + proof::MAX_SKEW);
        inner.disk.devices[index].last_used = Some(now);
        if inner.last_flush.is_none_or(|t| now.saturating_sub(t) >= 60)
            && self.persist(&inner.disk).is_ok()
        {
            inner.last_flush = Some(now)
        }
        Ok(inner.disk.devices[index].clone())
    }
    /// Forgets a device: its next request fails and its streams close.
    pub fn remove_device(&self, id: &str) -> Result<Device> {
        let mut inner = self.lock();
        let mut next = inner.disk.clone();
        let index = next
            .devices
            .iter()
            .position(|d| d.id == id)
            .ok_or(Error::UnknownToken)?;
        let device = next.devices.remove(index);
        self.persist(&next)?;
        inner.disk = next;
        signal(&mut inner, id);
        Ok(device)
    }
    pub fn rename_device(&self, id: &str, name: &str) -> Result<Device> {
        if !valid_device_name(name) {
            return invalid("invalid device name");
        }
        self.change(|d, _| {
            let device = d
                .devices
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or(Error::UnknownToken)?;
            device.name = name.into();
            Ok(device.clone())
        })
    }
    /// Drops expired device requests and long-dead capability lineages.
    pub fn sweep(&self, now: i64) -> Result<()> {
        self.lock().nonces.retain(|_, until| *until >= now);
        self.change(|d, _| {
            d.requests.retain(|r| now < r.request.expires);
            d.capabilities
                .retain(|c| now < c.expires.saturating_add(RECORD_RETENTION));
            Ok(())
        })
    }
    /// Registration and revocation share a lock, preventing admission races.
    /// The ID is a device's or a capability lineage's.
    pub fn watch(&self, id: &str, now: i64) -> RevocationWatch {
        let signal = Arc::new(Signal(AtomicBool::new(false)));
        let mut inner = self.lock();
        let live = inner.disk.devices.iter().any(|d| d.id == id)
            || inner
                .disk
                .capabilities
                .iter()
                .any(|c| c.id == id && c.revoked_at.is_none() && now < c.expires);
        if live {
            let watches = inner.watches.entry(id.into()).or_default();
            watches.retain(|s| s.strong_count() > 0);
            watches.push(Arc::downgrade(&signal));
        } else {
            signal.cancel()
        }
        RevocationWatch(signal)
    }
}
pub fn valid_device_label(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn valid_device_name(s: &str) -> bool {
    valid_device_label(s, 100)
        && s != "envoy"
        && s != "orchestrator-envoy"
        && !s.starts_with("sleeve:")
}
fn normalize_code(s: &str) -> String {
    s.trim().replace('-', "").to_ascii_uppercase()
}
fn user_code() -> Result<String> {
    const ABC: &[u8] = b"23456789BCDFGHJKLMNPQRSTVWXYZ";
    let mut code = String::new();
    while code.len() < 8 {
        let b = random_bytes(1)?;
        let limit = 256 - 256 % ABC.len();
        if (b[0] as usize) < limit {
            code.push(ABC[b[0] as usize % ABC.len()] as char)
        }
    }
    code.insert(4, '-');
    Ok(code)
}
struct Signal(AtomicBool);
impl Signal {
    fn cancel(&self) {
        self.0.store(true, Ordering::Release)
    }
}
/// Dropping the watch releases its registration; no credential is retained.
pub struct RevocationWatch(Arc<Signal>);
impl RevocationWatch {
    pub fn is_cancelled(&self) -> bool {
        self.0 .0.load(Ordering::Acquire)
    }
}
