const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const noise = @import("noise.zig");

pub fn Handshake(comptime IO: type) type {
    io.assert_contract(IO);
    return struct {
        const Self = @This();
        socket: io.Socket,
        static_key: noise.KeyPair,
        allowed: []const [32]u8,
        initiator: ?noise.InitiatorHandshake = null,
        session: ?noise.Session = null,
        recv_op: io.Operation,
        send_op: io.Operation,
        recv_result: ?io.Result = null,
        send_result: ?io.Result = null,
        recv_buf: [noise.msg1_len]u8 = undefined,
        recv_len: usize = 0,
        recv_target: usize,
        send_buf: [noise.msg1_len]u8 = undefined,
        send_len: usize = 0,
        send_target: usize = 0,
        stopped: bool = false,
        responder: bool,

        fn ephemeral(backend: *IO) noise.KeyPair {
            var seed: [32]u8 = undefined;
            for (0..4) |i| std.mem.writeInt(u64, seed[i * 8 ..][0..8], backend.random(), .little);
            defer std.crypto.secureZero(u8, &seed);
            return noise.KeyPair.generate_deterministic(seed);
        }

        pub fn init_initiator(self: *Self, backend: *IO, socket: io.Socket, key: noise.KeyPair, peer: [32]u8) !void {
            self.* = .{
                .socket = socket,
                .static_key = key,
                .allowed = &.{},
                .recv_op = .{ .context = self, .callback = recv_complete },
                .send_op = .{ .context = self, .callback = send_complete },
                .recv_target = noise.msg2_len,
                .send_target = noise.msg1_len,
                .responder = false,
            };
            errdefer self.static_key.zeroize();
            var key_ephemeral = ephemeral(backend);
            defer key_ephemeral.zeroize();
            self.initiator = try noise.InitiatorHandshake.start(key, key_ephemeral, peer, &self.send_buf);
        }

        pub fn init_responder(self: *Self, socket: io.Socket, key: noise.KeyPair, allowed: []const [32]u8) void {
            self.* = .{
                .socket = socket,
                .static_key = key,
                .allowed = allowed,
                .recv_op = .{ .context = self, .callback = recv_complete },
                .send_op = .{ .context = self, .callback = send_complete },
                .recv_target = noise.msg1_len,
                .responder = true,
            };
        }

        fn recv_complete(context: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(context));
            tiger.assert(self.recv_result == null);
            self.recv_result = result;
        }

        fn send_complete(context: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(context));
            tiger.assert(self.send_result == null);
            self.send_result = result;
        }

        pub fn stop(self: *Self, backend: *IO) void {
            self.stopped = true;
            if (self.recv_op.state == .submitted) backend.cancel(&self.recv_op);
            if (self.send_op.state == .submitted) backend.cancel(&self.send_op);
        }

        pub fn drained(self: *const Self) bool {
            return self.recv_op.state == .idle and self.send_op.state == .idle;
        }

        pub fn deinit(self: *Self) void {
            tiger.assert(self.drained());
            if (self.initiator) |*handshake| handshake.deinit();
            if (self.session) |*session| session.deinit();
            self.static_key.zeroize();
        }

        pub fn take(self: *Self) noise.Session {
            tiger.assert(self.ready());
            const session = self.session.?;
            self.session.?.deinit();
            self.session = null;
            return session;
        }

        pub fn ready(self: *const Self) bool {
            return self.session != null and self.send_len == self.send_target and self.drained();
        }

        pub fn drive(self: *Self, backend: *IO) !void {
            if (self.recv_result) |result| {
                self.recv_result = null;
                if (!self.stopped) switch (result) {
                    .received => |n| {
                        if (n == 0) return error.Closed;
                        self.recv_len += n;
                    },
                    else => return error.ReceiveFailed,
                };
            }
            if (self.send_result) |result| {
                self.send_result = null;
                if (!self.stopped) switch (result) {
                    .sent => |n| {
                        if (n == 0) return error.Closed;
                        self.send_len += n;
                    },
                    else => return error.SendFailed,
                };
            }
            if (self.stopped) return;
            if (self.recv_len == self.recv_target and self.session == null) {
                if (self.responder) {
                    var reply: [noise.msg2_len]u8 = undefined;
                    var key_ephemeral = ephemeral(backend);
                    defer key_ephemeral.zeroize();
                    self.session = try noise.responder_accept_any(self.static_key, key_ephemeral, self.allowed, self.recv_buf[0..self.recv_target], &reply, backend.now());
                    @memcpy(self.send_buf[0..noise.msg2_len], &reply);
                    self.send_target = noise.msg2_len;
                } else {
                    self.session = try self.initiator.?.finish(self.recv_buf[0..self.recv_target], backend.now());
                    self.initiator = null;
                }
            }
            if (self.recv_len < self.recv_target and self.recv_op.state == .idle) {
                try backend.submit(&self.recv_op, .{ .recv = .{ .socket = self.socket, .buffer = self.recv_buf[self.recv_len..self.recv_target] } });
            }
            if (self.send_len < self.send_target and self.send_op.state == .idle) {
                try backend.submit(&self.send_op, .{ .send = .{ .socket = self.socket, .buffer = self.send_buf[self.send_len..self.send_target] } });
            }
        }
    };
}
