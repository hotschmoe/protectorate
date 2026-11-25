const std = @import("std");
const tiger = @import("../tiger.zig");
const io = @import("io.zig");
const noise = @import("noise.zig");
const frame = @import("frame.zig");
const bounded = @import("bounded.zig");
const channel = @import("channel.zig");
const handshake = @import("handshake.zig");

pub const Handle = struct { slot: u8, generation: u64 };
pub const Terminal = enum { response, end, remote_error, timeout, peer_gone, peer_replaced };
pub const Event = union(enum) {
    connected: Handle,
    disconnected: Handle,
    request: struct { peer: Handle, id: u64, size: usize, serial: u64 },
    chunk: struct { peer: Handle, id: u64, size: usize, serial: u64 },
    terminal: struct { peer: Handle, id: u64, result: Terminal, size: usize, serial: u64 },
};

pub const HelloResult = struct { identity: [32]u8, reply_len: usize };
pub const allowed_max = 1024;

pub const Hooks = struct {
    context: *anyopaque,
    hello: *const fn (*anyopaque, u8, [32]u8, []const u8, []u8) ?HelloResult,
};

pub fn FleetBus(comptime IO: type) type {
    io.assert_contract(IO);
    return struct {
        const Self = @This();
        const Closing = enum { handshake, channel };
        const State = union(enum) {
            free: void,
            accepting: void,
            handshake: *handshake.Handshake(IO),
            hello: *channel.Channel(IO),
            established: *channel.Channel(IO),
            closing: Closing,
            draining: void,
        };
        const Rpc = struct {
            id: u64 = 0,
            deadline: u64 = 0,
            terminal: ?Terminal = null,
        };
        const Slot = struct {
            state: State = .{ .free = {} },
            generation: u64 = 0,
            socket: io.Socket = undefined,
            hs: handshake.Handshake(IO) = undefined,
            conn: channel.Channel(IO) = undefined,
            close_op: io.Operation = undefined,
            close_result: ?io.Result = null,
            deadline: u64 = 0,
            last_rx: u64 = 0,
            rx_serial: u64 = 0,
            rx_progress: u64 = 0,
            peer: [32]u8 = undefined,
            identity: [32]u8 = undefined,
            identity_valid: bool = false,
            next_id: u64 = 1,
            rpcs: [bounded.rpc_slots_per_node]Rpc = @splat(.{}),
            connected_event: bool = false,
            disconnected_event: bool = false,
            quiet_close: bool = false,

            fn phase(self: *const Slot) std.meta.Tag(State) {
                return std.meta.activeTag(self.state);
            }
        };
        comptime {
            tiger.comptime_assert(@sizeOf(Slot) <= 160 * 1024, "fleet slot budget");
        }
        backend: *IO,
        pool: *bounded.SlabPool,
        listener: io.Socket,
        key: noise.KeyPair,
        allowed: [allowed_max][32]u8 = undefined,
        allowed_count: usize = 0,
        hooks: Hooks,
        slots: [bounded.fleet_slots]Slot = @splat(.{}),
        slot_limit: usize = bounded.fleet_slots,
        rpc_limit: usize = bounded.rpc_slots_per_node,
        accept_op: io.Operation = undefined,
        accept_result: ?io.Result = null,
        accept_slot: ?usize = null,
        listener_close_op: io.Operation = undefined,
        listener_close_result: ?io.Result = null,
        overflow_op: io.Operation = undefined,
        overflow_result: ?io.Result = null,
        overflow_socket: ?io.Socket = null,
        stopping: bool = false,
        refused: u64 = 0,
        listener_closed: bool = false,
        scratch: [frame.max_payload]u8 = undefined,
        reply: [frame.max_payload]u8 = undefined,

        pub fn init(self: *Self, backend: *IO, pool: *bounded.SlabPool, listener: io.Socket, key: noise.KeyPair, allowed: []const [32]u8, hooks: Hooks) error{Full}!void {
            tiger.assert(@sizeOf(Self) <= 12 * 1024 * 1024);
            self.* = .{ .backend = backend, .pool = pool, .listener = listener, .key = key, .hooks = hooks };
            try self.set_allowed(allowed);
            self.accept_op = .{ .context = self, .callback = accept_complete };
            self.listener_close_op = .{ .context = self, .callback = listener_close_complete };
            self.overflow_op = .{ .context = self, .callback = overflow_complete };
            for (&self.slots) |*slot| slot.close_op = .{ .context = slot, .callback = close_complete };
        }

        fn accept_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(ctx));
            tiger.assert(self.accept_result == null);
            self.accept_result = result;
        }
        fn listener_close_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(ctx));
            self.listener_close_result = result;
        }
        fn overflow_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const self: *Self = @ptrCast(@alignCast(ctx));
            self.overflow_result = result;
        }
        fn close_complete(ctx: *anyopaque, _: *io.Operation, result: io.Result) void {
            const slot: *Slot = @ptrCast(@alignCast(ctx));
            tiger.assert(slot.close_result == null);
            slot.close_result = result;
        }

        fn handle(self: *Self, index: usize) Handle {
            return .{ .slot = @intCast(index), .generation = self.slots[index].generation };
        }
        fn find(self: *Self, peer: Handle) ?*Slot {
            if (peer.slot >= self.slots.len) return null;
            const slot = &self.slots[peer.slot];
            if (slot.phase() == .free or slot.generation != peer.generation) return null;
            return slot;
        }
        pub fn slots_in_use(self: *const Self) usize {
            var count: usize = 0;
            for (&self.slots) |*slot| {
                if (slot.phase() != .free and slot.phase() != .accepting) count += 1;
            }
            return count;
        }

        pub fn current(self: *Self, wanted: [32]u8) ?Handle {
            for (&self.slots, 0..) |*slot, i| {
                if (slot.phase() == .established and std.mem.eql(u8, &slot.identity, &wanted)) return self.handle(i);
            }
            return null;
        }

        pub fn set_allowed(self: *Self, allowed: []const [32]u8) error{Full}!void {
            if (allowed.len > self.allowed.len) return error.Full;
            @memcpy(self.allowed[0..allowed.len], allowed);
            self.allowed_count = allowed.len;
            for (&self.slots) |*slot| {
                if (slot.phase() == .handshake) slot.hs.allowed = self.allowed[0..self.allowed_count];
                if (slot.phase() == .hello and !self.is_allowed(slot.peer)) self.begin_close(slot, false);
                if (slot.phase() == .established and !self.is_allowed(slot.peer)) {
                    slot.quiet_close = true;
                    self.begin_close(slot, false);
                }
            }
        }

        fn is_allowed(self: *const Self, peer: [32]u8) bool {
            var found = false;
            for (self.allowed[0..self.allowed_count]) |key| {
                if (std.crypto.timing_safe.eql([32]u8, key, peer)) found = true;
            }
            return found;
        }

        pub fn identity(self: *Self, peer: Handle) ?[32]u8 {
            const slot = self.find(peer) orelse return null;
            if (!slot.identity_valid) return null;
            return slot.identity;
        }

        pub fn disconnect(self: *Self, peer: Handle) void {
            const slot = self.find(peer) orelse return;
            self.begin_close(slot, false);
        }

        /// Closes the session without a disconnect event, for an owner that already knows it is gone.
        pub fn disconnect_quiet(self: *Self, peer: Handle) void {
            const slot = self.find(peer) orelse return;
            slot.quiet_close = true;
            self.begin_close(slot, false);
        }

        fn begin_close(self: *Self, slot: *Slot, replaced: bool) void {
            const established = slot.phase() == .established;
            const closing: Closing = switch (slot.state) {
                .handshake => |hs| blk: {
                    hs.stop(self.backend);
                    break :blk .handshake;
                },
                .hello, .established => |conn| blk: {
                    conn.stop(self.backend);
                    break :blk .channel;
                },
                else => return,
            };
            if (established and !replaced and !slot.quiet_close) slot.disconnected_event = true;
            slot.connected_event = false;
            for (&slot.rpcs) |*rpc| {
                if (rpc.id != 0 and rpc.terminal == null) rpc.terminal = if (replaced) .peer_replaced else .peer_gone;
            }
            slot.state = .{ .closing = closing };
        }

        pub fn shutdown(self: *Self) void {
            if (self.stopping) return;
            self.stopping = true;
            if (self.accept_op.state == .submitted) self.backend.cancel(&self.accept_op);
            for (&self.slots) |*slot| self.begin_close(slot, false);
        }

        pub fn done(self: *const Self) bool {
            if (!self.stopping or !self.listener_closed or self.overflow_socket != null or self.accept_op.state != .idle) return false;
            for (&self.slots) |*slot| if (slot.phase() != .free) return false;
            return true;
        }

        pub fn deinit(self: *Self) void {
            tiger.assert(self.done());
            self.key.zeroize();
        }

        pub fn assert_invariants(self: *const Self) void {
            for (&self.slots, 0..) |*slot, i| {
                switch (slot.state) {
                    .free => tiger.assert(slot.close_op.state == .idle),
                    .accepting => tiger.assert(slot.generation != 0 and self.accept_slot != null and self.accept_slot.? == i),
                    .handshake => |hs| tiger.assert(hs == &slot.hs and slot.generation != 0),
                    .hello, .established => |conn| tiger.assert(conn == &slot.conn and slot.generation != 0),
                    .closing => tiger.assert(slot.generation != 0),
                    .draining => tiger.assert(slot.generation != 0),
                }
                for (slot.rpcs, 0..) |rpc, j| {
                    if (rpc.id == 0) continue;
                    tiger.assert(rpc.id % 2 == 1 and slot.phase() != .free);
                    for (slot.rpcs[j + 1 ..]) |other| tiger.assert(other.id != rpc.id);
                }
                if (slot.phase() == .established) for (self.slots[i + 1 ..]) |*other| {
                    if (other.phase() == .established) tiger.assert(!std.mem.eql(u8, &slot.identity, &other.identity));
                };
            }
        }

        pub fn call(self: *Self, peer: Handle, payload: []const u8, deadline: u64) !u64 {
            const slot = self.find(peer) orelse return error.PeerGone;
            if (slot.phase() != .established) return error.PeerGone;
            for (slot.rpcs[0..self.rpc_limit]) |*rpc| {
                if (rpc.id != 0) continue;
                const id = slot.next_id;
                if (id > std.math.maxInt(u64) - 2) return error.IdExhausted;
                try slot.conn.queue(self.pool, .request, id, payload, false);
                slot.next_id += 2;
                rpc.* = .{ .id = id, .deadline = deadline };
                return id;
            }
            return error.Busy;
        }

        pub fn send(self: *Self, peer: Handle, kind: frame.Kind, id: u64, payload: []const u8) !void {
            const slot = self.find(peer) orelse return error.PeerGone;
            if (slot.phase() != .established) return error.PeerGone;
            if (kind == .request or id % 2 != 0) return error.BadId;
            try slot.conn.queue(self.pool, kind, id, payload, payload.len <= 4096);
        }

        fn rpc_for(slot: *Slot, id: u64) ?*Rpc {
            for (&slot.rpcs) |*rpc| if (rpc.id == id and id != 0) return rpc;
            return null;
        }

        pub fn event_for(self: *Self, i: usize) ?Event {
            if (i >= self.slots.len) return null;
            const slot = &self.slots[i];
            const peer = self.handle(i);
            if (slot.connected_event) return .{ .connected = peer };
            if (slot.phase() == .established and slot.conn.frame_ready) {
                const hdr = slot.conn.header(self.pool) catch unreachable;
                if (hdr.kind == .request) return .{ .request = .{ .peer = peer, .id = hdr.id, .size = hdr.payload_len, .serial = slot.conn.rx_serial } };
                if (rpc_for(slot, hdr.id)) |rpc| {
                    if (rpc.terminal == null and hdr.kind == .chunk) return .{ .chunk = .{ .peer = peer, .id = hdr.id, .size = hdr.payload_len, .serial = slot.conn.rx_serial } };
                    if (rpc.terminal == null) return .{ .terminal = .{ .peer = peer, .id = hdr.id, .result = switch (hdr.kind) {
                        .response => .response,
                        .end => .end,
                        .err => .remote_error,
                        else => unreachable,
                    }, .size = hdr.payload_len, .serial = slot.conn.rx_serial } };
                }
            }
            for (slot.rpcs) |rpc| if (rpc.terminal) |result| return .{ .terminal = .{ .peer = peer, .id = rpc.id, .result = result, .size = 0, .serial = 0 } };
            if (slot.disconnected_event) return .{ .disconnected = peer };
            return null;
        }

        pub fn event(self: *Self) ?Event {
            for (0..self.slots.len) |i| {
                if (self.event_for(i)) |ev| return ev;
            }
            return null;
        }

        pub fn read(self: *Self, peer: Handle, serial: u64, offset: usize, out: []u8) !void {
            const slot = self.find(peer) orelse return error.PeerGone;
            if (slot.phase() != .established or !slot.conn.frame_ready or slot.conn.rx_serial != serial) return error.NoFrame;
            const hdr = try slot.conn.header(self.pool);
            if (offset > hdr.payload_len or out.len > hdr.payload_len - offset) return error.BadOffset;
            slot.conn.read_payload(self.pool, offset, out);
        }

        pub fn consume(self: *Self, ev: Event) void {
            const event_peer = switch (ev) {
                .connected, .disconnected => |p| p,
                .request => |e| e.peer,
                .chunk => |e| e.peer,
                .terminal => |e| e.peer,
            };
            const current_event = self.event_for(event_peer.slot) orelse return;
            if (!std.meta.eql(ev, current_event)) return;
            switch (ev) {
                .connected => |peer| self.find(peer).?.connected_event = false,
                .disconnected => |peer| self.find(peer).?.disconnected_event = false,
                .request => |e| self.find(e.peer).?.conn.take(self.pool),
                .chunk => |e| self.find(e.peer).?.conn.take(self.pool),
                .terminal => |e| {
                    const slot = self.find(e.peer).?;
                    const rpc = rpc_for(slot, e.id).?;
                    if (rpc.terminal == null) slot.conn.take(self.pool);
                    rpc.* = .{};
                },
            }
        }

        pub fn drive(self: *Self) !void {
            if (self.overflow_result != null) {
                self.overflow_result = null;
                self.overflow_socket = null;
            }
            if (self.accept_result) |result| {
                self.accept_result = null;
                const reserved = self.accept_slot;
                self.accept_slot = null;
                switch (result) {
                    .accepted => |socket| {
                        if (reserved != null and !self.stopping) {
                            const slot = &self.slots[reserved.?];
                            tiger.assert(slot.phase() == .accepting);
                            slot.socket = socket;
                            slot.hs.init_responder(socket, self.key, self.allowed[0..self.allowed_count]);
                            slot.state = .{ .handshake = &slot.hs };
                            slot.deadline = self.backend.now() +| 10_000;
                        } else {
                            if (reserved) |index| self.slots[index].state = .{ .free = {} };
                            tiger.assert(self.overflow_socket == null);
                            self.overflow_socket = socket;
                            self.refused +%= 1;
                        }
                    },
                    else => if (reserved) |index| {
                        tiger.assert(self.slots[index].phase() == .accepting);
                        self.slots[index].state = .{ .free = {} };
                    },
                }
            }
            if (self.overflow_socket) |socket| {
                if (self.overflow_op.state == .idle and self.overflow_result == null) self.backend.submit(&self.overflow_op, .{ .close = socket }) catch {};
            } else if (!self.stopping and self.accept_op.state == .idle) {
                tiger.assert(self.accept_slot == null);
                for (self.slots[0..self.slot_limit], 0..) |*slot, index| {
                    if (slot.phase() != .free) continue;
                    tiger.assert(slot.close_op.state == .idle);
                    slot.generation +%= 1;
                    if (slot.generation == 0) slot.generation = 1;
                    slot.state = .{ .accepting = {} };
                    self.accept_slot = index;
                    break;
                }
                self.backend.submit(&self.accept_op, .{ .accept = self.listener }) catch {
                    if (self.accept_slot) |index| self.slots[index].state = .{ .free = {} };
                    self.accept_slot = null;
                };
            }
            if (self.listener_close_result != null) {
                self.listener_close_result = null;
                self.listener_closed = true;
            }
            if (self.stopping and !self.listener_closed and self.accept_op.state == .idle and self.accept_result == null and self.listener_close_op.state == .idle) {
                self.backend.submit(&self.listener_close_op, .{ .close = self.listener }) catch {};
            }
            for (&self.slots, 0..) |*slot, index| {
                switch (slot.state) {
                    .free, .accepting => {},
                    .handshake => {
                        if (self.backend.now() >= slot.deadline) {
                            self.begin_close(slot, false);
                            continue;
                        }
                        slot.hs.drive(self.backend) catch {
                            self.begin_close(slot, false);
                            continue;
                        };
                        if (slot.hs.ready()) {
                            var session = slot.hs.take();
                            defer session.deinit();
                            if (!self.is_allowed(session.peer_static)) {
                                self.begin_close(slot, false);
                                continue;
                            }
                            slot.peer = session.peer_static;
                            slot.hs.deinit();
                            slot.conn.init(slot.socket, session);
                            slot.state = .{ .hello = &slot.conn };
                        }
                    },
                    .hello, .established => {
                        if (self.backend.now() >= slot.deadline and slot.phase() == .hello) {
                            self.begin_close(slot, false);
                            continue;
                        }
                        slot.conn.drive(self.backend, self.pool) catch {
                            self.begin_close(slot, false);
                            continue;
                        };
                        if (slot.rx_progress != slot.conn.rx_progress or slot.conn.rx_blocked()) {
                            slot.rx_progress = slot.conn.rx_progress;
                            slot.last_rx = self.backend.now();
                        }
                        if (slot.phase() == .established and self.backend.now() -| slot.last_rx > 20_000) {
                            self.begin_close(slot, false);
                            continue;
                        }
                        if (slot.conn.frame_ready) {
                            if (slot.rx_serial != slot.conn.rx_serial) {
                                slot.rx_serial = slot.conn.rx_serial;
                                slot.last_rx = self.backend.now();
                            }
                            const hdr = slot.conn.header(self.pool) catch {
                                self.begin_close(slot, false);
                                continue;
                            };
                            if (slot.phase() == .hello) {
                                if (hdr.kind != .request or hdr.id % 2 != 0) {
                                    self.begin_close(slot, false);
                                    continue;
                                }
                                slot.conn.read_payload(self.pool, 0, self.scratch[0..hdr.payload_len]);
                                const hello_result = self.hooks.hello(self.hooks.context, @intCast(index), slot.peer, self.scratch[0..hdr.payload_len], &self.reply) orelse {
                                    self.begin_close(slot, false);
                                    continue;
                                };
                                var established: usize = 0;
                                var replacement = false;
                                for (&self.slots) |*other| {
                                    if (other.phase() == .established) {
                                        established += 1;
                                        if (std.mem.eql(u8, &other.identity, &hello_result.identity)) replacement = true;
                                    }
                                }
                                if (established >= self.slot_limit - self.slot_limit / 4 and !replacement) {
                                    self.refused +%= 1;
                                    self.begin_close(slot, false);
                                    continue;
                                }
                                if (hello_result.reply_len > self.reply.len) {
                                    self.begin_close(slot, false);
                                    continue;
                                }
                                slot.conn.queue(self.pool, .response, hdr.id, self.reply[0..hello_result.reply_len], true) catch |err| {
                                    if (err != error.Busy and err != error.Full) self.begin_close(slot, false);
                                    continue;
                                };
                                slot.conn.take(self.pool);
                                slot.identity = hello_result.identity;
                                slot.identity_valid = true;
                                slot.state = .{ .established = &slot.conn };
                                for (&self.slots, 0..) |*other, j| {
                                    if (index != j and other.phase() == .established and std.mem.eql(u8, &other.identity, &slot.identity)) self.begin_close(other, true);
                                }
                                slot.connected_event = true;
                            } else {
                                if (hdr.kind == .request and hdr.id % 2 != 0 or hdr.kind != .request and hdr.id % 2 != 1) {
                                    self.begin_close(slot, false);
                                    continue;
                                }
                                if (hdr.kind != .request and rpc_for(slot, hdr.id) == null) slot.conn.take(self.pool);
                            }
                        }
                        if (slot.phase() == .established) for (&slot.rpcs) |*rpc| {
                            if (rpc.id == 0 or rpc.terminal != null or self.backend.now() < rpc.deadline) continue;
                            if (slot.conn.frame_ready) {
                                const hdr = slot.conn.header(self.pool) catch unreachable;
                                if (hdr.id == rpc.id and (hdr.kind == .response or hdr.kind == .end or hdr.kind == .err)) continue;
                            }
                            rpc.terminal = .timeout;
                        };
                    },
                    .closing => |kind| {
                        const drained = if (kind == .channel) slot.conn.drained() else slot.hs.drained();
                        if (!drained) continue;
                        self.backend.submit(&slot.close_op, .{ .close = slot.socket }) catch continue;
                        if (kind == .channel) {
                            slot.conn.deinit(self.pool);
                        } else slot.hs.deinit();
                        slot.state = .{ .draining = {} };
                    },
                    .draining => {
                        if (slot.close_result == null) continue;
                        var pending = slot.disconnected_event;
                        for (slot.rpcs) |rpc| if (rpc.id != 0) {
                            pending = true;
                        };
                        if (!pending) {
                            const generation = slot.generation;
                            slot.* = .{ .generation = generation, .close_op = .{ .context = slot, .callback = close_complete } };
                        }
                    },
                }
            }
        }
    };
}
