//! Who is calling: the owner, a paired device, Envoy, a sleeve, a vendor agent or nobody, from
//! the bearer token and, on the unix sockets, the caller's cgroup.

use pt_api::Scope;
use pt_core::{Life, Party, SleeveId, State};

use crate::envoy::envoy_id;
use crate::hub::Hub;
use crate::view::View;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Principal {
    /// The owner token, or the hub's own user on the unix socket.
    Owner,
    /// A paired app's `pd1.` token: the owner's scope.
    Device(String),
    /// Envoy's capability: sleeve scope, reaching every sleeve.
    Envoy(SleeveId),
    /// A sleeve's `pc1.` capability: itself and its children. Mail also reaches its parent and Envoy.
    Sleeve(SleeveId),
    /// Vendor general agents: the Envoy methods only (D9).
    EnvoyOnly,
    /// No credential: public methods only.
    Anonymous,
}

impl Principal {
    pub fn scope(&self) -> Scope {
        match self {
            Principal::Owner | Principal::Device(_) => Scope::Owner,
            Principal::Envoy(_) | Principal::Sleeve(_) => Scope::Sleeve,
            Principal::EnvoyOnly => Scope::EnvoyOnly,
            Principal::Anonymous => Scope::Public,
        }
    }

    pub fn party(&self) -> Party {
        match self {
            Principal::Owner | Principal::Device(_) | Principal::Anonymous => Party::Owner,
            Principal::Envoy(_) => Party::Envoy,
            Principal::Sleeve(x) => Party::Sleeve(*x),
            Principal::EnvoyOnly => Party::Agent,
        }
    }

    /// Reaches every sleeve.
    pub fn full(&self) -> bool {
        matches!(
            self,
            Principal::Owner | Principal::Device(_) | Principal::Envoy(_)
        )
    }
}

pub fn sleeve_principal(state: &State, id: SleeveId) -> Principal {
    if envoy_id(state) == Some(id) {
        Principal::Envoy(id)
    } else {
        Principal::Sleeve(id)
    }
}

pub fn from_token(hub: &Hub, view: &View, token: &str) -> Option<Principal> {
    if hub.tokens.is_owner(token) {
        return Some(Principal::Owner);
    }
    if let Some(id) = hub.devices.check(token) {
        return Some(Principal::Device(id));
    }
    if token == hub.tokens.envoy_only() {
        return Some(Principal::EnvoyOnly);
    }
    view.state
        .sleeves
        .values()
        .filter(|s| s.life != Life::Removed)
        .find(|s| hub.tokens.sleeve(s.id) == token)
        .map(|s| sleeve_principal(&view.state, s.id))
}

/// The cgroup of `pid` from `/proc/<pid>/cgroup`, or `None` when it cannot be read (the
/// process is gone, or never was).
pub fn peer_cgroup(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()
}

pub enum CgroupSleeve {
    /// In no sleeve scope of this hub.
    None,
    Live(Principal),
    /// In the scope of a sleeve that is removed or unknown: refused.
    Gone,
}

/// The sleeve whose scope a cgroup is. A process inside a sleeve is that sleeve whatever token
/// it presents.
pub fn cgroup_sleeve(hub: &Hub, view: &View, cgroup: &str) -> CgroupSleeve {
    let needle = format!("/pt-{}-", hub.tokens.hub_tag);
    let Some(at) = cgroup.find(&needle).map(|i| i + needle.len()) else {
        return CgroupSleeve::None;
    };
    let digits: String = cgroup[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let Ok(n) = digits.parse() else {
        return CgroupSleeve::Gone;
    };
    let id = SleeveId(n);
    match view.state.sleeve(id) {
        Some(s) if s.life != Life::Removed => CgroupSleeve::Live(sleeve_principal(&view.state, id)),
        _ => CgroupSleeve::Gone,
    }
}
