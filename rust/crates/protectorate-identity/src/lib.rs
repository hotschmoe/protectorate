//! The devices and capabilities a hub has issued. A device is a named bearer
//! token, `pd1.<base64url of 32 random bytes>`, of which the hub keeps only
//! the SHA-256; a capability record names one lineage of tokens the hub
//! minted, so it can list, renew, refresh and revoke it. The hub's capability
//! key lives in the same file. Timestamps are Unix seconds. One hub process
//! owns each store file.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use protectorate_auth::capability::{self, Claims};
use protectorate_fsutil::hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    thread,
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEVICE_PREFIX: &str = "pd1.";
const DEVICE_TOKEN_BYTES: usize = 32;
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
    /// The first 8 bytes of `hash`, hex; also the audit `token_id`.
    pub id: String,
    pub name: String,
    /// SHA-256 of the token, hex.
    pub hash: String,
    pub added_at: i64,
    pub added_by: String,
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

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Disk {
    #[serde(default)]
    key: String,
    #[serde(default)]
    device_tokens: Vec<Device>,
    #[serde(default)]
    capabilities: Vec<Capability>,
}
struct Inner {
    disk: Disk,
    last_flush: Option<i64>,
    watches: BTreeMap<String, Vec<Weak<Signal>>>,
}
/// Readers take only `inner`, briefly, and never wait on the disk. Writers
/// take `writer` first, so they run one at a time and in order: each builds
/// its next state from a snapshot, writes it durably without holding `inner`,
/// and only then swaps it in. A failed write leaves memory and disk unchanged.
/// Until the swap readers still see the old state, so a change takes effect
/// for them when its caller is told it succeeded.
pub struct Store {
    key: Zeroizing<Vec<u8>>,
    inner: Arc<Mutex<Inner>>,
    disk: Arc<DiskFile>,
}
struct DiskFile {
    path: Option<PathBuf>,
    writer: Mutex<()>,
    flushing: AtomicBool,
}
impl DiskFile {
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
    /// Writes the newest committed state; the snapshot is taken after the
    /// writer lock, so an older state never lands over a newer one.
    fn flush(&self, inner: &Mutex<Inner>) -> Result<()> {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let disk = inner.lock().unwrap_or_else(|e| e.into_inner()).disk.clone();
        self.persist(&disk)
    }
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
fn ttl_seconds(ttl: Duration) -> Result<i64> {
    if ttl.is_zero() || ttl > MAX_TTL {
        return invalid("capability lifetime must be positive and at most 24 hours");
    }
    Ok(ttl.as_secs().max(1) as i64)
}
/// Decodes the 64 hex digits of a stored key. Works on bytes: slicing the
/// text could split a multibyte character in a corrupt file and panic.
fn key_from_hex(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let digits = text.as_bytes();
    (digits.len() == 64).then_some(())?;
    let nibble = |digit: u8| char::from(digit).to_digit(16).map(|value| value as u8);
    let bytes = digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[high, low]| Some(nibble(high)? << 4 | nibble(low)?))
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
            key,
            disk: Arc::new(DiskFile {
                path,
                writer: Mutex::new(()),
                flushing: AtomicBool::new(false),
            }),
            inner: Arc::new(Mutex::new(Inner {
                disk,
                last_flush: None,
                watches: BTreeMap::new(),
            })),
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
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn change<T>(&self, change: impl FnOnce(&mut Disk, &[u8]) -> Result<T>) -> Result<T> {
        self.apply(change, |_| {})
    }
    /// `after` runs under the state lock at the moment the change becomes visible.
    fn apply<T>(
        &self,
        change: impl FnOnce(&mut Disk, &[u8]) -> Result<T>,
        after: impl FnOnce(&mut Inner),
    ) -> Result<T> {
        let _writer = self.disk.writer.lock().unwrap_or_else(|e| e.into_inner());
        let current = self.lock().disk.clone();
        let mut next = current.clone();
        let out = change(&mut next, &self.key)?;
        if next == current {
            return Ok(out);
        }
        self.disk.persist(&next)?;
        let mut inner = self.lock();
        for device in &mut next.device_tokens {
            let used = inner
                .disk
                .device_tokens
                .iter()
                .find(|d| d.id == device.id)
                .and_then(|d| d.last_used);
            device.last_used = device.last_used.max(used);
        }
        inner.disk = next;
        after(&mut inner);
        Ok(out)
    }
    pub fn flush(&self) -> Result<()> {
        self.disk.flush(&self.inner)
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
        let claims = capability::verify(&self.key, token).map_err(|_| Error::UnknownToken)?;
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
        self.apply(
            |d, _| {
                let record = d
                    .capabilities
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or(Error::UnknownToken)?;
                if record.revoked_at.is_none() {
                    record.revoked_at = Some(now)
                }
                Ok(record.clone())
            },
            |inner| signal(inner, id),
        )
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

    /// Adds a device and returns its token, which only the caller sees.
    pub fn add_device(&self, name: &str, added_by: &str, now: i64) -> Result<(Device, Secret)> {
        if !valid_device_name(name) || added_by.is_empty() {
            return invalid("invalid device name");
        }
        self.change(|d, _| loop {
            let token = Secret(Zeroizing::new(format!(
                "{DEVICE_PREFIX}{}",
                B64.encode(random_bytes(DEVICE_TOKEN_BYTES)?)
            )));
            let hash = hex(&hash(token.expose()));
            let id = hash[..16].to_owned();
            if d.device_tokens.iter().any(|known| known.id == id) {
                continue;
            }
            let device = Device {
                id,
                name: name.into(),
                hash,
                added_at: now,
                added_by: added_by.into(),
                last_used: None,
            };
            d.device_tokens.push(device.clone());
            return Ok((device, token));
        })
    }
    pub fn devices(&self) -> Vec<Device> {
        self.lock().disk.device_tokens.clone()
    }
    /// Admits a device token: a well-formed token whose hash a device holds.
    pub fn verify_device(&self, token: &str, now: i64) -> Result<Device> {
        let body = token
            .strip_prefix(DEVICE_PREFIX)
            .ok_or(Error::UnknownToken)?;
        if body.len() > 64 || B64.decode(body).map(|b| b.len()) != Ok(DEVICE_TOKEN_BYTES) {
            return Err(Error::UnknownToken);
        }
        let presented = hex(&hash(token));
        let mut inner = self.lock();
        let index = inner
            .disk
            .device_tokens
            .iter()
            .position(|d| d.id == presented[..16])
            .ok_or(Error::UnknownToken)?;
        let stored = &inner.disk.device_tokens[index].hash;
        if !bool::from(stored.as_bytes().ct_eq(presented.as_bytes())) {
            return Err(Error::UnknownToken);
        }
        inner.disk.device_tokens[index].last_used = Some(now);
        if self.disk.path.is_some()
            && inner.last_flush.is_none_or(|t| now.saturating_sub(t) >= 60)
            && !self.disk.flushing.swap(true, Ordering::AcqRel)
        {
            inner.last_flush = Some(now);
            self.flush_in_background();
        }
        Ok(inner.disk.device_tokens[index].clone())
    }
    /// The last-used stamp is advisory, so its write runs off the request.
    fn flush_in_background(&self) {
        let (disk, inner) = (self.disk.clone(), self.inner.clone());
        thread::spawn(move || {
            let _ = disk.flush(&inner);
            disk.flushing.store(false, Ordering::Release);
        });
    }
    /// Forgets a device: its next request fails and its streams close.
    pub fn remove_device(&self, id: &str) -> Result<Device> {
        self.apply(
            |d, _| {
                let index = d
                    .device_tokens
                    .iter()
                    .position(|d| d.id == id)
                    .ok_or(Error::UnknownToken)?;
                Ok(d.device_tokens.remove(index))
            },
            |inner| signal(inner, id),
        )
    }
    pub fn rename_device(&self, id: &str, name: &str) -> Result<Device> {
        if !valid_device_name(name) {
            return invalid("invalid device name");
        }
        self.change(|d, _| {
            let device = d
                .device_tokens
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or(Error::UnknownToken)?;
            device.name = name.into();
            Ok(device.clone())
        })
    }
    /// Forgets long-dead capability lineages.
    pub fn sweep(&self, now: i64) -> Result<()> {
        self.change(|d, _| {
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
        let live = inner.disk.device_tokens.iter().any(|d| d.id == id)
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
fn valid_device_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && !s.chars().any(char::is_control)
        && s != "envoy"
        && s != "orchestrator-envoy"
        && !s.starts_with("sleeve:")
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
