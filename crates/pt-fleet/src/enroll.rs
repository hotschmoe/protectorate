//! Scan to join, the hub's half (SPEC 3.2): the owner hands the hub what the node showed (the QR
//! or its text), the hub checks the control-key fingerprint against headscale's own, then
//! registers, tags and renames the node. Headscale keeps a pending registration only in memory
//! for 15 minutes and cannot list it, so `pending` is what the owner pasted or scanned.

use std::fs;
use std::path::{Path, PathBuf};

use pt_api::EpochMs;
use pt_json::{FromJson, ToJson, Value, record};

use crate::headscale::{HsNode, TAG_HUB, TAG_NODE};
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

/// What `Fleet::remove_node` did.
#[derive(Debug, Clone, PartialEq)]
pub struct Removal {
    pub id: u64,
    pub name: String,
    /// Headscale had already forgotten the node; only its enrollment record was dropped.
    pub record_only: bool,
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

/// A tailnet DNS label: lowercase letters, digits and inner hyphens, at most 63 bytes, and not
/// digits alone, which would read as a node id.
pub fn valid_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.bytes().all(|b| b.is_ascii_digit())
        && !name.starts_with('-')
        && !name.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!(
            "node name {name:?}: use 1 to 63 lowercase letters, digits and inner hyphens, \
             not digits alone (that reads as a node id)"
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
    f.config().dir.join("pending.json")
}

fn enrolled_path(f: &Fleet) -> PathBuf {
    f.config().dir.join("enrolled.json")
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
        // Held across headscale's steps too, so a rename or removal cannot slip in between.
        let _g = self.enroll_lock();
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

    /// Renames node `id` in headscale, then in its enrollment record. When the record cannot be
    /// written, headscale's name is put back; the error says if that failed too.
    pub fn rename_node(&self, id: u64, to: &str) -> Result<HsNode> {
        valid_name(to)?;
        let ctx = |e: Error| e.context(format!("renaming node {id} to {to}"));
        let _g = self.enroll_lock();
        let admin = self.admin();
        let nodes = admin.nodes().map_err(ctx)?;
        let node = nodes
            .iter()
            .find(|n| n.id == id && n.has_tag(TAG_NODE))
            .ok_or_else(|| ctx(Error::new(format!("headscale has no node {id}"))))?;
        if let Some(other) = nodes.iter().find(|n| n.id != id && n.name == to) {
            return Err(ctx(Error::new(format!(
                "a node named {to} already exists (headscale node {})",
                other.id
            ))));
        }
        let path = enrolled_path(self);
        let mut enrolled: Vec<Enrolled> = read_list(&path).map_err(ctx)?;
        // Headscale may hold the name already: an earlier rename stopped before its record.
        let renamed = if node.name == to {
            node.clone()
        } else {
            admin.rename(id, to).map_err(ctx)?
        };
        if renamed.name != to {
            return Err(ctx(Error::new(format!(
                "headscale named it {:?} instead",
                renamed.name
            ))));
        }
        let mut changed = false;
        for e in enrolled.iter_mut().filter(|e| e.id == id && e.name != to) {
            e.name = to.to_string();
            changed = true;
        }
        if !changed {
            return Ok(renamed);
        }
        if let Err(e) = write_list(&path, &enrolled) {
            if node.name == to {
                return Err(ctx(e.context("updating the enrollment record")));
            }
            return Err(ctx(match admin.rename(id, &node.name) {
                Ok(_) => e.context(format!(
                    "updating the enrollment record failed, so headscale's name is back to {}",
                    node.name
                )),
                Err(back) => Error::new(format!(
                    "headscale names it {to} but the enrollment record still says {}: {e}; \
                     putting headscale's name back failed too: {back}",
                    node.name
                )),
            }));
        }
        Ok(renamed)
    }

    /// Deletes node `id` from headscale, which revokes its key, then drops its enrollment record.
    /// A node only the record still lists (an earlier removal cut short) loses the record.
    pub fn remove_node(&self, id: u64) -> Result<Removal> {
        let ctx = |e: Error| e.context(format!("removing node {id}"));
        let _g = self.enroll_lock();
        let admin = self.admin();
        let path = enrolled_path(self);
        let mut enrolled: Vec<Enrolled> = read_list(&path).map_err(ctx)?;
        let listed = admin.nodes().map_err(ctx)?.into_iter().find(|n| n.id == id);
        let recorded = enrolled.iter().find(|e| e.id == id).map(|e| e.name.clone());
        let name = match (&listed, recorded) {
            (Some(n), _) if n.has_tag(TAG_HUB) || !n.has_tag(TAG_NODE) => {
                return Err(ctx(Error::new(format!(
                    "headscale node {id} ({}) is not a fleet node",
                    n.name
                ))));
            }
            (Some(n), _) => n.name.clone(),
            (None, Some(name)) => name,
            (None, None) => return Err(ctx(Error::new(format!("no node {id}")))),
        };
        if listed.is_some() {
            admin.delete(id).map_err(ctx)?;
        }
        let before = enrolled.len();
        enrolled.retain(|e| e.id != id);
        if enrolled.len() != before {
            write_list(&path, &enrolled).map_err(|e| {
                ctx(e.context(format!(
                    "headscale no longer has {name}, but dropping its enrollment record failed; \
                     finish with: protectorate node rm {id}"
                )))
            })?;
        }
        Ok(Removal {
            id,
            name,
            record_only: listed.is_none(),
        })
    }
}
