const std = @import("std");
const tiger = @import("../tiger.zig");
const model = @import("model.zig");

pub fn TimeWindow(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        items: [capacity]i64 = @splat(0),
        len: usize = 0,
        head: usize = 0,

        pub fn push(self: *Self, ts: i64) void {
            if (self.len < capacity) {
                const idx = (self.head + self.len) % capacity;
                self.items[idx] = ts;
                self.len += 1;
            } else {
                self.items[self.head] = ts;
                self.head = (self.head + 1) % capacity;
            }
        }

        pub fn count_in_window(self: *const Self, now_ms: i64, window_ms: i64) u32 {
            var count: u32 = 0;
            for (0..self.len) |i| {
                const ts = self.items[(self.head + i) % capacity];
                if (now_ms >= ts and now_ms - ts <= window_ms) count += 1;
            }
            return count;
        }
    };
}

pub const PendingLaunch = struct {
    harness: model.Harness,
    model_name: []const u8,
    provider: []const u8,
    effort: ?[]const u8,
    cause: model.Cause,
};

pub const IncarnationState = struct {
    g: model.Gen,
    harness: model.Harness,
    mode: model.Mode,
    model_name: []const u8,
    provider: []const u8,
    effort: ?[]const u8 = null,
    native_id: ?[]const u8 = null,
    activity: model.Activity = .booting,
    started_at_ms: i64,
    last_event_ms: i64,
    turn_started_ms: ?i64 = null,
    stop_requested_ms: ?i64 = null,
    turn: u64 = 0,
    context_tokens: u64 = 0,
    context_window: u64 = 0,
    last_tool_sig: ?[]const u8 = null,
    tool_repeat_count: u32 = 0,
    delivered_request_id: ?[]const u8 = null,
    delivered_kind: ?model.MsgKind = null,
    turn_is_agent_mail: bool = false,
    next_launch: ?PendingLaunch = null,
};

pub const MailItem = struct {
    request_id: []const u8,
    from: []const u8,
    kind: model.MsgKind,
    mode: model.SendMode,
    text: []const u8,
    queued_at_ms: i64,
    until_done: ?u64 = null,
};

pub const SleeveState = struct {
    id: []const u8,
    name: []const u8,
    role: model.SleeveRole,
    path: []const u8,
    workspace_id: []const u8,
    parent_id: ?[]const u8 = null,
    layout: model.Layout,
    branch: ?[]const u8 = null,
    lifecycle: model.Lifecycle,
    park_reason: ?model.ParkReason = null,
    d9: bool = false,
    next_g: model.Gen = 0,
    inc: ?IncarnationState = null,
    queued: std.ArrayList(MailItem) = .empty,
    targets: [model.max_targets]model.Harness = undefined,
    target_len: u8 = 0,
    preferred_mode: model.Mode = .headless,
    preferred_model: ?[]const u8 = null,
    preferred_provider: ?[]const u8 = null,
    preferred_effort: ?[]const u8 = null,
    last_status: ?model.DeclaredStatus = null,
    budget_used: u64 = 0,
    budget_limit: ?u64 = null,
    until_done_left: u64 = 0,
    owed_continuations: u32 = 0,
    children_active: u32 = 0,
    resleeve_win: TimeWindow(16) = .{},
    resume_win: TimeWindow(16) = .{},
    strike_win: TimeWindow(16) = .{},
    last_oom_ms: ?i64 = null,
    oom_notice_pending: bool = false,
};

pub const ApprovalRecord = struct {
    id: []const u8,
    sleeve_id: []const u8,
    g: model.Gen,
    kind: model.ApprovalKind,
    title: []const u8,
    summary: []const u8,
    is_d9: bool,
    requested_at_ms: i64,
    timeout_at_ms: i64,
    state: model.ApprovalState,
    verdict: ?model.Verdict = null,
    decided_by: ?model.Party = null,
    decided_at_ms: ?i64 = null,
};

pub const NodeRecord = struct {
    id: []const u8,
    name: []const u8,
    pub_key: [32]u8,
    approved: bool = false,
    online: bool = false,
    arch: []const u8 = "",
    version: []const u8 = "",
    last_seen_ms: i64 = 0,
    jobs_policy: model.NodeJobs = .any,
    presence: model.ProbeResult = .unknown,
    excluded_until_ms: i64 = 0,
    removed: bool = false,
};

pub const JobRecord = struct {
    id: []const u8,
    target_node: ?[]const u8 = null,
    target_arch: ?[]const u8 = null,
    assigned_node: ?[]const u8 = null,
    requested_by: []const u8,
    on_present: model.OnPresent = .idle,
    even_if_present: bool = false,
    maintenance: bool = false,
    state: model.JobState,
    attempt: u32 = 0,
    verify_probes: u32 = 0,
    queued_at_ms: i64,
    state_at_ms: i64,
    lost_at_ms: ?i64 = null,
    exit_code: ?i32 = null,
    err: ?[]const u8 = null,
};

pub const RolloutNodeState = struct {
    node_id: []const u8,
    done: bool = false,
    ok: bool = false,
    detail: []const u8 = "",
};

pub const RolloutRecord = struct {
    id: []const u8,
    version: []const u8,
    cancelled: bool = false,
    nodes: std.ArrayList(RolloutNodeState) = .empty,
};

pub const State = struct {
    arena_state: std.heap.ArenaAllocator,
    event_seq: u64 = 0,
    sleeves: std.StringHashMapUnmanaged(SleeveState) = .empty,
    approvals: std.StringHashMapUnmanaged(ApprovalRecord) = .empty,
    nodes: std.StringHashMapUnmanaged(NodeRecord) = .empty,
    jobs: std.StringHashMapUnmanaged(JobRecord) = .empty,
    rollouts: std.StringHashMapUnmanaged(RolloutRecord) = .empty,
    envoy_mail_win: TimeWindow(32) = .{},

    pub fn init(gpa: std.mem.Allocator) State {
        return .{
            .arena_state = std.heap.ArenaAllocator.init(gpa),
        };
    }

    pub fn deinit(self: *State) void {
        self.arena_state.deinit();
    }

    pub fn alloc(self: *State) std.mem.Allocator {
        return self.arena_state.allocator();
    }

    pub fn dupe_str(self: *State, s: []const u8) ![]const u8 {
        return self.arena_state.allocator().dupe(u8, s);
    }

    pub fn dupe_opt(self: *State, s: ?[]const u8) !?[]const u8 {
        if (s) |v| return try self.dupe_str(v);
        return null;
    }

    pub fn active_claim_conflict(self: *const State, sleeve_id: []const u8, path: []const u8, layout: model.Layout) ?[]const u8 {
        if (layout == .readonly or layout == .shared) return null;
        var it = self.sleeves.iterator();
        while (it.next()) |e| {
            const other = e.value_ptr.*;
            if (std.mem.eql(u8, other.id, sleeve_id)) continue;
            if (other.lifecycle == .removed or other.lifecycle == .stopped or other.lifecycle == .parked) continue;
            if (other.layout == .readonly or other.layout == .shared) continue;
            if (model.paths_overlap(path, other.path)) return other.id;
        }
        return null;
    }

    pub fn state_hash(self: *const State, gpa: std.mem.Allocator) !u64 {
        var h = std.hash.Wyhash.init(0x50545f5354415445);
        h.update(std.mem.asBytes(&self.event_seq));

        // Sort sleeve keys for order-independent canonical hashing.
        var skeys: std.ArrayList([]const u8) = .empty;
        defer skeys.deinit(gpa);
        var sit = self.sleeves.keyIterator();
        while (sit.next()) |kp| try skeys.append(gpa, kp.*);
        sort_strs(skeys.items);
        for (skeys.items) |k| {
            const sl = self.sleeves.get(k).?;
            hash_str(&h, sl.id);
            hash_u8(&h, @intFromEnum(sl.lifecycle));
            hash_u64(&h, sl.next_g);
            hash_u8(&h, if (sl.d9) 1 else 0);
            hash_u64(&h, sl.queued.items.len);
            hash_u64(&h, sl.budget_used);
            hash_u64(&h, sl.until_done_left);
            if (sl.inc) |inc| {
                hash_u8(&h, 1);
                hash_u64(&h, inc.g);
                hash_u8(&h, @intFromEnum(inc.harness));
                hash_u8(&h, @intFromEnum(inc.activity));
                hash_u64(&h, inc.turn);
                hash_u64(&h, inc.context_tokens);
            } else {
                hash_u8(&h, 0);
            }
        }

        var akeys: std.ArrayList([]const u8) = .empty;
        defer akeys.deinit(gpa);
        var ait = self.approvals.keyIterator();
        while (ait.next()) |kp| try akeys.append(gpa, kp.*);
        sort_strs(akeys.items);
        for (akeys.items) |k| {
            const a = self.approvals.get(k).?;
            hash_str(&h, a.id);
            hash_u8(&h, @intFromEnum(a.state));
            hash_u64(&h, a.g);
        }

        var nkeys: std.ArrayList([]const u8) = .empty;
        defer nkeys.deinit(gpa);
        var nit = self.nodes.keyIterator();
        while (nit.next()) |kp| try nkeys.append(gpa, kp.*);
        sort_strs(nkeys.items);
        for (nkeys.items) |k| {
            const n = self.nodes.get(k).?;
            hash_str(&h, n.id);
            hash_u8(&h, if (n.approved) 1 else 0);
            hash_u8(&h, if (n.online) 1 else 0);
            hash_u8(&h, if (n.removed) 1 else 0);
            hash_u8(&h, @intFromEnum(n.presence));
        }

        var jkeys: std.ArrayList([]const u8) = .empty;
        defer jkeys.deinit(gpa);
        var jit = self.jobs.keyIterator();
        while (jit.next()) |kp| try jkeys.append(gpa, kp.*);
        sort_strs(jkeys.items);
        for (jkeys.items) |k| {
            const j = self.jobs.get(k).?;
            hash_str(&h, j.id);
            hash_u8(&h, @intFromEnum(j.state));
            hash_u64(&h, j.attempt);
            hash_u64(&h, j.verify_probes);
        }

        return h.final();
    }
};

fn sort_strs(items: [][]const u8) void {
    std.mem.sort([]const u8, items, {}, struct {
        fn less(_: void, a: []const u8, b: []const u8) bool {
            return std.mem.lessThan(u8, a, b);
        }
    }.less);
}

fn hash_str(h: *std.hash.Wyhash, s: []const u8) void {
    hash_u64(h, s.len);
    h.update(s);
}

fn hash_u8(h: *std.hash.Wyhash, v: u8) void {
    h.update(&.{v});
}

fn hash_u64(h: *std.hash.Wyhash, v: u64) void {
    var b: [8]u8 = undefined;
    std.mem.writeInt(u64, &b, v, .little);
    h.update(&b);
}
