const std = @import("std");
const tiger = @import("../tiger.zig");
const noise = @import("noise.zig");

pub const max_frame: usize = 1_048_576;
pub const head_bytes: usize = 9;
pub const max_payload: usize = max_frame - head_bytes;
pub const prefix_bytes: usize = 4 + head_bytes;

pub const decoder_buffer_max: usize = 4 + max_frame + noise.max_record_plaintext;

pub const Kind = enum(u8) {
    request = 1,
    response = 2,
    chunk = 3,
    end = 4,
    err = 5,

    pub fn from_u8(b: u8) ?Kind {
        return switch (b) {
            1 => .request,
            2 => .response,
            3 => .chunk,
            4 => .end,
            5 => .err,
            else => null,
        };
    }
};

pub const Frame = struct {
    kind: Kind,
    id: u64,
    payload: []u8,

    pub fn deinit(self: *Frame, gpa: std.mem.Allocator) void {
        gpa.free(self.payload);
    }
};

pub const FrameError = error{
    TooLarge,
    TooShort,
    BadKind,
    Truncated,
    OutOfMemory,
};

pub fn encode_into(
    gpa: std.mem.Allocator,
    kind: Kind,
    id: u64,
    payload: []const u8,
    out: *std.ArrayList(u8),
) FrameError!void {
    if (payload.len > max_payload) return error.TooLarge;
    const frame_len: u32 = @intCast(payload.len + head_bytes);
    try out.ensureUnusedCapacity(gpa, prefix_bytes + payload.len);
    var hdr: [prefix_bytes]u8 = undefined;
    std.mem.writeInt(u32, hdr[0..4], frame_len, .little);
    hdr[4] = @intFromEnum(kind);
    std.mem.writeInt(u64, hdr[5..13], id, .little);
    out.appendSliceAssumeCapacity(&hdr);
    out.appendSliceAssumeCapacity(payload);
}

pub fn encode_alloc(
    gpa: std.mem.Allocator,
    kind: Kind,
    id: u64,
    payload: []const u8,
) FrameError![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    try encode_into(gpa, kind, id, payload, &out);
    return out.toOwnedSlice(gpa);
}

pub const DecodedSlice = struct {
    kind: Kind,
    id: u64,
    payload: []const u8,
    consumed: usize,
};

pub fn decode_slice(buf: []const u8) FrameError!?DecodedSlice {
    if (buf.len < 4) return null;
    const len = std.mem.readInt(u32, buf[0..4], .little);
    if (len > max_frame) return error.TooLarge;
    if (len < head_bytes) return error.TooShort;
    if (buf.len < 5) return null;
    const kind = Kind.from_u8(buf[4]) orelse return error.BadKind;
    const total: usize = 4 + @as(usize, len);
    if (buf.len < total) return null;
    const id = std.mem.readInt(u64, buf[5..13], .little);
    return .{
        .kind = kind,
        .id = id,
        .payload = buf[prefix_bytes..total],
        .consumed = total,
    };
}

pub const Decoder = struct {
    buf: std.ArrayList(u8) = .empty,
    failed: ?FrameError = null,

    pub fn deinit(self: *Decoder, gpa: std.mem.Allocator) void {
        self.buf.deinit(gpa);
    }

    pub fn feed(self: *Decoder, gpa: std.mem.Allocator, bytes: []const u8) FrameError!void {
        if (self.failed != null) return;
        if (bytes.len > decoder_buffer_max - self.buf.items.len) {
            self.failed = error.TooLarge;
            self.buf.clearAndFree(gpa);
            return error.TooLarge;
        }
        try self.buf.appendSlice(gpa, bytes);
    }

    pub fn next_frame(self: *Decoder, gpa: std.mem.Allocator) FrameError!?Frame {
        if (self.failed) |err_val| return err_val;
        const dec = decode_slice(self.buf.items) catch |err_val| {
            self.failed = err_val;
            self.buf.clearAndFree(gpa);
            return err_val;
        };
        const d = dec orelse return null;
        const payload = try gpa.dupe(u8, d.payload);
        const remaining = self.buf.items.len - d.consumed;
        if (remaining > 0) {
            std.mem.copyForwards(u8, self.buf.items[0..remaining], self.buf.items[d.consumed..]);
        }
        self.buf.items.len = remaining;
        return .{
            .kind = d.kind,
            .id = d.id,
            .payload = payload,
        };
    }

    pub fn buffered(self: *const Decoder) usize {
        return self.buf.items.len;
    }

    pub fn finish(self: *const Decoder) FrameError!void {
        if (self.failed) |err_val| return err_val;
        if (self.buf.items.len != 0) return error.Truncated;
    }
};
