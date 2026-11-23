const std = @import("std");
const tiger = @import("../tiger.zig");

pub const Crc32c = std.hash.crc.@"CRC-32/ISCSI";

pub const max_frame: usize = 16_777_216;
pub const frame_header: usize = 8;

pub const EncodeError = error{
    Empty,
    TooLarge,
    OutOfMemory,
};

pub const Stop = enum {
    short_header,
    zero_length,
    too_large,
    short_payload,
    crc_mismatch,
};

pub const Recovery = union(enum) {
    clean: usize,
    torn: struct { prefix: usize, stop: Stop },
    corrupt: usize,
};

pub fn recovery(valid_len: usize, stop: ?Stop, intact_after: bool) Recovery {
    const s = stop orelse return .{ .clean = valid_len };
    if (s == .crc_mismatch and intact_after) return .{ .corrupt = valid_len };
    return .{ .torn = .{ .prefix = valid_len, .stop = s } };
}

pub fn crc32c(bytes: []const u8) u32 {
    return Crc32c.hash(bytes);
}

pub fn encode_frame(gpa: std.mem.Allocator, payload: []const u8, out: *std.ArrayList(u8)) EncodeError!void {
    if (payload.len == 0) return error.Empty;
    if (payload.len > max_frame) return error.TooLarge;
    try out.ensureUnusedCapacity(gpa, frame_header + payload.len);
    var hdr: [frame_header]u8 = undefined;
    std.mem.writeInt(u32, hdr[0..4], @intCast(payload.len), .little);
    std.mem.writeInt(u32, hdr[4..8], crc32c(payload), .little);
    out.appendSliceAssumeCapacity(&hdr);
    out.appendSliceAssumeCapacity(payload);
}

pub fn complete_len(buf: []const u8) ?usize {
    if (buf.len < frame_header) return null;
    const len: usize = std.mem.readInt(u32, buf[0..4], .little);
    if (len != 0 and len <= max_frame and buf.len - frame_header >= len) {
        return frame_header + len;
    }
    return null;
}

pub const Decoded = struct {
    frames: [][]const u8,
    valid_len: usize,
    stop: ?Stop,
};

pub fn decode_frames(gpa: std.mem.Allocator, buf: []const u8) error{OutOfMemory}!Decoded {
    var frames: std.ArrayList([]const u8) = .empty;
    errdefer frames.deinit(gpa);
    var pos: usize = 0;
    const stop: ?Stop = while (true) {
        const rest = buf[pos..];
        if (rest.len == 0) break null;
        if (rest.len < frame_header) break .short_header;
        const len: usize = std.mem.readInt(u32, rest[0..4], .little);
        const crc = std.mem.readInt(u32, rest[4..8], .little);
        if (len == 0) break .zero_length;
        if (len > max_frame) break .too_large;
        if (rest.len - frame_header < len) break .short_payload;
        const payload = rest[frame_header .. frame_header + len];
        if (crc32c(payload) != crc) break .crc_mismatch;
        try frames.append(gpa, payload);
        pos += frame_header + len;
    };
    return .{
        .frames = try frames.toOwnedSlice(gpa),
        .valid_len = pos,
        .stop = stop,
    };
}
