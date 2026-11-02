//! What the core publishes after each journaled batch for every other thread to read.

use std::collections::BTreeMap;

use pt_core::{EffectId, EffectKind, EffectState, Millis, Proc, SleeveId, State};

/// The read-only snapshot published after each journaled batch.
pub struct View {
    pub state: State,
    pub batch: u64,
    /// When each sleeve's activity last changed kind.
    pub since: BTreeMap<SleeveId, Millis>,
}

impl View {
    /// Whether the record is still exactly what this intent was released for.
    pub fn fresh(&self, effect: EffectId, attempt: u32, kind: &EffectKind) -> bool {
        let st = &self.state;
        let Some(e) = st.effects.get(&effect) else {
            return false;
        };
        if e.attempt != attempt || e.state != EffectState::Intent {
            return false;
        }
        if let Some((sid, g)) = kind.incarnation()
            && st.sleeve(sid).is_none_or(|s| s.g != g)
        {
            return false;
        }
        if let EffectKind::StartIncarnation { sleeve, g, .. } = kind {
            return st
                .sleeve(*sleeve)
                .is_some_and(|s| s.g == *g && s.proc == Proc::Starting(effect));
        }
        true
    }
}
