//! Envoy, the hub's own sleeve: its name, how the state names it, and how its D9 turn origin
//! (journaled by the core as `Sleeve::d9_turn`) reads.

use pt_core::{Party, SleeveId, State};

pub const ENVOY: &str = pt_core::ENVOY_NAME;

/// D9: who gave Envoy's current turn its origin, `agent:<name>` or `mail:<name>`; `None`
/// outside such a turn.
pub fn d9_origin(state: &State, envoy: SleeveId) -> Option<String> {
    let t = state.sleeve(envoy)?.d9_turn.as_ref()?;
    Some(party_label(state, t.from, t.agent.as_deref()))
}

/// The live (not removed) sleeve named `envoy` with no parent.
pub fn envoy_id(state: &State) -> Option<SleeveId> {
    state
        .sleeves
        .values()
        .find(|s| s.name == ENVOY && s.parent.is_none() && s.life != pt_core::Life::Removed)
        .map(|s| s.id)
}

/// The `from` label a harness sees on a user message: owner, envoy, agent:<name>, or
/// mail:<name> from a sleeve. A resleeve brief is an effect, not a message, so it is
/// labeled owner. A continuation carries the turn's agent name; an agent message with no
/// recorded name is `agent:`.
pub fn from_label(state: &State, to: SleeveId, request_id: &str) -> String {
    match state.sleeve(to).and_then(|s| s.messages.get(request_id)) {
        Some(m) => party_label(state, m.from, m.agent.as_deref()),
        None => "owner".into(),
    }
}

fn party_label(state: &State, from: Party, agent: Option<&str>) -> String {
    match from {
        Party::Owner => "owner".into(),
        Party::Envoy => "envoy".into(),
        Party::Agent => format!("agent:{}", agent.unwrap_or("")),
        Party::Sleeve(x) => state
            .sleeve(x)
            .map_or_else(|| format!("mail:{}", x.0), |s| format!("mail:{}", s.name)),
    }
}
