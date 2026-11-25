const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const noise = @import("noise.zig");
const frame = @import("frame.zig");
const bounded = @import("bounded.zig");

pub fn Channel(comptime IO: type) type {
    io.assert_contract(IO);
    return struct {
        const Self = @This();
        socket: io.Socket,
        session: noise.Session,
        rx_op: io.Operation,
        tx_op: io.Operation,
        rx_result: ?io.Result = null,
        tx_result: ?io.Result = null,
        rx_wire: [noise.max_record_wire]u8 = undefined,
        rx_have: usize = 0,
        rx_progress: u64 = 0,
        rx_need: usize = 2,
        rx_plain: [noise.max_record_plaintext]u8 = undefined,
        rx_plain_pos: usize = 0,
        rx_plain_len: usize = 0,
        rx_prefix: [frame.prefix_bytes]u8 = undefined,
        rx_prefix_len: usize = 0,
        rx_frame: bounded.FrameBuffer = .{},
        rx_frame_len: ?usize = null,
        frame_ready: bool = false,
        rx_serial: u64 = 0,
        tx_wire: [noise.max_record_wire]u8 = undefined,
        tx_plain: [noise.max_record_plaintext]u8 = undefined,
        tx_wire_pos: usize = 0,
        tx_wire_len: usize = 0,
        tx_frame: bounded.FrameBuffer = .{},
        tx_frame_pos: usize = 0,
        stopping: bool = false,

        pub fn init(self: *Self, socket: io.Socket, session: noise.Session) void {
            self.* = .{
                .socket = socket,
                .session = session,
                .rx_op = .{ .context = self, .callback = rx_complete },
                .tx_op = .{ .context = self, .callback = tx_complete },
            };
        }

        fn rx_complete(context: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(context));
            tiger.assert(self.rx_result == null);
            self.rx_result = result;
        }

        fn tx_complete(context: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(context));
            tiger.assert(self.tx_result == null);
            self.tx_result = result;
        }

        pub fn queue(self: *Self, pool: *bounded.SlabPool, kind: frame.Kind, id: u64, payload: []const u8, control: bool) !void {
            if (self.stopping or self.tx_frame.len != 0) return error.Busy;
            if (payload.len > frame.max_payload) return error.TooLarge;
            try self.tx_frame.reserve_send(pool, frame.prefix_bytes + payload.len, control and kind != .chunk and frame.prefix_bytes + payload.len <= 4096);
            errdefer self.tx_frame.clear(pool);
            var prefix: [frame.prefix_bytes]u8 = undefined;
            std.mem.writeInt(u32, prefix[0..4], @intCast(frame.head_bytes + payload.len), .little);
            prefix[4] = @backingInt(kind);
            std.mem.writeInt(u64, prefix[5..13], id, .little);
            try self.tx_frame.append(pool, &prefix, control);
            try self.tx_frame.append(pool, payload, control);
        }

        /// True while the peer is held back by this side: an unconsumed frame, or no slab free for the next one.
        pub fn rx_blocked(self: *const Self) bool {
            return self.frame_ready or (self.rx_frame_len != null and self.rx_prefix_len == self.rx_prefix.len and self.rx_frame.len == 0);
        }

        pub fn take(self: *Self, pool: *bounded.SlabPool) void {
            tiger.assert(self.frame_ready);
            self.rx_frame.clear(pool);
            self.rx_frame_len = null;
            self.rx_prefix_len = 0;
            self.frame_ready = false;
        }

        pub fn header(self: *Self, pool: *bounded.SlabPool) !struct { kind: frame.Kind, id: u64, payload_len: usize } {
            tiger.assert(self.frame_ready);
            var bytes: [frame.prefix_bytes]u8 = undefined;
            self.rx_frame.read(pool, 0, &bytes);
            return .{
                .kind = frame.Kind.from_u8(bytes[4]) orelse return error.BadKind,
                .id = std.mem.readInt(u64, bytes[5..13], .little),
                .payload_len = self.rx_frame.len - frame.prefix_bytes,
            };
        }

        pub fn read_payload(self: *Self, pool: *bounded.SlabPool, offset: usize, out: []u8) void {
            self.rx_frame.read(pool, frame.prefix_bytes + offset, out);
        }

        pub fn stop(self: *Self, backend: *IO) void {
            self.stopping = true;
            if (self.rx_op.state == .submitted) backend.cancel(&self.rx_op);
            if (self.tx_op.state == .submitted) backend.cancel(&self.tx_op);
        }

        pub fn drained(self: *const Self) bool {
            return self.rx_op.state == .idle and self.tx_op.state == .idle;
        }

        pub fn deinit(self: *Self, pool: *bounded.SlabPool) void {
            tiger.assert(self.drained());
            self.rx_frame.clear(pool);
            self.tx_frame.clear(pool);
            self.session.deinit();
        }

        pub fn drive(self: *Self, backend: *IO, pool: *bounded.SlabPool) !void {
            if (self.rx_result) |result| {
                self.rx_result = null;
                if (!self.stopping) switch (result) {
                    .received => |n| {
                        if (n == 0) return error.Closed;
                        self.rx_have += n;
                        self.rx_progress +%= n;
                    },
                    else => return error.ReceiveFailed,
                };
            }
            if (self.tx_result) |result| {
                self.tx_result = null;
                if (!self.stopping) switch (result) {
                    .sent => |n| {
                        if (n == 0) return error.Closed;
                        self.tx_wire_pos += n;
                    },
                    else => return error.SendFailed,
                };
            }
            if (self.stopping) return;
            if (self.session.expired(backend.now())) return error.SessionExpired;

            if (!self.frame_ready) {
                if (self.rx_prefix_len < self.rx_prefix.len and self.rx_plain_pos < self.rx_plain_len) {
                    const n = @min(self.rx_prefix.len - self.rx_prefix_len, self.rx_plain_len - self.rx_plain_pos);
                    @memcpy(self.rx_prefix[self.rx_prefix_len..][0..n], self.rx_plain[self.rx_plain_pos..][0..n]);
                    self.rx_prefix_len += n;
                    self.rx_plain_pos += n;
                    if (self.rx_prefix_len >= 4 and self.rx_frame_len == null) {
                        const len = std.mem.readInt(u32, self.rx_prefix[0..4], .little);
                        if (len < frame.head_bytes or len > frame.max_frame) return error.BadFrameLength;
                        self.rx_frame_len = 4 + @as(usize, len);
                    }
                }
                if (self.rx_frame_len != null and self.rx_prefix_len == self.rx_prefix.len) {
                    const total = self.rx_frame_len.?;
                    if (self.rx_frame.len == 0) {
                        const kind = frame.Kind.from_u8(self.rx_prefix[4]) orelse return error.BadKind;
                        if (self.rx_frame.reserve(pool, total, total <= 4096 and kind != .chunk)) |_| {
                            try self.rx_frame.append(pool, &self.rx_prefix, true);
                        } else |err| switch (err) {
                            error.Full => {},
                            else => return err,
                        }
                    }
                    if (self.rx_frame.len != 0 and self.rx_plain_pos < self.rx_plain_len) {
                        const n = @min(total - self.rx_frame.len, self.rx_plain_len - self.rx_plain_pos);
                        try self.rx_frame.append(pool, self.rx_plain[self.rx_plain_pos..][0..n], true);
                        self.rx_plain_pos += n;
                    }
                    if (self.rx_frame.len == total) {
                        self.frame_ready = true;
                        self.rx_serial +%= 1;
                        if (self.rx_serial == 0) self.rx_serial = 1;
                    }
                }
                if (!self.frame_ready and (self.rx_prefix_len < self.rx_prefix.len or self.rx_frame.len != 0) and self.rx_plain_pos == self.rx_plain_len) {
                    if (self.rx_have == self.rx_need) {
                        if (self.rx_need == 2) {
                            const len = std.mem.readInt(u16, self.rx_wire[0..2], .big);
                            if (len <= noise.tag_len or len > noise.max_record_plaintext + noise.tag_len) return error.BadRecordLength;
                            self.rx_need = 2 + @as(usize, len);
                        } else {
                            const plain = try self.session.open_record(self.rx_wire[2..self.rx_need], &self.rx_plain);
                            self.rx_plain_pos = 0;
                            self.rx_plain_len = plain.len;
                            self.rx_have = 0;
                            self.rx_need = 2;
                        }
                    }
                    if (self.rx_plain_pos == self.rx_plain_len and self.rx_op.state == .idle and self.rx_have < self.rx_need) {
                        try backend.submit(&self.rx_op, .{ .recv = .{ .socket = self.socket, .buffer = self.rx_wire[self.rx_have..self.rx_need] } });
                    }
                }
            }
            if (self.tx_wire_pos == self.tx_wire_len and self.tx_frame.len != 0) {
                if (self.tx_frame_pos == self.tx_frame.len) {
                    self.tx_frame.clear(pool);
                    self.tx_frame_pos = 0;
                    self.tx_wire_pos = 0;
                    self.tx_wire_len = 0;
                } else {
                    const n = @min(self.tx_plain.len, self.tx_frame.len - self.tx_frame_pos);
                    self.tx_frame.read(pool, self.tx_frame_pos, self.tx_plain[0..n]);
                    const sealed = try self.session.seal_record(self.tx_plain[0..n], &self.tx_wire);
                    self.tx_frame_pos += n;
                    self.tx_wire_pos = 0;
                    self.tx_wire_len = sealed.len;
                }
            }
            if (self.tx_wire_pos < self.tx_wire_len and self.tx_op.state == .idle) {
                try backend.submit(&self.tx_op, .{ .send = .{ .socket = self.socket, .buffer = self.tx_wire[self.tx_wire_pos..self.tx_wire_len] } });
            }
        }
    };
}
