const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");

pub const max_tool_output: usize = 16 << 10;
pub const max_tool_input: usize = 16 << 10;
pub const max_block: usize = 1 << 20;
pub const truncated_marker = "\n[protectorate: block truncated at 1 MiB]";
pub const max_token_count: u64 = 1 << 40;
pub const max_summary_len: usize = 300;

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
};

pub const Permission = enum {
    unsupervised,
    supervised,
};

pub const AckKind = enum {
    confirmed,
    rejected,
};

pub const ToolKind = enum {
    command,
    read,
    edit,
    search,
    web,
    mcp,
    subagent,
    other,
};

pub const ToolStatus = enum {
    completed,
    failed,
    declined,
    cancelled,
};

pub const Stop = enum {
    end_turn,
    cancelled,
    @"error",
    max_turns,
    other,
};

pub const ErrorClass = enum {
    auth,
    quota,
    rate_limit,
    protocol,
    crash,
    other,
};

pub const Decision = enum {
    allow_once,
    allow_session,
    deny,
    cancel,
};

pub const Level = enum {
    info,
    warn,
    alarm,
};

pub const Usage = struct {
    input: u64 = 0,
    output: u64 = 0,
    cache_read: u64 = 0,
    cache_write: u64 = 0,
    cost_usd: ?f64 = null,
    context_tokens: ?u64 = null,
    context_window: ?u64 = null,
};

pub const FileDelta = struct {
    path: []const u8,
    op: ?[]const u8 = null,
};

pub const DriverEntry = union(enum) {
    session_info: struct {
        harness: Harness,
        native_id: []const u8,
        model: ?[]const u8 = null,
        provider: ?[]const u8 = null,
        effort: ?[]const u8 = null,
        effort_notice: ?[]const u8 = null,
        cwd: ?[]const u8 = null,
    },
    user_message: struct {
        request_id: []const u8,
        from: []const u8,
        text: []const u8,
        steer: bool,
    },
    prompt_ack: struct {
        request_id: []const u8,
        ack: AckKind,
        native_turn_id: ?[]const u8 = null,
        detail: ?[]const u8 = null,
    },
    assistant_text: struct {
        id: []const u8,
        text: []const u8,
        delta: bool,
    },
    reasoning: struct {
        id: []const u8,
        text: []const u8,
        delta: bool,
    },
    tool_call: struct {
        id: []const u8,
        tool: ToolKind,
        name: []const u8,
        summary: []const u8,
        input_json: ?[]const u8 = null,
        signature: ?[]const u8 = null,
        truncated: bool,
    },
    tool_result: struct {
        id: []const u8,
        status: ToolStatus,
        output: []const u8,
        truncated: bool,
        duration_ms: ?u64 = null,
    },
    file_change: struct {
        tool_id: ?[]const u8 = null,
        files: []const FileDelta,
    },
    approval_request: struct {
        id: []const u8,
        tool: ToolKind,
        name: []const u8,
        summary: []const u8,
        input_json: ?[]const u8 = null,
        options: []const Decision,
    },
    approval_decision: struct {
        id: []const u8,
        decision: Decision,
        by: ?[]const u8 = null,
        confirmed: bool,
    },
    turn_started: struct {
        request_id: ?[]const u8 = null,
        native_turn_id: ?[]const u8 = null,
    },
    turn_settled: struct {
        stop: Stop,
        native_stop: ?[]const u8 = null,
        usage: Usage = .{},
        duration_ms: ?u64 = null,
    },
    @"error": struct {
        kind: ErrorClass,
        text: []const u8,
    },
    compaction: struct {
        trigger: ?[]const u8 = null,
        tokens_before: ?u64 = null,
        tokens_after: ?u64 = null,
    },
    notice: struct {
        level: Level,
        code: ?[]const u8 = null,
        text: []const u8,
    },
};

pub const DriverEvent = struct {
    seq: u64,
    g: u64,
    time_ms: i64,
    entry: DriverEntry,
};

pub const WriteLine = struct {
    json_line: []const u8,
    request_id: ?[]const u8 = null,
};

pub const SpawnSpec = struct {
    harness: Harness,
    program: []const u8,
    cwd: []const u8,
    g: u64 = 0,
    model: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    effort: ?[]const u8 = null,
    resume_id: ?[]const u8 = null,
    session_id: ?[]const u8 = null,
    permission: Permission = .unsupervised,
    extra_args: []const []const u8 = &.{},
};

const OpenText = struct {
    reasoning: bool,
    key: []const u8,
    part: u32,
    buf: std.ArrayList(u8),
    truncated: bool,

    fn make_id(self: *const OpenText, alloc: std.mem.Allocator) ![]const u8 {
        if (self.part <= 1) return try alloc.dupe(u8, self.key);
        return std.fmt.allocPrint(alloc, "{s}~{d}", .{ self.key, self.part });
    }
};

pub const Out = struct {
    g: u64,
    next_seq: u64 = 1,
    now_ms: i64 = 0,
    refused: bool = false,
    stray: u64 = 0,
    open: ?OpenText = null,
    events: std.ArrayList(DriverEvent) = .empty,
    writes: std.ArrayList(WriteLine) = .empty,
    seen_notes: std.StringHashMapUnmanaged(void) = .empty,
    alloc: std.mem.Allocator,

    pub fn init(alloc: std.mem.Allocator, g: u64) Out {
        return .{ .g = g, .alloc = alloc };
    }

    pub fn write_raw_line(self: *Out, json_line: []const u8, req_id: ?[]const u8) !void {
        try self.writes.append(self.alloc, .{
            .json_line = try self.alloc.dupe(u8, json_line),
            .request_id = if (req_id) |r| try self.alloc.dupe(u8, r) else null,
        });
    }

    pub fn write_value(self: *Out, val: anytype, req_id: ?[]const u8) !void {
        const s = try json_util.stringify_alloc(self.alloc, val);
        try self.writes.append(self.alloc, .{
            .json_line = s,
            .request_id = if (req_id) |r| try self.alloc.dupe(u8, r) else null,
        });
    }

    pub fn flush_open(self: *Out) !void {
        var o = self.open orelse return;
        self.open = null;
        const id = try o.make_id(self.alloc);
        const txt = try self.alloc.dupe(u8, o.buf.items);
        if (o.reasoning) {
            try self.push_raw(.{ .reasoning = .{ .id = id, .text = txt, .delta = false } });
        } else {
            try self.push_raw(.{ .assistant_text = .{ .id = id, .text = txt, .delta = false } });
        }
    }

    pub fn delta(self: *Out, reasoning: bool, key: []const u8, chunk: []const u8) !void {
        if (chunk.len == 0) return;
        if (self.open) |*o| {
            if (o.reasoning != reasoning or !std.mem.eql(u8, o.key, key)) {
                try self.flush_open();
            }
        }
        if (self.open == null) {
            self.open = .{
                .reasoning = reasoning,
                .key = try self.alloc.dupe(u8, key),
                .part = 1,
                .buf = .empty,
                .truncated = false,
            };
        }
        var o = &self.open.?;
        if (!o.truncated) {
            const avail = max_block -| o.buf.items.len;
            if (chunk.len <= avail) {
                try o.buf.appendSlice(self.alloc, chunk);
            } else {
                const take = clip_utf8_len(chunk, avail);
                try o.buf.appendSlice(self.alloc, chunk[0..take]);
                try o.buf.appendSlice(self.alloc, truncated_marker);
                o.truncated = true;
            }
        }
        const id = try o.make_id(self.alloc);
        const dup_chunk = try self.alloc.dupe(u8, chunk);
        if (reasoning) {
            try self.push_raw(.{ .reasoning = .{ .id = id, .text = dup_chunk, .delta = true } });
        } else {
            try self.push_raw(.{ .assistant_text = .{ .id = id, .text = dup_chunk, .delta = true } });
        }
    }

    pub fn finish_text(self: *Out, reasoning: bool, key: []const u8, final_text: []const u8) !void {
        if (self.open) |*o| {
            if (o.reasoning == reasoning and std.mem.eql(u8, o.key, key)) {
                try self.flush_open();
                return;
            }
        }
        try self.flush_open();
        const clipped = clip_utf8(final_text, max_block);
        const dup_id = try self.alloc.dupe(u8, key);
        const dup_txt = try self.alloc.dupe(u8, clipped);
        if (reasoning) {
            try self.push_raw(.{ .reasoning = .{ .id = dup_id, .text = dup_txt, .delta = false } });
        } else {
            try self.push_raw(.{ .assistant_text = .{ .id = dup_id, .text = dup_txt, .delta = false } });
        }
    }

    pub fn emit(self: *Out, entry: DriverEntry) !void {
        try self.flush_open();
        try self.push_raw(entry);
    }

    fn push_raw(self: *Out, entry: DriverEntry) !void {
        const seq = self.next_seq;
        self.next_seq += 1;
        try self.events.append(self.alloc, .{
            .seq = seq,
            .g = self.g,
            .time_ms = self.now_ms,
            .entry = entry,
        });
    }

    pub fn emit_error(self: *Out, kind: ErrorClass, text: []const u8) !void {
        try self.emit(.{ .@"error" = .{
            .kind = kind,
            .text = try self.alloc.dupe(u8, text),
        } });
    }

    pub fn notice(self: *Out, code: []const u8, text: []const u8) !void {
        try self.emit(.{ .notice = .{
            .level = .warn,
            .code = try self.alloc.dupe(u8, code),
            .text = try self.alloc.dupe(u8, text),
        } });
    }

    pub fn unmapped(self: *Out, kind: []const u8) !void {
        if (self.seen_notes.contains(kind)) return;
        const k = try self.alloc.dupe(u8, kind);
        try self.seen_notes.put(self.alloc, k, {});
        const msg = try std.fmt.allocPrint(self.alloc, "unhandled harness event: {s}", .{kind});
        try self.emit(.{ .notice = .{ .level = .info, .code = "unmapped", .text = msg } });
    }

    pub fn tool_call(self: *Out, id: []const u8, tool: ToolKind, name: []const u8, input_val: std.json.Value) !void {
        const raw_json: ?[]const u8 = if (input_val == .null) null else try json_util.stringify_alloc(self.alloc, input_val);
        const truncated = if (raw_json) |rj| rj.len > max_tool_input else false;
        const clipped_json: ?[]const u8 = if (raw_json) |rj| clip_utf8(rj, max_tool_input) else null;
        const summary = try summarize_alloc(self.alloc, name, input_val);
        const signature = if (raw_json) |raw| blk: {
            var digest: [32]u8 = undefined;
            std.crypto.hash.sha2.Sha256.hash(raw, &digest, .{});
            break :blk try std.fmt.allocPrint(self.alloc, "{s}:{x}", .{ name, std.fmt.bytesToHex(digest, .lower) });
        } else null;
        try self.emit(.{
            .tool_call = .{
                .id = try self.alloc.dupe(u8, id),
                .tool = tool,
                .name = try self.alloc.dupe(u8, name),
                .summary = summary,
                .input_json = clipped_json,
                .signature = signature,
                .truncated = truncated,
            },
        });
    }

    pub fn count_u64(self: *Out, name: []const u8, v: std.json.Value) ?u64 {
        _ = self;
        _ = name;
        const n = json_util.as_u64(v) orelse return null;
        if (n > max_token_count) return null;
        return n;
    }
};

pub fn clip_utf8_len(s: []const u8, max_bytes: usize) usize {
    if (s.len <= max_bytes) return s.len;
    var i = max_bytes;
    while (i > 0 and (s[i] & 0xc0) == 0x80) i -= 1;
    return i;
}

pub fn clip_utf8(s: []const u8, max_bytes: usize) []const u8 {
    return s[0..clip_utf8_len(s, max_bytes)];
}

pub fn bounded_output(alloc: std.mem.Allocator, raw: []const u8) !struct { text: []const u8, truncated: bool } {
    if (raw.len <= max_tool_output) {
        return .{ .text = try alloc.dupe(u8, raw), .truncated = false };
    }
    return .{ .text = try alloc.dupe(u8, clip_utf8(raw, max_tool_output)), .truncated = true };
}

pub fn summarize_alloc(alloc: std.mem.Allocator, name: []const u8, input_val: std.json.Value) ![]const u8 {
    if (json_util.value_str(input_val, "CommandLine") orelse
        json_util.value_str(input_val, "command") orelse
        json_util.value_str(input_val, "cmd") orelse
        json_util.value_str(input_val, "file_path") orelse
        json_util.value_str(input_val, "path") orelse
        json_util.value_str(input_val, "pattern") orelse
        json_util.value_str(input_val, "query") orelse
        json_util.value_str(input_val, "description")) |detail|
    {
        const combined = try std.fmt.allocPrint(alloc, "{s}: {s}", .{ name, detail });
        return clip_utf8(combined, max_summary_len);
    }
    return try alloc.dupe(u8, clip_utf8(name, max_summary_len));
}

pub fn content_text_alloc(alloc: std.mem.Allocator, v: std.json.Value) ![]const u8 {
    switch (v) {
        .string => |s| return try alloc.dupe(u8, s),
        .array => |arr| {
            var buf: std.ArrayList(u8) = .empty;
            for (arr.items) |it| {
                const t: ?[]const u8 = switch (it) {
                    .string => |s| s,
                    .object => json_util.value_str(it, "text") orelse json_util.value_str(it, "content"),
                    else => null,
                };
                if (t) |piece| {
                    if (buf.items.len > 0) try buf.append(alloc, '\n');
                    try buf.appendSlice(alloc, piece);
                }
            }
            return buf.items;
        },
        else => return "",
    }
}

pub fn classify_error(text: []const u8) ErrorClass {
    if (contains_ci(text, "unauthorized") or contains_ci(text, "invalid api key") or contains_ci(text, "authentication") or contains_ci(text, "not logged in") or contains_ci(text, "401")) {
        return .auth;
    }
    if (contains_ci(text, "quota") or contains_ci(text, "insufficient_quota") or contains_ci(text, "usage limit") or contains_ci(text, "credit balance")) {
        return .quota;
    }
    if (contains_ci(text, "rate limit") or contains_ci(text, "rate_limit") or contains_ci(text, "429") or contains_ci(text, "overloaded")) {
        return .rate_limit;
    }
    if (contains_ci(text, "sigkill") or contains_ci(text, "oom") or contains_ci(text, "killed") or contains_ci(text, "segmentation")) {
        return .crash;
    }
    return .other;
}

fn contains_ci(hay: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > hay.len) return false;
    var i: usize = 0;
    while (i + needle.len <= hay.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(hay[i .. i + needle.len], needle)) return true;
    }
    return false;
}
