//! The journal encoding of [`Event`]: a version byte, then tag bytes, little-endian fixed-width
//! integers and u32-length-prefixed UTF-8 strings. The encoding is canonical: `decode` accepts
//! exactly the bytes `encode` produces, and fails loudly on an unknown tag or trailing bytes.
//!
//! Journals are never rewritten, so a tag keeps its layout forever: new variants and fields get
//! new tags. `Obs::ApprovalRequested` is tag 2 without an ask and tag 11 with one, and
//! `EventKind::Spawn` is tag 0 without an effort and tag 32 with one.

use crate::event::*;
use crate::model::*;

pub const FORMAT: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Format(u8),
    Truncated,
    UnknownTag { what: &'static str, tag: u8 },
    Utf8,
    Trailing(usize),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Format(v) => write!(f, "event format {v} (this build reads {FORMAT})"),
            DecodeError::Truncated => write!(f, "event truncated"),
            DecodeError::UnknownTag { what, tag } => write!(f, "unknown {what} tag {tag}"),
            DecodeError::Utf8 => write!(f, "string is not UTF-8"),
            DecodeError::Trailing(n) => write!(f, "{n} bytes after the event"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn encode(e: &Event) -> Vec<u8> {
    let mut w = Vec::with_capacity(64);
    w.push(FORMAT);
    e.put(&mut w);
    w
}

pub fn decode(buf: &[u8]) -> Result<Event, DecodeError> {
    let mut r = Reader { buf, pos: 0 };
    let v = r.u8()?;
    if v != FORMAT {
        return Err(DecodeError::Format(v));
    }
    let e = Event::get(&mut r)?;
    if r.pos != buf.len() {
        return Err(DecodeError::Trailing(buf.len() - r.pos));
    }
    Ok(e)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.buf.len() - self.pos < n {
            return Err(DecodeError::Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn tag(&mut self, what: &'static str, max: u8) -> Result<u8, DecodeError> {
        let tag = self.u8()?;
        if tag > max {
            return Err(DecodeError::UnknownTag { what, tag });
        }
        Ok(tag)
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
}

trait Wire: Sized {
    fn put(&self, w: &mut Vec<u8>);
    fn get(r: &mut Reader) -> Result<Self, DecodeError>;
}

impl Wire for u8 {
    fn put(&self, w: &mut Vec<u8>) {
        w.push(*self);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        r.u8()
    }
}

macro_rules! le_int {
    ($t:ty, $n:expr) => {
        impl Wire for $t {
            fn put(&self, w: &mut Vec<u8>) {
                w.extend_from_slice(&self.to_le_bytes());
            }
            fn get(r: &mut Reader) -> Result<Self, DecodeError> {
                Ok(<$t>::from_le_bytes(r.take($n)?.try_into().expect("width")))
            }
        }
    };
}
le_int!(u32, 4);
le_int!(u64, 8);
le_int!(i32, 4);

impl Wire for bool {
    fn put(&self, w: &mut Vec<u8>) {
        w.push(*self as u8);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(r.tag("bool", 1)? == 1)
    }
}

impl Wire for String {
    fn put(&self, w: &mut Vec<u8>) {
        (self.len() as u32).put(w);
        w.extend_from_slice(self.as_bytes());
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        let n = u32::get(r)? as usize;
        let b = r.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| DecodeError::Utf8)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            None => w.push(0),
            Some(v) => {
                w.push(1);
                v.put(w);
            }
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.tag("option", 1)? {
            0 => None,
            _ => Some(T::get(r)?),
        })
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn put(&self, w: &mut Vec<u8>) {
        (self.len() as u32).put(w);
        for v in self {
            v.put(w);
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        let n = u32::get(r)? as usize;
        // Every element takes at least one byte, so this bounds the allocation.
        if n > r.remaining() {
            return Err(DecodeError::Truncated);
        }
        (0..n).map(|_| T::get(r)).collect()
    }
}

macro_rules! id {
    ($t:ident, $inner:ty) => {
        impl Wire for $t {
            fn put(&self, w: &mut Vec<u8>) {
                self.0.put(w);
            }
            fn get(r: &mut Reader) -> Result<Self, DecodeError> {
                Ok($t(<$inner>::get(r)?))
            }
        }
    };
}
id!(SleeveId, u64);
id!(Gen, u64);
id!(EffectId, u64);
id!(JobId, u64);
id!(NodeId, u64);

/// Unit-only enums: one tag byte, in listed order.
macro_rules! unit_enum {
    ($t:ident, $what:expr, [$($v:ident),+ $(,)?]) => {
        impl Wire for $t {
            fn put(&self, w: &mut Vec<u8>) {
                const ALL: &[$t] = &[$($t::$v),+];
                w.push(ALL.iter().position(|x| x == self).expect("listed") as u8);
            }
            fn get(r: &mut Reader) -> Result<Self, DecodeError> {
                const ALL: &[$t] = &[$($t::$v),+];
                let tag = r.tag($what, (ALL.len() - 1) as u8)?;
                Ok(ALL[tag as usize])
            }
        }
    };
}
impl Wire for Harness {
    fn put(&self, w: &mut Vec<u8>) {
        w.push(match self {
            Harness::Claude => 0,
            Harness::Codex => 1,
            Harness::Grok => 2,
            Harness::Antigravity => 3,
            Harness::PiVendor => 4,
            Harness::PiLocal => 5,
        });
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Harness::Claude,
            1 => Harness::Codex,
            2 => Harness::Grok,
            3 => Harness::Antigravity,
            4 => Harness::PiVendor,
            5 => Harness::PiLocal,
            6 => {
                return Err(DecodeError::UnknownTag {
                    what: "removed pi_durable harness",
                    tag: 6,
                });
            }
            tag => {
                return Err(DecodeError::UnknownTag {
                    what: "harness",
                    tag,
                });
            }
        })
    }
}
unit_enum!(Mode, "mode", [Headless, Pane]);
unit_enum!(Layout, "layout", [Root, Worktree, View]);
unit_enum!(SendMode, "send mode", [Queue, Steer]);
unit_enum!(
    TurnEnd,
    "turn end",
    [Done, Interrupted, UsageLimit, AuthFailed, Error]
);
unit_enum!(Verdict, "verdict", [Idle, Working, Blocked, NoMatch]);
unit_enum!(Choice, "choice", [AllowOnce, AllowSession, Deny]);
unit_enum!(
    ProbeResult,
    "probe result",
    [Applied, NotApplied, Unknowable]
);

impl Wire for Party {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            Party::Owner => w.push(0),
            Party::Envoy => w.push(1),
            Party::Agent => w.push(2),
            Party::Sleeve(s) => {
                w.push(3);
                s.put(w);
            }
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.tag("party", 3)? {
            0 => Party::Owner,
            1 => Party::Envoy,
            2 => Party::Agent,
            _ => Party::Sleeve(SleeveId::get(r)?),
        })
    }
}

impl Wire for ExitCause {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            ExitCause::Exit(c) => {
                w.push(0);
                c.put(w);
            }
            ExitCause::Killed => w.push(1),
            ExitCause::Oom => w.push(2),
            ExitCause::Auth => w.push(3),
            ExitCause::Quota => w.push(4),
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.tag("exit cause", 4)? {
            0 => ExitCause::Exit(i32::get(r)?),
            1 => ExitCause::Killed,
            2 => ExitCause::Oom,
            3 => ExitCause::Auth,
            _ => ExitCause::Quota,
        })
    }
}

impl Wire for Claim {
    fn put(&self, w: &mut Vec<u8>) {
        self.path.put(w);
        self.writable.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Claim {
            path: Wire::get(r)?,
            writable: Wire::get(r)?,
        })
    }
}

impl Wire for Target {
    fn put(&self, w: &mut Vec<u8>) {
        self.harness.put(w);
        self.mode.put(w);
        self.provider.put(w);
        self.model.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Target {
            harness: Wire::get(r)?,
            mode: Wire::get(r)?,
            provider: Wire::get(r)?,
            model: Wire::get(r)?,
        })
    }
}

impl Wire for SpawnSpec {
    fn put(&self, w: &mut Vec<u8>) {
        self.request_id.put(w);
        self.name.put(w);
        self.parent.put(w);
        self.layout.put(w);
        self.claim.put(w);
        self.target.put(w);
        self.resleeve_to.put(w);
        self.budget.put(w);
        self.disk_limit.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(SpawnSpec {
            request_id: Wire::get(r)?,
            name: Wire::get(r)?,
            parent: Wire::get(r)?,
            layout: Wire::get(r)?,
            claim: Wire::get(r)?,
            target: Wire::get(r)?,
            effort: None,
            resleeve_to: Wire::get(r)?,
            budget: Wire::get(r)?,
            disk_limit: Wire::get(r)?,
        })
    }
}

impl Wire for Ask {
    fn put(&self, w: &mut Vec<u8>) {
        self.title.put(w);
        self.summary.put(w);
        self.detail.put(w);
        self.options.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Ask {
            title: Wire::get(r)?,
            summary: Wire::get(r)?,
            detail: Wire::get(r)?,
            options: Wire::get(r)?,
        })
    }
}

impl Wire for JobSpec {
    fn put(&self, w: &mut Vec<u8>) {
        self.digest.put(w);
        self.arch.put(w);
        self.command.put(w);
        self.even_if_present.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(JobSpec {
            digest: Wire::get(r)?,
            arch: Wire::get(r)?,
            command: Wire::get(r)?,
            even_if_present: Wire::get(r)?,
        })
    }
}

impl Wire for JobReport {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            JobReport::Running { offset } => {
                w.push(0);
                offset.put(w);
            }
            JobReport::Exited { code } => {
                w.push(1);
                code.put(w);
            }
            JobReport::Interrupted => w.push(2),
            JobReport::Cancelled => w.push(3),
            JobReport::Refused { invalid } => {
                w.push(4);
                invalid.put(w);
            }
            JobReport::Unknown { epoch } => {
                w.push(5);
                epoch.put(w);
            }
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.tag("job report", 5)? {
            0 => JobReport::Running {
                offset: Wire::get(r)?,
            },
            1 => JobReport::Exited {
                code: Wire::get(r)?,
            },
            2 => JobReport::Interrupted,
            3 => JobReport::Cancelled,
            4 => JobReport::Refused {
                invalid: Wire::get(r)?,
            },
            _ => JobReport::Unknown {
                epoch: Wire::get(r)?,
            },
        })
    }
}

impl Wire for Obs {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            Obs::Ready { session } => {
                w.push(0);
                session.put(w);
            }
            Obs::Progress { tool } => {
                w.push(1);
                tool.put(w);
            }
            Obs::ApprovalRequested { id, ask: None } => {
                w.push(2);
                id.put(w);
            }
            Obs::ApprovalRequested { id, ask: Some(ask) } => {
                w.push(11);
                id.put(w);
                ask.put(w);
            }
            Obs::ApprovalResolved { id } => {
                w.push(3);
                id.put(w);
            }
            Obs::TurnSettled { end } => {
                w.push(4);
                end.put(w);
            }
            Obs::CompactDone => w.push(5),
            Obs::Desync { detail } => {
                w.push(6);
                detail.put(w);
            }
            Obs::Screen(v) => {
                w.push(7);
                v.put(w);
            }
            Obs::Usage { tokens } => {
                w.push(8);
                tokens.put(w);
            }
            Obs::Disk { bytes } => {
                w.push(9);
                bytes.put(w);
            }
            Obs::ForbiddenPath { path } => {
                w.push(10);
                path.put(w);
            }
        }
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(match r.tag("observation", 11)? {
            0 => Obs::Ready {
                session: Wire::get(r)?,
            },
            1 => Obs::Progress {
                tool: Wire::get(r)?,
            },
            2 => Obs::ApprovalRequested {
                id: Wire::get(r)?,
                ask: None,
            },
            3 => Obs::ApprovalResolved { id: Wire::get(r)? },
            4 => Obs::TurnSettled { end: Wire::get(r)? },
            5 => Obs::CompactDone,
            6 => Obs::Desync {
                detail: Wire::get(r)?,
            },
            7 => Obs::Screen(Wire::get(r)?),
            8 => Obs::Usage {
                tokens: Wire::get(r)?,
            },
            9 => Obs::Disk {
                bytes: Wire::get(r)?,
            },
            10 => Obs::ForbiddenPath {
                path: Wire::get(r)?,
            },
            _ => Obs::ApprovalRequested {
                id: Wire::get(r)?,
                ask: Some(Wire::get(r)?),
            },
        })
    }
}

impl Wire for Event {
    fn put(&self, w: &mut Vec<u8>) {
        self.at.put(w);
        self.kind.put(w);
    }
    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Event {
            at: Wire::get(r)?,
            kind: Wire::get(r)?,
        })
    }
}

impl Wire for EventKind {
    fn put(&self, w: &mut Vec<u8>) {
        use EventKind as E;
        match self {
            E::Spawn(spec) => match &spec.effort {
                None => {
                    w.push(0);
                    spec.put(w);
                }
                Some(effort) => {
                    w.push(32);
                    spec.put(w);
                    effort.put(w);
                }
            },
            E::Send {
                to,
                request_id,
                from,
                text,
                mode,
            } => {
                w.push(1);
                to.put(w);
                request_id.put(w);
                from.put(w);
                text.put(w);
                mode.put(w);
            }
            E::Interrupt { sleeve } => {
                w.push(2);
                sleeve.put(w);
            }
            E::Compact { sleeve } => {
                w.push(3);
                sleeve.put(w);
            }
            E::RespondApproval {
                sleeve,
                approval,
                allow,
                note,
            } => {
                w.push(4);
                sleeve.put(w);
                approval.put(w);
                allow.put(w);
                note.put(w);
            }
            E::Stop { sleeve } => {
                w.push(5);
                sleeve.put(w);
            }
            E::Remove { sleeve } => {
                w.push(6);
                sleeve.put(w);
            }
            E::Start { sleeve, target } => {
                w.push(7);
                sleeve.put(w);
                target.put(w);
            }
            E::Resleeve { sleeve, target } => {
                w.push(8);
                sleeve.put(w);
                target.put(w);
            }
            E::Abandon { effect } => {
                w.push(9);
                effect.put(w);
            }
            E::Viewed { sleeve } => {
                w.push(10);
                sleeve.put(w);
            }
            E::CreateJob { request_id, spec } => {
                w.push(11);
                request_id.put(w);
                spec.put(w);
            }
            E::CancelJob { job } => {
                w.push(12);
                job.put(w);
            }
            E::AbandonJob { job } => {
                w.push(13);
                job.put(w);
            }
            E::RequeueJob { job } => {
                w.push(14);
                job.put(w);
            }
            E::Dispatched { effect, attempt } => {
                w.push(15);
                effect.put(w);
                attempt.put(w);
            }
            E::Acked {
                effect,
                attempt,
                native,
            } => {
                w.push(16);
                effect.put(w);
                attempt.put(w);
                native.put(w);
            }
            E::Nacked {
                effect,
                attempt,
                reason,
            } => {
                w.push(17);
                effect.put(w);
                attempt.put(w);
                reason.put(w);
            }
            E::Lost { effect, attempt } => {
                w.push(18);
                effect.put(w);
                attempt.put(w);
            }
            E::Probed {
                effect,
                attempt,
                result,
            } => {
                w.push(19);
                effect.put(w);
                attempt.put(w);
                result.put(w);
            }
            E::Launched {
                sleeve,
                g,
                scope,
                pid,
            } => {
                w.push(20);
                sleeve.put(w);
                g.put(w);
                scope.put(w);
                pid.put(w);
            }
            E::ScopeProbed { sleeve, g, live } => {
                w.push(21);
                sleeve.put(w);
                g.put(w);
                live.put(w);
            }
            E::TargetChosen { sleeve, g, target } => {
                w.push(22);
                sleeve.put(w);
                g.put(w);
                target.put(w);
            }
            E::NodeChosen {
                job,
                attempt,
                node,
                epoch,
            } => {
                w.push(23);
                job.put(w);
                attempt.put(w);
                node.put(w);
                epoch.put(w);
            }
            E::JobStatus {
                job,
                attempt,
                node,
                report,
            } => {
                w.push(24);
                job.put(w);
                attempt.put(w);
                node.put(w);
                report.put(w);
            }
            E::ArtifactsChecked { job, attempt, ok } => {
                w.push(25);
                job.put(w);
                attempt.put(w);
                ok.put(w);
            }
            E::Observed { sleeve, g, obs } => {
                w.push(26);
                sleeve.put(w);
                g.put(w);
                obs.put(w);
            }
            E::Exited { sleeve, g, cause } => {
                w.push(27);
                sleeve.put(w);
                g.put(w);
                cause.put(w);
            }
            E::Tick => w.push(28),
            E::HubRecovered => w.push(29),
            E::JobNodeRemoved { job, node } => {
                w.push(31);
                job.put(w);
                node.put(w);
            }
            E::RequestApproval {
                sleeve,
                ask,
                timeout,
            } => {
                w.push(30);
                sleeve.put(w);
                ask.put(w);
                timeout.put(w);
            }
        }
    }

    fn get(r: &mut Reader) -> Result<Self, DecodeError> {
        use EventKind as E;
        Ok(match r.tag("event", 32)? {
            0 => E::Spawn(Wire::get(r)?),
            1 => E::Send {
                to: Wire::get(r)?,
                request_id: Wire::get(r)?,
                from: Wire::get(r)?,
                text: Wire::get(r)?,
                mode: Wire::get(r)?,
            },
            2 => E::Interrupt {
                sleeve: Wire::get(r)?,
            },
            3 => E::Compact {
                sleeve: Wire::get(r)?,
            },
            4 => E::RespondApproval {
                sleeve: Wire::get(r)?,
                approval: Wire::get(r)?,
                allow: Wire::get(r)?,
                note: Wire::get(r)?,
            },
            5 => E::Stop {
                sleeve: Wire::get(r)?,
            },
            6 => E::Remove {
                sleeve: Wire::get(r)?,
            },
            7 => E::Start {
                sleeve: Wire::get(r)?,
                target: Wire::get(r)?,
            },
            8 => E::Resleeve {
                sleeve: Wire::get(r)?,
                target: Wire::get(r)?,
            },
            9 => E::Abandon {
                effect: Wire::get(r)?,
            },
            10 => E::Viewed {
                sleeve: Wire::get(r)?,
            },
            11 => E::CreateJob {
                request_id: Wire::get(r)?,
                spec: Wire::get(r)?,
            },
            12 => E::CancelJob { job: Wire::get(r)? },
            13 => E::AbandonJob { job: Wire::get(r)? },
            14 => E::RequeueJob { job: Wire::get(r)? },
            15 => E::Dispatched {
                effect: Wire::get(r)?,
                attempt: Wire::get(r)?,
            },
            16 => E::Acked {
                effect: Wire::get(r)?,
                attempt: Wire::get(r)?,
                native: Wire::get(r)?,
            },
            17 => E::Nacked {
                effect: Wire::get(r)?,
                attempt: Wire::get(r)?,
                reason: Wire::get(r)?,
            },
            18 => E::Lost {
                effect: Wire::get(r)?,
                attempt: Wire::get(r)?,
            },
            19 => E::Probed {
                effect: Wire::get(r)?,
                attempt: Wire::get(r)?,
                result: Wire::get(r)?,
            },
            20 => E::Launched {
                sleeve: Wire::get(r)?,
                g: Wire::get(r)?,
                scope: Wire::get(r)?,
                pid: Wire::get(r)?,
            },
            21 => E::ScopeProbed {
                sleeve: Wire::get(r)?,
                g: Wire::get(r)?,
                live: Wire::get(r)?,
            },
            22 => E::TargetChosen {
                sleeve: Wire::get(r)?,
                g: Wire::get(r)?,
                target: Wire::get(r)?,
            },
            23 => E::NodeChosen {
                job: Wire::get(r)?,
                attempt: Wire::get(r)?,
                node: Wire::get(r)?,
                epoch: Wire::get(r)?,
            },
            24 => E::JobStatus {
                job: Wire::get(r)?,
                attempt: Wire::get(r)?,
                node: Wire::get(r)?,
                report: Wire::get(r)?,
            },
            25 => E::ArtifactsChecked {
                job: Wire::get(r)?,
                attempt: Wire::get(r)?,
                ok: Wire::get(r)?,
            },
            26 => E::Observed {
                sleeve: Wire::get(r)?,
                g: Wire::get(r)?,
                obs: Wire::get(r)?,
            },
            27 => E::Exited {
                sleeve: Wire::get(r)?,
                g: Wire::get(r)?,
                cause: Wire::get(r)?,
            },
            28 => E::Tick,
            29 => E::HubRecovered,
            30 => E::RequestApproval {
                sleeve: Wire::get(r)?,
                ask: Wire::get(r)?,
                timeout: Wire::get(r)?,
            },
            31 => E::JobNodeRemoved {
                job: Wire::get(r)?,
                node: Wire::get(r)?,
            },
            _ => {
                let mut spec: SpawnSpec = Wire::get(r)?;
                spec.effort = Some(Wire::get(r)?);
                E::Spawn(spec)
            }
        })
    }
}
