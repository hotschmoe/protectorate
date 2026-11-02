//! Envoy, the hub's own sleeve: its name, how the state names it, and who started its current
//! turn (D9).

use pt_core::{Party, SleeveId, State};

pub const ENVOY: &str = pt_core::ENVOY_NAME;

/// D9: who started Envoy's current turn. `origin` is `agent:<name>` or `mail:<name>` when an
/// agent message, or mail from a D9-marked sleeve, was written into it (as its prompt or a
/// steer). A crash keeps the origin and the spawn count: the exit does not clear them, and
/// the continuation replaces the origin. A usage-limit or auth settle keeps them too, because
/// Envoy resleeves and that brief is sent as the owner, so it cannot put the origin back.
/// A finished settle, or a prompt that is not a continuation or a resleeve brief, clears both.
#[derive(Debug, Default)]
pub struct EnvoyTurn {
    pub origin: Option<String>,
    /// Sleeves Envoy spawned in this turn.
    pub spawns: u64,
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
    let Some(m) = state.sleeve(to).and_then(|s| s.messages.get(request_id)) else {
        return "owner".into();
    };
    match m.from {
        Party::Owner => "owner".into(),
        Party::Envoy => "envoy".into(),
        Party::Agent => format!("agent:{}", m.agent.as_deref().unwrap_or("")),
        Party::Sleeve(x) => state
            .sleeve(x)
            .map_or_else(|| format!("mail:{}", x.0), |s| format!("mail:{}", s.name)),
    }
}
