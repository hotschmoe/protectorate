const std = @import("std");
const tiger = @import("../tiger.zig");

pub const Channel = enum {
    stable,
    dev,

    pub fn from_wire(s: []const u8) ?Channel {
        return std.meta.stringToEnum(Channel, s);
    }
};

pub const Version = struct {
    major: u32,
    minor: u32,
    patch: u32,
    dev: ?u32 = null,

    pub fn parse_tag(raw: []const u8) ?Version {
        const s = std.mem.trim(u8, raw, " \t\r\n");
        if (s.len == 0) return null;
        const body = if (s[0] == 'v' or s[0] == 'V') s[1..] else s;

        var core_str = body;
        var dev_num: ?u32 = null;
        if (std.mem.findScalar(u8, body, '-')) |dash| {
            core_str = body[0..dash];
            const pre = body[dash + 1 ..];
            if (!std.mem.startsWith(u8, pre, "dev.")) return null;
            dev_num = parse_u32_strict(pre[4..]) orelse return null;
        }

        var it = std.mem.splitScalar(u8, core_str, '.');
        const maj_s = it.next() orelse return null;
        const min_s = it.next() orelse return null;
        const pat_s = it.next() orelse return null;
        if (it.next() != null) return null;

        const major = parse_u32_strict(maj_s) orelse return null;
        const minor = parse_u32_strict(min_s) orelse return null;
        const patch = parse_u32_strict(pat_s) orelse return null;

        return .{
            .major = major,
            .minor = minor,
            .patch = patch,
            .dev = dev_num,
        };
    }

    pub fn order(a: Version, b: Version) std.math.Order {
        if (a.major != b.major) return std.math.order(a.major, b.major);
        if (a.minor != b.minor) return std.math.order(a.minor, b.minor);
        if (a.patch != b.patch) return std.math.order(a.patch, b.patch);
        if (a.dev == null and b.dev == null) return .eq;
        if (a.dev == null) return .gt;
        if (b.dev == null) return .lt;
        return std.math.order(a.dev.?, b.dev.?);
    }

    pub fn is_newer(candidate: Version, current: Version, channel: Channel) bool {
        if (channel == .stable and candidate.dev != null) return false;
        return order(candidate, current) == .gt;
    }

    pub fn format_buf(self: Version, buf: []u8) []const u8 {
        if (self.dev) |d| {
            return std.fmt.bufPrint(buf, "v{d}.{d}.{d}-dev.{d}", .{ self.major, self.minor, self.patch, d }) catch unreachable;
        }
        return std.fmt.bufPrint(buf, "v{d}.{d}.{d}", .{ self.major, self.minor, self.patch }) catch unreachable;
    }
};

fn parse_u32_strict(s: []const u8) ?u32 {
    if (s.len == 0) return null;
    if (s.len > 1 and s[0] == '0') return null;
    for (s) |c| {
        if (c < '0' or c > '9') return null;
    }
    return std.fmt.parseInt(u32, s, 10) catch null;
}

pub const SumEntry = struct {
    sha256_hex: [64]u8,
    filename: []const u8,
};

pub fn parse_sha256sums(gpa: std.mem.Allocator, text: []const u8) ![]SumEntry {
    var list: std.ArrayList(SumEntry) = .empty;
    errdefer {
        for (list.items) |it| gpa.free(it.filename);
        list.deinit(gpa);
    }

    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;
        if (line.len < 66) return error.InvalidSumsFormat;

        var hex: [64]u8 = undefined;
        for (line[0..64], 0..) |c, idx| {
            const lc = std.ascii.toLower(c);
            if (!((lc >= '0' and lc <= '9') or (lc >= 'a' and lc <= 'f'))) {
                return error.InvalidSumsFormat;
            }
            hex[idx] = lc;
        }
        if (line[64] != ' ' and line[64] != '\t') return error.InvalidSumsFormat;
        var rest = std.mem.trimStart(u8, line[64..], " \t");
        if (rest.len > 0 and (rest[0] == '*' or rest[0] == ' ')) {
            rest = rest[1..];
        }
        if (std.mem.startsWith(u8, rest, "./")) {
            rest = rest[2..];
        }
        if (rest.len == 0) return error.InvalidSumsFormat;

        try list.append(gpa, .{
            .sha256_hex = hex,
            .filename = try gpa.dupe(u8, rest),
        });
    }

    return list.toOwnedSlice(gpa);
}

pub fn verify_sha256(bytes: []const u8, expected_hex: []const u8) bool {
    if (expected_hex.len != 64) return false;
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(bytes, &digest, .{});
    const hex_chars = "0123456789abcdef";
    var actual: [64]u8 = undefined;
    for (digest, 0..) |b, i| {
        actual[i * 2] = hex_chars[b >> 4];
        actual[i * 2 + 1] = hex_chars[b & 0x0f];
    }
    var diff: u8 = 0;
    for (0..64) |i| {
        diff |= actual[i] ^ std.ascii.toLower(expected_hex[i]);
    }
    return diff == 0;
}
