const std = @import("std");
const model = @import("model.zig");
const event = @import("event.zig");

pub const format_version: u32 = 1;
pub const max_str_len: usize = 4 << 20;
pub const max_list_len: usize = 4096;

pub const CodecError = error{
    CorruptEvent,
    UnsupportedFormat,
    OutOfMemory,
};

fn write_u8(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: u8) !void {
    try out.append(gpa, v);
}

fn write_bool(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: bool) !void {
    try out.append(gpa, if (v) 1 else 0);
}

fn write_u32(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: u32) !void {
    var b: [4]u8 = undefined;
    std.mem.writeInt(u32, &b, v, .little);
    try out.appendSlice(gpa, &b);
}

fn write_i32(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: i32) !void {
    var b: [4]u8 = undefined;
    std.mem.writeInt(i32, &b, v, .little);
    try out.appendSlice(gpa, &b);
}

fn write_u64(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: u64) !void {
    var b: [8]u8 = undefined;
    std.mem.writeInt(u64, &b, v, .little);
    try out.appendSlice(gpa, &b);
}

fn write_i64(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: i64) !void {
    var b: [8]u8 = undefined;
    std.mem.writeInt(i64, &b, v, .little);
    try out.appendSlice(gpa, &b);
}

fn write_str(gpa: std.mem.Allocator, out: *std.ArrayList(u8), s: []const u8) !void {
    try write_u32(gpa, out, @intCast(s.len));
    try out.appendSlice(gpa, s);
}

fn write_opt_str(gpa: std.mem.Allocator, out: *std.ArrayList(u8), s: ?[]const u8) !void {
    if (s) |v| {
        try write_u8(gpa, out, 1);
        try write_str(gpa, out, v);
    } else {
        try write_u8(gpa, out, 0);
    }
}

fn write_opt_u64(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: ?u64) !void {
    if (v) |n| {
        try write_u8(gpa, out, 1);
        try write_u64(gpa, out, n);
    } else {
        try write_u8(gpa, out, 0);
    }
}

fn write_opt_i32(gpa: std.mem.Allocator, out: *std.ArrayList(u8), v: ?i32) !void {
    if (v) |n| {
        try write_u8(gpa, out, 1);
        try write_i32(gpa, out, n);
    } else {
        try write_u8(gpa, out, 0);
    }
}

fn write_quota_selection(gpa: std.mem.Allocator, out: *std.ArrayList(u8), selection: event.QuotaSelection) !void {
    try write_bool(gpa, out, selection.harness != null);
    if (selection.harness) |harness| {
        try write_u8(gpa, out, @backingInt(harness));
        try write_opt_str(gpa, out, selection.model_name);
        try write_opt_str(gpa, out, selection.provider);
    }
    try write_str(gpa, out, selection.reason);
}

pub fn encode_stamped(gpa: std.mem.Allocator, se: event.StampedEvent, out: *std.ArrayList(u8)) CodecError!void {
    try write_u32(gpa, out, format_version);
    try write_u64(gpa, out, se.seq);
    try write_i64(gpa, out, se.ts_ms);
    const tag: u8 = @intFromEnum(std.meta.activeTag(se.event));
    try write_u8(gpa, out, tag);

    switch (se.event) {
        .sleeve_created => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_str(gpa, out, e.name);
            try write_u8(gpa, out, @intFromEnum(e.role));
            try write_str(gpa, out, e.path);
            try write_str(gpa, out, e.workspace_id);
            try write_opt_str(gpa, out, e.parent_id);
            try write_u8(gpa, out, @intFromEnum(e.layout));
            try write_opt_str(gpa, out, e.branch);
            try write_opt_u64(gpa, out, e.budget_limit);
            try write_u64(gpa, out, e.until_done);
        },
        .provisioned => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_opt_str(gpa, out, e.branch);
        },
        .provision_failed => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_str(gpa, out, e.reason);
        },
        .start_requested => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u8(gpa, out, e.target_len);
            for (0..e.target_len) |i| try write_u8(gpa, out, @intFromEnum(e.targets[i]));
            try write_u8(gpa, out, @intFromEnum(e.mode));
            try write_opt_str(gpa, out, e.model_name);
            try write_opt_str(gpa, out, e.provider);
            try write_opt_str(gpa, out, e.effort);
            try write_opt_u64(gpa, out, e.until_done);
            if (e.selection_reason) |reason| try write_str(gpa, out, reason);
        },
        .launched => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @intFromEnum(e.harness));
            try write_u8(gpa, out, @intFromEnum(e.mode));
            try write_str(gpa, out, e.model_name);
            try write_str(gpa, out, e.provider);
            try write_opt_str(gpa, out, e.effort);
        },
        .session_bound => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_str(gpa, out, e.native_id);
        },
        .prompt_enqueued => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_str(gpa, out, e.request_id);
            try write_str(gpa, out, e.from);
            try write_u8(gpa, out, @intFromEnum(e.kind));
            try write_u8(gpa, out, @intFromEnum(e.mode));
            try write_str(gpa, out, e.text);
            try write_bool(gpa, out, e.supersede);
            try write_opt_u64(gpa, out, e.until_done);
        },
        .prompt_delivered => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_str(gpa, out, e.request_id);
        },
        .prompt_acked => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_str(gpa, out, e.request_id);
            try write_bool(gpa, out, e.confirmed);
            try write_opt_str(gpa, out, e.detail);
        },
        .turn_started => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_opt_str(gpa, out, e.request_id);
            try write_bool(gpa, out, e.from_agent_mail);
        },
        .turn_settled => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @intFromEnum(e.turn_end));
            if (e.declared_status) |ds| {
                try write_u8(gpa, out, 1);
                try write_u8(gpa, out, @intFromEnum(ds));
            } else {
                try write_u8(gpa, out, 0);
            }
            try write_u64(gpa, out, e.context_tokens);
            try write_u64(gpa, out, e.context_window);
            if (e.quota_handoff) |handoff| {
                try write_bool(gpa, out, true);
                try write_u8(gpa, out, @backingInt(handoff.cause));
                if (handoff.selection) |selection| try write_quota_selection(gpa, out, selection);
            }
        },
        .tool_activity => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_str(gpa, out, e.tool_sig);
        },
        .compacted => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u64(gpa, out, e.tokens_after);
        },
        .interrupted => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @intFromEnum(e.by));
        },
        .exited => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @intFromEnum(e.cause));
            try write_bool(gpa, out, e.requested);
            if (e.quota_selection) |selection| try write_quota_selection(gpa, out, selection);
        },
        .stop_requested => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u8(gpa, out, @intFromEnum(e.by));
        },
        .stopped => |e| try write_str(gpa, out, e.sleeve_id),
        .resleeve_requested => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u8(gpa, out, @intFromEnum(e.harness));
            try write_opt_str(gpa, out, e.model_name);
            try write_opt_str(gpa, out, e.provider);
            try write_opt_str(gpa, out, e.effort);
            try write_u8(gpa, out, @intFromEnum(e.by));
            try write_u8(gpa, out, @intFromEnum(e.cause));
            if (!e.inherit_preferences) try write_bool(gpa, out, false);
        },
        .parked => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u8(gpa, out, @intFromEnum(e.reason));
        },
        .removed => |e| try write_str(gpa, out, e.sleeve_id),
        .drop_queued => |e| try write_str(gpa, out, e.sleeve_id),
        .approval_requested => |e| {
            try write_str(gpa, out, e.approval_id);
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @intFromEnum(e.kind));
            try write_str(gpa, out, e.title);
            try write_str(gpa, out, e.summary);
            try write_bool(gpa, out, e.is_d9);
            if (e.timeout_ms != model.approval_timeout_ms) try write_i64(gpa, out, e.timeout_ms);
        },
        .approval_decided => |e| {
            try write_str(gpa, out, e.approval_id);
            try write_u8(gpa, out, @intFromEnum(e.verdict));
            try write_u8(gpa, out, @intFromEnum(e.by));
            try write_str(gpa, out, e.note);
        },
        .approval_expired => |e| try write_str(gpa, out, e.approval_id),
        .node_registered => |e| {
            try write_str(gpa, out, e.node_id);
            try write_str(gpa, out, e.name);
            try out.appendSlice(gpa, &e.pub_key);
        },
        .node_approved => |e| {
            try write_str(gpa, out, e.node_id);
            try write_str(gpa, out, e.name);
        },
        .node_connected => |e| {
            try write_str(gpa, out, e.node_id);
            try write_str(gpa, out, e.arch);
            try write_str(gpa, out, e.version);
        },
        .node_disconnected => |e| {
            try write_str(gpa, out, e.node_id);
            try write_str(gpa, out, e.reason);
        },
        .node_removed => |e| try write_str(gpa, out, e.node_id),
        .node_renamed => |e| {
            try write_str(gpa, out, e.node_id);
            try write_str(gpa, out, e.name);
        },
        .node_jobs_updated => |e| {
            try write_str(gpa, out, e.node_id);
            try write_u8(gpa, out, @intFromEnum(e.jobs));
            try write_opt_str(gpa, out, e.note);
        },
        .node_presence_updated => |e| {
            try write_str(gpa, out, e.node_id);
            try write_u8(gpa, out, @intFromEnum(e.presence));
        },
        .job_queued => |e| {
            try write_str(gpa, out, e.job_id);
            try write_opt_str(gpa, out, e.target_node);
            try write_opt_str(gpa, out, e.target_arch);
            try write_str(gpa, out, e.requested_by);
            try write_u8(gpa, out, @intFromEnum(e.on_present));
            try write_bool(gpa, out, e.even_if_present);
            try write_bool(gpa, out, e.maintenance);
        },
        .job_submitted => |e| {
            try write_str(gpa, out, e.job_id);
            try write_str(gpa, out, e.node_id);
            try write_u32(gpa, out, e.attempt);
        },
        .job_submit_uncertain => |e| {
            try write_str(gpa, out, e.job_id);
            try write_str(gpa, out, e.node_id);
            try write_u32(gpa, out, e.attempt);
        },
        .job_started => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u32(gpa, out, e.attempt);
        },
        .job_completed => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u32(gpa, out, e.attempt);
            try write_i32(gpa, out, e.exit_code);
        },
        .job_failed => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u32(gpa, out, e.attempt);
            try write_str(gpa, out, e.err);
        },
        .job_interrupted => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u32(gpa, out, e.attempt);
        },
        .job_lost => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u32(gpa, out, e.attempt);
        },
        .job_cancel_requested => |e| {
            try write_str(gpa, out, e.job_id);
            try write_u8(gpa, out, @intFromEnum(e.by));
        },
        .job_cancelled => |e| try write_str(gpa, out, e.job_id),
        .job_abandoned => |e| {
            try write_str(gpa, out, e.job_id);
            try write_str(gpa, out, e.reason);
        },
        .job_verified => |e| {
            try write_str(gpa, out, e.job_id);
            try write_bool(gpa, out, e.still_running);
            try write_opt_i32(gpa, out, e.exit_code);
        },
        .rollout_started => |e| {
            try write_str(gpa, out, e.rollout_id);
            try write_str(gpa, out, e.version);
            try write_u32(gpa, out, @intCast(e.node_ids.len));
            for (e.node_ids) |nid| try write_str(gpa, out, nid);
        },
        .rollout_node_result => |e| {
            try write_str(gpa, out, e.rollout_id);
            try write_str(gpa, out, e.node_id);
            try write_bool(gpa, out, e.ok);
            try write_str(gpa, out, e.detail);
        },
        .rollout_cancelled => |e| try write_str(gpa, out, e.rollout_id),
        .tick => {},
        .exit_classified => |e| {
            try write_str(gpa, out, e.sleeve_id);
            try write_u64(gpa, out, e.g);
            try write_u8(gpa, out, @backingInt(e.cause));
        },
    }
}

const Reader = struct {
    b: []const u8,
    i: usize = 0,

    fn read_bytes(self: *Reader, n: usize) CodecError![]const u8 {
        if (n > self.b.len - self.i) return error.CorruptEvent;
        const s = self.b[self.i .. self.i + n];
        self.i += n;
        return s;
    }

    fn read_u8(self: *Reader) CodecError!u8 {
        return (try self.read_bytes(1))[0];
    }

    fn read_bool(self: *Reader) CodecError!bool {
        return switch (try self.read_u8()) {
            0 => false,
            1 => true,
            else => error.CorruptEvent,
        };
    }

    fn read_u32(self: *Reader) CodecError!u32 {
        const s = try self.read_bytes(4);
        return std.mem.readInt(u32, s[0..4], .little);
    }

    fn read_i32(self: *Reader) CodecError!i32 {
        const s = try self.read_bytes(4);
        return std.mem.readInt(i32, s[0..4], .little);
    }

    fn read_u64(self: *Reader) CodecError!u64 {
        const s = try self.read_bytes(8);
        return std.mem.readInt(u64, s[0..8], .little);
    }

    fn read_i64(self: *Reader) CodecError!i64 {
        const s = try self.read_bytes(8);
        return std.mem.readInt(i64, s[0..8], .little);
    }

    fn read_enum(self: *Reader, comptime E: type) CodecError!E {
        const b = try self.read_u8();
        return std.enums.fromInt(E, b) orelse return error.CorruptEvent;
    }

    fn read_str(self: *Reader, arena: std.mem.Allocator) CodecError![]const u8 {
        const n = try self.read_u32();
        if (n > max_str_len) return error.CorruptEvent;
        const raw = try self.read_bytes(n);
        if (!std.unicode.utf8ValidateSlice(raw)) return error.CorruptEvent;
        return arena.dupe(u8, raw);
    }

    fn read_opt_str(self: *Reader, arena: std.mem.Allocator) CodecError!?[]const u8 {
        return switch (try self.read_u8()) {
            0 => null,
            1 => try self.read_str(arena),
            else => error.CorruptEvent,
        };
    }

    fn read_opt_u64(self: *Reader) CodecError!?u64 {
        return switch (try self.read_u8()) {
            0 => null,
            1 => try self.read_u64(),
            else => error.CorruptEvent,
        };
    }

    fn read_opt_i32(self: *Reader) CodecError!?i32 {
        return switch (try self.read_u8()) {
            0 => null,
            1 => try self.read_i32(),
            else => error.CorruptEvent,
        };
    }
};

fn read_quota_selection(r: *Reader, arena: std.mem.Allocator) CodecError!event.QuotaSelection {
    const launching = try r.read_bool();
    return .{
        .harness = if (launching) try r.read_enum(model.Harness) else null,
        .model_name = if (launching) try r.read_opt_str(arena) else null,
        .provider = if (launching) try r.read_opt_str(arena) else null,
        .reason = try r.read_str(arena),
    };
}

pub fn decode_stamped(arena: std.mem.Allocator, bytes: []const u8) CodecError!event.StampedEvent {
    var r = Reader{ .b = bytes };
    const ver = try r.read_u32();
    if (ver != format_version) return error.UnsupportedFormat;
    const seq = try r.read_u64();
    const ts_ms = try r.read_i64();
    const EventTag = std.meta.Tag(event.Event);
    const tag = try r.read_enum(EventTag);

    const ev: event.Event = switch (tag) {
        .sleeve_created => .{ .sleeve_created = .{
            .sleeve_id = try r.read_str(arena),
            .name = try r.read_str(arena),
            .role = try r.read_enum(model.SleeveRole),
            .path = try r.read_str(arena),
            .workspace_id = try r.read_str(arena),
            .parent_id = try r.read_opt_str(arena),
            .layout = try r.read_enum(model.Layout),
            .branch = try r.read_opt_str(arena),
            .budget_limit = try r.read_opt_u64(),
            .until_done = try r.read_u64(),
        } },
        .provisioned => .{ .provisioned = .{
            .sleeve_id = try r.read_str(arena),
            .branch = try r.read_opt_str(arena),
        } },
        .provision_failed => .{ .provision_failed = .{
            .sleeve_id = try r.read_str(arena),
            .reason = try r.read_str(arena),
        } },
        .start_requested => blk: {
            const sid = try r.read_str(arena);
            const tlen = try r.read_u8();
            if (tlen == 0 or tlen > model.max_targets) return error.CorruptEvent;
            var targets: [model.max_targets]model.Harness = undefined;
            for (0..tlen) |i| targets[i] = try r.read_enum(model.Harness);
            break :blk .{ .start_requested = .{
                .sleeve_id = sid,
                .targets = targets,
                .target_len = tlen,
                .mode = try r.read_enum(model.Mode),
                .model_name = try r.read_opt_str(arena),
                .provider = try r.read_opt_str(arena),
                .effort = try r.read_opt_str(arena),
                .until_done = try r.read_opt_u64(),
                .selection_reason = if (r.i == r.b.len) null else try r.read_str(arena),
            } };
        },
        .launched => .{ .launched = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .harness = try r.read_enum(model.Harness),
            .mode = try r.read_enum(model.Mode),
            .model_name = try r.read_str(arena),
            .provider = try r.read_str(arena),
            .effort = try r.read_opt_str(arena),
        } },
        .session_bound => .{ .session_bound = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .native_id = try r.read_str(arena),
        } },
        .prompt_enqueued => .{ .prompt_enqueued = .{
            .sleeve_id = try r.read_str(arena),
            .request_id = try r.read_str(arena),
            .from = try r.read_str(arena),
            .kind = try r.read_enum(model.MsgKind),
            .mode = try r.read_enum(model.SendMode),
            .text = try r.read_str(arena),
            .supersede = try r.read_bool(),
            .until_done = try r.read_opt_u64(),
        } },
        .prompt_delivered => .{ .prompt_delivered = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .request_id = try r.read_str(arena),
        } },
        .prompt_acked => .{ .prompt_acked = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .request_id = try r.read_str(arena),
            .confirmed = try r.read_bool(),
            .detail = try r.read_opt_str(arena),
        } },
        .turn_started => .{ .turn_started = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .request_id = try r.read_opt_str(arena),
            .from_agent_mail = try r.read_bool(),
        } },
        .turn_settled => .{ .turn_settled = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .turn_end = try r.read_enum(model.TurnEnd),
            .declared_status = switch (try r.read_u8()) {
                0 => null,
                1 => try r.read_enum(model.DeclaredStatus),
                else => return error.CorruptEvent,
            },
            .context_tokens = try r.read_u64(),
            .context_window = try r.read_u64(),
            .quota_handoff = if (r.i < r.b.len and try r.read_bool()) .{
                .cause = if (r.i == r.b.len) .quota_exhausted else try r.read_enum(model.ExitCause),
                .selection = if (r.i == r.b.len) null else try read_quota_selection(&r, arena),
            } else null,
        } },
        .tool_activity => .{ .tool_activity = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .tool_sig = try r.read_str(arena),
        } },
        .compacted => .{ .compacted = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .tokens_after = try r.read_u64(),
        } },
        .interrupted => .{ .interrupted = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .by = try r.read_enum(model.Party),
        } },
        .exited => .{ .exited = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .cause = try r.read_enum(model.ExitCause),
            .requested = try r.read_bool(),
            .quota_selection = if (r.i == r.b.len) null else try read_quota_selection(&r, arena),
        } },
        .stop_requested => .{ .stop_requested = .{
            .sleeve_id = try r.read_str(arena),
            .by = try r.read_enum(model.Party),
        } },
        .stopped => .{ .stopped = .{ .sleeve_id = try r.read_str(arena) } },
        .resleeve_requested => .{ .resleeve_requested = .{
            .sleeve_id = try r.read_str(arena),
            .harness = try r.read_enum(model.Harness),
            .model_name = try r.read_opt_str(arena),
            .provider = try r.read_opt_str(arena),
            .effort = try r.read_opt_str(arena),
            .by = try r.read_enum(model.Party),
            .cause = try r.read_enum(model.Cause),
            .inherit_preferences = if (r.i == r.b.len) true else try r.read_bool(),
        } },
        .parked => .{ .parked = .{
            .sleeve_id = try r.read_str(arena),
            .reason = try r.read_enum(model.ParkReason),
        } },
        .removed => .{ .removed = .{ .sleeve_id = try r.read_str(arena) } },
        .drop_queued => .{ .drop_queued = .{ .sleeve_id = try r.read_str(arena) } },
        .approval_requested => .{ .approval_requested = .{
            .approval_id = try r.read_str(arena),
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .kind = try r.read_enum(model.ApprovalKind),
            .title = try r.read_str(arena),
            .summary = try r.read_str(arena),
            .is_d9 = try r.read_bool(),
            .timeout_ms = if (r.i == r.b.len) model.approval_timeout_ms else try r.read_i64(),
        } },
        .approval_decided => .{ .approval_decided = .{
            .approval_id = try r.read_str(arena),
            .verdict = try r.read_enum(model.Verdict),
            .by = try r.read_enum(model.Party),
            .note = try r.read_str(arena),
        } },
        .approval_expired => .{ .approval_expired = .{ .approval_id = try r.read_str(arena) } },
        .node_registered => blk: {
            const nid = try r.read_str(arena);
            const nm = try r.read_str(arena);
            const pk = try r.read_bytes(32);
            var key: [32]u8 = undefined;
            @memcpy(&key, pk);
            break :blk .{ .node_registered = .{ .node_id = nid, .name = nm, .pub_key = key } };
        },
        .node_approved => .{ .node_approved = .{
            .node_id = try r.read_str(arena),
            .name = try r.read_str(arena),
        } },
        .node_connected => .{ .node_connected = .{
            .node_id = try r.read_str(arena),
            .arch = try r.read_str(arena),
            .version = try r.read_str(arena),
        } },
        .node_disconnected => .{ .node_disconnected = .{
            .node_id = try r.read_str(arena),
            .reason = try r.read_str(arena),
        } },
        .node_removed => .{ .node_removed = .{ .node_id = try r.read_str(arena) } },
        .node_renamed => .{ .node_renamed = .{
            .node_id = try r.read_str(arena),
            .name = try r.read_str(arena),
        } },
        .node_jobs_updated => .{ .node_jobs_updated = .{
            .node_id = try r.read_str(arena),
            .jobs = try r.read_enum(model.NodeJobs),
            .note = try r.read_opt_str(arena),
        } },
        .node_presence_updated => .{ .node_presence_updated = .{
            .node_id = try r.read_str(arena),
            .presence = try r.read_enum(model.ProbeResult),
        } },
        .job_queued => .{ .job_queued = .{
            .job_id = try r.read_str(arena),
            .target_node = try r.read_opt_str(arena),
            .target_arch = try r.read_opt_str(arena),
            .requested_by = try r.read_str(arena),
            .on_present = try r.read_enum(model.OnPresent),
            .even_if_present = try r.read_bool(),
            .maintenance = try r.read_bool(),
        } },
        .job_submitted => .{ .job_submitted = .{
            .job_id = try r.read_str(arena),
            .node_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
        } },
        .job_submit_uncertain => .{ .job_submit_uncertain = .{
            .job_id = try r.read_str(arena),
            .node_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
        } },
        .job_started => .{ .job_started = .{
            .job_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
        } },
        .job_completed => .{ .job_completed = .{
            .job_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
            .exit_code = try r.read_i32(),
        } },
        .job_failed => .{ .job_failed = .{
            .job_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
            .err = try r.read_str(arena),
        } },
        .job_interrupted => .{ .job_interrupted = .{
            .job_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
        } },
        .job_lost => .{ .job_lost = .{
            .job_id = try r.read_str(arena),
            .attempt = try r.read_u32(),
        } },
        .job_cancel_requested => .{ .job_cancel_requested = .{
            .job_id = try r.read_str(arena),
            .by = try r.read_enum(model.Party),
        } },
        .job_cancelled => .{ .job_cancelled = .{ .job_id = try r.read_str(arena) } },
        .job_abandoned => .{ .job_abandoned = .{
            .job_id = try r.read_str(arena),
            .reason = try r.read_str(arena),
        } },
        .job_verified => .{ .job_verified = .{
            .job_id = try r.read_str(arena),
            .still_running = try r.read_bool(),
            .exit_code = try r.read_opt_i32(),
        } },
        .rollout_started => blk: {
            const rid = try r.read_str(arena);
            const ver_s = try r.read_str(arena);
            const cnt = try r.read_u32();
            if (cnt > max_list_len) return error.CorruptEvent;
            const nids = try arena.alloc([]const u8, cnt);
            for (0..cnt) |k| nids[k] = try r.read_str(arena);
            break :blk .{ .rollout_started = .{ .rollout_id = rid, .version = ver_s, .node_ids = nids } };
        },
        .rollout_node_result => .{ .rollout_node_result = .{
            .rollout_id = try r.read_str(arena),
            .node_id = try r.read_str(arena),
            .ok = try r.read_bool(),
            .detail = try r.read_str(arena),
        } },
        .rollout_cancelled => .{ .rollout_cancelled = .{ .rollout_id = try r.read_str(arena) } },
        .tick => .{ .tick = .{} },
        .exit_classified => .{ .exit_classified = .{
            .sleeve_id = try r.read_str(arena),
            .g = try r.read_u64(),
            .cause = try r.read_enum(model.ExitCause),
        } },
    };

    if (r.i != r.b.len) return error.CorruptEvent;
    return .{ .seq = seq, .ts_ms = ts_ms, .event = ev };
}
