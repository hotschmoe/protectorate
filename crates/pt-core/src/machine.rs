use crate::intent::{Code, Intent, Notice, Reply};
use crate::model::*;
use crate::state::*;

pub(crate) struct Core<'a> {
    pub st: &'a mut State,
    pub out: Vec<Intent>,
    pub now: Millis,
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

    /// The sleeve, which callers have already checked exists.
    pub(crate) fn sl(&mut self, id: SleeveId) -> &mut Sleeve {
        self.st.sleeves.get_mut(&id).expect("sleeve exists")
    }
}
