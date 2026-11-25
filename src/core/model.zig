const std = @import("std");
const tiger = @import("../tiger.zig");

pub const SleeveId = []const u8;
pub const Gen = u64;
pub const EffectId = []const u8;
pub const JobId = []const u8;
pub const NodeId = []const u8;
pub const Millis = i64;

pub const start_timeout_ms: Millis = 60_000;
pub const silent_ms: Millis = 15 * 60_000;
pub const loop_repeats: u32 = 6;
pub const turn_wallclock_ms: Millis = 90 * 60_000;
pub const approval_timeout_ms: Millis = 30 * 60_000;
pub const interrupt_timeout_ms: Millis = 30_000;
pub const stop_grace_ms: Millis = 30_000;

pub const resume_limit: u32 = 2;
pub const resume_window_ms: Millis = 10 * 60_000;
pub const resleeve_limit: u32 = 2;
pub const resleeve_window_ms: Millis = 60 * 60_000;
pub const strike_limit: u32 = 2;
pub const strike_window_ms: Millis = 60 * 60_000;
pub const oom_window_ms: Millis = 60 * 60_000;

pub const quiesce_wait_ms: Millis = 5 * 60_000;
pub const quiesce_grace_ms: Millis = 30_000;
pub const cooldown_ms: Millis = 10 * 60_000;

pub const max_targets: usize = 3;
pub const max_children: usize = 8;
pub const max_sleeves: usize = 256;
pub const max_nodes: usize = 256;
pub const max_jobs: usize = 2048;
pub const max_approvals: usize = 512;

pub const envoy_mail_limit: u32 = 8;
pub const envoy_mail_window_ms: Millis = 10 * 60_000;
pub const envoy_context_ceiling: u64 = 500_000;

pub const job_lease_ms: Millis = 5 * 60_000;
pub const lost_abandon_ms: Millis = 24 * 60 * 60_000;
pub const exclude_for_ms: Millis = 10 * 60_000;
pub const max_verify: u32 = 3;
pub const node_resends: u32 = 30;

pub const Harness = enum {
    claude,
    codex,
    grok,
    pi_vendor,
    pi_local,
    antigravity,

    pub fn to_wire(self: Harness) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?Harness {
        return std.meta.stringToEnum(Harness, s);
    }
};

pub const Mode = enum {
    headless,
    pane,

    pub fn to_wire(self: Mode) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?Mode {
        return std.meta.stringToEnum(Mode, s);
    }
};

pub const Target = union(enum) {
    harness: Harness,
    auto: struct {
        chain: [max_targets]Harness,
        len: usize,
    },

    pub fn from_single(h: Harness) Target {
        return .{ .harness = h };
    }

    pub fn from_chain(items: []const Harness) Target {
        tiger.assert(items.len > 0 and items.len <= max_targets);
        var buf: [max_targets]Harness = undefined;
        for (items, 0..) |h, i| buf[i] = h;
        return .{ .auto = .{ .chain = buf, .len = items.len } };
    }
};

pub const Layout = enum {
    worktree,
    readonly,
    shared,

    pub fn to_wire(self: Layout) []const u8 {
        return @tagName(self);
    }
};

pub const SleeveRole = enum {
    envoy,
    root,
    child,

    pub fn to_wire(self: SleeveRole) []const u8 {
        return @tagName(self);
    }
};

pub const Lifecycle = enum {
    provisioning,
    launching,
    running,
    recovering,
    resleeving,
    stopping,
    stopped,
    parked,
    removed,
};

pub const Activity = enum {
    booting,
    idle,
    delivering,
    working,
    blocked,
    interrupting,
    compacting,
    ended,
    unknown,
};

pub const DeclaredStatus = enum {
    done,
    continuing,
    blocked,

    pub fn to_wire(self: DeclaredStatus) []const u8 {
        return @tagName(self);
    }

    pub fn from_wire(s: []const u8) ?DeclaredStatus {
        return std.meta.stringToEnum(DeclaredStatus, s);
    }
};

pub const Claim = enum {
    none,
    sent,
    ended,
};

pub const Party = enum {
    owner,
    envoy,
    rule,
    system,
    sleeve,
};

pub const MsgKind = enum {
    prompt,
    mail,
    report,
    system,
    agent_mail,
};

pub const SendMode = enum {
    queue,
    steer,
};

pub const ExitCause = enum {
    clean,
    crash,
    oom,
    killed,
    quota_exhausted,
    auth_error,
};

pub const TurnEnd = enum {
    end_turn,
    cancelled,
    @"error",
    max_turns,
    other,
};

pub const Verdict = enum {
    allow_once,
    allow_session,
    deny,
};

pub const Choice = enum {
    resume_in_place,
    resleeve,
    park,
};

pub const ProbeResult = enum {
    present,
    away,
    unknown,
};

pub const Guardrail = enum {
    g1_max_targets,
    g2_resleeve_rate,
    g3_resume_rate,
    g4_strike_rate,
    g5_silence_timeout,
    g6_loop_detection,
    g7_wallclock_timeout,
    g8_approval_timeout,
    g9_unrequested_interrupt,
    g10_max_children,
    g11_child_budget,
    g12_envoy_mail_rate,
    g13_envoy_context_ceiling,
    g14_presence_gate,
};

pub const Cause = enum {
    manual,
    crash,
    oom,
    quota,
    silent,
    loop,
    wallclock,
    approval_timeout,
    handoff,
    presence,
};

pub const ParkReason = enum {
    resleeve_rate_exceeded,
    resume_rate_exceeded,
    strike_rate_exceeded,
    no_fallback_target,
    budget_exhausted,
    provision_failed,
    quiesced_without_target,
    removed,
};

pub const ApprovalKind = enum {
    tool,
    action,
};

pub const ApprovalState = enum {
    pending,
    approved,
    denied,
    expired,
    cancelled,
    fenced,
    abandoned,
};

pub const JobState = enum {
    queued,
    submitting,
    submit_uncertain,
    running,
    collecting,
    succeeded,
    failed,
    interrupted,
    lost,
    cancelling,
    cancelled,
    abandoned,

    pub fn is_terminal(self: JobState) bool {
        return switch (self) {
            .succeeded, .failed, .cancelled, .abandoned => true,
            else => false,
        };
    }

    pub fn is_in_flight(self: JobState) bool {
        return switch (self) {
            .submitting, .submit_uncertain, .running, .collecting, .cancelling => true,
            else => false,
        };
    }
};

pub const NodeJobs = enum {
    any,
    owner,
    none,
};

pub const OnPresent = enum {
    idle,
    kill,
};

pub fn path_ok(p: []const u8) bool {
    if (p.len == 0) return false;
    if (p[0] != '/') return false;
    if (p.len > 1 and p[p.len - 1] == '/') return false;
    var it = std.mem.splitScalar(u8, p[1..], '/');
    while (it.next()) |seg| {
        if (seg.len == 0) return false;
        if (std.mem.eql(u8, seg, ".") or std.mem.eql(u8, seg, "..")) return false;
    }
    return true;
}

pub fn path_within(child: []const u8, parent: []const u8) bool {
    if (!path_ok(child) or !path_ok(parent)) return false;
    if (std.mem.eql(u8, child, parent)) return true;
    if (std.mem.eql(u8, parent, "/")) return true;
    if (child.len <= parent.len) return false;
    return std.mem.startsWith(u8, child, parent) and child[parent.len] == '/';
}

pub fn paths_overlap(a: []const u8, b: []const u8) bool {
    return path_within(a, b) or path_within(b, a);
}
