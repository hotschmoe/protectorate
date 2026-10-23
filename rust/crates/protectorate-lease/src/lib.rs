//! Persistent resource leases and priority/FIFO tickets. Times are Unix
//! milliseconds; callers drive `sweep` with their scheduler. One hub owns a file.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
pub const MAX_LEASE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CLAIM_WINDOW: Duration = Duration::from_secs(120);
pub const MAX_QUEUE_WAIT: Duration = Duration::from_secs(24 * 60 * 60);
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Busy(String),
    Unavailable(String),
    NotFound(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "invalid lease: {s}"),
            Self::Busy(s) => write!(f, "resource is leased: {s}"),
            Self::Unavailable(s) => write!(f, "resource unavailable: {s}"),
            Self::NotFound(s) => write!(f, "lease {s:?} not found or expired"),
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub node: String,
    pub gpu: Option<u32>,
}
impl Resource {
    pub fn conflicts(&self, other: &Self) -> bool {
        self.node == other.node
            && (self.gpu.is_none() || other.gpu.is_none() || self.gpu == other.gpu)
    }
}
impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.node)?;
        if let Some(g) = self.gpu {
            write!(f, ":gpu{g}")?
        }
        Ok(())
    }
}
fn valid_node(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 63
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}
fn valid_resource(r: &Resource) -> bool {
    valid_node(&r.node)
}
pub fn parse_resource(raw: &str) -> Result<Resource> {
    let raw = raw.trim().to_ascii_lowercase();
    let (node, gpu) = match raw.split_once(':') {
        Some((node, suffix)) => {
            let n = suffix
                .strip_prefix("gpu")
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| Error::Invalid("want node or node:gpu<index>".into()))?;
            (node, Some(n))
        }
        None => (raw.as_str(), None),
    };
    if !valid_node(node) {
        return Err(Error::Invalid("invalid node name".into()));
    }
    Ok(Resource {
        node: node.into(),
        gpu,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    pub id: String,
    pub resource: Resource,
    pub holder: String,
    pub ttl: Duration,
    pub acquired_at: i64,
    pub expires_at: i64,
    /// Present until the first renewal claims a queued grant.
    pub wanted_ttl: Option<Duration>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ticket {
    pub id: String,
    pub resource: String,
    pub holder: String,
    pub notify: String,
    pub priority: i32,
    pub sequence: u64,
    pub enqueued_at: i64,
    pub ttl: Duration,
    /// Opaque selector passed back to the chooser, including after restart.
    pub request: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Active(Lease),
    Queued { ticket: Ticket, position: usize },
}
impl Entry {
    pub fn id(&self) -> &str {
        match self {
            Self::Active(l) => &l.id,
            Self::Queued { ticket, .. } => &ticket.id,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Wait {
    pub holder: String,
    pub notify: String,
    pub resource: String,
    pub request: String,
    pub ttl: Duration,
    pub priority: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Acquired,
    Renewed,
    Released,
    Expired,
    Queued,
    Granted,
    Dropped,
}
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub lease: Option<Lease>,
    pub ticket: Option<Ticket>,
    pub position: Option<usize>,
    pub reason: String,
}
impl Event {
    fn lease(kind: EventKind, lease: Lease) -> Self {
        Self {
            kind,
            lease: Some(lease),
            ticket: None,
            position: None,
            reason: String::new(),
        }
    }
    fn ticket(kind: EventKind, ticket: Ticket, reason: String) -> Self {
        Self {
            kind,
            lease: None,
            ticket: Some(ticket),
            position: None,
            reason,
        }
    }
}
/// Choosers run under the table lock and must not reenter it. `blocked` is a
/// consistent view; the table independently verifies the returned resource.
pub type Chooser =
    Arc<dyn Fn(&str, &dyn Fn(&Resource) -> Option<Lease>) -> Result<Resource> + Send + Sync>;
type EventHandler = Arc<dyn Fn(&Event) + Send + Sync>;
type GrantHandler = Arc<dyn Fn(&Ticket, &Lease) + Send + Sync>;
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Disk {
    #[serde(default)]
    leases: BTreeMap<String, Lease>,
    #[serde(default)]
    queue: Vec<Ticket>,
    #[serde(default)]
    next_sequence: u64,
}
pub struct LeaseTable {
    path: Option<PathBuf>,
    disk: Mutex<Disk>,
    chooser: Chooser,
    admission: Option<Arc<std::sync::RwLock<()>>>,
    on_event: Option<EventHandler>,
    on_grant: Option<GrantHandler>,
}
fn ttl_millis(ttl: Duration) -> Result<i64> {
    if ttl.is_zero() || ttl > MAX_LEASE_TTL {
        return Err(Error::Invalid(
            "ttl must be positive and at most 24 hours".into(),
        ));
    }
    Ok(i64::try_from(ttl.as_nanos().div_ceil(1_000_000)).unwrap())
}
fn expiry(now: i64, ttl: Duration) -> Result<i64> {
    now.checked_add(ttl_millis(ttl)?)
        .ok_or_else(|| Error::Invalid("expiry timestamp overflow".into()))
}
fn holder_ttl(holder: &str, ttl: Duration) -> Result<()> {
    if holder.trim().is_empty() {
        return Err(Error::Invalid("holder is required".into()));
    }
    ttl_millis(ttl)?;
    Ok(())
}
fn sort_queue(queue: &mut [Ticket]) {
    queue.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.sequence.cmp(&b.sequence))
    })
}
fn random_id(prefix: &str, d: &Disk) -> Result<String> {
    loop {
        let mut b = [0u8; 8];
        getrandom::getrandom(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
        let id = format!(
            "{prefix}{}",
            b.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        if !d.leases.contains_key(&id) && !d.queue.iter().any(|q| q.id == id) {
            return Ok(id);
        }
    }
}
fn blocked(d: &Disk, resource: &Resource) -> Option<Lease> {
    d.leases
        .values()
        .find(|l| l.resource.conflicts(resource))
        .cloned()
}
fn acquire_into(
    d: &mut Disk,
    chooser: &Chooser,
    holder: &str,
    ttl: Duration,
    want: Option<Duration>,
    request: &str,
    now: i64,
) -> Result<Lease> {
    let resource = chooser(request, &|r| blocked(d, r))?;
    if !valid_resource(&resource) {
        return Err(Error::Invalid(
            "chooser returned an invalid resource".into(),
        ));
    }
    if let Some(l) = blocked(d, &resource) {
        return Err(Error::Busy(format!("{} held by {}", resource, l.holder)));
    }
    let lease = Lease {
        id: random_id("lease-", d)?,
        resource,
        holder: holder.into(),
        ttl,
        acquired_at: now,
        expires_at: expiry(now, ttl)?,
        wanted_ttl: want,
    };
    d.leases.insert(lease.id.clone(), lease.clone());
    Ok(lease)
}
impl LeaseTable {
    pub fn memory() -> Self {
        Self {
            path: None,
            disk: Mutex::new(Disk::default()),
            chooser: Arc::new(|request, blocked| {
                let resource = parse_resource(request)?;
                if let Some(l) = blocked(&resource) {
                    return Err(Error::Busy(format!("{} held by {}", resource, l.holder)));
                }
                Ok(resource)
            }),
            admission: None,
            on_event: None,
            on_grant: None,
        }
    }
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut disk: Disk = match fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Disk::default(),
            Err(e) => return Err(e.into()),
        };
        let mut ids = BTreeSet::new();
        for (id, l) in &disk.leases {
            if id.is_empty()
                || id != &l.id
                || !ids.insert(id.clone())
                || !valid_resource(&l.resource)
                || holder_ttl(&l.holder, l.ttl).is_err()
                || l.expires_at < l.acquired_at
                || l.wanted_ttl.is_some_and(|t| ttl_millis(t).is_err())
            {
                return Err(Error::Invalid("corrupt persisted lease".into()));
            }
        }
        let mut seqs = BTreeSet::new();
        for q in &disk.queue {
            if q.id.is_empty()
                || !ids.insert(q.id.clone())
                || !seqs.insert(q.sequence)
                || holder_ttl(&q.holder, q.ttl).is_err()
            {
                return Err(Error::Invalid("corrupt persisted ticket".into()));
            }
            disk.next_sequence = disk.next_sequence.max(q.sequence);
        }
        let values: Vec<_> = disk.leases.values().collect();
        for (i, a) in values.iter().enumerate() {
            if values[i + 1..].iter().any(|b| {
                a.resource.conflicts(&b.resource)
                    && a.acquired_at < b.expires_at
                    && b.acquired_at < a.expires_at
            }) {
                return Err(Error::Invalid("overlapping persisted leases".into()));
            }
        }
        sort_queue(&mut disk.queue);
        let mut table = Self::memory();
        table.path = Some(path);
        *table.disk.get_mut().unwrap() = disk;
        Ok(table)
    }
    pub fn with_chooser(mut self, chooser: Chooser) -> Self {
        self.chooser = chooser;
        self
    }
    /// Serialize every lease mutation with an external node drain barrier.
    /// A drain takes the write side before marking a node unavailable.
    pub fn with_admission(mut self, admission: Arc<std::sync::RwLock<()>>) -> Self {
        self.admission = Some(admission);
        self
    }
    /// Notifications run after persistence and outside the lock; reentry is allowed.
    pub fn with_on_event(mut self, handler: impl Fn(&Event) + Send + Sync + 'static) -> Self {
        self.on_event = Some(Arc::new(handler));
        self
    }
    pub fn with_on_grant(
        mut self,
        handler: impl Fn(&Ticket, &Lease) + Send + Sync + 'static,
    ) -> Self {
        self.on_grant = Some(Arc::new(handler));
        self
    }
    fn persist(&self, d: &Disk) -> Result<()> {
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
        protectorate_fsutil::write_file(path, &serde_json::to_vec_pretty(d)?, 0o600)?;
        Ok(())
    }
    fn transaction<T>(
        &self,
        change: impl FnOnce(&mut Disk, &mut Vec<Event>) -> Result<T>,
    ) -> Result<T> {
        let _admission = self
            .admission
            .as_ref()
            .map(|lock| lock.read().unwrap_or_else(|error| error.into_inner()));
        let mut guard = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let mut events = Vec::new();
        let out = change(&mut next, &mut events)?;
        if next != *guard {
            self.persist(&next)?;
        }
        *guard = next;
        drop(guard);
        for event in events {
            if let Some(handler) = &self.on_event {
                handler(&event)
            }
            if event.kind == EventKind::Granted {
                if let (Some(handler), Some(t), Some(l)) =
                    (&self.on_grant, &event.ticket, &event.lease)
                {
                    handler(t, l)
                }
            }
        }
        Ok(out)
    }
    fn grant_into(&self, d: &mut Disk, events: &mut Vec<Event>, now: i64) -> Result<()> {
        let mut i = 0;
        while i < d.queue.len() {
            let ticket = d.queue[i].clone();
            if now.saturating_sub(ticket.enqueued_at) > MAX_QUEUE_WAIT.as_millis() as i64 {
                d.queue.remove(i);
                events.push(Event::ticket(
                    EventKind::Dropped,
                    ticket,
                    "waited too long".into(),
                ));
                continue;
            }
            let ttl = CLAIM_WINDOW.min(ticket.ttl);
            match acquire_into(
                d,
                &self.chooser,
                &ticket.holder,
                ttl,
                Some(ticket.ttl),
                &ticket.request,
                now,
            ) {
                Ok(l) => {
                    d.queue.remove(i);
                    events.push(Event::lease(EventKind::Acquired, l.clone()));
                    events.push(Event {
                        kind: EventKind::Granted,
                        lease: Some(l),
                        ticket: Some(ticket),
                        position: None,
                        reason: String::new(),
                    });
                }
                Err(Error::Invalid(reason)) => {
                    d.queue.remove(i);
                    events.push(Event::ticket(EventKind::Dropped, ticket, reason));
                }
                Err(Error::Busy(_) | Error::Unavailable(_)) => i += 1,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn sweep(&self, now: i64) -> Result<()> {
        self.transaction(|d, events| {
            let expired: Vec<_> = d
                .leases
                .values()
                .filter(|l| l.expires_at <= now)
                .cloned()
                .collect();
            for l in expired {
                d.leases.remove(&l.id);
                events.push(Event::lease(EventKind::Expired, l));
            }
            self.grant_into(d, events, now)
        })
    }
    pub fn acquire(&self, holder: &str, ttl: Duration, request: &str, now: i64) -> Result<Lease> {
        holder_ttl(holder, ttl)?;
        self.sweep(now)?;
        self.transaction(|d, events| {
            let l = acquire_into(d, &self.chooser, holder, ttl, None, request, now)?;
            events.push(Event::lease(EventKind::Acquired, l.clone()));
            Ok(l)
        })
    }
    pub fn acquire_wait(&self, w: Wait, now: i64) -> Result<Entry> {
        holder_ttl(&w.holder, w.ttl)?;
        self.sweep(now)?;
        self.transaction(|d, events| {
            match acquire_into(d, &self.chooser, &w.holder, w.ttl, None, &w.request, now) {
                Ok(l) => {
                    events.push(Event::lease(EventKind::Acquired, l.clone()));
                    return Ok(Entry::Active(l));
                }
                Err(Error::Busy(_)) => {}
                Err(e) => return Err(e),
            }
            d.next_sequence = d
                .next_sequence
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("queue sequence overflow".into()))?;
            let ticket = Ticket {
                id: random_id("ticket-", d)?,
                resource: w.resource,
                holder: w.holder,
                notify: w.notify,
                priority: w.priority,
                sequence: d.next_sequence,
                enqueued_at: now,
                ttl: w.ttl,
                request: w.request,
            };
            d.queue.push(ticket.clone());
            sort_queue(&mut d.queue);
            let position = d.queue.iter().position(|q| q.id == ticket.id).unwrap() + 1;
            let mut event = Event::ticket(EventKind::Queued, ticket.clone(), String::new());
            event.position = Some(position);
            events.push(event);
            Ok(Entry::Queued { ticket, position })
        })
    }
    pub fn renew(&self, id: &str, ttl: Option<Duration>, now: i64) -> Result<Lease> {
        self.sweep(now)?;
        self.transaction(|d, events| {
            let l = d
                .leases
                .get_mut(id)
                .ok_or_else(|| Error::NotFound(id.into()))?;
            let ttl = ttl
                .filter(|t| !t.is_zero())
                .unwrap_or(l.wanted_ttl.unwrap_or(l.ttl));
            l.expires_at = expiry(now, ttl)?;
            l.ttl = ttl;
            l.wanted_ttl = None;
            let out = l.clone();
            events.push(Event::lease(EventKind::Renewed, out.clone()));
            Ok(out)
        })
    }
    pub fn release(&self, id: &str, now: i64) -> Result<()> {
        self.sweep(now)?;
        self.transaction(|d, events| {
            if let Some(i) = d.queue.iter().position(|q| q.id == id) {
                events.push(Event::ticket(
                    EventKind::Dropped,
                    d.queue.remove(i),
                    "cancelled".into(),
                ));
                return Ok(());
            }
            let l = d
                .leases
                .remove(id)
                .ok_or_else(|| Error::NotFound(id.into()))?;
            events.push(Event::lease(EventKind::Released, l));
            self.grant_into(d, events, now)
        })
    }
    /// Returns active leases or queued tickets, including one-based queue position.
    pub fn get(&self, id: &str, now: i64) -> Result<Entry> {
        self.sweep(now)?;
        let guard = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(l) = guard.leases.get(id) {
            return Ok(Entry::Active(l.clone()));
        }
        if let Some((i, t)) = guard.queue.iter().enumerate().find(|(_, t)| t.id == id) {
            return Ok(Entry::Queued {
                ticket: t.clone(),
                position: i + 1,
            });
        }
        Err(Error::NotFound(id.into()))
    }
    pub fn list(&self, now: i64) -> Result<Vec<Entry>> {
        self.sweep(now)?;
        let d = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        let mut active: Vec<_> = d.leases.values().cloned().collect();
        active.sort_by(|a, b| a.expires_at.cmp(&b.expires_at).then(a.id.cmp(&b.id)));
        Ok(active
            .into_iter()
            .map(Entry::Active)
            .chain(d.queue.iter().enumerate().map(|(i, t)| Entry::Queued {
                ticket: t.clone(),
                position: i + 1,
            }))
            .collect())
    }
    pub fn blocked(&self, resource: &Resource, now: i64) -> Result<Option<Lease>> {
        self.sweep(now)?;
        Ok(blocked(
            &self.disk.lock().unwrap_or_else(|e| e.into_inner()),
            resource,
        ))
    }
}
