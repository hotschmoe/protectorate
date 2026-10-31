//! Identifiers and small value types shared by events, intents and state.

/// Milliseconds since the Unix epoch, as carried by events. The core never reads a clock.
pub type Millis = u64;

pub const SECOND: Millis = 1_000;
pub const MINUTE: Millis = 60 * SECOND;
pub const HOUR: Millis = 60 * MINUTE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SleeveId(pub u64);

/// A sleeve's incarnation generation: 1 for the first incarnation, +1 for each later one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Gen(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// A workspace claim: an absolute, normalized directory path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub path: String,
    pub writable: bool,
}

/// True for an absolute path with no empty, `.` or `..` component and no trailing slash
/// (except `/` itself). The hub canonicalizes paths; the core only refuses what it cannot compare.
pub fn path_ok(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    let Some(rest) = path.strip_prefix('/') else {
        return false;
    };
    rest.split('/')
        .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// True when `inner` equals `outer` or lies inside it, compared by whole components.
pub fn path_within(inner: &str, outer: &str) -> bool {
    if outer == "/" || inner == outer {
        return true;
    }
    inner.len() > outer.len() && inner.starts_with(outer) && inner.as_bytes()[outer.len()] == b'/'
}

/// True when one path equals, contains or lies inside the other.
pub fn paths_overlap(a: &str, b: &str) -> bool {
    path_within(a, b) || path_within(b, a)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Harness {
    Claude,
    Codex,
    Grok,
    Antigravity,
    PiVendor,
    PiLocal,
    PiDurable,
}

impl Harness {
    /// The headless protocol can add input to a running turn (Codex `turn/steer`, Pi `steer`).
    pub fn steers(self) -> bool {
        matches!(
            self,
            Harness::Codex | Harness::PiVendor | Harness::PiLocal | Harness::PiDurable
        )
    }

    /// Antigravity is pane-only (D13).
    pub fn has_headless(self) -> bool {
        self != Harness::Antigravity
    }

    /// A prompt to pi_durable is keyed by its request id, so it may be probed and retried.
    pub fn keyed_prompts(self) -> bool {
        self == Harness::PiDurable
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Headless,
    Pane,
}

/// What an incarnation runs: harness, mode, provider and model.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    pub harness: Harness,
    pub mode: Mode,
    pub provider: String,
    pub model: String,
}

impl Target {
    pub fn valid(&self) -> bool {
        self.mode == Mode::Pane || self.harness.has_headless()
    }
}

/// How a sleeve's workspace is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A root on its own directory (writable claim).
    Root,
    /// A child on its own git worktree and branch (writable claim).
    Worktree,
    /// A child with a read-only view of its root's directory.
    View,
}

/// Who sent a message, or who a notice is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Party {
    Owner,
    Envoy,
    Agent,
    Sleeve(SleeveId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendMode {
    Queue,
    Steer,
}

/// How an incarnation's scope emptied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitCause {
    Exit(i32),
    Killed,
    Oom,
}

/// How a headless turn ended, as the driver classified it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnEnd {
    Done,
    Interrupted,
    UsageLimit,
    AuthFailed,
    Error,
}

/// A pane's debounced screen verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Idle,
    Working,
    Blocked,
    NoMatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeResult {
    Applied,
    NotApplied,
    Unknowable,
}

/// The guardrails G1 to G14.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Guardrail {
    QuotaExhausted,
    AuthFailed,
    Crash,
    Oom,
    StartFailed,
    Silent,
    Loop,
    TurnWallclock,
    ApprovalTimeout,
    Budget,
    ForbiddenPath,
    DiskLimit,
    InterruptUnresponsive,
    ProtocolDesync,
}

impl Guardrail {
    pub const ALL: [Guardrail; 14] = [
        Guardrail::QuotaExhausted,
        Guardrail::AuthFailed,
        Guardrail::Crash,
        Guardrail::Oom,
        Guardrail::StartFailed,
        Guardrail::Silent,
        Guardrail::Loop,
        Guardrail::TurnWallclock,
        Guardrail::ApprovalTimeout,
        Guardrail::Budget,
        Guardrail::ForbiddenPath,
        Guardrail::DiskLimit,
        Guardrail::InterruptUnresponsive,
        Guardrail::ProtocolDesync,
    ];

    /// The G number, 1 to 14.
    pub fn number(self) -> u8 {
        Guardrail::ALL
            .iter()
            .position(|&g| g == self)
            .expect("listed") as u8
            + 1
    }
}

/// Why a resume or resleeve happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Guard(Guardrail),
    HubRestart,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkReason {
    Provision,
    NoTarget,
    Launch,
    Flapping,
    BriefUnconfirmed,
    Guard(Guardrail),
}
