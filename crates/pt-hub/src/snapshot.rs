//! The hub as a client first sees it: `GetState` and the stream's opening snapshot.

use pt_api as api;
use pt_core::{Life, Sleeve, SleeveId};

use crate::approvals;
use crate::bus::SNAPSHOT_EVENTS;
use crate::fleet::Status;
use crate::hub::Hub;
use crate::jobs;
use crate::lock::lock;
use crate::view::View;
use crate::wire;

pub fn hub_info(hub: &Hub) -> api::HubInfo {
    let mut info = lock(&hub.info).clone();
    info.history_capped = hub.history.capped_sleeves();
    info
}

/// What the poller last learned of every node; none while the fleet is off or down.
pub fn nodes(hub: &Hub) -> Vec<api::Node> {
    match hub.fleet.status() {
        Status::Up(up) => up.poller.nodes().into_iter().map(|v| v.node).collect(),
        _ => Vec::new(),
    }
}

/// The full snapshot at stream seq `seq`, limited to the sleeves `visible` admits.
pub fn state(
    hub: &Hub,
    view: &View,
    seq: u64,
    visible: &dyn Fn(SleeveId) -> bool,
) -> api::HubState {
    let st = &view.state;
    let shown: Vec<&Sleeve> = st
        .sleeves
        .values()
        .filter(|s| s.life != Life::Removed && visible(s.id))
        .collect();
    let approvals = st
        .approvals
        .values()
        .filter(|a| visible(a.sleeve))
        .map(approvals::view)
        .collect();
    api::HubState {
        seq,
        hub: hub_info(hub),
        workspaces: shown
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| wire::workspace(s))
            .collect(),
        sleeves: shown
            .iter()
            .map(|s| wire::sleeve(&hub.transcripts, view, s))
            .collect(),
        approvals,
        nodes: nodes(hub),
        jobs: st.jobs.values().map(|j| jobs::view(&hub.jobs, j)).collect(),
        quotas: hub.quotas.list(),
        events: hub.bus.recent_events(SNAPSHOT_EVENTS),
    }
}
