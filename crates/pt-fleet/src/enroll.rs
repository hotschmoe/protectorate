//! Scan to join, the hub's half (SPEC 3.2): the owner hands the hub what the node showed (the QR
//! or its text), the hub checks the control-key fingerprint against headscale's own, then
//! registers, tags and renames the node. Headscale keeps a pending registration only in memory
//! for 15 minutes and cannot list it, so `pending` is what the owner pasted or scanned.

use std::fs;
use std::path::{Path, PathBuf};

use pt_api::EpochMs;
use pt_json::{FromJson, ToJson, Value, record};

use crate::headscale::{HsNode, TAG_NODE};
use crate::{Error, Fleet, Result};

/// headscale's registration cache lifetime (`registerCacheExpiration`).
pub const PENDING_TTL_MS: u64 = 15 * 60 * 1000;

record! {
    /// What `protectorate-node join` shows as the QR, as compact JSON: `{"v":1,"auth_id":...,
    /// "control_key":"sha256:...","name":...}`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Registration {
        pub v: u32 = 1,
        pub auth_id: String,
        pub control_key: String,
        pub name: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Pending {
        pub registration: Registration,
        pub seen_at_ms: u64,
    }

    /// The pin recorded at approval: the node approved under this name had pinned this key.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Enrolled {
        pub id: u64,
        pub name: String,
        pub control_key: String,
        pub auth_id: String,
        pub approved_at_ms: u64,
    }
}

impl Registration {
    /// The QR payload or its pasted text.
    pub fn parse(text: &str) -> Result<Registration> {
        let r: Registration = pt_json::from_str(text.trim())
            .map_err(|e| Error::new(format!("not a registration: {e}")))?;
        if r.v != 1 {
            return Err(Error::new(format!(
                "registration version {} is unknown",
                r.v
            )));
        }
        valid_name(&r.name)?;
        valid_fingerprint(&r.control_key)?;
        if r.auth_id.is_empty()
            || r.auth_id.len() > 128
            || !r.auth_id.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(Error::new("registration auth_id is malformed"));
        }
        Ok(r)
    }
}

/// A tailnet DNS label: lowercase letters, digits and inner hyphens, at most 63 bytes.
pub fn valid_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!(
            "node name {name:?}: use 1 to 63 lowercase letters, digits and inner hyphens"
        )))
    }
}

fn valid_fingerprint(fp: &str) -> Result<()> {
    match fp.strip_prefix("sha256:") {
        Some(hex) if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) => Ok(()),
        _ => Err(Error::new(format!(
            "control key fingerprint {fp:?} is not sha256:<64 hex>"
        ))),
    }
}

fn pending_path(f: &Fleet) -> PathBuf {
    f.config().tailnet_dir().join("pending.json")
}

fn enrolled_path(f: &Fleet) -> PathBuf {
    f.config().tailnet_dir().join("enrolled.json")
}

fn read_list<T: FromJson>(path: &Path) -> Result<Vec<T>> {
    match fs::read(path) {
        Ok(b) => {
            pt_json::from_bytes(&b).map_err(|e| Error::new(format!("{}: {e}", path.display())))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(Error::new(format!("reading {}: {e}", path.display()))),
    }
}

fn write_list<T: ToJson>(path: &Path, items: &[T]) -> Result<()> {
    crate::write_file(
        path,
        Value::Array(items.iter().map(ToJson::to_json).collect())
            .to_string()
            .as_bytes(),
    )
}

impl Fleet {
    /// Registrations the owner scanned or pasted that are neither approved nor expired.
    pub fn pending(&self) -> Result<Vec<Pending>> {
        let _g = self.enroll_lock();
        let now = EpochMs::now().0;
        Ok(read_list::<Pending>(&pending_path(self))?
            .into_iter()
            .filter(|p| now.saturating_sub(p.seen_at_ms) < PENDING_TTL_MS)
            .collect())
    }

    /// Records a registration the owner has seen but not approved yet.
    pub fn record_pending(&self, reg: Registration) -> Result<()> {
        Registration::parse(&reg.to_json().to_string())?;
        let _g = self.enroll_lock();
        let now = EpochMs::now().0;
        let mut list: Vec<Pending> = read_list::<Pending>(&pending_path(self))?
            .into_iter()
            .filter(|p| {
                p.registration.auth_id != reg.auth_id
                    && now.saturating_sub(p.seen_at_ms) < PENDING_TTL_MS
            })
            .collect();
        list.push(Pending {
            registration: reg,
            seen_at_ms: now,
        });
        write_list(&pending_path(self), &list)
    }

    /// Every node approved through this hub, with the control key it had pinned.
    pub fn enrolled(&self) -> Result<Vec<Enrolled>> {
        let _g = self.enroll_lock();
        read_list(&enrolled_path(self))
    }

    /// Approves a node: its fingerprint must match the key headscale serves, then headscale
    /// registers it under the fleet user, tags it `tag:node` and renames it `name`.
    pub fn approve(&self, auth_id: &str, name: &str, expected_fingerprint: &str) -> Result<HsNode> {
        valid_name(name)?;
        valid_fingerprint(expected_fingerprint)?;
        let ctx = |e: Error| e.context(format!("approving {name}"));
        let served = self.control_fingerprint().map_err(ctx)?;
        if !served.eq_ignore_ascii_case(expected_fingerprint) {
            return Err(ctx(Error::new(format!(
                "control key mismatch: the node saw {expected_fingerprint}, headscale serves {served}; \
                 the node is not talking to this hub's headscale"
            ))));
        }
        if self
            .admin()
            .nodes()
            .map_err(ctx)?
            .iter()
            .any(|n| n.name == name)
        {
            return Err(ctx(Error::new(format!(
                "a node named {name} already exists"
            ))));
        }
        let admin = self.admin();
        let node = admin.register(auth_id).map_err(ctx)?;
        let node = admin.tag(node.id, TAG_NODE).map_err(ctx)?;
        let node = if node.name == name {
            node
        } else {
            admin.rename(node.id, name).map_err(ctx)?
        };
        let _g = self.enroll_lock();
        let mut enrolled: Vec<Enrolled> = read_list(&enrolled_path(self))?;
        enrolled.retain(|e| e.id != node.id);
        enrolled.push(Enrolled {
            id: node.id,
            name: name.to_string(),
            control_key: served,
            auth_id: auth_id.to_string(),
            approved_at_ms: EpochMs::now().0,
        });
        write_list(&enrolled_path(self), &enrolled)?;
        let pending: Vec<Pending> = read_list::<Pending>(&pending_path(self))?
            .into_iter()
            .filter(|p| p.registration.auth_id != auth_id)
            .collect();
        write_list(&pending_path(self), &pending)?;
        Ok(node)
    }
}
