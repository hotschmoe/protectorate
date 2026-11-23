const std = @import("std");
const tiger = @import("../tiger.zig");
const json_util = @import("../json_util.zig");

pub const text_max: usize = 32 << 10;
pub const tool_max: usize = 512;

pub const Role = enum(u8) {
    user = 0,
    assistant = 1,
    tool_call = 2,
    tool_error = 3,
    compaction = 4,

    pub fn from_code(c: u64) ?Role {
        return switch (c) {
            0 => .user,
            1 => .assistant,
            2 => .tool_call,
            3 => .tool_error,
            4 => .compaction,
            else => null,
        };
    }

    pub fn to_wire(self: Role) []const u8 {
        return @tagName(self);
    }
};

pub const Harness = enum(u8) {
    claude = 0,
    codex = 1,
    grok = 2,
    pi_vendor = 3,
    pi_local = 4,
    antigravity = 5,

    pub fn from_wire(s: []const u8) ?Harness {
        return std.meta.stringToEnum(Harness, s);
    }

    pub fn to_wire(self: Harness) []const u8 {
        return @tagName(self);
    }
};

pub const Draft = struct {
    role: Role,
    turn: u32,
    time_ms: u64,
    session: []const u8,
    harness: ?Harness,
    tool: ?[]const u8,
    files: []const []const u8,
    text: []const u8,
};

pub const Item = union(enum) {
    doc: Draft,
    usage: struct {
        session: []const u8,
        harness: ?Harness,
        tokens: u64,
        window: ?u64,
        at_ms: u64,
    },
    compacted: struct {
        session: []const u8,
    },
};

pub const ExtractState = struct {
    turn: u32 = 0,
    time_ms: u64 = 0,
    session: []const u8 = "",
    harness: ?Harness = null,

    pub fn push(
        self: *ExtractState,
        arena: std.mem.Allocator,
        out: *std.ArrayList(Item),
        role: Role,
        raw_text: []const u8,
        tool: ?[]const u8,
        raw_files: []const []const u8,
    ) !void {
        const text = std.mem.trim(u8, raw_text, " \t\r\n");
        const files = try take_files(arena, raw_files);
        if (text.len == 0 and files.len == 0) return;
        if (role == .user) self.turn +|= 1;
        const max_len: usize = switch (role) {
            .tool_call, .tool_error => tool_max,
            else => text_max,
        };
        const t_clipped = clip_utf8(text, max_len);
        const tool_dup: ?[]const u8 = if (tool) |tl| try arena.dupe(u8, clip_utf8(tl, 128)) else null;
        try out.append(arena, .{
            .doc = .{
                .role = role,
                .turn = self.turn,
                .time_ms = self.time_ms,
                .session = try arena.dupe(u8, self.session),
                .harness = self.harness,
                .tool = tool_dup,
                .files = files,
                .text = try arena.dupe(u8, t_clipped),
            },
        });
    }

    pub fn stamp(self: *ExtractState, v: std.json.Value) void {
        switch (v) {
            .string => |s| {
                if (parse_iso8601_ms(s)) |ms| self.time_ms = ms;
            },
            .integer => |n| {
                if (n >= 0) self.time_ms = @intCast(n);
            },
            else => {},
        }
    }
};

pub fn clip_utf8(s: []const u8, max_bytes: usize) []const u8 {
    if (s.len <= max_bytes) return s;
    var i = max_bytes;
    while (i > 0 and (s[i] & 0xc0) == 0x80) {
        i -= 1;
    }
    return s[0..i];
}

pub fn parse_iso8601_ms(s: []const u8) ?u64 {
    if (s.len < 19) return null;
    if (s[4] != '-' or s[7] != '-' or (s[10] != 'T' and s[10] != ' ')) return null;
    if (s[13] != ':' or s[16] != ':') return null;

    const yr = parse_digits(s[0..4]) orelse return null;
    const mo = parse_digits(s[5..7]) orelse return null;
    const dy = parse_digits(s[8..10]) orelse return null;
    const hr = parse_digits(s[11..13]) orelse return null;
    const mi = parse_digits(s[14..16]) orelse return null;
    const sc = parse_digits(s[17..19]) orelse return null;

    if (mo < 1 or mo > 12 or dy < 1 or dy > 31 or hr > 23 or mi > 59 or sc > 60) return null;

    var i: usize = 19;
    var ms: i64 = 0;
    if (i < s.len and s[i] == '.') {
        i += 1;
        const start = i;
        while (i < s.len and std.ascii.isDigit(s[i])) : (i += 1) {
            if (i - start < 3) {
                ms = ms * 10 + @as(i64, s[i] - '0');
            }
        }
        if (i == start) return null;
        var k = i - start;
        while (k < 3) : (k += 1) ms *= 10;
    }

    var offset_min: i64 = 0;
    if (i < s.len) {
        const ch = s[i];
        if (ch == 'Z' or ch == 'z') {
            i += 1;
        } else if (ch == '+' or ch == '-') {
            if (i + 5 > s.len) return null;
            const oh = parse_digits(s[i + 1 .. i + 3]) orelse return null;
            const om = if (i + 3 < s.len and s[i + 3] == ':')
                (if (i + 6 <= s.len) parse_digits(s[i + 4 .. i + 6]) orelse return null else return null)
            else
                parse_digits(s[i + 3 .. i + 5]) orelse return null;
            const m = oh * 60 + om;
            offset_min = if (ch == '+') m else -m;
        } else {
            return null;
        }
    }

    const y = if (mo <= 2) yr - 1 else yr;
    const era = @divFloor(y, 400);
    const yoe = y - era * 400;
    const mp = @rem(mo + 9, 12);
    const doy = @divFloor(153 * mp + 2, 5) + dy - 1;
    const doe = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    const days = era * 146_097 + doe - 719_468;
    const secs = days * 86_400 + hr * 3600 + mi * 60 + sc - offset_min * 60;
    const total = secs * 1000 + ms;
    if (total < 0) return null;
    return @intCast(total);
}

fn parse_digits(slice: []const u8) ?i64 {
    if (slice.len == 0) return null;
    var acc: i64 = 0;
    for (slice) |c| {
        if (!std.ascii.isDigit(c)) return null;
        acc = acc * 10 + @as(i64, c - '0');
    }
    return acc;
}

const wrapped_prefixes = [_][]const u8{
    "<system-reminder>",
    "<environment_context>",
    "<local-command-caveat>",
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<command-name>",
    "<command-message>",
    "<command-args>",
};

pub fn is_wrapped(text: []const u8) bool {
    const t = std.mem.trimStart(u8, text, " \t\r\n");
    for (wrapped_prefixes) |p| {
        if (std.mem.startsWith(u8, t, p)) return true;
    }
    return false;
}

pub fn text_of_alloc(arena: std.mem.Allocator, v: std.json.Value) ![]const u8 {
    switch (v) {
        .string => |s| return try arena.dupe(u8, s),
        .array => |arr| {
            var out: std.ArrayList(u8) = .empty;
            for (arr.items) |b| {
                const t: ?[]const u8 = switch (b) {
                    .string => |s| s,
                    .object => blk: {
                        const kind = json_util.value_str(b, "type") orelse break :blk null;
                        if (std.mem.eql(u8, kind, "text") or
                            std.mem.eql(u8, kind, "input_text") or
                            std.mem.eql(u8, kind, "output_text"))
                        {
                            break :blk json_util.value_str(b, "text");
                        }
                        break :blk null;
                    },
                    else => null,
                };
                if (t) |piece| {
                    if (out.items.len > 0) try out.append(arena, '\n');
                    try out.appendSlice(arena, piece);
                }
            }
            return out.toOwnedSlice(arena);
        },
        else => return "",
    }
}

fn take_files(arena: std.mem.Allocator, files: []const []const u8) ![]const []const u8 {
    var out: std.ArrayList([]const u8) = .empty;
    var used: usize = 0;
    for (files) |raw_f| {
        const f = clip_utf8(std.mem.trim(u8, raw_f, " \t\r\n"), tool_max);
        if (f.len == 0) continue;
        if (out.items.len > 0 and used +| f.len > tool_max) break;
        used +|= f.len;
        try out.append(arena, try arena.dupe(u8, f));
    }
    return out.toOwnedSlice(arena);
}

const path_keys = [_][]const u8{ "file_path", "path", "notebook_path", "target_file", "filePath" };
const arg_keys = [_][]const u8{ "command", "cmd", "pattern", "query", "url", "description", "old_string", "prompt" };

fn extract_patch_files(arena: std.mem.Allocator, patch: []const u8) ![]const []const u8 {
    var out: std.ArrayList([]const u8) = .empty;
    var lines = std.mem.splitScalar(u8, patch, '\n');
    const markers = [_][]const u8{ "*** Update File: ", "*** Add File: ", "*** Delete File: " };
    while (lines.next()) |line| {
        for (markers) |m| {
            if (std.mem.startsWith(u8, line, m)) {
                const f = std.mem.trim(u8, line[m.len..], " \t\r");
                if (f.len > 0) {
                    var dup = false;
                    for (out.items) |prev| {
                        if (std.mem.eql(u8, prev, f)) {
                            dup = true;
                            break;
                        }
                    }
                    if (!dup) try out.append(arena, try arena.dupe(u8, f));
                }
            }
        }
    }
    return out.toOwnedSlice(arena);
}

pub fn tool_call_extract(
    arena: std.mem.Allocator,
    name: []const u8,
    input: std.json.Value,
) !struct { text: []const u8, files: []const []const u8 } {
    var parsed_input = input;
    if (input == .string) {
        const s = input.string;
        if (std.json.parseFromSliceLeaky(std.json.Value, arena, s, .{})) |pv| {
            if (pv == .object) {
                parsed_input = pv;
            } else {
                const files = try extract_patch_files(arena, s);
                return .{
                    .text = try std.fmt.allocPrint(arena, "{s} {s}", .{ name, clip_utf8(s, tool_max) }),
                    .files = files,
                };
            }
        } else |_| {
            const files = try extract_patch_files(arena, s);
            return .{
                .text = try std.fmt.allocPrint(arena, "{s} {s}", .{ name, clip_utf8(s, tool_max) }),
                .files = files,
            };
        }
    }

    var text: std.ArrayList(u8) = .empty;
    try text.appendSlice(arena, name);
    var files: std.ArrayList([]const u8) = .empty;

    for (path_keys) |k| {
        if (json_util.value_str(parsed_input, k)) |p| {
            try text.append(arena, ' ');
            try text.appendSlice(arena, p);
            try files.append(arena, p);
        }
    }

    for (arg_keys) |k| {
        if (json_util.value_get(parsed_input, k)) |val| {
            switch (val) {
                .string => |s| {
                    try text.append(arena, ' ');
                    try text.appendSlice(arena, s);
                },
                .array => |arr| {
                    for (arr.items) |it| {
                        if (it == .string) {
                            try text.append(arena, ' ');
                            try text.appendSlice(arena, it.string);
                        }
                    }
                },
                else => {},
            }
        }
        if (text.items.len > tool_max) break;
    }

    if (json_util.value_str(parsed_input, "patch") orelse json_util.value_str(parsed_input, "input")) |patch_str| {
        const pf = try extract_patch_files(arena, patch_str);
        try files.appendSlice(arena, pf);
    }

    return .{
        .text = clip_utf8(text.items, text_max),
        .files = try files.toOwnedSlice(arena),
    };
}

pub fn extract_claude_line(
    arena: std.mem.Allocator,
    v: std.json.Value,
    st: *ExtractState,
    out: *std.ArrayList(Item),
) !bool {
    const kind = json_util.value_str(v, "type") orelse return false;
    if (!std.mem.eql(u8, kind, "user") and
        !std.mem.eql(u8, kind, "assistant") and
        !std.mem.eql(u8, kind, "system"))
    {
        return true;
    }
    if (json_util.value_bool(v, "isSidechain") == true or
        json_util.value_bool(v, "isMeta") == true)
    {
        return true;
    }
    if (json_util.value_get(v, "timestamp")) |ts| st.stamp(ts);

    if (std.mem.eql(u8, kind, "user")) {
        const msg = json_util.value_get(v, "message") orelse return true;
        const content = json_util.value_get(msg, "content") orelse .null;
        if (json_util.value_bool(v, "isCompactSummary") == true) {
            const t = try text_of_alloc(arena, content);
            try st.push(arena, out, .compaction, t, null, &.{});
            return true;
        }
        var prompt: std.ArrayList(u8) = .empty;
        switch (content) {
            .string => |s| {
                if (!is_wrapped(s)) try prompt.appendSlice(arena, s);
            },
            .array => |blocks| {
                for (blocks.items) |b| {
                    const bt = json_util.value_str(b, "type") orelse continue;
                    if (std.mem.eql(u8, bt, "text")) {
                        if (json_util.value_str(b, "text")) |t| {
                            if (!is_wrapped(t)) {
                                if (prompt.items.len > 0) try prompt.append(arena, '\n');
                                try prompt.appendSlice(arena, t);
                            }
                        }
                    } else if (std.mem.eql(u8, bt, "tool_result") and json_util.value_bool(b, "is_error") == true) {
                        const err_c = json_util.value_get(b, "content") orelse .null;
                        const t = try text_of_alloc(arena, err_c);
                        try st.push(arena, out, .tool_error, t, null, &.{});
                    }
                }
            },
            else => {},
        }
        try st.push(arena, out, .user, prompt.items, null, &.{});
    } else if (std.mem.eql(u8, kind, "assistant")) {
        const msg = json_util.value_get(v, "message") orelse return true;
        if (json_util.value_get(msg, "usage")) |u| {
            const total = (json_util.value_u64(u, "input_tokens") orelse 0) +|
                (json_util.value_u64(u, "cache_creation_input_tokens") orelse 0) +|
                (json_util.value_u64(u, "cache_read_input_tokens") orelse 0);
            if (total > 0) {
                try out.append(arena, .{
                    .usage = .{
                        .session = try arena.dupe(u8, st.session),
                        .harness = st.harness,
                        .tokens = total,
                        .window = null,
                        .at_ms = st.time_ms,
                    },
                });
            }
        }
        const blocks = json_util.value_array(msg, "content") orelse return true;
        for (blocks) |b| {
            const bt = json_util.value_str(b, "type") orelse continue;
            if (std.mem.eql(u8, bt, "text")) {
                if (json_util.value_str(b, "text")) |t| {
                    try st.push(arena, out, .assistant, t, null, &.{});
                }
            } else if (std.mem.eql(u8, bt, "tool_use")) {
                const name = json_util.value_str(b, "name") orelse "tool";
                const inp = json_util.value_get(b, "input") orelse .null;
                const tc = try tool_call_extract(arena, name, inp);
                try st.push(arena, out, .tool_call, tc.text, name, tc.files);
            }
        }
    } else {
        if (json_util.value_str(v, "subtype")) |sub| {
            if (std.mem.eql(u8, sub, "compact_boundary")) {
                try out.append(arena, .{ .compacted = .{ .session = try arena.dupe(u8, st.session) } });
            }
        }
    }
    return true;
}

pub fn extract_codex_line(
    arena: std.mem.Allocator,
    v: std.json.Value,
    st: *ExtractState,
    out: *std.ArrayList(Item),
) !bool {
    if (json_util.value_get(v, "timestamp")) |ts| st.stamp(ts);
    const kind = json_util.value_str(v, "type") orelse return false;
    const p = json_util.value_get(v, "payload") orelse .null;

    if (std.mem.eql(u8, kind, "response_item")) {
        const pt = json_util.value_str(p, "type") orelse return false;
        if (std.mem.eql(u8, pt, "message")) {
            const text = try text_of_alloc(arena, json_util.value_get(p, "content") orelse .null);
            const role_s = json_util.value_str(p, "role") orelse "";
            if (std.mem.eql(u8, role_s, "user") and !is_wrapped(text)) {
                try st.push(arena, out, .user, text, null, &.{});
            } else if (std.mem.eql(u8, role_s, "assistant")) {
                try st.push(arena, out, .assistant, text, null, &.{});
            }
        } else if (std.mem.eql(u8, pt, "function_call") or std.mem.eql(u8, pt, "custom_tool_call")) {
            const name = json_util.value_str(p, "name") orelse "tool";
            const arg = json_util.value_get(p, "arguments") orelse json_util.value_get(p, "input") orelse .null;
            const tc = try tool_call_extract(arena, name, arg);
            try st.push(arena, out, .tool_call, tc.text, name, tc.files);
        } else if (std.mem.eql(u8, pt, "local_shell_call")) {
            const act = json_util.value_get(p, "action") orelse .null;
            const tc = try tool_call_extract(arena, "shell", act);
            try st.push(arena, out, .tool_call, tc.text, "shell", tc.files);
        } else if (std.mem.eql(u8, pt, "function_call_output") or std.mem.eql(u8, pt, "custom_tool_call_output")) {
            const o = json_util.value_get(p, "output") orelse .null;
            const text = if (o == .object)
                try text_of_alloc(arena, json_util.value_get(o, "content") orelse .null)
            else
                try text_of_alloc(arena, o);
            if (std.mem.startsWith(u8, text, "Script failed") or std.mem.find(u8, text, "Process exited with code ") != null) {
                try st.push(arena, out, .tool_error, text, null, &.{});
            }
        }
    } else if (std.mem.eql(u8, kind, "event_msg")) {
        const pt = json_util.value_str(p, "type") orelse return false;
        if (std.mem.eql(u8, pt, "token_count")) {
            if (json_util.value_get(p, "info")) |info| {
                if (json_util.value_get(info, "last_token_usage")) |ltu| {
                    if (json_util.value_u64(ltu, "input_tokens")) |t| {
                        try out.append(arena, .{
                            .usage = .{
                                .session = try arena.dupe(u8, st.session),
                                .harness = st.harness,
                                .tokens = t,
                                .window = json_util.value_u64(info, "model_context_window"),
                                .at_ms = st.time_ms,
                            },
                        });
                    }
                }
            }
        } else if (std.mem.eql(u8, pt, "context_compacted")) {
            try out.append(arena, .{ .compacted = .{ .session = try arena.dupe(u8, st.session) } });
        }
    } else if (std.mem.eql(u8, kind, "compacted")) {
        try out.append(arena, .{ .compacted = .{ .session = try arena.dupe(u8, st.session) } });
        if (json_util.value_str(p, "message")) |m| {
            try st.push(arena, out, .compaction, m, null, &.{});
        }
    }
    return true;
}

pub fn extract_pi_line(
    arena: std.mem.Allocator,
    v: std.json.Value,
    st: *ExtractState,
    out: *std.ArrayList(Item),
) !bool {
    if (json_util.value_get(v, "timestamp")) |ts| st.stamp(ts);
    const kind = json_util.value_str(v, "type") orelse return false;
    if (std.mem.eql(u8, kind, "message")) {
        const m = json_util.value_get(v, "message") orelse return false;
        const role_s = json_util.value_str(m, "role") orelse return false;
        if (std.mem.eql(u8, role_s, "user")) {
            const t = try text_of_alloc(arena, json_util.value_get(m, "content") orelse .null);
            try st.push(arena, out, .user, t, null, &.{});
        } else if (std.mem.eql(u8, role_s, "assistant")) {
            if (json_util.value_get(m, "usage")) |u| {
                const total = (json_util.value_u64(u, "input") orelse 0) +|
                    (json_util.value_u64(u, "cacheRead") orelse 0) +|
                    (json_util.value_u64(u, "cacheWrite") orelse 0);
                if (total > 0) {
                    try out.append(arena, .{
                        .usage = .{
                            .session = try arena.dupe(u8, st.session),
                            .harness = st.harness,
                            .tokens = total,
                            .window = null,
                            .at_ms = st.time_ms,
                        },
                    });
                }
            }
            if (json_util.value_array(m, "content")) |blocks| {
                for (blocks) |b| {
                    const bt = json_util.value_str(b, "type") orelse continue;
                    if (std.mem.eql(u8, bt, "text")) {
                        if (json_util.value_str(b, "text")) |t| {
                            try st.push(arena, out, .assistant, t, null, &.{});
                        }
                    } else if (std.mem.eql(u8, bt, "toolCall")) {
                        const name = json_util.value_str(b, "name") orelse "tool";
                        const args = json_util.value_get(b, "arguments") orelse .null;
                        const tc = try tool_call_extract(arena, name, args);
                        try st.push(arena, out, .tool_call, tc.text, name, tc.files);
                    }
                }
            }
        } else if (std.mem.eql(u8, role_s, "toolResult")) {
            if (json_util.value_bool(m, "isError") == true) {
                const tool = json_util.value_str(m, "toolName");
                const t = try text_of_alloc(arena, json_util.value_get(m, "content") orelse .null);
                try st.push(arena, out, .tool_error, t, tool, &.{});
            }
        }
    } else if (std.mem.eql(u8, kind, "compaction")) {
        try out.append(arena, .{ .compacted = .{ .session = try arena.dupe(u8, st.session) } });
        if (json_util.value_str(v, "summary")) |s| {
            try st.push(arena, out, .compaction, s, null, &.{});
        }
    }
    return true;
}

pub fn extract_grok_line(
    arena: std.mem.Allocator,
    v: std.json.Value,
    st: *ExtractState,
    out: *std.ArrayList(Item),
) !bool {
    const kind = json_util.value_str(v, "type") orelse return false;
    if (std.mem.eql(u8, kind, "user")) {
        if (json_util.value_get(v, "prompt_index")) |pi| {
            if (pi != .null) {
                const raw = try text_of_alloc(arena, json_util.value_get(v, "content") orelse .null);
                var t = std.mem.trim(u8, raw, " \t\r\n");
                if (std.mem.startsWith(u8, t, "<user_query>")) t = t["<user_query>".len..];
                if (std.mem.endsWith(u8, t, "</user_query>")) t = t[0 .. t.len - "</user_query>".len];
                try st.push(arena, out, .user, t, null, &.{});
            }
        }
    } else if (std.mem.eql(u8, kind, "assistant")) {
        const raw = try text_of_alloc(arena, json_util.value_get(v, "content") orelse .null);
        try st.push(arena, out, .assistant, raw, null, &.{});
        if (json_util.value_array(v, "tool_calls")) |calls| {
            for (calls) |c| {
                const name = json_util.value_str(c, "name") orelse "tool";
                const args = json_util.value_get(c, "arguments") orelse .null;
                const tc = try tool_call_extract(arena, name, args);
                try st.push(arena, out, .tool_call, tc.text, name, tc.files);
            }
        }
    }
    return true;
}

pub fn extract_hub_line(
    arena: std.mem.Allocator,
    v: std.json.Value,
    st: *ExtractState,
    out: *std.ArrayList(Item),
) !bool {
    if (json_util.value_u64(v, "at_ms")) |ms| st.time_ms = ms;
    const e = json_util.value_get(v, "entry") orelse v;
    const t = json_util.value_str(e, "type") orelse return false;
    if (std.mem.eql(u8, t, "session_info")) {
        if (json_util.value_str(e, "harness")) |hs| st.harness = Harness.from_wire(hs);
        if (json_util.value_str(e, "native_id")) |nid| st.session = try arena.dupe(u8, clip_utf8(nid, 1024));
    } else if (std.mem.eql(u8, t, "user_message")) {
        if (json_util.value_str(e, "text")) |txt| try st.push(arena, out, .user, txt, null, &.{});
    } else if (std.mem.eql(u8, t, "assistant_text")) {
        if (json_util.value_bool(e, "delta") != true) {
            if (json_util.value_str(e, "text")) |txt| try st.push(arena, out, .assistant, txt, null, &.{});
        }
    } else if (std.mem.eql(u8, t, "tool_call")) {
        const name = json_util.value_str(e, "name") orelse "tool";
        const sum = json_util.value_str(e, "summary") orelse "";
        const inp = json_util.value_get(e, "input") orelse .null;
        const tc = try tool_call_extract(arena, name, inp);
        const line_txt = try std.fmt.allocPrint(arena, "{s} {s}", .{ name, sum });
        try st.push(arena, out, .tool_call, line_txt, name, tc.files);
    } else if (std.mem.eql(u8, t, "tool_result")) {
        const status = json_util.value_str(e, "status") orelse "";
        if (std.mem.eql(u8, status, "failed")) {
            if (json_util.value_str(e, "output")) |txt| try st.push(arena, out, .tool_error, txt, null, &.{});
        }
    } else if (std.mem.eql(u8, t, "error")) {
        if (json_util.value_str(e, "text")) |txt| try st.push(arena, out, .tool_error, txt, null, &.{});
    } else if (std.mem.eql(u8, t, "compaction")) {
        try out.append(arena, .{ .compacted = .{ .session = try arena.dupe(u8, st.session) } });
    }
    return true;
}
