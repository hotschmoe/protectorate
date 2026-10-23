//! Persistent token issuance and device approval. Public metadata never includes
//! credential hashes; secrets are returned once, redact Debug, and zero on drop.
//! Timestamps are Unix seconds. One hub process owns each store file.
use protectorate_auth::{self as auth, Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
pub const SECRET_PREFIX: &str = "ptk_";
pub const MAX_GRANT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const DEVICE_LIFETIME: i64 = 600;
pub const DEVICE_INTERVAL: u32 = 5;
pub const MAX_PENDING_DEVICES: usize = 100;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Scopes(auth::ScopeError),
    UnknownToken,
    Expired,
    Revoked,
    DeviceCapacity,
    DeviceRequest,
    Ambiguous(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Scopes(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::UnknownToken => write!(f, "unknown token"),
            Self::Expired => write!(f, "token expired"),
            Self::Revoked => write!(f, "token revoked"),
            Self::DeviceCapacity => write!(f, "device authorization capacity reached"),
            Self::DeviceRequest => write!(f, "no pending device request matches that code"),
            Self::Ambiguous(s) => write!(f, "token name {s:?} is ambiguous; use an ID"),
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
impl From<auth::ScopeError> for Error {
    fn from(e: auth::ScopeError) -> Self {
        Self::Scopes(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
/// Does not implement Serialize or Clone; reveal only when delivering to its owner.
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
pub struct Token {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created: i64,
    pub expires: Option<i64>,
    pub last_used: Option<i64>,
    pub revoked_at: Option<i64>,
    pub task: String,
    pub minted_by: String,
    pub device: bool,
    pub platform: String,
    pub approved_at: Option<i64>,
    pub approved_by: String,
}
impl Token {
    pub fn is_active(&self, now: i64) -> bool {
        self.revoked_at.is_none() && self.expires.is_none_or(|e| now < e)
    }
    fn check(&self, now: i64) -> Result<()> {
        if self.revoked_at.is_some() {
            Err(Error::Revoked)
        } else if self.expires.is_some_and(|e| now >= e) {
            Err(Error::Expired)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug)]
pub struct Issued {
    pub token: Token,
    pub secret: Secret,
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
    pub issued: Option<Issued>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredToken {
    token: Token,
    hash: [u8; 32],
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredDevice {
    request: DeviceAuth,
    code_hash: [u8; 32],
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Disk {
    #[serde(default)]
    tokens: Vec<StoredToken>,
    #[serde(default)]
    devices: Vec<StoredDevice>,
}
struct Inner {
    disk: Disk,
    last_flush: Option<i64>,
    watches: BTreeMap<String, Vec<Weak<Signal>>>,
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
fn random_hex(n: usize) -> Result<Secret> {
    let mut b = Zeroizing::new(vec![0u8; n]);
    getrandom::getrandom(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
    let text = b.iter().map(|b| format!("{b:02x}")).collect();
    Ok(Secret(Zeroizing::new(text)))
}
fn expires(now: i64, ttl: Duration) -> Result<Option<i64>> {
    if ttl.is_zero() {
        return Ok(None);
    }
    let secs = ttl
        .as_secs()
        .checked_add(u64::from(ttl.subsec_nanos() > 0))
        .and_then(|s| i64::try_from(s).ok())
        .ok_or_else(|| Error::Invalid("ttl is too large".into()))?;
    Ok(Some(now.checked_add(secs).ok_or_else(|| {
        Error::Invalid("expiry overflows timestamp".into())
    })?))
}
fn token_template(name: &str, scopes: Vec<String>, now: i64) -> Token {
    Token {
        id: String::new(),
        name: name.into(),
        scopes,
        created: now,
        expires: None,
        last_used: None,
        revoked_at: None,
        task: String::new(),
        minted_by: String::new(),
        device: false,
        platform: String::new(),
        approved_at: None,
        approved_by: String::new(),
    }
}
fn issue(disk: &mut Disk, mut token: Token, ttl: Duration, now: i64) -> Result<Issued> {
    loop {
        token.id = random_hex(8)?.expose().into();
        if !disk.tokens.iter().any(|t| t.token.id == token.id) {
            break;
        }
    }
    let raw = random_hex(32)?;
    let secret = Secret(Zeroizing::new(format!("{SECRET_PREFIX}{}", raw.expose())));
    token.created = now;
    token.expires = expires(now, ttl)?;
    token.last_used = None;
    token.revoked_at = None;
    disk.tokens.push(StoredToken {
        hash: hash(secret.expose()),
        token: token.clone(),
    });
    Ok(Issued { token, secret })
}
impl Store {
    pub fn memory() -> Self {
        Self {
            path: None,
            inner: Mutex::new(Inner {
                disk: Disk::default(),
                last_flush: None,
                watches: BTreeMap::new(),
            }),
        }
    }
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let disk = match fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Disk::default(),
            Err(e) => return Err(e.into()),
        };
        let mut s = Self::memory();
        s.path = Some(path);
        s.inner.get_mut().unwrap().disk = disk;
        Ok(s)
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
    fn change<T>(&self, change: impl FnOnce(&mut Disk) -> Result<T>) -> Result<T> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = inner.disk.clone();
        let out = change(&mut next)?;
        if next != inner.disk {
            self.persist(&next)?;
        }
        inner.disk = next;
        Ok(out)
    }
    pub fn create(
        &self,
        name: &str,
        scopes: &[&str],
        ttl: Duration,
        now: i64,
        methods: &[Method<'_>],
    ) -> Result<Issued> {
        if name.is_empty() {
            return invalid("name is required");
        }
        let scopes = auth::validate_scopes(scopes, methods)?;
        self.change(|d| issue(d, token_template(name, scopes, now), ttl, now))
    }
    pub fn create_grant(
        &self,
        node: &str,
        task: &str,
        minted_by: &str,
        ttl: Duration,
        now: i64,
        methods: &[Method<'_>],
    ) -> Result<Issued> {
        if task.trim().is_empty() || task.len() > 500 {
            return invalid("a grant needs a task of 1-500 bytes");
        }
        if minted_by.is_empty() {
            return invalid("a grant needs its minter");
        }
        if ttl.is_zero() || ttl > MAX_GRANT_TTL {
            return invalid("grant ttl must be positive and at most 24 hours");
        }
        let scopes = auth::grant_scopes(node, methods)?;
        let mut token = token_template(&format!("grant:{node}"), scopes, now);
        token.task = task.trim().into();
        token.minted_by = minted_by.into();
        self.change(|d| issue(d, token, ttl, now))
    }
    pub fn renew(&self, id: &str, ttl: Duration, now: i64) -> Result<Issued> {
        self.change(|d| {
            let token = d
                .tokens
                .iter()
                .find(|t| t.token.id == id)
                .ok_or(Error::UnknownToken)?
                .token
                .clone();
            token.check(now)?;
            if !token.task.is_empty() && (ttl.is_zero() || ttl > MAX_GRANT_TTL) {
                return invalid("grant ttl must be positive and at most 24 hours");
            }
            issue(d, token, ttl, now)
        })
    }
    pub fn list(&self) -> Vec<Token> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .disk
            .tokens
            .iter()
            .map(|t| t.token.clone())
            .collect()
    }
    pub fn lookup(&self, secret: &str, now: i64) -> Result<Token> {
        let got = hash(secret);
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut found = None;
        for (i, t) in inner.disk.tokens.iter().enumerate() {
            if bool::from(got.ct_eq(&t.hash)) {
                found = Some(i)
            }
        }
        let index = found.ok_or(Error::UnknownToken)?;
        inner.disk.tokens[index].token.check(now)?;
        inner.disk.tokens[index].token.last_used = Some(now);
        if inner.last_flush.is_none_or(|t| now.saturating_sub(t) >= 60)
            && self.persist(&inner.disk).is_ok()
        {
            inner.last_flush = Some(now)
        }
        Ok(inner.disk.tokens[index].token.clone())
    }
    pub fn revoke(&self, id: &str, now: i64) -> Result<Token> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = inner.disk.clone();
        let token = &mut next
            .tokens
            .iter_mut()
            .find(|t| t.token.id == id)
            .ok_or(Error::UnknownToken)?
            .token;
        if token.revoked_at.is_none() {
            token.revoked_at = Some(now)
        }
        let out = token.clone();
        if next != inner.disk {
            self.persist(&next)?;
        }
        inner.disk = next;
        if let Some(watches) = inner.watches.remove(id) {
            for signal in watches.into_iter().filter_map(|s| s.upgrade()) {
                signal.cancel()
            }
        }
        Ok(out)
    }
    pub fn update_scopes(
        &self,
        id_or_name: &str,
        add: &[&str],
        remove: &[&str],
        now: i64,
        methods: &[Method<'_>],
    ) -> Result<(Token, Vec<String>)> {
        let additions = if add.is_empty() {
            vec![]
        } else {
            auth::validate_scopes(add, methods)?
        };
        self.change(|d| {
            let exact = d.tokens.iter().position(|t| t.token.id == id_or_name);
            let index = if let Some(i) = exact {
                i
            } else {
                let hits: Vec<_> = d
                    .tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.token.name == id_or_name && t.token.is_active(now))
                    .map(|(i, _)| i)
                    .collect();
                match hits.len() {
                    0 => return Err(Error::UnknownToken),
                    1 => hits[0],
                    _ => return Err(Error::Ambiguous(id_or_name.into())),
                }
            };
            let t = &mut d.tokens[index].token;
            t.check(now)?;
            if t.device {
                return invalid("devices have Full access; revoke the device to remove access");
            }
            let before = t.scopes.clone();
            t.scopes.extend(additions);
            t.scopes.retain(|s| !remove.contains(&s.as_str()));
            t.scopes.sort();
            t.scopes.dedup();
            Ok((t.clone(), before))
        })
    }
    pub fn flush(&self) -> Result<()> {
        self.persist(&self.inner.lock().unwrap_or_else(|e| e.into_inner()).disk)
    }
    pub fn start_device(
        &self,
        name: &str,
        platform: &str,
        ip: &str,
        now: i64,
    ) -> Result<DeviceStarted> {
        if !valid_device_name(name) || !valid_device_label(platform, 40) {
            return invalid("invalid device name or platform");
        }
        let secret = random_hex(32)?;
        self.change(|d| {
            d.devices.retain(|r| now < r.request.expires);
            if d.devices.len() >= 1000
                || d.devices
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
                if !d.devices.iter().any(|r| r.request.user_code == code) {
                    break code;
                }
            };
            let request = DeviceAuth {
                user_code: code,
                name: name.into(),
                platform: platform.into(),
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
            d.devices.push(StoredDevice {
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
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .disk
            .devices
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
        self.change(|d| {
            let mut found = None;
            for (i, r) in d.devices.iter().enumerate() {
                if bool::from(got.ct_eq(&hash(&normalize_code(&r.request.user_code)))) {
                    found = Some(i)
                }
            }
            let i = found.ok_or(Error::DeviceRequest)?;
            let r = &mut d.devices[i].request;
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
    pub fn poll_device(&self, code: &str, now: i64) -> Result<DevicePoll> {
        let got = hash(code);
        self.change(|d| {
            let mut found = None;
            for (i, r) in d.devices.iter().enumerate() {
                if bool::from(got.ct_eq(&r.code_hash)) {
                    found = Some(i)
                }
            }
            let expired = || DevicePoll {
                status: PollStatus::Expired,
                interval: DEVICE_INTERVAL,
                issued: None,
            };
            let Some(i) = found else { return Ok(expired()) };
            let r = &mut d.devices[i].request;
            if now >= r.expires {
                return Ok(expired());
            }
            let poll = |status, interval| DevicePoll {
                status,
                interval,
                issued: None,
            };
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
            let interval = r.interval;
            let mut token = token_template(&r.name, vec![auth::SCOPE_FULL.into()], now);
            token.device = true;
            token.platform = r.platform.clone();
            token.approved_at = r.approved_at;
            token.approved_by = r.approved_by.clone();
            r.status = DeviceStatus::Consumed;
            let issued = issue(d, token, Duration::ZERO, now)?;
            Ok(DevicePoll {
                status: PollStatus::Approved,
                interval,
                issued: Some(issued),
            })
        })
    }
    pub fn rename_device(&self, id: &str, name: &str) -> Result<Token> {
        if !valid_device_name(name) {
            return invalid("invalid device name");
        }
        self.change(|d| {
            let t = &mut d
                .tokens
                .iter_mut()
                .find(|t| t.token.id == id && t.token.device)
                .ok_or(Error::UnknownToken)?
                .token;
            t.name = name.into();
            Ok(t.clone())
        })
    }
    pub fn sweep_devices(&self, now: i64) -> Result<()> {
        self.change(|d| {
            d.devices.retain(|r| now < r.request.expires);
            Ok(())
        })
    }
    /// Registration and revocation share a lock, preventing admission races.
    pub fn watch_token(&self, id: &str, now: i64) -> RevocationWatch {
        let signal = Arc::new(Signal {
            cancelled: AtomicBool::new(false),
            mutex: Mutex::new(()),
            changed: Condvar::new(),
        });
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner
            .disk
            .tokens
            .iter()
            .any(|t| t.token.id == id && t.token.is_active(now))
        {
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
        let mut b = [0u8; 1];
        getrandom::getrandom(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
        let limit = 256 - 256 % ABC.len();
        if (b[0] as usize) < limit {
            code.push(ABC[b[0] as usize % ABC.len()] as char)
        }
    }
    code.insert(4, '-');
    Ok(code)
}
struct Signal {
    cancelled: AtomicBool,
    mutex: Mutex<()>,
    changed: Condvar,
}
impl Signal {
    fn cancel(&self) {
        let _guard = self.mutex.lock().unwrap_or_else(|e| e.into_inner());
        self.cancelled.store(true, Ordering::Release);
        self.changed.notify_all()
    }
}
/// Dropping the watch releases its registration; no token secret is retained.
pub struct RevocationWatch(Arc<Signal>);
impl RevocationWatch {
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    pub fn wait(&self, timeout: Duration) -> bool {
        let guard = self.0.mutex.lock().unwrap_or_else(|e| e.into_inner());
        let _result = self
            .0
            .changed
            .wait_timeout_while(guard, timeout, |_| !self.is_cancelled())
            .unwrap_or_else(|e| e.into_inner());
        self.is_cancelled()
    }
}
