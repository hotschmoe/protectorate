//! Every approval the owner sees: tool approvals the harnesses ask for, and action approvals the
//! D9 rule requires. Each change is streamed as an `approval` message; a blocked API call waits
//! on the condvar for the owner's decision.

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use pt_api::{
    Approval, ApprovalKind, ApprovalState, DecidedBy, Decision, EpochMs, HubStreamMessage,
};
use pt_core::SleeveId;

use crate::bus::Bus;
use crate::home::now_ms;
use crate::transcript::lock;

/// What the hub needs besides the API view: whose harness asked, under which generation.
#[derive(Clone, Debug)]
pub struct Origin {
    pub sleeve: SleeveId,
    /// Tool approvals: the incarnation that asked, and the harness's own id.
    pub g: u64,
    pub native: String,
}

struct Rec {
    approval: Approval,
    origin: Origin,
}

#[derive(Default)]
struct Inner {
    next: u64,
    map: BTreeMap<String, Rec>,
}

#[derive(Default)]
pub struct Approvals {
    inner: Mutex<Inner>,
    cond: Condvar,
}

/// The id of a tool approval: the sleeve, then the harness's own id.
pub fn tool_id(sleeve: SleeveId, native: &str) -> String {
    format!("{}:{native}", sleeve.0)
}

pub const ACTION_PREFIX: &str = "action:";

pub struct ToolAsk {
    pub sleeve: SleeveId,
    pub g: u64,
    pub native: String,
    pub title: String,
    pub summary: String,
    pub detail: String,
    pub options: Vec<Decision>,
}

fn push(bus: &Bus, a: &Approval) {
    bus.push(|seq| HubStreamMessage::Approval {
        seq,
        approval: a.clone(),
    });
}

impl Approvals {
    pub fn open_tool(&self, bus: &Bus, ask: ToolAsk) -> Approval {
        let a = Approval {
            id: tool_id(ask.sleeve, &ask.native),
            sleeve_id: ask.sleeve.0.to_string(),
            kind: ApprovalKind::Tool,
            title: ask.title,
            summary: ask.summary,
            detail: ask.detail,
            options: ask.options,
            requested_at_ms: EpochMs(now_ms()),
            timeout_at_ms: Some(EpochMs(now_ms() + pt_core::limits::APPROVAL_TIMEOUT)),
            state: ApprovalState::Pending,
            decision: None,
            decided_by: None,
            decided_at_ms: None,
        };
        let origin = Origin {
            sleeve: ask.sleeve,
            g: ask.g,
            native: ask.native,
        };
        lock(&self.inner).map.insert(
            a.id.clone(),
            Rec {
                approval: a.clone(),
                origin,
            },
        );
        push(bus, &a);
        a
    }

    /// A new pending action approval on Envoy's behalf.
    pub fn open_action(
        &self,
        bus: &Bus,
        envoy: SleeveId,
        title: String,
        summary: String,
        timeout_ms: u64,
    ) -> Approval {
        let mut inner = lock(&self.inner);
        inner.next += 1;
        let now = now_ms();
        let a = Approval {
            id: format!("{ACTION_PREFIX}{}-{}", now, inner.next),
            sleeve_id: envoy.0.to_string(),
            kind: ApprovalKind::Action,
            title,
            summary,
            detail: String::new(),
            options: vec![Decision::AllowOnce, Decision::Deny],
            requested_at_ms: EpochMs(now),
            timeout_at_ms: Some(EpochMs(now + timeout_ms)),
            state: ApprovalState::Pending,
            decision: None,
            decided_by: None,
            decided_at_ms: None,
        };
        inner.map.insert(
            a.id.clone(),
            Rec {
                approval: a.clone(),
                origin: Origin {
                    sleeve: envoy,
                    g: 0,
                    native: String::new(),
                },
            },
        );
        drop(inner);
        push(bus, &a);
        a
    }

    pub fn get(&self, id: &str) -> Option<(Approval, Origin)> {
        lock(&self.inner)
            .map
            .get(id)
            .map(|r| (r.approval.clone(), r.origin.clone()))
    }

    pub fn list(&self, pending_only: bool) -> Vec<Approval> {
        lock(&self.inner)
            .map
            .values()
            .map(|r| &r.approval)
            .filter(|a| !pending_only || a.state == ApprovalState::Pending)
            .cloned()
            .collect()
    }

    /// Ends a pending approval; `None` when it is unknown or already ended.
    pub fn resolve(
        &self,
        bus: &Bus,
        id: &str,
        state: ApprovalState,
        decision: Option<Decision>,
        by: DecidedBy,
    ) -> Option<Approval> {
        let mut inner = lock(&self.inner);
        let rec = inner.map.get_mut(id)?;
        if rec.approval.state != ApprovalState::Pending {
            return None;
        }
        let a = &mut rec.approval;
        a.state = state;
        a.decision = decision;
        a.decided_by = Some(by);
        a.decided_at_ms = Some(EpochMs(now_ms()));
        let a = a.clone();
        drop(inner);
        self.cond.notify_all();
        push(bus, &a);
        Some(a)
    }

    /// Cancels pending tool approvals of incarnations that are gone (`live` says which are not).
    pub fn cancel_gone(&self, bus: &Bus, live: &dyn Fn(SleeveId, u64) -> bool) {
        let gone: Vec<String> = lock(&self.inner)
            .map
            .values()
            .filter(|r| {
                r.approval.kind == ApprovalKind::Tool
                    && r.approval.state == ApprovalState::Pending
                    && !live(r.origin.sleeve, r.origin.g)
            })
            .map(|r| r.approval.id.clone())
            .collect();
        for id in gone {
            self.resolve(bus, &id, ApprovalState::Cancelled, None, DecidedBy::Rule);
        }
    }

    /// Blocks until approval `id` is no longer pending or `deadline` passes.
    pub fn wait(&self, id: &str, deadline: Instant) -> Option<Approval> {
        let mut inner = lock(&self.inner);
        loop {
            match inner.map.get(id) {
                Some(r) if r.approval.state != ApprovalState::Pending => {
                    return Some(r.approval.clone());
                }
                None => return None,
                Some(_) => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            inner = self
                .cond
                .wait_timeout(inner, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
}
