use std::collections::BTreeSet;

use crate::intent::{Code, Intent, Notice, Reply};
use crate::model::*;
use crate::state::*;

pub(crate) struct Core<'a> {
    pub st: &'a mut State,
    pub out: Vec<Intent>,
    pub now: Millis,
    #[cfg(feature = "fuzz-reference")]
    pub reference_jobs: bool,
    pub dirty_sleeves: BTreeSet<SleeveId>,
}

impl Core<'_> {
    pub(crate) fn reply(&mut self, r: Reply) {
        self.out.push(Intent::Reply(r));
    }

    pub(crate) fn refuse(&mut self, code: Code, why: impl Into<String>) {
        self.reply(Reply::Refused(code, why.into()));
    }

    pub(crate) fn notify(&mut self, to: Party, sleeve: Option<SleeveId>, notice: Notice) {
        self.out.push(Intent::Notify { to, sleeve, notice });
    }

    pub(crate) fn index_sleeves(&mut self) {
        for id in std::mem::take(&mut self.dirty_sleeves) {
            let s = &self.st.sleeves[&id];
            if s.proc != Proc::Down || matches!(s.guard.state, GuardState::Cooldown { .. }) {
                self.st.ticking_sleeves.insert(id);
            } else {
                self.st.ticking_sleeves.remove(&id);
            }
            if s.proc == Proc::Up
                || s.guard.state == GuardState::Selecting
                || (s.auto_continue.is_some()
                    && matches!(s.activity, Activity::Idle { .. })
                    && s.guard.state == GuardState::Armed)
            {
                self.st.recovering_sleeves.insert(id);
            } else {
                self.st.recovering_sleeves.remove(&id);
            }
            if s.retry_unwritten && s.foreign.is_none() {
                self.st.unwritten_sleeves.insert(id);
            } else {
                self.st.unwritten_sleeves.remove(&id);
            }
        }
    }

    /// The sleeve, which callers have already checked exists.
    pub(crate) fn sl(&mut self, id: SleeveId) -> &mut Sleeve {
        self.dirty_sleeves.insert(id);
        self.st.sleeves.get_mut(&id).expect("sleeve exists")
    }
}
