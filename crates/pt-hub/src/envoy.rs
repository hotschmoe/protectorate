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
/// labeled owner. A continuation has no `ga:` name and uses the turn's recorded agent.
pub fn from_label(state: &State, to: SleeveId, request_id: &str) -> String {
    let party = state
        .sleeve(to)
        .and_then(|s| s.messages.get(request_id))
        .map(|m| m.from);
    match party {
        Some(Party::Owner) | None => "owner".into(),
        Some(Party::Envoy) => "envoy".into(),
        Some(Party::Agent) => match request_id.strip_prefix("ga:") {
            Some(rest) => format!("agent:{}", rest.split(':').next().unwrap_or("")),
            None if request_id.starts_with("continue:") => state
                .sleeve(to)
                .and_then(|s| s.turn_agent.clone())
                .map(|name| format!("agent:{name}"))
                // No recorded name still carries the prefix, so D9 fails closed.
                .unwrap_or_else(|| "agent:".into()),
            None => "agent".into(),
        },
        Some(Party::Sleeve(x)) => state
            .sleeve(x)
            .map_or_else(|| format!("mail:{}", x.0), |s| format!("mail:{}", s.name)),
    }
}
