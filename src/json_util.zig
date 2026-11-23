const std = @import("std");
const tiger = @import("tiger.zig");

pub fn write_string(w: *std.Io.Writer, s: []const u8) std.Io.Writer.Error!void {
    try std.json.Stringify.encodeJsonString(s, .{}, w);
}

pub fn stringify_alloc(gpa: std.mem.Allocator, value: anytype) error{OutOfMemory}![]u8 {
    return std.json.Stringify.valueAlloc(gpa, value, .{});
}

pub fn value_get(val: std.json.Value, key: []const u8) ?std.json.Value {
    return switch (val) {
        .object => |obj| obj.get(key),
        else => null,
    };
}

pub fn value_str(val: std.json.Value, key: []const u8) ?[]const u8 {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

pub fn value_nonempty_str(val: std.json.Value, key: []const u8) ?[]const u8 {
    const s = value_str(val, key) orelse return null;
    if (s.len == 0) return null;
    return s;
}

pub fn value_u64(val: std.json.Value, key: []const u8) ?u64 {
    const v = value_get(val, key) orelse return null;
    return as_u64(v);
}

pub fn as_u64(v: std.json.Value) ?u64 {
    return switch (v) {
        .integer => |n| if (n >= 0) @intCast(n) else null,
        .float => |f| blk: {
            if (!std.math.isFinite(f) or f < 0 or f > @as(f64, @floatFromInt(std.math.maxInt(u64)))) break :blk null;
            if (@trunc(f) != f) break :blk null;
            break :blk @intFromFloat(f);
        },
        else => null,
    };
}

pub fn value_i64(val: std.json.Value, key: []const u8) ?i64 {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .integer => |n| n,
        .float => |f| if (std.math.isFinite(f) and @trunc(f) == f and f >= -9223372036854775808.0 and f <= 9223372036854774784.0) @intFromFloat(f) else null,
        else => null,
    };
}

pub fn value_f64(val: std.json.Value, key: []const u8) ?f64 {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .float => |f| if (std.math.isFinite(f)) f else null,
        .integer => |n| @floatFromInt(n),
        .number_string => |s| blk: {
            const f = std.fmt.parseFloat(f64, s) catch break :blk null;
            break :blk if (std.math.isFinite(f)) f else null;
        },
        else => null,
    };
}

pub fn value_bool(val: std.json.Value, key: []const u8) ?bool {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .bool => |b| b,
        else => null,
    };
}

pub fn value_array(val: std.json.Value, key: []const u8) ?[]const std.json.Value {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .array => |a| a.items,
        else => null,
    };
}

pub fn value_object(val: std.json.Value, key: []const u8) ?std.json.Value {
    const v = value_get(val, key) orelse return null;
    return switch (v) {
        .object => v,
        else => null,
    };
}

pub fn json_value_to_string_alloc(gpa: std.mem.Allocator, val: std.json.Value) error{OutOfMemory}![]u8 {
    return std.json.Stringify.valueAlloc(gpa, val, .{});
}

pub fn clone_value(arena: std.mem.Allocator, val: std.json.Value) error{OutOfMemory}!std.json.Value {
    switch (val) {
        .null => return .null,
        .bool => |b| return .{ .bool = b },
        .integer => |n| return .{ .integer = n },
        .float => |f| return .{ .float = f },
        .number_string => |s| return .{ .number_string = try arena.dupe(u8, s) },
        .string => |s| return .{ .string = try arena.dupe(u8, s) },
        .array => |arr| {
            var out: std.json.Array = .init(arena);
            try out.ensureTotalCapacity(arr.items.len);
            for (arr.items) |item| {
                out.appendAssumeCapacity(try clone_value(arena, item));
            }
            return .{ .array = out };
        },
        .object => |obj| {
            var out: std.json.ObjectMap = .empty;
            try out.ensureTotalCapacity(arena, @intCast(obj.count()));
            var it = obj.iterator();
            while (it.next()) |entry| {
                const k = try arena.dupe(u8, entry.key_ptr.*);
                const v = try clone_value(arena, entry.value_ptr.*);
                out.putAssumeCapacityNoClobber(k, v);
            }
            return .{ .object = out };
        },
    }
}

pub fn hex_encode(src: []const u8, dst: []u8) []const u8 {
    tiger.assert(dst.len >= src.len * 2);
    const hex_chars = "0123456789abcdef";
    for (src, 0..) |b, i| {
        dst[i * 2] = hex_chars[b >> 4];
        dst[i * 2 + 1] = hex_chars[b & 0x0f];
    }
    return dst[0 .. src.len * 2];
}

pub fn hex_encode_alloc(gpa: std.mem.Allocator, src: []const u8) error{OutOfMemory}![]u8 {
    const dst = try gpa.alloc(u8, src.len * 2);
    _ = hex_encode(src, dst);
    return dst;
}

pub fn hex_decode(hex: []const u8, dst: []u8) error{InvalidHex}![]u8 {
    if (hex.len % 2 != 0 or dst.len < hex.len / 2) return error.InvalidHex;
    const out_len = hex.len / 2;
    for (0..out_len) |i| {
        const hi = hex_nibble(hex[i * 2]) orelse return error.InvalidHex;
        const lo = hex_nibble(hex[i * 2 + 1]) orelse return error.InvalidHex;
        dst[i] = (@as(u8, hi) << 4) | @as(u8, lo);
    }
    return dst[0..out_len];
}

fn hex_nibble(c: u8) ?u4 {
    return switch (c) {
        '0'...'9' => @intCast(c - '0'),
        'a'...'f' => @intCast(c - 'a' + 10),
        'A'...'F' => @intCast(c - 'A' + 10),
        else => null,
    };
}
